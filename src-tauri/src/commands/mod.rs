use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::State;

use crate::core::app_config::{export_config_json, parse_config_json, AppConfig, WebDavConfig};
use crate::core::backup::{export_full_backup, restore_full_backup, RestoreReport};
use crate::core::webdav::{download_backup, upload_backup};
// The aggregation logic now lives in `core::app_config`; re-export so the rest
// of this module (and its command handlers) keeps resolving the same names.
pub use crate::core::app_config::{
    get_origin_rules_impl, has_tool_dir_override, load_app_config, normalize_rules,
    resolve_tool_global_dir, save_app_config_impl, CUSTOM_SCAN_DIRS_KEY, ORIGIN_RULES_KEY,
    TOOL_DIR_OVERRIDE_PREFIX,
};
use crate::core::cache_cleanup::{
    cleanup_git_cache_dirs, get_git_cache_cleanup_days as get_git_cache_cleanup_days_core,
    get_git_cache_ttl_secs as get_git_cache_ttl_secs_core,
    set_git_cache_cleanup_days as set_git_cache_cleanup_days_core,
    set_git_cache_ttl_secs as set_git_cache_ttl_secs_core,
};
use crate::core::cancel_token::CancelToken;
use crate::core::central_repo::{ensure_central_repo, resolve_central_repo_path};
use crate::core::content_hash::hash_dir;
use crate::core::expand_home_path;
use crate::core::explore_sources::{
    get_explore_skills as get_explore_skills_core, get_explore_sources as get_explore_sources_core,
    save_explore_sources as save_explore_sources_core, ExploreSkill, ExploreSourceConfig,
};
use crate::core::featured_skills::{fetch_featured_skills, FeaturedSkill};
use crate::core::github_search::{search_github_repos, RepoSummary};
use crate::core::installer::{
    check_all_managed_skill_updates, check_managed_skill_update, install_git_skill,
    install_git_skill_from_selection, install_local_skill, install_local_skill_from_selection,
    install_package_skill, list_git_skills, list_local_skills, publish_managed_skill_to_remote,
    update_managed_skill_from_source, GitSkillCandidate, InstallResult, LocalSkillCandidate,
};
use crate::core::onboarding::{build_onboarding_plan, OnboardingPlan};
use crate::core::skill_store::{SkillOriginRecord, SkillStore, SkillTargetRecord};
use crate::core::skills_search::{
    search_skills_online as search_skills_online_core, OnlineSkillResult,
};
use crate::core::sync_engine::{
    copy_dir_recursive, sync_dir_for_tool_with_overwrite, sync_dir_hybrid, SyncMode,
};
use crate::core::tool_adapters::{
    adapter_by_key, adapters_sharing_project_skills_dir, is_tool_installed, resolve_default_path,
    resolve_project_path, supports_project_scope,
};

const FEATURED_SKILLS_CACHE_KEY: &str = "featured_skills_cache";
const BUNDLED_FEATURED_SKILLS_JSON: &str = include_str!("../../../featured-skills.json");
const KNOWN_OFFICIAL_SKILL_SOURCES: &[(&str, &str)] = &[
    (
        "agents-sdk",
        "https://github.com/cloudflare/skills/tree/main/skills/agents-sdk",
    ),
    (
        "cloudflare",
        "https://github.com/cloudflare/skills/tree/main/skills/cloudflare",
    ),
    (
        "cloudflare-email-service",
        "https://github.com/cloudflare/skills/tree/main/skills/cloudflare-email-service",
    ),
    (
        "cloudflare-one",
        "https://github.com/cloudflare/skills/tree/main/skills/cloudflare-one",
    ),
    (
        "cloudflare-one-migrations",
        "https://github.com/cloudflare/skills/tree/main/skills/cloudflare-one-migrations",
    ),
    (
        "durable-objects",
        "https://github.com/cloudflare/skills/tree/main/skills/durable-objects",
    ),
    (
        "find-skills",
        "https://github.com/vercel-labs/skills/tree/main/skills/find-skills",
    ),
    (
        "sandbox-sdk",
        "https://github.com/cloudflare/skills/tree/main/skills/sandbox-sdk",
    ),
    (
        "turnstile-spin",
        "https://github.com/cloudflare/skills/tree/main/skills/turnstile-spin",
    ),
    (
        "web-perf",
        "https://github.com/cloudflare/skills/tree/main/skills/web-perf",
    ),
    (
        "workers-best-practices",
        "https://github.com/cloudflare/skills/tree/main/skills/workers-best-practices",
    ),
    (
        "wrangler",
        "https://github.com/cloudflare/skills/tree/main/skills/wrangler",
    ),
];
const OFFICIAL_SOURCE_PATTERNS: &[&str] = &[
    "github.com/anthropics/skills",
    "github.com/openai/skills",
    "github.com/cloudflare/skills",
    "github.com/vercel-labs/skills",
    "github.com/openai/codex",
    "github.com/openai/openai-cookbook",
    ".codex/vendor_imports/skills",
    ".codex/plugins/cache/openai-bundled",
    ".codex/plugins/cache/openai-primary-runtime",
    ".codex/skills/.system",
];

// `CustomScanDirEntry` and `OriginRules` are owned by `core::app_config`
// (single source of truth); re-export so the rest of this module keeps using
// the same names without a second, divergent definition.
pub use crate::core::app_config::{CustomScanDirEntry, OriginRules};

#[derive(Debug, Deserialize)]
struct FeaturedSkillsOriginData {
    skills: Vec<FeaturedSkillOriginEntry>,
}

#[derive(Debug, Deserialize)]
struct FeaturedSkillOriginEntry {
    slug: String,
    name: String,
    #[serde(default)]
    source_url: String,
}

fn is_tool_available(
    adapter: &crate::core::tool_adapters::ToolAdapter,
    adapter_key: &str,
    store: &SkillStore,
) -> anyhow::Result<bool> {
    if is_tool_installed(adapter)? {
        return Ok(true);
    }

    if !has_tool_dir_override(adapter_key, store)? {
        return Ok(false);
    }

    let skills_dir = resolve_tool_global_dir(adapter_key, store)?;
    Ok(Path::new(&skills_dir).exists())
}

fn custom_tool_dir(tool_key: &str) -> Option<PathBuf> {
    tool_key
        .strip_prefix("custom:")
        .filter(|path| !path.trim().is_empty())
        .map(PathBuf::from)
}

fn normalize_source_ref(source: &str) -> String {
    let normalized = source
        .trim()
        .to_lowercase()
        .trim_start_matches("git+")
        .trim_start_matches("ssh://")
        .trim_end_matches(".git")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .replace('\\', "/");
    normalized.replace("git@github.com:", "github.com/")
}

fn is_official_source(source: &str) -> bool {
    let normalized = normalize_source_ref(source);
    OFFICIAL_SOURCE_PATTERNS
        .iter()
        .any(|pattern| normalized.contains(&pattern.to_lowercase()))
}

fn matches_repo_rule(source: &str, rules: &[String]) -> bool {
    let normalized = normalize_source_ref(source);
    rules.iter().any(|rule| normalized.contains(rule))
}

/// Check whether an owner or owner/repo pair is listed in the user's `my_git_owners` or `my_git_repos`.
fn matches_my_git_rules(owner: Option<&str>, repo: Option<&str>, rules: &OriginRules) -> bool {
    if let Some(o) = owner {
        let o_lower = o.to_lowercase();
        if rules
            .my_git_owners
            .iter()
            .any(|r| r.to_lowercase() == o_lower)
        {
            return true;
        }
    }
    if let (Some(o), Some(r)) = (owner, repo) {
        let key = format!("{}/{}", o, r).to_lowercase();
        if rules
            .my_git_repos
            .iter()
            .any(|rule| rule.to_lowercase() == key)
        {
            return true;
        }
    }
    false
}

fn normalize_skill_key(value: &str) -> String {
    value.trim().to_lowercase()
}

fn parse_featured_origin_map(json: &str) -> HashMap<String, String> {
    let Ok(data) = serde_json::from_str::<FeaturedSkillsOriginData>(json) else {
        return HashMap::new();
    };

    let mut out = HashMap::new();
    for skill in data.skills {
        if skill.source_url.trim().is_empty() {
            continue;
        }
        out.insert(normalize_skill_key(&skill.name), skill.source_url.clone());
        out.insert(normalize_skill_key(&skill.slug), skill.source_url);
    }
    out
}

fn featured_origin_map(store: &SkillStore) -> HashMap<String, String> {
    let mut known = KNOWN_OFFICIAL_SKILL_SOURCES
        .iter()
        .map(|(name, source)| (normalize_skill_key(name), (*source).to_string()))
        .collect::<HashMap<_, _>>();
    if let Ok(Some(cached)) = store.get_setting(FEATURED_SKILLS_CACHE_KEY) {
        let cached_map = parse_featured_origin_map(&cached);
        if !cached_map.is_empty() {
            known.extend(cached_map);
            return known;
        }
    }
    known.extend(parse_featured_origin_map(BUNDLED_FEATURED_SKILLS_JSON));
    known
}

fn find_git_root(path: &Path) -> Option<PathBuf> {
    let start = if path.is_dir() {
        path.to_path_buf()
    } else {
        path.parent()?.to_path_buf()
    };

    for ancestor in start.ancestors() {
        let dot_git = ancestor.join(".git");
        if dot_git.exists() {
            return Some(ancestor.to_path_buf());
        }
    }
    None
}

fn git_remote_origin(git_root: &Path) -> Option<String> {
    let dot_git = git_root.join(".git");
    let config_path = if dot_git.is_dir() {
        dot_git.join("config")
    } else {
        let content = std::fs::read_to_string(&dot_git).ok()?;
        let gitdir = content.trim().strip_prefix("gitdir:")?.trim();
        let path = PathBuf::from(gitdir);
        let resolved = if path.is_absolute() {
            path
        } else {
            git_root.join(path)
        };
        resolved.join("config")
    };
    let config = std::fs::read_to_string(config_path).ok()?;
    let mut in_origin = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("[remote ") {
            in_origin = trimmed.contains("\"origin\"");
            continue;
        }
        if in_origin && trimmed.starts_with("url") {
            return trimmed
                .split_once('=')
                .map(|(_, value)| value.trim().to_string());
        }
    }
    None
}

#[derive(Clone, Debug)]
struct InferredOrigin {
    origin_kind: String,
    origin_role: String,
    provider: Option<String>,
    remote_url: Option<String>,
    owner: Option<String>,
    repo: Option<String>,
    update_strategy: String,
    publish_strategy: String,
    reason: String,
}

fn parse_github_owner_repo(source: &str) -> (Option<String>, Option<String>) {
    let normalized = normalize_source_ref(source);
    let Some(rest) = normalized.split_once("github.com/").map(|(_, rest)| rest) else {
        return (None, None);
    };
    let mut parts = rest.split('/');
    let owner = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let repo = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(|value| value.trim_end_matches(".git").to_string());
    (owner, repo)
}

fn infer_source_origin(
    source_type: &str,
    source_ref: Option<&str>,
    central_path: &str,
    rules: &OriginRules,
) -> InferredOrigin {
    if let Some(source) = source_ref {
        if is_official_source(source) || matches_repo_rule(source, &rules.official_git_repos) {
            let (owner, repo) = parse_github_owner_repo(source);
            return InferredOrigin {
                origin_kind: "official".to_string(),
                origin_role: "official".to_string(),
                provider: Some("official".to_string()),
                remote_url: Some(source.to_string()),
                owner,
                repo,
                update_strategy: "git_pull".to_string(),
                publish_strategy: "none".to_string(),
                reason: "source_ref matched official source rule".to_string(),
            };
        }
    }
    if is_official_source(central_path) {
        return InferredOrigin {
            origin_kind: "official".to_string(),
            origin_role: "official".to_string(),
            provider: Some("official".to_string()),
            remote_url: None,
            owner: None,
            repo: None,
            update_strategy: "local_copy".to_string(),
            publish_strategy: "none".to_string(),
            reason: "central_path matched official source rule".to_string(),
        };
    }

    if source_type.to_lowercase().contains("git") {
        let remote = source_ref.map(str::to_string);
        let (owner, repo) = source_ref
            .map(parse_github_owner_repo)
            .unwrap_or((None, None));
        // Check if the owner/repo matches the user's "my git" rules.
        let is_mine = matches_my_git_rules(owner.as_deref(), repo.as_deref(), rules);
        let (origin_role, publish_strategy, reason) = if is_mine {
            (
                "mine".to_string(),
                "git_push".to_string(),
                "source_type is git and matches my_git_owners/my_git_repos".to_string(),
            )
        } else {
            (
                "repository".to_string(),
                "none".to_string(),
                "source_type is git".to_string(),
            )
        };
        return InferredOrigin {
            origin_kind: "git".to_string(),
            origin_role,
            provider: Some("git".to_string()),
            remote_url: remote,
            owner,
            repo,
            update_strategy: "git_pull".to_string(),
            publish_strategy,
            reason,
        };
    }

    if source_type.to_lowercase().contains("package") {
        return InferredOrigin {
            origin_kind: "package".to_string(),
            origin_role: "repository".to_string(),
            provider: Some("npm".to_string()),
            remote_url: source_ref.map(str::to_string),
            owner: None,
            repo: None,
            update_strategy: "package_refresh".to_string(),
            publish_strategy: "none".to_string(),
            reason: "source_type is package".to_string(),
        };
    }

    for path in [source_ref, Some(central_path)].into_iter().flatten() {
        let path = PathBuf::from(path);
        if let Some(git_root) = find_git_root(&path) {
            if let Some(remote) = git_remote_origin(&git_root) {
                if is_official_source(&remote)
                    || matches_repo_rule(&remote, &rules.official_git_repos)
                {
                    let (owner, repo) = parse_github_owner_repo(&remote);
                    return InferredOrigin {
                        origin_kind: "official".to_string(),
                        origin_role: "official".to_string(),
                        provider: Some("official".to_string()),
                        remote_url: Some(remote),
                        owner,
                        repo,
                        update_strategy: "git_pull".to_string(),
                        publish_strategy: "none".to_string(),
                        reason: "git remote matched official source rule".to_string(),
                    };
                }
                let (owner, repo) = parse_github_owner_repo(&remote);
                let is_mine = matches_my_git_rules(owner.as_deref(), repo.as_deref(), rules);
                let (origin_role, publish_strategy, reason) = if is_mine {
                    (
                        "mine".to_string(),
                        "git_push".to_string(),
                        "local git remote matches my_git_owners/my_git_repos".to_string(),
                    )
                } else {
                    (
                        "repository".to_string(),
                        "none".to_string(),
                        "local source path is inside a git repository".to_string(),
                    )
                };
                return InferredOrigin {
                    origin_kind: "git".to_string(),
                    origin_role,
                    provider: Some("git".to_string()),
                    remote_url: Some(remote),
                    owner,
                    repo,
                    update_strategy: "git_pull".to_string(),
                    publish_strategy,
                    reason,
                };
            }
            return InferredOrigin {
                origin_kind: "git".to_string(),
                origin_role: "repository".to_string(),
                provider: Some("git".to_string()),
                remote_url: None,
                owner: None,
                repo: None,
                update_strategy: "git_pull".to_string(),
                publish_strategy: "none".to_string(),
                reason: "local source path is inside a git repository without origin remote"
                    .to_string(),
            };
        }
    }

    InferredOrigin {
        origin_kind: "local".to_string(),
        origin_role: "mine".to_string(),
        provider: Some("local".to_string()),
        remote_url: None,
        owner: None,
        repo: None,
        update_strategy: "local_copy".to_string(),
        publish_strategy: "none".to_string(),
        reason: "no git or official source metadata found".to_string(),
    }
}
use uuid::Uuid;

const RECENT_PROJECTS_SETTING: &str = "recent_projects_v1";

fn format_anyhow_error(err: anyhow::Error) -> String {
    let first = err.to_string();
    // Frontend relies on these prefixes for special flows.
    if first.starts_with("MULTI_SKILLS|")
        || first.starts_with("TARGET_EXISTS|")
        || first.starts_with("TOOL_NOT_INSTALLED|")
    {
        return first;
    }

    // Include the full error chain (causes), not just the top context.
    let mut full = format!("{:#}", err);

    // Redact noisy temp paths from clone context (we care about the cause, not the dest).
    // Example: `clone https://... into "/Users/.../skilldo-git-<uuid>"`
    if let Some(head) = full.lines().next() {
        if head.starts_with("clone ") {
            if let Some(pos) = head.find(" into ") {
                let head_redacted = format!("{} (已省略临时目录)", &head[..pos]);
                let rest: String = full.lines().skip(1).collect::<Vec<_>>().join("\n");
                full = if rest.is_empty() {
                    head_redacted
                } else {
                    format!("{}\n{}", head_redacted, rest)
                };
            }
        }
    }

    let root = err.root_cause().to_string();
    let lower = full.to_lowercase();

    // Heuristic-friendly messaging for GitHub clone failures.
    if lower.contains("github.com")
        && (lower.contains("clone ") || lower.contains("remote") || lower.contains("fetch"))
    {
        if lower.contains("securetransport") {
            return format!(
        "无法从 GitHub 拉取仓库：TLS/证书校验失败（macOS SecureTransport）。\n\n建议：\n- 检查网络/代理是否拦截 HTTPS\n- 如在公司网络，可能需要安装公司根证书或使用可信代理\n- 也可在终端确认 `git clone {}` 是否可用\n\n详细：{}",
        "https://github.com/<owner>/<repo>",
        root
      );
        }
        let hint = if lower.contains("authentication")
            || lower.contains("permission denied")
            || lower.contains("credentials")
        {
            "无法访问该仓库：可能是私有仓库/权限不足/需要鉴权。"
        } else if lower.contains("not found") {
            "仓库不存在或无权限访问（GitHub 返回 not found）。"
        } else if lower.contains("failed to resolve")
            || lower.contains("could not resolve")
            || lower.contains("dns")
        {
            "无法解析 GitHub 域名（DNS）。请检查网络/代理。"
        } else if lower.contains("timed out") || lower.contains("timeout") {
            "连接 GitHub 超时。请检查网络/代理。"
        } else if lower.contains("connection refused") || lower.contains("connection reset") {
            "连接 GitHub 失败（连接被拒绝/重置）。请检查网络/代理。"
        } else {
            "无法从 GitHub 拉取仓库。请检查网络/代理，或稍后重试。"
        };

        return format!("{}\n\n详细：{}", hint, root);
    }

    full
}

#[derive(Debug, Serialize)]
pub struct ToolInfoDto {
    pub key: String,
    pub label: String,
    pub installed: bool,
    pub skills_dir: String,
    pub supports_project_scope: bool,
}

#[derive(Debug, Serialize)]
pub struct ToolStatusDto {
    pub tools: Vec<ToolInfoDto>,
    pub installed: Vec<String>,
    pub newly_installed: Vec<String>,
}

#[tauri::command]
pub async fn get_tool_status(store: State<'_, SkillStore>) -> Result<ToolStatusDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapters = crate::core::tool_adapters::default_tool_adapters();
        let mut tools: Vec<ToolInfoDto> = Vec::new();
        let mut installed: Vec<String> = Vec::new();

        for adapter in &adapters {
            let key = adapter.id.as_key().to_string();
            let ok = is_tool_available(adapter, &key, &store)?;
            let skills_dir = resolve_tool_global_dir(&key, &store)?;
            tools.push(ToolInfoDto {
                key: key.clone(),
                label: adapter.display_name.to_string(),
                installed: ok,
                skills_dir,
                supports_project_scope: supports_project_scope(adapter),
            });
            if ok {
                installed.push(key);
            }
        }

        installed.dedup();

        let prev: Vec<String> = store
            .get_setting("installed_tools_v1")?
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .unwrap_or_default();

        let prev_set: std::collections::HashSet<String> = prev.into_iter().collect();
        let newly_installed: Vec<String> = installed
            .iter()
            .filter(|k| !prev_set.contains(*k))
            .cloned()
            .collect();

        // Persist current set (best effort).
        let _ = store.set_setting(
            "installed_tools_v1",
            &serde_json::to_string(&installed).unwrap_or_else(|_| "[]".to_string()),
        );

        Ok::<_, anyhow::Error>(ToolStatusDto {
            tools,
            installed,
            newly_installed,
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[derive(Debug, Serialize)]
pub struct ToolDirOverrideDto {
    pub tool_key: String,
    pub label: String,
    pub default_dir: String,
    pub current_dir: String,
    pub has_override: bool,
}

#[tauri::command]
pub async fn get_tool_skills_dir_overrides(
    store: State<'_, SkillStore>,
) -> Result<Vec<ToolDirOverrideDto>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let adapters = crate::core::tool_adapters::default_tool_adapters();
        let mut result = Vec::new();
        for adapter in &adapters {
            let key = adapter.id.as_key().to_string();
            let has_override = has_tool_dir_override(&key, &store)?;
            if !is_tool_installed(adapter)? && !has_override {
                continue;
            }
            let default_dir = resolve_default_path(adapter)?.to_string_lossy().to_string();
            let current_dir = resolve_tool_global_dir(&key, &store)?;
            result.push(ToolDirOverrideDto {
                tool_key: key,
                label: adapter.display_name.to_string(),
                default_dir,
                current_dir,
                has_override,
            });
        }
        Ok::<_, anyhow::Error>(result)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn set_tool_skills_dir_override(
    store: State<'_, SkillStore>,
    tool_key: String,
    path: String,
) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let expanded = expand_home_path(&path)?;
        if !expanded.is_absolute() {
            anyhow::bail!("path must be absolute");
        }
        let override_key = format!("{}{}", TOOL_DIR_OVERRIDE_PREFIX, tool_key);
        store.set_setting(&override_key, &path)?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn reset_tool_skills_dir_override(
    store: State<'_, SkillStore>,
    tool_key: String,
) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let key = format!("{}{}", TOOL_DIR_OVERRIDE_PREFIX, tool_key);
        store.delete_setting(&key)?;
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn get_custom_scan_dirs(
    store: State<'_, SkillStore>,
) -> Result<Vec<CustomScanDirEntry>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let raw = store.get_setting(CUSTOM_SCAN_DIRS_KEY)?;
        let dirs: Vec<CustomScanDirEntry> = match raw {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Vec::new(),
        };
        Ok::<_, anyhow::Error>(dirs)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn browse_directory_show_hidden(
    app_handle: tauri::AppHandle,
) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    app_handle
        .run_on_main_thread(move || {
            let result = browse_directory_hidden_impl();
            let _ = tx.send(result);
        })
        .map_err(|e| format!("failed to dispatch to main thread: {e}"))?;
    rx.recv()
        .map_err(|_| "main thread task cancelled".to_string())?
}

#[cfg(target_os = "macos")]
fn browse_directory_hidden_impl() -> Result<Option<String>, String> {
    use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};

    let mtm = objc2::MainThreadMarker::new().ok_or_else(|| "not on main thread".to_string())?;
    let panel = NSOpenPanel::openPanel(mtm);
    panel.setCanChooseDirectories(true);
    panel.setCanChooseFiles(false);
    panel.setAllowsMultipleSelection(false);
    panel.setShowsHiddenFiles(true);
    let result = panel.runModal();
    if result == NSModalResponseOK {
        let path = panel
            .URL()
            .and_then(|url| url.path())
            .map(|s| s.to_string())
            .ok_or_else(|| "could not read selected path".to_string())?;
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

#[cfg(not(target_os = "macos"))]
fn browse_directory_hidden_impl() -> Result<Option<String>, String> {
    Ok(None)
}

#[tauri::command]
pub async fn add_custom_scan_dir(
    store: State<'_, SkillStore>,
    name: String,
    path: String,
) -> Result<Vec<CustomScanDirEntry>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let raw = store.get_setting(CUSTOM_SCAN_DIRS_KEY)?;
        let mut dirs: Vec<CustomScanDirEntry> = match raw {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Vec::new(),
        };
        let expanded = expand_home_path(&path)?;
        let canonical = expanded.canonicalize().context("path does not exist")?;
        let canonical_str = canonical.to_string_lossy().to_string();
        if !dirs.iter().any(|e| e.path == canonical_str) {
            dirs.push(CustomScanDirEntry {
                name,
                path: canonical_str,
            });
        }
        store.set_setting(CUSTOM_SCAN_DIRS_KEY, &serde_json::to_string(&dirs)?)?;
        Ok::<_, anyhow::Error>(dirs)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn remove_custom_scan_dir(
    store: State<'_, SkillStore>,
    path: String,
) -> Result<Vec<CustomScanDirEntry>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let raw = store.get_setting(CUSTOM_SCAN_DIRS_KEY)?;
        let mut dirs: Vec<CustomScanDirEntry> = match raw {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Vec::new(),
        };
        let expanded = expand_home_path(&path)?;
        let canonical = expanded.canonicalize()?;
        let canonical_str = canonical.to_string_lossy().to_string();
        dirs.retain(|e| e.path != canonical_str);
        store.set_setting(CUSTOM_SCAN_DIRS_KEY, &serde_json::to_string(&dirs)?)?;
        Ok::<_, anyhow::Error>(dirs)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn get_onboarding_plan(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
) -> Result<OnboardingPlan, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || build_onboarding_plan(&app, &store))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn get_git_cache_cleanup_days(store: State<'_, SkillStore>) -> Result<i64, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(get_git_cache_cleanup_days_core(&store))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn set_git_cache_cleanup_days(
    store: State<'_, SkillStore>,
    days: i64,
) -> Result<i64, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || set_git_cache_cleanup_days_core(&store, days))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn clear_git_cache_now(app: tauri::AppHandle) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        cleanup_git_cache_dirs(&app, std::time::Duration::from_secs(0))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn get_git_cache_ttl_secs(store: State<'_, SkillStore>) -> Result<i64, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(get_git_cache_ttl_secs_core(&store))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn set_git_cache_ttl_secs(
    store: State<'_, SkillStore>,
    secs: i64,
) -> Result<i64, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || set_git_cache_ttl_secs_core(&store, secs))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

#[derive(Debug, Serialize)]
pub struct InstallResultDto {
    pub skill_id: String,
    pub name: String,
    pub central_path: String,
    pub content_hash: Option<String>,
}

fn normalize_scope(scope: Option<&str>) -> Result<&'static str, anyhow::Error> {
    match scope.unwrap_or("global") {
        "global" => Ok("global"),
        "project" => Ok("project"),
        other => anyhow::bail!("invalid scope: {}", other),
    }
}

#[tauri::command]
pub async fn get_recent_projects(store: State<'_, SkillStore>) -> Result<Vec<String>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || get_recent_projects_impl(&store))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn save_recent_project(
    store: State<'_, SkillStore>,
    projectPath: String,
) -> Result<Vec<String>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || save_recent_project_impl(&store, &projectPath))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

fn get_recent_projects_impl(store: &SkillStore) -> Result<Vec<String>, anyhow::Error> {
    let projects = store
        .get_setting(RECENT_PROJECTS_SETTING)?
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default();
    Ok(projects)
}

fn save_recent_project_impl(
    store: &SkillStore,
    project_path: &str,
) -> Result<Vec<String>, anyhow::Error> {
    let path = expand_home_path(project_path)?;
    if !path.is_dir() {
        anyhow::bail!("projectPath must be an existing directory: {:?}", path);
    }
    let normalized = path.to_string_lossy().to_string();
    let mut projects = get_recent_projects_impl(store)?;
    projects.retain(|item| item != &normalized);
    projects.insert(0, normalized);
    projects.truncate(8);
    store.set_setting(
        RECENT_PROJECTS_SETTING,
        &serde_json::to_string(&projects).unwrap_or_else(|_| "[]".to_string()),
    )?;
    Ok(projects)
}

#[tauri::command]
pub async fn get_central_repo_path(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
) -> Result<String, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let path = resolve_central_repo_path(&app, &store)?;
        ensure_central_repo(&path)?;
        Ok::<_, anyhow::Error>(path.to_string_lossy().to_string())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn set_central_repo_path(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    path: String,
) -> Result<String, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let new_base = expand_home_path(&path)?;
        if !new_base.is_absolute() {
            anyhow::bail!("storage path must be absolute");
        }
        ensure_central_repo(&new_base)?;

        let current_base = resolve_central_repo_path(&app, &store)?;
        let skills = store.list_skills()?;
        if current_base == new_base {
            store.set_setting("central_repo_path", new_base.to_string_lossy().as_ref())?;
            return Ok::<_, anyhow::Error>(new_base.to_string_lossy().to_string());
        }

        if !skills.is_empty() {
            for skill in skills {
                let old_path = std::path::PathBuf::from(&skill.central_path);
                if !old_path.exists() {
                    anyhow::bail!("central path not found: {:?}", old_path);
                }
                let file_name = old_path
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("invalid central path: {:?}", old_path))?;
                let new_path = new_base.join(file_name);
                if new_path.exists() {
                    anyhow::bail!("target path already exists: {:?}", new_path);
                }

                if let Err(err) = std::fs::rename(&old_path, &new_path) {
                    copy_dir_recursive(&old_path, &new_path)
                        .with_context(|| format!("copy {:?} -> {:?}", old_path, new_path))?;
                    std::fs::remove_dir_all(&old_path)
                        .with_context(|| format!("cleanup {:?}", old_path))?;
                    // Surface rename error in logs for troubleshooting.
                    eprintln!("rename failed, fallback used: {}", err);
                }

                let mut updated = skill.clone();
                updated.central_path = new_path.to_string_lossy().to_string();
                updated.updated_at = now_ms();
                store.upsert_skill(&updated)?;
            }
        }

        store.set_setting("central_repo_path", new_base.to_string_lossy().as_ref())?;
        Ok::<_, anyhow::Error>(new_base.to_string_lossy().to_string())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn install_local(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    sourcePath: String,
    name: Option<String>,
) -> Result<InstallResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = install_local_skill(&app, &store, sourcePath.as_ref(), name)?;
        to_install_dto_with_origin(&store, result)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn list_local_skills_cmd(basePath: String) -> Result<Vec<LocalSkillCandidate>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = std::path::PathBuf::from(basePath);
        list_local_skills(&path)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn install_local_selection(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    basePath: String,
    subpath: String,
    name: Option<String>,
) -> Result<InstallResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let base = std::path::PathBuf::from(basePath);
        let result =
            install_local_skill_from_selection(&app, &store, base.as_ref(), &subpath, name)?;
        to_install_dto_with_origin(&store, result)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn install_git(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    cancel: State<'_, Arc<CancelToken>>,
    repoUrl: String,
    name: Option<String>,
) -> Result<InstallResultDto, String> {
    let store = store.inner().clone();
    cancel.reset();
    let cancel_token = Arc::clone(cancel.inner());
    tauri::async_runtime::spawn_blocking(move || {
        let result = install_git_skill(&app, &store, &repoUrl, name, Some(&cancel_token))?;
        to_install_dto_with_origin(&store, result)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn list_git_skills_cmd(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    repoUrl: String,
) -> Result<Vec<GitSkillCandidate>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || list_git_skills(&app, &store, &repoUrl))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn install_git_selection(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    repoUrl: String,
    subpath: String,
    name: Option<String>,
) -> Result<InstallResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = install_git_skill_from_selection(&app, &store, &repoUrl, &subpath, name)?;
        to_install_dto_with_origin(&store, result)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn install_package(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    packageName: String,
    command: Option<String>,
    name: Option<String>,
) -> Result<InstallResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let result = install_package_skill(&app, &store, &packageName, command.as_deref(), name)?;
        to_install_dto_with_origin(&store, result)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[derive(Debug, Serialize)]
pub struct SyncResultDto {
    pub mode_used: String,
    pub target_path: String,
}

#[tauri::command]
pub async fn sync_skill_dir(
    source_path: String,
    target_path: String,
) -> Result<SyncResultDto, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let result = sync_dir_hybrid(source_path.as_ref(), target_path.as_ref())?;
        Ok::<_, anyhow::Error>(SyncResultDto {
            mode_used: match result.mode_used {
                SyncMode::Auto => "auto",
                SyncMode::Symlink => "symlink",
                SyncMode::Junction => "junction",
                SyncMode::Copy => "copy",
            }
            .to_string(),
            target_path: result.target_path.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
#[allow(clippy::too_many_arguments)]
pub async fn sync_skill_to_tool(
    store: State<'_, SkillStore>,
    sourcePath: String,
    skillId: String,
    tool: String,
    name: String,
    overwrite: Option<bool>,
    overwriteIfSameContent: Option<bool>,
    scope: Option<String>,
    projectPath: Option<String>,
) -> Result<SyncResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(custom_dir) = custom_tool_dir(&tool) {
            let scope = normalize_scope(scope.as_deref())?;
            if scope != "global" {
                anyhow::bail!("PROJECT_SCOPE_UNSUPPORTED|{}", tool);
            }
            let tool_root = custom_dir;
            if let Err(err) = std::fs::create_dir_all(&tool_root) {
                if err.kind() == std::io::ErrorKind::PermissionDenied {
                    anyhow::bail!("TOOL_NOT_WRITABLE|{}|{}", tool, tool_root.to_string_lossy());
                }
                anyhow::bail!("failed to create skills dir {:?}: {}", tool_root, err);
            }
            let target = tool_root.join(&name);
            if let Some(existing) = store.get_skill_target(&skillId, &tool, scope, None)? {
                if existing.target_path == target.to_string_lossy() && target.exists() {
                    return Ok::<_, anyhow::Error>(SyncResultDto {
                        mode_used: existing.mode,
                        target_path: existing.target_path,
                    });
                }
            }
            let overwrite = overwrite.unwrap_or(false)
                || (overwriteIfSameContent.unwrap_or(false)
                    && target_has_same_content(sourcePath.as_ref(), &target));
            let result =
                sync_dir_for_tool_with_overwrite("custom", sourcePath.as_ref(), &target, overwrite)
                    .map_err(|err| {
                        let msg = err.to_string();
                        if msg.contains("target already exists") {
                            anyhow::anyhow!("TARGET_EXISTS|{}", target.to_string_lossy())
                        } else if msg.contains("os error 5")
                            || msg.contains("Access is denied")
                            || msg.contains("Permission denied")
                        {
                            anyhow::anyhow!(
                                "TOOL_NOT_WRITABLE|{}|{}",
                                tool,
                                tool_root.to_string_lossy()
                            )
                        } else {
                            anyhow::anyhow!(msg)
                        }
                    })?;
            let record = SkillTargetRecord {
                id: Uuid::new_v4().to_string(),
                skill_id: skillId,
                tool,
                scope: scope.to_string(),
                project_path: None,
                target_path: result.target_path.to_string_lossy().to_string(),
                mode: match result.mode_used {
                    SyncMode::Auto => "auto",
                    SyncMode::Symlink => "symlink",
                    SyncMode::Junction => "junction",
                    SyncMode::Copy => "copy",
                }
                .to_string(),
                status: "ok".to_string(),
                last_error: None,
                synced_at: Some(now_ms()),
            };
            store.upsert_skill_target(&record)?;

            return Ok::<_, anyhow::Error>(SyncResultDto {
                mode_used: match result.mode_used {
                    SyncMode::Auto => "auto",
                    SyncMode::Symlink => "symlink",
                    SyncMode::Junction => "junction",
                    SyncMode::Copy => "copy",
                }
                .to_string(),
                target_path: result.target_path.to_string_lossy().to_string(),
            });
        }

        let adapter = adapter_by_key(&tool).ok_or_else(|| anyhow::anyhow!("unknown tool"))?;
        let scope = normalize_scope(scope.as_deref())?;
        if scope == "project" && !supports_project_scope(&adapter) {
            anyhow::bail!("PROJECT_SCOPE_UNSUPPORTED|{}", adapter.id.as_key());
        }
        let project_root = if scope == "project" {
            let raw = projectPath
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("projectPath is required for project scope"))?;
            let path = expand_home_path(raw)?;
            if !path.is_dir() {
                anyhow::bail!("projectPath must be an existing directory: {:?}", path);
            }
            Some(path)
        } else {
            None
        };

        if scope == "global" && !is_tool_available(&adapter, &tool, &store)? {
            anyhow::bail!("TOOL_NOT_INSTALLED|{}", adapter.id.as_key());
        }
        let tool_root = if let Some(project_root) = &project_root {
            resolve_project_path(&adapter, project_root)?
        } else {
            let override_dir = resolve_tool_global_dir(&tool, &store)?;
            PathBuf::from(override_dir)
        };
        // Pre-check: ensure the skills directory is writable (fixes #20 — Windows OS error 5).
        if let Err(err) = std::fs::create_dir_all(&tool_root) {
            if err.kind() == std::io::ErrorKind::PermissionDenied {
                anyhow::bail!(
                    "TOOL_NOT_WRITABLE|{}|{}",
                    adapter.display_name,
                    tool_root.to_string_lossy()
                );
            }
            anyhow::bail!("failed to create skills dir {:?}: {}", tool_root, err);
        }
        let target = tool_root.join(&name);
        let project_path_for_record = project_root
            .as_ref()
            .map(|path| path.to_string_lossy().to_string());
        if let Some(existing) =
            store.get_skill_target(&skillId, &tool, scope, project_path_for_record.as_deref())?
        {
            if existing.target_path == target.to_string_lossy() && target.exists() {
                return Ok::<_, anyhow::Error>(SyncResultDto {
                    mode_used: existing.mode,
                    target_path: existing.target_path,
                });
            }
        }
        let overwrite = overwrite.unwrap_or(false)
            || (overwriteIfSameContent.unwrap_or(false)
                && target_has_same_content(sourcePath.as_ref(), &target));
        let result =
            sync_dir_for_tool_with_overwrite(&tool, sourcePath.as_ref(), &target, overwrite)
                .map_err(|err| {
                    let msg = err.to_string();
                    if msg.contains("target already exists") {
                        anyhow::anyhow!("TARGET_EXISTS|{}", target.to_string_lossy())
                    } else if msg.contains("os error 5")
                        || msg.contains("Access is denied")
                        || msg.contains("Permission denied")
                    {
                        anyhow::anyhow!(
                            "TOOL_NOT_WRITABLE|{}|{}",
                            adapter.display_name,
                            tool_root.to_string_lossy()
                        )
                    } else {
                        anyhow::anyhow!(msg)
                    }
                })?;

        // Some tools share the same skills directory; keep DB records consistent across them.
        let group = if scope == "project" {
            adapters_sharing_project_skills_dir(&adapter)
        } else {
            crate::core::tool_adapters::adapters_sharing_skills_dir(&adapter)
        };
        for a in group {
            let key = a.id.as_key().to_string();
            if !is_tool_available(&a, &key, &store)? {
                continue;
            }
            let record = SkillTargetRecord {
                id: Uuid::new_v4().to_string(),
                skill_id: skillId.clone(),
                tool: key,
                scope: scope.to_string(),
                project_path: project_path_for_record.clone(),
                target_path: result.target_path.to_string_lossy().to_string(),
                mode: match result.mode_used {
                    SyncMode::Auto => "auto",
                    SyncMode::Symlink => "symlink",
                    SyncMode::Junction => "junction",
                    SyncMode::Copy => "copy",
                }
                .to_string(),
                status: "ok".to_string(),
                last_error: None,
                synced_at: Some(now_ms()),
            };
            store.upsert_skill_target(&record)?;
        }

        Ok::<_, anyhow::Error>(SyncResultDto {
            mode_used: match result.mode_used {
                SyncMode::Auto => "auto",
                SyncMode::Symlink => "symlink",
                SyncMode::Junction => "junction",
                SyncMode::Copy => "copy",
            }
            .to_string(),
            target_path: result.target_path.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

fn target_has_same_content(source: &std::path::Path, target: &std::path::Path) -> bool {
    if !source.is_dir() || !target.is_dir() {
        return false;
    }
    match (hash_dir(source), hash_dir(target)) {
        (Ok(source_hash), Ok(target_hash)) => source_hash == target_hash,
        _ => false,
    }
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn unsync_skill_from_tool(
    store: State<'_, SkillStore>,
    skillId: String,
    tool: String,
    scope: Option<String>,
    projectPath: Option<String>,
) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let scope = normalize_scope(scope.as_deref())?;
        let project_path = if scope == "project" {
            let raw = projectPath
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("projectPath is required for project scope"))?;
            Some(expand_home_path(raw)?.to_string_lossy().to_string())
        } else {
            None
        };

        // Some tools share the same skills directory; unsync should update all of them.
        let group_tool_keys: Vec<String> = if let Some(adapter) = adapter_by_key(&tool) {
            let group = if scope == "project" {
                adapters_sharing_project_skills_dir(&adapter)
            } else {
                crate::core::tool_adapters::adapters_sharing_skills_dir(&adapter)
            };
            // If none of the group tools are installed, do nothing (treat as already not effective).
            if scope == "global" {
                let mut any_installed = false;
                for a in &group {
                    let key = a.id.as_key().to_string();
                    if is_tool_available(a, &key, &store)? {
                        any_installed = true;
                        break;
                    }
                }
                if !any_installed {
                    return Ok::<_, anyhow::Error>(());
                }
            }
            group
                .into_iter()
                .map(|a| a.id.as_key().to_string())
                .collect()
        } else {
            vec![tool.clone()]
        };

        // Remove filesystem target once (shared dir => shared target path).
        let mut removed = false;
        for k in &group_tool_keys {
            if let Some(target) =
                store.get_skill_target(&skillId, k, scope, project_path.as_deref())?
            {
                if !removed {
                    remove_path_any(&target.target_path).map_err(anyhow::Error::msg)?;
                    removed = true;
                }
                store.delete_skill_target(&skillId, k, scope, project_path.as_deref())?;
            }
        }

        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[derive(Debug, Serialize)]
pub struct UpdateResultDto {
    pub skill_id: String,
    pub name: String,
    pub content_hash: Option<String>,
    pub source_revision: Option<String>,
    pub updated_targets: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct UpdateCheckResultDto {
    pub skill_id: String,
    pub name: String,
    pub checkable: bool,
    pub has_update: bool,
    pub has_local_changes: bool,
    pub current_revision: Option<String>,
    pub latest_revision: Option<String>,
    pub current_hash: Option<String>,
    pub latest_hash: Option<String>,
    pub message: Option<String>,
}

impl From<crate::core::installer::UpdateCheckResult> for UpdateCheckResultDto {
    fn from(value: crate::core::installer::UpdateCheckResult) -> Self {
        Self {
            skill_id: value.skill_id,
            name: value.name,
            checkable: value.checkable,
            has_update: value.has_update,
            has_local_changes: value.has_local_changes,
            current_revision: value.current_revision,
            latest_revision: value.latest_revision,
            current_hash: value.current_hash,
            latest_hash: value.latest_hash,
            message: value.message,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct PublishResultDto {
    pub skill_id: String,
    pub name: String,
    pub commit: Option<String>,
    pub pushed: bool,
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn update_managed_skill(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    skillId: String,
) -> Result<UpdateResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let res = update_managed_skill_from_source(&app, &store, &skillId)?;
        Ok::<_, anyhow::Error>(UpdateResultDto {
            skill_id: res.skill_id,
            name: res.name,
            content_hash: res.content_hash,
            source_revision: res.source_revision,
            updated_targets: res.updated_targets,
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn check_managed_skill_update_cmd(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    skillId: String,
) -> Result<UpdateCheckResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        check_managed_skill_update(&app, &store, &skillId).map(UpdateCheckResultDto::from)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn check_all_managed_skill_updates_cmd(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
) -> Result<Vec<UpdateCheckResultDto>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        check_all_managed_skill_updates(&app, &store)
            .map(|items| items.into_iter().map(UpdateCheckResultDto::from).collect())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn publish_managed_skill(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    skillId: String,
    message: Option<String>,
) -> Result<PublishResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let origin = store.get_skill_origin(&skillId)?;
        // Permission guard: only "my" skills (origin_role == "mine") can be pushed.
        if origin.as_ref().map(|item| item.origin_role.as_str()) != Some("mine") {
            anyhow::bail!("this skill is not yours — only skills matched via my_git_owners/my_git_repos can be pushed");
        }
        if origin.as_ref().map(|item| item.publish_strategy.as_str()) != Some("git_push") {
            anyhow::bail!("this skill is not configured for Git push");
        }
        let res = publish_managed_skill_to_remote(&app, &store, &skillId, message.as_deref())?;
        Ok::<_, anyhow::Error>(PublishResultDto {
            skill_id: res.skill_id,
            name: res.name,
            commit: res.commit,
            pushed: res.pushed,
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn search_github(
    store: State<'_, SkillStore>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<RepoSummary>, String> {
    let store = store.inner().clone();
    let limit = limit.unwrap_or(10) as usize;
    tauri::async_runtime::spawn_blocking(move || {
        let token = store.get_setting("github_token")?.unwrap_or_default();
        let token_opt = if token.is_empty() {
            None
        } else {
            Some(token.as_str())
        };
        search_github_repos(&query, limit, token_opt)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn get_github_token(store: State<'_, SkillStore>) -> Result<String, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok::<_, anyhow::Error>(store.get_setting("github_token")?.unwrap_or_default())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn set_github_token(store: State<'_, SkillStore>, token: String) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            store.set_setting("github_token", "")?;
        } else {
            store.set_setting("github_token", trimmed)?;
        }
        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub fn get_origin_rules(store: State<'_, SkillStore>) -> Result<OriginRules, String> {
    get_origin_rules_impl(store.inner()).map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn set_origin_rules(
    store: State<'_, SkillStore>,
    rules: OriginRules,
) -> Result<OriginRules, String> {
    let normalized = normalize_rules(rules);
    let json = serde_json::to_string(&normalized).map_err(|err| err.to_string())?;
    store
        .set_setting(ORIGIN_RULES_KEY, &json)
        .map_err(format_anyhow_error)?;
    Ok(normalized)
}

// ---------------------------------------------------------------------------
// Unified app config — aggregation now lives in `core::app_config`.
// The Tauri command handlers below delegate to those core functions.
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn get_app_config(store: State<'_, SkillStore>) -> Result<AppConfig, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || load_app_config(&store))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn save_app_config(
    store: State<'_, SkillStore>,
    config: AppConfig,
) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || save_app_config_impl(&store, &config))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn export_config(store: State<'_, SkillStore>) -> Result<String, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<String> {
        let mut cfg = load_app_config(&store)?;
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        cfg.exported_at = Some(secs.to_string());
        Ok(export_config_json(&cfg))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn import_config(
    store: State<'_, SkillStore>,
    json: String,
) -> Result<AppConfig, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<AppConfig> {
        let mut cfg = parse_config_json(&json)?;
        let current = load_app_config(&store)?;
        cfg.preserve_missing_secrets_from(&current);
        save_app_config_impl(&store, &cfg)?;
        Ok(cfg)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

// `GithubTokenStatus` and `compute_github_token_status` live in
// `core::github_auth` (shared with the CLI); re-export for the command layer.
pub use crate::core::github_auth::{
    compute_github_token_status, GithubOwnerEntry, GithubTokenStatus,
};

#[tauri::command]
pub async fn validate_github_token(token: String) -> Result<GithubTokenStatus, String> {
    let token = token.trim().to_string();
    let status = tauri::async_runtime::spawn_blocking(move || compute_github_token_status(token))
        .await
        .map_err(|err| err.to_string())?;
    Ok(status)
}

/// List unique GitHub owners/orgs from the authenticated user's repos.
#[tauri::command]
pub async fn list_github_owners(
    store: State<'_, SkillStore>,
) -> Result<Vec<GithubOwnerEntry>, String> {
    let store = store.inner().clone();
    let token = store
        .get_setting("github_token")
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    if token.is_empty() {
        return Err("GitHub token 未配置".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        crate::core::github_auth::list_github_owners(token)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(|err| err.to_string())
}

#[tauri::command]
pub async fn write_text_file(path: String, contents: String) -> Result<(), String> {
    std::fs::write(&path, contents).map_err(|err| format!("写入文件失败: {}", err))
}

#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String, String> {
    std::fs::read_to_string(&path).map_err(|err| format!("读取文件失败: {}", err))
}

// ---------------------------------------------------------------------------
// Full-state backup / restore (settings + skills list)
// ---------------------------------------------------------------------------

/// One entry in a [`RestoreReportDto`] (a skipped or failed skill).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreItemDto {
    pub name: String,
    pub reason: String,
}

/// Structured result of a restore, for the GUI to render.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RestoreReportDto {
    pub installed: Vec<String>,
    pub skipped: Vec<RestoreItemDto>,
    pub failed: Vec<RestoreItemDto>,
    pub summary: String,
}

fn to_report_dto(report: &RestoreReport) -> RestoreReportDto {
    RestoreReportDto {
        installed: report.installed.clone(),
        skipped: report
            .skipped
            .iter()
            .map(|(name, reason)| RestoreItemDto {
                name: name.clone(),
                reason: reason.clone(),
            })
            .collect(),
        failed: report
            .failed
            .iter()
            .map(|(name, reason)| RestoreItemDto {
                name: name.clone(),
                reason: reason.clone(),
            })
            .collect(),
        summary: report.summary(),
    }
}

/// Produce the combined backup blob as a JSON string (for the GUI to save via
/// a file dialog).
#[tauri::command]
pub async fn export_full_backup_json(store: State<'_, SkillStore>) -> Result<String, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || export_full_backup(&store))
        .await
        .map_err(|err| err.to_string())?
        .map_err(format_anyhow_error)
}

/// Write the combined backup blob to a local file.
#[tauri::command]
pub async fn backup_to_file(store: State<'_, SkillStore>, path: String) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<()> {
        let json = export_full_backup(&store)?;
        std::fs::write(&path, json).map_err(|err| anyhow::anyhow!("写入文件失败: {err}"))?;
        Ok(())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

/// Upload the combined backup to the configured WebDAV server.
#[tauri::command]
pub async fn backup_webdav(store: State<'_, SkillStore>) -> Result<String, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<String> {
        let cfg = load_app_config(&store)?;
        let wd = cfg
            .webdav
            .ok_or_else(|| anyhow::anyhow!("WebDAV 未配置，请先在设置中填写"))?;
        let body = export_full_backup(&store)?;
        upload_backup(&wd, &body)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

/// Restore the combined backup from a local file.
#[tauri::command]
pub async fn restore_from_file(
    store: State<'_, SkillStore>,
    path: String,
) -> Result<RestoreReportDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<RestoreReportDto> {
        let raw =
            std::fs::read_to_string(&path).map_err(|err| anyhow::anyhow!("读取文件失败: {err}"))?;
        let report = restore_full_backup(&store, &raw)?;
        Ok(to_report_dto(&report))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

/// Restore the combined backup from the configured WebDAV server.
#[tauri::command]
pub async fn restore_from_webdav(store: State<'_, SkillStore>) -> Result<RestoreReportDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<RestoreReportDto> {
        let cfg = load_app_config(&store)?;
        let wd = cfg
            .webdav
            .ok_or_else(|| anyhow::anyhow!("WebDAV 未配置，请先在设置中填写"))?;
        let raw = download_backup(&wd)?;
        let report = restore_full_backup(&store, &raw)?;
        Ok(to_report_dto(&report))
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

/// Persist only the WebDAV connection profile (loads fresh config, so it never
/// clobbers other settings edited elsewhere).
#[tauri::command]
pub async fn set_webdav_config(
    store: State<'_, SkillStore>,
    webdav: WebDavConfig,
) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<()> {
        let mut cfg = load_app_config(&store)?;
        cfg.webdav = Some(webdav);
        save_app_config_impl(&store, &cfg)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

fn manual_origin_record(
    skill: &crate::core::skill_store::SkillRecord,
    existing: Option<SkillOriginRecord>,
    source_origin: &str,
) -> anyhow::Result<SkillOriginRecord> {
    let mut base = existing.unwrap_or_else(|| {
        let rules = OriginRules::default();
        let inferred = infer_source_origin(
            &skill.source_type,
            skill.source_ref.as_deref(),
            &skill.central_path,
            &rules,
        );
        SkillOriginRecord {
            skill_id: skill.id.clone(),
            origin_kind: inferred.origin_kind,
            origin_role: inferred.origin_role,
            provider: inferred.provider,
            remote_url: inferred.remote_url,
            owner: inferred.owner,
            repo: inferred.repo,
            branch: None,
            subpath: skill.source_subpath.clone(),
            update_strategy: inferred.update_strategy,
            publish_strategy: inferred.publish_strategy,
            manual_override: false,
            reason: Some(inferred.reason),
            updated_at: now_ms(),
        }
    });

    match source_origin {
        "official" => {
            base.origin_kind = "official".to_string();
            base.origin_role = "official".to_string();
            base.provider = Some("official".to_string());
            base.update_strategy = "provider_refresh".to_string();
            base.publish_strategy = "none".to_string();
        }
        "my_git" => {
            base.origin_kind = "git".to_string();
            base.origin_role = "mine".to_string();
            base.provider = Some("git".to_string());
            base.update_strategy = "git_pull".to_string();
            base.publish_strategy = "git_push".to_string();
        }
        "third_party_git" => {
            base.origin_kind = "git".to_string();
            base.origin_role = "third_party".to_string();
            base.provider = Some("git".to_string());
            base.update_strategy = "git_pull".to_string();
            base.publish_strategy = "none".to_string();
        }
        "package" => {
            base.origin_kind = "package".to_string();
            base.origin_role = "repository".to_string();
            base.provider = Some("npm".to_string());
            base.update_strategy = "package_refresh".to_string();
            base.publish_strategy = "none".to_string();
        }
        "local" => {
            base.origin_kind = "local".to_string();
            base.origin_role = "mine".to_string();
            base.provider = Some("local".to_string());
            base.update_strategy = "local_copy".to_string();
            base.publish_strategy = "none".to_string();
        }
        other => anyhow::bail!("invalid source_origin: {}", other),
    }
    base.manual_override = true;
    base.reason = Some("manual override".to_string());
    base.updated_at = now_ms();
    Ok(base)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn set_skill_origin_override(
    store: State<'_, SkillStore>,
    skillId: String,
    sourceOrigin: String,
) -> Result<(), String> {
    let skill = store
        .get_skill_by_id(&skillId)
        .map_err(format_anyhow_error)?
        .ok_or_else(|| "skill not found".to_string())?;
    let existing = store
        .get_skill_origin(&skillId)
        .map_err(format_anyhow_error)?;
    let record =
        manual_origin_record(&skill, existing, &sourceOrigin).map_err(format_anyhow_error)?;
    store
        .upsert_skill_origin(&record)
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn reset_skill_origin_override(
    store: State<'_, SkillStore>,
    skillId: String,
) -> Result<(), String> {
    let skill = store
        .get_skill_by_id(&skillId)
        .map_err(format_anyhow_error)?
        .ok_or_else(|| "skill not found".to_string())?;
    let rules = get_origin_rules_impl(store.inner()).map_err(format_anyhow_error)?;
    let inferred = infer_source_origin(
        &skill.source_type,
        skill.source_ref.as_deref(),
        &skill.central_path,
        &rules,
    );
    let record = SkillOriginRecord {
        skill_id: skill.id,
        origin_kind: inferred.origin_kind,
        origin_role: inferred.origin_role,
        provider: inferred.provider,
        remote_url: inferred.remote_url,
        owner: inferred.owner,
        repo: inferred.repo,
        branch: None,
        subpath: skill.source_subpath,
        update_strategy: inferred.update_strategy,
        publish_strategy: inferred.publish_strategy,
        manual_override: false,
        reason: Some(inferred.reason),
        updated_at: now_ms(),
    };
    store
        .upsert_skill_origin(&record)
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn import_existing_skill(
    app: tauri::AppHandle,
    store: State<'_, SkillStore>,
    sourcePath: String,
    name: Option<String>,
) -> Result<InstallResultDto, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let source = std::path::Path::new(&sourcePath);
        // Validate SKILL.md exists before importing (fixes #8: prevents importing
        // directories that were "discovered" but lack a valid SKILL.md).
        if !source.join("SKILL.md").exists() {
            anyhow::bail!("SKILL_INVALID|missing_skill_md");
        }
        let result = install_local_skill(&app, &store, source, name)?;
        to_install_dto_with_origin(&store, result)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[derive(Debug, Serialize)]
pub struct ManagedSkillDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub source_type: String,
    pub source_ref: Option<String>,
    pub source_origin: String,
    pub origin_kind: String,
    pub origin_role: String,
    pub origin_provider: Option<String>,
    pub origin_remote_url: Option<String>,
    pub origin_owner: Option<String>,
    pub origin_repo: Option<String>,
    pub update_strategy: String,
    pub publish_strategy: String,
    pub origin_manual_override: bool,
    pub source_origin_reason: Option<String>,
    pub central_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_sync_at: Option<i64>,
    pub status: String,
    pub tags: Vec<TagDto>,
    pub targets: Vec<SkillTargetDto>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TagDto {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TagWithCountDto {
    pub id: i64,
    pub name: String,
    pub skill_count: i64,
    pub updated_at: i64,
}

#[derive(Debug, Serialize)]
pub struct SkillTargetDto {
    pub tool: String,
    pub scope: String,
    pub project_path: Option<String>,
    pub mode: String,
    pub status: String,
    pub target_path: String,
    pub synced_at: Option<i64>,
}

#[tauri::command]
pub fn get_managed_skills(store: State<'_, SkillStore>) -> Result<Vec<ManagedSkillDto>, String> {
    get_managed_skills_impl(store.inner())
}

#[tauri::command]
pub fn get_tags(store: State<'_, SkillStore>) -> Result<Vec<TagWithCountDto>, String> {
    store
        .list_tags_with_counts()
        .map(|tags| {
            tags.into_iter()
                .map(|tag| TagWithCountDto {
                    id: tag.id,
                    name: tag.name,
                    skill_count: tag.skill_count,
                    updated_at: tag.updated_at,
                })
                .collect()
        })
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn create_tag(store: State<'_, SkillStore>, name: String) -> Result<TagDto, String> {
    store
        .create_tag(&name)
        .map(|tag| TagDto {
            id: tag.id,
            name: tag.name,
        })
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn rename_tag(
    store: State<'_, SkillStore>,
    tagId: i64,
    name: String,
) -> Result<TagDto, String> {
    store
        .rename_tag(tagId, &name)
        .map(|tag| TagDto {
            id: tag.id,
            name: tag.name,
        })
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn delete_tag(store: State<'_, SkillStore>, tagId: i64) -> Result<(), String> {
    store.delete_tag(tagId).map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn get_skill_tags(
    store: State<'_, SkillStore>,
    skillId: String,
) -> Result<Vec<TagDto>, String> {
    store
        .get_skill_tags(&skillId)
        .map(|tags| {
            tags.into_iter()
                .map(|tag| TagDto {
                    id: tag.id,
                    name: tag.name,
                })
                .collect()
        })
        .map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub fn set_skill_tags(
    store: State<'_, SkillStore>,
    skillId: String,
    tagIds: Vec<i64>,
) -> Result<(), String> {
    store
        .set_skill_tags(&skillId, &tagIds)
        .map_err(format_anyhow_error)
}

#[tauri::command]
pub fn get_untagged_skill_ids(store: State<'_, SkillStore>) -> Result<Vec<String>, String> {
    store.list_untagged_skill_ids().map_err(format_anyhow_error)
}

#[tauri::command]
#[allow(non_snake_case)]
pub async fn delete_managed_skill(
    store: State<'_, SkillStore>,
    skillId: String,
) -> Result<(), String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // 便于排查“按钮点了没反应”：确认前端确实触发了命令
        println!("[delete_managed_skill] skillId={}", skillId);

        // 先删除已同步到各工具目录的副本/软链接
        // 注意：如果先删 skills 行，会触发 skill_targets cascade，导致无法再拿到 target_path
        let targets = store.list_skill_targets(&skillId)?;

        let mut remove_failures: Vec<String> = Vec::new();
        for target in targets {
            if let Err(err) = remove_path_any(&target.target_path) {
                remove_failures.push(format!("{}: {}", target.target_path, err));
            }
        }

        let record = store.get_skill_by_id(&skillId)?;
        if let Some(skill) = record {
            let path = std::path::PathBuf::from(skill.central_path);
            if path.exists() {
                std::fs::remove_dir_all(&path)?;
            }
            store.delete_skill(&skillId)?;
        }

        if !remove_failures.is_empty() {
            anyhow::bail!(
                "已删除托管记录，但清理部分工具目录失败：\n- {}",
                remove_failures.join("\n- ")
            );
        }

        Ok::<_, anyhow::Error>(())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

fn remove_path_any(path: &str) -> Result<(), String> {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Ok(());
    }

    let meta = std::fs::symlink_metadata(p).map_err(|err| err.to_string())?;
    let ft = meta.file_type();

    // 软链接（即使指向目录）也应该用 remove_file 删除链接本身
    if ft.is_symlink() {
        std::fs::remove_file(p).map_err(|err| err.to_string())?;
        return Ok(());
    }

    if ft.is_dir() {
        std::fs::remove_dir_all(p).map_err(|err| err.to_string())?;
        return Ok(());
    }

    std::fs::remove_file(p).map_err(|err| err.to_string())?;
    Ok(())
}

fn to_install_dto(result: InstallResult) -> InstallResultDto {
    InstallResultDto {
        skill_id: result.skill_id,
        name: result.name,
        central_path: result.central_path.to_string_lossy().to_string(),
        content_hash: result.content_hash,
    }
}

fn to_install_dto_with_origin(
    store: &SkillStore,
    result: InstallResult,
) -> anyhow::Result<InstallResultDto> {
    if let Some(skill) = store.get_skill_by_id(&result.skill_id)? {
        let rules = get_origin_rules_impl(store)?;
        let inferred = infer_source_origin(
            &skill.source_type,
            skill.source_ref.as_deref(),
            &skill.central_path,
            &rules,
        );
        store.upsert_skill_origin(&SkillOriginRecord {
            skill_id: skill.id,
            origin_kind: inferred.origin_kind,
            origin_role: inferred.origin_role,
            provider: inferred.provider,
            remote_url: inferred.remote_url,
            owner: inferred.owner,
            repo: inferred.repo,
            branch: None,
            subpath: skill.source_subpath,
            update_strategy: inferred.update_strategy,
            publish_strategy: inferred.publish_strategy,
            manual_override: false,
            reason: Some(inferred.reason),
            updated_at: now_ms(),
        })?;
    }
    Ok(to_install_dto(result))
}

fn now_ms() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    now.as_millis() as i64
}

fn get_managed_skills_impl(store: &SkillStore) -> Result<Vec<ManagedSkillDto>, String> {
    let skills = store.list_skills().map_err(|err| err.to_string())?;
    let rules = get_origin_rules_impl(store).map_err(|err| err.to_string())?;
    let featured_sources = featured_origin_map(store);
    Ok(skills
        .into_iter()
        .map(|mut skill| {
            let origin = match store.get_skill_origin(&skill.id) {
                Ok(Some(origin)) if origin.manual_override => origin,
                _ => {
                    let featured_source = featured_sources
                        .get(&normalize_skill_key(&skill.name))
                        .filter(|source| {
                            skill.source_type == "local"
                                && !skill.source_ref.as_deref().is_some_and(|source_ref| {
                                    normalize_source_ref(source_ref)
                                        == normalize_source_ref(source.as_str())
                                })
                        })
                        .cloned();
                    if let Some(source_url) = featured_source {
                        skill.source_type = "git".to_string();
                        skill.source_ref = Some(source_url);
                        skill.source_subpath = None;
                        let _ = store.upsert_skill(&skill);
                    }
                    let inferred = infer_source_origin(
                        &skill.source_type,
                        skill.source_ref.as_deref(),
                        &skill.central_path,
                        &rules,
                    );
                    let record = SkillOriginRecord {
                        skill_id: skill.id.clone(),
                        origin_kind: inferred.origin_kind,
                        origin_role: inferred.origin_role,
                        provider: inferred.provider,
                        remote_url: inferred.remote_url,
                        owner: inferred.owner,
                        repo: inferred.repo,
                        branch: None,
                        subpath: skill.source_subpath.clone(),
                        update_strategy: inferred.update_strategy,
                        publish_strategy: inferred.publish_strategy,
                        manual_override: false,
                        reason: Some(inferred.reason),
                        updated_at: now_ms(),
                    };
                    let _ = store.upsert_skill_origin(&record);
                    record
                }
            };
            let source_origin = match (origin.origin_kind.as_str(), origin.origin_role.as_str()) {
                ("official", _) | (_, "official") => "official",
                ("git", "owned") | ("git", "mine") => "my_git",
                ("git", _) => "git",
                ("package", _) => "package",
                ("local", _) => "local",
                _ => "local",
            }
            .to_string();
            let targets = store
                .list_skill_targets(&skill.id)
                .unwrap_or_default()
                .into_iter()
                .map(|target| SkillTargetDto {
                    tool: target.tool,
                    scope: target.scope,
                    project_path: target.project_path,
                    mode: target.mode,
                    status: target.status,
                    target_path: target.target_path,
                    synced_at: target.synced_at,
                })
                .collect();
            let tags = store
                .get_skill_tags(&skill.id)
                .unwrap_or_default()
                .into_iter()
                .map(|tag| TagDto {
                    id: tag.id,
                    name: tag.name,
                })
                .collect();

            ManagedSkillDto {
                id: skill.id,
                name: skill.name,
                description: skill.description,
                source_origin,
                origin_kind: origin.origin_kind,
                origin_role: origin.origin_role,
                origin_provider: origin.provider,
                origin_remote_url: origin.remote_url,
                origin_owner: origin.owner,
                origin_repo: origin.repo,
                update_strategy: origin.update_strategy,
                publish_strategy: origin.publish_strategy,
                origin_manual_override: origin.manual_override,
                source_origin_reason: origin.reason,
                source_type: skill.source_type,
                source_ref: skill.source_ref,
                central_path: skill.central_path,
                created_at: skill.created_at,
                updated_at: skill.updated_at,
                last_sync_at: skill.last_sync_at,
                status: skill.status,
                tags,
                targets,
            }
        })
        .collect())
}

#[derive(Debug, Serialize)]
pub struct FeaturedSkillDto {
    pub slug: String,
    pub name: String,
    pub summary: String,
    pub downloads: u64,
    pub stars: u64,
    pub source_url: String,
}

impl From<FeaturedSkill> for FeaturedSkillDto {
    fn from(s: FeaturedSkill) -> Self {
        Self {
            slug: s.slug,
            name: s.name,
            summary: s.summary,
            downloads: s.downloads,
            stars: s.stars,
            source_url: s.source_url,
        }
    }
}

#[tauri::command]
pub async fn get_featured_skills(
    store: State<'_, SkillStore>,
) -> Result<Vec<FeaturedSkillDto>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let skills = fetch_featured_skills(&store)?;
        Ok::<_, anyhow::Error>(skills.into_iter().map(FeaturedSkillDto::from).collect())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[derive(Debug, Serialize)]
pub struct OnlineSkillDto {
    pub name: String,
    pub installs: u64,
    pub source: String,
    pub source_url: String,
}

impl From<OnlineSkillResult> for OnlineSkillDto {
    fn from(r: OnlineSkillResult) -> Self {
        Self {
            name: r.name,
            installs: r.installs,
            source: r.source,
            source_url: r.source_url,
        }
    }
}

#[tauri::command]
pub async fn search_skills_online(
    query: String,
    limit: Option<u32>,
) -> Result<Vec<OnlineSkillDto>, String> {
    let limit = limit.unwrap_or(20) as usize;
    tauri::async_runtime::spawn_blocking(move || {
        let results = search_skills_online_core(&query, limit)?;
        Ok::<_, anyhow::Error>(results.into_iter().map(OnlineSkillDto::from).collect())
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreSourceConfigDto {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub endpoint: String,
    pub enabled: bool,
    pub builtin: bool,
}

impl From<ExploreSourceConfig> for ExploreSourceConfigDto {
    fn from(source: ExploreSourceConfig) -> Self {
        Self {
            id: source.id,
            name: source.name,
            kind: source.kind,
            endpoint: source.endpoint,
            enabled: source.enabled,
            builtin: source.builtin,
        }
    }
}

impl From<ExploreSourceConfigDto> for ExploreSourceConfig {
    fn from(source: ExploreSourceConfigDto) -> Self {
        Self {
            id: source.id,
            name: source.name,
            kind: source.kind,
            endpoint: source.endpoint,
            enabled: source.enabled,
            builtin: source.builtin,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreSkillDto {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub source_url: String,
    pub source_name: String,
    pub source_kind: String,
    pub downloads: u64,
    pub stars: u64,
}

impl From<ExploreSkill> for ExploreSkillDto {
    fn from(skill: ExploreSkill) -> Self {
        Self {
            id: skill.id,
            name: skill.name,
            summary: skill.summary,
            source_url: skill.source_url,
            source_name: skill.source_name,
            source_kind: skill.source_kind,
            downloads: skill.downloads,
            stars: skill.stars,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreFetchResultDto {
    pub skills: Vec<ExploreSkillDto>,
    pub errors: Vec<String>,
}

#[tauri::command]
pub async fn get_explore_sources(
    store: State<'_, SkillStore>,
) -> Result<Vec<ExploreSourceConfigDto>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sources = get_explore_sources_core(&store)?;
        Ok::<_, anyhow::Error>(
            sources
                .into_iter()
                .map(ExploreSourceConfigDto::from)
                .collect(),
        )
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn save_explore_sources(
    store: State<'_, SkillStore>,
    sources: Vec<ExploreSourceConfigDto>,
) -> Result<Vec<ExploreSourceConfigDto>, String> {
    let store = store.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let sources: Vec<ExploreSourceConfig> = sources.into_iter().map(Into::into).collect();
        let saved = save_explore_sources_core(&store, &sources)?;
        Ok::<_, anyhow::Error>(
            saved
                .into_iter()
                .map(ExploreSourceConfigDto::from)
                .collect(),
        )
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn get_explore_skills(
    store: State<'_, SkillStore>,
    query: Option<String>,
    limit: Option<u32>,
) -> Result<ExploreFetchResultDto, String> {
    let store = store.inner().clone();
    let limit = limit.unwrap_or(80) as usize;
    tauri::async_runtime::spawn_blocking(move || {
        let result = get_explore_skills_core(&store, query.as_deref(), limit)?;
        Ok::<_, anyhow::Error>(ExploreFetchResultDto {
            skills: result
                .skills
                .into_iter()
                .map(ExploreSkillDto::from)
                .collect(),
            errors: result.errors,
        })
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillFileEntry {
    pub path: String,
    pub size: u64,
}

#[tauri::command]
pub async fn list_skill_files(central_path: String) -> Result<Vec<SkillFileEntry>, String> {
    let path = std::path::PathBuf::from(&central_path);
    tauri::async_runtime::spawn_blocking(move || {
        let entries = crate::core::skill_files::list_files(&path)?;
        Ok::<_, anyhow::Error>(
            entries
                .into_iter()
                .map(|e| SkillFileEntry {
                    path: e.path,
                    size: e.size,
                })
                .collect(),
        )
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub async fn read_skill_file(central_path: String, file_path: String) -> Result<String, String> {
    let base = std::path::PathBuf::from(&central_path);
    tauri::async_runtime::spawn_blocking(move || {
        crate::core::skill_files::read_file(&base, &file_path)
    })
    .await
    .map_err(|err| err.to_string())?
    .map_err(format_anyhow_error)
}

#[tauri::command]
pub fn cancel_current_operation(cancel: State<'_, Arc<CancelToken>>) -> Result<(), String> {
    cancel.cancel();
    Ok(())
}

#[cfg(test)]
#[path = "tests/commands.rs"]
mod tests;
