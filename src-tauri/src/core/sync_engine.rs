use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum SyncMode {
    Auto,
    Symlink,
    Junction,
    Copy,
}

#[derive(Clone, Debug)]
pub struct SyncOutcome {
    pub mode_used: SyncMode,
    pub target_path: PathBuf,
    pub replaced: bool,
}

pub fn sync_dir_hybrid(source: &Path, target: &Path) -> Result<SyncOutcome> {
    if !source.is_dir() {
        anyhow::bail!("sync source is not a directory: {:?}", source);
    }
    // `Path::exists()` follows links, so a dangling symlink/junction looks
    // absent and the code would try to create over it (or copy into it).
    // symlink_metadata counts every reparse point as an existing target.
    if std::fs::symlink_metadata(target).is_ok() {
        if is_same_link(target, source) {
            return Ok(SyncOutcome {
                mode_used: link_mode_at(target).unwrap_or(SyncMode::Symlink),
                target_path: target.to_path_buf(),
                replaced: false,
            });
        }
        anyhow::bail!("target already exists: {:?}", target);
    }

    ensure_parent_dir(target)?;

    // On Windows a directory symlink needs elevation / Developer Mode while a
    // junction does not. Prefer the junction so ordinary installs keep a real
    // link instead of silently degrading to a full copy.
    #[cfg(windows)]
    {
        match try_junction(source, target) {
            Ok(()) => {
                return Ok(SyncOutcome {
                    mode_used: SyncMode::Junction,
                    target_path: target.to_path_buf(),
                    replaced: false,
                });
            }
            Err(err) => {
                log::warn!(
                    "[sync_engine] junction {:?} -> {:?} failed: {:#}; trying symlink",
                    target,
                    source,
                    err
                );
                cleanup_failed_junction(target);
            }
        }
    }

    match try_link_dir(source, target) {
        Ok(()) => {
            return Ok(SyncOutcome {
                mode_used: SyncMode::Symlink,
                target_path: target.to_path_buf(),
                replaced: false,
            });
        }
        Err(err) => log::debug!(
            "[sync_engine] symlink {:?} -> {:?} failed: {:#}",
            target,
            source,
            err
        ),
    }

    log::warn!(
        "[sync_engine] link creation failed for {:?} -> {:?}; falling back to a full copy (the target will no longer track central changes)",
        target,
        source
    );
    #[cfg(windows)]
    cleanup_failed_junction(target);
    copy_dir_recursive(source, target)?;
    Ok(SyncOutcome {
        mode_used: SyncMode::Copy,
        target_path: target.to_path_buf(),
        replaced: false,
    })
}

pub fn sync_dir_hybrid_with_overwrite(
    source: &Path,
    target: &Path,
    overwrite: bool,
) -> Result<SyncOutcome> {
    let mut did_replace = false;
    if std::fs::symlink_metadata(target).is_ok() {
        // A healthy link already tracks the central directory: replacing it
        // with a copy is a regression (wasted space + a stale target).
        if is_same_link(target, source) {
            return Ok(SyncOutcome {
                mode_used: link_mode_at(target).unwrap_or(SyncMode::Symlink),
                target_path: target.to_path_buf(),
                replaced: false,
            });
        }

        if overwrite {
            // NEVER remove_dir_all here: on Windows the target can be a
            // junction, and traversing it would delete the central repo.
            remove_path_any(target)
                .with_context(|| format!("remove existing target {:?}", target))?;
            did_replace = true;
        } else {
            anyhow::bail!("target already exists: {:?}", target);
        }
    }

    sync_dir_hybrid(source, target).map(|mut out| {
        out.replaced = did_replace;
        out
    })
}

pub fn sync_dir_copy_with_overwrite(
    source: &Path,
    target: &Path,
    overwrite: bool,
) -> Result<SyncOutcome> {
    let mut did_replace = false;
    if std::fs::symlink_metadata(target).is_ok() {
        if overwrite {
            remove_path_any(target)
                .with_context(|| format!("remove existing target {:?}", target))?;
            did_replace = true;
        } else {
            anyhow::bail!("target already exists: {:?}", target);
        }
    }

    ensure_parent_dir(target)?;
    copy_dir_recursive(source, target)?;

    Ok(SyncOutcome {
        mode_used: SyncMode::Copy,
        target_path: target.to_path_buf(),
        replaced: did_replace,
    })
}

pub fn sync_dir_for_tool_with_overwrite(
    tool_key: &str,
    source: &Path,
    target: &Path,
    overwrite: bool,
) -> Result<SyncOutcome> {
    // Cursor 目前不支持软链/junction：强制使用 copy，避免同步后在 Cursor 内不可用。
    if tool_key.eq_ignore_ascii_case("cursor") {
        return sync_dir_copy_with_overwrite(source, target, overwrite);
    }
    sync_dir_hybrid_with_overwrite(source, target, overwrite)
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create dir {:?}", parent))?;
    }
    Ok(())
}

/// Removes a target path without ever traversing a reparse point.
///
/// This is shared by the sync engine and by uninstall/unsync in `installer.rs`.
/// A junction is a directory reparse point whose `file_type().is_symlink()` is
/// false on Windows, so a plain `remove_dir_all` is not safe here.
pub(crate) fn remove_path_any(path: &Path) -> Result<()> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err).with_context(|| format!("stat {:?}", path)),
    };
    let ft = meta.file_type();

    // Symlinks and NTFS junctions are links: remove only the link itself.
    // `remove_dir` deletes a directory link (and fails on a file link), while
    // `remove_file` handles file symlinks and Unix symlinks.
    if ft.is_symlink() || (ft.is_dir() && read_link_target(path).is_some()) {
        if std::fs::remove_dir(path).is_ok() {
            return Ok(());
        }
        std::fs::remove_file(path).with_context(|| format!("remove link {:?}", path))?;
        return Ok(());
    }
    if ft.is_dir() {
        std::fs::remove_dir_all(path).with_context(|| format!("remove dir {:?}", path))?;
        return Ok(());
    }
    std::fs::remove_file(path).with_context(|| format!("remove file {:?}", path))?;
    Ok(())
}

/// Reads the stored destination of a symlink or an NTFS junction.
///
/// `std::fs::read_link` understands both, but older junctions created without
/// a PrintName are best read through the junction crate as a fallback.
fn read_link_target(path: &Path) -> Option<PathBuf> {
    if let Ok(existing) = std::fs::read_link(path) {
        return Some(existing);
    }
    #[cfg(windows)]
    if let Ok(existing) = junction::get_target(path) {
        return Some(existing);
    }
    None
}

/// Windows junction targets are stored as NT paths (`\\?\C:\...`), while the
/// central path in the database is a normal Win32 path (`C:\...`). Comparing
/// them byte-for-byte always fails, which used to make every `sync` delete a
/// healthy junction and replace it with a copy.
fn normalized_key(path: &Path) -> String {
    #[cfg(windows)]
    let raw = path.to_string_lossy().replace('/', "\\");
    #[cfg(not(windows))]
    let raw = path.to_string_lossy().to_string();

    #[cfg(windows)]
    let stripped = if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = raw.strip_prefix(r"\\?\") {
        rest.to_owned()
    } else {
        raw.clone()
    };
    #[cfg(not(windows))]
    let stripped = raw;

    let key = stripped.trim_end_matches(['\\', '/']).to_owned();
    #[cfg(windows)]
    {
        key.to_lowercase()
    }
    #[cfg(not(windows))]
    {
        key
    }
}

fn paths_equivalent(left: &Path, right: &Path) -> bool {
    if normalized_key(left) == normalized_key(right) {
        return true;
    }
    // Both paths usually exist; canonicalize resolves `..`, case differences
    // and the junction chain (`.skillshub` -> `.agents/skills`) for us.
    match (std::fs::canonicalize(left), std::fs::canonicalize(right)) {
        (Ok(a), Ok(b)) => normalized_key(&a) == normalized_key(&b),
        _ => false,
    }
}

/// Returns true when `link_path` is a link whose destination is `target`.
pub(crate) fn is_same_link(link_path: &Path, target: &Path) -> bool {
    match read_link_target(link_path) {
        Some(existing) => paths_equivalent(&existing, target),
        None => false,
    }
}

/// Reports the real link type at `path` (used so DB `mode` stays accurate).
pub(crate) fn link_mode_at(path: &Path) -> Option<SyncMode> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    #[cfg(windows)]
    {
        // Rust's FileType reports *any* reparse point (junctions included) as
        // `is_symlink() == true`, so check the actual reparse tag first.
        if junction::exists(path).unwrap_or(false) {
            return Some(SyncMode::Junction);
        }
    }
    if meta.file_type().is_symlink() || read_link_target(path).is_some() {
        return Some(SyncMode::Symlink);
    }
    None
}

fn try_link_dir(source: &Path, target: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(source, target)
            .with_context(|| format!("symlink {:?} -> {:?}", target, source))?;
        Ok(())
    }

    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_dir(source, target)
            .with_context(|| format!("symlink {:?} -> {:?}", target, source))?;
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    anyhow::bail!("symlink not supported on this platform");
}

#[cfg(windows)]
fn try_junction(source: &Path, target: &Path) -> Result<()> {
    // junction 1.x writes "\??\" + source into the reparse buffer. A Win32
    // verbatim prefix in `source` would produce an invalid mount point, so
    // strip it before handing the path to the crate.
    let raw = source.to_string_lossy();
    let source = match raw.strip_prefix(r"\\?\UNC\") {
        Some(rest) => PathBuf::from(format!(r"\\{rest}")),
        None => raw
            .strip_prefix(r"\\?\")
            .map(PathBuf::from)
            .unwrap_or_else(|| source.to_path_buf()),
    };
    junction::create(&source, target)
        .with_context(|| format!("junction {:?} -> {:?}", target, source))?;
    Ok(())
}

#[cfg(windows)]
fn cleanup_failed_junction(path: &Path) {
    // junction 1.x creates the (empty) directory *before* setting its reparse
    // point; a later failure leaves it behind and every retry then fails with
    // AlreadyExists. `remove_dir` only succeeds on an empty directory, so this
    // can never delete skill content.
    if std::fs::symlink_metadata(path).is_err() {
        return;
    }
    if let Err(err) = std::fs::remove_dir(path) {
        log::debug!("[sync_engine] keeping existing target {:?}: {}", path, err);
    }
}

fn should_skip_copy(entry: &walkdir::DirEntry) -> bool {
    entry.file_name() == ".git"
}

pub fn copy_dir_recursive(source: &Path, target: &Path) -> Result<()> {
    let profile = std::env::var("SKILLS_HUB_PROFILE_IO")
        .ok()
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let started = std::time::Instant::now();
    let mut copied_files: u64 = 0;
    let mut copied_bytes: u64 = 0;

    for entry in walkdir::WalkDir::new(source)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !should_skip_copy(entry))
    {
        let entry = entry?;
        if should_skip_copy(&entry) {
            continue;
        }
        let relative = entry.path().strip_prefix(source)?;
        let target_path = target.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target_path)
                .with_context(|| format!("create dir {:?}", target_path))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let bytes = std::fs::copy(entry.path(), &target_path)
                .with_context(|| format!("copy file {:?} -> {:?}", entry.path(), target_path))?;
            if profile {
                copied_files += 1;
                copied_bytes = copied_bytes.saturating_add(bytes);
            }
        }
    }
    if profile {
        log::info!(
            "[sync_engine] copy_dir_recursive {} files, {} bytes in {}s (src={:?} dst={:?})",
            copied_files,
            copied_bytes,
            started.elapsed().as_secs_f32(),
            source,
            target
        );
    }
    Ok(())
}

#[cfg(test)]
#[path = "tests/sync_engine.rs"]
mod tests;
