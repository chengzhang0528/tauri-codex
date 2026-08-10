---
name: evolve-document-driven-workflow
description: Use when the user asks to form a 方法轮, improve the document-driven coding workflow from conversation history, turn repeated corrections into a skill, or gives a durable future-facing agent/process instruction such as 以后必须、一律、统一、不要再. On the first request, produce a grounded proposal, have that exact proposal challenged, recommend the next user action, and stop without workspace changes; implement only on a later explicit user request. Keep one-off product requirements local and never store chat narrative.
---

# Evolve Document-Driven Workflow

## Goal

Turn the user's durable corrections into the smallest verified improvement to how this workspace is operated. Keep the workflow adapted to this user's established decisions as they evolve over time, while preserving current product and code ownership.

Use this as a recurring method loop, not a one-time redesign:

`observe -> identify wrong assumption -> choose generality -> reconcile owners -> verify -> learn from the next task`

Do not solve the concrete product problem in this skill. First use `../align-solution-direction/SKILL.md` to preserve the latest intent. On the first governance-correction request, use `../technical-solution/SKILL.md` to form the proposal and `../challenge-solution/SKILL.md` to review that exact artifact, then recommend the next user action and end the task. Use `../document-governance/SKILL.md` only after a later user request explicitly authorizes implementation.

## Trigger Gate

Run the loop when either condition is true:

- the user explicitly asks to evolve, review, optimize, or create a skill/method from the document-driven workflow or prior conversation;
- the user gives a future-facing rule about agent or process behavior using durable scope such as “以后”, “必须”, “一律”, “统一”, “不要再”, or “沉淀成 skill”.

Do not trigger merely because a product requirement says a field, API, page, test, or deployment “must” behave a certain way. Keep one-off acceptance and local implementation constraints in the current task and their actual product/code owner. If durable scope is genuinely ambiguous and would broaden authority, stop for that decision; otherwise choose the narrowest local scope.

The request that first introduces or materially revises a governance correction is review-only, even when it uses imperative wording. A later message that explicitly asks to execute the already presented and challenged proposal is a new authorized Development task. Do not represent the gap between those requests as a workflow phase, pending task, approval state, ChangePlan, or other persisted lifecycle.

## Load

1. Read the latest user instruction and only the earlier turns needed to recover its correction chain: rejected assumptions, repeated failures, accepted revisions, and current desired behavior.
2. Read root `AGENTS.md`, `文档/TASK_CONTROL.md`, the actual governance owners under review, and only the relevant skills/checker tests. Read `文档/工作流/WORKFLOW_CONTRACT.md` when it is itself a review target or when the later implementation request changes workspace governance; a read-only proposal does not select or enter a Workflow.
3. Read `WORK_CANDIDATES.md` under `文档/` only when the correction concerns known future work, promotion, or completeness answers.
4. Treat conversation as authority for desired agent behavior, not as product/code fact. Do not use Archive as a current rule source or scan unrelated documents.

## Run the Method Loop

### 1. Build an in-memory correction contract

Capture the latest instruction, concrete examples, explicitly rejected behavior, expected future behavior, and apparent scope. Latest explicit corrections supersede incompatible earlier preferences. Do not persist this ledger or create a user-profile document.

### 2. Find the generating assumption

Explain what default caused the repeated failure. Classify it as one of: direction/scope, execution type, control strength, persistence, knowledge ownership, validation/completion, interaction/output, or repository closeout. Fix the cause rather than adding an exception for the latest example.

### 3. Choose the narrowest supported generality

Evaluate `case -> work class -> project -> workspace`. Select the lowest level that covers all accepted examples. An explicit future-facing user rule can establish broader scope; repetition without such scope is evidence to investigate, not automatic authority to globalize.

### 4. Inspect all three governance surfaces

Always evaluate each surface, but edit only actual owners:

- **Method surface**: relevant skills and their `agents/openai.yaml` routing.
- **Activity surface**: `TASK_CONTROL.md`, `WORK_CANDIDATES.md`, activity-plan lifecycle, completion and Git separation.
- **Workflow surface**: root/project AGENTS, WorkflowContract/WF, StructureContract, CurrentDesign and machine checker/tests.

Record “no change” when a surface already enforces the corrected rule. Never create a task, candidate, plan, or document merely to prove the review happened.

### 5. Map the rule to one owner

- Agent entry or mandatory router: AGENTS.
- Reusable specialist method: skill.
- Authorization, task classification, lifecycle or Action semantics: WorkflowContract/WF.
- Document Kind and placement: StructureContract.
- Current control-plane mechanics: the sole StructureContract, WorkflowContract, checker implementation, and focused checker tests.
- Recoverable authorized work: TASK_CONTROL; evidence-backed uncommitted outcome: WORK_CANDIDATES.
- Product behavior or implementation: its ProductContract, CurrentDesign, source and tests, outside this method.

### 6. Produce a reviewed proposal and stop

For the first request, use `technical-solution` to state the old assumption, proposed invariant, boundary, actual owners, positive and negative cases, expected impact, and proportionate verification. Pass that fixed proposal to `challenge-solution`. Present the proposal and its review without silently rewriting either one, recommend whether the user should request revision, implementation, or no further action, and end the current task.

Do not edit files, register a task, create an activity plan, update candidates, or write Git state during this proposal task. A review verdict never authorizes implementation.

### 7. Implement only on a later explicit request

When the user later asks to execute the presented proposal, verify that the requested scope still matches the reviewed artifact. If implementation requires a material change to the outcome, boundary, owners, or completion rule, produce and challenge a revised proposal and stop again. Otherwise use `document-governance` and replace the conflicting rule at its owner; update only direct routers, dependent skills and necessary machine guards.

Do not preserve both old and new rules as caveats, replicate risk matrices, or rewrite unrelated governance.

### 8. Verify behavior, not wording

Add a positive case that must pass and a nearby negative case that must fail or remain local. Run the modified skill's `quick_validate.py`, focused checker tests, `npm run check:docs`, targeted conflict scans, and scoped whitespace checks. Do not run product SystemTest or Deployment unless separately requested.

### 9. Close and keep learning

For an implementation task, use WF-0004. A temporary controlled evolution has no TASK_CONTROL row or ChangePlan. Persist only the resulting invariant at its owner; never save proposals, challenge reports, chat transcripts, correction ledgers, generic preference profiles, or review reports. On a later contradiction, re-enter this method and update the same rule rather than adding a parallel mechanism.

## Completion Rule

A proposal task is complete when one grounded proposal and its read-only challenge have been returned with one recommended next user action; no workspace or lifecycle state remains open. A later implementation task is complete only when the accepted instruction is enforced at the right owner, all three surfaces were evaluated, stale conflicting behavior was removed, positive and negative evidence pass, and task/document lifecycle is clean. Repository commit or push remains a separate status.
