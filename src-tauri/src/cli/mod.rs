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

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use serde::Serialize;

use crate::core::app_config::{
    export_config_json, load_app_config, parse_config_json, save_app_config_impl, AppConfig,
    WebDavConfig,
};
use crate::core::backup::{export_full_backup, restore_full_backup, RestoreReport};
use crate::core::explore_sources::{self, ExploreSourceConfig};
use crate::core::github_auth::compute_github_token_status;
use crate::core::installer;
use crate::core::skill_store::{default_db_path_cli, SkillStore};
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
    },
    /// Remove a skill from a specific AI tool (unsync).
    Unsync {
        /// Skill ID or name.
        #[arg(long)]
        skill: String,
        /// Target tool key.
        #[arg(long)]
        tool: String,
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
        value: String,
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

/// Entry point invoked from the `skilldo` binary.
pub fn run() {
    if let Err(err) = execute() {
        eprintln!("error: {:#}", err);
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

fn execute() -> Result<()> {
    let cli = Cli::parse();
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
            ConfigAction::Set { key, value } => cmd_config_set(&store, &key, &value, cli.json),
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
        Commands::Backup { target } => match target {
            BackupTarget::File { path } => cmd_backup_file(&store, path.as_deref(), cli.json),
            BackupTarget::Webdav => cmd_backup_webdav(&store, cli.json),
        },
        Commands::Restore { target } => match target {
            RestoreTarget::File { path } => cmd_restore_file(&store, &path, cli.json),
            RestoreTarget::Webdav => cmd_restore_webdav(&store, cli.json),
        },
        Commands::Install { url, name, yes } => cmd_install(&store, &url, name, yes, cli.json),
        Commands::Sync { skill, tool } => cmd_sync(&store, &skill, &tool, cli.json),
        Commands::Unsync { skill, tool } => cmd_unsync(&store, &skill, &tool, cli.json),
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
    syncable: bool,
    central_path: String,
    status: String,
    created_at: i64,
    updated_at: i64,
    targets: Vec<CliSkillTarget>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CliSkillTarget {
    tool: String,
    scope: String,
    target_path: String,
    mode: String,
    status: String,
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
    target_path: String,
    mode: String,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_list(store: &SkillStore, filter: &str, json: bool) -> Result<()> {
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
        let targets = store
            .list_skill_targets(&rec.id)
            .unwrap_or_default()
            .into_iter()
            .map(|t| CliSkillTarget {
                tool: t.tool,
                scope: t.scope,
                target_path: t.target_path,
                mode: t.mode,
                status: t.status,
            })
            .collect();
        skills.push(CliManagedSkill {
            id: rec.id,
            name: rec.name,
            description: rec.description,
            source_type: rec.source_type,
            source_ref: rec.source_ref,
            syncable,
            central_path: rec.central_path,
            status: rec.status,
            created_at: rec.created_at,
            updated_at: rec.updated_at,
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

fn config_get_value(cfg: &AppConfig, key: &str) -> Option<String> {
    match key {
        "language" => cfg.language.clone(),
        "storage_path" => cfg.storage_path.clone(),
        "github_token" => Some(cfg.github_token.clone()),
        "git_cache_cleanup_days" => Some(cfg.git_cache_cleanup_days.to_string()),
        "git_cache_ttl_secs" => Some(cfg.git_cache_ttl_secs.to_string()),
        "webdav.url" => cfg.webdav.as_ref().map(|w| w.url.clone()),
        "webdav.user" => cfg.webdav.as_ref().map(|w| w.user.clone()),
        "webdav.password" => cfg.webdav.as_ref().map(|w| w.password.clone()),
        "webdav.remote_dir" => cfg.webdav.as_ref().map(|w| w.remote_dir.clone()),
        _ => None,
    }
}

fn config_set_value(cfg: &mut AppConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "language" => cfg.language = Some(value.to_string()),
        "storage_path" => cfg.storage_path = Some(value.to_string()),
        "github_token" => cfg.github_token = value.to_string(),
        "git_cache_cleanup_days" => {
            cfg.git_cache_cleanup_days = value
                .parse::<i64>()
                .with_context(|| format!("无效的整数: {value}"))?
        }
        "git_cache_ttl_secs" => {
            cfg.git_cache_ttl_secs = value
                .parse::<i64>()
                .with_context(|| format!("无效的整数: {value}"))?
        }
        "webdav.url" => {
            cfg.webdav.get_or_insert_with(WebDavConfig::default).url = value.to_string()
        }
        "webdav.user" => {
            cfg.webdav.get_or_insert_with(WebDavConfig::default).user = value.to_string()
        }
        "webdav.password" => {
            cfg.webdav.get_or_insert_with(WebDavConfig::default).password = value.to_string()
        }
        "webdav.remote_dir" => {
            cfg.webdav.get_or_insert_with(WebDavConfig::default).remote_dir = value.to_string()
        }
        other => anyhow::bail!(
            "未知配置键: {other}（支持 language/storage_path/github_token/git_cache_cleanup_days/git_cache_ttl_secs/webdav.*）"
        ),
    }
    Ok(())
}

fn cmd_config_get(store: &SkillStore, key: Option<&str>, _json: bool) -> Result<()> {
    let cfg = load_app_config(store)?;
    match key {
        None => println!("{}", export_config_json(&cfg)),
        Some(k) => {
            let v = config_get_value(&cfg, k).unwrap_or_default();
            if v.is_empty() {
                anyhow::bail!("未知配置键: {k}");
            }
            println!("{v}");
        }
    }
    Ok(())
}

fn cmd_config_set(store: &SkillStore, key: &str, value: &str, _json: bool) -> Result<()> {
    let mut cfg = load_app_config(store)?;
    config_set_value(&mut cfg, key, value)?;
    save_app_config_impl(store, &cfg)?;
    println!("已更新配置: {key} = {value}");
    Ok(())
}

fn cmd_config_export(store: &SkillStore, path: Option<&str>, _json: bool) -> Result<()> {
    let cfg = load_app_config(store)?;
    let json = export_config_json(&cfg);
    match path {
        Some(p) => {
            std::fs::write(p, &json).with_context(|| format!("写入文件失败: {p}"))?;
            println!("配置已导出到 {p}");
        }
        None => println!("{json}"),
    }
    Ok(())
}

fn cmd_config_import(store: &SkillStore, path: &str, _json: bool) -> Result<()> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("读取文件失败: {path}"))?;
    let cfg = parse_config_json(&raw)?;
    save_app_config_impl(store, &cfg)?;
    println!("配置已从 {path} 导入");
    Ok(())
}

// ===========================================================================
// Github token subcommands
// ===========================================================================

fn cmd_github_token_set(store: &SkillStore, token: &str, _json: bool) -> Result<()> {
    let mut cfg = load_app_config(store)?;
    cfg.github_token = token.to_string();
    save_app_config_impl(store, &cfg)?;
    println!("GitHub token 已保存");
    Ok(())
}

fn cmd_github_token_get(store: &SkillStore, _json: bool) -> Result<()> {
    let cfg = load_app_config(store)?;
    if cfg.github_token.is_empty() {
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

fn cmd_backup_file(store: &SkillStore, path: Option<&str>, _json: bool) -> Result<()> {
    let json = export_full_backup(store)?;
    match path {
        Some(p) => {
            std::fs::write(p, &json).with_context(|| format!("写入文件失败: {p}"))?;
            println!("已备份到 {p}");
        }
        None => println!("{json}"),
    }
    Ok(())
}

fn cmd_backup_webdav(store: &SkillStore, _json: bool) -> Result<()> {
    let wd = load_webdav_cfg(store)?;
    let body = export_full_backup(store)?;
    let remote = upload_backup(&wd, &body)?;
    println!("已备份到 WebDAV: {remote}");
    Ok(())
}

fn cmd_restore_file(store: &SkillStore, path: &str, _json: bool) -> Result<()> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("读取文件失败: {path}"))?;
    let report = restore_full_backup(store, &raw)?;
    print_restore_report(&report);
    Ok(())
}

fn cmd_restore_webdav(store: &SkillStore, _json: bool) -> Result<()> {
    let wd = load_webdav_cfg(store)?;
    let raw = download_backup(&wd)?;
    let report = restore_full_backup(store, &raw)?;
    print_restore_report(&report);
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
        let out = CliInstallResult {
            success: true,
            skill_id: result.skill_id,
            name: result.name,
            central_path: result.central_path.to_string_lossy().to_string(),
            source_type: "local".to_string(),
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

fn cmd_sync(store: &SkillStore, skill_name: &str, tool_key: &str, json: bool) -> Result<()> {
    let skill_id = resolve_skill_id(store, skill_name)?;
    let outcome = installer::sync_skill_to_tool_cli(store, &skill_id, tool_key)?;
    let out = CliSyncResult {
        success: true,
        skill_id,
        tool: tool_key.to_string(),
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

fn cmd_unsync(store: &SkillStore, skill_name: &str, tool_key: &str, json: bool) -> Result<()> {
    let skill_id = resolve_skill_id(store, skill_name)?;
    installer::unsync_skill_cli(store, &skill_id, tool_key)?;
    if json {
        let out = serde_json::json!({
            "success": true,
            "skillId": skill_id,
            "tool": tool_key,
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
}
