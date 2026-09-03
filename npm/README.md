# SkillDo

> The agent-native skill manager for 45 AI coding tools — install once, sync everywhere.

## Install

```bash
npm install -g skilldo
```

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

`npm install -g skilldo` downloads a pre-compiled native binary for your platform:

| Platform | Architecture | Binary |
|---|---|---|
| macOS | Apple Silicon (arm64) | `skilldo-macos-aarch64` |
| macOS | Intel (x64) | `skilldo-macos-aarch64` (via Rosetta 2) |
| Linux | x64 | `skilldo-linux-x64` |
| Linux | arm64 | `skilldo-linux-aarch64` |
| Windows | x64 | `skilldo-windows-x64.exe` |
| Windows | arm64 | `skilldo-windows-arm64.exe` |

No Node.js runtime is needed at execution — `skilldo` is a standalone Rust binary.

## Links

- [GitHub](https://github.com/yancongya/skilldo)
- [Releases](https://github.com/yancongya/skilldo/releases)
- [Documentation](https://yancongya.github.io/skilldo/)

## License

MIT
