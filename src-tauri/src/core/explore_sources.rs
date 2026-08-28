use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::skill_store::SkillStore;
use super::skills_search::{search_skills_online_with_base, OnlineSkillResult};

const EXPLORE_SOURCES_KEY: &str = "explore_sources_v1";
const DEFAULT_FEATURED_URL: &str =
    "https://raw.githubusercontent.com/qufei1993/skills-hub/main/featured-skills.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreSourceConfig {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub endpoint: String,
    pub enabled: bool,
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExploreSkill {
    pub id: String,
    pub name: String,
    pub summary: String,
    pub source_url: String,
    pub source_name: String,
    pub source_kind: String,
    pub downloads: u64,
    pub stars: u64,
}

pub fn get_explore_sources(store: &SkillStore) -> Result<Vec<ExploreSourceConfig>> {
    let mut sources = default_sources();
    if let Some(raw) = store.get_setting(EXPLORE_SOURCES_KEY)? {
        if let Ok(saved) = serde_json::from_str::<Vec<ExploreSourceConfig>>(&raw) {
            sources = merge_saved_sources(saved);
        }
    }
    Ok(sources)
}

pub fn save_explore_sources(
    store: &SkillStore,
    sources: &[ExploreSourceConfig],
) -> Result<Vec<ExploreSourceConfig>> {
    let normalized = merge_saved_sources(sources.to_vec());
    store.set_setting(EXPLORE_SOURCES_KEY, &serde_json::to_string(&normalized)?)?;
    Ok(normalized)
}

pub fn get_explore_skills(
    store: &SkillStore,
    query: Option<&str>,
    limit: usize,
) -> Result<Vec<ExploreSkill>> {
    let sources = get_explore_sources(store)?;
    let query = query.unwrap_or("").trim();
    let limit = limit.clamp(1, 100);
    let client = Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .context("build explore source client")?;
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for source in sources.into_iter().filter(|source| source.enabled) {
        let mut skills = match source.kind.as_str() {
            "skills_sh" => {
                if query.len() < 2 {
                    Vec::new()
                } else {
                    search_skills_online_with_base(&source.endpoint, query, limit)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|item| skill_from_online(item, &source))
                        .collect()
                }
            }
            "featured_json" | "json_index" | "git_index" => {
                fetch_index_source(store, &client, &source).unwrap_or_default()
            }
            _ => Vec::new(),
        };

        if !query.is_empty() && source.kind != "skills_sh" {
            let lower = query.to_lowercase();
            skills.retain(|skill| {
                skill.name.to_lowercase().contains(&lower)
                    || skill.summary.to_lowercase().contains(&lower)
                    || skill.source_url.to_lowercase().contains(&lower)
            });
        }

        for skill in skills {
            let key = format!(
                "{}|{}",
                skill.name.to_lowercase(),
                normalize_source_url(&skill.source_url)
            );
            if seen.insert(key) {
                out.push(skill);
            }
            if out.len() >= limit {
                return Ok(out);
            }
        }
    }

    Ok(out)
}

fn default_sources() -> Vec<ExploreSourceConfig> {
    vec![
        ExploreSourceConfig {
            id: "official-featured".to_string(),
            name: "Skills Hub Official".to_string(),
            kind: "featured_json".to_string(),
            endpoint: DEFAULT_FEATURED_URL.to_string(),
            enabled: true,
            builtin: true,
        },
        ExploreSourceConfig {
            id: "skills-sh".to_string(),
            name: "skills.sh".to_string(),
            kind: "skills_sh".to_string(),
            endpoint: "https://skills.sh".to_string(),
            enabled: true,
            builtin: true,
        },
        ExploreSourceConfig {
            id: "prompthub-json".to_string(),
            name: "PromptHub JSON".to_string(),
            kind: "json_index".to_string(),
            endpoint: "https://prompthub.click/skills.json".to_string(),
            enabled: false,
            builtin: true,
        },
        ExploreSourceConfig {
            id: "private-json-index".to_string(),
            name: "Private JSON Index".to_string(),
            kind: "json_index".to_string(),
            endpoint: String::new(),
            enabled: false,
            builtin: false,
        },
    ]
}

fn merge_saved_sources(saved: Vec<ExploreSourceConfig>) -> Vec<ExploreSourceConfig> {
    let mut by_id: HashMap<String, ExploreSourceConfig> = saved
        .into_iter()
        .map(|source| (source.id.clone(), source))
        .collect();
    let mut out = Vec::new();
    for default in default_sources() {
        if let Some(mut source) = by_id.remove(&default.id) {
            source.builtin = default.builtin;
            if source.kind.trim().is_empty() {
                source.kind = default.kind;
            }
            if source.name.trim().is_empty() {
                source.name = default.name;
            }
            out.push(source);
        } else {
            out.push(default);
        }
    }
    out.extend(
        by_id
            .into_values()
            .filter(|source| !source.id.trim().is_empty()),
    );
    out
}

fn fetch_index_source(
    store: &SkillStore,
    client: &Client,
    source: &ExploreSourceConfig,
) -> Result<Vec<ExploreSkill>> {
    let cache_key = format!("explore_source_cache_{}", source.id);
    if let Ok(body) = read_endpoint(client, &source.endpoint) {
        if let Ok(skills) = parse_index_json(&body, source) {
            if !skills.is_empty() {
                let _ = store.set_setting(&cache_key, &body);
                return Ok(skills);
            }
        }
    }
    if let Ok(Some(cached)) = store.get_setting(&cache_key) {
        if let Ok(skills) = parse_index_json(&cached, source) {
            if !skills.is_empty() {
                return Ok(skills);
            }
        }
    }
    if source.id == "official-featured" {
        parse_index_json(include_str!("../../../featured-skills.json"), source)
    } else {
        Ok(Vec::new())
    }
}

fn read_endpoint(client: &Client, endpoint: &str) -> Result<String> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        anyhow::bail!("empty explore source endpoint");
    }
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return client
            .get(endpoint)
            .header("User-Agent", "skills-hub")
            .send()
            .context("fetch explore source")?
            .error_for_status()
            .context("explore source HTTP error")?
            .text()
            .context("read explore source body");
    }
    let path = endpoint
        .strip_prefix("file://")
        .unwrap_or(endpoint)
        .strip_prefix("~/")
        .map(|rest| {
            std::env::var("HOME")
                .map(|home| format!("{}/{}", home, rest))
                .unwrap_or_else(|_| endpoint.to_string())
        })
        .unwrap_or_else(|| endpoint.to_string());
    fs::read_to_string(Path::new(&path)).context("read local explore source")
}

fn parse_index_json(json: &str, source: &ExploreSourceConfig) -> Result<Vec<ExploreSkill>> {
    let value: Value = serde_json::from_str(json).context("parse explore source JSON")?;
    let items = value
        .get("skills")
        .or_else(|| value.get("items"))
        .or_else(|| value.get("data"))
        .unwrap_or(&value);
    let Some(items) = items.as_array() else {
        return Ok(Vec::new());
    };

    Ok(items
        .iter()
        .filter_map(|item| parse_index_item(item, source))
        .collect())
}

fn parse_index_item(item: &Value, source: &ExploreSourceConfig) -> Option<ExploreSkill> {
    let name = string_field(item, &["name", "title", "slug"])?;
    let source_url = string_field(
        item,
        &[
            "source_url",
            "sourceUrl",
            "repository",
            "repo",
            "url",
            "source",
        ],
    )
    .map(normalize_source_value)?;
    if source_url.is_empty() {
        return None;
    }
    let summary = string_field(item, &["summary", "description", "desc"])
        .unwrap_or_default()
        .to_string();
    let slug = string_field(item, &["slug", "id"]).unwrap_or(name);
    Some(ExploreSkill {
        id: format!("{}:{}", source.id, slug),
        name: name.to_string(),
        summary,
        source_url,
        source_name: source.name.clone(),
        source_kind: source.kind.clone(),
        downloads: number_field(item, &["downloads", "installs"]).unwrap_or(0),
        stars: number_field(item, &["stars", "github_stars", "star_count"]).unwrap_or(0),
    })
}

fn skill_from_online(item: OnlineSkillResult, source: &ExploreSourceConfig) -> ExploreSkill {
    ExploreSkill {
        id: format!("{}:{}", source.id, item.source),
        name: item.name,
        summary: String::new(),
        source_url: item.source_url,
        source_name: source.name.clone(),
        source_kind: source.kind.clone(),
        downloads: item.installs,
        stars: 0,
    }
}

fn string_field<'a>(item: &'a Value, fields: &[&str]) -> Option<&'a str> {
    fields.iter().find_map(|field| item.get(field)?.as_str())
}

fn number_field(item: &Value, fields: &[&str]) -> Option<u64> {
    fields.iter().find_map(|field| item.get(field)?.as_u64())
}

fn normalize_source_value(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches(".git");
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else if trimmed.contains('/') {
        format!("https://github.com/{}", trimmed)
    } else {
        trimmed.to_string()
    }
}

fn normalize_source_url(value: &str) -> String {
    value
        .trim()
        .trim_end_matches(".git")
        .replace("https://github.com/", "")
        .replace("http://github.com/", "")
        .split("/tree/")
        .next()
        .unwrap_or(value)
        .to_lowercase()
}
