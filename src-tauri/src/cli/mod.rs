//! Skills Hub command-line interface.
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
use crate::core::skill_store::{default_db_path_cli, SkillStore};
use crate::core::tool_adapters;

#[derive(Parser)]
#[command(
    name = "skillhub",
    version,
    about = "Skills Hub CLI - manage AI agent skills from the terminal (agent-native)",
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
    /// List skills currently managed by Skills Hub.
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
}

#[derive(Subcommand)]
enum SourcesAction {
    /// List configured explore sources.
    List,
}

/// Entry point invoked from the `skillhub` binary.
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
    let store = SkillStore::new(db_path);
    store
        .ensure_schema()
        .context("failed to ensure database schema")?;
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
