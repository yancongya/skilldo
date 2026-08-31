//! GitHub token validation, shared by the Tauri command layer and the CLI.

use serde::{Deserialize, Serialize};

use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap};

use crate::core::config::PRODUCT_NAME;

/// Result of validating a GitHub personal access token.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubTokenStatus {
    pub valid: bool,
    pub login: Option<String>,
    pub scopes: Vec<String>,
    pub error: Option<String>,
}

/// Validate a GitHub token by calling the authenticated `/user` endpoint and
/// reading the `x-oauth-scopes` response header. Pure network + parse logic so
/// it can run from both the Tauri runtime and the headless CLI.
pub fn compute_github_token_status(token: String) -> GithubTokenStatus {
    if token.is_empty() {
        return GithubTokenStatus {
            valid: false,
            login: None,
            scopes: vec![],
            error: Some("token 为空".to_string()),
        };
    }
    let resp = reqwest::blocking::Client::new()
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", PRODUCT_NAME)
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send();
    match resp {
        Ok(response) => {
            let status = response.status();
            let scopes: Vec<String> = response
                .headers()
                .get("x-oauth-scopes")
                .and_then(|v| v.to_str().ok())
                .map(|s| {
                    s.split(',')
                        .map(|x| x.trim().to_string())
                        .filter(|x| !x.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            if status == 401 || status == 403 {
                GithubTokenStatus {
                    valid: false,
                    login: None,
                    scopes,
                    error: Some(format!("GitHub 拒绝该 token（HTTP {}）", status)),
                }
            } else if status.is_success() {
                let login = response.json::<serde_json::Value>().ok().and_then(|v| {
                    v.get("login")
                        .and_then(|l| l.as_str())
                        .map(|s| s.to_string())
                });
                GithubTokenStatus {
                    valid: true,
                    login,
                    scopes,
                    error: None,
                }
            } else {
                GithubTokenStatus {
                    valid: false,
                    login: None,
                    scopes,
                    error: Some(format!("GitHub 返回 HTTP {}", status)),
                }
            }
        }
        Err(e) => GithubTokenStatus {
            valid: false,
            login: None,
            scopes: vec![],
            error: Some(format!("网络错误: {}", e)),
        },
    }
}

// ---------------------------------------------------------------------------
// List unique owners from the authenticated user's repositories
// ---------------------------------------------------------------------------

/// A single owner/org discovered from the user's GitHub repos.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubOwnerEntry {
    pub login: String,
    pub repo_count: usize,
    /// Avatar URL for displaying in the UI.
    pub avatar_url: Option<String>,
}

/// Fetch the authenticated user's repos (paginated, up to 100 per page) and
/// extract unique owner/org logins with their repo counts.
pub fn list_github_owners(token: String) -> anyhow::Result<Vec<GithubOwnerEntry>> {
    let client = reqwest::blocking::Client::new();
    let mut owners: BTreeSet<String> = BTreeSet::new();
    let mut repo_counts: HashMap<String, usize> = HashMap::new();
    let mut avatar_urls: HashMap<String, Option<String>> = HashMap::new();

    // Fetch up to 300 repos (3 pages of 100) to cover most personal accounts.
    for page in 1..=3 {
        let resp = client
            .get("https://api.github.com/user/repos")
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", PRODUCT_NAME)
            .header("Accept", "application/vnd.github+json")
            .query(&[("per_page", "100"), ("page", &page.to_string())])
            .query(&[("affiliation", "owner,organization_member")])
            .timeout(std::time::Duration::from_secs(15))
            .send()?;

        if !resp.status().is_success() {
            anyhow::bail!("GitHub API returned HTTP {}", resp.status());
        }

        let repos: Vec<serde_json::Value> = resp.json()?;
        if repos.is_empty() {
            break;
        }

        for repo in &repos {
            let owner_login = repo
                .get("owner")
                .and_then(|o| o.get("login"))
                .and_then(|l| l.as_str());
            let owner_type = repo
                .get("owner")
                .and_then(|o| o.get("type"))
                .and_then(|t| t.as_str())
                .unwrap_or("User");

            if let Some(login) = owner_login {
                let key = login.to_string();
                owners.insert(key.clone());
                *repo_counts.entry(key.clone()).or_insert(0) += 1;
                // Prefer User avatar_url, but don't overwrite with Organization URL
                avatar_urls.entry(key).or_insert_with(|| {
                    repo.get("owner")
                        .and_then(|o| o.get("avatar_url"))
                        .and_then(|a| a.as_str())
                        .map(str::to_string)
                });
                // If owner is an Organization, also include its repos count under
                // a separate "orgs" concept. For now we just list all unique owners.
                if owner_type == "Organization" {
                    // Organizations might own repos the user has access to but
                    // doesn't "own". We still list them since the user might want
                    // to classify them.
                }
            }
        }
    }

    let mut result: Vec<GithubOwnerEntry> = owners
        .into_iter()
        .map(|login| GithubOwnerEntry {
            repo_count: repo_counts.get(&login).copied().unwrap_or(0),
            avatar_url: avatar_urls.remove(&login).flatten(),
            login,
        })
        .collect();
    result.sort_by_key(|entry| Reverse(entry.repo_count));
    Ok(result)
}
