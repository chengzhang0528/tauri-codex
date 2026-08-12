# 构建Windows桌面应用

用于在 Windows 10 22H2 / Windows 11 x64 上准备固定构建环境、生成 Here 式 NSIS x64 薄安装器与组件闭包，并验证本地候选制品。

## 前置条件

- Windows 10 22H2 或 Windows 11 x64。
- Node.js `^18.0.0`、`^20.0.0` 或 `>=22.0.0` 及 npm；CI 使用 Node.js `24.19.0`。
- Rustup、`stable-x86_64-pc-windows-gnu` 和 MSYS2 UCRT64；MSYS2 需要 `windres.exe`、`gcc.exe`、`ar.exe`。
- 首次准备依赖时可以访问 npm、crates.io 和 nodejs.org；网络资源会缓存到 `.codex-build/cache`，后续构建优先复用已校验缓存。

## 首次准备

在工作空间根目录的 PowerShell 中运行：

```powershell
npm run bootstrap
```

脚本会在依赖目录缺失时执行 `npm ci`，否则复用并检查已有依赖；随后检查固定 Rust target，并准备构建组件所需的 `@openai/codex 0.147.0` 与 Node.js `24.19.0` 资源。Installer 本身不再包含这两个大资源；它们会在 `installer:build` 阶段生成独立资产并写入清单。输出包含 `bootstrapped: true` 即完成。

## 日常构建

只编译 Windows x64 release 应用、不生成安装包：

```powershell
npm run build
```

成功标志是 `.codex-build/build/0.1.2/windows-x64/tauri-codex.exe` 存在。

生成 NSIS x64 薄安装器与组件闭包：

```powershell
npm run installer:build
npm run installer:verify
```

验证成功会输出 `verified: true`。当前候选 Installer、`manifest.json`、`bootstrap.json` 和 `components/` 位于 `.codex-build/releases/0.1.3/windows-x64/`。Installer 只负责安装稳定入口；首次启动再由 Launcher 下载并校验组件。兼容命令 `npm run app:bundle` 等价于 `npm run installer:build`。

## 一次完成

需要构建并验证完整候选时运行：

```powershell
npm run build:release
```

该命令只生成本机候选，不安装、不上传、不发布。已有候选可以单独运行 `npm run verify:release` 复核。修改前端、Rust 或正式文档后运行 `npm run test`，它会执行前端构建、Rust 格式/类型检查与定向单测，以及文档门禁。修改依赖时再运行 `npm run audit:dependencies`。

## 自动触发

`.github/workflows/ci.yml` 在 Pull Request 和 `main` 推送时安装锁定依赖，执行依赖审计和 `npm test`。`.github/workflows/windows-release.yml` 支持推送形如 `v0.1.3` 的 tag，或在 GitHub Actions 页面手工运行并填写 `0.1.3`；它恢复构建缓存后先执行同一测试门禁，再运行 `build:release` 和 `verify:release`。Installer 首次发布 tag 会上传薄 Installer，普通 tag 复用该公开资产并只上传 manifest、Bootstrap 和组件。手工触发只保留 Actions artifact，Actions cache 不是发布资产。
