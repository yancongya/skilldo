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

use crate::core::explore_sources::{self, ExploreSourceConfig};
use crate::core::installer;
use crate::core::skill_store::{default_db_path_cli, SkillStore};
use crate::core::tool_adapters;

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
    List,
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
}

#[derive(Subcommand)]
enum SourcesAction {
    /// List configured explore sources.
    List,
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
        Commands::List => cmd_list(&store, cli.json),
        Commands::Status => cmd_status(cli.json),
        Commands::Explore { query } => cmd_explore(&store, query, cli.json),
        Commands::Sources { action } => match action {
            SourcesAction::List => cmd_sources_list(&store, cli.json),
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

fn cmd_list(store: &SkillStore, json: bool) -> Result<()> {
    let records = store
        .list_skills()
        .context("failed to list managed skills")?;
    let mut skills = Vec::with_capacity(records.len());
    for rec in records {
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
            println!(
                "{}  [{}]  {}  -> {} target(s)",
                s.name,
                s.source_type,
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
