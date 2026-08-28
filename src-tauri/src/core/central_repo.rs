use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use dirs::home_dir;
use tauri::Manager;

use super::skill_store::SkillStore;

const CENTRAL_DIR_NAME: &str = ".skillshub";

pub fn resolve_central_repo_path<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    store: &SkillStore,
) -> Result<PathBuf> {
    if let Some(path) = store.get_setting("central_repo_path")? {
        return Ok(PathBuf::from(path));
    }

    if let Some(home) = home_dir() {
        return Ok(home.join(CENTRAL_DIR_NAME));
    }

    let base = app
        .path()
        .app_data_dir()
        .context("failed to resolve app data dir")?;
    Ok(base.join(CENTRAL_DIR_NAME))
}

/// CLI-compatible version: resolve the central repo path without a Tauri
/// `AppHandle`. Falls back to `~/` when no setting or home dir is available.
pub fn resolve_central_repo_path_cli(store: &SkillStore) -> Result<PathBuf> {
    if let Some(path) = store.get_setting("central_repo_path")? {
        return Ok(PathBuf::from(path));
    }

    if let Some(home) = home_dir() {
        return Ok(home.join(CENTRAL_DIR_NAME));
    }

    anyhow::bail!("failed to resolve central repo path: no setting and home dir unavailable");
}

pub fn ensure_central_repo(path: &Path) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("create {:?}", path))?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/central_repo.rs"]
mod tests;
