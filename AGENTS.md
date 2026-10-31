# tauri-codex 工作空间智能体入口

Status: Active
Scope: tauri-codex
Owner: 项目维护者
Updated: 2026-10-31
Depends On:
- none

本工作空间承载 tauri-codex 产品事实与文档驱动协同控制面。当前项目入口为 `文档/项目/项目_tauri-codex/AGENTS.md`；尚未建立或验证源码根。

## 启动

1. 先读 `文档/TASK_CONTROL.md`；仅在用户询问后续或路线图且文件存在时读 `文档/` 下的 `WORK_CANDIDATES.md`。
2. 命中项目时读 `文档/项目/项目_<id>/AGENTS.md`；涉及代码再读目标源码根 `AGENTS.md`、类型和测试，并先用 `rg` 查现有实现。
3. 改变工作空间、制品或交付环境时，读 `文档/工作流/WORKFLOW_CONTRACT.md` 并选择唯一主 Workflow；收口读 WF-0004。
4. 创建、移动、删除或无法判断文档位置时，读 `文档/WORKSPACE_STRUCTURE.md`。

事实优先级：当前用户要求 > 源码、类型、迁移与测试 > 唯一 ProductContract、CurrentDesign、Decision、Runbook > Git 与 Archive。来源冲突时停止扩张并修复唯一事实所有者。

## 门禁

- Codex 自动发现 `.agents/skills`；任务命中 skill 时按其方法执行，根入口不复制 skill 触发表。
- 对用户与智能体自己的关键前提保持独立判断；按[方向对齐方法](.agents/skills/align-solution-direction/SKILL.md)核实事实与推理，直接说明有依据的错误、遗漏及其影响，不把目标、偏好或假设当作事实。
- 任务类型只由用户目标决定；不得从测试规模、CI、候选、环境或 Git 状态推断 SystemTest 或 Deployment。
- 调用代码或命令前查真实契约；公共接口、DTO、数据库、权限或跨项目变更先识别消费者，保持最小改动。
- Development 完成范围匹配的源码或类型检查及必要定向测试；独立系统测试和部署必须由用户明确建立。
- 仅需跨会话恢复的受控任务登记 `TASK_CONTROL.md` 并绑定对应活动计划；长期能力事实变化时更新唯一正式所有者。
- 服务命令、配置、迁移与恢复只由项目 Runbook 定义；正式文档或治理入口变化后运行 `npm run check:docs`。
- 不写入或提交密钥、连接串、Token、密码、私钥、客户数据、日志或生成物；不发送可选过程播报。

## 收口

最终按 [WF-0004](文档/工作流/WF-0004-任务收口.md) 分为 `通过项`、`不通过项` 两段。每个不通过项写明失败、受阻或未验证的结果、所属服务或项目、卡点与证据、对本次目标的影响和可执行的推进建议；无事项的段落写 `无`。报告本次任务类型、实际改动、受影响范围、证据及独立的 Git 状态，不声明项目或会话阶段。独立工作通过后使用 `.agents/skills/git-closeout/SKILL.md` 精确提交并推送，Git 状态不反转功能结论。
