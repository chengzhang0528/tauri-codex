# 为 tauri-codex 贡献代码

感谢你帮助改进 tauri-codex。提交前请确认改动范围清楚、不包含密钥、日志、客户数据或生成物。

## 开始之前

- Bug、兼容性问题和不支持的平台请先按 [SUPPORT.md](SUPPORT.md) 选择对应 Issue 表单。
- 开发环境和首次准备步骤见[构建Windows桌面应用](人类-文档/开发/构建Windows桌面应用.md)。
- 参与讨论和评审时遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

## 提交 Pull Request

1. 从 `main` 创建分支并完成单一目的的改动。
2. 在工作空间根目录运行 `npm run test`；修改依赖时再运行 `npm run audit:dependencies`。
3. 阅读 [CLA.md](CLA.md)，并在 Pull Request 描述中保留以下确认：

   `I have read and agree to the tauri-codex Contributor License Agreement.`

4. 说明改动内容、验证结果和已知限制。

未明确接受 CLA 的贡献不会合并。CLA 让贡献者继续保留著作权，同时保证项目可以维持 GPL 与商业双重授权。

第三方代码必须注明来源和许可证；不得提交与 GPLv3 分发不兼容，或无权再许可的代码、素材和二进制文件。
