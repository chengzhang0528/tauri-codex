---
name: technical-solution
description: Use after align-solution-direction when producing or revising a Development-phase reviewer-first technical solution, implementation/refactor/integration/migration plan, staged engineering scheme, DOC/TASK handoff, or other plan meant to drive code. Consumes the stable direction contract and translates only impacted development dimensions into grounded changes and white-box completion; SystemTest and Deployment remain separate tasks.
---

# Technical Solution

## Goal

First use `../align-solution-direction/SKILL.md`. Carry its direction contract: outcome, boundary, this task's execution type, invariants, ownership, completion rule, and material open decisions. Do not reopen or reinterpret those decisions here. Return to alignment only when verified evidence makes the direction impossible or exposes a genuinely material choice. If alignment stops, do not create a coding-ready solution.

Start with a compact review decision sheet that maps each approved outcome to current support, required change, owner, evidence, and any material open decision. Add UI, API, data, operations, security, migration, or other impact only when that dimension is actually affected. Do not enumerate empty layers or turn a review plan into a file-by-file coding recipe.

Use this skill only for a requested solution or a controlled technical change under WF-0002. Do not invoke it for a change classified as normal by `WORKFLOW_CONTRACT.md`; those changes use the user request, targeted fact discovery, source, types and tests directly.

This skill is Development-only. A coding-ready solution may define scoped white-box verification and name a possible candidate output, but must not include an independent system-test campaign or deployment sequence as implementation steps. Route an explicitly requested SystemTest objective to its skill, which separately chooses control strength and persistence; Deployment always receives its own durable controlled task and plan. Do not call either a project or session phase transition.

Only produce a coding-ready handoff after the reviewer has approved the direction or explicitly asks for implementation detail. If a blocking fact or observable acceptance surface is missing, keep the result at review/research status.

## Load

Read root `AGENTS.md`, `文档/TASK_CONTROL.md`, `文档/工作流/WORKFLOW_CONTRACT.md`, WF-0002, the matched project `AGENTS.md`, `references/solution-template.md`, and the actual durable formal owner selected by the task: AgentEntry, ProductContract, CurrentDesign, Decision, Runbook, StructureContract, WorkflowContract or Workflow. Read only other sources needed by the scope. Do not create a placeholder CurrentDesign when another owner or source and tests already carry the facts; stop if no truthful durable owner can be identified.

## Build the solution

### Review-first format (default)

Lead with one decision table. Each candidate capability must state:

| Outcome / decision | Current support | Required change | Owner | Evidence | Open decision |
|---|---|---|---|---|---|

- Describe only affected dimensions. For example, add a short UI/API/data/security/operations note when it changes the decision; do not require a fixed layer checklist for every outcome.
- Group closely related surfaces and actions by business capability; do not repeat every component, field, endpoint, test class, or historical detail in the main plan.
- State the recommended order and no more than the prerequisite that affects a decision.
- Separate “can start after approval” from “blocked until a product/ownership decision”.
- State schema, migration, public-contract, operational, or compatibility impact only when the outcome reaches that boundary. Name concrete assets only when verified.
- Turn each unresolved product rule into two or three mutually exclusive, labelled choices and mark one as recommended. Do not write vague requests such as “confirm the rule”, “clarify the model”, or “determine ownership” without saying what the reviewer can choose.
- Give a short explicit list of rejected/non-goal legacy behavior so review does not reopen it accidentally.

When a user correction changes the direction contract, regenerate every dependent section and remove incompatible scope, completion, plan, skill, or document wording. Do not preserve the old assumption as a caveat or add a second rule beside it.

Use exact paths, routes, contracts, tables, jobs, and owners only to substantiate a decision or when a coding-ready follow-up is requested. Keep the main plan concise.

### Coding-ready supplement (only after approval or explicit request)

Include these sections in substance:

1. Goal and Scope
2. Facts and Sources
3. Plan and Change Boundary
4. Agent Constraints
5. Success Criteria
6. Stop Implementation Conditions
7. Verification
8. Open Questions and Non-Goals

Use exact paths, routes, contracts, tables, jobs, and owners when verified. Separate verified facts from assumptions and inferences. For phased work, state prerequisites, this phase's stopping boundary, downstream handoff, and explicit exclusions.

## Coding-ready gate

Do not hand the plan to coding unless:

- it preserves the direction contract without redefining its outcome, invariants, ownership, or completion rule;
- the allowed change surface and prohibited dependencies are explicit;
- critical behavior is grounded in source, measured state, or an authoritative contract;
- success is observable through named UI actions, tests, APIs, CLI output, artifacts, or database checks;
- validation commands/checks are concrete and proportionate to risk;
- Development white-box completion evidence and repository state are reported separately; SystemTest or Deployment appears only when the user requested or the workflow actually established those independent tasks;
- missing facts, unavailable secrets/access, unauthorized expansion, and unresolved product choices are stop conditions;
- non-goals prevent scope drift.

Agent constraints must directly state what to read, what may/must not change, required validation, secret limits, and behavior that must remain stable.

Before coding any controlled technical change, establish its A→B delta, agent constraints, observable success criteria and stop conditions. For temporary execution, keep that plan in the current task and do not create persistence artifacts. For durable recovery, register the unfinished task and save exactly one `Kind: ChangePlan`, `Workflow: WF-0002` document under the project's or workspace's `推进中/` directory. `Draft` does not authorize coding. A persisted ChangePlan must depend on at least one actual durable formal owner and state the delta that owner will receive when its facts change; CurrentDesign is required only for capability-design changes.

## Closeout

Keep non-blocking questions separate from stop conditions. Register only user-authorized executable work that needs durable recovery in `文档/TASK_CONTROL.md`; controlled strength alone is insufficient. Do not keep a Development task or ChangePlan active solely because Git closeout or another explicitly requested task remains after implementation and scoped white-box completion has passed. A requested SystemTest objective is a separate task and gets an activity plan only when durable and controlled; Deployment always gets its own durable task and plan. Add `WORK_CANDIDATES.md` only when verified work leaves an independent, evidence-backed, uncommitted outcome; do not type it or auto-promote it. Before reporting completion, execute WF-0004 conditionally and report repository state separately.
