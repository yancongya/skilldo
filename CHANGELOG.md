# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- **客户端自动更新通知**：桌面客户端启动后自动检查新版本，并复用现有更新弹窗展示中文更新说明与安装入口。
- **Windows 自动更新清单**：Release 流水线现在校验并发布 Windows x64、ARM64 安装包签名，并将四个平台完整写入 `updater.json`。
- **Main 自动发布**：代码推送到 `main` 并通过 CI 后，自动递增补丁版本、生成中文版本记录、创建标签并触发签名 Release 流水线。

### Changed
- **中文 Release 规范**：GitHub Release 标题、更新说明、安装提示和平台限制默认使用中文。
- **完整版本同步**：版本设置和检查现在同时覆盖 npm 与 Cargo 的清单和锁文件，避免自动发布出现版本漂移。

### Fixed
- **安装后立即重启**：macOS 和 Windows 完成更新安装后可直接从客户端重新启动，不再仅提示用户手动退出。

## [0.7.1] - 2026-09-01

### Fixed
- **Release updater signing**: Provision a dedicated updater signing key so macOS and Windows release artifacts include verifiable signatures and `updater.json` can be published reliably.

## [0.7.0] - 2026-09-01

### Added
- **Complete cross-device synchronization**: Add union-based pull and publish pipelines for Git/package Skills, global targets, tags, revisions, and portable settings across multiple devices.
- **Portable Profile v3**: Track project-owned Skills by parent repository URL, branch/revision, and repository-relative path without leaking device-specific absolute paths.
- **Lossless WebDAV backup**: Preserve the complete SQLite state, settings, origins, targets, tags, discovery data, and configured credentials in a checksummed backup payload.
- **Standalone CLI distribution**: Publish verified macOS and Windows CLI archives with SHA-256 checksums, plus shell and PowerShell installers that do not require cloning the repository.
- **Device pipelines**: Add CLI, shell, batch, and desktop actions for inspecting, pulling, and publishing complete device state.
- **Source repair and ownership detection**: Recover Git provenance for incorrectly local Skills, distinguish owned and third-party sources, and detect the current author from an authenticated GitHub CLI session.

### Changed
- **Project Skill ownership**: Project-local Skills now follow their parent project repository; untracked project Skills remain visible locally but are excluded from portable Profiles until committed.
- **Unified configuration**: Expand structured CLI and desktop settings for Explore sources, author identity, cache policy, WebDAV, GitHub authentication status, and backup/restore operations.
- **SkillDo branding**: Replace legacy app/frontend artwork with a consistent terminal-inspired SkillDo mark across desktop and light/dark UI surfaces.

### Fixed
- **Repository source recognition**: Improve Git worktree, subpath, lock metadata, plugin manifest, and content-based source detection so repository-backed Skills are not incorrectly treated as local-only.
- **Concurrent device changes**: Merge independent Skill lists, tags, and targets while surfacing source/revision and delete-versus-edit conflicts instead of silently overwriting another device.

## [0.6.1] - 2026-05-16

### Fixed
- **Window close behavior**: Closing the main window now exits the app instead of hiding it in the background, fixing cases where the app kept running but could not be reopened from the Dock or taskbar ([PR #68](https://github.com/qufei1993/skills-hub/pull/68)).

## [0.6.0] - 2026-05-05

### Added
- **Skill tags**: Add custom tags to managed skills for easier organization and filtering.
- **Tags page**: Manage tags from a dedicated Tags page, including create, rename, delete, and quick navigation back to filtered My Skills views.
- **Tag filtering**: Filter My Skills by one or more tags with OR matching, including a virtual `Untagged` filter for skills without tags.
- **Per-skill tag editor**: Edit a skill's tag assignments directly from the skill card.
- **Import search**: Search discovered skill candidates by name, description, or path before importing from a local directory or Git repository.

### Changed
- **My Skills filter bar**: Removed the manual refresh button; install, delete, sync, and tag-edit flows already refresh the list automatically.

### Fixed
- **Chinese filter bar layout**: Removing the refresh button fixes the cramped button layout in Chinese.
- **Discovered skills review**: The discovered skills review dialog now supports search and keeps selection counts aligned with filtered results.

## [0.5.0] - 2026-04-16

### Added
- **Project-level skill sync**: Skills can now be synced to selected project directories instead of only global tool directories.
- **Skill scope controls**: My Skills cards now show a scope badge (`Global` / project count) and include a scope modal for switching between global and project sync.
- **Scope filtering**: My Skills can be filtered by All / Global / Project scope.
- **Hermes Agent adapter**: Added global sync support for Hermes Agent via `~/.hermes/skills` ([#54](https://github.com/qufei1993/skills-hub/issues/54)).

### Changed
- **My Skills filter bar**: The section title now displays the total skill count, search is more compact, and filter controls stay on one line in the default window.
- **Default window size**: Increased the default desktop window from `800x600` to `960x680`.
- **macOS close behavior**: Closing the main window now hides it instead of quitting the app; reopening from the Dock restores and focuses the window.
- **Project sync support matrix**: Project-level sync is now treated as an explicit per-tool capability; tools without a confirmed project skills directory are global-only.

### Fixed
- **Import takeover for identical skills**: Importing an existing skill can now safely take over same-name targets when the content hash matches.
- **Import modal close behavior**: The local Skill import modal now closes correctly after partial import failures, including when some sync targets already exist.
- **Unsynced tool re-enable entry**: Tool buttons that were unsynced from a skill remain visible so they can be re-enabled.
- **SKILL.md metadata parsing**: YAML block scalar descriptions in frontmatter now render correctly in skill cards and detail views.

## [0.4.3] - 2026-04-11

### Added
- **Copaw tool adapter**: Support for Copaw AI coding tool (thanks @LeonDevLifeLog [PR#50](https://github.com/qufei1993/skills-hub/pull/50)).

### Fixed
- **Git skill install & frontmatter rendering**: Fixed issues with Git-based skill installation and frontmatter metadata rendering.
- **Git skill discovery for container paths**: Fixed skill discovery failing when repository uses container-style directory paths.

## [0.4.2] - 2026-04-06

### Fixed
- **New tools modal style**: "New tools detected" dialog now uses consistent header/footer structure (`modal-header` + `modal-footer`) matching all other modals, fixing missing padding and border separators ([#46](https://github.com/qufei1993/skills-hub/issues/46)).
- **Git skill name derivation**: Installing a Git skill from a repo root (subpath `"."`) now correctly derives the name from the repository URL instead of using `"."` as the display name.

## [0.4.1] - 2026-03-21

### Added
- **Frontmatter metadata table**: Markdown files with YAML frontmatter now render a GitHub-style metadata table at the top of the skill detail view.

## [0.4.0] - 2026-03-20

### Added
- **In-app update check**: Check for updates directly within Settings, download and install without leaving the app ([#33](https://github.com/qufei1993/skills-hub/issues/33)).
- **QoderWork tool adapter**: Support for QoderWork desktop AI agent (`~/.qoderwork/skills/`) ([#34](https://github.com/qufei1993/skills-hub/issues/34)).

### Changed
- **Settings promoted to full page**: Settings moved from a modal dialog to a dedicated page view, consistent with My Skills / Explore navigation pattern.
- **Curated skills aggregation**: Explore page now sources skills from a curated list of 7 high-quality repositories.

### Fixed
- Language toggle briefly flashing "Installing Skills..." loading overlay on Explore page.

## [0.3.0] - 2026-03-15

### Added
- **Explore page**: Explore promoted from a modal tab to an independent page with My Skills / Explore top-level navigation.
- **Featured skills**: Explore page displays curated skills from ClawHub API (updated daily via GitHub Actions) with frontend filtering and one-click install.
- **Online skill search**: Real-time search via skills.sh API (triggered at 2+ characters, 500ms debounce), results deduplicated against the featured list and shown in separate sections.
- **Skill detail view**: Click a skill name to browse its files with a file tree, Markdown rendering (GFM + frontmatter stripping), and syntax highlighting (40+ languages, light/dark theme adaptive).
- **Skill description field**: Description extracted from SKILL.md frontmatter at install time, stored in database, and displayed on My Skills cards.
- **GitHub Token setting**: Optional GitHub Token input in settings to increase API rate limit from 60 to 5,000 requests/hour.
- **MoltBot tool adapter**: Added standalone MoltBot tool support after OpenClaw rename/split.

### Fixed
- Git install deriving skill name as "skills" when URL points to a `skills/` subdirectory, causing duplicated sync paths ([#28](https://github.com/qufei1993/skills-hub/issues/28)).
- GitHub API rate-limit errors now display the exact reset time instead of a generic message.
- Windows "Access Denied" OS error 5 when syncing to tools ([#20](https://github.com/qufei1993/skills-hub/issues/20)).
- Git repo directory structures not correctly recognized as skills ([#18](https://github.com/qufei1993/skills-hub/issues/18), [#8](https://github.com/qufei1993/skills-hub/issues/8)).
- Repos using `.claude/skills/` directory format not detected ([#27](https://github.com/qufei1993/skills-hub/issues/27)).
- OpenClaw path updated from `.moltbot/skills` to `.openclaw/skills` ([#29](https://github.com/qufei1993/skills-hub/issues/29)).

### Changed
- My Skills list: tool badges now only show synced tools, collapsing to `+N more` beyond 5.
- Manual Add modal simplified to Local Directory / Git Repository tabs only (Explore tab removed).
- Multi-skill repo online install now auto-matches target skill (exact → unique-contains → fallback to manual picker).

## [0.2.0] - 2026-02-01

### Added
- **Windows platform support**: Full support for Windows build and release (thanks @jrtxio [PR#6](https://github.com/qufei1993/skills-hub/pull/6)).
- Support and display for many new tools (e.g., Kimi Code CLI, Augment, OpenClaw, Cline, CodeBuddy, Command Code, Continue, Crush, Junie, iFlow CLI, Kiro CLI, Kode, MCPJam, Mistral Vibe, Mux, OpenClaude IDE, OpenHands, Pi, Qoder, Qwen Code, Trae/Trae CN, Zencoder, Neovate, Pochi, AdaL).
- UI confirmation and linked selection for tools that share the same global skills directory.
- Local import multi-skill discovery aligned with Git rules, with a selection list and invalid-item reasons.
- New local import commands for listing candidates and installing a selected subpath with SKILL.md validation.

### Changed
- Antigravity global skills directory updated to `~/.gemini/antigravity/global_skills`.
- OpenCode global skills directory corrected to `~/.config/opencode/skills`.
- Tool status now includes `skills_dir`; frontend tool list/sync is driven by backend data and deduped by directory.
- Sync/unsync now updates records across tools sharing a skills directory to avoid duplicate filesystem work and inconsistent state.
- Local import flow now scans candidates first; single valid candidate installs directly, multi-candidate opens selection.

## [0.1.1] - 2026-01-26

### Changed
- GitHub Actions release workflow for macOS packaging and uploading `updater.json` (`.github/workflows/release.yml`).
- Cursor sync now always uses directory copy due to Cursor not following symlinks when discovering skills: https://forum.cursor.com/t/cursor-doesnt-follow-symlinks-to-discover-skills/149693/4
- Managed skill update now re-syncs copy-mode targets using copy-only overwrite, and forces Cursor targets to copy to avoid accidental relinking.

## [0.1.0] - 2026-01-25

### Added
- Initial release of Skills Hub desktop app (Tauri + React).
- Central repository for Skills; sync to multiple AI coding tools (symlink/junction preferred, copy fallback).
- Local import from folders.
- Git import via repository URL or folder URL (`/tree/<branch>/<path>`), with multi-skill selection and batch install.
- Sync and update: copy-mode targets can be refreshed; managed skills can be updated from source.
- Migration intake: scan existing tool directories, import into central repo, and one‑click sync.
- New tool detection and optional sync.
- Basic settings: storage path, language, and theme.
- Git cache with cleanup (days) and freshness window (seconds).

### Build & Release
- Local packaging scripts for macOS (dmg), Windows (msi/nsis), Linux (deb/appimage).
- GitHub Actions build validation and tag-based draft releases (release notes pulled from `CHANGELOG.md`).

### Performance
- Git import and batch install optimizations: cached clones reduce repeated fetches; timeouts and non‑interactive git improve stability.
