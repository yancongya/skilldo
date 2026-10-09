//! Audit and repair managed Skills that were imported as local directories
//! even though their source path belongs to a Git repository.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::Repository;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use super::app_config::get_origin_rules_impl;
use super::content_hash::hash_dir;
use super::git_fetcher::{clone_or_pull, clone_or_pull_sparse};
use super::skill_store::{SkillOriginRecord, SkillRecord, SkillStore};

#[derive(Debug, Clone)]
pub struct DetectedGitSource {
    pub repo_root: PathBuf,
    pub remote_url: String,
    pub branch: Option<String>,
    pub subpath: Option<String>,
    pub revision: Option<String>,
}

#[derive(Debug, Clone)]
pub enum LocalGitSourceAssessment {
    NotGitWorktree,
    Clean(Option<DetectedGitSource>),
    Dirty(Option<DetectedGitSource>),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRepairItem {
    pub skill_id: String,
    pub name: String,
    pub status: String,
    pub reason: String,
    pub previous_source_type: String,
    pub previous_source_ref: Option<String>,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
    pub subpath: Option<String>,
    pub applied: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRepairReport {
    pub dry_run: bool,
    pub scanned: usize,
    pub repairable: usize,
    pub applied: usize,
    pub unresolved: usize,
    pub already_portable: usize,
    pub items: Vec<SourceRepairItem>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitOriginMigrationReport {
    pub dry_run: bool,
    pub applied: bool,
    pub skill_id: String,
    pub name: String,
    pub remote_url: String,
    pub subpath: Option<String>,
    pub verified_revision: String,
    pub previous_origin_kind: String,
    pub previous_update_strategy: String,
    pub previous_manual_override: bool,
    pub next_origin_kind: String,
    pub next_update_strategy: String,
    pub next_manual_override: bool,
    pub target_count: usize,
}

#[derive(Debug, Deserialize)]
struct SkillLock {
    #[serde(default)]
    skills: HashMap<String, SkillLockEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SkillLockEntry {
    source_type: String,
    source_url: String,
    skill_path: String,
}

#[derive(Debug, Deserialize)]
struct PluginManifest {
    repository: Option<String>,
    skills: Option<String>,
}

#[derive(Debug, Clone)]
struct ProvenanceCandidate {
    detected: DetectedGitSource,
    reason: String,
}

fn normalize_subpath(path: &Path) -> Option<String> {
    let value = path.to_string_lossy().replace('\\', "/");
    if value.is_empty() || value == "." {
        None
    } else {
        Some(value)
    }
}

pub fn detect_git_source(source_path: &Path) -> Result<Option<DetectedGitSource>> {
    let repository = match Repository::discover(source_path) {
        Ok(repository) => repository,
        Err(_) => return Ok(None),
    };
    let repo_root = repository
        .workdir()
        .map(Path::to_path_buf)
        .or_else(|| repository.path().parent().map(Path::to_path_buf))
        .context("无法确定 Git 工作树根目录")?;
    let remote_url = repository
        .find_remote("origin")
        .ok()
        .and_then(|remote| remote.url().map(str::to_string))
        .filter(|value| !value.trim().is_empty());
    let Some(remote_url) = remote_url else {
        return Ok(None);
    };
    let canonical_source = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf());
    let canonical_root = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.clone());
    let subpath = canonical_source
        .strip_prefix(&canonical_root)
        .ok()
        .and_then(normalize_subpath);
    let branch = repository
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(str::to_string));
    let revision = repository
        .head()
        .ok()
        .and_then(|head| head.target().map(|oid| oid.to_string()));
    Ok(Some(DetectedGitSource {
        repo_root,
        remote_url,
        branch,
        subpath,
        revision,
    }))
}

pub fn assess_local_git_source(source_path: &Path) -> Result<LocalGitSourceAssessment> {
    if Repository::discover(source_path).is_err() {
        return Ok(LocalGitSourceAssessment::NotGitWorktree);
    }
    let detected = detect_git_source(source_path)?;
    if source_subpath_is_committed_and_clean(source_path)? {
        Ok(LocalGitSourceAssessment::Clean(detected))
    } else {
        Ok(LocalGitSourceAssessment::Dirty(detected))
    }
}

fn source_subpath_is_committed_and_clean(source_path: &Path) -> Result<bool> {
    let repository = match Repository::discover(source_path) {
        Ok(repository) => repository,
        Err(_) => return Ok(false),
    };
    let Some(workdir) = repository.workdir() else {
        return Ok(false);
    };
    let canonical_source = source_path
        .canonicalize()
        .unwrap_or_else(|_| source_path.to_path_buf());
    let canonical_workdir = workdir
        .canonicalize()
        .unwrap_or_else(|_| workdir.to_path_buf());
    let Ok(relative) = canonical_source.strip_prefix(&canonical_workdir) else {
        return Ok(false);
    };
    let relative = relative.to_string_lossy().replace('\\', "/");
    let prefix = if relative.is_empty() {
        String::new()
    } else {
        format!("{}/", relative.trim_end_matches('/'))
    };
    let index = repository.index()?;
    let has_tracked_files = index.iter().any(|entry| {
        std::str::from_utf8(&entry.path).ok().is_some_and(|path| {
            if prefix.is_empty() {
                true
            } else {
                path.starts_with(&prefix)
            }
        })
    });
    if !has_tracked_files {
        return Ok(false);
    }

    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        // Ignored files still affect the copied Skill contents and are not
        // part of the repository revision that a remote install can reproduce.
        .include_ignored(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(false);
    if !relative.is_empty() {
        options.pathspec(relative.as_str());
    }
    let statuses = repository.statuses(Some(&mut options))?;
    Ok(statuses.is_empty())
}

pub(crate) fn apply_detected_source(
    store: &SkillStore,
    skill: &SkillRecord,
    detected: &DetectedGitSource,
) -> Result<()> {
    let mut patched = skill.clone();
    let same_source = source_matches(skill, detected);
    patched.source_type = "git".to_string();
    patched.source_ref = Some(detected.remote_url.clone());
    patched.source_subpath = detected.subpath.clone();
    patched.source_revision = detected
        .revision
        .clone()
        .or_else(|| same_source.then(|| skill.source_revision.clone()).flatten());
    store.upsert_skill(&patched)?;

    let origin = detected_origin_record(store, skill, detected)?;
    store.upsert_skill_origin(&origin)
}

fn parse_github_owner_repo(remote_url: &str) -> (Option<String>, Option<String>) {
    let normalized = remote_url
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .replace("git@github.com:", "github.com/");
    let Some((_, path)) = normalized.split_once("github.com/") else {
        return (None, None);
    };
    let mut parts = path.split('/');
    let owner = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let repo = parts
        .next()
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    (owner, repo)
}

fn matches_my_git_rules(
    owner: Option<&str>,
    repo: Option<&str>,
    rules: &super::app_config::OriginRules,
) -> bool {
    let owner_matches = owner.is_some_and(|value| {
        rules
            .my_git_owners
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(value))
    });
    let repo_matches = match (owner, repo) {
        (Some(owner), Some(repo)) => rules
            .my_git_repos
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(&format!("{owner}/{repo}"))),
        _ => false,
    };
    owner_matches || repo_matches
}

fn detected_origin_record(
    store: &SkillStore,
    skill: &SkillRecord,
    detected: &DetectedGitSource,
) -> Result<SkillOriginRecord> {
    let previous = store.get_skill_origin(&skill.id)?;
    let same_remote = previous.as_ref().is_some_and(|origin| {
        origin.remote_url.as_deref() == Some(detected.remote_url.as_str())
            && origin.subpath == detected.subpath
    });
    let branch = detected.branch.clone().or_else(|| {
        same_remote
            .then(|| previous.as_ref().and_then(|origin| origin.branch.clone()))
            .flatten()
    });
    if previous
        .as_ref()
        .is_some_and(|origin| origin.manual_override)
    {
        let mut origin = previous.expect("checked above");
        origin.remote_url = Some(detected.remote_url.clone());
        origin.branch = branch;
        origin.subpath = detected.subpath.clone();
        origin.updated_at = now_ms();
        return Ok(origin);
    }

    let rules = get_origin_rules_impl(store)?;
    let (owner, repo) = parse_github_owner_repo(&detected.remote_url);
    let is_mine = matches_my_git_rules(owner.as_deref(), repo.as_deref(), &rules);
    Ok(SkillOriginRecord {
        skill_id: skill.id.clone(),
        origin_kind: "git".to_string(),
        origin_role: if is_mine {
            "mine".to_string()
        } else {
            "repository".to_string()
        },
        provider: Some("git".to_string()),
        remote_url: Some(detected.remote_url.clone()),
        owner,
        repo,
        branch,
        subpath: detected.subpath.clone(),
        update_strategy: "git_pull".to_string(),
        publish_strategy: if is_mine {
            "git_push".to_string()
        } else {
            "none".to_string()
        },
        manual_override: false,
        reason: Some("repaired and classified from Git remote".to_string()),
        updated_at: now_ms(),
    })
}

fn detected_origin_needs_repair(
    store: &SkillStore,
    skill: &SkillRecord,
    detected: &DetectedGitSource,
) -> Result<bool> {
    let expected = detected_origin_record(store, skill, detected)?;
    let Some(current) = store.get_skill_origin(&skill.id)? else {
        return Ok(true);
    };
    Ok(current.origin_kind != expected.origin_kind
        || current.origin_role != expected.origin_role
        || current.remote_url != expected.remote_url
        || current.owner != expected.owner
        || current.repo != expected.repo
        || current.branch != expected.branch
        || current.subpath != expected.subpath
        || current.update_strategy != expected.update_strategy
        || current.publish_strategy != expected.publish_strategy
        || current.manual_override != expected.manual_override)
}

fn normalize_git_url(value: &str) -> String {
    value
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase()
}

fn source_matches(skill: &SkillRecord, detected: &DetectedGitSource) -> bool {
    skill.source_type == "git"
        && skill.source_ref.as_deref().is_some_and(|value| {
            normalize_git_url(value) == normalize_git_url(&detected.remote_url)
        })
        && skill.source_subpath == detected.subpath
}

fn is_remote_git_url(value: &str) -> bool {
    let value = value.trim();
    value.starts_with("https://")
        || value.starts_with("http://")
        || value.starts_with("ssh://")
        || value.starts_with("git@")
        || value.starts_with("file://")
}

fn recorded_git_candidate(skill: &SkillRecord) -> Option<ProvenanceCandidate> {
    if skill.source_type != "git" {
        return None;
    }
    let remote_url = skill.source_ref.as_deref()?.trim();
    if !is_remote_git_url(remote_url) {
        return None;
    }
    Some(ProvenanceCandidate {
        detected: DetectedGitSource {
            repo_root: PathBuf::new(),
            remote_url: remote_url.to_string(),
            branch: None,
            subpath: skill.source_subpath.clone(),
            revision: None,
        },
        reason: "从现有 Git 来源重新校验归属规则".to_string(),
    })
}

fn default_skill_lock_path() -> Option<PathBuf> {
    std::env::var_os("SKILLDO_SKILL_LOCK")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".agents/.skill-lock.json")))
}

fn load_skill_lock_candidates() -> HashMap<String, ProvenanceCandidate> {
    let Some(path) = default_skill_lock_path() else {
        return HashMap::new();
    };
    load_skill_lock_candidates_from(&path)
}

fn load_skill_lock_candidates_from(path: &Path) -> HashMap<String, ProvenanceCandidate> {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return HashMap::new();
    };
    let Ok(lock) = serde_json::from_str::<SkillLock>(&raw) else {
        return HashMap::new();
    };
    lock.skills
        .into_iter()
        .filter(|(_, entry)| entry.source_type == "github" && !entry.source_url.trim().is_empty())
        .map(|(name, entry)| {
            let subpath = Path::new(&entry.skill_path)
                .parent()
                .and_then(normalize_subpath);
            (
                name,
                ProvenanceCandidate {
                    detected: DetectedGitSource {
                        repo_root: PathBuf::new(),
                        remote_url: entry.source_url,
                        branch: None,
                        subpath,
                        revision: None,
                    },
                    reason: "由 .agents/.skill-lock.json 的 sourceUrl 与 skillPath 恢复"
                        .to_string(),
                },
            )
        })
        .collect()
}

fn frontmatter_name(path: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    raw.lines()
        .take(20)
        .find_map(|line| line.trim().strip_prefix("name:"))
        .map(|value| value.trim().trim_matches(['\'', '"']).to_string())
        .filter(|value| !value.is_empty())
}

fn default_plugin_cache_path() -> Option<PathBuf> {
    std::env::var_os("SKILLDO_PLUGIN_CACHE")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex/plugins/cache")))
}

fn load_plugin_candidates(managed_skills: &[SkillRecord]) -> HashMap<String, ProvenanceCandidate> {
    let Some(cache_root) = default_plugin_cache_path() else {
        return HashMap::new();
    };
    let mut candidates = HashMap::new();
    for entry in WalkDir::new(cache_root)
        .max_depth(6)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name() == "plugin.json")
    {
        let Ok(raw) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<PluginManifest>(&raw) else {
            continue;
        };
        let (Some(repository), Some(skills_dir)) = (manifest.repository, manifest.skills) else {
            continue;
        };
        if repository.trim().is_empty() {
            continue;
        }
        let Some(plugin_root) = entry.path().parent().and_then(Path::parent) else {
            continue;
        };
        let skills_rel = skills_dir.trim_start_matches("./").trim_end_matches('/');
        let skills_root = plugin_root.join(skills_rel);
        let Ok(children) = std::fs::read_dir(&skills_root) else {
            continue;
        };
        for child in children.filter_map(Result::ok) {
            let skill_md = child.path().join("SKILL.md");
            if !skill_md.is_file() {
                continue;
            }
            let folder = child.file_name().to_string_lossy().to_string();
            let name = frontmatter_name(&skill_md).unwrap_or_else(|| folder.clone());
            let Some(managed) = managed_skills.iter().find(|skill| skill.name == name) else {
                continue;
            };
            let managed_path = Path::new(&managed.central_path);
            if hash_dir(managed_path).ok() != hash_dir(&child.path()).ok() {
                continue;
            }
            candidates
                .entry(name)
                .or_insert_with(|| ProvenanceCandidate {
                    detected: DetectedGitSource {
                        repo_root: PathBuf::new(),
                        remote_url: repository.clone(),
                        branch: None,
                        subpath: normalize_subpath(&Path::new(skills_rel).join(folder)),
                        revision: None,
                    },
                    reason: "由已安装 Codex 插件的 plugin.json 与 skills 目录恢复".to_string(),
                });
        }
    }
    candidates
}

pub fn detect_recorded_source(name: &str, central_path: &Path) -> Option<DetectedGitSource> {
    if let Some(candidate) = load_skill_lock_candidates().remove(name) {
        return Some(candidate.detected);
    }
    let placeholder = SkillRecord {
        id: String::new(),
        name: name.to_string(),
        description: None,
        source_type: "local".to_string(),
        source_ref: None,
        source_subpath: None,
        source_revision: None,
        central_path: central_path.to_string_lossy().to_string(),
        content_hash: None,
        created_at: 0,
        updated_at: 0,
        last_sync_at: None,
        last_seen_at: 0,
        status: "ok".to_string(),
    };
    load_plugin_candidates(&[placeholder])
        .remove(name)
        .map(|candidate| candidate.detected)
}

pub fn repair_skill_sources(store: &SkillStore, apply: bool) -> Result<SourceRepairReport> {
    let skills = store.list_skills()?;
    let lock_candidates = load_skill_lock_candidates();
    let plugin_candidates = load_plugin_candidates(&skills);
    let mut items = Vec::new();
    for skill in &skills {
        if skill.source_type == "package" {
            continue;
        }
        let candidate = if let Some(candidate) = recorded_git_candidate(skill) {
            Some(candidate)
        } else {
            let metadata_candidate = lock_candidates
                .get(&skill.name)
                .or_else(|| plugin_candidates.get(&skill.name))
                .cloned();
            let path_candidate = if metadata_candidate.is_none() {
                skill
                    .source_ref
                    .as_deref()
                    .map(Path::new)
                    .map(detect_git_source)
                    .transpose()?
                    .flatten()
                    .map(|detected| ProvenanceCandidate {
                        detected,
                        reason: "sourceRef 位于带 origin 的 Git 工作树中".to_string(),
                    })
            } else {
                None
            };
            metadata_candidate.or(path_candidate)
        };
        // Local repository paths must be verified before accepting *any*
        // provenance candidate, including lockfile and plugin metadata. A
        // dirty or untracked project Skill remains a local source.
        let (candidate, local_source_uncommitted) =
            reject_uncommitted_local_source_candidate(skill, candidate)?;
        if let Some(candidate) = candidate {
            let origin_needs_repair =
                detected_origin_needs_repair(store, skill, &candidate.detected)?;
            if source_matches(skill, &candidate.detected) && !origin_needs_repair {
                continue;
            }
            if apply {
                apply_detected_source(store, skill, &candidate.detected)?;
            }
            items.push(SourceRepairItem {
                skill_id: skill.id.clone(),
                name: skill.name.clone(),
                status: "repairable".to_string(),
                reason: candidate.reason,
                previous_source_type: skill.source_type.clone(),
                previous_source_ref: skill.source_ref.clone(),
                remote_url: Some(candidate.detected.remote_url),
                branch: candidate.detected.branch,
                subpath: candidate.detected.subpath,
                applied: apply,
            });
        } else {
            let source_path = skill.source_ref.as_deref().map(Path::new);
            let reason = if skill.source_type == "git" {
                "Git Skill 缺少可验证的远程来源"
            } else if local_source_uncommitted {
                "sourceRef 子路径包含未跟踪或未提交内容，需先提交并核对上游后再改为 Git 来源"
            } else if source_path.is_some_and(Path::is_symlink) {
                "sourceRef 是指向中央副本的符号链接，副本不包含 .git"
            } else if source_path.is_some_and(Path::exists) {
                "sourceRef 不在带 origin 的 Git 工作树中"
            } else if source_path.is_none() {
                "本地 Skill 没有 sourceRef"
            } else {
                "sourceRef 不存在"
            };
            items.push(unresolved_item(skill, reason));
        }
    }
    let repairable = items
        .iter()
        .filter(|item| item.status == "repairable")
        .count();
    let unresolved = items
        .iter()
        .filter(|item| item.status == "unresolved")
        .count();
    Ok(SourceRepairReport {
        dry_run: !apply,
        scanned: skills.len(),
        repairable,
        applied: if apply { repairable } else { 0 },
        unresolved,
        already_portable: skills.len().saturating_sub(repairable + unresolved),
        items,
    })
}

fn reject_uncommitted_local_source_candidate(
    skill: &SkillRecord,
    candidate: Option<ProvenanceCandidate>,
) -> Result<(Option<ProvenanceCandidate>, bool)> {
    if skill.source_type != "local" {
        return Ok((candidate, false));
    }
    let Some(source_path) = skill.source_ref.as_deref().map(Path::new) else {
        return Ok((candidate, false));
    };
    match assess_local_git_source(source_path)? {
        LocalGitSourceAssessment::Dirty(_) => Ok((None, true)),
        LocalGitSourceAssessment::Clean(_) | LocalGitSourceAssessment::NotGitWorktree => {
            Ok((candidate, false))
        }
    }
}

pub fn repair_skill_source(
    store: &SkillStore,
    skill_name_or_id: &str,
    remote_url: &str,
    subpath: Option<&str>,
    apply: bool,
) -> Result<SourceRepairReport> {
    let skill = store
        .list_skills()?
        .into_iter()
        .find(|skill| {
            skill.id == skill_name_or_id || skill.name.eq_ignore_ascii_case(skill_name_or_id)
        })
        .with_context(|| format!("Skill 不存在: {skill_name_or_id}"))?;
    let remote_url = remote_url.trim();
    if remote_url.is_empty() {
        anyhow::bail!("Git remote URL 不能为空");
    }
    let detected = DetectedGitSource {
        repo_root: PathBuf::new(),
        remote_url: remote_url.to_string(),
        branch: None,
        subpath: subpath
            .map(str::trim)
            .filter(|value| !value.is_empty() && *value != ".")
            .map(str::to_string),
        revision: None,
    };
    let matches = source_matches(&skill, &detected);
    if !matches {
        verify_explicit_source(&skill, &detected)?;
    }
    let origin_needs_repair = detected_origin_needs_repair(store, &skill, &detected)?;
    let needs_repair = !matches || origin_needs_repair;
    if apply && needs_repair {
        apply_detected_source(store, &skill, &detected)?;
    }
    let items = if !needs_repair {
        Vec::new()
    } else {
        vec![SourceRepairItem {
            skill_id: skill.id.clone(),
            name: skill.name.clone(),
            status: "repairable".to_string(),
            reason: "由用户确认的显式 Git remote 与子路径恢复".to_string(),
            previous_source_type: skill.source_type.clone(),
            previous_source_ref: skill.source_ref.clone(),
            remote_url: Some(detected.remote_url),
            branch: None,
            subpath: detected.subpath,
            applied: apply,
        }]
    };
    Ok(SourceRepairReport {
        dry_run: !apply,
        scanned: 1,
        repairable: items.len(),
        applied: usize::from(apply && needs_repair),
        unresolved: 0,
        already_portable: usize::from(matches),
        items,
    })
}

/// Migrate a Git-backed Skill whose update behavior was manually overridden
/// to local-copy mode. The remote is cloned and its selected Skill directory
/// verified before any database write. Only the origin row is changed.
pub fn migrate_git_origin_override(
    store: &SkillStore,
    skill_name_or_id: &str,
    remote_url: &str,
    subpath: Option<&str>,
    apply: bool,
) -> Result<GitOriginMigrationReport> {
    let skill = store
        .list_skills()?
        .into_iter()
        .find(|skill| {
            skill.id == skill_name_or_id || skill.name.eq_ignore_ascii_case(skill_name_or_id)
        })
        .with_context(|| format!("Skill 不存在: {skill_name_or_id}"))?;
    if skill.source_type != "git" {
        anyhow::bail!(
            "仅支持迁移 Git 来源 Skill，当前来源类型为 {}",
            skill.source_type
        );
    }

    let remote_url = remote_url.trim();
    if !is_remote_git_url(remote_url) {
        anyhow::bail!("必须提供已登记的 Git 远端 URL");
    }
    if remote_url.starts_with("https://") || remote_url.starts_with("http://") {
        let authority = remote_url
            .split_once("://")
            .map(|(_, rest)| rest.split('/').next().unwrap_or_default())
            .unwrap_or_default();
        if authority.contains('@') {
            anyhow::bail!("Git URL 不得嵌入用户名或凭据");
        }
    }
    let clean_subpath = normalize_requested_subpath(subpath)?;
    if skill.source_ref.as_deref().map_or(true, |saved| {
        normalize_git_url(saved) != normalize_git_url(remote_url)
    }) {
        anyhow::bail!("指定远端与 Skill 已登记的 Git 来源不一致");
    }
    if skill.source_subpath != clean_subpath {
        anyhow::bail!("指定子目录与 Skill 已登记的来源子目录不一致");
    }

    let previous = store
        .get_skill_origin(&skill.id)?
        .with_context(|| format!("Skill '{}' 缺少来源元数据", skill.name))?;
    if previous.origin_kind != "local"
        || !previous.manual_override
        || previous.update_strategy != "local_copy"
    {
        anyhow::bail!(
            "来源状态不是可迁移的 local_copy 手动覆盖（kind={}, strategy={}, override={}）",
            previous.origin_kind,
            previous.update_strategy,
            previous.manual_override
        );
    }
    if previous
        .remote_url
        .as_deref()
        .is_some_and(|saved| normalize_git_url(saved) != normalize_git_url(remote_url))
        || previous.subpath != clean_subpath
    {
        anyhow::bail!("指定远端或子目录与现存来源元数据不一致");
    }

    let verified_revision =
        verify_migration_source(remote_url, clean_subpath.as_deref(), &skill.name)?;
    let (remote_owner, remote_repo) = parse_github_owner_repo(remote_url);
    let mut next = previous.clone();
    next.origin_kind = "git".to_string();
    next.provider = Some("git".to_string());
    next.remote_url = Some(remote_url.to_string());
    next.owner = remote_owner.or(next.owner);
    next.repo = remote_repo.or(next.repo);
    next.subpath = clean_subpath.clone();
    next.update_strategy = "git_pull".to_string();
    next.manual_override = false;
    next.reason =
        Some("Git remote and Skill subpath verified; migrated to Git updates".to_string());
    next.updated_at = now_ms();

    let target_count = store.list_skill_targets(&skill.id)?.len();
    if apply {
        store.migrate_skill_origin_to_git_tracked(&skill, &previous, &next)?;
    }

    Ok(GitOriginMigrationReport {
        dry_run: !apply,
        applied: apply,
        skill_id: skill.id,
        name: skill.name,
        remote_url: remote_url.to_string(),
        subpath: clean_subpath,
        verified_revision,
        previous_origin_kind: previous.origin_kind,
        previous_update_strategy: previous.update_strategy,
        previous_manual_override: previous.manual_override,
        next_origin_kind: next.origin_kind,
        next_update_strategy: next.update_strategy,
        next_manual_override: next.manual_override,
        target_count,
    })
}

fn normalize_requested_subpath(subpath: Option<&str>) -> Result<Option<String>> {
    let Some(value) = subpath
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != ".")
    else {
        return Ok(None);
    };
    let normalized = value.replace('\\', "/");
    let path = Path::new(&normalized);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        anyhow::bail!("Skill 子目录必须是仓库内的相对路径");
    }
    Ok(Some(normalized.trim_matches('/').to_string()))
}

fn verify_migration_source(
    remote_url: &str,
    subpath: Option<&str>,
    skill_name: &str,
) -> Result<String> {
    let checkout =
        std::env::temp_dir().join(format!("skilldo-origin-migrate-{}", uuid::Uuid::new_v4()));
    let result = (|| -> Result<String> {
        let revision = clone_or_pull(remote_url, &checkout, None, None)
            .with_context(|| format!("无法验证 Git 远端: {remote_url}"))?;
        let repository = Repository::open(&checkout).context("打开验证用 Git checkout 失败")?;
        let actual_remote = repository
            .find_remote("origin")?
            .url()
            .context("验证用 checkout 缺少 origin URL")?
            .to_string();
        if normalize_git_url(&actual_remote) != normalize_git_url(remote_url) {
            anyhow::bail!("clone 后的 origin 与请求的 Git 远端不一致");
        }
        let candidate = subpath
            .map(|path| checkout.join(path))
            .unwrap_or_else(|| checkout.clone());
        let canonical_checkout = checkout.canonicalize()?;
        let canonical_candidate = candidate
            .canonicalize()
            .context("Git 远端中找不到指定 Skill 子目录")?;
        if !canonical_candidate.starts_with(&canonical_checkout) || !canonical_candidate.is_dir() {
            anyhow::bail!("指定 Skill 子目录不在 Git 仓库内");
        }
        let skill_md = canonical_candidate.join("SKILL.md");
        if skill_md.is_symlink() || !skill_md.is_file() {
            anyhow::bail!("Git 远端指定子目录必须包含普通文件 SKILL.md");
        }
        let matches = canonical_candidate
            .file_name()
            .is_some_and(|folder| folder.to_string_lossy().eq_ignore_ascii_case(skill_name))
            || frontmatter_name(&skill_md)
                .is_some_and(|name| name.eq_ignore_ascii_case(skill_name));
        if !matches {
            anyhow::bail!("Git 远端子目录中的 Skill 名称与 '{}' 不匹配", skill_name);
        }
        Ok(revision)
    })();
    let _ = std::fs::remove_dir_all(&checkout);
    result
}

fn verify_explicit_source(skill: &SkillRecord, detected: &DetectedGitSource) -> Result<()> {
    let checkout =
        std::env::temp_dir().join(format!("skilldo-source-verify-{}", uuid::Uuid::new_v4()));
    let clone_result = if let Some(subpath) = detected.subpath.as_deref() {
        clone_or_pull_sparse(&detected.remote_url, &checkout, None, subpath, None)
    } else {
        clone_or_pull(&detected.remote_url, &checkout, None, None)
    }
    .with_context(|| format!("无法克隆待验证来源: {}", detected.remote_url));
    match clone_result {
        Ok(_) => {}
        Err(error) => {
            let _ = std::fs::remove_dir_all(&checkout);
            return Err(error);
        }
    }
    let candidate = detected
        .subpath
        .as_deref()
        .map(|subpath| checkout.join(subpath))
        .unwrap_or_else(|| checkout.clone());
    let skill_md = candidate.join("SKILL.md");
    let valid = skill_md.is_file()
        && (candidate
            .file_name()
            .is_some_and(|folder| folder.to_string_lossy().eq_ignore_ascii_case(&skill.name))
            || frontmatter_name(&skill_md)
                .is_some_and(|name| name.eq_ignore_ascii_case(&skill.name)));
    let _ = std::fs::remove_dir_all(&checkout);
    if !valid {
        anyhow::bail!(
            "SOURCE_MISMATCH|远端子路径不存在 SKILL.md，或名称与 {} 不匹配",
            skill.name
        );
    }
    Ok(())
}

fn unresolved_item(skill: &SkillRecord, reason: &str) -> SourceRepairItem {
    SourceRepairItem {
        skill_id: skill.id.clone(),
        name: skill.name.clone(),
        status: "unresolved".to_string(),
        reason: reason.to_string(),
        previous_source_type: skill.source_type.clone(),
        previous_source_ref: skill.source_ref.clone(),
        remote_url: None,
        branch: None,
        subpath: None,
        applied: false,
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill_store::{SkillOriginRecord, SkillStore, SkillTargetRecord};

    fn git_skill_fixture() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let repository = Repository::init(directory.path()).unwrap();
        repository
            .remote("origin", "https://github.com/example/skills.git")
            .unwrap();
        let skill = directory.path().join("skills/demo");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(skill.join("SKILL.md"), "# Demo").unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("skills/demo/SKILL.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("SkillDo", "test@example.com").unwrap();
        repository
            .commit(Some("HEAD"), &signature, &signature, "init", &tree, &[])
            .unwrap();
        (directory, skill)
    }

    fn bare_remote_fixture() -> (tempfile::TempDir, String, String) {
        let directory = tempfile::tempdir().unwrap();
        let bare_path = directory.path().join("remote.git");
        let bare = Repository::init_bare(&bare_path).unwrap();
        let work_path = directory.path().join("work");
        let work = Repository::init(&work_path).unwrap();
        work.set_head("refs/heads/main").unwrap();
        let skill = work_path.join("skills/demo");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: demo\ndescription: test\n---\n# Demo\n",
        )
        .unwrap();
        let mut index = work.index().unwrap();
        index.add_path(Path::new("skills/demo/SKILL.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = work.find_tree(tree_id).unwrap();
        let signature = git2::Signature::now("SkillDo", "test@example.com").unwrap();
        work.commit(
            Some("refs/heads/main"),
            &signature,
            &signature,
            "add demo skill",
            &tree,
            &[],
        )
        .unwrap();
        work.remote("origin", bare_path.to_str().unwrap()).unwrap();
        let mut remote = work.find_remote("origin").unwrap();
        remote
            .push(&["refs/heads/main:refs/heads/main"], None)
            .unwrap();
        bare.set_head("refs/heads/main").unwrap();
        let url = format!("file://{}", bare_path.display());
        let revision = work.head().unwrap().target().unwrap().to_string();
        (directory, url, revision)
    }

    fn seed_overridden_git_skill(
        store: &SkillStore,
        remote_url: &str,
    ) -> (SkillRecord, SkillOriginRecord, SkillTargetRecord) {
        let skill = SkillRecord {
            id: "stable-skill-id".to_string(),
            name: "demo".to_string(),
            description: Some("keep this description".to_string()),
            source_type: "git".to_string(),
            source_ref: Some(remote_url.to_string()),
            source_subpath: Some("skills/demo".to_string()),
            source_revision: Some("old-revision".to_string()),
            central_path: "/central/demo".to_string(),
            content_hash: Some("stable-hash".to_string()),
            created_at: 10,
            updated_at: 20,
            last_sync_at: Some(25),
            last_seen_at: 30,
            status: "ok".to_string(),
        };
        let origin = SkillOriginRecord {
            skill_id: skill.id.clone(),
            origin_kind: "local".to_string(),
            origin_role: "mine".to_string(),
            provider: Some("local".to_string()),
            remote_url: Some(remote_url.to_string()),
            owner: Some("example".to_string()),
            repo: Some("demo".to_string()),
            branch: Some("main".to_string()),
            subpath: skill.source_subpath.clone(),
            update_strategy: "local_copy".to_string(),
            publish_strategy: "none".to_string(),
            manual_override: true,
            reason: Some("explicit local source".to_string()),
            updated_at: 35,
        };
        let target = SkillTargetRecord {
            id: "stable-target-id".to_string(),
            skill_id: skill.id.clone(),
            tool: "codex".to_string(),
            scope: "global".to_string(),
            project_path: None,
            target_path: "/agent/skills/demo".to_string(),
            mode: "symlink".to_string(),
            status: "ok".to_string(),
            last_error: None,
            synced_at: Some(40),
        };
        store.upsert_skill(&skill).unwrap();
        store.upsert_skill_origin(&origin).unwrap();
        store.upsert_skill_target(&target).unwrap();
        (skill, origin, target)
    }

    fn assert_skill_fields_equal(actual: &SkillRecord, expected: &SkillRecord) {
        assert_eq!(actual.id, expected.id);
        assert_eq!(actual.name, expected.name);
        assert_eq!(actual.description, expected.description);
        assert_eq!(actual.source_type, expected.source_type);
        assert_eq!(actual.source_ref, expected.source_ref);
        assert_eq!(actual.source_subpath, expected.source_subpath);
        assert_eq!(actual.source_revision, expected.source_revision);
        assert_eq!(actual.central_path, expected.central_path);
        assert_eq!(actual.content_hash, expected.content_hash);
        assert_eq!(actual.created_at, expected.created_at);
        assert_eq!(actual.updated_at, expected.updated_at);
        assert_eq!(actual.last_sync_at, expected.last_sync_at);
        assert_eq!(actual.last_seen_at, expected.last_seen_at);
        assert_eq!(actual.status, expected.status);
    }

    #[test]
    fn git_origin_migration_dry_run_and_apply_preserve_skill_and_targets() {
        let (remote_dir, remote_url, revision) = bare_remote_fixture();
        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        let (original_skill, original_origin, original_target) =
            seed_overridden_git_skill(&store, &remote_url);

        let preview =
            migrate_git_origin_override(&store, "demo", &remote_url, Some("skills/demo"), false)
                .unwrap();
        assert!(preview.dry_run);
        assert!(!preview.applied);
        assert_eq!(preview.verified_revision, revision);
        assert_eq!(preview.target_count, 1);
        assert_skill_fields_equal(
            &store.get_skill_by_id(&original_skill.id).unwrap().unwrap(),
            &original_skill,
        );
        assert_eq!(
            store.get_skill_origin(&original_skill.id).unwrap(),
            Some(original_origin.clone())
        );
        assert_eq!(
            store.list_skill_targets(&original_skill.id).unwrap()[0].id,
            original_target.id
        );

        let applied = migrate_git_origin_override(
            &store,
            &original_skill.id,
            &remote_url,
            Some("skills/demo"),
            true,
        )
        .unwrap();
        assert!(applied.applied);
        assert_skill_fields_equal(
            &store.get_skill_by_id(&original_skill.id).unwrap().unwrap(),
            &original_skill,
        );
        assert_eq!(
            store.list_skill_targets(&original_skill.id).unwrap(),
            vec![original_target]
        );
        let migrated = store.get_skill_origin(&original_skill.id).unwrap().unwrap();
        assert_eq!(migrated.skill_id, original_origin.skill_id);
        assert_eq!(migrated.origin_kind, "git");
        assert_eq!(migrated.provider.as_deref(), Some("git"));
        assert_eq!(migrated.remote_url.as_deref(), Some(remote_url.as_str()));
        assert_eq!(migrated.update_strategy, "git_pull");
        assert!(!migrated.manual_override);
        assert_eq!(migrated.origin_role, original_origin.origin_role);
        assert_eq!(migrated.publish_strategy, original_origin.publish_strategy);
        assert_eq!(migrated.owner, original_origin.owner);
        assert_eq!(migrated.repo, original_origin.repo);
        assert_eq!(migrated.branch, original_origin.branch);
        assert_eq!(migrated.subpath, original_origin.subpath);
        drop(remote_dir);
    }

    #[test]
    fn git_origin_migration_remote_mismatch_does_not_change_database() {
        let (_remote_dir, remote_url, _) = bare_remote_fixture();
        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        let (skill, origin, target) = seed_overridden_git_skill(&store, &remote_url);
        let embedded_credentials = migrate_git_origin_override(
            &store,
            "demo",
            "https://user:secret@github.com/example/repo.git",
            Some("skills/demo"),
            true,
        )
        .unwrap_err();
        assert!(embedded_credentials.to_string().contains("凭据"));
        let error = migrate_git_origin_override(
            &store,
            "demo",
            "https://github.com/other/repo.git",
            Some("skills/demo"),
            true,
        )
        .unwrap_err();
        assert!(error.to_string().contains("不一致"));
        assert_skill_fields_equal(&store.get_skill_by_id(&skill.id).unwrap().unwrap(), &skill);
        assert_eq!(store.get_skill_origin(&skill.id).unwrap(), Some(origin));
        assert_eq!(
            store.list_skill_targets(&skill.id).unwrap()[0].id,
            target.id
        );
    }

    #[test]
    fn git_origin_migration_rejects_subpath_traversal() {
        assert!(normalize_requested_subpath(Some("../skills/demo")).is_err());
        assert!(normalize_requested_subpath(Some("skills/../../outside")).is_err());
    }

    #[test]
    fn git_origin_migration_database_failure_rolls_back_origin_row() {
        let (_remote_dir, remote_url, _) = bare_remote_fixture();
        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        let (skill, origin, target) = seed_overridden_git_skill(&store, &remote_url);
        let connection = rusqlite::Connection::open(store.db_path()).unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER fail_origin_migration BEFORE UPDATE ON skill_origins
                 BEGIN SELECT RAISE(ABORT, 'test migration failure'); END;",
            )
            .unwrap();
        drop(connection);

        assert!(migrate_git_origin_override(
            &store,
            "demo",
            &remote_url,
            Some("skills/demo"),
            true
        )
        .is_err());
        assert_skill_fields_equal(&store.get_skill_by_id(&skill.id).unwrap().unwrap(), &skill);
        assert_eq!(store.get_skill_origin(&skill.id).unwrap(), Some(origin));
        assert_eq!(
            store.list_skill_targets(&skill.id).unwrap()[0].id,
            target.id
        );
    }

    #[test]
    fn detects_repo_remote_branch_revision_and_subpath() {
        let (_directory, skill) = git_skill_fixture();
        let detected = detect_git_source(&skill).unwrap().unwrap();
        assert_eq!(detected.remote_url, "https://github.com/example/skills.git");
        assert_eq!(detected.subpath.as_deref(), Some("skills/demo"));
        assert!(detected.branch.is_some());
        assert!(detected.revision.is_some());
    }

    #[test]
    fn repair_promotes_local_record_without_changing_identity() {
        let (_directory, skill_path) = git_skill_fixture();
        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        store
            .upsert_skill(&SkillRecord {
                id: "same-id".to_string(),
                name: "demo".to_string(),
                description: None,
                source_type: "local".to_string(),
                source_ref: Some(skill_path.to_string_lossy().to_string()),
                source_subpath: None,
                source_revision: None,
                central_path: db_dir
                    .path()
                    .join("central/demo")
                    .to_string_lossy()
                    .to_string(),
                content_hash: None,
                created_at: 1,
                updated_at: 2,
                last_sync_at: None,
                last_seen_at: 3,
                status: "ok".to_string(),
            })
            .unwrap();

        let dry_run = repair_skill_sources(&store, false).unwrap();
        assert_eq!(dry_run.repairable, 1);
        assert_eq!(
            store
                .get_skill_by_id("same-id")
                .unwrap()
                .unwrap()
                .source_type,
            "local"
        );

        let applied = repair_skill_sources(&store, true).unwrap();
        assert_eq!(applied.applied, 1);
        let repaired = store.get_skill_by_id("same-id").unwrap().unwrap();
        assert_eq!(repaired.id, "same-id");
        assert_eq!(repaired.source_type, "git");
        assert_eq!(repaired.source_subpath.as_deref(), Some("skills/demo"));
        assert_eq!(
            repaired.source_ref.as_deref(),
            Some("https://github.com/example/skills.git")
        );
    }

    #[test]
    fn repair_keeps_uncommitted_local_skill_as_local_source() {
        let (_directory, tracked_skill) = git_skill_fixture();
        let untracked_skill = tracked_skill.parent().unwrap().join("new-skill");
        std::fs::create_dir_all(&untracked_skill).unwrap();
        std::fs::write(untracked_skill.join("SKILL.md"), "# New Skill").unwrap();
        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        store
            .upsert_skill(&SkillRecord {
                id: "new-skill-id".to_string(),
                name: "new-skill".to_string(),
                description: None,
                source_type: "local".to_string(),
                source_ref: Some(untracked_skill.to_string_lossy().to_string()),
                source_subpath: None,
                source_revision: None,
                central_path: db_dir
                    .path()
                    .join("central/new-skill")
                    .to_string_lossy()
                    .to_string(),
                content_hash: None,
                created_at: 1,
                updated_at: 1,
                last_sync_at: None,
                last_seen_at: 1,
                status: "ok".to_string(),
            })
            .unwrap();

        let report = repair_skill_sources(&store, true).unwrap();
        assert_eq!(report.applied, 0);
        assert_eq!(report.unresolved, 1);
        assert!(report.items[0].reason.contains("未跟踪或未提交"));
        assert_eq!(
            store
                .get_skill_by_id("new-skill-id")
                .unwrap()
                .unwrap()
                .source_type,
            "local"
        );
    }

    #[test]
    fn repair_does_not_promote_skill_with_ignored_subtree_content() {
        let (directory, skill_path) = git_skill_fixture();
        let repository = Repository::open(directory.path()).unwrap();
        std::fs::write(
            directory.path().join(".gitignore"),
            "skills/demo/generated.txt\n",
        )
        .unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new(".gitignore")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let parent = repository.head().unwrap().peel_to_commit().unwrap();
        let signature = git2::Signature::now("SkillDo", "test@example.com").unwrap();
        repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "ignore generated Skill artifact",
                &tree,
                &[&parent],
            )
            .unwrap();
        std::fs::write(skill_path.join("generated.txt"), "local generated content").unwrap();

        assert!(matches!(
            assess_local_git_source(&skill_path).unwrap(),
            LocalGitSourceAssessment::Dirty(_)
        ));

        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        store
            .upsert_skill(&SkillRecord {
                id: "ignored-skill".to_string(),
                name: "demo".to_string(),
                description: None,
                source_type: "local".to_string(),
                source_ref: Some(skill_path.to_string_lossy().to_string()),
                source_subpath: None,
                source_revision: None,
                central_path: db_dir
                    .path()
                    .join("central/demo")
                    .to_string_lossy()
                    .to_string(),
                content_hash: None,
                created_at: 1,
                updated_at: 1,
                last_sync_at: None,
                last_seen_at: 1,
                status: "ok".to_string(),
            })
            .unwrap();

        let report = repair_skill_sources(&store, true).unwrap();
        assert_eq!(report.applied, 0);
        assert_eq!(report.unresolved, 1);
        assert!(report.items[0].reason.contains("未跟踪或未提交"));
        assert_eq!(
            store
                .get_skill_by_id("ignored-skill")
                .unwrap()
                .unwrap()
                .source_type,
            "local"
        );
    }

    #[test]
    fn repair_rejects_lock_or_plugin_candidate_for_untracked_local_source() {
        let (_directory, tracked_skill) = git_skill_fixture();
        let untracked_skill = tracked_skill.parent().unwrap().join("new-skill");
        std::fs::create_dir_all(&untracked_skill).unwrap();
        std::fs::write(untracked_skill.join("SKILL.md"), "# New Skill").unwrap();
        let skill = SkillRecord {
            id: "new-skill-id".to_string(),
            name: "new-skill".to_string(),
            description: None,
            source_type: "local".to_string(),
            source_ref: Some(untracked_skill.to_string_lossy().to_string()),
            source_subpath: None,
            source_revision: None,
            central_path: "/tmp/central/new-skill".to_string(),
            content_hash: None,
            created_at: 1,
            updated_at: 1,
            last_sync_at: None,
            last_seen_at: 1,
            status: "ok".to_string(),
        };
        let metadata_candidate = ProvenanceCandidate {
            detected: DetectedGitSource {
                repo_root: _directory.path().to_path_buf(),
                remote_url: "https://github.com/example/skills.git".to_string(),
                branch: Some("main".to_string()),
                subpath: Some("skills/new-skill".to_string()),
                revision: None,
            },
            reason: "synthetic lockfile/plugin provenance".to_string(),
        };

        let (candidate, uncommitted) =
            reject_uncommitted_local_source_candidate(&skill, Some(metadata_candidate)).unwrap();

        assert!(uncommitted);
        assert!(candidate.is_none());
    }

    #[test]
    fn repair_reclassifies_existing_git_record_when_owner_rules_change() {
        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        store
            .set_setting(
                super::super::app_config::ORIGIN_RULES_KEY,
                r#"{"myGitOwners":["yancongya"]}"#,
            )
            .unwrap();
        store
            .upsert_skill(&SkillRecord {
                id: "cli-anything".to_string(),
                name: "cli-anything".to_string(),
                description: None,
                source_type: "git".to_string(),
                source_ref: Some("https://github.com/yancongya/cli-anything.git".to_string()),
                source_subpath: None,
                source_revision: Some("kept-revision".to_string()),
                central_path: db_dir
                    .path()
                    .join("central/cli-anything")
                    .to_string_lossy()
                    .to_string(),
                content_hash: None,
                created_at: 1,
                updated_at: 1,
                last_sync_at: None,
                last_seen_at: 1,
                status: "ok".to_string(),
            })
            .unwrap();
        store
            .upsert_skill_origin(&SkillOriginRecord {
                skill_id: "cli-anything".to_string(),
                origin_kind: "git".to_string(),
                origin_role: "mine".to_string(),
                provider: Some("git".to_string()),
                remote_url: Some("https://github.com/yancongya/cli-anything.git".to_string()),
                owner: None,
                repo: None,
                branch: Some("main".to_string()),
                subpath: None,
                update_strategy: "git_pull".to_string(),
                publish_strategy: "none".to_string(),
                manual_override: false,
                reason: None,
                updated_at: 1,
            })
            .unwrap();

        let report = repair_skill_sources(&store, true).unwrap();
        assert_eq!(report.repairable, 1);
        let repaired = store.get_skill_by_id("cli-anything").unwrap().unwrap();
        assert_eq!(repaired.source_revision.as_deref(), Some("kept-revision"));
        let origin = store.get_skill_origin("cli-anything").unwrap().unwrap();
        assert_eq!(origin.origin_role, "mine");
        assert_eq!(origin.owner.as_deref(), Some("yancongya"));
        assert_eq!(origin.repo.as_deref(), Some("cli-anything"));
        assert_eq!(origin.publish_strategy, "git_push");
    }

    #[test]
    fn reads_standard_skill_lock_provenance() {
        let directory = tempfile::tempdir().unwrap();
        let lock_path = directory.path().join(".skill-lock.json");
        std::fs::write(
            &lock_path,
            r#"{"version":3,"skills":{"demo":{"sourceType":"github","sourceUrl":"https://github.com/example/repo.git","skillPath":"skills/demo/SKILL.md"},"local":{"sourceType":"local","sourceUrl":"","skillPath":"skills/local/SKILL.md"}}}"#,
        )
        .unwrap();
        let candidates = load_skill_lock_candidates_from(&lock_path);
        let demo = candidates.get("demo").unwrap();
        assert_eq!(
            demo.detected.remote_url,
            "https://github.com/example/repo.git"
        );
        assert_eq!(demo.detected.subpath.as_deref(), Some("skills/demo"));
        assert!(!candidates.contains_key("local"));
    }

    #[test]
    fn explicit_repair_validates_remote_skill_identity() {
        let (repo_dir, skill_path) = git_skill_fixture();
        let db_dir = tempfile::tempdir().unwrap();
        let store = SkillStore::new(db_dir.path().join("test.db"));
        store.ensure_schema().unwrap();
        store
            .upsert_skill(&SkillRecord {
                id: "demo-id".to_string(),
                name: "demo".to_string(),
                description: None,
                source_type: "local".to_string(),
                source_ref: Some(skill_path.to_string_lossy().to_string()),
                source_subpath: None,
                source_revision: None,
                central_path: db_dir.path().join("demo").to_string_lossy().to_string(),
                content_hash: None,
                created_at: 1,
                updated_at: 1,
                last_sync_at: None,
                last_seen_at: 1,
                status: "ok".to_string(),
            })
            .unwrap();

        let report = repair_skill_source(
            &store,
            "demo",
            repo_dir.path().to_string_lossy().as_ref(),
            Some("skills/demo"),
            true,
        )
        .unwrap();
        assert_eq!(report.applied, 1);
        assert_eq!(
            store
                .get_skill_by_id("demo-id")
                .unwrap()
                .unwrap()
                .source_type,
            "git"
        );

        let error = repair_skill_source(
            &store,
            "demo",
            repo_dir.path().to_string_lossy().as_ref(),
            Some("skills/missing"),
            false,
        )
        .unwrap_err();
        assert!(error.to_string().contains("SOURCE_MISMATCH"));
    }
}
