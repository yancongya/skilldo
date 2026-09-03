<a id="readme-top"></a>

<p align="center">
  <a href="https://github.com/yancongya/skilldo">
    <img src="./assets/logo.svg" alt="SkillDo Logo" width="96" height="96">
  </a>
</p>

<h1 align="center">SkillDo</h1>

<p align="center">
  <a href="https://github.com/yancongya/skilldo/actions"><img alt="构建" src="https://img.shields.io/github/actions/workflow/status/yancongya/skilldo/ci.yml?branch=main"></a>
  <a href="https://github.com/yancongya/skilldo/releases"><img alt="版本" src="https://img.shields.io/github/v/release/yancongya/skilldo"></a>
  <a href="https://github.com/yancongya/skilldo/blob/main/LICENSE"><img alt="许可证" src="https://img.shields.io/github/license/yancongya/skilldo"></a>
  <a href="https://github.com/yancongya/skilldo"><img alt="平台" src="https://img.shields.io/badge/平台-Windows%20%7C%20macOS%20%7C%20Linux-blue"></a>
  <a href="https://github.com/yancongya/skilldo/stargazers"><img alt="Stars" src="https://img.shields.io/github/stars/yancongya/skilldo?style=social"></a>
</p>

<p align="center">
  <b>一次安装，处处同步。</b>面向 45 个 AI 编程工具的 agent-native 技能管理器。
  <br />
  <a href="../README.md"><strong>English</strong></a>
</p>

> [!NOTE]
> 本中文版与英文版（`../README.md`）保持结构镜像——相同章节大纲、相同工具数（45）、相同术语。可用 `readme-please` 的 `check_bilingual_headings.py` 校验。

---

## 目录

<details>
  <summary>点击展开</summary>

- [项目简介](#项目简介)
- [功能特性](#功能特性)
- [快速开始](#快速开始)
- [CLI 命令总表](#cli-命令总表)
- [架构](#架构)
- [跨设备配置](#跨设备配置)
- [支持的 AI 编程工具](#支持的-ai-编程工具)
- [构建脚本](#构建脚本)
- [技术栈](#技术栈)
- [路线规划](#路线规划)
- [常见问题](#常见问题)
- [支持的系统](#支持的系统)
- [参与贡献](#参与贡献)
- [许可证](#许可证)

</details>

---

## 项目简介

SkillDo 是一个跨平台桌面应用（Tauri + React），用于统一管理 Agent Skills，并把它们同步到多种 AI 编程工具的全局或项目级 skills 目录（优先 symlink/junction，失败回退 copy），实现 “Install once, sync everywhere”。所有技能以**中心仓库（Central Repo）**为唯一可信源。

**为什么需要 SkillDo：**
- *单一可信源* —— 技能在中心仓库安装一次，处处同步（symlink → junction → copy 三重回退）。
- *45 个 AI 工具，一套流程* —— Claude Code、Codex、Cursor、Windsurf、WorkBuddy 等，统一全局/项目级同步目标。
- *Agent-native* —— 每个命令都支持 `--json`；agent 通过技能目录中的 `SKILL.md` 自动发现 `skilldo`。

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 功能特性

- **Explore 探索页**：独立页面浏览精选技能推荐和在线搜索，一键安装并同步到所有已检测工具
- **Tags 标签页**：在独立页面中新建、重命名、删除自定义标签，并快速跳转到对应的 Skill 列表
- **标签筛选**：为 Skill 添加多个标签，并在 My Skills 中按标签筛选，包括查看 `无标签` Skill
- **全局 / 项目级同步**：Skill 可同步到全局目录，在所有项目中生效；也可限定到指定项目目录中生效
- **同步范围控制**：在全局和项目范围之间切换 Skill，管理项目目录，并按范围筛选 My Skills
- **技能详情页**：点击技能名称查看完整文件内容，支持文件树浏览、Markdown 渲染和代码语法高亮（40+ 语言）
- **统一视图**：查看 Hub 托管的 skills 总数、范围徽标及其在各工具的生效状态
- **迁移接管**：扫描本机工具目录已有 skills，导入到中心仓库并可一键同步
- **多来源导入**：本地目录 / Git 仓库 URL（含可搜索的 multi-skill 候选选择、`.claude/skills/` 目录格式支持）
- **更新**：从原来源更新中心仓库内容，并回灌 copy 模式的目标
- **新工具检测**：发现新安装工具时提示是否同步所有已托管 skills

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 快速开始

### 不 clone 源码安装

一行命令启动交互式安装向导——选择 CLI、桌面客户端、或让 agent 代劳：

```bash
curl -fsSL https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install.sh | bash
```

安装器自动检测平台（macOS/Linux/Windows），显示已安装状态，然后让你选择：

| 选项 | 安装内容 | 方式 |
|---|---|---|
| **1 → CLI** | `skilldo` 二进制 → `~/.local/bin` | 自动下载 + SHA-256 校验 |
| **2 → Desktop** | `SkillDo.app` / `.exe` | 打开 GitHub Releases 下载 |
| **3 → Agent** | 通过 agent 安装 CLI | 复制 prompt 给 Claude Code / Codex / Cursor |

直接安装 CLI（无需交互菜单）：

```bash
# macOS / Linux
curl -fsSL https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.sh | bash
```

```powershell
# Windows PowerShell
irm https://raw.githubusercontent.com/yancongya/skilldo/main/scripts/install-cli.ps1 | iex
```

Release 包含 `skilldo-cli-{macos,linux}-{aarch64,x64}.tar.gz` 和 `skilldo-cli-windows-{x64,arm64}.zip`，每个都有 `.sha256` 校验文件。只有开发或自行构建时才需要 clone 完整仓库。

### 桌面客户端

从 [Releases](https://github.com/yancongya/skilldo/releases) 下载 macOS 的 `.dmg` 或 Windows 的 `.exe`。GUI 提供可视化技能管理、探索与一键同步。Linux 目前可从源码构建，但尚未进入发布矩阵。

桌面客户端启动后会自动检查 GitHub Release；发现新版本时弹出中文版本说明。macOS 可以在弹窗中下载安装，并点击“立即重启”完成替换；Windows 的 NSIS updater 会在开始安装时自动退出当前客户端。也可以随时前往“设置 → 应用更新”手动检查。

自动更新签名链从 v0.7.1 建立：低于 v0.7.1 的旧客户端需要手动安装一次当前版本；Windows v0.7.1 还需要手动升级到首个包含 Windows updater 清单的新版本。之后只要持续使用同一 updater 私钥，macOS Intel/Apple Silicon 与 Windows x64/ARM64 都可沿 Release 链更新。普通发布不得轮换 updater 公私钥。

GitHub Release 标题、更新说明、安装提示和 Changelog 发布条目默认使用中文；命令、文件名与平台标识保持原格式。

代码推送到 `main` 且 CI 全部通过后，GitHub Actions 会自动递增补丁版本、把 Unreleased 内容整理为中文版本记录、创建标签，并调用现有的 macOS/Windows 签名发布流水线。仅修改文档、自动生成的精选 Skill 列表、提交信息包含 `[skip release]` 或 CI 失败时不会发布，避免无意义版本和残缺 Release。

### CLI（推荐 agent 与自动化使用）

```bash
# 从 GitHub 安装
skilldo install --url https://github.com/anthropics/skills/tree/main/skills/skill-creator --yes

# 同步到指定工具
skilldo sync --skill skill-creator --tool claude_code
skilldo sync --skill skill-creator --tool codex

# 查看状态
skilldo list --json
skilldo status --json
skilldo author detect --apply --json
skilldo project skills --path . --json

# 更新所有 git 管理的技能
skilldo update --all --yes

# 完整的跨设备流水线
skilldo device status --json
skilldo device pull --json
skilldo device publish --yes --json

# 浏览技能市场
skilldo explore --query "rag" --json
```

### 开发

```bash
git clone https://github.com/yancongya/skilldo.git
cd skilldo
npm install
npm run tauri:dev          # 桌面端（GUI + Rust 后端）
npm run dev                # 仅 Web 预览（无后端）
./scripts/build.sh         # 构建当前平台
```

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## CLI 命令总表

所有命令均支持 `--json`（agent 友好的结构化输出）与 `--yes`（跳过确认）。桌面端按钮与 `scripts/skilldo-pull|publish`（`.sh`/`.bat`）调用同一套共享流水线。

<details>
  <summary>展开全部命令</summary>

| 命令 | 说明 |
|---------|-------------|
| `skilldo list [--json]` | 列出已托管技能与同步目标 |
| `skilldo status [--json]` | 显示 45 个 AI 工具中哪些已安装 |
| `skilldo device status\|pull\|publish [--yes] [--json]` | 检查、拉取或发布完整的跨设备状态 |
| `skilldo author status\|detect\|set [--json]` | 检测或配置当前环境作者（不暴露 `gh` token） |
| `skilldo project skills [--path <project>] [--json]` | 发现项目本地 Skill 的父 Git 仓库、revision 与仓库内相对路径 |
| `skilldo explore [--query Q] [--json]` | 浏览技能市场 |
| `skilldo install --url <repo> [--name] [--yes]` | 从 git URL 或本地路径安装 |
| `skilldo sync --skill <name> --tool <key> [--scope project --project-path <path>]` | 全局或项目内同步 |
| `skilldo unsync --skill <name> --tool <key> [--scope project --project-path <path>]` | 移除全局或项目目标 |
| `skilldo config get\|set <key> [value] [--stdin] [--json]` | 读写标量或结构化配置；密钥用 stdin |
| `skilldo update --skill <name> [--yes]` | 从来源更新（自动 git pull） |
| `skilldo update --all [--yes]` | 更新所有 git 管理的技能 |
| `skilldo delete --skill <name> [--yes]` | 删除技能及其所有目标 |
| `skilldo push --skill <name> [-m "msg"]` | 提交并推送 git 管理的技能 |
| `skilldo sources list [--json]` | 列出探索来源 |
| `skilldo backup file [path] [--json]` | 将无损 SQLite 快照导出为单个 JSON 文件 |
| `skilldo backup webdav [--json]` | 上传无损快照（含已配置凭据） |
| `skilldo restore file <path> [--json]` | 校验并恢复本地快照 |
| `skilldo restore webdav [--json]` | 校验并恢复 WebDAV 快照 |
| `skilldo profile status [--json]` | 预览 WebDAV profile 合并（不写入） |
| `skilldo profile sync [--yes] [--json]` | 合并、拉取、安装并同步共享设备 profile |
| `skilldo repair sources [--apply] [--json]` | 审计本地记录并提升已验证的 Git-worktree 来源 |
| `skilldo repair source --skill <name> --url <repo> [--subpath <path>] [--apply] [--json]` | 验证远程 Skill 身份后重连单个来源 |
| `skilldo profile export <path> [--json]` | 导出不含 WebDAV 的可移植 Profile |
| `skilldo profile import <path> [--strategy abort\|local\|remote] [--json]` | 合并离线 Profile |
| `skilldo profile resolve --strategy local\|remote [--json]` | 解决当前 WebDAV 冲突并同步 |

</details>

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 架构

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

- **三套前端**共享同一个 `core/` 引擎与同一个 SQLite 数据库
- CLI 与 GUI 状态自动保持同步
- Agent 通过其技能目录中的 `SKILL.md` 自动发现 `skilldo`

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 跨设备配置

每台设备首次使用时都要输入一次 WebDAV 网址、用户名、密码和远程目录。客户端不填 NAS 物理路径，只填 HTTPS WebDAV 地址和远程目录。

```bash
skilldo config set webdav.url "https://dav.example.com" --json
skilldo config set webdav.user "username" --json
printf '%s' 'password' | skilldo config set webdav.password --stdin --json
skilldo config set webdav.remote_dir "services/skillsdo" --json

# 读取并验证配置（密码会自动脱敏）
skilldo config get webdav --json
skilldo device status --json

# 拉取、合并并应用其他设备的状态
skilldo device pull --json
```

桌面客户端中，在“设置 → WebDAV”填写同样的内容并保存，然后依次点击“检查设备状态”和“获取其他设备更新”。需要把当前状态发布给其他设备时，使用“发布到其他设备”或：

```bash
skilldo device publish --yes --json
```

不同设备的 Git/npm Skills 列表、标签和全局同步目标会取并集。来源/revision 不一致和“删除对修改”会保留为明确冲突，不会静默覆盖。本地独有且没有仓库来源的 Skills 无法在新设备自动重建。

版本化的 `skilldo-profile.json` 存储可移植的期望状态：Git/包管理的 Skill 来源与 revision、标准全局目标、标签、手动来源规则、语言、缓存策略与 Explore 来源。Git 管理的 Skill 在接收端电脑上 clone 或 pull。独立的 Skill 列表、标签与目标以并集合并。

密码、WebDAV 凭据、存储路径、自定义扫描目录、按工具路径覆盖、项目目标与本地独有 Skills 永远不会进入 Profile。新设备必须先输入一次 WebDAV 凭据才能下载任何内容。`config get` 会刻意脱敏密码。删除操作在未显式确认前只报告为待定。并发修改按各设备最近一次同步基线合并；上传使用 WebDAV ETags 防止覆盖更新的远端 revision。

若旧导入被错误记录为本地，先运行 `skilldo repair sources --json`。审计会读取标准 `.agents/.skill-lock.json` 溯源、内容匹配的 Codex 插件清单与真实 Git worktree。审阅结构化报告后，使用 `skilldo repair sources --apply --json`。存在歧义的中心副本保持未解决。对缺少本地元数据的已确认来源，使用 `repair source`；SkillDo 会 clone 远程并验证所选目录含有匹配的 `SKILL.md` 后再写入。

独立的 `skilldo-backup.json` v2 格式将一致的 SQLite 镜像以 Base64 + SHA-256 校验和嵌入。它保留每个数据库表、ID、时间戳、设置、标签、来源记录、目标、发现行、索引与序列。应要求它也会包含 GitHub 与 WebDAV 凭据，因此备份位置必须私有。仓库工作树与本地独有技能文件是文件系统内容而非数据库数据；请用 Profile/Git 流程在其他电脑上重建仓库技能。

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 支持的 AI 编程工具

SkillDo 支持 **45** 个 AI 编程工具。项目级 skills 目录相对所选项目根目录。标记为“不支持”的工具尚未确认项目级 skills 目录，仅支持全局同步。

| tool key | 工具 | 全局 skills 目录（相对 `~`） | 项目级 skills 目录（相对项目根目录） | 存在即视为已安装（相对 `~`） |
| --- | --- | --- | --- | --- |
| `cursor` | Cursor | `.cursor/skills` | `.agents/skills` | `.cursor` |
| `claude_code` | Claude Code | `.claude/skills` | `.claude/skills` | `.claude` |
| `codex` | Codex | `.codex/skills` | `.agents/skills` | `.codex` |
| `opencode` | OpenCode | `.config/opencode/skills` | `.agents/skills` | `.config/opencode` |
| `antigravity` | Antigravity | `.gemini/antigravity/skills` | `.agents/skills` | `.gemini/antigravity` |
| `amp` | Amp | `.config/agents/skills` | `.agents/skills` | `.config/agents` |
| `kimi_cli` | Kimi Code CLI | `.config/agents/skills` | `.agents/skills` | `.config/agents` |
| `augment` | Augment | `.augment/skills` | `.augment/skills` | `.augment` |
| `openclaw` | OpenClaw | `.openclaw/skills` | `skills` | `.openclaw` |
| `copaw` | Copaw | `.copaw/skill_pool` | `.copaw/skill_pool` | `.copaw` |
| `cline` | Cline | `.agents/skills` | `.agents/skills` | `.agents` |
| `codebuddy` | CodeBuddy | `.codebuddy/skills` | `.codebuddy/skills` | `.codebuddy` |
| `command_code` | Command Code | `.commandcode/skills` | `.commandcode/skills` | `.commandcode` |
| `continue` | Continue | `.continue/skills` | `.continue/skills` | `.continue` |
| `crush` | Crush | `.config/crush/skills` | `.crush/skills` | `.config/crush` |
| `junie` | Junie | `.junie/skills` | `.junie/skills` | `.junie` |
| `iflow_cli` | iFlow CLI | `.iflow/skills` | `.iflow/skills` | `.iflow` |
| `kiro_cli` | Kiro CLI | `.kiro/skills` | `.kiro/skills` | `.kiro` |
| `kode` | Kode | `.kode/skills` | `.kode/skills` | `.kode` |
| `mcpjam` | MCPJam | `.mcpjam/skills` | `.mcpjam/skills` | `.mcpjam` |
| `mistral_vibe` | Mistral Vibe | `.vibe/skills` | `.vibe/skills` | `.vibe` |
| `mux` | Mux | `.mux/skills` | `.mux/skills` | `.mux` |
| `openclaude` | OpenClaude IDE | `.openclaude/skills` | `.openclaude/skills` | `.openclaude` |
| `openhands` | OpenHands | `.openhands/skills` | `.openhands/skills` | `.openhands` |
| `pi` | Pi | `.pi/agent/skills` | `.pi/skills` | `.pi` |
| `qoder` | Qoder | `.qoder/skills` | `.qoder/skills` | `.qoder` |
| `qoderwork` | QoderWork | `.qoderwork/skills` | `.qoderwork/skills` | `.qoderwork` |
| `qwen_code` | Qwen Code | `.qwen/skills` | `.qwen/skills` | `.qwen` |
| `trae` | Trae | `.trae/skills` | `.trae/skills` | `.trae` |
| `trae_cn` | Trae CN | `.trae-cn/skills` | `.trae/skills` | `.trae-cn` |
| `zencoder` | Zencoder | `.zencoder/skills` | `.zencoder/skills` | `.zencoder` |
| `neovate` | Neovate | `.neovate/skills` | `.neovate/skills` | `.neovate` |
| `pochi` | Pochi | `.pochi/skills` | `.pochi/skills` | `.pochi` |
| `adal` | AdaL | `.adal/skills` | `.adal/skills` | `.adal` |
| `kilo_code` | Kilo Code | `.kilocode/skills` | `.kilocode/skills` | `.kilocode` |
| `roo_code` | Roo Code | `.roo/skills` | `.roo/skills` | `.roo` |
| `goose` | Goose | `.config/goose/skills` | `.goose/skills` | `.config/goose` |
| `gemini_cli` | Gemini CLI | `.gemini/skills` | `.agents/skills` | `.gemini` |
| `github_copilot` | GitHub Copilot | `.copilot/skills` | `.agents/skills` | `.copilot` |
| `clawdbot` | Clawdbot | `.clawdbot/skills` | `.clawdbot/skills` | `.clawdbot` |
| `droid` | Droid | `.factory/skills` | `.factory/skills` | `.factory` |
| `windsurf` | Windsurf | `.codeium/windsurf/skills` | `.windsurf/skills` | `.codeium/windsurf` |
| `moltbot` | MoltBot | `.moltbot/skills` | `.moltbot/skills` | `.moltbot` |
| `hermes_agent` | Hermes Agent | `.hermes/skills` | 不支持 | `.hermes` |
| `workbuddy` | WorkBuddy | `.workbuddy/skills` | `.workbuddy/skills` | `.workbuddy` |

> 工具数由 `readme-please` 的 `gen_tool_table.py` 从源码自动生成——请保持同步，不要手抄维护。

完整路径规则与检测逻辑见 [`src-tauri/src/core/tool_adapters/mod.rs`](../src-tauri/src/core/tool_adapters/mod.rs)。

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 构建脚本

```bash
./scripts/build.sh            # macOS DMG
./scripts/build.sh universal  # Universal DMG (Intel + Apple Silicon)
./scripts/build.sh cli        # 仅 CLI 二进制
./scripts/build.sh release    # 构建并安装 CLI 到 ~/.local/bin
./scripts/build.sh win        # Windows NSIS 安装包
./scripts/build.sh linux      # Linux AppImage + deb
```

构建产物收集到 `out/`，中间文件会被清理。

推送到 `main` 且 CI 通过后，若改动涉及应用或 CLI 代码，会自动发布：递增补丁版本、把 Unreleased 整理为中文版本记录、创建标签并触发已有的 macOS/Windows 签名发布流水线。仅修改文档、自动生成的精选 Skill 列表、含 `[skip release]` 的提交或 CI 失败都不会发布。

各系统构建命令（来自 `package.json`）：

- macOS（dmg）：`npm run tauri:build:mac:dmg`
- macOS（universal dmg）：`npm run tauri:build:mac:universal:dmg`
- Windows（MSI）：`npm run tauri:build:win:msi`
- Windows（NSIS exe）：`npm run tauri:build:win:exe`
- Windows（MSI+NSIS）：`npm run tauri:build:win:all`
- Linux（deb）：`npm run tauri:build:linux:deb`
- Linux（AppImage）：`npm run tauri:build:linux:appimage`
- Linux（deb+AppImage）：`npm run tauri:build:linux:all`

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 技术栈

- **前端**：React 19 + TypeScript + Vite 7 + Tailwind CSS 4
- **后端**：Rust（Tauri 2）+ SQLite（rusqlite）+ libgit2
- **CLI**：clap + 与桌面端相同的 core 引擎
- **同步**：Symlink → junction（Windows）→ copy（三重回退）

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 路线规划

- [x] 桌面端（macOS + Windows）可视化技能管理
- [x] 独立 CLI，支持结构化 `--json` 输出
- [x] 基于 WebDAV 的跨设备 profile
- [ ] Linux 一等发布矩阵（deb + AppImage 进入发布流水线）
- [ ] 更多 AI 工具与更精细的项目级路径检测
- [ ] 更丰富的 Explore 市场与精选目录

完整清单见 [开放议题](https://github.com/yancongya/skilldo/issues)。

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 常见问题

- Skill 存在哪里？中心仓库（Central Repo）默认是 `~/.skillshub`，可在设置里修改。
- 标签用于什么？标签只用于查找和整理 Skill，不会改变 Skill 的同步目录，也不会改变哪些工具可以使用它。
- 什么是项目级同步？外部通用 Skill 仍然只在中心仓库保存一份，项目目录只是它的同步目标。项目自己维护的 Skill 则应直接提交到父项目 Git 仓库。跨设备 Profile 只记录父仓库 URL、分支/revision 和仓库内相对路径，不保存电脑 A 的绝对项目路径；电脑 B clone/pull 父项目后即可重新识别。
- Cursor 为什么强制 Copy？Cursor 当前不支持软链（symlink/junction）形式的技能目录，因此同步到 Cursor 时会固定使用目录复制（copy）。
- 为什么有时会变成 Copy？默认优先 symlink/junction，但在某些系统（尤其 Windows）可能因为权限/策略导致无法创建链接，会自动回退到目录复制。
- `TARGET_EXISTS|...` 是什么意思？目标目录已存在且默认不覆盖（为了安全）。你需要先清理目标目录，或在“接管/覆盖”的明确流程里重试。
- macOS Gatekeeper 备注（未签名/未公证构建，不同 macOS 版本表现可能不同）：如提示“已损坏/无法验证开发者”，可执行 `xattr -cr "/Applications/SkillDo.app"`（[参考](https://v2.tauri.app/distribute/#macos)）。

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 支持的系统

- macOS（已验证）
- Windows（按架构应支持，未做本地验证）
- Linux（按架构应支持，未做本地验证）

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 参与贡献

1. Fork 本仓库
2. 创建特性分支（`git checkout -b feature/amazing-feature`）
3. 提交改动（`git commit -m 'Add some amazing feature'`）
4. 推送分支（`git push origin feature/amazing-feature`）
5. 提交 Pull Request

提交前请运行 `npm run check`。涉及应用/CLI 的代码改动会在 CI 通过后自动发布；在提交信息中加入 `[skip release]` 可抑制发布。

<p align="right">(<a href="#readme-top">回到顶部</a>)</p>

## 许可证

基于 [MIT License](../LICENSE) 发布。
