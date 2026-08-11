# tauri-codex 安全政策

安全修复面向 `main` 和最新公开版本；旧版本不保证继续接收修复。项目尚无公开 Release 时，以 `main` 的源码为唯一维护基线。

## 私下报告漏洞

优先使用仓库 Security 页的 Private vulnerability reporting 提交报告：

https://github.com/chengzhang0528/tauri-codex/security/advisories/new

如果该入口不可用，请创建一个不包含漏洞细节的普通 Issue，只说明需要与维护者建立私下联系。不要在公开 Issue、讨论、日志或截图中披露利用方法、API Key、Token、连接串、个人信息或客户数据。

报告应尽量包含：

- 受影响的应用版本、提交和 Windows 版本。
- 影响、攻击前置条件和可重复的最小步骤。
- 已确认的缓解方式，以及是否已公开披露给其他项目。

项目不会承诺固定响应时限。OpenAI Codex、Node.js、Tauri 或其他第三方组件的漏洞也应报告给对应上游；若 tauri-codex 的组合或分发方式受到影响，可以同时通知本项目。

