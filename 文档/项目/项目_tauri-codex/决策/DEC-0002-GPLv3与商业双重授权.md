# GPLv3 与商业双重授权

Status: Accepted
Kind: Decision
Decision ID: DEC-0002
Scope: tauri-codex / 项目自有代码授权与贡献许可
Owner: 项目维护者
Updated: 2026-08-11
Depends On:
- ../AGENTS.md

## 结论

tauri-codex 项目自有代码采用 `GPL-3.0-or-later` 与独立商业许可证双重授权。社区许可证允许个人、教育、研究和企业内部免费使用，也允许按 GPL 条件修改和再分发。

希望在不履行 GPL 源码公开义务的情况下进行闭源再分发、OEM、白标、预装或闭源产品集成的主体，必须与项目权利人另行签订商业许可证。企业仅在内部运行未再分发的应用不需要商业许可证。

## 责任边界

- 根 `LICENSE` 是社区授权唯一法律文本；README 只提供摘要，不增加或减少 GPL 权利。
- `COMMERCIAL-LICENSE.md` 只说明商业授权入口和适用场景，不自行授予商业权利；具体权利以双方另行签署的协议为准。
- 项目名称、标识和发布者身份不因 GPL 自动获得商标授权；不得把修改版表述为 OpenAI 或原项目维护者发布的官方版本。
- OpenAI Codex、Node.js、Tauri、xterm.js、Lucide 和其他第三方组件继续适用各自许可证；项目的商业许可证不能替代或限制第三方授权。

## 贡献许可

外部贡献者保留其贡献的著作权，同时通过 `CLA.md` 授予项目维护者在 GPL 和商业许可证下使用、修改、再许可和分发该贡献所必需的权利。未明确接受 CLA 的贡献不得合并，以免破坏双重授权链。

## 公开入口

- 社区许可证：`LICENSE`
- 商业授权说明：`COMMERCIAL-LICENSE.md`
- 第三方声明：`THIRD_PARTY_NOTICES.md`
- 贡献流程与 CLA：`CONTRIBUTING.md`、`CLA.md`

价格、支持等级、定制交付和具体商业合同不属于本 Decision，由项目权利人与客户另行确定。
