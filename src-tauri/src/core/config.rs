//! Centralized configuration constants for Skills Hub.
//!
//! All project-wide names, identifiers, and URLs live here so that renaming
//! the project or changing the remote only requires edits in one place.
//!
//! To change the GitHub owner/repo, update `GITHUB_OWNER` / `GITHUB_REPO` and
//! keep `GITHUB_REPO_PATH` and the URL constants below in sync.

/// GitHub organization/user for remote URLs.
pub const GITHUB_OWNER: &str = "yancongya";

/// GitHub repository name.
pub const GITHUB_REPO: &str = "skills-hub";

/// Full GitHub repo path (`owner/repo`), used in git URLs and references.
pub const GITHUB_REPO_PATH: &str = "yancongya/skills-hub";

/// macOS app bundle identifier / Tauri app identifier.
/// Also used to resolve the SQLite database path on disk.
/// WARNING: changing this will cause existing users to lose their data
/// unless a migration is implemented.
pub const APP_IDENTIFIER: &str = "com.qufei1993.skillshub";

/// Directory name for the central skill repository (`~/.skillshub`).
pub const CENTRAL_DIR_NAME: &str = ".skillshub";

/// Git cache directory name (inside the platform cache dir).
pub const GIT_CACHE_DIR_NAME: &str = "skills-hub-git-cache";

/// URL for the featured skills JSON index.
pub const FEATURED_SKILLS_URL: &str =
    "https://raw.githubusercontent.com/yancongya/skills-hub/main/featured-skills.json";

/// Updater endpoint URL (GitHub releases).
pub const UPDATER_URL: &str =
    "https://github.com/yancongya/skills-hub/releases/latest/download/updater.json";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_consistent() {
        assert!(APP_IDENTIFIER.contains("skillshub"));
        assert!(CENTRAL_DIR_NAME.starts_with('.'));
        assert!(GIT_CACHE_DIR_NAME.contains("skills-hub"));
        assert!(FEATURED_SKILLS_URL.contains(GITHUB_REPO_PATH));
        assert!(FEATURED_SKILLS_URL.ends_with(".json"));
        assert!(UPDATER_URL.contains(GITHUB_REPO_PATH));
        assert!(UPDATER_URL.ends_with("updater.json"));
    }

    #[test]
    fn github_paths_compile() {
        assert_eq!(GITHUB_REPO_PATH, "yancongya/skills-hub");
    }
}
