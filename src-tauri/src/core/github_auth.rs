//! GitHub token validation, shared by the Tauri command layer and the CLI.

use serde::{Deserialize, Serialize};

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
