//! High-level cross-device publish and pull pipelines.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

use super::app_config::load_app_config;
use super::backup::export_full_backup;
use super::content_hash::hash_dir;
use super::installer::{check_managed_skill_update_cli, push_skill_cli};
use super::profile_sync::{synchronize_profile, ConflictStrategy, ProfileSyncReport};
use super::skill_store::{SkillRecord, SkillStore};
use super::webdav::upload_backup;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevicePipelineStage {
    pub id: String,
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DevicePipelineReport {
    pub mode: String,
    pub state: String,
    pub stages: Vec<DevicePipelineStage>,
    pub profile: Option<ProfileSyncReport>,
    pub local_ahead: usize,
    pub remote_ahead: usize,
    pub dirty_repositories: usize,
    pub pushable_repositories: usize,
    pub pullable_skills: usize,
    pub local_only_skills: Vec<String>,
    pub pushed: Vec<String>,
    pub no_changes: Vec<String>,
    pub backup_remote_path: Option<String>,
    pub failures: Vec<(String, String)>,
}

fn stage(id: &str, status: &str, message: impl Into<String>) -> DevicePipelineStage {
    DevicePipelineStage {
        id: id.to_string(),
        status: status.to_string(),
        message: message.into(),
    }
}

fn local_changed(skill: &SkillRecord) -> bool {
    let current = hash_dir(Path::new(&skill.central_path)).ok();
    skill.content_hash.is_some() && current.is_some() && skill.content_hash != current
}

fn source_key(skill: &SkillRecord) -> String {
    skill
        .source_ref
        .as_deref()
        .unwrap_or(&skill.id)
        .trim()
        .trim_end_matches('/')
        .trim_end_matches(".git")
        .to_ascii_lowercase()
}

fn publishable(store: &SkillStore, skill: &SkillRecord) -> Result<bool> {
    Ok(store
        .get_skill_origin(&skill.id)?
        .as_ref()
        .is_some_and(|origin| origin.publish_strategy == "git_push"))
}

fn refresh_git_snapshot(store: &SkillStore, skill: &SkillRecord) -> Result<()> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(&skill.central_path)
        .output()
        .context("failed to read pushed Git revision")?;
    if !output.status.success() {
        anyhow::bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let mut updated = skill.clone();
    updated.source_revision = Some(String::from_utf8_lossy(&output.stdout).trim().to_string());
    updated.content_hash = hash_dir(Path::new(&skill.central_path)).ok();
    store.upsert_skill(&updated)
}

pub fn device_status(store: &SkillStore) -> Result<DevicePipelineReport> {
    let profile = synchronize_profile(store, true, false, ConflictStrategy::Abort)?;
    let mut report = DevicePipelineReport {
        mode: "status".to_string(),
        state: if profile.conflicts.is_empty() {
            "ready"
        } else {
            "conflict"
        }
        .to_string(),
        profile: Some(profile),
        ..DevicePipelineReport::default()
    };
    let mut repos = BTreeMap::<String, (bool, bool)>::new();
    for skill in store.list_skills()? {
        if skill.source_type == "local" {
            report.local_only_skills.push(skill.name);
            continue;
        }
        if skill.source_type != "git" {
            report.pullable_skills += 1;
            continue;
        }
        let can_push = publishable(store, &skill)?;
        let dirty = local_changed(&skill);
        let entry = repos.entry(source_key(&skill)).or_default();
        entry.0 |= can_push;
        entry.1 |= dirty;
        if dirty {
            report.local_ahead += 1;
        }
        report.pullable_skills += 1;
    }
    report.pushable_repositories = repos.values().filter(|(push, _)| *push).count();
    report.dirty_repositories = repos.values().filter(|(_, dirty)| *dirty).count();
    report.stages.push(stage(
        "inspect",
        "completed",
        "Device synchronization state inspected",
    ));
    Ok(report)
}

pub fn device_pull(store: &SkillStore, apply_deletions: bool) -> Result<DevicePipelineReport> {
    let mut report = DevicePipelineReport {
        mode: "pull".to_string(),
        state: "running".to_string(),
        ..DevicePipelineReport::default()
    };
    report.stages.push(stage(
        "profile",
        "running",
        "Downloading and merging WebDAV Profile",
    ));
    let profile = synchronize_profile(store, false, apply_deletions, ConflictStrategy::Abort)?;
    report.local_only_skills = profile.skipped_local.clone();
    report.failures.extend(profile.failures.clone());
    report.state = if !profile.conflicts.is_empty() {
        "conflict"
    } else if !report.failures.is_empty() {
        "partial"
    } else {
        "completed"
    }
    .to_string();
    report.stages.push(stage(
        "profile",
        if profile.conflicts.is_empty() {
            "completed"
        } else {
            "blocked"
        },
        format!(
            "Installed {}, updated {}, conflicts {}",
            profile.installed.len(),
            profile.updated.len(),
            profile.conflicts.len()
        ),
    ));
    report.profile = Some(profile);
    Ok(report)
}

pub fn device_publish(store: &SkillStore, confirm_push: bool) -> Result<DevicePipelineReport> {
    let mut report = DevicePipelineReport {
        mode: "publish".to_string(),
        state: "running".to_string(),
        ..DevicePipelineReport::default()
    };
    report.stages.push(stage(
        "pull",
        "running",
        "Merging Profile and refreshing Git/package sources",
    ));
    let initial = synchronize_profile(store, false, false, ConflictStrategy::Abort)?;
    if !initial.conflicts.is_empty() {
        report.state = "conflict".to_string();
        report.stages.push(stage(
            "pull",
            "blocked",
            "Profile conflicts must be resolved before publishing",
        ));
        report.profile = Some(initial);
        return Ok(report);
    }
    report.failures.extend(initial.failures.clone());
    report.stages.push(stage(
        "pull",
        "completed",
        format!("Updated {} source Skill(s)", initial.updated.len()),
    ));

    let mut dirty_by_repo = BTreeMap::<String, Vec<SkillRecord>>::new();
    for skill in store.list_skills()? {
        if skill.source_type == "local" {
            report.local_only_skills.push(skill.name);
        } else if skill.source_type == "git" && publishable(store, &skill)? {
            report.pushable_repositories += 1;
            if local_changed(&skill) {
                dirty_by_repo
                    .entry(source_key(&skill))
                    .or_default()
                    .push(skill);
            } else {
                report.no_changes.push(skill.name);
            }
        }
    }
    report.dirty_repositories = dirty_by_repo.len();
    report.local_ahead = dirty_by_repo.values().map(Vec::len).sum();
    report.stages.push(stage(
        "publish",
        "running",
        "Checking owned repositories before push",
    ));
    for skills in dirty_by_repo.into_values() {
        if skills.len() > 1 {
            let names = skills
                .iter()
                .map(|skill| skill.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            report.failures.push((
                names,
                "同一仓库有多个 Skill 同时存在本地修改；为避免稀疏仓库互相覆盖，已停止自动推送"
                    .to_string(),
            ));
            continue;
        }
        let skill = &skills[0];
        if !confirm_push {
            report.failures.push((
                skill.name.clone(),
                "需要 --yes 才能提交并推送本地修改".to_string(),
            ));
            continue;
        }
        match check_managed_skill_update_cli(store, &skill.id) {
            Ok(check)
                if check.latest_revision.is_some()
                    && check.current_revision.is_some()
                    && check.latest_revision != check.current_revision =>
            {
                report.failures.push((
                    skill.name.clone(),
                    "远端仓库已领先；请先处理本地修改与远端版本冲突".to_string(),
                ));
                continue;
            }
            Err(error) => {
                report
                    .failures
                    .push((skill.name.clone(), format!("远端版本检查失败: {error:#}")));
                continue;
            }
            _ => {}
        }
        match push_skill_cli(store, &skill.id, None) {
            Ok(result) if result.pushed => {
                if let Err(error) = refresh_git_snapshot(store, skill) {
                    report.failures.push((
                        skill.name.clone(),
                        format!("推送后保存 revision 失败: {error:#}"),
                    ));
                } else {
                    report.pushed.push(skill.name.clone());
                }
            }
            Ok(result) => report.failures.push((skill.name.clone(), result.message)),
            Err(error) => report
                .failures
                .push((skill.name.clone(), format!("推送失败: {error:#}"))),
        }
    }
    report.stages.push(stage(
        "publish",
        if report.failures.is_empty() {
            "completed"
        } else {
            "partial"
        },
        format!("Pushed {} Skill(s)", report.pushed.len()),
    ));

    let final_profile = synchronize_profile(store, false, false, ConflictStrategy::Abort)?;
    report.failures.extend(final_profile.failures.clone());
    report.stages.push(stage(
        "profile",
        if final_profile.conflicts.is_empty() {
            "completed"
        } else {
            "blocked"
        },
        "Published exact desired state to WebDAV Profile",
    ));
    report.profile = Some(final_profile);

    let config = load_app_config(store)?;
    let webdav = config
        .webdav
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("WebDAV 未配置"))?;
    let backup = export_full_backup(store)?;
    match upload_backup(webdav, &backup) {
        Ok(remote_path) => {
            report.backup_remote_path = Some(remote_path);
            report.stages.push(stage(
                "backup",
                "completed",
                "Uploaded lossless database backup",
            ));
        }
        Err(error) => {
            report.failures.push((
                "webdav-backup".to_string(),
                format!("完整备份上传失败: {error:#}"),
            ));
            report
                .stages
                .push(stage("backup", "failed", "Lossless backup upload failed"));
        }
    }
    report.state = if report
        .profile
        .as_ref()
        .is_some_and(|profile| !profile.conflicts.is_empty())
    {
        "conflict"
    } else if report.failures.is_empty() {
        "completed"
    } else {
        "partial"
    }
    .to_string();
    Ok(report)
}
