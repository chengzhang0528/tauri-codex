# tauri-codex app

这里是 Windows x64 桌面应用源码根。Tauri/Rust 位于 `src-tauri/`，控制窗口和终端窗口前端位于 `src/`。

完整产品边界见[产品契约](../文档/项目/项目_tauri-codex/产品契约.md)，已确认的封装方向见[DEC-0001](../文档/项目/项目_tauri-codex/决策/DEC-0001-Windows-x64-Codex桌面封装方案.md)。
固定构建版本见 `build-versions.json`；根目录的 `bootstrap`、`build`、`installer:build` 和 `installer:verify` 是稳定交付入口。

常用命令：

```powershell
npm run build
rustup run stable-x86_64-pc-windows-gnu cargo check --manifest-path src-tauri/Cargo.toml
rustup run stable-x86_64-pc-windows-gnu cargo test --release --manifest-path src-tauri/Cargo.toml --lib --target x86_64-pc-windows-gnu
npm run tauri:windows -- dev
npm run tauri:windows -- build
```

`tauri:windows` 会自动定位 Rustup、`stable-x86_64-pc-windows-gnu` 和 MSYS2 UCRT64；也可以用 `TAURI_MINGW_BIN` 指定 `ucrt64\\bin`。

开发与候选安装包构建步骤见[构建Windows桌面应用](../人类-文档/开发/构建Windows桌面应用.md)。
