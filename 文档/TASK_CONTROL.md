# 根任务总控

Status: Active
Kind: TaskControl
Scope: coder_driver-template / 可恢复活动任务
Owner: 项目维护者
Updated: 2026-08-09
Depends On:
- ../AGENTS.md

这里是当前已授权且需要跨会话恢复的活动工作唯一事实源，不是全部后续工作的清单。状态只允许 `InProgress`、`Review`、`Blocked`；未开始或未授权的结果不在此登记。

## 当前队列

| ID | 状态 | 执行类型 | 范围 | 候选/制品 | 下一可验证结果 | 入口 |
|---|---|---|---|---|---|---|
| ST-0002 | InProgress | SystemTest | tauri-codex / Windows x64 本机线上安装验收 | GitHubRelease:v0.1.6:e84987734574fc91fe83b4c7a101a05d5573f1b4709a4e1f20e0b992e06bf7eb | 就地安装并断言 Installer、Launcher、不可变 release 激活、Manager 和更新入口 | `文档/项目/项目_tauri-codex/推进中/ST-0002-v0.1.6-Windows安装验收.md` |

## 维护规则

- 只有已授权工作需要跨会话恢复、持续阻断恢复或外部部分状态恢复时才登记。
- 每个独立结果只保留一个活动项；同一结果的纠正更新原项。
- Development 使用 `-` 作为候选/制品。持久 SystemTest 与 Deployment 分别链接 SystemTestPlan 与 DeploymentPlan，并固定其候选或制品。
- Git 未提交、未推送或并行改动不维持活动项，也不反转任务结论。
