# Skills Hub - 长期笔记

## 产品方向（讨论中，未拍板）
- 老板探索把 Skills Hub 做成 agent-native：除桌面客户端外，提供 CLI，让 agents（Claude Code / Codex / Cursor 等）通过 shell 读取与操作（安装/同步技能），不依赖打开 GUI。
- 结论：CLI 化**可行且优于 web 化** —— CLI 跑在本机、有完整文件系统权限（读 SQLite、扫 47 工具目录、git clone、建 symlink 全可做），web 做不到；且 agents 本就通过 shell 调命令，天然友好。
- 参考仓库：HKUDS/CLI-Anything（agent-native 范式：结构化 JSON 输出 + 自动生成 SKILL.md 让 agent 自发现 + --help 自描述）；suchlab/anything-cli（Rust + clap，REST API → CLI，技术栈与本项目一致可借鉴）。
- 实现路径：`core/` 已是纯 Rust 业务层、与 Tauri 解耦；新增 `cli/` 层用 clap 直接调 core，`commands/` 是 Tauri 壳可平行复用。CLI 与 Tauri GUI **互补**（人用 GUI、agent 用 CLI），共享 core 引擎。

## 架构关键（来自 AGENTS.md，补充强调）
- `core/` = 纯业务逻辑（与 Tauri 解耦、可独立测试）；`commands/` = Tauri command 壳（仅 DTO 转换/错误格式化）；`src/` = React 前端。
- 加新能力优先放 core，再在 commands 注册；前端走 `invoke`。web 预览是浏览器模式，无 Tauri 后端会报 "Tauri API is not available"。
