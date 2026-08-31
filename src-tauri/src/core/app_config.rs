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

use crate::core::cache_cleanup::{
    get_git_cache_cleanup_days, get_git_cache_ttl_secs, set_git_cache_cleanup_days,
    set_git_cache_ttl_secs,
};
use crate::core::expand_home_path;
use crate::core::explore_sources::{self, ExploreSourceConfig};
use crate::core::skill_store::SkillStore;
use crate::core::tool_adapters::{
    adapter_by_key, default_tool_adapters, is_tool_installed, resolve_default_path,
};

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

/// WebDAV connection profile used for remote backup / restore.
///
/// Persisted in its own settings key (`webdav_config`) and included in the
/// portable config export so a restored machine can reconnect without
/// re-entering credentials. (User accepted plaintext storage in the backup.)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WebDavConfig {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub password: String,
    /// Remote directory (path on the WebDAV server) where backups are stored.
    #[serde(default)]
    pub remote_dir: String,
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
    /// WebDAV connection profile for remote backup / restore.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webdav: Option<WebDavConfig>,
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

    /// Return a portable copy that does not expose authentication secrets.
    pub fn sanitized_for_export(&self) -> Self {
        let mut sanitized = self.clone();
        sanitized.github_token.clear();
        if let Some(webdav) = &mut sanitized.webdav {
            webdav.password.clear();
        }
        sanitized
    }

    /// Keep locally configured credentials when importing a sanitized backup.
    pub fn preserve_missing_secrets_from(&mut self, current: &Self) {
        if self.github_token.is_empty() {
            self.github_token = current.github_token.clone();
        }
        if let (Some(imported), Some(existing)) = (&mut self.webdav, &current.webdav) {
            if imported.password.is_empty()
                && imported.url == existing.url
                && imported.user == existing.user
            {
                imported.password = existing.password.clone();
            }
        }
    }
}

/// Serialize a config to a pretty JSON string for backup export.
pub fn export_config_json(cfg: &AppConfig) -> String {
    serde_json::to_string_pretty(&cfg.sanitized_for_export())
        .expect("AppConfig is always serializable")
}

/// Parse a config from a JSON backup string, validating schema / version.
pub fn parse_config_json(raw: &str) -> anyhow::Result<AppConfig> {
    let cfg: AppConfig =
        serde_json::from_str(raw).map_err(|err| anyhow::anyhow!("配置 JSON 解析失败: {err}"))?;
    cfg.validate()?;
    Ok(cfg)
}

// ===========================================================================
// Persistence constants + aggregation.
//
// These live in `core` (not the `commands` shell) so both the Tauri command
// layer and the CLI front-end can load / save the unified config from a single
// source of truth, with zero duplicated logic.
// ===========================================================================

/// Settings key prefix for per-tool skills-directory overrides.
pub const TOOL_DIR_OVERRIDE_PREFIX: &str = "tool_global_dir_override_";
/// Settings key holding the custom scan directories (JSON array).
pub const CUSTOM_SCAN_DIRS_KEY: &str = "custom_scan_dirs";
/// Settings key holding the origin rules (JSON).
pub const ORIGIN_RULES_KEY: &str = "origin_rules_v1";
/// Settings key holding the WebDAV profile (JSON).
pub const WEBDAV_CONFIG_KEY: &str = "webdav_config";

/// Normalize a single origin-rule item for stable comparison: trim scheme,
/// `www.` prefix and `.git` suffix, then lowercase.
pub fn normalize_rule_item(value: &str) -> String {
    value
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_end_matches(".git")
        .to_lowercase()
}

/// Deduplicate and sort origin rules for stable, comparable storage.
pub fn normalize_rules(mut rules: OriginRules) -> OriginRules {
    rules.my_git_owners = rules
        .my_git_owners
        .into_iter()
        .map(|item| normalize_rule_item(&item))
        .filter(|item| !item.is_empty())
        .collect();
    rules.my_git_repos = rules
        .my_git_repos
        .into_iter()
        .map(|item| normalize_rule_item(&item))
        .filter(|item| !item.is_empty())
        .collect();
    rules.official_git_repos = rules
        .official_git_repos
        .into_iter()
        .map(|item| normalize_rule_item(&item))
        .filter(|item| !item.is_empty())
        .collect();
    rules.my_git_owners.sort();
    rules.my_git_owners.dedup();
    rules.my_git_repos.sort();
    rules.my_git_repos.dedup();
    rules.official_git_repos.sort();
    rules.official_git_repos.dedup();
    rules
}

/// Read origin rules from the settings table (normalized).
pub fn get_origin_rules_impl(store: &SkillStore) -> anyhow::Result<OriginRules> {
    let raw = store.get_setting(ORIGIN_RULES_KEY)?;
    let rules = raw
        .and_then(|json| serde_json::from_str::<OriginRules>(&json).ok())
        .unwrap_or_default();
    Ok(normalize_rules(rules))
}

/// Resolve the effective skills directory for a tool (override or default).
pub fn resolve_tool_global_dir(adapter_key: &str, store: &SkillStore) -> anyhow::Result<String> {
    let override_key = format!("{}{}", TOOL_DIR_OVERRIDE_PREFIX, adapter_key);
    if let Some(override_path) = store.get_setting(&override_key)? {
        let expanded = expand_home_path(&override_path)?;
        return Ok(expanded.to_string_lossy().to_string());
    }
    let adapter = adapter_by_key(adapter_key)
        .ok_or_else(|| anyhow::anyhow!("unknown tool: {}", adapter_key))?;
    let path = resolve_default_path(&adapter)?;
    Ok(path.to_string_lossy().to_string())
}

/// Whether a per-tool skills-directory override is currently set.
pub fn has_tool_dir_override(adapter_key: &str, store: &SkillStore) -> anyhow::Result<bool> {
    let override_key = format!("{}{}", TOOL_DIR_OVERRIDE_PREFIX, adapter_key);
    Ok(store.get_setting(&override_key)?.is_some())
}

/// Collect per-tool skills-directory overrides as canonical config entries.
pub fn collect_tool_dir_overrides(store: &SkillStore) -> anyhow::Result<Vec<ToolDirOverride>> {
    let adapters = default_tool_adapters();
    let mut result = Vec::new();
    for adapter in &adapters {
        let key = adapter.id.as_key().to_string();
        let has_override = has_tool_dir_override(&key, store)?;
        if !is_tool_installed(adapter)? && !has_override {
            continue;
        }
        let default_dir = resolve_default_path(adapter)?.to_string_lossy().to_string();
        let current_dir = resolve_tool_global_dir(&key, store)?;
        result.push(ToolDirOverride {
            tool_key: key,
            label: adapter.display_name.to_string(),
            default_dir,
            current_dir,
            has_override,
        });
    }
    Ok(result)
}

/// Persist per-tool skills-directory overrides back to the settings table.
pub fn save_tool_dir_overrides(
    store: &SkillStore,
    overrides: &[ToolDirOverride],
) -> anyhow::Result<()> {
    for o in overrides {
        let override_key = format!("{}{}", TOOL_DIR_OVERRIDE_PREFIX, o.tool_key);
        if o.has_override {
            store.set_setting(&override_key, &o.current_dir)?;
        } else {
            store.delete_setting(&override_key)?;
        }
    }
    Ok(())
}

/// Convert between two structs with identical serde shapes via JSON round-trip.
pub fn convert_via_json<T, U>(value: &T) -> anyhow::Result<U>
where
    T: Serialize,
    U: serde::de::DeserializeOwned,
{
    let json = serde_json::to_value(value)?;
    Ok(serde_json::from_value(json)?)
}

/// Aggregate every user-facing setting into a single `AppConfig`.
pub fn load_app_config(store: &SkillStore) -> anyhow::Result<AppConfig> {
    let language = store.get_setting("language")?;
    let storage_path = store.get_setting("central_repo_path")?;
    let git_cache_cleanup_days = get_git_cache_cleanup_days(store);
    let git_cache_ttl_secs = get_git_cache_ttl_secs(store);
    let github_token = store.get_setting("github_token")?.unwrap_or_default();
    let origin_rules: OriginRules = convert_via_json(&get_origin_rules_impl(store)?)?;
    let tool_dir_overrides = collect_tool_dir_overrides(store)?;
    let custom_scan_dirs: Vec<CustomScanDirEntry> = match store.get_setting(CUSTOM_SCAN_DIRS_KEY)? {
        Some(json) => serde_json::from_str(&json).unwrap_or_default(),
        None => Vec::new(),
    };
    let explore_sources = explore_sources::get_explore_sources(store)?;
    let webdav: Option<WebDavConfig> = store
        .get_setting(WEBDAV_CONFIG_KEY)?
        .and_then(|json| serde_json::from_str::<WebDavConfig>(&json).ok());
    Ok(AppConfig {
        config_version: CONFIG_VERSION,
        language,
        storage_path,
        git_cache_cleanup_days,
        git_cache_ttl_secs,
        github_token,
        origin_rules,
        tool_dir_overrides,
        custom_scan_dirs,
        explore_sources,
        webdav,
        exported_at: None,
    })
}

/// Persist a full `AppConfig` back into the various sinks.
pub fn save_app_config_impl(store: &SkillStore, cfg: &AppConfig) -> anyhow::Result<()> {
    if let Some(language) = &cfg.language {
        store.set_setting("language", language)?;
    }
    if let Some(path) = &cfg.storage_path {
        store.set_setting("central_repo_path", path)?;
    }
    let cleanup_days = cfg.git_cache_cleanup_days.clamp(0, 3650);
    let ttl_secs = cfg.git_cache_ttl_secs.clamp(0, 3600);
    set_git_cache_cleanup_days(store, cleanup_days)?;
    set_git_cache_ttl_secs(store, ttl_secs)?;
    store.set_setting("github_token", cfg.github_token.trim())?;

    let rules: OriginRules = convert_via_json(&cfg.origin_rules)?;
    let normalized = normalize_rules(rules);
    store.set_setting(ORIGIN_RULES_KEY, &serde_json::to_string(&normalized)?)?;

    save_tool_dir_overrides(store, &cfg.tool_dir_overrides)?;

    store.set_setting(
        CUSTOM_SCAN_DIRS_KEY,
        &serde_json::to_string(&cfg.custom_scan_dirs)?,
    )?;

    explore_sources::save_explore_sources(store, &cfg.explore_sources)?;

    match &cfg.webdav {
        Some(wd) => store.set_setting(WEBDAV_CONFIG_KEY, &serde_json::to_string(wd)?)?,
        None => store.delete_setting(WEBDAV_CONFIG_KEY)?,
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_secrets() -> AppConfig {
        AppConfig {
            config_version: CONFIG_VERSION,
            github_token: "github-secret".to_string(),
            webdav: Some(WebDavConfig {
                url: "https://dav.example.test".to_string(),
                user: "user".to_string(),
                password: "webdav-secret".to_string(),
                remote_dir: "skilldo".to_string(),
            }),
            ..AppConfig::default()
        }
    }

    #[test]
    fn exported_config_redacts_credentials() {
        let exported = export_config_json(&config_with_secrets());
        assert!(!exported.contains("github-secret"));
        assert!(!exported.contains("webdav-secret"));
        let parsed: AppConfig = serde_json::from_str(&exported).unwrap();
        assert!(parsed.github_token.is_empty());
        assert_eq!(parsed.webdav.unwrap().password, "");
    }

    #[test]
    fn import_preserves_matching_local_credentials() {
        let current = config_with_secrets();
        let mut imported = current.sanitized_for_export();
        imported.preserve_missing_secrets_from(&current);
        assert_eq!(imported.github_token, "github-secret");
        assert_eq!(imported.webdav.unwrap().password, "webdav-secret");
    }

    #[test]
    fn import_does_not_reuse_webdav_password_for_another_server() {
        let current = config_with_secrets();
        let mut imported = current.sanitized_for_export();
        imported.webdav.as_mut().unwrap().url = "https://other.example.test".to_string();
        imported.preserve_missing_secrets_from(&current);
        assert_eq!(imported.webdav.unwrap().password, "");
    }
}
