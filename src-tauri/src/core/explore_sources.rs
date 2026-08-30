use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::expand_home_path;
use super::skill_store::SkillStore;
use super::skills_search::{search_skills_online_with_base, OnlineSkillResult};

use super::config::FEATURED_SKILLS_URL;

const EXPLORE_SOURCES_KEY: &str = "explore_sources_v1";

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

/// Result of fetching skills from all enabled explore sources.
///
/// `errors` carries a human-readable message per source that failed to load so
/// the UI can surface partial failures instead of silently dropping them.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExploreFetchResult {
    pub skills: Vec<ExploreSkill>,
    pub errors: Vec<String>,
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

/// Fetch skills from every enabled source in parallel, deduplicate across
/// sources, and truncate to `limit`. Failures from individual sources are
/// collected into `ExploreFetchResult::errors` rather than swallowed.
pub fn get_explore_skills(
    store: &SkillStore,
    query: Option<&str>,
    limit: usize,
) -> Result<ExploreFetchResult> {
    let sources = get_explore_sources(store)?;
    let query = query.unwrap_or("").trim().to_string();
    let limit = limit.clamp(1, 100);
    let enabled: Vec<ExploreSourceConfig> = sources.into_iter().filter(|s| s.enabled).collect();
    if enabled.is_empty() {
        return Ok(ExploreFetchResult::default());
    }

    let client = Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .context("build explore source client")?;
    // `SkillStore` is cheap to clone (it only holds the db path) and is `Send +
    // Sync`, so each worker thread gets its own clone for isolated reads.
    let store = Arc::new(store.clone());

    let mut handles = Vec::with_capacity(enabled.len());
    for source in enabled {
        let store = Arc::clone(&store);
        let client = client.clone();
        let query = query.clone();
        handles.push(std::thread::spawn(move || {
            fetch_source_skills(&store, &client, &source, &query, limit)
                .map_err(|err| format!("{}: {:#}", source.name, err))
        }));
    }

    let mut errors = Vec::new();
    let mut collected: Vec<ExploreSkill> = Vec::new();
    for handle in handles {
        match handle.join() {
            Ok(Ok(skills)) => collected.extend(skills),
            Ok(Err(err)) => errors.push(err),
            Err(_) => errors.push("a source worker thread panicked".to_string()),
        }
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for skill in collected {
        let key = format!(
            "{}|{}",
            skill.name.to_lowercase(),
            normalize_source_url(&skill.source_url)
        );
        if seen.insert(key) {
            out.push(skill);
            if out.len() >= limit {
                break;
            }
        }
    }

    Ok(ExploreFetchResult {
        skills: out,
        errors,
    })
}

fn fetch_source_skills(
    store: &SkillStore,
    client: &Client,
    source: &ExploreSourceConfig,
    query: &str,
    limit: usize,
) -> Result<Vec<ExploreSkill>> {
    let mut skills = match source.kind.as_str() {
        "skills_sh" => {
            if query.len() < 2 {
                Vec::new()
            } else {
                search_skills_online_with_base(&source.endpoint, query, limit)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|item| skill_from_online(item, source))
                    .collect()
            }
        }
        "featured_json" | "json_index" => fetch_index_source(store, client, source)?,
        "git_index" => fetch_git_index_source(store, source)?,
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

    Ok(skills)
}

fn default_sources() -> Vec<ExploreSourceConfig> {
    vec![
        ExploreSourceConfig {
            id: "official-featured".to_string(),
            name: "Skills Hub Official".to_string(),
            kind: "featured_json".to_string(),
            endpoint: FEATURED_SKILLS_URL.to_string(),
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

/// Fetch a JSON index source with a three-stage fallback:
/// live fetch -> cached body in the settings table -> bundled featured data
/// (only for the official source). Returns an error when nothing can be loaded
/// so the caller can surface it instead of showing an empty list silently.
fn fetch_index_source(
    store: &SkillStore,
    client: &Client,
    source: &ExploreSourceConfig,
) -> Result<Vec<ExploreSkill>> {
    let cache_key = format!("explore_source_cache_{}", source.id);
    match read_endpoint(client, &source.endpoint) {
        Ok(body) => {
            if let Ok(skills) = parse_index_json(&body, source) {
                if !skills.is_empty() {
                    let _ = store.set_setting(&cache_key, &body);
                    return Ok(skills);
                }
            }
            // Fetched but empty/unparseable: fall through to cache.
            log::warn!(
                "[explore] index for {} returned no usable skills",
                source.id
            );
        }
        Err(err) => {
            log::warn!("[explore] fetch failed for {}: {:#}", source.id, err);
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
        return parse_index_json(include_str!("../../../featured-skills.json"), source);
    }

    anyhow::bail!(
        "failed to load explore source '{}' (no live data and no cache)",
        source.name
    );
}

/// Fetch a `git_index` source by cloning/pulling the repo into a local cache
/// and reading its bundled index file, then falling back to the cached body
/// (and the bundled featured data for the official source).
fn fetch_git_index_source(
    store: &SkillStore,
    source: &ExploreSourceConfig,
) -> Result<Vec<ExploreSkill>> {
    let cache_key = format!("explore_source_cache_{}", source.id);
    let cache_dir = std::env::temp_dir()
        .join("skills-hub-explore")
        .join(sanitize_id(&source.id));
    let _ = std::fs::create_dir_all(&cache_dir);

    let mut fetched = false;
    match super::git_fetcher::clone_or_pull(&source.endpoint, &cache_dir, None, None) {
        Ok(_) => {
            if let Some(body) = read_git_index_file(&cache_dir) {
                if let Ok(skills) = parse_index_json(&body, source) {
                    if !skills.is_empty() {
                        let _ = store.set_setting(&cache_key, &body);
                        return Ok(skills);
                    }
                }
                fetched = true;
            }
        }
        Err(err) => {
            log::warn!(
                "[explore] git_index clone/pull failed for {}: {:#}",
                source.id,
                err
            );
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
        return parse_index_json(include_str!("../../../featured-skills.json"), source);
    }

    if fetched {
        anyhow::bail!(
            "git repo for '{}' cloned but no index file was found",
            source.name
        );
    }
    anyhow::bail!(
        "failed to load git explore source '{}' (clone failed and no cache)",
        source.name
    );
}

fn read_git_index_file(dir: &Path) -> Option<String> {
    let candidates = [
        "featured-skills.json",
        "skills.json",
        "index.json",
        "skills/index.json",
    ];
    for candidate in candidates {
        let path = dir.join(candidate);
        if path.is_file() {
            if let Ok(body) = fs::read_to_string(&path) {
                return Some(body);
            }
        }
    }
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext.eq_ignore_ascii_case("json") {
                        if let Ok(body) = fs::read_to_string(&path) {
                            return Some(body);
                        }
                    }
                }
            }
        }
    }
    None
}

/// Replace any path-hostile characters so a source id is safe to use as a
/// directory name inside the local cache.
fn sanitize_id(id: &str) -> String {
    id.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
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
    // Local file: supports plain absolute paths, `file://`, and `~/`.
    let path_input = endpoint.strip_prefix("file://").unwrap_or(endpoint);
    let path = expand_home_path(path_input).context("expand explore source path")?;
    fs::read_to_string(&path).with_context(|| format!("read local explore source {:?}", path))
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

/// Normalize a source URL value.
///
/// - Full http(s) URLs and non-GitHub protocols (git@, ssh://, file://) are
///   preserved as-is.
/// - A two-segment `owner/repo`-style slug (no dots, no scheme) is treated as
///   a GitHub repo and expanded to a full URL.
/// - Anything else (e.g. a full GitLab path `gitlab.com/group/project`, a bare
///   slug) is returned unchanged instead of being wrongly rewritten to GitHub.
fn normalize_source_value(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches(".git");
    if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("git@")
        || trimmed.starts_with("ssh://")
        || trimmed.starts_with("file://")
    {
        return trimmed.to_string();
    }
    let segments: Vec<&str> = trimmed.split('/').collect();
    if segments.len() == 2
        && !trimmed.contains('.')
        && !trimmed.contains(':')
        && segments.iter().all(|s| !s.is_empty())
    {
        return format!("https://github.com/{}", trimmed);
    }
    trimmed.to_string()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    use tempfile::tempdir;

    fn test_source(id: &str) -> ExploreSourceConfig {
        ExploreSourceConfig {
            id: id.to_string(),
            name: id.to_string(),
            kind: "json_index".to_string(),
            endpoint: String::new(),
            enabled: true,
            builtin: false,
        }
    }

    /// Built-in sources fully disabled, so `get_explore_skills` only touches the
    /// sources we add below — keeping the tests offline and deterministic.
    fn disabled_builtins() -> Vec<ExploreSourceConfig> {
        vec![
            ExploreSourceConfig {
                id: "official-featured".to_string(),
                name: String::new(),
                kind: String::new(),
                endpoint: String::new(),
                enabled: false,
                builtin: true,
            },
            ExploreSourceConfig {
                id: "skills-sh".to_string(),
                name: String::new(),
                kind: String::new(),
                endpoint: String::new(),
                enabled: false,
                builtin: true,
            },
            ExploreSourceConfig {
                id: "prompthub-json".to_string(),
                name: String::new(),
                kind: String::new(),
                endpoint: String::new(),
                enabled: false,
                builtin: true,
            },
            ExploreSourceConfig {
                id: "private-json-index".to_string(),
                name: String::new(),
                kind: String::new(),
                endpoint: String::new(),
                enabled: false,
                builtin: true,
            },
        ]
    }

    #[test]
    fn parse_index_supports_skills_items_and_data() {
        let src = test_source("s");
        let a = parse_index_json(
            r#"{"skills":[{"name":"a","source_url":"https://github.com/x/a"}]}"#,
            &src,
        )
        .unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].name, "a");

        let b = parse_index_json(r#"{"items":[{"title":"b","url":"owner/b"}]}"#, &src).unwrap();
        assert_eq!(b[0].name, "b");
        assert_eq!(b[0].source_url, "https://github.com/owner/b");

        let c = parse_index_json(
            r#"{"data":[{"slug":"c","repo":"https://gitlab.com/g/c"}]}"#,
            &src,
        )
        .unwrap();
        assert_eq!(c[0].name, "c");
        assert_eq!(c[0].source_url, "https://gitlab.com/g/c");
    }

    #[test]
    fn parse_index_skips_items_without_source_url() {
        let src = test_source("s");
        let skills = parse_index_json(
            r#"{"skills":[{"name":"a","source_url":"https://github.com/x/a"},{"name":"bad"}]}"#,
            &src,
        )
        .unwrap();
        assert_eq!(skills.len(), 1);
    }

    #[test]
    fn normalize_source_url_strips_github_prefix_and_tree() {
        assert_eq!(
            normalize_source_url("https://github.com/owner/repo"),
            "owner/repo"
        );
        assert_eq!(
            normalize_source_url("https://github.com/owner/repo.git"),
            "owner/repo"
        );
        assert_eq!(
            normalize_source_url("https://github.com/owner/repo/tree/main"),
            "owner/repo"
        );
    }

    #[test]
    fn normalize_source_value_only_expands_github_slugs() {
        assert_eq!(
            normalize_source_value("https://gitlab.com/g/p"),
            "https://gitlab.com/g/p"
        );
        assert_eq!(
            normalize_source_value("owner/repo"),
            "https://github.com/owner/repo"
        );
        assert_eq!(
            normalize_source_value("owner/repo.git"),
            "https://github.com/owner/repo"
        );
        // Three-segment or dotted paths are not rewritten to GitHub.
        assert_eq!(normalize_source_value("gitlab.com/g/p"), "gitlab.com/g/p");
        assert_eq!(
            normalize_source_value("https://github.com/x/y"),
            "https://github.com/x/y"
        );
        assert_eq!(
            normalize_source_value("git@github.com:x/y.git"),
            "git@github.com:x/y"
        );
    }

    #[test]
    fn merge_saved_sources_keeps_builtin_and_custom() {
        let saved = vec![
            ExploreSourceConfig {
                id: "official-featured".to_string(),
                name: String::new(),
                kind: String::new(),
                endpoint: String::new(),
                enabled: false,
                builtin: false,
            },
            ExploreSourceConfig {
                id: "custom-1".to_string(),
                name: "My".to_string(),
                kind: "json_index".to_string(),
                endpoint: "https://x/y.json".to_string(),
                enabled: true,
                builtin: false,
            },
        ];
        let merged = merge_saved_sources(saved);
        let official = merged.iter().find(|s| s.id == "official-featured").unwrap();
        assert!(official.builtin);
        assert_eq!(official.name, "Skills Hub Official");
        assert_eq!(official.kind, "featured_json");
        assert!(!official.enabled);
        assert!(merged.iter().any(|s| s.id == "custom-1"));
    }

    #[test]
    fn get_explore_skills_reads_local_and_deduplicates() {
        let dir = tempdir().unwrap();
        let idx = dir.path().join("skills.json");
        let mut f = fs::File::create(&idx).unwrap();
        write!(
            f,
            r#"{{"skills":[{{"name":"alpha","source_url":"https://github.com/x/alpha"}},{{"name":"beta","source_url":"https://github.com/x/beta"}}]}}"#
        )
        .unwrap();
        drop(f);

        let store = SkillStore::new(dir.path().join("db.sqlite"));
        store.ensure_schema().unwrap();
        let endpoint = idx.to_str().unwrap().to_string();
        let mut sources = disabled_builtins();
        sources.push(ExploreSourceConfig {
            id: "a".to_string(),
            name: "A".to_string(),
            kind: "json_index".to_string(),
            endpoint: endpoint.clone(),
            enabled: true,
            builtin: false,
        });
        sources.push(ExploreSourceConfig {
            id: "b".to_string(),
            name: "B".to_string(),
            kind: "json_index".to_string(),
            endpoint,
            enabled: true,
            builtin: false,
        });
        save_explore_sources(&store, &sources).unwrap();

        let result = get_explore_skills(&store, None, 100).unwrap();
        assert_eq!(
            result.skills.len(),
            2,
            "identical skills from 2 sources should dedupe to 2"
        );
        assert!(result.errors.is_empty());
    }

    #[test]
    fn get_explore_skills_reports_source_errors() {
        let dir = tempdir().unwrap();
        let good = dir.path().join("good.json");
        let mut f = fs::File::create(&good).unwrap();
        write!(
            f,
            r#"{{"skills":[{{"name":"alpha","source_url":"https://github.com/x/alpha"}}]}}"#
        )
        .unwrap();
        drop(f);

        let store = SkillStore::new(dir.path().join("db.sqlite"));
        store.ensure_schema().unwrap();
        let mut sources = disabled_builtins();
        sources.push(ExploreSourceConfig {
            id: "good".to_string(),
            name: "Good".to_string(),
            kind: "json_index".to_string(),
            endpoint: good.to_str().unwrap().to_string(),
            enabled: true,
            builtin: false,
        });
        sources.push(ExploreSourceConfig {
            id: "bad".to_string(),
            name: "Bad".to_string(),
            kind: "json_index".to_string(),
            endpoint: "file:///this/does/not/exist.json".to_string(),
            enabled: true,
            builtin: false,
        });
        save_explore_sources(&store, &sources).unwrap();

        let result = get_explore_skills(&store, None, 100).unwrap();
        assert_eq!(result.skills.len(), 1);
        assert!(
            !result.errors.is_empty(),
            "failed source should be reported"
        );
    }
}
