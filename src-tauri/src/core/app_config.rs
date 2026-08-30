//! Unified application configuration model + (de)serialization.
//!
//! `AppConfig` is the single serializable representation of all user-facing
//! settings. The Tauri command layer (`commands`) is responsible for loading it
//! from / saving it to the various persistence sinks (SQLite settings table,
//! per-tool directory overrides, etc.). This module only owns the schema and
//! the pure JSON (de)serialization used for backup / restore.
//!
//! Backward compatibility: every field is `#[serde(default)]` so an older
//! backup missing a newer field still deserializes, and a newer backup is
//! rejected with a clear error via [`AppConfig::validate`].

use serde::{Deserialize, Serialize};

use crate::core::explore_sources::ExploreSourceConfig;

/// Bump when the schema changes in a backward-incompatible way.
pub const CONFIG_VERSION: u32 = 1;

/// Source-classification rules (which remotes count as "mine" vs "official").
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OriginRules {
    #[serde(default)]
    pub my_git_owners: Vec<String>,
    #[serde(default)]
    pub my_git_repos: Vec<String>,
    #[serde(default)]
    pub official_git_repos: Vec<String>,
}

/// A user-added local directory scanned for existing skills.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CustomScanDirEntry {
    pub name: String,
    pub path: String,
}

/// Per-tool skills directory override (empty / default when not overridden).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolDirOverride {
    pub tool_key: String,
    pub label: String,
    pub default_dir: String,
    pub current_dir: String,
    pub has_override: bool,
}

/// The aggregated, serializable application configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub config_version: u32,
    /// UI language (`en` / `zh`). Stored in the SQLite settings table so it is
    /// captured by backup/restore even though the live UI also uses localStorage.
    #[serde(default)]
    pub language: Option<String>,
    /// Central skills repository path override (resolved from `central_repo_path`).
    #[serde(default)]
    pub storage_path: Option<String>,
    #[serde(default)]
    pub git_cache_cleanup_days: i64,
    #[serde(default)]
    pub git_cache_ttl_secs: i64,
    #[serde(default)]
    pub github_token: String,
    #[serde(default)]
    pub origin_rules: OriginRules,
    #[serde(default)]
    pub tool_dir_overrides: Vec<ToolDirOverride>,
    #[serde(default)]
    pub custom_scan_dirs: Vec<CustomScanDirEntry>,
    #[serde(default)]
    pub explore_sources: Vec<ExploreSourceConfig>,
    /// Set only when exporting a backup; ignored on import.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exported_at: Option<String>,
}

impl AppConfig {
    /// Validate structural integrity and version compatibility of an imported
    /// backup. Returns an error with a human-readable Chinese message.
    pub fn validate(&self) -> anyhow::Result<()> {
        if self.config_version > CONFIG_VERSION {
            anyhow::bail!(
                "配置文件由更新版本 (v{}) 生成，当前应用仅支持 v{}；请先升级应用或选择兼容的备份。",
                self.config_version,
                CONFIG_VERSION
            );
        }
        for src in &self.explore_sources {
            if src.id.trim().is_empty() || src.name.trim().is_empty() {
                anyhow::bail!("技能源配置缺少 id 或 name，导入已中止");
            }
        }
        Ok(())
    }
}

/// Serialize a config to a pretty JSON string for backup export.
pub fn export_config_json(cfg: &AppConfig) -> String {
    serde_json::to_string_pretty(cfg).expect("AppConfig is always serializable")
}

/// Parse a config from a JSON backup string, validating schema / version.
pub fn parse_config_json(raw: &str) -> anyhow::Result<AppConfig> {
    let cfg: AppConfig =
        serde_json::from_str(raw).map_err(|err| anyhow::anyhow!("配置 JSON 解析失败: {err}"))?;
    cfg.validate()?;
    Ok(cfg)
}
