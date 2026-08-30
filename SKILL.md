---
name: skilldo-cli
description: Manage AI agent skills from the terminal with the SkillDo CLI (skilldo). Install, sync, update, delete, and push skills across 47+ AI tools. Agent-native, structured JSON output.
---

# SkillDo CLI (`skilldo`)

`skilldo` is the command-line interface for SkillDo — the "install once, sync everywhere" manager for AI Agent Skills. It lets AI coding agents (Claude Code, Codex, Cursor, etc.) and humans read and manage skills from a plain terminal, without launching the desktop GUI.

The CLI shares the exact same SQLite database as the desktop client, so state stays in sync across both.

## When to use

- **Read**: list skills, check tool status, browse the market
- **Write**: install new skills from git/local, sync to tools, update, delete
- **Git**: commit and push changes for git-managed skills
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
| `skilldo update --all [--yes]` | Update all git-managed skills |
| `skilldo delete --skill <name> [--yes]` | Delete a skill and remove all sync targets |
| `skilldo push --skill <name> [-m "msg"]` | Commit and push changes for a git-managed skill |
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

# Delete a skill
skilldo delete --skill old-skill --yes
```

## Tool keys

Common tool keys for `--tool`: `claude_code`, `codex`, `opencode`, `gemini_cli`, `cline`, `augment`, `openclaw`, `iflow_cli`, `kiro_cli`, `pi`, `qoder`, `qwen_code`, `antigravity`, `cursor`. Run `skilldo status` for the full list.

## Discovery

If `skilldo` is on `PATH` (e.g. via shell function, `cargo install`, or bundled with the desktop app), an agent can invoke it directly after reading this skill. Prefer `--json` for programmatic use and parse the exit code to detect failures.
