# Windows构建与发布根脚本

Status: Active
Kind: Runbook
Scope: tauri-codex / Windows x64 构建、NSIS 制品与 GitHub Actions 触发
Owner: 项目维护者
Updated: 2026-08-11
Depends On:
- ../../../../package.json
- ../../../../app/build-versions.json
- ../../../../.github/workflows/ci.yml
- ../../../../.github/workflows/windows-release.yml

所有根脚本从工作空间根目录执行。固定构建版本由 `app/build-versions.json` 唯一管理；本地候选制品和校验清单只写入 Git 忽略的 `.codex-build/`。

网络资源缓存也固定在 `.codex-build/cache/`：Node.js MSI 按 SHA-256 保存于 `node/<sha256>/`，Codex 安装使用同一目录下的 `npm/` 缓存，Rust 依赖沿用 Cargo 的用户缓存。资源只有在摘要校验通过后才会从 `.partial` 原子改名为正式缓存文件；损坏缓存会被丢弃并重新下载，`.partial` 文件永远不会被复用。Node/Codex 所需条目命中缓存时不重复下载。需要代理时由执行环境提供代理变量（例如 Windows `curl.exe` 支持的 `ALL_PROXY`），不会写入应用配置。

| 脚本 | 职责 | 副作用 |
|---|---|---|
| `bootstrap` | 检查 Rust GNU、Rust target、MSYS2 UCRT64，执行 `npm ci` 并准备固定 Codex/Node 资源；优先复用 `.codex-build/cache/` | 修改 `app/node_modules`、缓存与被忽略的资源目录 |
| `build` | 编译 Windows x64 release 应用，不生成安装包 | 写入 `.codex-build/build/` 与 Tauri target |
| `installer:build` | 构建 NSIS x64，并复制到固定候选目录 | 写入 `.codex-build/releases/` 与 Tauri target |
| `installer:verify` | 校验版本、目标、资源版本、文件大小和 SHA-256 | 只读 |
| `build:release` | 依次执行 bootstrap、installer build、installer verify | 只生成本机候选，不上传、不安装 |
| `verify:release` | 对已有候选重复执行 release 验证 | 只读 |
| `test:rust` / `test` | 运行 Rust 格式、类型检查与定向单测；`test` 另含前端和文档门禁 | 只写入被忽略构建缓存 |

兼容入口 `app:bundle` 等价于 `installer:build`；开发入口仍是 `app:dev`。GitHub Actions 的 `ci.yml` 在 Pull Request、`main` 推送和手工触发时先用根 `bootstrap` 准备锁定依赖与安装包资源，再执行依赖审计和 `npm test`。`windows-release.yml` 在 `vX.Y.Z` tag 或手工触发时执行同一 bootstrap 与源码门禁，再执行 `build:release` 和 `verify:release`；`build:release` 内部的重复 bootstrap 只复用已准备输入和验证缓存。tag 触发才把已验证安装包上传到同名 GitHub Release，本地脚本不承担发布。

重试时直接重复同一个脚本即可。固定版本不匹配、工具链缺失、资源版本不一致或校验失败时脚本立即停止；网络恢复后，已验证缓存会避免重复下载，不覆盖已存在的不同候选。setup-msys2 的安装根由 Action 动态决定，CI 与 Release Workflow 都在其 `msys2 {0}` shell 中把 `/ucrt64/bin` 转为 Windows 路径并通过 `TAURI_MINGW_BIN` 交给根脚本，不依赖 runner 临时目录。GitHub Actions 通过 `actions/cache@v4` 恢复 `.codex-build/cache`、Cargo registry/git 和 Tauri target，缓存键包含 `app/package-lock.json`、`app/build-versions.json` 与 `Cargo.lock`；这些缓存不是发布资产。
