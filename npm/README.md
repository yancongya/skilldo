# SkillDo

> The agent-native skill manager for 47 AI coding tools — install once, sync everywhere.

## Install

```bash
npm install -g skilldo
```

For the latest version, prefer the [standalone CLI installer](https://github.com/yancongya/skilldo#install-without-cloning) — the npm package is published on its own cadence and may lag behind GitHub Releases.

## Usage

```bash
# List managed skills
skilldo list --json

# Install a skill from GitHub
skilldo install --url https://github.com/anthropics/skills/tree/main/skills/skill-creator --yes

# Sync to a specific tool
skilldo sync --skill skill-creator --tool claude_code

# Check which AI tools are installed
skilldo status

# Browse the skill market
skilldo explore --query "rag" --json
```

## What happens during install

`npm install -g skilldo` downloads a pre-compiled native binary for your platform, verifies its SHA-256 checksum, and falls back to the GitHub API asset endpoint if the release CDN is unavailable:

| Platform | Architecture | Asset | Binary |
|---|---|---|---|
| macOS | Apple Silicon (arm64) | `skilldo-cli-macos-aarch64.tar.gz` | `skilldo-macos-aarch64` |
| macOS | Intel (x64) | `skilldo-cli-macos-aarch64.tar.gz` (via Rosetta 2) | `skilldo-macos-aarch64` |
| Windows | x64 | `skilldo-cli-windows-x64.zip` | `skilldo-windows-x64.exe` |
| Windows | arm64 | `skilldo-cli-windows-arm64.zip` | `skilldo-windows-arm64.exe` |
| Linux | — | No prebuilt asset yet — build from source with `./scripts/build.sh cli` | — |

No Node.js runtime is needed at execution — `skilldo` is a standalone Rust binary.

## Links

- [GitHub](https://github.com/yancongya/skilldo)
- [Releases](https://github.com/yancongya/skilldo/releases)
- [Documentation](https://yancongya.github.io/skilldo/)

## License

MIT
