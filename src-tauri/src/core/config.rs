//! Centralized configuration constants for SkillDo.
//!
//! All project-wide names, identifiers, and URLs live here so that renaming
//! the project or changing the remote only requires edits in one place.
//!
//! To change the GitHub owner/repo, update `GITHUB_OWNER` / `GITHUB_REPO` and
//! keep `GITHUB_REPO_PATH` and the URL constants below in sync.
//!
//! `PRODUCT_NAME` / `CLI_NAME` are the single source of truth for the
//! human-facing brand. Keep these build manifests in sync with them:
//! - `src-tauri/Cargo.toml` `[[bin]] name` -> `CLI_NAME`
//! - `src-tauri/tauri.conf.json` `productName` + window `title` -> `PRODUCT_NAME`

/// Human-facing product name (GUI window title, docs, release notes, git author).
pub const PRODUCT_NAME: &str = "SkillDo";

/// Name of the CLI binary / command (e.g. `skilldo install`).
/// Must match the `[[bin]] name` in `src-tauri/Cargo.toml`.
pub const CLI_NAME: &str = "skilldo";

/// GitHub organization/user for remote URLs.
pub const GITHUB_OWNER: &str = "yancongya";

/// GitHub repository name.
pub const GITHUB_REPO: &str = "skilldo";

/// Full GitHub repo path (`owner/repo`), used in git URLs and references.
pub const GITHUB_REPO_PATH: &str = "yancongya/skilldo";

/// macOS app bundle identifier / Tauri app identifier.
/// Also used to resolve the SQLite database path on disk.
/// WARNING: changing this will cause existing users to lose their data
/// unless a migration is implemented.
pub const APP_IDENTIFIER: &str = "com.qufei1993.skillshub";

/// Directory name for the central skill repository (`~/.skillshub`).
pub const CENTRAL_DIR_NAME: &str = ".skillshub";

/// Git cache directory name (inside the platform cache dir).
pub const GIT_CACHE_DIR_NAME: &str = "skilldo-git-cache";

/// URL for the featured skills JSON index.
pub const FEATURED_SKILLS_URL: &str =
    "https://raw.githubusercontent.com/yancongya/skilldo/main/featured-skills.json";

/// Updater endpoint URL (GitHub releases).
pub const UPDATER_URL: &str =
    "https://github.com/yancongya/skilldo/releases/latest/download/updater.json";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_consistent() {
        assert!(APP_IDENTIFIER.contains("skillshub"));
        assert!(CENTRAL_DIR_NAME.starts_with('.'));
        assert!(GIT_CACHE_DIR_NAME.contains("skilldo"));
        assert_eq!(PRODUCT_NAME, "SkillDo");
        assert_eq!(CLI_NAME, "skilldo");
        assert!(FEATURED_SKILLS_URL.contains(GITHUB_REPO_PATH));
        assert!(FEATURED_SKILLS_URL.ends_with(".json"));
        assert!(UPDATER_URL.contains(GITHUB_REPO_PATH));
        assert!(UPDATER_URL.ends_with("updater.json"));
    }

    #[test]
    fn github_paths_compile() {
        assert_eq!(GITHUB_REPO_PATH, "yancongya/skilldo");
    }
}
