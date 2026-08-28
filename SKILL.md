---
name: skillhub-cli
description: Manage AI agent skills from the terminal with the Skills Hub CLI (skillhub). List managed skills, check which AI tools are installed, and browse the skill market. Agent-native, structured JSON output.
---

# Skills Hub CLI (`skillhub`)

`skillhub` is the command-line interface for Skills Hub — the "install once, sync everywhere" manager for AI Agent Skills. It lets AI coding agents (Claude Code, Codex, Cursor, etc.) and humans read and manage skills from a plain terminal, without launching the desktop GUI.

The CLI shares the exact same SQLite database as the desktop client, so state stays in sync across both.

## When to use

- Read which skills are currently managed (`skillhub list`)
- Check which of the 47+ supported AI tools are installed (`skillhub status`)
- Browse the skill market across configured sources (`skillhub explore`)
- Inspect configured explore sources (`skillhub sources list`)
- Drive any of the above from an agent via `--json` for structured parsing

## Commands

| Command | Description |
|---------|-------------|
| `skillhub list [--json]` | List managed skills and the tools they are synced to |
| `skillhub status [--json]` | Show installation status of all supported AI tools |
| `skillhub explore [--query Q] [--json]` | Browse the skill market across enabled sources |
| `skillhub sources list [--json]` | List configured explore sources |
| `skillhub --help` | Show all commands and flags |

## Global flags

- `--json` — Emit structured JSON instead of human-readable text. Errors print to stderr with a non-zero exit code.
- `--db <path>` — Override the SQLite database path (defaults to the shared app data dir). Handy for portable/testing use.

## Output contract (for agents)

- Success: human-readable text by default, or a JSON object/array with `--json`.
- Failure: message on stderr + non-zero exit code. When `--json` is set, errors from individual explore sources are returned in the `errors` array rather than aborting, so partial results are still usable.

### Examples

```bash
# List managed skills as JSON (agent-friendly)
skillhub list --json

# Check which AI tools are installed
skillhub status

# Search the skill market
skillhub explore --query "rag"

# Inspect the database at a custom location
skillhub --db /tmp/skills_hub.db list --json
```

## Discovery

If `skillhub` is on `PATH` (e.g. via `cargo install` or bundled with the desktop app), an agent can invoke it directly after reading this skill. Prefer `--json` for programmatic use and parse the `exit code` to detect failures.
