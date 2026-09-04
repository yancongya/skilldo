use std::path::Path;
use std::process::Command;
use std::process::Stdio;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use git2::{FetchOptions, Repository};

use super::cancel_token::CancelToken;

pub fn clone_or_pull(
    repo_url: &str,
    dest: &Path,
    branch: Option<&str>,
    cancel: Option<&CancelToken>,
) -> Result<String> {
    // Prefer the system `git` binary if available. It tends to work better on macOS
    // networks because it respects user git config (proxy/certs) and OS trust store.
    if let Some(git_bin) = resolve_git_bin() {
        let started = Instant::now();
        match clone_or_pull_via_git_cli(repo_url, dest, branch, cancel) {
            Ok(head) => {
                log::info!(
                    "[git_fetcher] git-cli ok (bin={}) {}s url={}",
                    git_bin,
                    started.elapsed().as_secs_f32(),
                    repo_url
                );
                return Ok(head);
            }
            Err(err) => {
                let allow_fallback = std::env::var("SKILLS_HUB_ALLOW_LIBGIT2_FALLBACK")
                    .ok()
                    .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
                    .unwrap_or(false);
                log::warn!(
                    "[git_fetcher] git-cli failed (bin={}) {}s url={} err={:#}",
                    git_bin,
                    started.elapsed().as_secs_f32(),
                    repo_url,
                    err
                );
                if !allow_fallback {
                    anyhow::bail!(
                        "git 命令执行失败（为避免卡死，已停止并不再回退到内置 git）。请检查系统 git/网络/代理；或设置环境变量 SKILLS_HUB_ALLOW_LIBGIT2_FALLBACK=1 允许回退。\n{:#}",
                        err
                    );
                }
                log::warn!(
                    "[git_fetcher] falling back to libgit2 (SKILLS_HUB_ALLOW_LIBGIT2_FALLBACK=1)"
                );
            }
        }
    } else {
        log::info!("[git_fetcher] system git not available; using libgit2");
    }

    let repo = if dest.exists() {
        let repo = Repository::open(dest).with_context(|| format!("open repo at {:?}", dest))?;
        fetch_origin(&repo)?;
        repo
    } else {
        Repository::clone(repo_url, dest)
            .with_context(|| format!("clone {} into {:?}", repo_url, dest))?
    };

    // Best-effort: move working tree HEAD to the fetched remote head (so "pull" actually updates).
    if let Some(branch) = branch {
        if let Ok(obj) = repo.revparse_single(&format!("refs/remotes/origin/{}", branch)) {
            repo.checkout_tree(&obj, None)?;
            repo.set_head_detached(obj.id())?;
        }
    } else {
        let candidates = [
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
            "refs/remotes/origin/master",
        ];
        for r in candidates {
            if let Ok(obj) = repo.revparse_single(r) {
                repo.checkout_tree(&obj, None)?;
                repo.set_head_detached(obj.id())?;
                break;
            }
        }
    }

    let head = repo.head()?.target().context("missing HEAD target")?;
    Ok(head.to_string())
}

pub fn clone_or_pull_sparse(
    repo_url: &str,
    dest: &Path,
    branch: Option<&str>,
    subpath: &str,
    cancel: Option<&CancelToken>,
) -> Result<String> {
    let clean_subpath = subpath.trim_matches('/');
    if clean_subpath.is_empty() {
        anyhow::bail!("sparse checkout path is empty");
    }

    if resolve_git_bin().is_none() {
        anyhow::bail!("system git is required for sparse checkout");
    }

    // Ensure parent exists so `git clone` can create dest.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir {:?}", parent))?;
    }

    if dest.exists() {
        let git_dir = dest.join(".git");
        for lock_name in &["index.lock", "shallow.lock", "HEAD.lock"] {
            let lock_path = git_dir.join(lock_name);
            if lock_path.exists() {
                log::warn!("[git_fetcher] removing stale lock file: {:?}", lock_path);
                let _ = std::fs::remove_file(&lock_path);
            }
        }

        let out = run_cmd_with_timeout(
            {
                let mut cmd = git_cmd();
                cmd.arg("-C").arg(dest).args([
                    "sparse-checkout",
                    "set",
                    "--no-cone",
                    clean_subpath,
                ]);
                cmd
            },
            git_fetch_timeout(),
            format!("git sparse-checkout set {} in {:?}", clean_subpath, dest),
            cancel,
        )?;
        if !out.status.success() {
            anyhow::bail!(
                "git sparse-checkout set failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }

        let out = run_cmd_with_timeout(
            {
                let mut cmd = git_cmd();
                cmd.arg("-C").arg(dest).args(["fetch", "--prune", "origin"]);
                cmd
            },
            git_fetch_timeout(),
            format!("git fetch in {:?}", dest),
            cancel,
        )?;
        if !out.status.success() {
            anyhow::bail!("git fetch failed: {}", String::from_utf8_lossy(&out.stderr));
        }

        if let Some(branch) = branch {
            let out = run_cmd_with_timeout(
                {
                    let mut cmd = git_cmd();
                    cmd.arg("-C").arg(dest).args([
                        "checkout",
                        "-B",
                        branch,
                        &format!("origin/{}", branch),
                    ]);
                    cmd
                },
                git_fetch_timeout(),
                format!("git checkout -B {} in {:?}", branch, dest),
                cancel,
            )?;
            if !out.status.success() {
                anyhow::bail!(
                    "git checkout branch failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        } else {
            let out = run_cmd_with_timeout(
                {
                    let mut cmd = git_cmd();
                    cmd.arg("-C")
                        .arg(dest)
                        .args(["reset", "--hard", "FETCH_HEAD"]);
                    cmd
                },
                git_fetch_timeout(),
                format!("git reset --hard in {:?}", dest),
                cancel,
            )?;
            if !out.status.success() {
                anyhow::bail!(
                    "git reset --hard failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
    } else {
        let mut cmd = git_cmd();
        cmd.arg("clone").args([
            "--depth",
            "1",
            "--filter=blob:none",
            "--sparse",
            "--no-tags",
        ]);
        if let Some(branch) = branch {
            cmd.arg("--branch").arg(branch).arg("--single-branch");
        }
        cmd.arg(repo_url).arg(dest);
        let out = run_cmd_with_timeout(
            cmd,
            git_timeout(),
            format!("git clone {} into {:?}", repo_url, dest),
            cancel,
        )?;
        if !out.status.success() {
            anyhow::bail!("git clone failed: {}", String::from_utf8_lossy(&out.stderr));
        }

        let out = run_cmd_with_timeout(
            {
                let mut cmd = git_cmd();
                cmd.arg("-C").arg(dest).args([
                    "sparse-checkout",
                    "set",
                    "--no-cone",
                    clean_subpath,
                ]);
                cmd
            },
            git_fetch_timeout(),
            format!("git sparse-checkout set {} in {:?}", clean_subpath, dest),
            cancel,
        )?;
        if !out.status.success() {
            anyhow::bail!(
                "git sparse-checkout set failed: {}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
    }

    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("-C").arg(dest).args(["rev-parse", "HEAD"]);
            cmd
        },
        git_fetch_timeout(),
        format!("git rev-parse HEAD in {:?}", dest),
        cancel,
    )?;
    if !out.status.success() {
        anyhow::bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub struct GitPublishResult {
    pub commit: Option<String>,
    pub pushed: bool,
}

pub fn commit_all_and_push(
    repo_dir: &Path,
    branch: Option<&str>,
    message: &str,
) -> Result<GitPublishResult> {
    if resolve_git_bin().is_none() {
        anyhow::bail!("system git is required for publishing skills");
    }

    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("-C").arg(repo_dir).args(["add", "-A"]);
            cmd
        },
        git_fetch_timeout(),
        format!("git add in {:?}", repo_dir),
        None,
    )?;
    if !out.status.success() {
        anyhow::bail!("git add failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("-C")
                .arg(repo_dir)
                .args(["diff", "--cached", "--quiet"]);
            cmd
        },
        git_fetch_timeout(),
        format!("git diff --cached in {:?}", repo_dir),
        None,
    )?;
    if out.status.success() {
        return Ok(GitPublishResult {
            commit: None,
            pushed: false,
        });
    }

    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("-C").arg(repo_dir).args(["commit", "-m", message]);
            cmd
        },
        git_timeout(),
        format!("git commit in {:?}", repo_dir),
        None,
    )?;
    if !out.status.success() {
        anyhow::bail!(
            "git commit failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("-C").arg(repo_dir).args(["rev-parse", "HEAD"]);
            cmd
        },
        git_fetch_timeout(),
        format!("git rev-parse HEAD in {:?}", repo_dir),
        None,
    )?;
    if !out.status.success() {
        anyhow::bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    let commit = String::from_utf8_lossy(&out.stdout).trim().to_string();

    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("-C").arg(repo_dir).arg("push").arg("origin");
            if let Some(branch) = branch {
                cmd.arg(format!("HEAD:{branch}"));
            } else {
                cmd.arg("HEAD");
            }
            cmd
        },
        git_timeout(),
        format!("git push in {:?}", repo_dir),
        None,
    )?;
    if !out.status.success() {
        anyhow::bail!("git push failed: {}", String::from_utf8_lossy(&out.stderr));
    }

    Ok(GitPublishResult {
        commit: Some(commit),
        pushed: true,
    })
}

/// Clone a local Git worktree into `dst` so the consolidated copy keeps a real
/// `.git` connected to the source's remote. This is the correct "complete"
/// migration for Git-backed skills: after this, `skilldo push` can operate on
/// the central copy directly instead of silently losing version history.
pub fn clone_local_repo(src: &Path, dst: &Path) -> Result<()> {
    if resolve_git_bin().is_none() {
        anyhow::bail!("system git not found; cannot preserve Git history while consolidating");
    }
    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("clone").arg("--no-hardlinks").arg(src).arg(dst);
            cmd
        },
        git_timeout(),
        format!("git clone {:?} -> {:?}", src, dst),
        None,
    )?;
    if !out.status.success() {
        anyhow::bail!("git clone failed: {}", String::from_utf8_lossy(&out.stderr));
    }
    Ok(())
}

pub(crate) fn git_timeout() -> Duration {
    let secs = std::env::var("SKILLS_HUB_GIT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(300);
    Duration::from_secs(secs)
}

pub(crate) fn git_fetch_timeout() -> Duration {
    let secs = std::env::var("SKILLS_HUB_GIT_FETCH_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(180);
    Duration::from_secs(secs)
}

static GIT_BIN: OnceLock<Option<String>> = OnceLock::new();

fn resolve_git_bin() -> Option<String> {
    GIT_BIN
        .get_or_init(|| {
            // Allow overriding from environment for debugging / enterprise setups.
            for key in ["SKILLS_HUB_GIT_BIN", "SKILLS_HUB_GIT_PATH"] {
                if let Ok(v) = std::env::var(key) {
                    let v = v.trim().to_string();
                    if !v.is_empty() && git_bin_works(&v) {
                        log::info!("[git_fetcher] using git bin from {}: {}", key, v);
                        return Some(v);
                    }
                }
            }

            // Try PATH lookup first (works in dev; sometimes missing in macOS bundles).
            if git_bin_works("git") {
                log::info!("[git_fetcher] using git bin from PATH: git");
                return Some("git".to_string());
            }

            // Common macOS locations (system git and Homebrew).
            for cand in [
                "/usr/bin/git",
                "/opt/homebrew/bin/git",
                "/usr/local/bin/git",
            ] {
                if git_bin_works(cand) {
                    log::info!("[git_fetcher] using git bin: {}", cand);
                    return Some(cand.to_string());
                }
            }

            log::warn!("[git_fetcher] no usable git binary found");
            None
        })
        .clone()
}

fn git_bin_works(bin: &str) -> bool {
    Command::new(bin)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub(crate) fn git_cmd() -> Command {
    let bin = resolve_git_bin().unwrap_or_else(|| "git".to_string());
    let mut cmd = Command::new(bin);
    // Never block on interactive auth prompts inside a GUI app.
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "echo");
    // Abort stalled HTTPS transfers (helps avoid "spinner forever" on bad networks).
    cmd.env("GIT_HTTP_LOW_SPEED_LIMIT", "1024")
        .env("GIT_HTTP_LOW_SPEED_TIME", "120");
    cmd
}

pub(crate) fn run_cmd_with_timeout(
    mut cmd: Command,
    timeout: Duration,
    context: String,
    cancel: Option<&CancelToken>,
) -> Result<std::process::Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().with_context(|| context.clone())?;
    let start = Instant::now();
    loop {
        if cancel.is_some_and(|c| c.is_cancelled()) {
            let _ = child.kill();
            let _ = child.wait();
            anyhow::bail!("CANCELLED|操作已被用户取消。");
        }

        if start.elapsed() > timeout {
            let _ = child.kill();
            let stderr = child
                .wait_with_output()
                .map(|out| String::from_utf8_lossy(&out.stderr).to_string())
                .unwrap_or_default();
            anyhow::bail!(
                "git 操作超时（{}s）。请检查网络/代理是否可访问 GitHub；也可设置环境变量 SKILLS_HUB_GIT_TIMEOUT_SECS 增大超时。\n{}",
                timeout.as_secs(),
                stderr.trim()
            );
        }

        match child.try_wait() {
            Ok(Some(_)) => return child.wait_with_output().with_context(|| context.clone()),
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(err) => return Err(err).with_context(|| context.clone()),
        }
    }
}

fn clone_or_pull_via_git_cli(
    repo_url: &str,
    dest: &Path,
    branch: Option<&str>,
    cancel: Option<&CancelToken>,
) -> Result<String> {
    // Ensure parent exists so `git clone` can create dest.
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent dir {:?}", parent))?;
    }

    if dest.exists() {
        // Remove stale lock files left by a previously crashed git process.
        let git_dir = dest.join(".git");
        for lock_name in &["index.lock", "shallow.lock", "HEAD.lock"] {
            let lock_path = git_dir.join(lock_name);
            if lock_path.exists() {
                log::warn!("[git_fetcher] removing stale lock file: {:?}", lock_path);
                let _ = std::fs::remove_file(&lock_path);
            }
        }

        // Fetch updates.
        let out = run_cmd_with_timeout(
            {
                let mut cmd = git_cmd();
                cmd.arg("-C").arg(dest).args(["fetch", "--prune", "origin"]);
                cmd
            },
            git_fetch_timeout(),
            format!("git fetch in {:?}", dest),
            cancel,
        )?;
        if !out.status.success() {
            anyhow::bail!("git fetch failed: {}", String::from_utf8_lossy(&out.stderr));
        }

        // Move local HEAD to fetched commit.
        if let Some(branch) = branch {
            let out = run_cmd_with_timeout(
                {
                    let mut cmd = git_cmd();
                    cmd.arg("-C").arg(dest).args([
                        "checkout",
                        "-B",
                        branch,
                        &format!("origin/{}", branch),
                    ]);
                    cmd
                },
                git_fetch_timeout(),
                format!("git checkout -B {} in {:?}", branch, dest),
                cancel,
            )?;
            if !out.status.success() {
                anyhow::bail!(
                    "git checkout branch failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        } else {
            let out = run_cmd_with_timeout(
                {
                    let mut cmd = git_cmd();
                    cmd.arg("-C")
                        .arg(dest)
                        .args(["reset", "--hard", "FETCH_HEAD"]);
                    cmd
                },
                git_fetch_timeout(),
                format!("git reset --hard in {:?}", dest),
                cancel,
            )?;
            if !out.status.success() {
                anyhow::bail!(
                    "git reset --hard failed: {}",
                    String::from_utf8_lossy(&out.stderr)
                );
            }
        }
    } else {
        // Clone.
        let mut cmd = git_cmd();
        cmd.arg("clone")
            .args(["--depth", "1", "--filter=blob:none", "--no-tags"]);
        if let Some(branch) = branch {
            cmd.arg("--branch").arg(branch).arg("--single-branch");
        }
        cmd.arg(repo_url).arg(dest);
        let out = run_cmd_with_timeout(
            cmd,
            git_timeout(),
            format!("git clone {} into {:?}", repo_url, dest),
            cancel,
        )?;
        if !out.status.success() {
            anyhow::bail!("git clone failed: {}", String::from_utf8_lossy(&out.stderr));
        }
    }

    // Checkout desired branch if specified (best-effort; shallow clones may already be on it).
    if let Some(branch) = branch {
        let out = run_cmd_with_timeout(
            {
                let mut cmd = git_cmd();
                cmd.arg("-C").arg(dest).args(["checkout", branch]);
                cmd
            },
            git_fetch_timeout(),
            format!("git checkout {} in {:?}", branch, dest),
            cancel,
        )?;
        if !out.status.success() {
            // Don't hard-fail; still return HEAD for caller.
            // But include useful context for debugging.
            let stderr = String::from_utf8_lossy(&out.stderr);
            if !stderr.trim().is_empty() {
                eprintln!("[git_fetcher] checkout warning: {}", stderr);
            }
        }
    }

    // Read HEAD revision.
    let out = run_cmd_with_timeout(
        {
            let mut cmd = git_cmd();
            cmd.arg("-C").arg(dest).args(["rev-parse", "HEAD"]);
            cmd
        },
        git_fetch_timeout(),
        format!("git rev-parse HEAD in {:?}", dest),
        cancel,
    )?;
    if !out.status.success() {
        anyhow::bail!(
            "git rev-parse failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn fetch_origin(repo: &Repository) -> Result<()> {
    let mut remote = repo.find_remote("origin")?;
    let mut opts = FetchOptions::new();
    remote.fetch(
        &["refs/heads/*:refs/remotes/origin/*"],
        Some(&mut opts),
        None,
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "tests/git_fetcher.rs"]
mod tests;
