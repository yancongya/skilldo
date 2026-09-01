use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use git2::{Repository, StatusOptions};
use serde::{Deserialize, Serialize};

use super::skill_store::SkillStore;
use super::tool_adapters::{
    default_tool_adapters, project_relative_skills_dir, supports_project_scope,
};

pub const RECENT_PROJECTS_SETTING: &str = "recent_projects_v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRepositoryInfo {
    pub root: String,
    pub remote_url: Option<String>,
    pub branch: Option<String>,
    pub revision: Option<String>,
    pub dirty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSkillEntry {
    pub name: String,
    pub path: String,
    pub relative_dir: String,
    pub repository_subpath: Option<String>,
    pub tracked: bool,
    pub tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSkillInventory {
    pub project_path: String,
    pub repository: Option<ProjectRepositoryInfo>,
    pub skills: Vec<ProjectSkillEntry>,
}

pub fn normalize_repository_url(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .trim_start_matches("git+")
        .trim_start_matches("ssh://")
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("www.")
        .trim_end_matches(".git")
        .replace('\\', "/")
        .replace("git@github.com:", "github.com/")
}

fn inspect_repository(project_root: &Path) -> Result<Option<(Repository, PathBuf)>> {
    let Ok(repository) = Repository::discover(project_root) else {
        return Ok(None);
    };
    let workdir = repository
        .workdir()
        .context("项目位于裸 Git 仓库中")?
        .canonicalize()
        .context("解析 Git 工作树根目录失败")?;
    if !project_root.starts_with(&workdir) {
        return Ok(None);
    }
    Ok(Some((repository, workdir)))
}

pub fn inspect_project(project_path: &Path) -> Result<ProjectSkillInventory> {
    let project_root = project_path
        .canonicalize()
        .with_context(|| format!("项目路径不存在: {:?}", project_path))?;
    if !project_root.is_dir() {
        anyhow::bail!("项目路径不是目录: {:?}", project_root);
    }

    let repository = inspect_repository(&project_root)?;
    let repository_info = if let Some((repo, workdir)) = repository.as_ref() {
        let remote_url = repo
            .find_remote("origin")
            .ok()
            .and_then(|remote| remote.url().map(str::to_string));
        let head = repo.head().ok();
        let branch = head
            .as_ref()
            .and_then(|value| value.shorthand().map(str::to_string));
        let revision = head
            .as_ref()
            .and_then(|value| value.target())
            .map(|oid| oid.to_string());
        let mut options = StatusOptions::new();
        options
            .include_untracked(true)
            .recurse_untracked_dirs(true)
            .exclude_submodules(true);
        let dirty = repo
            .statuses(Some(&mut options))
            .map(|statuses| !statuses.is_empty())
            .unwrap_or(false);
        Some(ProjectRepositoryInfo {
            root: workdir.to_string_lossy().to_string(),
            remote_url,
            branch,
            revision,
            dirty,
        })
    } else {
        None
    };

    let mut dirs: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for adapter in default_tool_adapters() {
        if supports_project_scope(&adapter) {
            let relative = project_relative_skills_dir(&adapter).to_string();
            dirs.entry(relative)
                .or_default()
                .insert(adapter.id.as_key().to_string());
        }
    }

    let repository_root = repository.as_ref().map(|(_, workdir)| workdir.as_path());
    let mut entries = Vec::new();
    for (relative, tools) in dirs {
        let skills_dir = project_root.join(&relative);
        let Ok(children) = std::fs::read_dir(&skills_dir) else {
            continue;
        };
        for child in children.flatten() {
            let skill_path = child.path();
            if !skill_path.is_dir() || !skill_path.join("SKILL.md").is_file() {
                continue;
            }
            let repository_subpath = repository_root.and_then(|root| {
                skill_path
                    .strip_prefix(root)
                    .ok()
                    .map(|path| path.to_string_lossy().replace('\\', "/"))
            });
            let tracked = repository_subpath.as_deref().is_some_and(|subpath| {
                repository.as_ref().is_some_and(|(repo, _)| {
                    let index_path = Path::new(subpath).join("SKILL.md");
                    repo.index()
                        .ok()
                        .is_some_and(|index| index.get_path(&index_path, 0).is_some())
                })
            });
            entries.push(ProjectSkillEntry {
                name: child.file_name().to_string_lossy().to_string(),
                path: skill_path.to_string_lossy().to_string(),
                relative_dir: relative.clone(),
                repository_subpath,
                tracked,
                tools: tools.iter().cloned().collect(),
            });
        }
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name).then(left.path.cmp(&right.path)));

    Ok(ProjectSkillInventory {
        project_path: project_root.to_string_lossy().to_string(),
        repository: repository_info,
        skills: entries,
    })
}

pub fn known_project_paths(store: &SkillStore) -> Result<Vec<String>> {
    let mut paths = BTreeSet::new();
    if let Some(raw) = store.get_setting(RECENT_PROJECTS_SETTING)? {
        if let Ok(recent) = serde_json::from_str::<Vec<String>>(&raw) {
            paths.extend(recent.into_iter().filter(|path| !path.trim().is_empty()));
        }
    }
    for skill in store.list_skills()? {
        for target in store.list_skill_targets(&skill.id)? {
            if target.scope == "project" {
                if let Some(path) = target.project_path {
                    if !path.trim().is_empty() {
                        paths.insert(path);
                    }
                }
            }
        }
    }
    Ok(paths.into_iter().collect())
}

pub fn remember_project_path(store: &SkillStore, project_path: &Path) -> Result<Vec<String>> {
    let normalized = project_path
        .canonicalize()
        .with_context(|| format!("项目路径不存在: {:?}", project_path))?
        .to_string_lossy()
        .to_string();
    let mut recent = store
        .get_setting(RECENT_PROJECTS_SETTING)?
        .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
        .unwrap_or_default();
    recent.retain(|path| path != &normalized);
    recent.insert(0, normalized);
    recent.truncate(20);
    store.set_setting(RECENT_PROJECTS_SETTING, &serde_json::to_string(&recent)?)?;
    Ok(recent)
}

pub fn known_project_inventories(store: &SkillStore) -> Result<Vec<ProjectSkillInventory>> {
    let mut inventories = Vec::new();
    for path in known_project_paths(store)? {
        if let Ok(inventory) = inspect_project(Path::new(&path)) {
            inventories.push(inventory);
        }
    }
    Ok(inventories)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspects_project_skills_relative_to_parent_repository() {
        let temp = tempfile::tempdir().unwrap();
        let repo = Repository::init(temp.path()).unwrap();
        repo.remote("origin", "git@github.com:example/project.git")
            .unwrap();
        let skill = temp.path().join(".agents/skills/project-ops");
        std::fs::create_dir_all(&skill).unwrap();
        std::fs::write(
            skill.join("SKILL.md"),
            "---\nname: project-ops\ndescription: Project operations.\n---\n",
        )
        .unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_path(Path::new(".agents/skills/project-ops/SKILL.md"))
            .unwrap();
        index.write().unwrap();

        let inventory = inspect_project(temp.path()).unwrap();
        let repository = inventory.repository.unwrap();
        assert_eq!(
            repository.remote_url.as_deref(),
            Some("git@github.com:example/project.git")
        );
        assert!(inventory.skills[0].tracked);
        assert!(repository.dirty);
        assert_eq!(inventory.skills.len(), 1);
        assert_eq!(
            inventory.skills[0].repository_subpath.as_deref(),
            Some(".agents/skills/project-ops")
        );
    }

    #[test]
    fn repository_url_normalization_ignores_github_transport() {
        assert_eq!(
            normalize_repository_url("git@github.com:Example/Project.git"),
            normalize_repository_url("https://github.com/example/project")
        );
    }
}
