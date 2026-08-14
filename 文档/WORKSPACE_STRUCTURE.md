# tauri-codex 工作空间结构与知识放置契约

Status: Active
Kind: StructureContract
Scope: tauri-codex / 目录与知识放置
Owner: 项目维护者
Updated: 2026-08-11
Depends On:
- ../AGENTS.md

本文是目录职责、文档 Kind 和知识放置的唯一当前契约。它只声明已有事实的位置，不从产品文档推断源码根、框架、服务或运行配置。

## 工作空间根目录

| 路径 | 当前职责 | 不放置 |
|---|---|---|
| `.agents/skills/` | Codex 自动发现的仓库级协作方法 | 单次任务过程和产品事实 |
| `.github/` | GitHub CI、发布自动化与公开协作模板 | 产品事实、密钥和构建生成物 |
| `LICENSES/` | 随项目分发的第三方标准许可证原文 | 项目自有代码许可证、商业合同和产品事实 |
| `文档/` | 任务、治理、产品契约、当前设计、决策、运行与活动推进 | 源码镜像、聊天过程、日志和生成物 |
| `app/` | 已验证的 Tauri 桌面应用源码、构建配置和定向测试 | 产品契约副本、真实密钥、运行日志和发布生成物 |
| `人类-文档/` | 开发者主动使用的任务导航 | 正式契约、智能体路由、源码事实副本和秘密 |

当实际项目事实已验证后，在本契约同一改动中加入其 `文档/项目/项目_<id>/` 的精确位置；源码根只在实际创建并验证后写入项目入口。只创建已有唯一事实的类别目录。

## 受控文档位置

- `TASK_CONTROL.md` -> `TaskControl`
- `WORKSPACE_STRUCTURE.md` -> `StructureContract`
- `工作流/` -> `WorkflowContract,Workflow`
- `工作空间/归档/` -> `Archive`
- `项目/项目_tauri-codex/AGENTS.md` -> `AgentEntry`
- `项目/项目_tauri-codex/产品契约.md` -> `ProductContract`
- `项目/项目_tauri-codex/当前设计.md` -> `CurrentDesign`
- `项目/项目_tauri-codex/决策/` -> `Decision`
- `项目/项目_tauri-codex/运行/` -> `Runbook`
- `项目/项目_tauri-codex/推进中/` -> `ChangePlan,SystemTestPlan,DeploymentPlan,Issue`

仅承载 `ChangePlan`、`SystemTestPlan`、`DeploymentPlan`、`Issue` 的活动目录随活动文档共同创建和删除；没有活动任务时目录可以不存在。其他已声明正式文件和目录必须持续存在。

首次出现有可靠依据的未承诺结果时，在本文件同一改动中声明 `WORK_CANDIDATES.md -> WorkInventory`；首次出现实际工作空间级设计、决策、运行或活动事实时，也在同一改动中声明其精确位置与 Kind。

## 文档状态协议

- `TaskControl`: `Active`
- `WorkInventory`: `Active`
- `StructureContract`: `Active`
- `WorkflowContract`: `Active`
- `Workflow`: `Active`
- `AgentEntry`: `Active`
- `ProductContract`: `Active`
- `CurrentDesign`: `Active`
- `Decision`: `Proposed,Accepted,Rejected,Superseded`
- `Runbook`: `Active`
- `ChangePlan`: `Draft,Active`
- `SystemTestPlan`: `Draft,Active`
- `DeploymentPlan`: `Draft,Active`
- `Issue`: `Active`
- `Material`: `Active`
- `Archive`: `Active`

正式文档禁止用 `Done` 或 `Completed` 保存任务快照。

## 生命周期

| Kind | 产生与消费 | 退出 |
|---|---|---|
| `TaskControl` | 初始化一次；可恢复的已授权活动任务在此登记 | 完成、取消或不再需要恢复时移除该项 |
| `WorkInventory` | 仅保存有可靠依据的未承诺独立结果 | 结果完成、放弃、失去依据或提升为用户任务时删除该项 |
| `AgentEntry` | 已验证项目需要独立路由时创建 | 项目完全退出后删除 |
| `ProductContract`、`CurrentDesign`、`Decision`、`Runbook` | 仅在长期事实缺少唯一所有者时创建 | 事实退出或迁入明确替代所有者后删除；Decision 仅更新状态 |
| `ChangePlan`、`SystemTestPlan`、`DeploymentPlan`、`Issue` | 仅在相应受控任务确实需要跨会话恢复时创建 | 该任务结论形成并归位长期事实后删除 |
| `Material` | 当前正式文档需要不可低成本重建的原始证据时创建 | 无当前消费方时删除 |
| `Archive` | 已登记任务完成且结果影响后续选择时追加最小事实 | 仅在完整迁移到新卷时封存 |

## 控制文件职责

- 根 `AGENTS.md`：最短启动、硬门禁与方法路由。
- 根 `README.md` 与 `人类-文档/README.md`：人类入口，不进入智能体默认事实热路径。
- `TASK_CONTROL.md`：唯一可恢复活动工作源。
- 本文：唯一目录、Kind 与知识放置契约。
- `工作流/WORKFLOW_CONTRACT.md`：任务分类、治理 Action 与 Workflow 选择。
- 项目 `AGENTS.md`：项目事实与源码根的最短路由。
- `check-docs.mjs`：依据本文验证控制面，不维护第二套目录分类。

## 知识保存门槛

长期文档必须有唯一所有者、更新触发和超出当前任务的价值，且不能从源码、类型、迁移、测试或可靠正式来源低成本重建。类、DTO、完整路由、字段清单、测试输出、日志、聊天和已修复问题默认不长期保存。外部材料只能参考，不能自动升级为项目规范。

## 发现与门禁

正常路径是 `AGENTS.md -> TASK_CONTROL.md -> 项目 AGENTS.md -> 一个命中文档或 skill -> 源码与测试`。新建或重命名受控目录前先更新本文；移动文档时修正依赖、链接和任务入口。`文档/` 下禁止 `README.md`，未声明位置、Kind 不匹配、状态非法或链接失效时 `npm run check:docs` 必须失败。
