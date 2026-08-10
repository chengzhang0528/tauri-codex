# tauri-codex 项目智能体入口

Status: Active
Kind: AgentEntry
Scope: tauri-codex / 项目事实路由
Owner: 项目维护者
Updated: 2026-08-10
Depends On:
- ../../WORKSPACE_STRUCTURE.md

当前产品契约与技术决策已建立，源码根为 [app](../../../app/AGENTS.md)；尚无独立服务、端口或 Runbook。

## 路由

- 产品目标、首个交付边界和产品不变量：读 [产品契约](产品契约.md)。
- Windows x64 封装、原生 Codex TUI、并发隔离、配置与 Codex 更新方向：读 [DEC-0001](决策/DEC-0001-Windows-x64-Codex桌面封装方案.md)。
- 根脚本、固定构建版本、安装包验证与 GitHub Actions 触发：读 [Windows构建与发布根脚本](运行/RUN-0001-Windows构建与发布根脚本.md)。
- 源码实现、命令与验证：读 [app 源码入口](../../../app/AGENTS.md)。
- 涉及编码时，必须先获得用户明确授权；源码根已验证为 `app/`，后续公共契约或目录职责变化必须同步更新本入口与 `文档/WORKSPACE_STRUCTURE.md`。

## 门禁

- `Status: Accepted` 的 Decision 固定已确认方向，但不是 CurrentDesign、编码计划或实施授权；用户后续纠正直接修订同一 Decision。
- 不把方案中的候选技术、命令、目录或更新策略当作已实现事实。
- 后续实现必须同时兼容 Windows 10 22H2 x64 和 Windows 11 x64；不得引入仅 Windows 11 可用且没有 Windows 10 路径的系统 API、安装行为或终端实现。
- 不读取或写入用户真实密钥；不得为产品擅自增加 Codex 之外的账号、密码或凭据管理体系。
- 应用运行时配置和用户输入的 Server SK 只写入用户本机应用数据目录，不写入仓库或样例文件。
