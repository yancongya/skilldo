//! One-shot repo-ification and publish for a local (not-yet-Git) skill.
//!
//! This complements `installer::publish_managed_skill_to_remote`, which only
//! handles skills that already live in a Git repository. `repoify_skill` takes a
//! locally-managed skill (living under the central repo) and:
//!
//!   1. `git init` (if it is not already a repository)
//!   2. creates a GitHub repository via the REST API
//!   3. adds the remote and force-pushes the initial commit (`-u`)
//!   4. records the origin so future updates go through `publish_managed_skill`
//!
//! The repository name may be supplied by the caller; otherwise a
//! `RepoNameStrategy` derives it from the skill name. An LLM-backed naming
//! strategy is intentionally left as a reserved hook (see `LlmRepoNameStrategy`)
//! so the UI can later wire in an AI suggestion without touching this module.

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use git2::{Cred, CredentialType, PushOptions, RemoteCallbacks};
use serde::Serialize;

use crate::core::config::PRODUCT_NAME;
use crate::core::content_hash::hash_dir;
use crate::core::skill_store::{SkillOriginRecord, SkillStore};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoifyResult {
    pub skill_id: String,
    pub name: String,
    pub repo_url: String,
    pub commit: Option<String>,
    pub pushed: bool,
}

/// Strategy for deriving a GitHub repository name from a skill.
pub trait RepoNameStrategy {
    fn repo_name(&self, skill_name: &str, suggested: Option<&str>) -> String;
}

/// Default strategy: slugify the skill name (lowercase, alnum + dashes).
pub struct SlugifyStrategy;

impl RepoNameStrategy for SlugifyStrategy {
    fn repo_name(&self, skill_name: &str, suggested: Option<&str>) -> String {
        if let Some(s) = suggested {
            if !s.trim().is_empty() {
                return slugify(s);
            }
        }
        slugify(skill_name)
    }
}

/// Reserved hook for an LLM-backed naming suggestion. Not invoked in this build;
/// a future caller may wire it to a model that returns a preferred repo name,
/// falling back to the slug when the model is unavailable.
pub struct LlmRepoNameStrategy;

impl RepoNameStrategy for LlmRepoNameStrategy {
    fn repo_name(&self, skill_name: &str, suggested: Option<&str>) -> String {
        SlugifyStrategy.repo_name(skill_name, suggested)
    }
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "skill".to_string()
    } else {
        trimmed
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn git(args: &[&str], dir: &Path) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .context("failed to spawn git — is git installed?")?;
    if !out.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn push_with_token(dir: &Path, branch: &str, token: &str) -> Result<()> {
    let repo = git2::Repository::open(dir).context("open repository for authenticated push")?;
    let mut remote = repo.find_remote("origin").context("find origin remote")?;
    let mut callbacks = RemoteCallbacks::new();
    callbacks.credentials(move |_url, _username, allowed| {
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            Cred::userpass_plaintext("x-access-token", token)
        } else {
            Err(git2::Error::from_str(
                "remote did not allow HTTPS token authentication",
            ))
        }
    });
    let mut options = PushOptions::new();
    options.remote_callbacks(callbacks);
    let refspec = format!("HEAD:refs/heads/{branch}");
    remote
        .push(&[refspec.as_str()], Some(&mut options))
        .context("push to origin failed")?;

    let mut config = repo.config().context("open repository config")?;
    config.set_str(&format!("branch.{branch}.remote"), "origin")?;
    config.set_str(
        &format!("branch.{branch}.merge"),
        &format!("refs/heads/{branch}"),
    )?;
    Ok(())
}

/// Resolve the GitHub token used for API calls and publishing.
///
/// Preference order:
///   1. An explicit token passed by the caller (highest priority).
///   2. The token stored in SkillDo's own settings (`github_token`).
///   3. The `gh` CLI login (`gh auth token`) — SkillDo reuses the user's
///      existing authentication instead of requiring a separately managed token.
fn resolve_github_token(store: &SkillStore, explicit: Option<&str>) -> Result<String> {
    if let Some(t) = explicit {
        let t = t.trim();
        if !t.is_empty() {
            return Ok(t.to_string());
        }
    }
    if let Some(stored) = store.get_setting("github_token")? {
        let stored = stored.trim();
        if !stored.is_empty() {
            return Ok(stored.to_string());
        }
    }
    if let Ok(output) = Command::new("gh").args(["auth", "token"]).output() {
        if output.status.success() {
            let gh_token = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !gh_token.is_empty() {
                return Ok(gh_token);
            }
        }
    }
    bail!(
        "GitHub token not found. Set one with `skilldo github token-set --stdin` \
         or authenticate the `gh` CLI with `gh auth login`"
    );
}

fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

fn authenticated_login(token: &str) -> Result<String> {
    let resp = reqwest::blocking::Client::new()
        .get("https://api.github.com/user")
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", PRODUCT_NAME)
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .context("query authenticated GitHub user")?;
    if !resp.status().is_success() {
        bail!(
            "GitHub API returned HTTP {} when resolving the authenticated user",
            resp.status()
        );
    }
    let value: serde_json::Value = resp.json().context("parse /user response")?;
    value
        .get("login")
        .and_then(|l| l.as_str())
        .map(str::to_string)
        .context("GitHub /user response missing login")
}

/// Create a GitHub repository and return its HTTPS clone URL.
///
/// When `owner` is empty or matches the authenticated user, the repository is
/// created under the user account (`POST /user/repos`); otherwise it is created
/// under the given organization (`POST /orgs/{owner}/repos`).
pub fn create_repo(token: &str, owner: &str, name: &str, private: bool) -> Result<String> {
    let login = authenticated_login(token)?;
    let (endpoint, body_owner): (&str, Option<&str>) = if owner.is_empty() || owner == login {
        ("https://api.github.com/user/repos", None)
    } else {
        (
            &format!("https://api.github.com/orgs/{}/repos", owner),
            Some(owner),
        )
    };

    let mut body = serde_json::json!({
        "name": name,
        "private": private,
        "auto_init": false,
    });
    if let Some(o) = body_owner {
        body["org"] = serde_json::json!(o);
    }

    let resp = reqwest::blocking::Client::new()
        .post(endpoint)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", PRODUCT_NAME)
        .header("Accept", "application/vnd.github+json")
        .timeout(std::time::Duration::from_secs(20))
        .json(&body)
        .send()
        .context("create GitHub repository")?;

    let status = resp.status();
    if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY {
        // 422: repository likely already exists — reuse the canonical clone URL.
        let effective = if owner.is_empty() { &login } else { owner };
        return Ok(format!("https://github.com/{}/{}.git", effective, name));
    }
    if !status.is_success() {
        let text = resp.text().unwrap_or_default();
        bail!("create_repo failed (HTTP {}): {}", status, text);
    }
    let value: serde_json::Value = resp.json().context("parse create_repo response")?;
    value
        .get("clone_url")
        .and_then(|u| u.as_str())
        .map(str::to_string)
        .context("create_repo response missing clone_url")
}

/// Repo-ify a local skill and push it to a freshly created GitHub repository.
pub fn repoify_skill(
    store: &SkillStore,
    skill_id: &str,
    repo_name: Option<&str>,
    owner: Option<&str>,
    private: bool,
    message: Option<&str>,
) -> Result<RepoifyResult> {
    let record = store
        .get_skill_by_id(skill_id)?
        .ok_or_else(|| anyhow::anyhow!("skill not found: {}", skill_id))?;

    let central_path = Path::new(&record.central_path);
    if !central_path.exists() {
        bail!("central path not found: {:?}", central_path);
    }

    let token = resolve_github_token(store, None)?;

    let login = authenticated_login(&token)?;
    let effective_owner = if owner.unwrap_or("").is_empty() {
        login.clone()
    } else {
        owner.unwrap().to_string()
    };
    let strategy = SlugifyStrategy;
    let name = strategy.repo_name(&record.name, repo_name);

    // 1. git init if needed
    if !is_git_repo(central_path) {
        git(&["init"], central_path)?;
        // Best effort: default the initial branch to `main`.
        let _ = git(&["symbolic-ref", "HEAD", "refs/heads/main"], central_path);
    }

    // 2. create the remote repository
    let clone_url = create_repo(&token, &effective_owner, &name, private)?;
    // 3. Configure only the clean remote URL. libgit2 receives the token via an
    // in-memory credentials callback during push, never through argv or config.
    let remotes = git(&["remote"], central_path).unwrap_or_default();
    if remotes.split_whitespace().any(|r| r == "origin") {
        git(&["remote", "set-url", "origin", &clone_url], central_path)?;
    } else {
        git(&["remote", "add", "origin", &clone_url], central_path)?;
    }

    // 4. stage + commit (if there is anything to commit)
    git(&["add", "-A"], central_path)?;
    let has_staged = git(&["diff", "--cached", "--quiet"], central_path).is_err();
    let commit = if has_staged {
        let msg = message
            .filter(|m| !m.trim().is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| format!("Initial commit for skill {}", record.name));
        git(&["commit", "-m", &msg], central_path)?;
        Some(
            git(&["rev-parse", "HEAD"], central_path)?
                .trim()
                .to_string(),
        )
    } else {
        // No staged changes — reuse the current HEAD (must already have a commit).
        Some(
            git(&["rev-parse", "HEAD"], central_path)?
                .trim()
                .to_string(),
        )
    };

    // Determine the actual branch name (handles master/main differences).
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"], central_path)?
        .trim()
        .to_string();
    if branch.is_empty() || branch == "HEAD" {
        bail!("could not determine current branch after commit");
    }

    // 5. Push using an in-memory credentials callback.
    push_with_token(central_path, &branch, &token)?;

    // 6. record the origin + flip the skill's source type so future
    //    `publish_managed_skill` calls take the git-update path.
    store.upsert_skill_origin(&SkillOriginRecord {
        skill_id: record.id.clone(),
        origin_kind: "git".to_string(),
        origin_role: "mine".to_string(),
        provider: Some("github".to_string()),
        remote_url: Some(clone_url.clone()),
        owner: Some(effective_owner.clone()),
        repo: Some(name.clone()),
        branch: Some(branch.clone()),
        subpath: None,
        update_strategy: "git_pull".to_string(),
        publish_strategy: "git_push".to_string(),
        manual_override: false,
        reason: None,
        updated_at: now_ms(),
    })?;
    store.update_skill_git_baseline(
        &record.id,
        "git",
        &clone_url,
        commit.as_deref(),
        hash_dir(central_path).ok().as_deref(),
    )?;

    Ok(RepoifyResult {
        skill_id: record.id,
        name: record.name,
        repo_url: clone_url,
        commit,
        pushed: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::Signature;

    fn repository_with_commit(path: &Path) -> (git2::Repository, String) {
        let repo = git2::Repository::init(path).unwrap();
        std::fs::write(path.join("README.md"), "test\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let signature = Signature::now("SkillDo Test", "skilldo-test@example.invalid").unwrap();
        repo.commit(Some("HEAD"), &signature, &signature, "test", &tree, &[])
            .unwrap();
        drop(tree);
        let branch = repo.head().unwrap().shorthand().unwrap().to_string();
        (repo, branch)
    }

    #[test]
    fn authenticated_push_keeps_token_out_of_remote_and_branch_config() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let remote_path = temp.path().join("remote.git");
        git2::Repository::init_bare(&remote_path).unwrap();
        let (repo, branch) = repository_with_commit(&source);
        repo.remote("origin", remote_path.to_str().unwrap())
            .unwrap();

        push_with_token(&source, &branch, "test-token").unwrap();

        let remote = repo.find_remote("origin").unwrap();
        assert_eq!(remote.url(), Some(remote_path.to_str().unwrap()));
        let config = repo.config().unwrap();
        assert_eq!(
            config
                .get_string(&format!("branch.{branch}.remote"))
                .unwrap(),
            "origin"
        );
        assert_eq!(
            config
                .get_string(&format!("branch.{branch}.merge"))
                .unwrap(),
            format!("refs/heads/{branch}")
        );
        let pushed = git2::Repository::open_bare(remote_path).unwrap();
        assert!(pushed
            .find_reference(&format!("refs/heads/{branch}"))
            .is_ok());
    }

    #[test]
    fn failed_authenticated_push_keeps_token_out_of_remote_config_and_error() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let missing_remote = temp.path().join("missing.git");
        let (repo, branch) = repository_with_commit(&source);
        repo.remote("origin", missing_remote.to_str().unwrap())
            .unwrap();

        let error = push_with_token(&source, &branch, "test-token").unwrap_err();

        assert!(!format!("{error:#}").contains("test-token"));
        assert_eq!(
            repo.find_remote("origin").unwrap().url(),
            Some(missing_remote.to_str().unwrap())
        );
    }
}
