//! SkillDo command-line interface.
//!
//! This module is an alternative front-end to the same `core` business logic
//! that powers the Tauri desktop client. It exists so that AI coding agents
//! (and humans) can read and manage skills from a plain terminal without
//! launching the GUI. The CLI and the GUI share the exact same SQLite
//! database, so state stays in sync across both.
//!
//! Output convention (agent-native, inspired by CLI-Anything):
//! - Default output is human-readable.
//! - `--json` emits structured JSON suitable for programmatic consumption.
//! - Errors print to stderr and exit with a non-zero code.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

use crate::core::app_config::{
    export_config_json, load_app_config, parse_config_json, save_app_config_impl, AppConfig,
    CurrentAuthorConfig, WebDavConfig,
};
use crate::core::backup::{export_full_backup, restore_full_backup, RestoreReport};
use crate::core::device_sync::{device_publish, device_pull, device_status, DevicePipelineReport};
use crate::core::explore_sources::{self, ExploreSourceConfig};
use crate::core::github_auth::compute_github_token_status;
use crate::core::installer;
use crate::core::profile_sync::{
    export_profile_json, import_profile_json, synchronize_profile, ConflictStrategy,
    ProfileSyncReport,
};
use crate::core::skill_store::{default_db_path_cli, SkillStore};
use crate::core::source_repair::{repair_skill_source, repair_skill_sources};
use crate::core::tool_adapters;
use crate::core::webdav::{download_backup, upload_backup};

#[derive(Parser)]
#[command(
    version,
    about = "SkillDo CLI - manage AI agent skills from the terminal (agent-native)",
    long_about = None
)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output results as JSON for programmatic/agent consumption.
    #[arg(long, global = true)]
    json: bool,

    /// Override the SQLite database path (defaults to the shared app data dir).
    #[arg(long, global = true)]
    db: Option<PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// List skills currently managed by SkillDo.
    List {
        /// Filter by syncability: all | syncable | local.
        #[arg(long, default_value = "all", value_parser = ["all", "syncable", "local"])]
        filter: String,
    },
    /// Show installation status of supported AI tools.
    Status,
    /// Browse the skill market across configured sources.
    Explore {
        /// Filter query for online/json sources.
        #[arg(short, long)]
        query: Option<String>,
    },
    /// Manage explore sources (read-only `list` in this release).
    Sources {
        #[command(subcommand)]
        action: SourcesAction,
    },
    /// Install a skill from a Git repository or local path.
    Install {
        /// Git repository URL (HTTPS, SSH) or local filesystem path.
        #[arg(long)]
        url: String,
        /// Optional display name for the skill.
        #[arg(long)]
        name: Option<String>,
        /// Skip confirmation prompts (agent mode).
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Sync a skill to a specific AI tool.
    Sync {
        /// Skill ID or name to sync.
        #[arg(long)]
        skill: String,
        /// Target tool key (e.g. claude_code, codex, cursor).
        #[arg(long)]
        tool: String,
        /// Target scope: global or project.
        #[arg(long, default_value = "global", value_parser = ["global", "project"])]
        scope: String,
        /// Project root, required when --scope project.
        #[arg(long)]
        project_path: Option<String>,
    },
    /// Remove a skill from a specific AI tool (unsync).
    Unsync {
        /// Skill ID or name.
        #[arg(long)]
        skill: String,
        /// Target tool key.
        #[arg(long)]
        tool: String,
        /// Target scope: global or project.
        #[arg(long, default_value = "global", value_parser = ["global", "project"])]
        scope: String,
        /// Project root, required when --scope project.
        #[arg(long)]
        project_path: Option<String>,
    },
    /// Update a skill from its source.
    Update {
        /// Skill ID or name to update.
        #[arg(long)]
        skill: String,
        /// Update all skills (overrides --skill).
        #[arg(long, default_value_t = false)]
        all: bool,
        /// Skip confirmation prompts (agent mode).
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Delete a skill and remove all its sync targets.
    Delete {
        /// Skill ID or name to delete.
        #[arg(long)]
        skill: String,
        /// Skip confirmation prompts (agent mode).
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Commit and push changes for a git-managed skill.
    Push {
        /// Skill ID or name to push.
        #[arg(long)]
        skill: String,
        /// Commit message.
        #[arg(short, long)]
        message: Option<String>,
        /// Skip confirmation prompts (agent mode).
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// View or modify the unified application config (settings).
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage the GitHub token used for private/rate-limited fetches.
    Github {
        #[command(subcommand)]
        action: GithubAction,
    },
    /// Detect and configure the current environment author.
    Author {
        #[command(subcommand)]
        action: AuthorAction,
    },
    /// Inspect project-local Skill directories supported by installed tools.
    Project {
        #[command(subcommand)]
        action: ProjectAction,
    },
    /// Back up the full state (settings + skills list) to a file or WebDAV.
    Backup {
        #[command(subcommand)]
        target: BackupTarget,
    },
    /// Restore the full state from a file or WebDAV backup.
    Restore {
        #[command(subcommand)]
        target: RestoreTarget,
    },
    /// Synchronize the portable desired-state profile across computers.
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
    /// Run the complete cross-device status, pull, or publish workflow.
    Device {
        #[command(subcommand)]
        action: DeviceAction,
    },
    /// Audit or repair managed Skill source metadata.
    Repair {
        #[command(subcommand)]
        action: RepairAction,
    },
}

#[derive(Subcommand)]
enum SourcesAction {
    /// List configured explore sources.
    List,
    /// Add a new explore source.
    Add {
        /// Display name for the source.
        #[arg(long)]
        name: String,
        /// Source kind: featured_json | skills_sh | json_index | git_index.
        #[arg(long)]
        kind: String,
        /// Endpoint URL or path for the source.
        #[arg(long)]
        endpoint: String,
        /// Enable the source immediately (default: enabled).
        #[arg(long, default_value_t = true)]
        enabled: bool,
    },
    /// Edit an existing explore source.
    Edit {
        /// Source id to edit.
        #[arg(long)]
        id: String,
        /// New display name.
        #[arg(long)]
        name: Option<String>,
        /// New source kind.
        #[arg(long)]
        kind: Option<String>,
        /// New endpoint URL or path.
        #[arg(long)]
        endpoint: Option<String>,
        /// New enabled flag.
        #[arg(long)]
        enabled: Option<bool>,
    },
    /// Remove an explore source.
    Remove {
        /// Source id to remove.
        #[arg(long)]
        id: String,
    },
    /// Enable an explore source.
    Enable {
        /// Source id to enable.
        #[arg(long)]
        id: String,
    },
    /// Disable an explore source.
    Disable {
        /// Source id to disable.
        #[arg(long)]
        id: String,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print the full config (or a single key) as JSON / text.
    Get {
        /// Optional config key (language, storage_path, github_token,
        /// git_cache_cleanup_days, git_cache_ttl_secs, webdav.url, ...).
        /// Omit to print the whole config.
        key: Option<String>,
    },
    /// Set a config value by key (dotted keys like `webdav.url` supported).
    Set {
        /// Config key to set.
        key: String,
        /// New value.
        value: Option<String>,
        /// Read the value from stdin (recommended for secrets).
        #[arg(long, default_value_t = false)]
        stdin: bool,
    },
    /// Export the config (settings only) to a JSON file or stdout.
    Export {
        /// Output path. Prints to stdout when omitted.
        path: Option<String>,
    },
    /// Import the config (settings only) from a JSON file.
    Import {
        /// Input path.
        path: String,
    },
}

#[derive(Subcommand)]
enum AuthorAction {
    /// Show configured author plus locally detected gh/git identity.
    Status,
    /// Detect the author from authenticated gh, falling back to git config.
    Detect {
        /// Persist the detected identity and add its GitHub login to myGitOwners.
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    /// Explicitly set the current environment author.
    Set {
        #[arg(long)]
        name: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        github_login: Option<String>,
    },
}

#[derive(Subcommand)]
enum ProjectAction {
    /// List Skill folders found in all supported project-local directories.
    Skills {
        /// Project root (defaults to current directory).
        #[arg(long)]
        path: Option<String>,
    },
}

#[derive(Subcommand)]
#[allow(clippy::enum_variant_names)]
enum GithubAction {
    /// Store the GitHub token.
    TokenSet {
        /// Personal access token.
        token: String,
    },
    /// Print the stored GitHub token.
    TokenGet,
    /// Validate the stored (or provided) token against GitHub.
    TokenValidate {
        /// Optional token to validate; defaults to the stored one.
        token: Option<String>,
    },
}

#[derive(Subcommand)]
enum BackupTarget {
    /// Back up to a local JSON file (or stdout when no path given).
    File {
        /// Output path. Prints to stdout when omitted.
        path: Option<String>,
    },
    /// Back up to the configured WebDAV server.
    Webdav,
}

#[derive(Subcommand)]
enum RestoreTarget {
    /// Restore from a local JSON file.
    File {
        /// Input path.
        path: String,
    },
    /// Restore from the configured WebDAV server.
    Webdav,
}

#[derive(Subcommand)]
enum ProfileAction {
    /// Compare local and remote profiles without changing either side.
    Status,
    /// Merge, apply, update Git skills, and upload the resulting profile.
    Sync {
        /// Also apply deletions propagated from another computer.
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Export the portable Profile to a local JSON file.
    Export {
        /// Output file path.
        path: String,
    },
    /// Import and apply a local Profile JSON file.
    Import {
        /// Input file path.
        path: String,
        /// Conflict strategy: abort | local | remote.
        #[arg(long, default_value = "abort", value_parser = ["abort", "local", "remote"])]
        strategy: String,
        /// Also apply deletions from the imported Profile.
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Resolve current WebDAV conflicts and synchronize.
    Resolve {
        /// Select local or remote values for every reported conflict.
        #[arg(long, value_parser = ["local", "remote"])]
        strategy: String,
        /// Also apply deletions selected by the resolved Profile.
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum DeviceAction {
    /// Inspect local, repository, and WebDAV state without changing it.
    Status,
    /// Pull the WebDAV Profile and apply repository/package/config changes.
    Pull {
        /// Also apply deletions propagated by another device.
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
    /// Refresh sources, push owned changes, then upload Profile and full backup.
    Publish {
        /// Allow Git commits and pushes for owned repositories.
        #[arg(long, default_value_t = false)]
        yes: bool,
    },
}

#[derive(Subcommand)]
enum RepairAction {
    /// Detect local records whose source path belongs to a Git repository.
    Sources {
        /// Apply high-confidence repairs. Omit for a read-only dry run.
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
    /// Reconnect one Skill to a verified Git remote and subpath.
    Source {
        /// Skill ID or name.
        #[arg(long)]
        skill: String,
        /// Git repository URL. The repository is cloned before any write.
        #[arg(long)]
        url: String,
        /// Directory containing SKILL.md inside the repository.
        #[arg(long)]
        subpath: Option<String>,
        /// Apply after remote identity validation. Omit for a dry run.
        #[arg(long, default_value_t = false)]
        apply: bool,
    },
}

/// Entry point invoked from the `skilldo` binary.
pub fn run() {
    let wants_json = std::env::args().any(|arg| arg == "--json");
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            use clap::error::ErrorKind;
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) {
                print!("{error}");
                return;
            }
            if wants_json {
                let output = serde_json::json!({
                    "ok": false,
                    "error": error.to_string()
                });
                eprintln!("{}", serde_json::to_string(&output).unwrap_or_default());
            } else {
                let _ = error.print();
            }
            std::process::exit(2);
        }
    };
    if let Err(err) = execute(cli) {
        if wants_json {
            let output = serde_json::json!({
                "ok": false,
                "error": format!("{err:#}")
            });
            eprintln!("{}", serde_json::to_string(&output).unwrap_or_default());
        } else {
            eprintln!("error: {:#}", err);
        }
        std::process::exit(1);
    }
}

fn open_store(db: Option<&PathBuf>) -> Result<SkillStore> {
    let db_path = match db {
        Some(p) => p.clone(),
        None => default_db_path_cli().context("failed to resolve default database path")?,
    };
    let store = SkillStore::new(db_path.clone());
    store
        .ensure_schema()
        .with_context(|| format!("failed to open db at \"{}\"", db_path.display()))?;
    // Keep legacy-database migration in sync with the desktop client. A failed
    // migration is non-fatal for read commands, so only surface it as a warning.
    if let Err(err) = crate::core::skill_store::migrate_legacy_db_if_needed(store.db_path()) {
        eprintln!("warning: failed to migrate legacy database: {:#}", err);
    }
    Ok(store)
}

fn execute(cli: Cli) -> Result<()> {
    let store = open_store(cli.db.as_ref())?;

    match cli.command {
        Commands::List { filter } => cmd_list(&store, &filter, cli.json),
        Commands::Status => cmd_status(cli.json),
        Commands::Explore { query } => cmd_explore(&store, query, cli.json),
        Commands::Sources { action } => match action {
            SourcesAction::List => cmd_sources_list(&store, cli.json),
            SourcesAction::Add {
                name,
                kind,
                endpoint,
                enabled,
            } => cmd_sources_add(&store, &name, &kind, &endpoint, enabled, cli.json),
            SourcesAction::Edit {
                id,
                name,
                kind,
                endpoint,
                enabled,
            } => cmd_sources_edit(&store, &id, name, kind, endpoint, enabled, cli.json),
            SourcesAction::Remove { id } => cmd_sources_remove(&store, &id, cli.json),
            SourcesAction::Enable { id } => cmd_sources_toggle(&store, &id, true, cli.json),
            SourcesAction::Disable { id } => cmd_sources_toggle(&store, &id, false, cli.json),
        },
        Commands::Config { action } => match action {
            ConfigAction::Get { key } => cmd_config_get(&store, key.as_deref(), cli.json),
            ConfigAction::Set { key, value, stdin } => {
                cmd_config_set(&store, &key, value.as_deref(), stdin, cli.json)
            }
            ConfigAction::Export { path } => cmd_config_export(&store, path.as_deref(), cli.json),
            ConfigAction::Import { path } => cmd_config_import(&store, &path, cli.json),
        },
        Commands::Github { action } => match action {
            GithubAction::TokenSet { token } => cmd_github_token_set(&store, &token, cli.json),
            GithubAction::TokenGet => cmd_github_token_get(&store, cli.json),
            GithubAction::TokenValidate { token } => {
                cmd_github_token_validate(&store, token.as_deref(), cli.json)
            }
        },
        Commands::Author { action } => match action {
            AuthorAction::Status => cmd_author_status(&store, cli.json),
            AuthorAction::Detect { apply } => cmd_author_detect(&store, apply, cli.json),
            AuthorAction::Set {
                name,
                email,
                github_login,
            } => cmd_author_set(&store, name, email, github_login, cli.json),
        },
        Commands::Project { action } => match action {
            ProjectAction::Skills { path } => cmd_project_skills(path.as_deref(), cli.json),
        },
        Commands::Backup { target } => match target {
            BackupTarget::File { path } => cmd_backup_file(&store, path.as_deref(), cli.json),
            BackupTarget::Webdav => cmd_backup_webdav(&store, cli.json),
        },
        Commands::Restore { target } => match target {
            RestoreTarget::File { path } => cmd_restore_file(&store, &path, cli.json),
            RestoreTarget::Webdav => cmd_restore_webdav(&store, cli.json),
        },
        Commands::Profile { action } => match action {
            ProfileAction::Status => cmd_profile_status(&store, cli.json),
            ProfileAction::Sync { yes } => cmd_profile_sync(&store, yes, cli.json),
            ProfileAction::Export { path } => cmd_profile_export(&store, &path, cli.json),
            ProfileAction::Import {
                path,
                strategy,
                yes,
            } => cmd_profile_import(&store, &path, &strategy, yes, cli.json),
            ProfileAction::Resolve { strategy, yes } => {
                cmd_profile_resolve(&store, &strategy, yes, cli.json)
            }
        },
        Commands::Device { action } => match action {
            DeviceAction::Status => cmd_device(&store, device_status(&store)?, cli.json),
            DeviceAction::Pull { yes } => cmd_device(&store, device_pull(&store, yes)?, cli.json),
            DeviceAction::Publish { yes } => {
                cmd_device(&store, device_publish(&store, yes)?, cli.json)
            }
        },
        Commands::Repair { action } => match action {
            RepairAction::Sources { apply } => cmd_repair_sources(&store, apply, cli.json),
            RepairAction::Source {
                skill,
                url,
                subpath,
                apply,
            } => cmd_repair_source(&store, &skill, &url, subpath.as_deref(), apply, cli.json),
        },
        Commands::Install { url, name, yes } => cmd_install(&store, &url, name, yes, cli.json),
        Commands::Sync {
            skill,
            tool,
            scope,
            project_path,
        } => cmd_sync(
            &store,
            &skill,
            &tool,
            &scope,
            project_path.as_deref(),
            cli.json,
        ),
        Commands::Unsync {
            skill,
            tool,
            scope,
            project_path,
        } => cmd_unsync(
            &store,
            &skill,
            &tool,
            &scope,
            project_path.as_deref(),
            cli.json,
        ),
        Commands::Update { skill, all, yes } => cmd_update(&store, &skill, all, yes, cli.json),
        Commands::Delete { skill, yes } => cmd_delete(&store, &skill, yes, cli.json),
        Commands::Push {
            skill,
            message,
            yes,
        } => cmd_push(&store, &skill, message.as_deref(), yes, cli.json),
    }
}

// ---------------------------------------------------------------------------
// Output models
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliManagedSkill {
    id: String,
    name: String,
    description: Option<String>,
    source_type: String,
    source_ref: Option<String>,
    source_subpath: Option<String>,
    source_revision: Option<String>,
    content_hash: Option<String>,
    syncable: bool,
    central_path: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    last_sync_at: Option<i64>,
    author: CliSkillAuthor,
    origin: Option<CliSkillOrigin>,
    tags: Vec<String>,
    targets: Vec<CliSkillTarget>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliSkillAuthor {
    kind: String,
    name: Option<String>,
    provider: Option<String>,
    repository: Option<String>,
    read_only: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliSkillOrigin {
    kind: String,
    role: String,
    provider: Option<String>,
    remote_url: Option<String>,
    owner: Option<String>,
    repo: Option<String>,
    branch: Option<String>,
    subpath: Option<String>,
    update_strategy: String,
    publish_strategy: String,
    manual_override: bool,
    reason: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliSkillTarget {
    tool: String,
    scope: String,
    project_path: Option<String>,
    target_path: String,
    mode: String,
    status: String,
    last_error: Option<String>,
    synced_at: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliToolStatus {
    id: String,
    display_name: String,
    installed: bool,
    skills_dir: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliInstallResult {
    success: bool,
    skill_id: String,
    name: String,
    central_path: String,
    source_type: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliUpdateResult {
    success: bool,
    skill_id: String,
    name: String,
    previous_revision: Option<String>,
    new_revision: Option<String>,
    updated_targets: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliDeleteResult {
    success: bool,
    skill_id: String,
    name: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliSyncResult {
    success: bool,
    skill_id: String,
    tool: String,
    scope: String,
    project_path: Option<String>,
    target_path: String,
    mode: String,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_list(store: &SkillStore, filter: &str, json: bool) -> Result<()> {
    let config = load_app_config(store)?;
    let records = store
        .list_skills()
        .context("failed to list managed skills")?;
    let mut skills = Vec::with_capacity(records.len());
    for rec in records {
        let syncable = rec.source_type == "git" || rec.source_type == "package";
        // Apply the `--filter` argument.
        match filter {
            "syncable" if !syncable => continue,
            "local" if syncable => continue,
            _ => {}
        }
        let origin_record = store.get_skill_origin(&rec.id)?;
        let fallback_origin = origin_record.is_none().then(|| {
            let (owner, repo) = rec
                .source_ref
                .as_deref()
                .map(parse_github_repository)
                .unwrap_or_default();
            let is_current = owner.as_deref().is_some_and(|value| {
                config
                    .origin_rules
                    .my_git_owners
                    .iter()
                    .any(|mine| mine.eq_ignore_ascii_case(value))
            });
            CliSkillOrigin {
                kind: rec.source_type.clone(),
                role: if rec.source_type == "local" || is_current {
                    "mine"
                } else {
                    "repository"
                }
                .to_string(),
                provider: Some(
                    if rec.source_type == "package" {
                        "npm"
                    } else {
                        &rec.source_type
                    }
                    .to_string(),
                ),
                remote_url: rec.source_ref.clone(),
                owner,
                repo,
                branch: None,
                subpath: rec.source_subpath.clone(),
                update_strategy: if rec.source_type == "git" {
                    "git_pull"
                } else if rec.source_type == "package" {
                    "package_refresh"
                } else {
                    "local_copy"
                }
                .to_string(),
                publish_strategy: if is_current && rec.source_type == "git" {
                    "git_push"
                } else {
                    "none"
                }
                .to_string(),
                manual_override: false,
                reason: Some("derived from Skill source metadata for CLI output".to_string()),
            }
        });
        let origin = origin_record
            .as_ref()
            .map(|item| CliSkillOrigin {
                kind: item.origin_kind.clone(),
                role: item.origin_role.clone(),
                provider: item.provider.clone(),
                remote_url: item.remote_url.clone(),
                owner: item.owner.clone(),
                repo: item.repo.clone(),
                branch: item.branch.clone(),
                subpath: item.subpath.clone(),
                update_strategy: item.update_strategy.clone(),
                publish_strategy: item.publish_strategy.clone(),
                manual_override: item.manual_override,
                reason: item.reason.clone(),
            })
            .or(fallback_origin);
        let package_author = if rec.source_type == "package" {
            rec.source_ref.as_deref().and_then(|source| {
                source
                    .trim_start_matches("npm:")
                    .strip_prefix('@')
                    .and_then(|value| value.split('/').next())
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            })
        } else {
            None
        };
        let author = CliSkillAuthor {
            kind: if origin.as_ref().is_some_and(|item| item.role == "mine") {
                "current".to_string()
            } else {
                "thirdParty".to_string()
            },
            name: origin
                .as_ref()
                .and_then(|item| item.owner.clone())
                .or(package_author),
            provider: origin.as_ref().and_then(|item| item.provider.clone()),
            repository: origin
                .as_ref()
                .and_then(|item| match (&item.owner, &item.repo) {
                    (Some(owner), Some(repo)) => Some(format!("{owner}/{repo}")),
                    _ => None,
                }),
            read_only: true,
        };
        let tags = store
            .get_skill_tags(&rec.id)?
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        let targets = store
            .list_skill_targets(&rec.id)
            .unwrap_or_default()
            .into_iter()
            .map(|t| CliSkillTarget {
                tool: t.tool,
                scope: t.scope,
                project_path: t.project_path,
                target_path: t.target_path,
                mode: t.mode,
                status: t.status,
                last_error: t.last_error,
                synced_at: t.synced_at,
            })
            .collect();
        skills.push(CliManagedSkill {
            id: rec.id,
            name: rec.name,
            description: rec.description,
            source_type: rec.source_type,
            source_ref: rec.source_ref,
            source_subpath: rec.source_subpath,
            source_revision: rec.source_revision,
            content_hash: rec.content_hash,
            syncable,
            central_path: rec.central_path,
            status: rec.status,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
            last_sync_at: rec.last_sync_at,
            author,
            origin,
            tags,
            targets,
        });
    }

    if json {
        print_json(&skills)?;
    } else if skills.is_empty() {
        println!("No managed skills yet.");
    } else {
        for s in &skills {
            let kind = if s.syncable { "syncable" } else { "local" };
            println!(
                "{}  [{}]  ({})  {}  -> {} target(s)",
                s.name,
                s.source_type,
                kind,
                s.central_path,
                s.targets.len()
            );
            for t in &s.targets {
                println!("    - {} ({})", t.tool, t.status);
            }
        }
        println!("\n{} skill(s) total.", skills.len());
    }
    Ok(())
}

fn parse_github_repository(source: &str) -> (Option<String>, Option<String>) {
    let normalized = source
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .replace("git@github.com:", "https://github.com/");
    let Some((_, tail)) = normalized.split_once("github.com/") else {
        return (None, None);
    };
    let mut parts = tail.split('/');
    (
        parts
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        parts
            .next()
            .filter(|value| !value.is_empty())
            .map(str::to_string),
    )
}

fn cmd_status(json: bool) -> Result<()> {
    let adapters = tool_adapters::default_tool_adapters();
    let mut statuses = Vec::with_capacity(adapters.len());
    let mut installed_count = 0usize;
    for adapter in &adapters {
        let installed = tool_adapters::is_tool_installed(adapter).unwrap_or(false);
        let skills_dir = tool_adapters::resolve_default_path(adapter)
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();
        if installed {
            installed_count += 1;
        }
        statuses.push(CliToolStatus {
            id: adapter.id.as_key().to_string(),
            display_name: adapter.display_name.to_string(),
            installed,
            skills_dir,
        });
    }

    if json {
        print_json(&statuses)?;
    } else {
        println!("Supported tools: {}", statuses.len());
        for s in &statuses {
            let mark = if s.installed { "yes" } else { "no" };
            println!("  [{}] {} ({})", mark, s.display_name, s.id);
        }
        println!("\n{} tool(s) installed.", installed_count);
    }
    Ok(())
}

fn cmd_explore(store: &SkillStore, query: Option<String>, json: bool) -> Result<()> {
    let limit = 100usize;
    let result = explore_sources::get_explore_skills(store, query.as_deref(), limit)
        .context("failed to fetch explore skills")?;

    if json {
        print_json(&result)?;
    } else {
        for e in &result.errors {
            eprintln!("warning: {}", e);
        }
        if result.skills.is_empty() {
            println!("No skills found from enabled sources.");
        } else {
            for s in &result.skills {
                println!(
                    "{}  [{}]  {}  stars={} downloads={}",
                    s.name, s.source_kind, s.source_url, s.stars, s.downloads
                );
            }
            println!(
                "\n{} skill(s) from enabled sources ({} source error(s)).",
                result.skills.len(),
                result.errors.len()
            );
        }
    }
    Ok(())
}

fn cmd_sources_list(store: &SkillStore, json: bool) -> Result<()> {
    let sources: Vec<ExploreSourceConfig> =
        explore_sources::get_explore_sources(store).context("failed to list explore sources")?;

    if json {
        print_json(&sources)?;
    } else if sources.is_empty() {
        println!("No explore sources configured.");
    } else {
        for s in &sources {
            let mark = if s.enabled { "on" } else { "off" };
            println!("  [{}] {}  ({})  {}", mark, s.name, s.kind, s.endpoint);
        }
        println!("\n{} source(s) configured.", sources.len());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Write commands
// ---------------------------------------------------------------------------

// ===========================================================================
// Config (settings) subcommands
// ===========================================================================

fn load_webdav_cfg(store: &SkillStore) -> Result<WebDavConfig> {
    let cfg = load_app_config(store)?;
    cfg.webdav.ok_or_else(|| {
        anyhow::anyhow!("WebDAV 未配置，请先用 `skilldo config set webdav.url ...` 设置")
    })
}

fn config_key_path(key: &str) -> Vec<&str> {
    key.split('.').collect()
}

fn canonical_config_key(key: &str) -> String {
    key.split('.')
        .map(|part| match part {
            "storage_path" => "storagePath",
            "github_token" => "githubToken",
            "git_cache_cleanup_days" => "gitCacheCleanupDays",
            "git_cache_ttl_secs" => "gitCacheTtlSecs",
            "origin_rules" => "originRules",
            "current_author" => "currentAuthor",
            "tool_dir_overrides" => "toolDirOverrides",
            "custom_scan_dirs" => "customScanDirs",
            "explore_sources" => "exploreSources",
            "my_git_owners" => "myGitOwners",
            "my_git_repos" => "myGitRepos",
            "official_git_repos" => "officialGitRepos",
            "github_login" => "githubLogin",
            "github_url" => "githubUrl",
            "remote_dir" => "remoteDir",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn config_set_value(cfg: &mut AppConfig, key: &str, value: &str) -> Result<()> {
    if key.starts_with("webdav.") && cfg.webdav.is_none() {
        cfg.webdav = Some(WebDavConfig::default());
    }
    let mut root = serde_json::to_value(&*cfg)?;
    let canonical_key = canonical_config_key(key);
    let path = config_key_path(&canonical_key);
    let parsed = serde_json::from_str::<serde_json::Value>(value)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_string()));
    let mut cursor = &mut root;
    for part in &path[..path.len().saturating_sub(1)] {
        cursor = cursor
            .as_object_mut()
            .and_then(|map| map.get_mut(*part))
            .ok_or_else(|| anyhow::anyhow!("未知配置键: {key}"))?;
    }
    let leaf = path
        .last()
        .ok_or_else(|| anyhow::anyhow!("配置键不能为空"))?;
    let map = cursor
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("配置键不是对象路径: {key}"))?;
    if !map.contains_key(*leaf) {
        anyhow::bail!("未知配置键: {key}");
    }
    map.insert((*leaf).to_string(), parsed);
    *cfg = serde_json::from_value(root).with_context(|| format!("配置值类型不匹配: {key}"))?;
    cfg.validate()?;
    Ok(())
}

fn cmd_config_get(store: &SkillStore, key: Option<&str>, json: bool) -> Result<()> {
    let cfg = load_app_config(store)?;
    match key {
        None if json => print_json(&cfg.sanitized_for_export())?,
        None => println!("{}", export_config_json(&cfg)),
        Some(k) => {
            let mut value = serde_json::to_value(cfg.sanitized_for_export())?;
            let canonical_key = canonical_config_key(k);
            for part in config_key_path(&canonical_key) {
                value = value
                    .get(part)
                    .cloned()
                    .ok_or_else(|| anyhow::anyhow!("未知配置键: {k}"))?;
            }
            if json {
                print_json(&serde_json::json!({"key": k, "value": value}))?;
            } else {
                match value {
                    serde_json::Value::String(text) => println!("{text}"),
                    other => println!("{}", serde_json::to_string_pretty(&other)?),
                }
            }
        }
    }
    Ok(())
}

fn cmd_config_set(
    store: &SkillStore,
    key: &str,
    value: Option<&str>,
    stdin: bool,
    json: bool,
) -> Result<()> {
    if stdin && value.is_some() {
        anyhow::bail!("不能同时提供 value 和 --stdin");
    }
    let mut stdin_value = String::new();
    let value = if stdin {
        std::io::stdin().read_to_string(&mut stdin_value)?;
        stdin_value.trim_end_matches(['\r', '\n'])
    } else {
        value.ok_or_else(|| anyhow::anyhow!("缺少配置值；也可以使用 --stdin"))?
    };
    let mut cfg = load_app_config(store)?;
    config_set_value(&mut cfg, key, value)?;
    save_app_config_impl(store, &cfg)?;
    if json {
        print_json(
            &serde_json::json!({"ok": true, "key": key, "sensitive": matches!(key, "github_token" | "webdav.password")}),
        )?;
    } else if matches!(key, "github_token" | "webdav.password") {
        println!("已更新配置: {key} = [已隐藏]");
    } else {
        println!("已更新配置: {key} = {value}");
    }
    Ok(())
}

fn cmd_config_export(store: &SkillStore, path: Option<&str>, json_output: bool) -> Result<()> {
    let cfg = load_app_config(store)?;
    let json = export_config_json(&cfg);
    match path {
        Some(p) => {
            std::fs::write(p, &json).with_context(|| format!("写入文件失败: {p}"))?;
            if json_output {
                print_json(&serde_json::json!({"ok": true, "path": p}))?;
            } else {
                println!("配置已导出到 {p}");
            }
        }
        None => println!("{json}"),
    }
    Ok(())
}

fn cmd_config_import(store: &SkillStore, path: &str, json: bool) -> Result<()> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("读取文件失败: {path}"))?;
    let mut cfg = parse_config_json(&raw)?;
    let current = load_app_config(store)?;
    cfg.preserve_missing_secrets_from(&current);
    save_app_config_impl(store, &cfg)?;
    if json {
        print_json(&serde_json::json!({"ok": true, "path": path}))?;
    } else {
        println!("配置已从 {path} 导入");
    }
    Ok(())
}

#[derive(Debug, Deserialize, Serialize, Default)]
struct DetectedAuthor {
    name: String,
    email: String,
    github_login: String,
    github_url: String,
    source: String,
    gh_authenticated: bool,
}

fn command_text(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn detect_local_author() -> DetectedAuthor {
    if let Some(raw) = command_text("gh", &["api", "user"]) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            let login = value
                .get("login")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if !login.is_empty() {
                return DetectedAuthor {
                    name: value
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(login)
                        .to_string(),
                    email: value
                        .get("email")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    github_login: login.to_string(),
                    github_url: value
                        .get("html_url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string(),
                    source: "gh".to_string(),
                    gh_authenticated: true,
                };
            }
        }
    }
    DetectedAuthor {
        name: command_text("git", &["config", "--global", "user.name"]).unwrap_or_default(),
        email: command_text("git", &["config", "--global", "user.email"]).unwrap_or_default(),
        source: "git".to_string(),
        ..DetectedAuthor::default()
    }
}

fn cmd_author_status(store: &SkillStore, json: bool) -> Result<()> {
    let configured = load_app_config(store)?.current_author;
    let detected = detect_local_author();
    let output = serde_json::json!({"configured": configured, "detected": detected});
    if json {
        print_json(&output)?;
    } else {
        println!(
            "当前作者: {}",
            if configured.name.is_empty() {
                "(未设置)"
            } else {
                &configured.name
            }
        );
        println!(
            "GitHub: {}",
            if detected.github_login.is_empty() {
                "(未检测到登录)"
            } else {
                &detected.github_login
            }
        );
    }
    Ok(())
}

fn cmd_author_detect(store: &SkillStore, apply: bool, json: bool) -> Result<()> {
    let detected = detect_local_author();
    if detected.name.is_empty() && detected.github_login.is_empty() && detected.email.is_empty() {
        anyhow::bail!("未检测到 gh 登录或全局 git 作者信息");
    }
    if apply {
        let mut cfg = load_app_config(store)?;
        cfg.current_author = CurrentAuthorConfig {
            name: detected.name.clone(),
            email: detected.email.clone(),
            github_login: detected.github_login.clone(),
            github_url: detected.github_url.clone(),
            source: detected.source.clone(),
        };
        if !detected.github_login.is_empty() {
            cfg.origin_rules
                .my_git_owners
                .push(detected.github_login.clone());
        }
        save_app_config_impl(store, &cfg)?;
        if !detected.github_login.is_empty() {
            for skill in store.list_skills()? {
                let Some(mut origin) = store.get_skill_origin(&skill.id)? else {
                    continue;
                };
                if origin.manual_override
                    || origin.origin_kind == "official"
                    || origin.origin_role == "official"
                {
                    continue;
                }
                if origin.origin_kind == "git" {
                    let is_current = origin
                        .owner
                        .as_deref()
                        .is_some_and(|owner| owner.eq_ignore_ascii_case(&detected.github_login));
                    origin.origin_role = if is_current { "mine" } else { "repository" }.to_string();
                    origin.publish_strategy =
                        if is_current { "git_push" } else { "none" }.to_string();
                    origin.reason = Some(
                        if is_current {
                            "repository owner matches detected current GitHub author"
                        } else {
                            "repository owner differs from detected current GitHub author"
                        }
                        .to_string(),
                    );
                    store.upsert_skill_origin(&origin)?;
                }
            }
        }
    }
    let output = serde_json::json!({"ok": true, "applied": apply, "author": detected});
    if json {
        print_json(&output)?;
    } else {
        println!("{}", serde_json::to_string_pretty(&output)?);
    }
    Ok(())
}

fn cmd_author_set(
    store: &SkillStore,
    name: Option<String>,
    email: Option<String>,
    github_login: Option<String>,
    json: bool,
) -> Result<()> {
    let mut cfg = load_app_config(store)?;
    if let Some(value) = name {
        cfg.current_author.name = value;
    }
    if let Some(value) = email {
        cfg.current_author.email = value;
    }
    if let Some(value) = github_login {
        cfg.current_author.github_login = value.clone();
        cfg.current_author.github_url = if value.is_empty() {
            String::new()
        } else {
            format!("https://github.com/{value}")
        };
        cfg.origin_rules.my_git_owners.push(value);
    }
    cfg.current_author.source = "manual".to_string();
    save_app_config_impl(store, &cfg)?;
    if json {
        print_json(&serde_json::json!({"ok": true, "author": cfg.current_author}))?;
    } else {
        println!("当前环境作者已更新");
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectSkillEntry {
    name: String,
    path: String,
    relative_dir: String,
    tools: Vec<String>,
}

fn cmd_project_skills(project_path: Option<&str>, json: bool) -> Result<()> {
    let root = match project_path {
        Some(value) => PathBuf::from(value),
        None => std::env::current_dir()?,
    }
    .canonicalize()
    .context("项目路径不存在")?;
    let mut dirs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for adapter in tool_adapters::default_tool_adapters() {
        if tool_adapters::supports_project_scope(&adapter) {
            let relative = tool_adapters::project_relative_skills_dir(&adapter).to_string();
            dirs.entry(relative)
                .or_default()
                .insert(adapter.id.as_key().to_string());
        }
    }
    let mut entries = Vec::new();
    for (relative, tools) in dirs {
        let skills_dir = root.join(&relative);
        let Ok(children) = std::fs::read_dir(&skills_dir) else {
            continue;
        };
        for child in children.flatten() {
            let skill_path = child.path();
            if skill_path.is_dir() && skill_path.join("SKILL.md").is_file() {
                entries.push(ProjectSkillEntry {
                    name: child.file_name().to_string_lossy().to_string(),
                    path: skill_path.to_string_lossy().to_string(),
                    relative_dir: relative.clone(),
                    tools: tools.iter().cloned().collect(),
                });
            }
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name).then(a.path.cmp(&b.path)));
    if json {
        print_json(&serde_json::json!({"projectPath": root, "skills": entries}))?;
    } else {
        for entry in &entries {
            println!(
                "{}  {}  [{}]",
                entry.name,
                entry.path,
                entry.tools.join(",")
            );
        }
        println!("\n{} project Skill(s).", entries.len());
    }
    Ok(())
}

// ===========================================================================
// Github token subcommands
// ===========================================================================

fn cmd_github_token_set(store: &SkillStore, token: &str, json: bool) -> Result<()> {
    let mut cfg = load_app_config(store)?;
    cfg.github_token = token.to_string();
    save_app_config_impl(store, &cfg)?;
    if json {
        print_json(&serde_json::json!({"ok": true, "configured": true}))?;
    } else {
        println!("GitHub token 已保存");
    }
    Ok(())
}

fn cmd_github_token_get(store: &SkillStore, json: bool) -> Result<()> {
    let cfg = load_app_config(store)?;
    if json {
        print_json(&serde_json::json!({
            "configured": !cfg.github_token.is_empty(),
            "token": cfg.github_token
        }))?;
    } else if cfg.github_token.is_empty() {
        println!("(未配置 GitHub token)");
    } else {
        println!("{}", cfg.github_token);
    }
    Ok(())
}

fn cmd_github_token_validate(store: &SkillStore, token: Option<&str>, json: bool) -> Result<()> {
    let token = match token {
        Some(t) => t.to_string(),
        None => load_app_config(store)?.github_token,
    };
    let status = compute_github_token_status(token);
    if json {
        print_json(&status)?;
    } else if status.valid {
        println!("token 有效");
        if let Some(login) = &status.login {
            println!("  登录用户: {login}");
        }
        if !status.scopes.is_empty() {
            println!("  权限范围: {}", status.scopes.join(", "));
        }
    } else {
        println!("token 无效: {}", status.error.unwrap_or_default());
    }
    Ok(())
}

// ===========================================================================
// Explore sources management subcommands
// ===========================================================================

fn cmd_sources_add(
    store: &SkillStore,
    name: &str,
    kind: &str,
    endpoint: &str,
    enabled: bool,
    json: bool,
) -> Result<()> {
    let mut sources = explore_sources::get_explore_sources(store)?;
    let id = format!(
        "cli-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    );
    sources.push(ExploreSourceConfig {
        id,
        name: name.to_string(),
        kind: kind.to_string(),
        endpoint: endpoint.to_string(),
        enabled,
        builtin: false,
    });
    explore_sources::save_explore_sources(store, &sources)?;
    if json {
        print_json(&sources)?;
    } else {
        println!("已添加源: {name} ({kind})");
    }
    Ok(())
}

fn cmd_sources_edit(
    store: &SkillStore,
    id: &str,
    name: Option<String>,
    kind: Option<String>,
    endpoint: Option<String>,
    enabled: Option<bool>,
    json: bool,
) -> Result<()> {
    let mut sources = explore_sources::get_explore_sources(store)?;
    let src = sources
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| anyhow::anyhow!("未找到源: {id}"))?;
    if let Some(v) = name {
        src.name = v;
    }
    if let Some(v) = kind {
        src.kind = v;
    }
    if let Some(v) = endpoint {
        src.endpoint = v;
    }
    if let Some(v) = enabled {
        src.enabled = v;
    }
    explore_sources::save_explore_sources(store, &sources)?;
    if json {
        print_json(&sources)?;
    } else {
        println!("已更新源: {id}");
    }
    Ok(())
}

fn cmd_sources_remove(store: &SkillStore, id: &str, json: bool) -> Result<()> {
    let mut sources = explore_sources::get_explore_sources(store)?;
    let before = sources.len();
    sources.retain(|s| s.id != id);
    if sources.len() == before {
        anyhow::bail!("未找到源: {id}");
    }
    explore_sources::save_explore_sources(store, &sources)?;
    if json {
        print_json(&sources)?;
    } else {
        println!("已删除源: {id}");
    }
    Ok(())
}

fn cmd_sources_toggle(store: &SkillStore, id: &str, enabled: bool, json: bool) -> Result<()> {
    let mut sources = explore_sources::get_explore_sources(store)?;
    let src = sources
        .iter_mut()
        .find(|s| s.id == id)
        .ok_or_else(|| anyhow::anyhow!("未找到源: {id}"))?;
    src.enabled = enabled;
    explore_sources::save_explore_sources(store, &sources)?;
    if json {
        print_json(&sources)?;
    } else {
        println!("源 {} 已{}", id, if enabled { "启用" } else { "禁用" });
    }
    Ok(())
}

// ===========================================================================
// Backup / restore subcommands
// ===========================================================================

fn cmd_backup_file(store: &SkillStore, path: Option<&str>, json_output: bool) -> Result<()> {
    let backup = export_full_backup(store)?;
    match path {
        Some(p) => {
            std::fs::write(p, &backup).with_context(|| format!("写入文件失败: {p}"))?;
            if json_output {
                print_json(&serde_json::json!({"ok": true, "target": "file", "path": p}))?;
            } else {
                println!("已备份到 {p}");
            }
        }
        None => println!("{backup}"),
    }
    Ok(())
}

fn cmd_backup_webdav(store: &SkillStore, json: bool) -> Result<()> {
    let wd = load_webdav_cfg(store)?;
    let body = export_full_backup(store)?;
    let remote = upload_backup(&wd, &body)?;
    if json {
        print_json(&serde_json::json!({"ok": true, "target": "webdav", "remotePath": remote}))?;
    } else {
        println!("已备份到 WebDAV: {remote}");
    }
    Ok(())
}

fn cmd_restore_file(store: &SkillStore, path: &str, json: bool) -> Result<()> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("读取文件失败: {path}"))?;
    let report = restore_full_backup(store, &raw)?;
    if json {
        print_json(&report)?;
    } else {
        print_restore_report(&report);
    }
    Ok(())
}

fn cmd_restore_webdav(store: &SkillStore, json: bool) -> Result<()> {
    let wd = load_webdav_cfg(store)?;
    let raw = download_backup(&wd)?;
    let report = restore_full_backup(store, &raw)?;
    if json {
        print_json(&report)?;
    } else {
        print_restore_report(&report);
    }
    Ok(())
}

fn print_restore_report(report: &RestoreReport) {
    println!("恢复完成: {}", report.summary());
    for name in &report.installed {
        println!("  + 已安装: {name}");
    }
    for (name, reason) in &report.skipped {
        println!("  - 跳过: {name} ({reason})");
    }
    for (name, err) in &report.failed {
        println!("  ! 失败: {name} ({err})");
    }
}

fn print_profile_report(report: &ProfileSyncReport) {
    println!("Profile: {}", report.profile_id);
    println!("Device: {}", report.device_id);
    println!(
        "remote={} changed={} uploaded={} conflicts={} failures={}",
        report.remote_found,
        report.changed,
        report.uploaded,
        report.conflicts.len(),
        report.failures.len()
    );
    for conflict in &report.conflicts {
        println!("  ! conflict {}: {}", conflict.path, conflict.reason);
    }
    for name in &report.installed {
        println!("  + installed: {name}");
    }
    for name in &report.updated {
        println!("  ↑ updated: {name}");
    }
    for name in &report.deleted {
        println!("  - deleted: {name}");
    }
    for name in &report.pending_deletions {
        println!("  ? pending deletion: {name} (rerun with --yes)");
    }
    for name in &report.skipped_local {
        println!("  · local-only: {name}");
    }
    for (name, error) in &report.failures {
        println!("  ! {name}: {error}");
    }
}

fn cmd_device(_store: &SkillStore, report: DevicePipelineReport, json: bool) -> Result<()> {
    if json {
        return print_json(&report);
    }
    println!("Device {}: {}", report.mode, report.state);
    for item in &report.stages {
        println!("  [{}] {}: {}", item.status, item.id, item.message);
    }
    println!(
        "pushable={} dirty={} pullable={} local-only={} pushed={} failures={}",
        report.pushable_repositories,
        report.dirty_repositories,
        report.pullable_skills,
        report.local_only_skills.len(),
        report.pushed.len(),
        report.failures.len()
    );
    for (name, error) in &report.failures {
        println!("  ! {name}: {error}");
    }
    Ok(())
}

fn cmd_profile_status(store: &SkillStore, json: bool) -> Result<()> {
    let report = synchronize_profile(store, true, false, ConflictStrategy::Abort)?;
    if json {
        print_json(&report)
    } else {
        print_profile_report(&report);
        Ok(())
    }
}

fn cmd_profile_sync(store: &SkillStore, apply_deletions: bool, json: bool) -> Result<()> {
    let report = synchronize_profile(store, false, apply_deletions, ConflictStrategy::Abort)?;
    if json {
        print_json(&report)
    } else {
        print_profile_report(&report);
        Ok(())
    }
}

fn cmd_profile_export(store: &SkillStore, path: &str, json: bool) -> Result<()> {
    let profile = export_profile_json(store)?;
    std::fs::write(path, profile).with_context(|| format!("写入 Profile 失败: {path}"))?;
    if json {
        print_json(&serde_json::json!({"ok": true, "path": path}))
    } else {
        println!("Profile 已导出到 {path}");
        Ok(())
    }
}

fn cmd_profile_import(
    store: &SkillStore,
    path: &str,
    strategy: &str,
    apply_deletions: bool,
    json: bool,
) -> Result<()> {
    let raw =
        std::fs::read_to_string(path).with_context(|| format!("读取 Profile 失败: {path}"))?;
    let report = import_profile_json(
        store,
        &raw,
        ConflictStrategy::parse(strategy)?,
        apply_deletions,
    )?;
    if json {
        print_json(&report)
    } else {
        print_profile_report(&report);
        Ok(())
    }
}

fn cmd_profile_resolve(
    store: &SkillStore,
    strategy: &str,
    apply_deletions: bool,
    json: bool,
) -> Result<()> {
    let report = synchronize_profile(
        store,
        false,
        apply_deletions,
        ConflictStrategy::parse(strategy)?,
    )?;
    if json {
        print_json(&report)
    } else {
        print_profile_report(&report);
        Ok(())
    }
}

fn cmd_repair_sources(store: &SkillStore, apply: bool, json: bool) -> Result<()> {
    let report = repair_skill_sources(store, apply)?;
    if json {
        print_json(&report)
    } else {
        println!(
            "scanned={} repairable={} applied={} unresolved={} already-portable={}",
            report.scanned,
            report.repairable,
            report.applied,
            report.unresolved,
            report.already_portable
        );
        for item in &report.items {
            println!("  {} {}: {}", item.status, item.name, item.reason);
        }
        Ok(())
    }
}

fn cmd_repair_source(
    store: &SkillStore,
    skill: &str,
    url: &str,
    subpath: Option<&str>,
    apply: bool,
    json: bool,
) -> Result<()> {
    let report = repair_skill_source(store, skill, url, subpath, apply)?;
    if json {
        print_json(&report)
    } else {
        println!(
            "scanned={} repairable={} applied={} already-portable={}",
            report.scanned, report.repairable, report.applied, report.already_portable
        );
        Ok(())
    }
}

/// Resolve a skill name or ID to a skill ID. If the input looks like a UUID,
/// try it directly; otherwise search by name.
fn resolve_skill_id(store: &SkillStore, name_or_id: &str) -> Result<String> {
    // Try exact ID match first.
    if let Some(record) = store.get_skill_by_id(name_or_id)? {
        return Ok(record.id);
    }
    // Try name match (case-insensitive).
    let all = store.list_skills()?;
    let lower = name_or_id.to_lowercase();
    let matches: Vec<_> = all
        .iter()
        .filter(|s| s.name.to_lowercase() == lower)
        .collect();
    match matches.len() {
        0 => anyhow::bail!("skill not found: {}", name_or_id),
        1 => Ok(matches[0].id.clone()),
        _ => {
            let ids: Vec<_> = matches
                .iter()
                .map(|s| format!("  {} ({})", s.id, s.name))
                .collect();
            anyhow::bail!(
                "ambiguous name '{}' matches {} skills:\n{}",
                name_or_id,
                matches.len(),
                ids.join("\n")
            );
        }
    }
}

fn cmd_install(
    store: &SkillStore,
    url: &str,
    name: Option<String>,
    yes: bool,
    json: bool,
) -> Result<()> {
    // Determine if this is a local path or a git URL.
    let is_local = !url.starts_with("http://")
        && !url.starts_with("https://")
        && !url.starts_with("git@")
        && !url.ends_with(".git");

    if is_local {
        let path = std::path::PathBuf::from(url);
        if !path.exists() {
            anyhow::bail!("local path not found: {:?}", path);
        }
        if !yes {
            eprintln!("About to install skill from local path: {}", path.display());
            eprint!("Continue? [y/N] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                anyhow::bail!("cancelled by user");
            }
        }
        let result = installer::install_local_skill_cli(store, &path, name)?;
        let source_type = store
            .get_skill_by_id(&result.skill_id)?
            .map(|skill| skill.source_type)
            .unwrap_or_else(|| "local".to_string());
        let out = CliInstallResult {
            success: true,
            skill_id: result.skill_id,
            name: result.name,
            central_path: result.central_path.to_string_lossy().to_string(),
            source_type,
        };
        if json {
            print_json(&out)?;
        } else {
            println!("Installed skill '{}' from local path.", out.name);
            println!("  ID: {}", out.skill_id);
            println!("  Path: {}", out.central_path);
        }
    } else {
        if !yes {
            eprintln!("About to install skill from: {}", url);
            eprint!("Continue? [y/N] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                anyhow::bail!("cancelled by user");
            }
        }
        let result = installer::install_git_skill_cli(store, url, name)?;
        let out = CliInstallResult {
            success: true,
            skill_id: result.skill_id,
            name: result.name,
            central_path: result.central_path.to_string_lossy().to_string(),
            source_type: "git".to_string(),
        };
        if json {
            print_json(&out)?;
        } else {
            println!("Installed skill '{}' from git.", out.name);
            println!("  ID: {}", out.skill_id);
            println!("  Path: {}", out.central_path);
        }
    }
    Ok(())
}

fn cmd_sync(
    store: &SkillStore,
    skill_name: &str,
    tool_key: &str,
    scope: &str,
    project_path: Option<&str>,
    json: bool,
) -> Result<()> {
    let skill_id = resolve_skill_id(store, skill_name)?;
    let outcome =
        installer::sync_skill_target_cli(store, &skill_id, tool_key, scope, project_path)?;
    let out = CliSyncResult {
        success: true,
        skill_id,
        tool: tool_key.to_string(),
        scope: scope.to_string(),
        project_path: project_path.map(str::to_string),
        target_path: outcome.target_path.to_string_lossy().to_string(),
        mode: format!("{:?}", outcome.mode_used),
    };
    if json {
        print_json(&out)?;
    } else {
        println!(
            "Synced '{}' to {} via {}.",
            out.skill_id, out.tool, out.mode
        );
        println!("  Target: {}", out.target_path);
    }
    Ok(())
}

fn cmd_unsync(
    store: &SkillStore,
    skill_name: &str,
    tool_key: &str,
    scope: &str,
    project_path: Option<&str>,
    json: bool,
) -> Result<()> {
    let skill_id = resolve_skill_id(store, skill_name)?;
    installer::unsync_skill_target_cli(store, &skill_id, tool_key, scope, project_path)?;
    if json {
        let out = serde_json::json!({
            "success": true,
            "skillId": skill_id,
            "tool": tool_key,
            "scope": scope,
            "projectPath": project_path,
        });
        print_json(&out)?;
    } else {
        println!("Unsynced '{}' from {}.", skill_id, tool_key);
    }
    Ok(())
}

fn cmd_update(
    store: &SkillStore,
    skill_name: &str,
    all: bool,
    yes: bool,
    json: bool,
) -> Result<()> {
    if all {
        // Update all skills.
        let skills = store.list_skills()?;
        let git_skills: Vec<_> = skills.iter().filter(|s| s.source_type == "git").collect();
        if git_skills.is_empty() {
            if json {
                println!("[]");
            } else {
                println!("No updatable skills found.");
            }
            return Ok(());
        }
        if !yes {
            eprintln!(
                "About to update {} git skill(s). This will pull from remote sources.",
                git_skills.len()
            );
            eprint!("Continue? [y/N] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                anyhow::bail!("cancelled by user");
            }
        }
        let mut results = Vec::new();
        for skill in &git_skills {
            match installer::update_managed_skill_from_source_cli(store, &skill.id) {
                Ok(result) => {
                    results.push(CliUpdateResult {
                        success: true,
                        skill_id: result.skill_id,
                        name: result.name,
                        previous_revision: result.source_revision.clone(),
                        new_revision: result.source_revision,
                        updated_targets: result.updated_targets,
                    });
                }
                Err(err) => {
                    if json {
                        results.push(CliUpdateResult {
                            success: false,
                            skill_id: skill.id.clone(),
                            name: skill.name.clone(),
                            previous_revision: None,
                            new_revision: None,
                            updated_targets: vec![],
                        });
                    } else {
                        eprintln!("warning: failed to update {}: {:#}", skill.name, err);
                    }
                }
            }
        }
        if json {
            print_json(&results)?;
        } else {
            let ok = results.iter().filter(|r| r.success).count();
            println!("\n{}/{} skill(s) updated.", ok, results.len());
        }
    } else {
        let skill_id = resolve_skill_id(store, skill_name)?;
        let record = store
            .get_skill_by_id(&skill_id)?
            .ok_or_else(|| anyhow::anyhow!("skill not found"))?;
        if record.source_type != "git" {
            anyhow::bail!(
                "skill '{}' is not a git skill (source_type={}), cannot update from source",
                record.name,
                record.source_type
            );
        }
        if !yes {
            eprintln!(
                "About to update skill '{}' from {}.",
                record.name,
                record.source_ref.as_deref().unwrap_or("unknown")
            );
            eprint!("Continue? [y/N] ");
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            if !input.trim().eq_ignore_ascii_case("y") {
                anyhow::bail!("cancelled by user");
            }
        }
        let result = installer::update_managed_skill_from_source_cli(store, &skill_id)?;
        let out = CliUpdateResult {
            success: true,
            skill_id: result.skill_id,
            name: result.name,
            previous_revision: None,
            new_revision: result.source_revision,
            updated_targets: result.updated_targets,
        };
        if json {
            print_json(&out)?;
        } else {
            println!("Updated skill '{}'.", out.name);
            if !out.updated_targets.is_empty() {
                println!("  Re-synced targets: {}", out.updated_targets.join(", "));
            }
        }
    }
    Ok(())
}

fn cmd_delete(store: &SkillStore, skill_name: &str, yes: bool, json: bool) -> Result<()> {
    let skill_id = resolve_skill_id(store, skill_name)?;
    let record = store
        .get_skill_by_id(&skill_id)?
        .ok_or_else(|| anyhow::anyhow!("skill not found"))?;

    if !yes {
        let targets = store.list_skill_targets(&skill_id)?;
        eprintln!(
            "About to delete skill '{}' ({} target(s), central path: {}).",
            record.name,
            targets.len(),
            record.central_path
        );
        eprintln!("This will remove the central repo copy and all symlinks.");
        eprint!("Continue? [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            anyhow::bail!("cancelled by user");
        }
    }

    let name = record.name.clone();
    installer::delete_skill_cli(store, &skill_id)?;

    if json {
        let out = CliDeleteResult {
            success: true,
            skill_id,
            name,
        };
        print_json(&out)?;
    } else {
        println!("Deleted skill '{}'.", name);
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliPushResult {
    success: bool,
    skill_id: String,
    name: String,
    committed: bool,
    pushed: bool,
    message: String,
}

fn cmd_push(
    store: &SkillStore,
    skill_name: &str,
    message: Option<&str>,
    yes: bool,
    json: bool,
) -> Result<()> {
    let skill_id = resolve_skill_id(store, skill_name)?;
    let record = store
        .get_skill_by_id(&skill_id)?
        .ok_or_else(|| anyhow::anyhow!("skill not found"))?;

    if !yes {
        eprintln!(
            "About to commit and push changes for skill '{}' ({})",
            record.name, record.central_path
        );
        eprint!("Continue? [y/N] ");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            anyhow::bail!("cancelled by user");
        }
    }

    let result = installer::push_skill_cli(store, &skill_id, message)?;
    let out = CliPushResult {
        success: true,
        skill_id,
        name: record.name,
        committed: result.committed,
        pushed: result.pushed,
        message: result.message,
    };

    if json {
        print_json(&out)?;
    } else {
        if out.committed {
            println!("Committed and pushed '{}'.", out.name);
        } else {
            println!("No changes to commit for '{}'.", out.name);
        }
        if !out.message.is_empty() {
            println!("  {}", out.message);
        }
    }
    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    let s = serde_json::to_string_pretty(value).context("failed to serialize JSON")?;
    println!("{}", s);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_db_path_matches_app_identifier() {
        // Ensures the CLI resolves the same directory the GUI uses.
        let path = default_db_path_cli().expect("resolve default db path");
        assert!(path
            .to_string_lossy()
            .contains(crate::core::skill_store::APP_IDENTIFIER));
        assert!(path.to_string_lossy().ends_with("skills_hub.db"));
    }

    #[test]
    fn parses_offline_profile_and_conflict_commands() {
        let export = Cli::try_parse_from(["skilldo", "profile", "export", "profile.json"])
            .expect("parse profile export");
        assert!(matches!(
            export.command,
            Commands::Profile {
                action: ProfileAction::Export { .. }
            }
        ));

        let resolve = Cli::try_parse_from([
            "skilldo",
            "profile",
            "resolve",
            "--strategy",
            "remote",
            "--json",
        ])
        .expect("parse profile resolve");
        assert!(resolve.json);
        assert!(matches!(
            resolve.command,
            Commands::Profile {
                action: ProfileAction::Resolve { strategy, .. }
            } if strategy == "remote"
        ));
    }

    #[test]
    fn parses_device_pipelines() {
        let pull = Cli::try_parse_from(["skilldo", "device", "pull", "--json"])
            .expect("parse device pull");
        assert!(matches!(
            pull.command,
            Commands::Device {
                action: DeviceAction::Pull { yes: false }
            }
        ));
        let publish = Cli::try_parse_from(["skilldo", "device", "publish", "--yes"])
            .expect("parse device publish");
        assert!(matches!(
            publish.command,
            Commands::Device {
                action: DeviceAction::Publish { yes: true }
            }
        ));
    }

    #[test]
    fn parses_source_repair_dry_run_and_apply() {
        let dry_run = Cli::try_parse_from(["skilldo", "repair", "sources", "--json"])
            .expect("parse source repair dry run");
        assert!(dry_run.json);
        assert!(matches!(
            dry_run.command,
            Commands::Repair {
                action: RepairAction::Sources { apply: false }
            }
        ));

        let apply = Cli::try_parse_from(["skilldo", "repair", "sources", "--apply", "--json"])
            .expect("parse source repair apply");
        assert!(matches!(
            apply.command,
            Commands::Repair {
                action: RepairAction::Sources { apply: true }
            }
        ));

        let one = Cli::try_parse_from([
            "skilldo",
            "repair",
            "source",
            "--skill",
            "demo",
            "--url",
            "https://github.com/example/repo.git",
            "--subpath",
            "skills/demo",
            "--apply",
            "--json",
        ])
        .expect("parse explicit source repair");
        assert!(matches!(
            one.command,
            Commands::Repair {
                action: RepairAction::Source { apply: true, .. }
            }
        ));
    }

    #[test]
    fn parses_project_scope_author_and_stdin_config_commands() {
        let sync = Cli::try_parse_from([
            "skilldo",
            "sync",
            "--skill",
            "demo",
            "--tool",
            "codex",
            "--scope",
            "project",
            "--project-path",
            "/tmp/project",
            "--json",
        ])
        .expect("parse project sync");
        assert!(matches!(
            sync.command,
            Commands::Sync { scope, project_path: Some(path), .. }
                if scope == "project" && path == "/tmp/project"
        ));

        let author = Cli::try_parse_from(["skilldo", "author", "detect", "--apply", "--json"])
            .expect("parse author detect");
        assert!(matches!(
            author.command,
            Commands::Author {
                action: AuthorAction::Detect { apply: true }
            }
        ));

        let secret = Cli::try_parse_from([
            "skilldo",
            "config",
            "set",
            "webdav.password",
            "--stdin",
            "--json",
        ])
        .expect("parse stdin config");
        assert!(matches!(
            secret.command,
            Commands::Config {
                action: ConfigAction::Set {
                    value: None,
                    stdin: true,
                    ..
                }
            }
        ));
    }

    #[test]
    fn structured_config_keys_remain_typed() {
        let mut cfg = AppConfig::default();
        config_set_value(&mut cfg, "origin_rules.my_git_owners", "[\"Example\"]").unwrap();
        config_set_value(&mut cfg, "current_author.github_login", "example").unwrap();
        assert_eq!(cfg.origin_rules.my_git_owners, vec!["Example"]);
        assert_eq!(cfg.current_author.github_login, "example");
    }
}
