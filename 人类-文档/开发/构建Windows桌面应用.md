# 构建Windows桌面应用

用于在 Windows 10 22H2 / Windows 11 x64 上准备固定构建环境、生成 Here 式 NSIS x64 薄安装器与组件闭包，并验证本地候选制品。

## 前置条件

- Windows 10 22H2 或 Windows 11 x64。
- 日常开发可使用 Node.js `^18.0.0`、`^20.0.0` 或 `>=22.0.0` 及 npm；候选构建必须不低于 `app/build-versions.json` 固定的 Node.js `24.19.0`，CI 使用该精确版本。
- Rustup、`stable-x86_64-pc-windows-gnu` 和 MSYS2 UCRT64；MSYS2 需要 `windres.exe`、`gcc.exe`、`ar.exe`。
- 首次准备依赖时可以访问 npm、crates.io 和 nodejs.org；网络资源会缓存到 `.codex-build/cache`，后续构建优先复用已校验缓存。

## 首次准备

在工作空间根目录的 PowerShell 中运行：

```powershell
npm run bootstrap
```

脚本会在依赖目录缺失时执行 `npm ci`，否则复用并检查已有依赖；随后检查固定 Rust target，并准备构建组件所需的 `@openai/codex 0.147.0` 与 Node.js `24.19.0` 资源。Installer 本身不再包含这两个大资源；它们会在 `installer:build` 阶段生成独立资产并写入清单。输出包含 `bootstrapped: true` 即完成。

## 日常构建

普通修复发布前运行 `npm run release:patch` 自动准备下一 Manager patch。修改了 Launcher、Installer 或 Installer-owned 资源时，再运行 `npm run release:installer:patch` 自动准备下一 Installer patch，并同步最低 Manager 版本；两个命令都只修改版本源，不构建、不上传。

只编译 Windows x64 release 应用、不生成安装包：

```powershell
npm run build
```

成功标志是 `.codex-build/build/0.2.1/windows-x64/tauri-codex.exe` 存在。

生成 NSIS x64 薄安装器与组件闭包：

```powershell
npm run installer:build
npm run installer:verify
```

验证成功会输出 `verified: true`。当前候选 Installer、`manifest.json`、`bootstrap.json` 和 `components/` 位于 `.codex-build/releases/0.2.1/windows-x64/`。Installer 只负责安装稳定入口；首次启动再由 Launcher 检测系统 Node，低于清单版本时自动下载固定 MSI、请求 UAC 安装并重新验证，然后下载和安全解包其余组件。兼容命令 `npm run app:bundle` 等价于 `npm run installer:build`。

## 一次完成

需要构建并验证完整候选时运行：

```powershell
npm run build:release
```

该命令只生成本机候选，不安装、不上传、不发布。已有候选可以单独运行 `npm run verify:release` 复核。修改前端、Rust 或正式文档后运行 `npm run test`，它会执行前端构建、Rust 格式/类型检查与定向单测，以及文档门禁。修改依赖时再运行 `npm run audit:dependencies`。

## CI 与候选

`.github/workflows/ci.yml` 在 Pull Request 和 `main` 推送时安装锁定依赖，执行依赖审计和 `npm test`。self-use 候选只通过 GitHub Actions 页面手工运行 `.github/workflows/windows-release.yml`：选择 `candidate` 并填写准备发布的完整 40 位 source commit，版本由该 commit 的 `app/package.json` 自动解析。该操作不需要 Authenticode PFX 或 Ed25519 发布密钥，只构建并复核一次，保留 14 天的 frozen candidate artifact；它不写 OSS、不创建 tag，也不创建 GitHub Release。

候选通过独立验收后，已获 Deployment 授权的执行者才可对同一 source commit 和 candidate run ID 运行 `publish`。该操作先验证 OSS 写入与匿名回读，再上传不可变 closure、保存旧 Bootstrap 精确快照，最后条件提交新 Bootstrap。公开 OSS 安装验收通过后运行 `finalize` 创建 tag 和只含 OSS 链接的 GitHub Release Notes。提交后验收失败时，在创建 tag 前用同一 candidate run ID 与 publish run ID 运行 `rollback`；它只在 Bootstrap 仍指向本候选时恢复快照，不删除已上传但未引用的不可变对象。
