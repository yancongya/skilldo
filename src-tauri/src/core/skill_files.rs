use std::path::Path;

use anyhow::{bail, Context, Result};
use walkdir::{DirEntry, WalkDir};

const IGNORE_NAMES: [&str; 6] = [
    ".git",
    ".DS_Store",
    "Thumbs.db",
    ".gitignore",
    ".skilldo-cache.json",
    "__pycache__",
];
const MAX_FILE_SIZE: u64 = 1_048_576; // 1 MB

fn is_ignored(entry: &DirEntry) -> bool {
    let file_name = entry.file_name().to_string_lossy();
    IGNORE_NAMES.iter().any(|name| name == &file_name.as_ref())
        || (entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "pyc"))
}

pub struct FileEntry {
    pub path: String,
    pub size: u64,
}

pub fn list_files(central_path: &Path) -> Result<Vec<FileEntry>> {
    let mut entries: Vec<FileEntry> = Vec::new();

    for entry in WalkDir::new(central_path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_ignored(e))
    {
        let entry = entry?;
        if !entry.file_type().is_file() || is_ignored(&entry) {
            continue;
        }

        let relative = entry
            .path()
            .strip_prefix(central_path)
            .with_context(|| format!("strip prefix {:?}", entry.path()))?;

        let metadata = entry.metadata()?;
        entries.push(FileEntry {
            path: relative.to_string_lossy().to_string(),
            size: metadata.len(),
        });
    }

    // Sort: SKILL.md first, then alphabetical
    entries.sort_by(|a, b| {
        let a_is_skill = a.path.eq_ignore_ascii_case("skill.md");
        let b_is_skill = b.path.eq_ignore_ascii_case("skill.md");
        match (a_is_skill, b_is_skill) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.path.to_lowercase().cmp(&b.path.to_lowercase()),
        }
    });

    Ok(entries)
}

pub fn read_file(central_path: &Path, relative_path: &str) -> Result<String> {
    // Path traversal protection
    if relative_path.contains("..") {
        bail!("Invalid file path: path traversal not allowed");
    }

    let full_path = central_path.join(relative_path);
    let canonical = full_path
        .canonicalize()
        .with_context(|| format!("resolve path: {:?}", full_path))?;
    let canonical_base = central_path
        .canonicalize()
        .with_context(|| format!("resolve base: {:?}", central_path))?;

    if !canonical.starts_with(&canonical_base) {
        bail!("Invalid file path: outside skill directory");
    }

    let metadata =
        std::fs::metadata(&canonical).with_context(|| format!("read metadata: {:?}", canonical))?;

    if metadata.len() > MAX_FILE_SIZE {
        bail!(
            "File too large to display ({:.1} KB, max 1 MB)",
            metadata.len() as f64 / 1024.0
        );
    }

    let bytes = std::fs::read(&canonical).with_context(|| format!("read file: {:?}", canonical))?;

    String::from_utf8(bytes)
        .map_err(|_| anyhow::anyhow!("File is not valid UTF-8 text and cannot be displayed"))
}

#[cfg(test)]
#[path = "tests/skill_files.rs"]
mod tests;
