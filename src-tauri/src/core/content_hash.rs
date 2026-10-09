use std::path::Path;

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use walkdir::{DirEntry, WalkDir};

const IGNORE_NAMES: [&str; 6] = [
    ".git",
    ".DS_Store",
    "Thumbs.db",
    ".gitignore",
    ".skilldo-cache.json",
    "__pycache__",
];

fn is_ignored(entry: &DirEntry) -> bool {
    let file_name = entry.file_name().to_string_lossy();
    IGNORE_NAMES.iter().any(|name| name == &file_name.as_ref())
        || (entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "pyc"))
}

pub fn hash_dir(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();

    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_ignored(entry))
    {
        let entry = entry?;
        if is_ignored(&entry) {
            continue;
        }

        let relative = entry
            .path()
            .strip_prefix(path)
            .with_context(|| format!("strip prefix {:?}", entry.path()))?;
        hasher.update(relative.to_string_lossy().as_bytes());

        if entry.file_type().is_file() {
            let bytes = std::fs::read(entry.path())
                .with_context(|| format!("read file {:?}", entry.path()))?;
            hasher.update(bytes);
        }
    }

    let digest = hasher.finalize();
    Ok(hex::encode(digest))
}

#[cfg(test)]
#[path = "tests/content_hash.rs"]
mod tests;
