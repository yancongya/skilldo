# Skills Hub

> **Install once, sync everywhere.** The agent-native skill manager for 47+ AI tools.

Skills Hub manages AI Agent Skills from a single source of truth and syncs them to all your coding tools. Use the **CLI** for automation, the **desktop app** for visual management, or let **agents** drive it programmatically via structured JSON output.

## Quick Start

### CLI (recommended for agents & automation)

```bash
# Install from GitHub
skillhub install --url https://github.com/anthropics/skills/tree/main/skills/skill-creator --yes

# Sync to all tools
skillhub sync --skill skill-creator --tool claude_code
skillhub sync --skill skill-creator --tool codex

# Check status
skillhub list --json
skillhub status --json

# Update all git-managed skills
skillhub update --all --yes

# Browse the skill market
skillhub explore --query "rag" --json
```

### Desktop App

Download the `.dmg` for macOS (or `.exe` for Windows, `.AppImage` for Linux) from [Releases](https://github.com/yancongya/skills-hub/releases). The GUI provides visual skill management, explore, and one-click sync.

### Development

```bash
git clone https://github.com/yancongya/skills-hub.git
cd skills-hub
npm install
npm run tauri:dev          # Desktop app (GUI + Rust backend)
npm run dev                # Web preview only (no backend)
./scripts/build.sh         # Build for current platform
```

## CLI Commands

| Command | Description |
|---------|-------------|
| `skillhub list [--json]` | List managed skills and sync targets |
| `skillhub status [--json]` | Show which of 47+ AI tools are installed |
| `skillhub explore [--query Q] [--json]` | Browse the skill market |
| `skillhub install --url <repo> [--name] [--yes]` | Install from git URL or local path |
| `skillhub sync --skill <name> --tool <key>` | Sync to a specific tool |
| `skillhub unsync --skill <name> --tool <key>` | Remove from a tool |
| `skillhub update --skill <name> [--yes]` | Update from source (auto git pull) |
| `skillhub update --all [--yes]` | Update all git-managed skills |
| `skillhub delete --skill <name> [--yes]` | Delete skill and all targets |
| `skillhub push --skill <name> [-m "msg"]` | Commit & push git-managed skill |
| `skillhub sources list [--json]` | List explore sources |

All commands support `--json` for agent-friendly structured output and `--yes` to skip confirmations.

## Architecture

```
┌─────────────┐     ┌─────────────┐     ┌──────────────┐
│  CLI (clap)  │     │  GUI (React)│     │  Agent scripts│
└──────┬──────┘     └──────┬──────┘     └──────┬───────┘
       │                   │                    │
       └─────────┬─────────┘                    │
                 │                              │
          ┌──────▼──────┐              ┌────────▼────────┐
          │  core/ (Rust)│◄─────────────│  SKILL.md      │
          │  Pure logic  │              │  Agent discovery│
          └──────┬──────┘              └─────────────────┘
                 │
          ┌──────▼──────┐
          │   SQLite     │
          │  (shared db) │
          └──────────────┘
```

- **Three front-ends** share one `core/` engine and one SQLite database
- CLI and GUI state stay in sync automatically
- Agents discover `skillhub` via `SKILL.md` in their skill directories

## Supported Tools (47+)

Claude Code, Codex, Cursor, OpenCode, Cline, Augment, OpenClaw, Gemini CLI, Kiro CLI, iFlow, Qoder, Qwen Code, Antigravity, Pi, Amp, Continue, Windsurf, GitHub Copilot, and 29 more.

## Build Scripts

```bash
./scripts/build.sh            # macOS DMG
./scripts/build.sh universal  # Universal DMG (Intel + Apple Silicon)
./scripts/build.sh cli        # CLI binary only
./scripts/build.sh release    # Build + install CLI to ~/.local/bin
./scripts/build.sh win        # Windows NSIS installer
./scripts/build.sh linux      # Linux AppImage + deb
```

Build artifacts are collected to `out/` with intermediates cleaned up.

## Tech Stack

- **Frontend**: React 19 + TypeScript + Vite 7 + Tailwind CSS 4
- **Backend**: Rust (Tauri 2) + SQLite (rusqlite) + libgit2
- **CLI**: clap + same core engine as the desktop app
- **Sync**: Symlink → junction (Windows) → copy (triple fallback)

## License

MIT
