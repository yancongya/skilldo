pub mod cache_cleanup;
pub mod cancel_token;
pub mod central_repo;
pub mod config;
pub mod content_hash;
pub mod explore_sources;
pub mod featured_skills;
pub mod git_fetcher;
pub mod github_download;
pub mod github_search;
pub mod installer;
pub mod onboarding;
pub mod skill_files;
pub mod skill_store;
pub mod skills_search;
pub mod sync_engine;
pub mod temp_cleanup;
pub mod tool_adapters;

use std::path::PathBuf;

use anyhow::{Context, Result};

/// Expand a path that may start with `~` or `~/` into an absolute path.
///
/// Shared by command handlers and the explore module so `~` expansion behaves
/// identically everywhere. Returns an error for empty input.
pub fn expand_home_path(input: &str) -> Result<PathBuf> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        anyhow::bail!("storage path is empty");
    }
    if trimmed == "~" {
        let home = dirs::home_dir().context("failed to resolve home directory")?;
        return Ok(home);
    }
    if let Some(stripped) = trimmed.strip_prefix("~/") {
        let home = dirs::home_dir().context("failed to resolve home directory")?;
        return Ok(home.join(stripped));
    }
    Ok(PathBuf::from(trimmed))
}
