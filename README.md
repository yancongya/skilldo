# SkillDo

[English](README.md) | [简体中文](docs/README.zh.md)

> **Install once, sync everywhere.** The agent-native skill manager for 47+ AI tools.

SkillDo manages AI Agent Skills from a single source of truth and syncs them to all your coding tools. Use the **CLI** for automation, the **desktop app** for visual management, or let **agents** drive it programmatically via structured JSON output.

## Quick Start

### Install without cloning

Download the desktop installer from [GitHub Releases](https://github.com/yancongya/skilldo/releases), or install the standalone CLI directly:

```bash
# macOS (Apple Silicon or Intel; detects the architecture and verifies SHA-256)
curl -fsSL https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.sh | bash
```

```powershell
# Windows PowerShell (x64 or ARM64; detects the architecture and verifies SHA-256)
irm https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.ps1 | iex
```

The release provides `skilldo-cli-macos-aarch64.tar.gz`, `skilldo-cli-macos-x86_64.tar.gz`, `skilldo-cli-windows-x64.zip`, and `skilldo-cli-windows-arm64.zip`, each with a `.sha256` file. Cloning the source repository is only required for development.

### CLI (recommended for agents & automation)

```bash
# Install from GitHub
skilldo install --url https://github.com/anthropics/skills/tree/main/skills/skill-creator --yes

# Sync to all tools
skilldo sync --skill skill-creator --tool claude_code
skilldo sync --skill skill-creator --tool codex

# Check status
skilldo list --json
skilldo status --json
skilldo author detect --apply --json
skilldo project skills --path . --json

# Update all git-managed skills
skilldo update --all --yes

# Complete cross-device pipelines
skilldo device status --json
skilldo device pull --json
skilldo device publish --yes --json

# Browse the skill market
skilldo explore --query "rag" --json
```

### Desktop App

Download the `.dmg` for macOS or `.exe` for Windows from [Releases](https://github.com/yancongya/skilldo/releases). The GUI provides visual skill management, explore, and one-click sync. Linux can currently be built from source but is not part of the release matrix.

The desktop app checks for updates after startup and shows the Chinese release notes when a newer version is available. macOS users can download, install, and relaunch from the prompt. On Windows, the NSIS updater exits the running app automatically before installation. Manual checks remain available under **Settings → App updates**.

Because the updater signing key was established in v0.7.1, installations older than v0.7.1 must install a current release manually once. Windows v0.7.1 also requires one manual upgrade to the first release that includes the Windows updater manifest. Do not rotate the updater key for ordinary releases; losing it breaks the trusted update chain for installed clients.

### Development

```bash
git clone https://github.com/yancongya/skilldo.git
cd skilldo
npm install
npm run tauri:dev          # Desktop app (GUI + Rust backend)
npm run dev                # Web preview only (no backend)
./scripts/build.sh         # Build for current platform
```

## CLI Commands

The desktop buttons and `scripts/skilldo-pull|publish` (`.sh`/`.bat`) call the same shared pipeline. Publish refreshes sources, pushes eligible owned repositories, records exact revisions, and then uploads both the portable Profile and lossless database backup. Local-only Skills are reported but cannot be reconstructed on another computer.

When devices have different repository-backed or package-backed Skill lists, synchronization keeps their union. Concurrent tags and global tool targets for the same Skill are also combined. Project-owned Skills remain files in their parent project repository; the Profile stores only the repository URL, branch/revision, and repository-relative Skill paths. It never treats another computer's absolute project path as portable data. Source/revision disagreements and delete-versus-edit cases remain explicit conflicts so one device cannot silently replace or remove another device's state.

| Command | Description |
|---------|-------------|
| `skilldo list [--json]` | List managed skills and sync targets |
| `skilldo status [--json]` | Show which of 47+ AI tools are installed |
| `skilldo device status\|pull\|publish [--yes] [--json]` | Inspect, retrieve, or publish complete cross-device state |
| `skilldo author status\|detect\|set [--json]` | Detect or configure the current environment author without exposing the `gh` token |
| `skilldo project skills [--path <project>] [--json]` | Discover project-local Skills with parent Git repository, revision, and repository-relative paths |
| `skilldo explore [--query Q] [--json]` | Browse the skill market |
| `skilldo install --url <repo> [--name] [--yes]` | Install from git URL or local path |
| `skilldo sync --skill <name> --tool <key> [--scope project --project-path <path>]` | Sync globally or into one project |
| `skilldo unsync --skill <name> --tool <key> [--scope project --project-path <path>]` | Remove a global or project target |
| `skilldo config get\|set <key> [value] [--stdin] [--json]` | Read/write scalar or structured config; use stdin for secrets |
| `skilldo update --skill <name> [--yes]` | Update from source (auto git pull) |
| `skilldo update --all [--yes]` | Update all git-managed skills |
| `skilldo delete --skill <name> [--yes]` | Delete skill and all targets |
| `skilldo push --skill <name> [-m "msg"]` | Commit & push git-managed skill |
| `skilldo sources list [--json]` | List explore sources |
| `skilldo backup file [path] [--json]` | Export a lossless SQLite snapshot in one JSON file |
| `skilldo backup webdav [--json]` | Upload the lossless snapshot, including configured credentials |
| `skilldo restore file <path> [--json]` | Validate and restore a local snapshot |
| `skilldo restore webdav [--json]` | Validate and restore the WebDAV snapshot |
| `skilldo profile status [--json]` | Preview the WebDAV profile merge without writing |
| `skilldo profile sync [--yes] [--json]` | Merge, pull, install, and sync the shared device profile |
| `skilldo repair sources [--apply] [--json]` | Audit local records and promote verified Git-worktree sources |
| `skilldo repair source --skill <name> --url <repo> [--subpath <path>] [--apply] [--json]` | Verify a remote Skill identity, then reconnect one source |
| `skilldo profile export <path> [--json]` | Export a portable Profile without WebDAV |
| `skilldo profile import <path> [--strategy abort\|local\|remote] [--json]` | Merge an offline Profile |
| `skilldo profile resolve --strategy local\|remote [--json]` | Resolve current WebDAV conflicts and synchronize |

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
- Agents discover `skilldo` via `SKILL.md` in their skill directories

## Cross-device profiles

On every new device, install either the desktop app or standalone CLI, then configure the same WebDAV endpoint. The NAS filesystem path is not entered on clients; use its HTTPS WebDAV URL and remote directory.

```bash
skilldo config set webdav.url "https://dav.example.com" --json
skilldo config set webdav.user "username" --json
printf '%s' 'password' | skilldo config set webdav.password --stdin --json
skilldo config set webdav.remote_dir "services/skillsdo" --json

# Verify the saved non-secret values and test remote Profile access
skilldo config get webdav --json
skilldo device status --json

# Retrieve and merge the shared state
skilldo device pull --json
```

In the desktop app, enter the same values under Settings → WebDAV, save them, choose **Check device state**, then **Get updates from other devices**. To publish changes back after reviewing them, use **Publish to other devices** or `skilldo device publish --yes --json`.

The versioned `skilldo-profile.json` stores portable desired state: Git/package Skill sources and revisions, standard global targets, tags, manual origin rules, language, cache policy, and Explore sources. Git-managed Skills are cloned or pulled on the receiving computer. Independent Skill lists, tags, and targets are merged as a union.

Passwords, WebDAV credentials, storage paths, custom scan directories, per-tool path overrides, project targets, and local-only Skills never enter the Profile. A new device must enter WebDAV credentials once before it can download anything. `config get` deliberately redacts the password. Deletions are reported as pending unless explicitly confirmed. Concurrent edits are merged against each device's last synchronized base; the upload uses WebDAV ETags to prevent overwriting a newer remote revision.

If an older import was incorrectly recorded as local, run `skilldo repair sources --json` first. The audit reads standard `.agents/.skill-lock.json` provenance, content-matched Codex plugin manifests, and real Git worktrees. Review the structured report, then use `skilldo repair sources --apply --json`. Ambiguous central copies remain unresolved. For a confirmed source that lacks local metadata, use `repair source`; SkillDo clones the remote and verifies the selected directory contains a matching `SKILL.md` before writing.

The separate `skilldo-backup.json` v2 format embeds a consistent SQLite image as Base64 with a SHA-256 checksum. It preserves every database table, ID, timestamp, setting, tag, origin record, target, discovery row, index, and sequence. At the user's request it also includes GitHub and WebDAV credentials, so the backup location must be private. Repository working trees and local-only skill files are filesystem content, not database data; use the Profile/Git flow to reconstruct repository skills on another computer.

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
