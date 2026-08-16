# tauri-codex

面向 Windows 10 22H2 / Windows 11 x64 的 Codex 桌面封装，使用 Here 式薄安装器、唯一 Tauri 主窗口、内嵌 xterm、ConPTY 和应用私有 `@openai/codex`。

tauri-codex 是独立的社区项目，不是 OpenAI 官方产品，也不代表 OpenAI 提供支持或担保。

## 当前可用范围

当前源码候选版本为 `v0.2.1`，对应的 Windows x64 Setup 尚未正式发布。当前请按[构建指南](人类-文档/开发/构建Windows桌面应用.md)从源码启动或生成 NSIS 薄安装器；正式发布后，安装程序只从项目固定阿里云 OSS 前缀提供。

- 支持 Windows 10 22H2 x64 和 Windows 11 x64。
- 不支持更早的 Windows、Windows ARM64、macOS 或 Linux；这些场景可以按[支持说明](SUPPORT.md)提交功能建议。
- 应用启动和运行期间会从固定 OSS 检查显式 self-use release closure，并可下载、校验和暂存兼容的 Manager、Codex 与 Node 组件；自有二进制按 size/SHA-256 校验，Codex 按固定上游包、精确可执行闭包和安装树摘要校验，并继续验证包内具备签名的 OpenAI 可执行文件，Node 与 WebView2 继续校验上游 Authenticode。用户仍需明确点击激活更新，活动会话不会被自动中断。新版 Installer/Launcher 只显示 `setup-required`，必须由用户在应用外运行新版 Setup。
- Bug、兼容性问题和使用反馈通过 GitHub Issues 跟踪，提交入口和必要信息见[支持说明](SUPPORT.md)。
- 安全漏洞不要公开披露，按[安全政策](SECURITY.md)报告。

## 项目入口

- [构建 Windows 桌面应用](人类-文档/开发/构建Windows桌面应用.md)
- [全部人类文档](人类-文档/README.md)
- [产品契约](文档/项目/项目_tauri-codex/产品契约.md)
- [Windows x64 封装方案](文档/项目/项目_tauri-codex/决策/DEC-0001-Windows-x64-Codex桌面封装方案.md)
- [贡献指南](CONTRIBUTING.md)

## 许可证

Copyright (C) 2026 chengzhang0528 and tauri-codex contributors.

项目自有代码采用 [GNU GPL v3 或更高版本](LICENSE)。个人使用、企业内部使用、修改，以及符合 GPL 的再分发均可免费进行。

希望在不履行 GPL 源码公开义务的情况下进行闭源再分发、OEM、白标或闭源产品集成时，需要取得[商业授权](COMMERCIAL-LICENSE.md)。第三方组件继续适用各自的许可证，详见[第三方声明](THIRD_PARTY_NOTICES.md)。

提交贡献前请阅读[贡献指南](CONTRIBUTING.md)、[行为准则](CODE_OF_CONDUCT.md)和[贡献者许可协议](CLA.md)。
