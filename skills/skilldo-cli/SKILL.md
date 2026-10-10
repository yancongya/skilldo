---
name: skilldo-cli
description: Manage AI agent skills from the terminal with the SkillDo CLI (skilldo). Use for install, sync, update, delete, publish, profiles, and cross-agent source work, including duplicate copies, filesystem drift, broken or unregistered symlinks. Supports 47+ AI tools and structured JSON output.
---

# SkillDo CLI (`skilldo`)

For ordinary cross-device work, prefer `skilldo device status|pull|publish`. Use `profile` for advanced offline import/export and conflict resolution. Never run `device publish --yes` without authorization because it may commit and push owned repositories.

`skilldo` is the command-line interface for SkillDo — the "install once, sync everywhere" manager for AI Agent Skills. It lets AI coding agents (Claude Code, Codex, Cursor, etc.) and humans read and manage skills from a plain terminal, without launching the desktop GUI.

The CLI shares the exact same SQLite database as the desktop client, so state stays in sync across both.

> **`list` and `device status` report database records, not the filesystem.** A target may be stored as `mode=symlink` + `status=ok` while an actual directory copy sits at that path. Use the filesystem health check below to verify that targets are real symlinks to the central directory.

## Install without cloning

macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.ps1 | iex
```

The installers select the current architecture, verify the release SHA-256, place `skilldo` on a user-local path, and run `skilldo --version`. On a new device, configure WebDAV before calling `device status` or `device pull`; pass the password through `config set webdav.password --stdin` and never print it. Store GitHub tokens with `skilldo github token-set --stdin`; never pass a token as a positional argument. `github token-get` reports only whether a token is configured, and `github token-validate` checks the stored token without revealing it.

## When to use

- **Read**: list skills, check tool status, browse the market
- **Write**: install new skills from git/local, sync to tools, update, delete
- **Git**: commit and push changes for git-managed skills
- **Device sync**: inspect, pull, or publish the complete WebDAV/Profile/repository pipeline
- Drive any of the above from an agent via `--json` for structured parsing

## Commands

| Command | Description |
|---------|-------------|
| `skilldo list [--json]` | List managed skills and their sync targets |
| `skilldo status [--json]` | Show installation status of all 47+ supported AI tools |
| `skilldo explore [--query Q] [--json]` | Browse the skill market across enabled sources |
| `skilldo sources list [--json]` | List configured explore sources |
| `skilldo install --url <repo> [--name N] [--yes]` | Install a skill from a git URL or local path |
| `skilldo sync --skill <name> --tool <key>` | Sync a skill to a specific AI tool |
| `skilldo unsync --skill <name> --tool <key>` | Remove a skill from a specific tool |
| `skilldo update --skill <name> [--yes]` | Update a skill from its source (git pull / local copy) |
| `skilldo track-local --skill <name> --path <dir> [--yes]` | Track a validated local Skill directory as the source; preserve prior remote provenance and never fetch/reset |
| `skilldo update --all [--yes]` | Update all git-managed skills |
| `skilldo delete --skill <name> [--yes]` | Delete a skill and remove all sync targets |
| `skilldo push --skill <name> [-m "msg"]` | Commit and push changes for a git-managed skill |
| `skilldo backup file [path] [--json]` | Write a full database snapshot without local authentication credentials |
| `skilldo backup webdav [--json]` | Upload the full snapshot without local authentication credentials |
| `skilldo restore file <path> [--json]` | Restore a validated local snapshot |
| `skilldo restore webdav [--json]` | Restore the validated WebDAV snapshot |
| `skilldo profile check [--json]` | Read-only GET to verify saved WebDAV access; 404 means reachable but no Profile exists |
| `skilldo profile status [--json]` | Preview cross-device changes and conflicts without uploading or applying |
| `skilldo profile sync [--yes] [--update-skills] [--json]` | Apply the profile; upstream Git Skills update only with `--update-skills`; `--yes` confirms pending deletions |
| `skilldo repair sources [--apply] [--json]` | Audit or promote local records with a verified Git origin |
| `skilldo repair source --skill <name> --url <repo> [--subpath <path>] [--apply] [--json]` | Validate and reconnect one confirmed Git source |
| `skilldo repair origin --skill <name> --url <repo> [--subpath <path>] (--dry-run\|--apply) [--json]` | Migrate a verified Git Skill from a manual local-copy override to Git updates |

| `skilldo profile export <path> [--json]` | Export an offline portable Profile |
| `skilldo profile import <path> [--strategy abort\|local\|remote] [--json]` | Import and merge an offline Profile |
| `skilldo profile resolve --strategy local\|remote [--json]` | Resolve WebDAV conflicts and synchronize |
| `skilldo --help` | Show all commands and flags |

WebDAV authentication uses HTTP Basic Auth, so remote URLs must use HTTPS. SkillDo rejects plain HTTP for remote hosts, URL-embedded credentials, and redirects; loopback HTTP is permitted only for local tests. After configuring WebDAV, use `profile check --json` for a read-only GET. It does not create a directory or upload anything; a 404 means the endpoint is reachable but the Profile file does not exist yet. `profile status` previews the merge. The first `profile sync` can create the remote collection and upload the Profile, so do not use it to test connectivity. `device publish` is broader still: it may push Git repositories and upload the Profile and a full backup.

When WebDAV is unavailable, `profile export <path>` creates a local portable desired-state checkpoint without contacting a server. The portable Profile excludes credentials, device paths, custom scan directories, per-tool path overrides, project targets, and local-only Skills; it is not a lossless database backup and does not synchronize automatically. Review it before moving it to another device, then use `profile import <path> --json` there; import merges desired state and syncs targets without refreshing upstream Skills. `profile sync` and `device pull` also leave upstream versions unchanged by default; pass `--update-skills` only after reviewing upstream changes. Use `device status` to inspect pending Git changes before `pull` or `publish`.

For project-owned Skills, `track-local` points SkillDo at the canonical Skill-only directory inside the local Git checkout. It validates `SKILL.md`, rejects project roots containing unrelated files, and records the Git origin as provenance. Then `update --skill` copies that directory into the SkillDo central copy and keeps the registered symlinks. Git fetch/pull remains a separate user-controlled repository operation; `update --all` continues to update remote Git-managed Skills only.

Use `repair origin` only when a Git Skill already has the same registered remote and subpath but an older manual override changed it to `local_copy`. Preview with `--dry-run`, then make a full-state backup with `skilldo backup file <path> --json` before applying. New backups exclude stored GitHub and WebDAV credentials; restore keeps credentials configured on the current device. The migration re-clones and verifies the remote and Skill directory, then atomically updates only origin metadata; Skill IDs, names, source rows, tags, and target links stay intact.

`repair sources` only promotes a local Skill to Git provenance when its source subpath contains tracked files and has no uncommitted or untracked changes. A dirty or newly added project Skill remains a local source and is reported as unresolved until the repository work is committed and the upstream path is verified. This prevents project metadata or lockfiles from overriding active local work.

Source repair preserves an existing `publish_strategy=none` even when repository ownership rules classify the repository as yours. It must not silently grant Git push capability; change publish access only through an explicit origin setting.

> **`delete` is destructive and cannot be undone.** It removes target paths and the central copy. Retiring a Skill file in Git does not remove its existing central copy or target links. Review the target list and back up first.

## Global flags

- `--json` — Emit structured JSON instead of human-readable text. Errors print to stderr with a non-zero exit code.
- `--db <path>` — Override the SQLite database path (defaults to the shared app data dir).

## Skill name resolution

Most commands accept `--skill <name>` which matches by **case-insensitive name** or **exact UUID**. If the name is ambiguous (multiple skills share the same name), the command will list the matching IDs and fail.

## Output contract (for agents)

- Success: human-readable text by default, or a JSON object/array with `--json`.
- Failure: message on stderr + non-zero exit code. When `--json` is set, errors from individual explore sources are returned in the `errors` array rather than aborting.
- Write commands (`install`, `sync`, `delete`, `push`) default to interactive confirmation. Use `--yes` to skip prompts (agent mode).

### Examples

```bash
# Install a skill from GitHub and sync to Claude Code
skilldo install --url https://github.com/anthropics/skills/tree/main/skills/skill-creator --yes
skilldo sync --skill skill-creator --tool claude_code

# List all managed skills as JSON
skilldo list --json

# Check which AI tools are installed
skilldo status

# Search the skill market
skilldo explore --query "rag" --json

# Update all git-managed skills
skilldo update --all --yes

# Push local changes for a skill
skilldo push --skill my-skill -m "update docs"

# Point an existing Skill at its canonical local checkout directory, then refresh the center
skilldo track-local --skill my-skill --path ./skills/my-skill --yes --json
skilldo update --skill my-skill --yes --json

# Preview, then synchronize the portable profile
skilldo profile status --json
skilldo profile sync --yes --json
# Only after reviewing source changes, explicitly refresh upstream Skills
skilldo profile sync --update-skills --json
skilldo repair sources --json
skilldo repair sources --apply --json
skilldo repair source --skill drawio --url https://github.com/bahayonghang/drawio-skills.git --subpath skills/drawio --json
skilldo repair origin --skill my-skill --url https://github.com/owner/repo.git --subpath skills/my-skill --dry-run --json

# Read-only filesystem truth check; converge only after reviewing the dry-run report
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_doctor.py --json
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_converge.py

# Delete a skill
skilldo delete --skill old-skill --yes
```

## Filesystem health check

`scripts/skilldo_doctor.py` reads the registered target paths from SkillDo, then checks their actual filesystem form. It is read-only; exit codes are `0` clean, `1` drift found, `2` environment error.

```bash
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_doctor.py --json
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_doctor.py --orphans
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_doctor.py --unregistered
```

The default scan checks every target registered in the database. `--orphans` also finds unregistered real-directory copies; `--unregistered` finds symlinks pointing at the central directory but missing from the database. The classes `A` (identical to center), `B` (different content), and `C` (no center copy) require different recovery decisions. Never overwrite a newer `B` copy without reviewing it.

For a complete filesystem audit, run the default check, `--orphans`, and `--unregistered`: these cover different states. A dangling symlink is distinct from a missing path, and a link can point somewhere other than the SkillDo center. Inspect links with `readlink`/`lstat` and test that the resolved target exists; file-type commands that follow symlinks can hide the link itself. Do not infer filesystem health from the database status alone.

## Converge loose copies

`scripts/skilldo_converge.py` converts loose tool-directory copies to central symlinks. It defaults to a dry-run. `--apply` first creates an archive and quarantines originals rather than deleting them.

```bash
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_converge.py
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_converge.py --class A,C
python3 ~/.skillshub/skilldo-cli/scripts/skilldo_converge.py --apply
```

**Never replace a skill directory inside a Git working tree with a symlink.** Project-local Skill sources must remain ordinary repository files. The converge tool skips Git worktrees unless explicitly overridden; do not use `--include-repos` without a deliberate review.

Avoid manual bulk `mv`/`ln` replacement. If a link has to be repaired by hand, preserve the original first, then register/reconcile that tool target with `skilldo sync --skill <name> --tool <tool>` and rerun the filesystem checks. A link that resolves correctly but is absent from the SkillDo target list is still unmanaged.

`skilldo update` refreshes the central Skill directory from its registered source and can remove local files that are not in that source. Keep machine state, notes, credentials, and custom runtime data outside the Skill directory; put reusable files in the authoritative source repository.

## Skills already present in the central directory

If a Skill was manually migrated into `~/.skillshub`, `skilldo install --url ~/.skillshub/<name> --yes` can register it without copying over the existing directory. If it is actually owned by a project repository, point it at that repo's Skill-only directory with `track-local`; do not leave its source pointing back at the central directory, or future updates will be self-copies with no project provenance.

## Tool keys

Common tool keys for `--tool`: `claude_code`, `codex`, `opencode`, `gemini_cli`, `cline`, `augment`, `openclaw`, `iflow_cli`, `kiro_cli`, `pi`, `qoder`, `qwen_code`, `antigravity`, `cursor`. Run `skilldo status` for the full list.

`status` may show the same `skillsDir` for multiple tool keys (for example Claude Code, Mimo Desktop and Mimo Code). To identify the owner of a particular target path, use the target's `tool` field from `skilldo list --json`.

## Discovery

If `skilldo` is on `PATH` (e.g. via shell function, `cargo install`, or bundled with the desktop app), an agent can invoke it directly after reading this skill. Prefer `--json` for programmatic use and parse the exit code to detect failures.
