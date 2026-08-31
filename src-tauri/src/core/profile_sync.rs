//! Portable desired-state profiles for cross-device SkillDo synchronization.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::app_config::{load_app_config, save_app_config_impl, AppConfig, OriginRules};
use super::content_hash::hash_dir;
use super::explore_sources::ExploreSourceConfig;
use super::installer::{
    delete_skill_cli, install_git_skill_cli, install_package_skill_cli, sync_skill_target_cli,
    unsync_skill_cli, update_managed_skill_from_source_cli,
};
use super::skill_store::{SkillOriginRecord, SkillRecord, SkillStore};
use super::webdav::{prepare_remote_dir, profile_remote_path, WebDavClient};

pub const PROFILE_VERSION: u32 = 2;
const PROFILE_ID_KEY: &str = "profile_sync_profile_id_v1";
const PROFILE_DEVICE_ID_KEY: &str = "profile_sync_device_id_v1";
const PROFILE_BASE_KEY: &str = "profile_sync_base_v1";
const PROFILE_ETAG_KEY: &str = "profile_sync_etag_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PortableProfileConfig {
    pub language: Option<String>,
    pub git_cache_cleanup_days: i64,
    pub git_cache_ttl_secs: i64,
    pub origin_rules: OriginRules,
    pub explore_sources: Vec<ExploreSourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileOriginOverride {
    pub origin_kind: String,
    pub origin_role: String,
    pub provider: Option<String>,
    pub remote_url: Option<String>,
    pub owner: Option<String>,
    pub repo: Option<String>,
    pub branch: Option<String>,
    pub subpath: Option<String>,
    pub update_strategy: String,
    pub publish_strategy: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSkill {
    pub key: String,
    pub name: String,
    pub source_type: String,
    pub source_ref: String,
    pub source_subpath: Option<String>,
    #[serde(default = "default_update_policy")]
    pub update_policy: String,
    #[serde(default)]
    pub pinned_revision: Option<String>,
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub origin_override: Option<ProfileOriginOverride>,
    #[serde(default)]
    pub deleted_at: Option<i64>,
    pub updated_at: i64,
    pub updated_by: String,
}

fn default_update_policy() -> String {
    "latest".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDocument {
    pub profile_version: u32,
    pub profile_id: String,
    pub generation: u64,
    pub updated_at: i64,
    pub updated_by: String,
    pub config: PortableProfileConfig,
    pub config_updated_at: i64,
    pub config_updated_by: String,
    pub skills: Vec<ProfileSkill>,
}

impl ProfileDocument {
    pub fn validate(&self) -> Result<()> {
        if self.profile_version > PROFILE_VERSION {
            anyhow::bail!(
                "Profile 由更新版本 (v{}) 创建，当前仅支持 v{}",
                self.profile_version,
                PROFILE_VERSION
            );
        }
        if self.profile_id.trim().is_empty() {
            anyhow::bail!("Profile 缺少 profileId");
        }
        let mut keys = BTreeSet::new();
        for skill in &self.skills {
            if skill.key.trim().is_empty()
                || skill.name.trim().is_empty()
                || skill.source_ref.trim().is_empty()
            {
                anyhow::bail!("Profile Skill 缺少 key、name 或 sourceRef");
            }
            if !keys.insert(skill.key.clone()) {
                anyhow::bail!("Profile 包含重复 Skill key: {}", skill.key);
            }
            if !matches!(skill.update_policy.as_str(), "latest" | "pinned" | "manual") {
                anyhow::bail!(
                    "Profile Skill {} 使用不支持的更新策略: {}",
                    skill.name,
                    skill.update_policy
                );
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProfileConflict {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSyncReport {
    pub profile_id: String,
    pub device_id: String,
    pub remote_found: bool,
    pub uploaded: bool,
    pub changed: bool,
    pub conflicts: Vec<ProfileConflict>,
    pub conflict_strategy: String,
    pub conflicts_resolved: bool,
    pub installed: Vec<String>,
    pub updated: Vec<String>,
    pub deleted: Vec<String>,
    pub pending_deletions: Vec<String>,
    pub synced_targets: Vec<String>,
    pub removed_targets: Vec<String>,
    pub skipped_local: Vec<String>,
    pub failures: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictStrategy {
    Abort,
    Local,
    Remote,
}

impl ConflictStrategy {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "abort" => Ok(Self::Abort),
            "local" => Ok(Self::Local),
            "remote" => Ok(Self::Remote),
            _ => anyhow::bail!("冲突策略必须是 abort、local 或 remote"),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Abort => "abort",
            Self::Local => "local",
            Self::Remote => "remote",
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn normalize_source_ref(value: &str) -> String {
    let normalized = value
        .trim()
        .to_lowercase()
        .trim_start_matches("git+")
        .trim_start_matches("ssh://")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_end_matches(".git")
        .replace('\\', "/");
    normalized.replace("git@github.com:", "github.com/")
}

pub fn stable_skill_key(record: &SkillRecord) -> Option<String> {
    if record.source_type != "git" && record.source_type != "package" {
        return None;
    }
    let source_ref = record.source_ref.as_deref()?.trim();
    if source_ref.is_empty() {
        return None;
    }
    let identity = format!(
        "{}\n{}\n{}\n{}",
        record.source_type.to_lowercase(),
        normalize_source_ref(source_ref),
        record.source_subpath.as_deref().unwrap_or_default(),
        record.name.trim().to_lowercase()
    );
    let digest = Sha256::digest(identity.as_bytes());
    Some(format!("v1:{}:{}", record.source_type, hex::encode(digest)))
}

fn portable_config(config: &AppConfig) -> PortableProfileConfig {
    PortableProfileConfig {
        language: config.language.clone(),
        git_cache_cleanup_days: config.git_cache_cleanup_days,
        git_cache_ttl_secs: config.git_cache_ttl_secs,
        origin_rules: config.origin_rules.clone(),
        explore_sources: config.explore_sources.clone(),
    }
}

fn apply_portable_config(store: &SkillStore, portable: &PortableProfileConfig) -> Result<()> {
    let mut current = load_app_config(store)?;
    current.language = portable.language.clone();
    current.git_cache_cleanup_days = portable.git_cache_cleanup_days;
    current.git_cache_ttl_secs = portable.git_cache_ttl_secs;
    current.origin_rules = portable.origin_rules.clone();
    current.explore_sources = portable.explore_sources.clone();
    save_app_config_impl(store, &current)
}

fn load_document_setting(store: &SkillStore, key: &str) -> Result<Option<ProfileDocument>> {
    let Some(raw) = store.get_setting(key)? else {
        return Ok(None);
    };
    let document: ProfileDocument =
        serde_json::from_str(&raw).context("解析本机 Profile 同步状态失败")?;
    document.validate()?;
    Ok(Some(document))
}

fn get_or_create_setting(store: &SkillStore, key: &str) -> Result<String> {
    if let Some(value) = store.get_setting(key)? {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }
    let value = Uuid::new_v4().to_string();
    store.set_setting(key, &value)?;
    Ok(value)
}

fn profile_origin(store: &SkillStore, skill_id: &str) -> Result<Option<ProfileOriginOverride>> {
    let Some(origin) = store.get_skill_origin(skill_id)? else {
        return Ok(None);
    };
    if !origin.manual_override {
        return Ok(None);
    }
    Ok(Some(ProfileOriginOverride {
        origin_kind: origin.origin_kind,
        origin_role: origin.origin_role,
        provider: origin.provider,
        remote_url: origin.remote_url,
        owner: origin.owner,
        repo: origin.repo,
        branch: origin.branch,
        subpath: origin.subpath,
        update_strategy: origin.update_strategy,
        publish_strategy: origin.publish_strategy,
    }))
}

fn semantic_skill_equal(left: &ProfileSkill, right: &ProfileSkill) -> bool {
    left.key == right.key
        && left.name == right.name
        && left.source_type == right.source_type
        && left.source_ref == right.source_ref
        && left.source_subpath == right.source_subpath
        && left.update_policy == right.update_policy
        && left.pinned_revision == right.pinned_revision
        && left.targets == right.targets
        && left.tags == right.tags
        && left.origin_override == right.origin_override
        && left.deleted_at == right.deleted_at
}

fn build_local_document(
    store: &SkillStore,
    base: Option<&ProfileDocument>,
    profile_id: &str,
    device_id: &str,
) -> Result<(ProfileDocument, Vec<String>)> {
    let timestamp = now_ms();
    let config = portable_config(&load_app_config(store)?);
    let base_skills: BTreeMap<&str, &ProfileSkill> = base
        .into_iter()
        .flat_map(|document| document.skills.iter())
        .map(|skill| (skill.key.as_str(), skill))
        .collect();
    let mut skills = Vec::new();
    let mut seen = BTreeSet::new();
    let mut skipped_local = Vec::new();

    for record in store.list_skills()? {
        let Some(key) = stable_skill_key(&record) else {
            skipped_local.push(record.name);
            continue;
        };
        let mut targets: Vec<String> = store
            .list_skill_targets(&record.id)?
            .into_iter()
            .filter(|target| target.scope == "global" && !target.tool.starts_with("custom:"))
            .map(|target| target.tool)
            .collect();
        targets.sort();
        targets.dedup();
        let mut tags: Vec<String> = store
            .get_skill_tags(&record.id)?
            .into_iter()
            .map(|tag| tag.name)
            .collect();
        tags.sort_by_key(|tag| tag.to_lowercase());
        tags.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

        let previous = base_skills.get(key.as_str()).copied();
        let mut skill = ProfileSkill {
            key: key.clone(),
            name: record.name,
            source_type: record.source_type,
            source_ref: record.source_ref.unwrap_or_default(),
            source_subpath: record.source_subpath,
            update_policy: if record.source_revision.is_some() {
                "pinned".to_string()
            } else {
                previous
                    .map(|item| item.update_policy.clone())
                    .unwrap_or_else(default_update_policy)
            },
            pinned_revision: record
                .source_revision
                .clone()
                .or_else(|| previous.and_then(|item| item.pinned_revision.clone())),
            targets,
            tags,
            origin_override: profile_origin(store, &record.id)?,
            deleted_at: None,
            updated_at: timestamp,
            updated_by: device_id.to_string(),
        };
        if let Some(previous) = previous {
            if semantic_skill_equal(&skill, previous) {
                skill.updated_at = previous.updated_at;
                skill.updated_by = previous.updated_by.clone();
            }
        }
        seen.insert(key);
        skills.push(skill);
    }

    if let Some(base) = base {
        for previous in &base.skills {
            if !seen.contains(&previous.key) && previous.deleted_at.is_none() {
                let mut tombstone = previous.clone();
                tombstone.deleted_at = Some(timestamp);
                tombstone.updated_at = timestamp;
                tombstone.updated_by = device_id.to_string();
                skills.push(tombstone);
            } else if previous.deleted_at.is_some() && !seen.contains(&previous.key) {
                skills.push(previous.clone());
            }
        }
    }

    skills.sort_by(|left, right| left.key.cmp(&right.key));
    let (config_updated_at, config_updated_by) = match base {
        Some(base) if base.config == config => {
            (base.config_updated_at, base.config_updated_by.clone())
        }
        _ => (timestamp, device_id.to_string()),
    };
    Ok((
        ProfileDocument {
            profile_version: PROFILE_VERSION,
            profile_id: profile_id.to_string(),
            generation: base.map(|item| item.generation).unwrap_or(0),
            updated_at: timestamp,
            updated_by: device_id.to_string(),
            config,
            config_updated_at,
            config_updated_by,
            skills,
        },
        skipped_local,
    ))
}

fn document_semantic_equal(left: &ProfileDocument, right: &ProfileDocument) -> bool {
    left.profile_id == right.profile_id
        && left.config == right.config
        && left.skills == right.skills
}

fn skill_map(document: Option<&ProfileDocument>) -> BTreeMap<String, ProfileSkill> {
    document
        .into_iter()
        .flat_map(|item| item.skills.iter().cloned())
        .map(|skill| (skill.key.clone(), skill))
        .collect()
}

fn skill_identity_equal(left: &ProfileSkill, right: &ProfileSkill) -> bool {
    left.key == right.key
        && left.name == right.name
        && left.source_type == right.source_type
        && left.source_ref == right.source_ref
        && left.source_subpath == right.source_subpath
        && left.update_policy == right.update_policy
        && left.pinned_revision == right.pinned_revision
        && left.origin_override == right.origin_override
        && left.deleted_at == right.deleted_at
}

fn union_sorted(values: impl IntoIterator<Item = String>, case_insensitive: bool) -> Vec<String> {
    let mut unique = BTreeMap::<String, String>::new();
    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let key = if case_insensitive {
            trimmed.to_lowercase()
        } else {
            trimmed.to_string()
        };
        unique.entry(key).or_insert_with(|| trimmed.to_string());
    }
    unique.into_values().collect()
}

/// Merge concurrent edits that only extend a Skill's set-like fields. Source,
/// revision, ownership and deletion state must still agree exactly.
fn merge_compatible_skill(
    local: &ProfileSkill,
    remote: &ProfileSkill,
    device_id: &str,
) -> Option<ProfileSkill> {
    if !skill_identity_equal(local, remote) {
        return None;
    }
    let mut merged = if local.updated_at >= remote.updated_at {
        local.clone()
    } else {
        remote.clone()
    };
    merged.targets = union_sorted(
        local.targets.iter().chain(remote.targets.iter()).cloned(),
        false,
    );
    merged.tags = union_sorted(local.tags.iter().chain(remote.tags.iter()).cloned(), true);
    merged.updated_at = now_ms();
    merged.updated_by = device_id.to_string();
    Some(merged)
}

fn merge_documents(
    base: Option<&ProfileDocument>,
    local: &ProfileDocument,
    remote: Option<&ProfileDocument>,
    device_id: &str,
    strategy: ConflictStrategy,
) -> (ProfileDocument, Vec<ProfileConflict>) {
    if base.is_none() {
        if let Some(remote) = remote {
            let mut merged = remote.clone();
            let mut skills = skill_map(Some(remote));
            let mut conflicts = Vec::new();
            for (key, local_skill) in skill_map(Some(local)) {
                if let Some(remote_skill) = skills.get(&key) {
                    if !semantic_skill_equal(remote_skill, &local_skill) {
                        if let Some(union) =
                            merge_compatible_skill(&local_skill, remote_skill, device_id)
                        {
                            skills.insert(key.clone(), union);
                        } else {
                            conflicts.push(ProfileConflict {
                                path: format!("skills.{key}"),
                                reason: "首次连接时本机和远端的同源 Skill 来源、revision 或删除状态不一致".to_string(),
                            });
                            if strategy == ConflictStrategy::Local {
                                skills.insert(key.clone(), local_skill.clone());
                            }
                        }
                    }
                } else {
                    skills.insert(key, local_skill);
                }
            }
            merged.skills = skills.into_values().collect();
            merged.updated_at = now_ms();
            merged.updated_by = device_id.to_string();
            return (merged, conflicts);
        }
        return (local.clone(), Vec::new());
    }

    let base = base.expect("checked above");
    let remote = remote.unwrap_or(base);
    let mut conflicts = Vec::new();
    let config = if local.config == base.config {
        if strategy == ConflictStrategy::Local {
            local.config.clone()
        } else {
            remote.config.clone()
        }
    } else if remote.config == base.config || local.config == remote.config {
        local.config.clone()
    } else {
        conflicts.push(ProfileConflict {
            path: "config".to_string(),
            reason: "本机和远端同时修改了共享配置".to_string(),
        });
        remote.config.clone()
    };

    let base_map = skill_map(Some(base));
    let local_map = skill_map(Some(local));
    let remote_map = skill_map(Some(remote));
    let keys: BTreeSet<String> = base_map
        .keys()
        .chain(local_map.keys())
        .chain(remote_map.keys())
        .cloned()
        .collect();
    let mut skills = Vec::new();
    for key in keys {
        let base_skill = base_map.get(&key);
        let local_skill = local_map.get(&key);
        let remote_skill = remote_map.get(&key);
        let selected = if local_skill == base_skill {
            remote_skill.cloned()
        } else if remote_skill == base_skill || local_skill == remote_skill {
            local_skill.cloned()
        } else if let (Some(local_skill), Some(remote_skill)) = (local_skill, remote_skill) {
            if let Some(union) = merge_compatible_skill(local_skill, remote_skill, device_id) {
                Some(union)
            } else {
                conflicts.push(ProfileConflict {
                    path: format!("skills.{key}"),
                    reason: "本机和远端同时修改了同一 Skill 的来源、revision 或删除状态"
                        .to_string(),
                });
                if strategy == ConflictStrategy::Local {
                    Some(local_skill.clone())
                } else {
                    Some(remote_skill.clone())
                }
            }
        } else {
            conflicts.push(ProfileConflict {
                path: format!("skills.{key}"),
                reason: "一台设备删除了 Skill，另一台设备仍保留或修改了它".to_string(),
            });
            if strategy == ConflictStrategy::Local {
                local_skill.cloned().or_else(|| remote_skill.cloned())
            } else {
                remote_skill.cloned().or_else(|| local_skill.cloned())
            }
        };
        if let Some(skill) = selected {
            skills.push(skill);
        }
    }
    skills.sort_by(|left, right| left.key.cmp(&right.key));
    let timestamp = now_ms();
    (
        ProfileDocument {
            profile_version: PROFILE_VERSION,
            profile_id: remote.profile_id.clone(),
            generation: remote.generation.max(local.generation),
            updated_at: timestamp,
            updated_by: device_id.to_string(),
            config_updated_at: if config == local.config {
                local.config_updated_at
            } else {
                remote.config_updated_at
            },
            config_updated_by: if config == local.config {
                local.config_updated_by.clone()
            } else {
                remote.config_updated_by.clone()
            },
            config,
            skills,
        },
        conflicts,
    )
}

fn parse_package_ref(raw: &str) -> Result<(String, Option<String>)> {
    #[derive(Deserialize)]
    struct PackageRef {
        package: String,
        #[serde(default)]
        command: Option<String>,
    }
    let parsed: PackageRef = serde_json::from_str(raw).context("解析 package 来源失败")?;
    Ok((parsed.package, parsed.command))
}

fn apply_tags(store: &SkillStore, skill_id: &str, names: &[String]) -> Result<()> {
    let mut known: HashMap<String, i64> = store
        .list_tags_with_counts()?
        .into_iter()
        .map(|tag| (tag.name.to_lowercase(), tag.id))
        .collect();
    let mut ids = Vec::new();
    for name in names {
        let normalized = name.trim().to_lowercase();
        if normalized.is_empty() {
            continue;
        }
        let id = if let Some(id) = known.get(&normalized) {
            *id
        } else {
            let tag = store.create_tag(name)?;
            known.insert(normalized, tag.id);
            tag.id
        };
        ids.push(id);
    }
    store.set_skill_tags(skill_id, &ids)
}

fn apply_origin_override(
    store: &SkillStore,
    skill_id: &str,
    override_value: Option<&ProfileOriginOverride>,
) -> Result<()> {
    let Some(origin) = override_value else {
        return Ok(());
    };
    store.upsert_skill_origin(&SkillOriginRecord {
        skill_id: skill_id.to_string(),
        origin_kind: origin.origin_kind.clone(),
        origin_role: origin.origin_role.clone(),
        provider: origin.provider.clone(),
        remote_url: origin.remote_url.clone(),
        owner: origin.owner.clone(),
        repo: origin.repo.clone(),
        branch: origin.branch.clone(),
        subpath: origin.subpath.clone(),
        update_strategy: origin.update_strategy.clone(),
        publish_strategy: origin.publish_strategy.clone(),
        manual_override: true,
        reason: Some("restored from synchronized profile".to_string()),
        updated_at: now_ms(),
    })
}

fn apply_document(
    store: &SkillStore,
    document: &ProfileDocument,
    apply_deletions: bool,
    report: &mut ProfileSyncReport,
) -> Result<()> {
    apply_portable_config(store, &document.config)?;
    let mut existing: HashMap<String, SkillRecord> = store
        .list_skills()?
        .into_iter()
        .filter_map(|record| stable_skill_key(&record).map(|key| (key, record)))
        .collect();

    for desired in &document.skills {
        if desired.deleted_at.is_some() {
            if let Some(record) = existing.remove(&desired.key) {
                if apply_deletions {
                    match delete_skill_cli(store, &record.id) {
                        Ok(()) => report.deleted.push(record.name),
                        Err(error) => report
                            .failures
                            .push((record.name, format!("删除失败: {error:#}"))),
                    }
                } else {
                    report.pending_deletions.push(record.name);
                }
            }
            continue;
        }

        let record = if let Some(record) = existing.get(&desired.key).cloned() {
            if matches!(desired.update_policy.as_str(), "latest" | "pinned")
                && record.source_type == "git"
            {
                let current_hash = hash_dir(std::path::Path::new(&record.central_path)).ok();
                let has_local_changes = record.content_hash.is_some()
                    && current_hash.is_some()
                    && record.content_hash != current_hash;
                if has_local_changes {
                    report.failures.push((
                        record.name.clone(),
                        "检测到未推送的本地修改，已跳过远端更新".to_string(),
                    ));
                } else {
                    match update_managed_skill_from_source_cli(store, &record.id) {
                        Ok(_) => report.updated.push(record.name.clone()),
                        Err(error) => report
                            .failures
                            .push((record.name.clone(), format!("更新失败: {error:#}"))),
                    }
                }
            }
            let refreshed = store.get_skill_by_id(&record.id)?.unwrap_or(record);
            if desired.update_policy == "pinned"
                && desired.pinned_revision.is_some()
                && refreshed.source_revision != desired.pinned_revision
            {
                report.failures.push((
                    refreshed.name.clone(),
                    format!(
                        "Profile 固定 revision {} 与当前 revision {} 不一致",
                        desired.pinned_revision.as_deref().unwrap_or("unknown"),
                        refreshed.source_revision.as_deref().unwrap_or("unknown")
                    ),
                ));
            }
            refreshed
        } else {
            let install = match desired.source_type.as_str() {
                "git" => {
                    install_git_skill_cli(store, &desired.source_ref, Some(desired.name.clone()))
                }
                "package" => {
                    let (package, command) = parse_package_ref(&desired.source_ref)?;
                    install_package_skill_cli(
                        store,
                        &package,
                        command.as_deref(),
                        Some(desired.name.clone()),
                    )
                }
                other => anyhow::bail!("Profile 包含不可同步来源类型: {other}"),
            };
            match install {
                Ok(result) => {
                    report.installed.push(result.name.clone());
                    store
                        .get_skill_by_id(&result.skill_id)?
                        .ok_or_else(|| anyhow::anyhow!("安装后未找到 Skill"))?
                }
                Err(error) => {
                    report
                        .failures
                        .push((desired.name.clone(), format!("安装失败: {error:#}")));
                    continue;
                }
            }
        };

        if let Err(error) = apply_tags(store, &record.id, &desired.tags) {
            report
                .failures
                .push((record.name.clone(), format!("恢复标签失败: {error:#}")));
        }
        if let Err(error) =
            apply_origin_override(store, &record.id, desired.origin_override.as_ref())
        {
            report
                .failures
                .push((record.name.clone(), format!("恢复来源覆盖失败: {error:#}")));
        }

        let desired_targets: BTreeSet<&str> = desired.targets.iter().map(String::as_str).collect();
        for target in store.list_skill_targets(&record.id)? {
            if target.scope == "global"
                && !target.tool.starts_with("custom:")
                && !desired_targets.contains(target.tool.as_str())
            {
                match unsync_skill_cli(store, &record.id, &target.tool) {
                    Ok(()) => report
                        .removed_targets
                        .push(format!("{} → {}", record.name, target.tool)),
                    Err(error) => report.failures.push((
                        record.name.clone(),
                        format!("移除目标 {} 失败: {error:#}", target.tool),
                    )),
                }
            }
        }
        for tool in &desired.targets {
            match sync_skill_target_cli(store, &record.id, tool, "global", None) {
                Ok(_) => report
                    .synced_targets
                    .push(format!("{} → {}", record.name, tool)),
                Err(error) => report.failures.push((
                    record.name.clone(),
                    format!("同步目标 {tool} 失败: {error:#}"),
                )),
            }
        }
    }
    Ok(())
}

fn load_remote(
    client: &WebDavClient,
    path: &str,
) -> Result<Option<(ProfileDocument, Option<String>)>> {
    let Some(remote) = client.get_optional(path)? else {
        return Ok(None);
    };
    let document: ProfileDocument =
        serde_json::from_str(&remote.body).context("解析远端 Profile 失败")?;
    document.validate()?;
    Ok(Some((document, remote.etag)))
}

/// Compare and optionally synchronize the local desired state with WebDAV.
pub fn synchronize_profile(
    store: &SkillStore,
    dry_run: bool,
    apply_deletions: bool,
    strategy: ConflictStrategy,
) -> Result<ProfileSyncReport> {
    let app_config = load_app_config(store)?;
    let webdav = app_config
        .webdav
        .clone()
        .ok_or_else(|| anyhow::anyhow!("WebDAV 未配置"))?;
    let client = WebDavClient::new(&webdav)?;
    let remote_path = profile_remote_path(&webdav.remote_dir);
    let remote_loaded = load_remote(&client, &remote_path)?;
    let remote_document = remote_loaded.as_ref().map(|(document, _)| document);
    let remote_etag = remote_loaded.as_ref().and_then(|(_, etag)| etag.as_deref());
    let base = load_document_setting(store, PROFILE_BASE_KEY)?;
    let device_id = get_or_create_setting(store, PROFILE_DEVICE_ID_KEY)?;
    let stored_profile_id = store.get_setting(PROFILE_ID_KEY)?;
    let profile_id = remote_document
        .map(|document| document.profile_id.clone())
        .or_else(|| base.as_ref().map(|document| document.profile_id.clone()))
        .or(stored_profile_id)
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Some(remote) = remote_document {
        if remote.profile_id != profile_id {
            anyhow::bail!("远端 Profile ID 与本机绑定不一致");
        }
    }
    let usable_base = base
        .as_ref()
        .filter(|document| document.profile_id == profile_id);
    let (local, skipped_local) = build_local_document(store, usable_base, &profile_id, &device_id)?;
    let (mut merged, conflicts) =
        merge_documents(usable_base, &local, remote_document, &device_id, strategy);
    let changed = remote_document
        .map(|remote| !document_semantic_equal(&merged, remote))
        .unwrap_or(true);
    let mut report = ProfileSyncReport {
        profile_id: profile_id.clone(),
        device_id: device_id.clone(),
        remote_found: remote_document.is_some(),
        changed,
        conflicts,
        conflict_strategy: strategy.as_str().to_string(),
        conflicts_resolved: strategy != ConflictStrategy::Abort,
        skipped_local,
        ..ProfileSyncReport::default()
    };
    if dry_run || (!report.conflicts.is_empty() && strategy == ConflictStrategy::Abort) {
        return Ok(report);
    }

    apply_document(store, &merged, apply_deletions, &mut report)?;
    if changed {
        merged.generation = remote_document
            .map(|document| document.generation)
            .unwrap_or(0)
            .max(usable_base.map(|document| document.generation).unwrap_or(0))
            + 1;
        merged.updated_at = now_ms();
        merged.updated_by = device_id.clone();
        let body = serde_json::to_string_pretty(&merged)?;
        let prepared = prepare_remote_dir(&webdav)?;
        let new_etag = prepared.put_conditional(&remote_path, &body, remote_etag)?;
        if let Some(etag) = new_etag {
            store.set_setting(PROFILE_ETAG_KEY, &etag)?;
        }
        report.uploaded = true;
    }
    store.set_setting(PROFILE_ID_KEY, &profile_id)?;
    store.set_setting(PROFILE_BASE_KEY, &serde_json::to_string(&merged)?)?;
    Ok(report)
}

/// Export the current portable desired state without contacting WebDAV.
pub fn export_profile_json(store: &SkillStore) -> Result<String> {
    let base = load_document_setting(store, PROFILE_BASE_KEY)?;
    let device_id = get_or_create_setting(store, PROFILE_DEVICE_ID_KEY)?;
    let profile_id = store
        .get_setting(PROFILE_ID_KEY)?
        .or_else(|| base.as_ref().map(|item| item.profile_id.clone()))
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let (document, _) = build_local_document(store, base.as_ref(), &profile_id, &device_id)?;
    serde_json::to_string_pretty(&document).context("序列化 Profile 失败")
}

/// Merge and apply an offline Profile document without contacting WebDAV.
pub fn import_profile_json(
    store: &SkillStore,
    raw: &str,
    strategy: ConflictStrategy,
    apply_deletions: bool,
) -> Result<ProfileSyncReport> {
    let imported: ProfileDocument = serde_json::from_str(raw).context("解析 Profile 文件失败")?;
    imported.validate()?;
    let base = load_document_setting(store, PROFILE_BASE_KEY)?;
    if let Some(base) = &base {
        if base.profile_id != imported.profile_id {
            anyhow::bail!("Profile ID 与当前设备绑定不一致");
        }
    }
    let device_id = get_or_create_setting(store, PROFILE_DEVICE_ID_KEY)?;
    let (local, skipped_local) =
        build_local_document(store, base.as_ref(), &imported.profile_id, &device_id)?;
    let (merged, conflicts) =
        merge_documents(base.as_ref(), &local, Some(&imported), &device_id, strategy);
    let mut report = ProfileSyncReport {
        profile_id: imported.profile_id.clone(),
        device_id,
        remote_found: false,
        changed: !document_semantic_equal(&local, &merged),
        conflicts,
        conflict_strategy: strategy.as_str().to_string(),
        conflicts_resolved: strategy != ConflictStrategy::Abort,
        skipped_local,
        ..ProfileSyncReport::default()
    };
    if !report.conflicts.is_empty() && strategy == ConflictStrategy::Abort {
        return Ok(report);
    }
    apply_document(store, &merged, apply_deletions, &mut report)?;
    store.set_setting(PROFILE_ID_KEY, &imported.profile_id)?;
    store.set_setting(PROFILE_BASE_KEY, &serde_json::to_string(&merged)?)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> PortableProfileConfig {
        PortableProfileConfig {
            language: Some("zh".to_string()),
            git_cache_cleanup_days: 30,
            git_cache_ttl_secs: 60,
            origin_rules: OriginRules::default(),
            explore_sources: Vec::new(),
        }
    }

    fn skill(key: &str, name: &str, by: &str) -> ProfileSkill {
        ProfileSkill {
            key: key.to_string(),
            name: name.to_string(),
            source_type: "git".to_string(),
            source_ref: format!("https://github.com/example/{name}"),
            source_subpath: None,
            update_policy: "manual".to_string(),
            pinned_revision: None,
            targets: vec!["codex".to_string()],
            tags: Vec::new(),
            origin_override: None,
            deleted_at: None,
            updated_at: 1,
            updated_by: by.to_string(),
        }
    }

    fn document(skills: Vec<ProfileSkill>) -> ProfileDocument {
        ProfileDocument {
            profile_version: PROFILE_VERSION,
            profile_id: "profile".to_string(),
            generation: 1,
            updated_at: 1,
            updated_by: "a".to_string(),
            config: config(),
            config_updated_at: 1,
            config_updated_by: "a".to_string(),
            skills,
        }
    }

    #[test]
    fn merges_independent_device_additions() {
        let base = document(Vec::new());
        let local = document(vec![skill("a", "a", "device-a")]);
        let remote = document(vec![skill("b", "b", "device-b")]);
        let (merged, conflicts) = merge_documents(
            Some(&base),
            &local,
            Some(&remote),
            "a",
            ConflictStrategy::Abort,
        );
        assert!(conflicts.is_empty());
        assert_eq!(merged.skills.len(), 2);
    }

    #[test]
    fn merges_concurrent_targets_and_tags_as_union() {
        let original = skill("a", "demo", "base");
        let base = document(vec![original.clone()]);
        let mut local_skill = original.clone();
        local_skill.targets.push("cursor".to_string());
        let mut remote_skill = original;
        remote_skill.tags.push("shared".to_string());
        let local = document(vec![local_skill]);
        let remote = document(vec![remote_skill]);
        let (merged, conflicts) = merge_documents(
            Some(&base),
            &local,
            Some(&remote),
            "a",
            ConflictStrategy::Abort,
        );
        assert!(conflicts.is_empty());
        assert_eq!(merged.skills[0].targets, vec!["codex", "cursor"]);
        assert_eq!(merged.skills[0].tags, vec!["shared"]);
    }

    #[test]
    fn first_connection_merges_compatible_same_skill() {
        let mut local_skill = skill("a", "demo", "local");
        local_skill.targets.push("cursor".to_string());
        let mut remote_skill = skill("a", "demo", "remote");
        remote_skill.tags.push("Shared".to_string());
        let local = document(vec![local_skill]);
        let remote = document(vec![remote_skill]);
        let (merged, conflicts) = merge_documents(
            None,
            &local,
            Some(&remote),
            "local",
            ConflictStrategy::Abort,
        );
        assert!(conflicts.is_empty());
        assert_eq!(merged.skills[0].targets, vec!["codex", "cursor"]);
        assert_eq!(merged.skills[0].tags, vec!["Shared"]);
    }

    #[test]
    fn conflicting_revisions_are_not_merged() {
        let mut original = skill("a", "demo", "base");
        original.update_policy = "pinned".to_string();
        original.pinned_revision = Some("base".to_string());
        let base = document(vec![original.clone()]);
        let mut local_skill = original.clone();
        local_skill.pinned_revision = Some("local".to_string());
        let mut remote_skill = original;
        remote_skill.pinned_revision = Some("remote".to_string());
        let (merged, conflicts) = merge_documents(
            Some(&base),
            &document(vec![local_skill]),
            Some(&document(vec![remote_skill])),
            "local",
            ConflictStrategy::Abort,
        );
        assert_eq!(conflicts.len(), 1);
        assert_eq!(merged.skills[0].pinned_revision.as_deref(), Some("remote"));
    }

    #[test]
    fn three_devices_accumulate_independent_skills() {
        let empty = document(Vec::new());
        let device_a = document(vec![skill("a", "a", "a")]);
        let device_b = document(vec![skill("b", "b", "b")]);
        let (ab, conflicts) = merge_documents(
            Some(&empty),
            &device_a,
            Some(&device_b),
            "a",
            ConflictStrategy::Abort,
        );
        assert!(conflicts.is_empty());
        let device_c = document(vec![skill("c", "c", "c")]);
        let (abc, conflicts) = merge_documents(
            Some(&empty),
            &device_c,
            Some(&ab),
            "c",
            ConflictStrategy::Abort,
        );
        assert!(conflicts.is_empty());
        assert_eq!(abc.skills.len(), 3);
    }

    #[test]
    fn stable_key_ignores_git_url_transport() {
        let record = |source_ref: &str| SkillRecord {
            id: "id".to_string(),
            name: "Demo".to_string(),
            description: None,
            source_type: "git".to_string(),
            source_ref: Some(source_ref.to_string()),
            source_subpath: None,
            source_revision: None,
            central_path: "/tmp/demo".to_string(),
            content_hash: None,
            created_at: 0,
            updated_at: 0,
            last_sync_at: None,
            last_seen_at: 0,
            status: "ok".to_string(),
        };
        assert_eq!(
            stable_skill_key(&record("https://github.com/example/demo.git")),
            stable_skill_key(&record("git@github.com:example/demo.git"))
        );
    }

    #[test]
    fn portable_config_excludes_device_paths_and_credentials() {
        let app = AppConfig {
            language: Some("zh".to_string()),
            storage_path: Some("/Users/device-a/private-skills".to_string()),
            webdav: Some(super::super::app_config::WebDavConfig {
                url: "https://dav.example.test".to_string(),
                user: "alice".to_string(),
                password: "secret".to_string(),
                remote_dir: "private".to_string(),
            }),
            ..AppConfig::default()
        };

        let json = serde_json::to_string(&portable_config(&app)).unwrap();
        assert!(!json.contains("private-skills"));
        assert!(!json.contains("dav.example.test"));
        assert!(!json.contains("alice"));
        assert!(!json.contains("secret"));
    }

    #[test]
    fn tombstone_propagates_when_other_device_is_unchanged() {
        let original = skill("a", "demo", "base");
        let base = document(vec![original.clone()]);
        let local = document(vec![ProfileSkill {
            deleted_at: Some(2),
            updated_at: 2,
            updated_by: "device-a".to_string(),
            ..original.clone()
        }]);
        let remote = document(vec![original]);

        let (merged, conflicts) = merge_documents(
            Some(&base),
            &local,
            Some(&remote),
            "device-a",
            ConflictStrategy::Abort,
        );

        assert!(conflicts.is_empty());
        assert_eq!(merged.skills[0].deleted_at, Some(2));
    }

    #[test]
    fn conflict_strategy_selects_requested_side() {
        let mut original = skill("a", "demo", "base");
        original.update_policy = "pinned".to_string();
        original.pinned_revision = Some("base".to_string());
        let base = document(vec![original.clone()]);
        let mut local_skill = original.clone();
        local_skill.pinned_revision = Some("local".to_string());
        let mut remote_skill = original;
        remote_skill.pinned_revision = Some("remote".to_string());
        let local = document(vec![local_skill]);
        let remote = document(vec![remote_skill]);

        let (local_result, local_conflicts) = merge_documents(
            Some(&base),
            &local,
            Some(&remote),
            "a",
            ConflictStrategy::Local,
        );
        let (remote_result, remote_conflicts) = merge_documents(
            Some(&base),
            &local,
            Some(&remote),
            "a",
            ConflictStrategy::Remote,
        );

        assert_eq!(local_conflicts.len(), 1);
        assert_eq!(remote_conflicts.len(), 1);
        assert_eq!(
            local_result.skills[0].pinned_revision.as_deref(),
            Some("local")
        );
        assert_eq!(
            remote_result.skills[0].pinned_revision.as_deref(),
            Some("remote")
        );
    }
}
