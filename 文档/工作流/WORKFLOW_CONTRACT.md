# coder_driver 智能体治理工作流契约

Status: Active
Kind: WorkflowContract
Scope: coder_driver-template / 智能体治理
Owner: 项目维护者
Updated: 2026-08-08
Depends On:
- ../WORKSPACE_STRUCTURE.md

本文只定义模型如何根据用户明确目标为每个任务判定执行类型、控制强度、持久化需求，并选择工作流和治理 Action。三个判断轴彼此独立；本文不规定项目命令、环境参数或产品实现。

## 任务执行类型

项目和会话不存在全局“当前阶段”。用户明确目标和授权决定每个实际执行任务的类型；模型只能据此判定允许动作，不能根据仓库状态、工具名称、测试规模或上一任务结果替用户升级任务。每个任务只属于一个类型；同一请求明确包含多个类型时拆成多个任务分别执行和收口。

| 任务类型 | 用户目标 | 允许动作 | 禁止动作 | 完成输出 |
|---|---|---|---|---|
| `Development` | 交付功能、修复 Bug、实现新需求、设计、重构、治理或修改测试代码 | 修改工作空间；执行证明本次改动所需的静态检查、构建以及定向单元、组件、契约、局部集成或必要真实依赖验证 | 自行追加用户未要求的独立系统/全量测试、正式制品发布、目标环境迁移或部署 | 实现结果、范围匹配的白盒证据，以及可选候选版本 |
| `SystemTest` | 用户明确要求把已有工作空间或候选的独立集成测试、系统测试、全量测试、候选验收或回归作为本任务主要结果 | 对明确的被测内容执行授权范围内的真实服务、浏览器、真实库、多角色、端到端或大范围回归并记录证据 | 修改产品源码、顺手修复缺陷、发布正式制品或改变部署目标 | 对该被测内容的通过、失败或受阻结论与缺陷清单 |
| `Deployment` | 用户明确要求把指定制品发布、迁移、上线、切换或回滚到命名目标 | 按 Runbook 执行已授权目标操作和部署后最小检查 | 修改产品源码、扩大功能范围、替换制品、补做系统级回归 | 指定制品在指定目标的部署或回滚结果 |

用户任务的主要交付结果是执行类型的唯一判定依据；范围、候选、CI 配置、发布门禁和目标环境只用于约束已获用户授权的任务，不能反向创造用户未提出的测试或部署任务。功能实现或 Bug 修复及其验收测试是一个 Development 任务，即使用户要求在实现后运行集成、浏览器、真实库或全量测试；只有用户要求对已有实现或候选独立形成测试结论时才建立 SystemTest。Git commit/push、本地构建和本地受管服务重启均不等于 Deployment。

不同任务之间只能显式交接，不存在项目或会话阶段转换：

- `Development → SystemTest`：只有用户提出新的独立测试目标时才进入 SystemTest；再分别判定控制强度和持久化需求，Development 完成本身不触发转换。
- `SystemTest → Development`：发现产品缺陷、测试实现缺陷或新需求时记录测试任务结论并停止修改被测内容；只有用户授权修复时才创建或关联独立 Development 任务，修复完成也不自动恢复测试。
- `SystemTest → Deployment`：只有用户明确要求部署时才新建独立 Deployment 任务，并绑定确定制品、目标、显式授权和回滚入口；测试通过本身不触发部署。
- `Deployment → Development/SystemTest`：产品缺陷需要用户授权独立 Development；候选或环境证据不足阻断当前 Deployment，只有用户另行要求测试时才建立 SystemTest。部署任务不得现场改代码或自动创建其他任务。

## 选择规则

只读问答、解释和一次性查看没有执行类型，不加载工作流。改变项目制品或项目交付环境但不改变本工作空间的操作，仍须根据用户该任务的明确目标选择执行类型和专业 skill；与项目交付无关的外部工具操作继续遵循其自身授权模型。

| 用户任务目标 | 执行类型 | 主工作流 | 后续处理 |
|---|---|---|---|
| 修改正式文档、任务控制或治理配置，但不进入应用编码 | `Development` | [WF-0001 日常变更](WF-0001-日常变更.md) | 完成或取消前进入 WF-0004 |
| 功能开发、Bug 修复，或修改源码、数据、API、构建、测试及技术工作流 | `Development` | [WF-0002 日常开发](WF-0002-技术变更.md) | 持续阻断进入 WF-0003；完成或取消前进入 WF-0004 |
| 用户要求对已有工作空间或确定候选独立执行集成、系统、全量、发布门禁或大范围验证 | `SystemTest` | [WF-0005 系统测试](WF-0005-系统测试.md) | 常规或受控执行，并独立判定是否持久化；缺陷经用户授权转独立 WF-0002 |
| 用户要求发布制品、推送正式包、执行目标环境迁移、上线、切换或回滚 | `Deployment` | [WF-0006 部署](WF-0006-部署.md) | 缺陷经用户授权转独立 WF-0002；证据不足阻断；完成或取消前进入 WF-0004 |
| 用户要求调查、诊断、评审，或现有工作出现跨会话阻断 | 纯只读无类型；受阻时保留来源任务类型 | [WF-0003 调查与阻断](WF-0003-调查与阻断.md) | 获得实施授权后按新任务目标重新判定 |
| 任一会改变工作空间或外部环境的任务准备报告完成、取消或继续受阻 | 保留该任务自己的执行类型 | [WF-0004 任务收口](WF-0004-任务收口.md) | 输出完成、取消或未完成三种结果之一 |

每个任务只选择一个主工作流；WF-0004 是共同收口，不与主工作流竞争。一个任务不能同时属于多个执行类型。实现或修复及其验收测试保持一个 Development 任务；只有同一请求包含可独立交付且分别授权的测试或部署结果时才拆成多个任务。不能仅因权限边界更严格、工具可用、CI 配置、发布门禁或希望提高信心就替用户新增 SystemTest 或 Deployment；用户目标确实不明确且会改变外部状态时停止确认。

## 控制强度

执行类型与控制强度是独立维度。Development 和 SystemTest 都可常规或受控；Deployment 始终受控。

- **常规 Development** 必须同时满足：授权与验收清楚；影响文件和消费者可可靠枚举；不改变公共 API/DTO、数据库或迁移、权限/安全/租户边界、跨项目契约、部署方式、业务能力边界、不变量、持久化数据/状态模型或对外失败/兼容策略。局部前端样式、布局、文案、可访问性，无行为变化的内部整理，以及明确孤立缺陷及其定向测试通常属于此类。
- **受控 Development** 是任一常规条件不成立的开发，或用户明确要求方案、迁移、跨模块/跨项目实施。它必须先明确 A→B、约束、可观察成功标准和停止条件；是否写入任务与 ChangePlan 由持久化需求决定。
- **常规 SystemTest** 必须由用户明确要求独立测试，且对象、环境、范围和断言均清楚，可在隔离本地环境内完成，不涉及共享环境、发布候选准入或持续测试活动。执行前确认被测工作空间与环境身份。
- **受控 SystemTest** 是任一常规条件不成立的系统测试，包括不可变候选验收、共享或命名测试环境、用户要求满足的 CI/发布门禁或持续测试活动。它必须记录用户请求来源并固定 Candidate 与 Environment；缺陷修复不在该测试任务中实施。是否写入任务与 SystemTestPlan 由持久化需求决定。
- **受控 Deployment** 需要任务入口和 DeploymentPlan；计划创建前就必须有确定 Artifact、Target 和当前用户授权，为 `Active` 前还必须绑定 `Admission Evidence` 与 Rollback。部署不得从 Development 或 SystemTest 自动串行继续；缺少准入证据只阻断该任务。
- **治理控制面强制受控**：改变根、项目或源码入口 `AGENTS.md` 的治理语义，或者修改 WorkflowContract、Workflow、StructureContract、治理 checker，以及负责任务登记、阶段、方案、安全、验证或收口门禁的 skill，均为受控 Development，但同回合可完成时不因受控而自动持久化。
- **正式事实定向发现**：常规 Development 在判定“无需长期事实”前，由项目入口为每种相关 Kind 确定至多一个候选正式所有者并定向搜索；发生冲突时升级为受控 Development。

## 持久化需求

持久化需求独立于执行类型和控制强度，只回答是否必须保存可恢复现场。

- **临时执行**：当前任务可以在同一回合完成或明确结束，没有需要后续恢复的阻断、共享环境修改、部分发布或回滚状态。它不登记 `TASK_CONTROL.md`，不创建活动计划；受控任务仍须在当前任务工作计划中执行完整门禁。
- **持久执行**：已授权工作需要跨会话恢复、存在持续阻断，或已经产生必须恢复的外部部分状态。它登记唯一活动任务；受控 Development、SystemTest、Deployment 分别绑定 ChangePlan、SystemTestPlan、DeploymentPlan，调查阻断按需绑定 Issue。
- Deployment 在改变目标前始终属于持久执行。共享可变测试环境需要恢复时属于持久执行；只读不可变候选验证可按实际恢复需求判定。
- `WORK_CANDIDATES.md` 保存已知、独立、有依据但未获当前用户授权的结果。候选不是任务，不携带执行类型、控制强度、环境或授权，不能触发工作流；用户明确发起后才在同一变更中移除候选并完成三个轴的判断。
- 默认“后续还有什么”只回答活动任务与候选清单中的已知结果，不宣称完整。只有在先声明项目、能力或代码范围并审计其长期事实源和代码证据后，才可给出该范围的完整性结论。

## 机器可校验策略

本表是任务执行语义的稳定机器接口；说明文字可以改写，策略值变更必须同步评审校验器及正反例。

| Policy | Value |
|---|---|
| `TASK_SCOPE` | `per-task` |
| `TASK_AUTHORITY` | `explicit-user-goal` |
| `DEVELOPMENT_ATTACHED_TESTS` | `remain-development` |
| `SYSTEM_TEST_CONTROL` | `immediate-or-controlled` |
| `SYSTEM_TEST_AUTOCREATE` | `forbidden` |
| `DEPLOYMENT_CONTROL` | `controlled-only` |
| `DEPLOYMENT_MISSING_EVIDENCE` | `block` |
| `REPORTING_SCOPE` | `established-tasks-only` |
| `PERSISTENCE_CONTROL` | `independent` |
| `TASK_CONTROL_SCOPE` | `recoverable-active-work` |
| `WORK_CANDIDATE_SCOPE` | `known-uncommitted-outcomes` |
| `WORK_CANDIDATE_PROMOTION` | `explicit-user-task` |
| `WORK_CANDIDATE_EXECUTION_TYPE` | `classify-on-promotion` |
| `WORK_QUERY_COMPLETENESS` | `known-unless-audited` |

## Action 契约

Action 是治理动作，不是命令。工作流只能引用下表已声明 Action；具体执行方法归专业 skill、源码、测试或 Runbook。

| Action | Trigger | Input | Output | Key Constraints / Stop When |
|---|---|---|---|---|
| `ACT-CLASSIFY-REQUEST` | 接收或恢复请求 | 用户目标、当前任务、工作区与外部状态 | 请求类型和授权边界 | 权限、范围或任务类型不明时停止确认 |
| `ACT-CLASSIFY-EXECUTION-TYPE` | 请求可能进入项目交付活动 | 用户明确目标、目标环境和候选/制品 | 本任务唯一执行类型与允许/禁止动作 | 不得声明项目/会话当前阶段，不得根据验证形式替用户新增 SystemTest 或 Deployment |
| `ACT-REGISTER-TASK` | 已授权工作需要跨会话恢复、持续阻断恢复或外部部分状态恢复 | 目标、执行类型、范围、验收、候选/制品和长期入口 | `TASK_CONTROL.md` 中唯一当前项 | 控制强度本身不触发登记；候选不得自动提升；不得混合多个执行类型 |
| `ACT-ROUTE-CONTEXT` | 主工作流已确定 | 当前任务、项目入口、触发表 | 最少必要事实源、Runbook 与 skill | 不默认遍历文档树，不用归档推断当前事实 |
| `ACT-OPEN-PLAN` | 持久受控 Development 准备编码 | 基线、授权范围、成功标准 | 唯一 Active ChangePlan | 临时受控任务使用当前任务工作计划；持久方案只绑定 WF-0002，Draft 不授权编码 |
| `ACT-OPEN-SYSTEM-TEST-PLAN` | 持久受控 SystemTest 准备执行 | 用户请求来源、候选版本、测试环境、范围、进入/退出条件 | 唯一 SystemTestPlan | 临时受控测试使用当前任务工作计划；Candidate 或 Environment 未固定时不得创建持久任务或方案 |
| `ACT-OPEN-DEPLOYMENT-PLAN` | Deployment 准备执行 | 制品、目标、当前用户授权、准入证据、回滚入口 | 唯一 DeploymentPlan | Artifact、Target 或 Authorization 不确定时不得创建；Rollback 或准入缺失时不得执行部署 |
| `ACT-VERIFY-BASELINE` | 实施或判断前 | 源码、类型、测试、正式来源、候选/制品与环境 | 可引用的当前态、差异和不确定项 | 来源冲突或关键事实缺失时停止扩大范围 |
| `ACT-EXECUTE-CHANGE` | Development 范围和前置条件满足 | 授权任务、ChangePlan 或文档目标 | 任务要求的工作空间增量 | 只在 Development 边界内执行，不运行系统测试或部署 |
| `ACT-RUN-SYSTEM-TEST` | 临时测试已在当前任务固定边界，或 Active SystemTestPlan 已满足进入条件 | 用户明确测试目标、被测内容、环境、场景和断言 | 测试结论、证据与缺陷 | 不修改产品源码；缺陷只记录，修复需用户授权独立 Development |
| `ACT-AUTHORIZE-DEPLOYMENT` | DeploymentPlan 申请执行 | 通过证据、确定制品、目标、用户授权和回滚入口 | 可执行或拒绝的部署判定 | 不从测试通过、Git push 或制品存在推断授权 |
| `ACT-EXECUTE-DEPLOYMENT` | 部署已授权 | Active DeploymentPlan 与命中 Runbook | 部署、回滚和最小部署后检查结果 | 不替换制品、不现场改代码、不扩大为系统测试 |
| `ACT-CAPTURE-BLOCKER` | 问题跨会话且影响当前任务 | 证据、任务类型、影响、责任边界、下一入口 | 总控状态及必要 Issue | 临时错误和排查日志不沉淀；不得伪造外部承诺 |
| `ACT-VERIFY-RESULT` | 本任务产生结果 | 用户要求、任务完成规则、白盒或实测证据 | 本任务结论及证据状态 | Development 由实现和白盒决定；SystemTest 由明确被测内容的断言决定；Deployment 由目标操作和回滚状态决定 |
| `ACT-CLASSIFY-KNOWLEDGE` | 收口前 | 已验证事实增量、现有唯一所有者、独立未承诺结果 | 正式事实更新、候选更新或“无长期增量”判定 | 候选必须有可靠 Basis 与 Owner；不因留痕创建占位文档 |
| `ACT-RECONCILE-KNOWLEDGE` | 存在长期事实或候选增量 | 分类结果、正式所有者或候选依据 | 唯一事实源与候选清单一致 | 候选不授权执行；找不到所有者、依据或事实冲突时停止收口 |
| `ACT-RESOLVE-ACTIVITY` | 完成、取消或继续受阻 | 任务类型、功能结论、任务和活动计划 | 活动入口保留或删除后的正确状态 | 只处理实际持久化的活动；Git 状态或候选结果不得维持活动入口 |
| `ACT-ARCHIVE-RESULT` | 已登记任务完成且结果影响后续选择 | 任务 ID、执行类型、结果、长期入口、最小证据 | 一行完成事实 | 不归档命令流水或代码可重建结果 |
| `ACT-VALIDATE-DOCS` | 正式文档或入口变化 | 当前文档树和结构契约 | 文档治理检查结果 | 失败不得收口；不替代语义判断 |
| `ACT-REPORT-RESULT` | 当前回合结束 | 本次任务类型、变更、证据、任务、文档与 Git 状态 | 可核验汇报 | 只报告实际要求或建立的任务类型；多个任务逐项报告，不声明全局当前阶段 |

## 公共输出要求

每次工作流结束必须报告“本次任务类型”和该任务结论，不得报告项目或会话的“当前阶段”。同一请求明确建立多个任务时逐项报告；未被用户要求、未建立的 SystemTest 或 Deployment 不作为“未执行阶段”列出。Development 的通过只表示实现与范围匹配白盒完成；不得把可能的后续测试或部署写成已授权、已建立或已完成。

## 变更门禁

- 新增 Action 前先证明现有 Action 无法表达稳定输入输出；不得为单个命令或工具调用创建 Action。
- 任务执行类型是封闭集合；新增类型前必须证明它具有不同的授权边界、输入、完成输出和失败回路，不能用项目状态、会话状态、人员、日期或单次工具调用冒充任务类型。
- Workflow 只回答应该执行哪些治理动作；专业做法归 skill，任务特有顺序归对应活动计划，实际行为归源码、测试和 Runbook。
- Action 表是条件契约；没有命中 Trigger 的 Action 必须跳过。
