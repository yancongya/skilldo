---
name: skilldo-cli
description: Manage AI agent skills from the terminal with the SkillDo CLI (skilldo). Install, sync, update, delete, and push skills across 45 AI tools. Agent-native, structured JSON output.
---

# SkillDo CLI (`skilldo`)

For ordinary cross-device work, prefer `skilldo device status|pull|publish`. Use `profile` for advanced offline import/export and conflict resolution. Never run `device publish --yes` without authorization because it may commit and push owned repositories.

`skilldo` is the command-line interface for SkillDo — the "install once, sync everywhere" manager for AI Agent Skills. It lets AI coding agents (Claude Code, Codex, Cursor, etc.) and humans read and manage skills from a plain terminal, without launching the desktop GUI.

The CLI shares the exact same SQLite database as the desktop client, so state stays in sync across both.

## Install without cloning

macOS:

```bash
curl -fsSL https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.sh | bash
```

Windows PowerShell:

```powershell
irm https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.ps1 | iex
```

The installers select the current architecture, verify the release SHA-256, place `skilldo` on a user-local path, and run `skilldo --version`. On a new device, configure WebDAV before calling `device status` or `device pull`; pass the password through `config set webdav.password --stdin` and never print it.

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
| `skilldo status [--json]` | Show installation status of all 45 supported AI tools |
| `skilldo explore [--query Q] [--json]` | Browse the skill market across enabled sources |
| `skilldo sources list [--json]` | List configured explore sources |
| `skilldo install --url <repo> [--name N] [--yes]` | Install a skill from a git URL or local path |
| `skilldo sync --skill <name> --tool <key>` | Sync a skill to a specific AI tool |
| `skilldo unsync --skill <name> --tool <key>` | Remove a skill from a specific tool |
| `skilldo update --skill <name> [--yes]` | Update a skill from its source (git pull / local copy) |
| `skilldo update --all [--yes]` | Update all git-managed skills |
| `skilldo delete --skill <name> [--yes]` | Delete a skill and remove all sync targets |
| `skilldo push --skill <name> [-m "msg"]` | Commit and push changes for a git-managed skill |
| `skilldo backup file [path] [--json]` | Write a lossless database snapshot JSON |
| `skilldo backup webdav [--json]` | Upload the snapshot, including stored credentials |
| `skilldo restore file <path> [--json]` | Restore a validated local snapshot |
| `skilldo restore webdav [--json]` | Restore the validated WebDAV snapshot |
| `skilldo profile status [--json]` | Preview cross-device changes and conflicts without writing |
| `skilldo profile sync [--yes] [--json]` | Apply the profile; `--yes` also confirms pending deletions |
| `skilldo repair sources [--apply] [--json]` | Audit or promote local records with a verified Git origin |
| `skilldo repair source --skill <name> --url <repo> [--subpath <path>] [--apply] [--json]` | Validate and reconnect one confirmed Git source |
| `skilldo profile export <path> [--json]` | Export an offline portable Profile |
| `skilldo profile import <path> [--strategy abort\|local\|remote] [--json]` | Import and merge an offline Profile |
| `skilldo profile resolve --strategy local\|remote [--json]` | Resolve WebDAV conflicts and synchronize |
| `skilldo --help` | Show all commands and flags |

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

# Preview, then synchronize the portable profile
skilldo profile status --json
skilldo profile sync --yes --json
skilldo repair sources --json
skilldo repair sources --apply --json
skilldo repair source --skill drawio --url https://github.com/bahayonghang/drawio-skills.git --subpath skills/drawio --json

# Delete a skill
skilldo delete --skill old-skill --yes
```

## Tool keys

Common tool keys for `--tool`: `claude_code`, `codex`, `opencode`, `gemini_cli`, `cline`, `augment`, `openclaw`, `iflow_cli`, `kiro_cli`, `pi`, `qoder`, `qwen_code`, `antigravity`, `cursor`. Run `skilldo status` for the full list.

## Discovery

If `skilldo` is on `PATH` (e.g. via shell function, `cargo install`, or bundled with the desktop app), an agent can invoke it directly after reading this skill. Prefer `--json` for programmatic use and parse the exit code to detect failures.
