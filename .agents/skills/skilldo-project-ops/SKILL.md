---
name: skilldo-project-ops
description: Automatically operate this repository's SkillDo configuration, Skill, backup, WebDAV, and cross-device Profile interfaces through the JSON CLI. Use when a request asks to inspect or change SkillDo state, synchronize computers, back up or restore data, or exercise a corresponding backend interface; do not wait for the user to separately request CLI execution.
---

# SkillDo Project Ops

Use the CLI as the operational interface for SkillDo state. A request for an outcome such as "check the profile", "back up the configuration", or "synchronize computer B" authorizes the matching CLI operation; do not merely print a command for the user to run.

## Resolve the executable

From the repository root, prefer the first available option:

1. `skilldo` on `PATH` when it is the current project version.
2. `src-tauri/target/debug/skilldo` when already built from this checkout.
3. If a build is necessary, use the existing `./scripts/build.sh cli`. Clean only artifacts created by that build after verification; retain normal Rust incremental caches.

On a new macOS or Windows device, prefer the standalone CLI release installers in `scripts/install-cli.sh` and `scripts/install-cli.ps1`; cloning the repository is not required. Both installers must verify the published SHA-256 before installing.

Use the shared default database unless the user explicitly requests an isolated database. Pass `--json` and parse both the exit code and JSON output. Never expose tokens or passwords in the response or command logs.

## Route requests

- Inspect or edit settings: `skilldo config get|set ... --json`.
- Detect and apply the current environment author from the authenticated GitHub CLI with `skilldo author detect --apply --json`; use `author set` only for explicit overrides. Repository/package authors in `list --json` are read-only source metadata and are not the current-author setting.
- Read or write arrays and objects with dotted config keys and JSON values. Use `skilldo config set <secret-key> --stdin --json` for credentials so they do not enter command history.
- Inspect, install, update, sync, unsync, delete, or push Skills: use the matching top-level command with `--json`.
- Inspect project-local Skills with `skilldo project skills --path <project> --json`. The result includes the parent Git repository, branch/revision, dirty state, and each Skill's repository-relative subpath. Project-owned Skills must follow the parent project repository; do not create nested or per-Skill repositories for them. Use `sync|unsync --scope project --project-path <project>` only when placing an external managed Skill into a project target.
- Create a lossless local snapshot: `skilldo backup file <path> --json`.
- Upload or restore the lossless WebDAV snapshot: `skilldo backup webdav --json` or `skilldo restore webdav --json`.
- Preview complete cross-device state first: `skilldo device status --json`.
- Get another device's configuration, Skill list, Git/npm revisions, targets and tags with `skilldo device pull --json`. Deletions remain pending unless explicitly authorized with `--yes`.
- Device synchronization is union-based: independent Skills, tags, and global targets from every device are retained. Treat source/revision disagreement and delete-versus-edit as conflicts; never resolve them implicitly.
- Portable Profiles keep project repositories and project-owned Skill subpaths, never another device's absolute project path. Report `missingProjects` so the destination device can clone or attach the parent repository at a user-selected location.
- Publish with `skilldo device publish --yes --json` only after repository commits/pushes are authorized. It merges remote state, checks and pushes owned repositories, records revisions, then uploads the Profile and lossless backup.
- Treat `skilldo profile ...` as the advanced interface for offline import/export and conflict resolution.
- Export or import an offline Profile: `skilldo profile export <path> --json` or `skilldo profile import <path> --strategy abort --json`.
- Audit incorrectly local source records: `skilldo repair sources --json`. It consumes standard Skill lock metadata, content-matched plugin manifests, and Git worktrees. Apply only high-confidence repairs with `skilldo repair sources --apply --json` after reviewing the dry-run report.
- Reconnect a confirmed source that has no retained metadata with `skilldo repair source --skill <name> --url <repo> [--subpath <path>] --json`, then repeat with `--apply`. The command clones the remote and validates the selected `SKILL.md` identity before writing.
- If a Profile reports conflicts, show the conflicting paths. Use `skilldo profile resolve --strategy local|remote --json` only when the user's preference is known from the request; otherwise ask which side should win.

## Safety boundaries

The lossless v2 backup intentionally contains GitHub and WebDAV credentials. Store it only at the user-selected private path or configured WebDAV location and do not quote its contents. Profile files remain portable and exclude device-local credentials and paths.

Do not apply deletions unless the user requested deletion or confirmed the pending deletion set; `profile sync --yes`, `profile import ... --yes`, restore, delete, and push are material mutations. A restore replaces the complete SQLite state but does not contain repository working trees or local-only Skill files. Use Profile/Git synchronization to reconstruct repository content on a different computer.

Source repair never guesses a repository for a central copy or a path outside Git. Report unresolved records so the user can reconnect them to a verified repository later.

After an operation, report the structured result: target/path, backup version, database restore status, installed/updated/deleted counts, pending deletions, conflicts, and failures as applicable. If the CLI returns `{ "ok": false }`, surface the error and do not claim success.
