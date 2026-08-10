# 构建Windows桌面应用

用于在 Windows 10 22H2 / Windows 11 x64 上准备固定构建环境、生成 NSIS x64 安装包，并验证本地候选制品。

## 前置条件

- Windows 10 22H2 或 Windows 11 x64。
- Node.js 16 或更高版本及 npm。
- Rustup、`stable-x86_64-pc-windows-gnu` 和 MSYS2 UCRT64；MSYS2 需要 `windres.exe`、`gcc.exe`、`ar.exe`。
- 首次准备依赖时可以访问 npm、crates.io 和 nodejs.org；网络资源会缓存到 `.codex-build/cache`，后续构建优先复用已校验缓存。

## 首次准备

在工作空间根目录的 PowerShell 中运行：

```powershell
npm run bootstrap
```

脚本会在依赖目录缺失时执行 `npm ci`，否则复用并检查已有依赖；随后检查固定 Rust target，并准备 `@openai/codex 0.147.0` 和 Node.js `24.19.0` 资源。Codex 使用项目级 npm cache，Node.js MSI 使用固定 SHA-256 校验并按摘要缓存；缓存命中时不重复下载，校验失败会删除并重试，`.partial` 文件不会被复用。输出包含 `bootstrapped: true` 即完成；失败时按脚本最后一条错误修复后重复执行。

## 日常构建

只编译 Windows x64 release 应用、不生成安装包：

```powershell
npm run build
```

成功标志是 `.codex-build/build/0.1.0/windows-x64/tauri-codex.exe` 存在。

生成 NSIS x64 安装包：

```powershell
npm run installer:build
npm run installer:verify
```

验证成功会输出 `verified: true`。候选安装包和校验清单位于 `.codex-build/releases/0.1.0/windows-x64/`。兼容命令 `npm run app:bundle` 等价于 `npm run installer:build`。

## 一次完成

需要构建并验证完整候选时运行：

```powershell
npm run build:release
```

该命令只生成本机候选，不安装、不上传、不发布。已有候选可以单独运行 `npm run verify:release` 复核。修改前端、Rust 或正式文档后运行 `npm run test`，它会执行前端构建、Rust 定向检查/单测和文档门禁。

## 自动触发

`.github/workflows/windows-release.yml` 支持两种触发方式：推送形如 `v0.1.0` 的 tag，或在 GitHub Actions 页面手工运行并填写 `0.1.0`。workflow 会先恢复按锁文件和固定版本键控的 `.codex-build/cache`、Cargo registry/git 与 Tauri target，再执行同一套 `build:release` 和 `verify:release`；只有 tag 触发会把已验证 NSIS 上传到对应 GitHub Release，手工触发只保留 Actions artifact。Actions cache 不是发布资产。
