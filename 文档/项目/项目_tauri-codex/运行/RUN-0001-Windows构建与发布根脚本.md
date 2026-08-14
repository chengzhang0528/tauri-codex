# Windows构建与发布根脚本

Status: Active
Kind: Runbook
Scope: tauri-codex / Windows x64 构建、NSIS 候选与 OSS 发布
Owner: 项目维护者
Updated: 2026-08-14
Depends On:
- ../当前设计.md
- ../../../../package.json
- ../../../../app/build-versions.json
- ../../../../app/installer-versions.json
- ../../../../.github/workflows/ci.yml
- ../../../../.github/workflows/windows-release.yml

所有根脚本从工作空间根目录执行。Manager 版本由 `app/package.json` 管理，Codex/Node 固定输入由 `app/build-versions.json` 管理，Installer/Launcher 版本由 `app/installer-versions.json` 管理。候选、缓存、签名中间文件和校验结果只写入 Git 忽略的 `.codex-build/`。

## 交付约束

- 固定公开根是 `https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex/`。Installer、signed Bootstrap、signed manifest、Manager、Codex、Node fallback 和 checksum 只发布到该前缀。
- GitHub 只保存源码、tag、Release Notes 和 OSS 链接，不上传 binary asset。
- schema v2 不兼容 schema v1；本版本不构建 legacy Bootstrap 或旧 Launcher bridge。
- Manager、Codex 与 Node 由同一个 signed manifest 发布。Codex 不从 npm registry 做运行时更新。
- 构建候选时先完成 Authenticode/Ed25519 签名，再计算 size/SHA-256。生产构建缺少签名身份直接失败，私钥不得写入仓库或日志。
- Installer/Launcher 行为变化时才递增 Installer 版本；普通 release 复用 OSS 上已发布且可匿名回读的稳定 Installer。

## 脚本

| 脚本 | 职责 | 副作用 |
|---|---|---|
| `bootstrap` | 检查 Rust GNU/MSYS2，安装锁定依赖并准备固定 Codex/Node 构建输入 | 写入依赖目录与 `.codex-build/cache/` |
| `build` | 编译未发布的 Windows x64 Launcher 和 Manager | 写入 `.codex-build/build/` 与 Tauri target |
| `installer:build` | 从一个冻结 source/version 构建并签名 Launcher、Manager、Codex/Node components，对最终归档运行 Manager/Codex doctor，再生成 manifest、Bootstrap 和需要时的 NSIS | 写入 `.codex-build/releases/<version>/windows-x64/` |
| `installer:verify` | 验证候选 identity、signed envelope、object key、size/SHA-256、组件闭包、Manager ZIP、Authenticode 要求和冻结元数据 | 只读 |
| `build:release` | 依次执行 bootstrap、候选构建与候选验证 | 只生成本地候选，不上传、不安装 |
| `verify:release` | 对已有冻结候选重复验证，不重建同版本 | 只读 |
| `publish:release:oss -- preflight <version>` | 用项目探针验证 OSS 凭据写入与匿名回读，不输出 Secret | 短暂创建并精确删除 probe object |
| `publish:release:oss -- stage <version>` | 上传或复用全部不可变对象，并逐个匿名回读验证；不移动 Bootstrap | 改变 OSS 不可变对象 |
| `publish:release:oss -- snapshot <version>` | 保存当前 Bootstrap 原始 bytes、SHA-256 与 ETag，并绑定目标候选 | 写入候选目录内的忽略文件 |
| `publish:release:oss -- commit <version>` | 重新验证完整 closure 与快照后，条件提交唯一 mutable Bootstrap | 改变 OSS Bootstrap |
| `publish:release:oss -- confirm <version>` | 匿名回读不可变 closure 与 Bootstrap，确认与候选逐字节一致 | 只读 |
| `publish:release:oss -- rollback <version>` | 仅当当前 Bootstrap 仍等于候选时，条件恢复快照并回读 | 改变 OSS Bootstrap |
| `release:patch` | 计算下一 Manager patch；Installer 版本保持独立 | 修改版本源，仍不发布 |
| `test:rust` / `test` | 运行 Development 白盒门禁 | 只写入忽略的构建缓存 |

## 签名输入

生产候选要求执行环境提供以下非空配置，脚本只检查存在性和公私钥匹配，不输出值：

- `TAURI_CODEX_RELEASE_KEY_ID`
- `TAURI_CODEX_RELEASE_PRIVATE_KEY`：base64 PKCS#8 Ed25519 private key
- `TAURI_CODEX_RELEASE_PUBLIC_KEY`：base64 raw 32-byte Ed25519 public key，同时编译进 Launcher trust policy
- `TAURI_CODEX_AUTHENTICODE_THUMBPRINT`
- `TAURI_CODEX_AUTHENTICODE_TIMESTAMP_URL`

GitHub Actions 由 `TAURI_CODEX_AUTHENTICODE_PFX_BASE64` 与 `TAURI_CODEX_AUTHENTICODE_PFX_PASSWORD` 临时导入当前用户证书库并生成 thumbprint；PFX 只存在于 runner 临时文件，步骤结束即删除。Local build 仍可直接使用已安装证书的 thumbprint。

本地白盒测试可以生成进程内 ephemeral Ed25519 key 和 test certificate，只能落入 `.codex-build/`，不得形成正式候选或进入 OSS。

## 候选事务

1. 固定 source commit、Manager version、Installer version、Codex version、Node version、key ID 和 object keys。
2. 清理当前版本输出后只构建一次。签名 Launcher/Manager 与需验证的 Windows 组件并生成最终 Manager/Codex/Node 归档；解开最终归档，实际执行 Manager `--runtime-check` 和 Codex `--version`。
3. 最终 doctor 通过后才计算组件 identity 并生成 Ed25519 signed manifest；随后按需构建并签名 Installer，生成最终 signed Bootstrap，再写入 `candidate.json` 固定所有 bytes、size、SHA-256、key ID 和路径。
4. `installer:verify` 只消费 `candidate.json`，不重新生成候选。
5. `stage` 对每个 immutable object 使用禁止覆盖上传；已存在对象只能在匿名回读后证明同 bytes 才复用。
6. `snapshot` 保存提交前 Bootstrap 的精确 bytes、SHA-256 与 ETag；首次 v2 发布允许从经结构校验的 schema v1 Bootstrap 条件迁移。
7. 完整匿名回读 closure 后，`commit` 只在当前 Bootstrap 仍等于快照时写 `bootstrap/windows-x64.json`，并再次匿名读取同 bytes。
8. 独立公开安装验收通过后，`finalize` 才创建 tag 和 Release Notes；正文只含 OSS Installer/manifest 链接，不上传文件。
9. 提交后验收失败且 tag 尚未创建时，`rollback` 只在当前 Bootstrap 仍等于本候选时恢复快照；不可变对象保留为未引用对象，不覆盖或删除。

任一步失败都不得改写冻结候选或移动 Bootstrap；重试复用同一 `candidate.json`。同版本 bytes 不一致必须换版本，不能覆盖。

## 构建图门禁

- Vite 必须产出 `index.html` 与 `launcher.html`。
- Cargo 必须显式产出 `tauri-codex.exe` Launcher 和 `tauri-codex-manager.exe` Manager。
- Manager ZIP 必须包含非空 `tauri-codex-manager.exe` 与同目录 `WebView2Loader.dll`。
- 最终 Manager/Codex 归档必须在 manifest 和 `candidate.json` 冻结前通过 Manager `--runtime-check` 与 Codex `--version`；依赖准备阶段的缓存命中不代替该检查。
- NSIS 必须嵌入 signed Bootstrap seed、许可证与 Installer-owned 资源，不携带 Manager/Codex/Node payload。
- 稳定 Installer 复用时从 OSS immutable object 读取并验证，禁止从 GitHub 下载或依赖本地旧文件。
- 首次公开新构建图前必须至少完成一次仅保留依赖缓存的 clean-output build。

## GitHub Workflow

`windows-release.yml` 只接受受保护的 `workflow_dispatch`，并把同一冻结候选分成四种可恢复操作：`candidate` 只构建并保留 14 天 artifact；`publish` 通过 candidate run ID 下载该候选，执行 preflight、stage、snapshot 和 commit；`finalize` 在独立公开安装验收通过后再次确认 OSS identity，再创建 GitHub tag/Release Notes；`rollback` 通过 candidate run ID 与 publish run ID 条件恢复提交前快照。GitHub Release job 不上传 binary assets，正文只含固定 OSS 链接。

本 Runbook 不构成 Deployment 授权。向 OSS 写对象、移动 Bootstrap 或公开 Release Notes 必须由单独授权的 Deployment 执行。

## 故障处理

- 缺少生产 Ed25519 或 Authenticode identity：停止候选构建，不降级为 unsigned。
- OSS preflight、上传或匿名回读失败：保留不可变对象供同候选重试，不提交 Bootstrap。
- Bootstrap 提交后的公开安装验收失败：在 tag 创建前运行 `rollback`；若 Bootstrap 已被其他写入者改变，停止并人工判定，不做盲覆盖。
- candidate metadata 与本地 bytes 不一致：停止并废弃该版本，不重建覆盖。
- schema、platform、architecture、compatibility、签名、size、digest 或 object key 不一致：拒绝候选。
- 不得改用 GitHub、npm registry 或其他 binary origin 绕过 OSS 故障。
