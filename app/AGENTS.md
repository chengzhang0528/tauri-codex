# tauri-codex 源码入口

Status: Active
Kind: AgentEntry
Scope: tauri-codex / app
Owner: 项目维护者
Updated: 2026-08-15
Depends On:
- ../文档/项目/项目_tauri-codex/AGENTS.md

源码根已建立并使用 Tauri 2、TypeScript/Vite 和 Rust。Windows x64 NSIS Setup 是 per-machine 薄安装器，只携带稳定 Launcher、self-use Bootstrap seed、图标和许可证；Launcher 作为隐藏 Broker 从固定项目 OSS 单源读取 schema v3 self-use closure，准备并激活 Manager、应用私有 Codex 和 Node fallback。桌面运行时由 `src-tauri/` 管理每会话独立的 Session Host、ConPTY、应用专属 `CODEX_HOME` 和模型实例文件；Manager 更新命令只通过受限 Windows Named Pipe 提交意图，事务由 Launcher 独占。`src/` 负责主页面内嵌的原生 Codex TUI xterm、会话启动器、模型实例、设置和单一更新控制。

## 命令

- `npm install`：安装前端与 Tauri CLI 依赖。
- `npm run build`：执行 TypeScript 检查并构建前端。
- `npm run tauri:windows -- dev`：自动定位 Rustup 和 MSYS2 UCRT64 后启动桌面开发模式。
- `npm run tauri:windows -- build`：自动定位 Rustup 和 MSYS2 UCRT64 后构建 Windows NSIS x64 安装制品。
- `rustup run stable-x86_64-pc-windows-gnu cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`：检查 Rust 格式。
- `rustup run stable-x86_64-pc-windows-gnu cargo check --manifest-path src-tauri/Cargo.toml`：检查 Rust 类型和依赖。
- `rustup run stable-x86_64-pc-windows-gnu cargo test --release --manifest-path src-tauri/Cargo.toml --lib --target x86_64-pc-windows-gnu`：运行 Windows x64 release 定向单元测试。

## 门禁

- 只启动应用私有 Codex 版本；不得搜索或调用系统全局 Codex。
- 每次启动 Codex 显式设置应用专属 `CODEX_HOME`；不得读取用户目录 `~/.codex`。
- Codex session、历史、恢复和删除由 Codex TUI 原生命令和数据负责；应用不建立 session 数据库，不保存 thread id，不读取内部 session 文件。
- 新会话启动无子命令的内置 `codex`，第一条 TUI 输入由 Codex 创建 session；恢复会话启动内置 `codex resume` 原生选择器。
- 应用不得使用 app-server、`codex exec --json` 或 Responses API 事件重建聊天；Codex TUI 控制序列必须经 ConPTY 原样交给 xterm。
- 主 Tauri 窗口是唯一产品窗口；禁止为会话创建 `WebviewWindow`、弹窗或外部终端。新建、恢复和会话选择必须在主页面内嵌 xterm 中呈现。
- 每个会话必须独立运行自己的 Session Host、ConPTY、xterm 和内置 Codex 进程树；隐藏或切换终端不得合并、停止、重启或脱离其运行链。这是不可变硬门禁。
- 左侧会话目录只能投影当前 `SessionManager` 运行实例，并按工作目录分组用于切换 xterm；不得为目录读取 Codex 内部 session 文件、保存 thread id 或建立持久会话索引。
- 应用不设置并发上限；输出背压或单会话故障不得阻塞主窗口和其他会话。
- Windows 10 22H2 x64 与 Windows 11 x64 共用路径；不得引入仅 Windows 11 的系统 API。
- GitHub 不得存放运行时二进制；Manager 不得下载、解包、安装、激活或独立更新 Codex。所有 release 组件只由 Launcher 从固定 OSS 根按显式 self-use manifest 管理。
- self-use 只允许 tauri-codex 自有 Launcher、Manager 和 Installer 无 Authenticode；Codex、Node 与 WebView2Loader 仍必须通过可用的上游 Authenticode 校验。
- schema v1/v2、GitHub binary fallback、独立 Codex `current` 和 `installer@version` 不兼容也不迁移；旧安装必须重新运行新版 Installer。
- 不写入示例密钥、Token、密码或生成日志。
