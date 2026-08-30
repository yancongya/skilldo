# skilldo (原 Skills Hub) - 长期笔记

## 产品方向（讨论中，未拍板）
- 老板探索把 Skills Hub 做成 agent-native：除桌面客户端外，提供 CLI，让 agents（Claude Code / Codex / Cursor 等）通过 shell 读取与操作（安装/同步技能），不依赖打开 GUI。
- 结论：CLI 化**可行且优于 web 化** —— CLI 跑在本机、有完整文件系统权限（读 SQLite、扫 47 工具目录、git clone、建 symlink 全可做），web 做不到；且 agents 本就通过 shell 调命令，天然友好。
- 参考仓库：HKUDS/CLI-Anything（agent-native 范式：结构化 JSON 输出 + 自动生成 SKILL.md 让 agent 自发现 + --help 自描述）；suchlab/anything-cli（Rust + clap，REST API → CLI，技术栈与本项目一致可借鉴）。
- 实现路径：`core/` 已是纯 Rust 业务层、与 Tauri 解耦；新增 `cli/` 层用 clap 直接调 core，`commands/` 是 Tauri 壳可平行复用。CLI 与 Tauri GUI **互补**（人用 GUI、agent 用 CLI），共享 core 引擎。

## 架构关键（来自 AGENTS.md，补充强调）
- `core/` = 纯业务逻辑（与 Tauri 解耦、可独立测试）；`commands/` = Tauri command 壳（仅 DTO 转换/错误格式化）；`src/` = React 前端。
- 加新能力优先放 core，再在 commands 注册；前端走 `invoke`。web 预览是浏览器模式，无 Tauri 后端会报 "Tauri API is not available"。

## 项目更名 + 脱离 fork（2026-08-30 完成）
- 项目从 **Skills Hub / skills-hub** 更名为 **skilldo / SkillDo**，并完全脱离上游 fork（原 qufei1993/skills-hub，后改名 yancongya/skills-hub，但仍是 fork）。
- 新仓库 **yancongya/skilldo**：`gh repo create` 全新创建（非 fork，isFork=false），原 yancongya/skills-hub 已归档（archived=true）。
- 全量迁移：11 个分支 + 11 个 tag（v0.1.0→v0.6.1）已 push 到 skilldo；本地 origin 已指向 skilldo。
- 命名：仓库/包名 `skilldo`、CLI 命令 `skilldo`、显示名 `SkillDo`。
- **务必保留不变**：`APP_IDENTIFIER = "com.qufei1993.skillshub"` 与 `CENTRAL_DIR_NAME = ".skillshub"` —— 改了会分裂 SQLite 数据库路径、丢失已有用户数据。
- 品牌名唯一来源：`src-tauri/src/core/config.rs` 的 `PRODUCT_NAME`("SkillDo") / `CLI_NAME`("skilldo")；Cargo.toml `[[bin]] name` 与 tauri.conf.json `productName`/窗口 `title` 须与之同步。

## 设置/配置能力现状（2026-08-30 调研）
- **Settings 页（SettingsPage.tsx）覆盖**：界面语言、技能存储路径、git 缓存清理天数/TTL、GitHub token、origin rules（officialGitRepos 等）、各工具 skills 目录覆盖（tool dir override）、自定义扫描目录（custom scan dirs）、应用更新。全部走 `invoke` + `SkillStore` settings 持久化；**非 Tauri/网页预览模式全部不可用**（isTauri 门控）。
- **skills 源（explore sources）管理不在 Settings，而在 Explore 页源弹窗**：增/删/改/启用禁用，kind 支持 featured_json/skills_sh/json_index/git_index，可填 endpoint；`save_explore_sources` 真正落盘（重启 merge 回 `get_explore_sources`）。
- **GitHub 认证 = 仅 PAT 明文 token**：Settings 里一个密码框，`set_github_token` 只 trim 存储，**无 OAuth/登录流程、无格式/有效性校验**（grep 全仓无 oauth/authorize/device_code）。
- **状态（tool status）不是配置项**：`get_tool_status` 扫描所有 adapter 返回 installed/skills_dir/newly_installed，但 GUI 里是**嵌入式使用**（AddSkillModal 同步勾选、SkillCard 已同步工具、NewToolsModal 新装提示），**无独立状态页/仪表盘**；完整状态视图只在 CLI `skilldo status`。
- 结论：三块功能都能动，但「源」管理主要搬到 Explore 页、GitHub 只有 token 框、状态不可配置且无 GUI 汇总页。缺口：① 源管理是否收进 Settings 统一入口；② GitHub 加 token 校验/OAuth；③ GUI 加状态页对齐 `skilldo status`。
