# Windows构建与发布根脚本

Status: Active
Kind: Runbook
Scope: tauri-codex / Windows x64 构建、NSIS 制品与 GitHub Actions 触发
Owner: 项目维护者
Updated: 2026-08-13
Depends On:
- ../../../../package.json
- ../../../../app/build-versions.json
- ../../../../.github/workflows/ci.yml
- ../../../../.github/workflows/windows-release.yml

所有根脚本从工作空间根目录执行。固定组件版本由 `app/build-versions.json` 唯一管理，Manager 版本由 `app/package.json` 管理，稳定 Installer 版本与首次发布 tag 由 `app/installer-versions.json` 管理；本地候选制品和校验清单只写入 Git 忽略的 `.codex-build/`。Installer 是薄安装器，运行时组件同时作为 GitHub Release 资产和固定 OSS object key 发布。

网络资源缓存也固定在 `.codex-build/cache/`：Node.js MSI 按 SHA-256 保存于 `node/<sha256>/`，Codex 安装使用同一目录下的 `npm/` 缓存，Rust 依赖沿用 Cargo 的用户缓存。资源只有在摘要校验通过后才会从 `.partial` 原子改名为正式缓存文件；损坏缓存会被丢弃并重新下载，`.partial` 文件永远不会被复用。Node/Codex 所需条目命中缓存时不重复下载。需要代理时由执行环境提供代理变量（例如 Windows `curl.exe` 支持的 `ALL_PROXY`），不会写入应用配置。

| 脚本 | 职责 | 副作用 |
|---|---|---|
| `bootstrap` | 检查 Rust GNU、Rust target、MSYS2 UCRT64，执行 `npm ci` 并准备固定 Codex/Node 资源；优先复用 `.codex-build/cache/` | 修改 `app/node_modules`、缓存与被忽略的资源目录 |
| `build` | 编译 Windows x64 release 应用，不生成安装包 | 写入 `.codex-build/build/` 与 Tauri target |
| `installer:build` | 显式构建 Launcher 与 Manager；Installer 版本首次发布时生成 NSIS，普通 release 复用其 GitHub 公开资产；同时生成 Codex/Node 组件资产、manifest 和 Bootstrap | 写入 `.codex-build/releases/`、Bootstrap 与 Tauri target |
| `installer:verify` | 校验薄 Installer、manifest、组件文件大小和 SHA-256、Bootstrap 闭包，以及 Manager ZIP 中的 EXE 与 `WebView2Loader.dll` 完整闭包 | 只读 |
| `build:release` | 依次执行 bootstrap、installer build、installer verify | 只生成本机候选，不上传、不安装 |
| `verify:release` | 对已有候选重复执行 release 验证 | 只读 |
| `publish:release:oss -- preflight <version>` | 用一次性项目探针验证 OSS 凭据写入、匿名回读与精确清理；不读取或输出 Secret 值 | 需要 Actions `oss-release` 环境中的 OSS Secret；仅短暂改变并清理探针对象 |
| `publish:release:oss -- stage <version>` | 从冻结候选上传或复用 OSS 不可变 Installer、组件与 manifest，逐个匿名回读，不移动 Bootstrap | 需要 Actions `oss-release` 环境中的 OSS Secret；改变公开不可变 OSS 对象 |
| `publish:release:oss -- commit <version>` | 校验 GitHub 与 OSS 均公开提供同一候选后，最后提交并确认 `bootstrap/windows-x64.json` | 需要 Actions `oss-release` 环境中的 OSS Secret；改变公开 OSS Bootstrap |
| `release:patch` | 当前 `main` 已完成且需要准备下一个补丁版本 | 自动读取当前版本并同步应用版本文件；提交、tag 和发布分别由 Git 收口与 Deployment 执行 |
| `test:rust` / `test` | 运行 Rust 格式、类型检查与定向单测；`test` 另含前端和文档门禁 | 只写入被忽略构建缓存 |

兼容入口 `app:bundle` 等价于 `installer:build`；开发入口仍是 `app:dev`。完成一轮已授权代码变更后，`npm run release:patch` 自动递增 patch 并同步 `package.json`、lockfile 与 Cargo 版本，不修改独立 Installer 版本，也不要求用户预先计算版本号；Launcher 或 Installer 行为变化时另行递增 `app/installer-versions.json` 并让 `releaseTag` 指向该 Installer 首次发布 tag。版本改动随候选通过 Git 收口精确提交，Deployment 再为该提交创建 `vX.Y.Z` tag。GitHub Actions 的 `ci.yml` 在 Pull Request、`main` 推送和手工触发时先用根 `bootstrap` 准备锁定依赖与组件构建资源，再执行依赖审计和 `npm test`。`windows-release.yml` 在 `vX.Y.Z` tag 或手工触发时执行同一 bootstrap 与源码门禁，再执行 `build:release` 和 `verify:release`；tag 发布先由 `oss-preflight` 验证受保护环境 Secret 及项目 OSS 写读权限，候选构建后由 `oss-stage` 上传并匿名回读全部不可变对象，随后才公开 GitHub Release，最后由 `oss-commit` 复核两端同字节闭包并提交 OSS Bootstrap。任一前置阶段失败时 GitHub Release 不公开，任一后置阶段失败时旧 OSS Bootstrap 保持可用；重试复用相同候选与不可变对象。稳定 Installer 首次发布 tag 上传 NSIS，普通 tag 复用其 GitHub 与 OSS 不可变对象。

重试时直接重复同一个脚本即可。固定版本不匹配、工具链缺失、资源版本不一致或校验失败时脚本立即停止；网络恢复后，已验证缓存会避免重复下载，不覆盖已存在的不同候选。setup-msys2 的安装根由 Action 动态决定，CI 与 Release Workflow 都在其 `msys2 {0}` shell 中把 `/ucrt64/bin` 转为 Windows 路径并通过 `TAURI_MINGW_BIN` 交给根脚本，不依赖 runner 临时目录。GitHub Actions 通过 `actions/cache@v4` 恢复 `.codex-build/cache`、Cargo registry/git 和 Tauri target，缓存键包含 `app/package-lock.json`、`app/build-versions.json` 与 `Cargo.lock`；这些缓存不是发布资产。

Manager 组件归档必须同时包含非空的 `tauri-codex-manager.exe` 与同目录 `WebView2Loader.dll`，不得依赖 Installer 目录或调用者工作目录提供动态库。Launcher 在该目录运行有超时的 `--runtime-check` 后才允许 stage/激活，并以同一目录作为正式启动工作目录；缺文件、doctor 失败或超时均保留当前可用 release。
