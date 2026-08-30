//! Combined backup / restore of the full SkillDo state.
//!
//! A single portable blob (`FullBackup`) carries both the unified [`AppConfig`]
//! and the list of managed skills with their install sources, so a machine can
//! be reconstructed end-to-end: settings are re-applied, and every `git` /
//! `package` skill is re-installed from its remote source. Skills of type
//! `local` cannot be synchronized (they live only on the originating machine)
//! and are reported as skipped.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::core::app_config::{load_app_config, save_app_config_impl, AppConfig};
use crate::core::installer::{install_git_skill_cli, install_package_skill_cli};
use crate::core::skill_store::SkillStore;

/// One managed skill captured for backup, with enough info to re-install it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillBackupEntry {
    pub id: String,
    pub name: String,
    pub source_type: String,
    pub source_ref: Option<String>,
    /// Tools this skill is currently synced to.
    #[serde(default)]
    pub targets: Vec<String>,
}

/// The combined, serializable backup blob.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FullBackup {
    pub config: AppConfig,
    pub skills: Vec<SkillBackupEntry>,
    /// Export timestamp (unix seconds) for human reference.
    #[serde(default)]
    pub exported_at: String,
}

/// Outcome of a restore: what was (re)installed, what was skipped, what failed.
#[derive(Debug, Clone, Default)]
pub struct RestoreReport {
    pub installed: Vec<String>,
    pub skipped: Vec<(String, String)>,
    pub failed: Vec<(String, String)>,
}

impl RestoreReport {
    /// One-line human summary, e.g. `installed 3, skipped 1, failed 0`.
    pub fn summary(&self) -> String {
        format!(
            "installed {}, skipped {}, failed {}",
            self.installed.len(),
            self.skipped.len(),
            self.failed.len()
        )
    }
}

/// Build the combined backup blob (pretty JSON) from the current store.
pub fn export_full_backup(store: &SkillStore) -> Result<String> {
    let config = load_app_config(store)?;
    let records = store.list_skills().context("列出已管理技能失败")?;
    let mut skills = Vec::with_capacity(records.len());
    for rec in &records {
        let targets = store
            .list_skill_targets(&rec.id)
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.tool)
            .collect();
        skills.push(SkillBackupEntry {
            id: rec.id.clone(),
            name: rec.name.clone(),
            source_type: rec.source_type.clone(),
            source_ref: rec.source_ref.clone(),
            targets,
        });
    }
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let backup = FullBackup {
        config,
        skills,
        exported_at: secs.to_string(),
    };
    serde_json::to_string_pretty(&backup).context("序列化备份失败")
}

/// Parse a combined backup blob.
pub fn parse_full_backup(raw: &str) -> Result<FullBackup> {
    serde_json::from_str(raw).map_err(|err| anyhow::anyhow!("备份 JSON 解析失败: {err}"))
}

/// Restore a previously exported backup:
/// 1. re-apply the unified config (settings),
/// 2. re-install every `git` / `package` skill from its remote source,
///    skipping `local` skills (not portable) and skills that already exist.
pub fn restore_full_backup(store: &SkillStore, raw: &str) -> Result<RestoreReport> {
    let backup = parse_full_backup(raw)?;
    save_app_config_impl(store, &backup.config)?;

    let existing: Vec<String> = store
        .list_skills()
        .unwrap_or_default()
        .into_iter()
        .map(|r| r.name)
        .collect();

    let mut report = RestoreReport::default();
    for entry in &backup.skills {
        if existing.iter().any(|n| n == &entry.name) {
            report
                .skipped
                .push((entry.name.clone(), "已存在，跳过".to_string()));
            continue;
        }
        match entry.source_type.as_str() {
            "git" => {
                if let Some(url) = &entry.source_ref {
                    match install_git_skill_cli(store, url, Some(entry.name.clone())) {
                        Ok(_) => report.installed.push(entry.name.clone()),
                        Err(e) => report.failed.push((entry.name.clone(), format!("{:#}", e))),
                    }
                } else {
                    report
                        .failed
                        .push((entry.name.clone(), "缺少 git 源地址".to_string()));
                }
            }
            "package" => {
                if let Some(ref_json) = &entry.source_ref {
                    match parse_package_ref(ref_json) {
                        Ok((package, command)) => {
                            match install_package_skill_cli(
                                store,
                                &package,
                                command.as_deref(),
                                Some(entry.name.clone()),
                            ) {
                                Ok(_) => report.installed.push(entry.name.clone()),
                                Err(e) => {
                                    report.failed.push((entry.name.clone(), format!("{:#}", e)))
                                }
                            }
                        }
                        Err(e) => report
                            .failed
                            .push((entry.name.clone(), format!("解析包源失败: {e}"))),
                    }
                } else {
                    report
                        .failed
                        .push((entry.name.clone(), "缺少 package 源".to_string()));
                }
            }
            _ => {
                report
                    .skipped
                    .push((entry.name.clone(), "本地技能无法远程同步".to_string()));
            }
        }
    }
    Ok(report)
}

/// Parse the package source_ref JSON (`{"package":..,"command":..}`).
fn parse_package_ref(raw: &str) -> Result<(String, Option<String>)> {
    #[derive(Deserialize)]
    struct PackageRef {
        package: String,
        #[serde(default)]
        command: Option<String>,
    }
    let parsed: PackageRef =
        serde_json::from_str(raw).map_err(|err| anyhow::anyhow!("package 源格式错误: {err}"))?;
    Ok((parsed.package, parsed.command))
}
