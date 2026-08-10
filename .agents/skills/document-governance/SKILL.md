---
name: document-governance
description: Use for document governance, 文档治理, user-controlled task execution-type and activity-plan alignment, AGENTS/Workflow/StructureContract control planes, deleting absorbed process documents or reports, reducing prompt hot paths, classifying document Kind, or aligning TASK_CONTROL with current facts.
---

# Document Governance

## Goal

Keep only current facts, runnable entrypoints, necessary boundaries, and unresolved evidence. Documentation is a control plane, not a process archive.

## Load

1. Read root `AGENTS.md` and `文档/TASK_CONTROL.md`.
   Read `WORK_CANDIDATES.md` under `文档/` only when the task concerns known later work, candidate inventory, promotion, or completeness of remaining work.
2. For a workspace change, read `文档/工作流/WORKFLOW_CONTRACT.md` and select one main Workflow; read WF-0004 before closeout.
3. Read the matched project `AGENTS.md` and only the ProductContract, CurrentDesign, Runbook or skill it routes to.
4. Read `文档/WORKSPACE_STRUCTURE.md` only when creating, moving, deleting or classifying documents, or when the owner is unclear.

## Workflow

1. Classify every in-scope document on two axes: its authoritative owner and its primary consumer. Then choose keep, simplify, merge, move, link or delete.
2. Put cross-project human tasks in `/人类-文档/`, project-specific human guidance in the adjacent README, and agent routing or reusable methods in AGENTS/Workflow/skill. Do not treat every file a person may read as human-documentation content.
3. Keep facts shared by people and agents in their sole ProductContract, CurrentDesign, Decision, Runbook, Material or source owner; expose them to people through links instead of copies. Split mixed documents before moving only the human task flow.
4. Preserve the sole source of a current contract, design, decision, runnable procedure, safety constraint, material or unresolved Issue. Merge duplicate facts and delete absorbed reviews, completed plans, reports, placeholders and stale navigation without tombstones.
5. Classify each task on three independent axes through `WORKFLOW_CONTRACT.md`: execution type, normal or controlled strength, and temporary or durable recovery. Never declare a project/session current phase or use CI, release gates, test tooling, breadth, candidates or environments to create an unrequested SystemTest. Put only user-authorized durable work and every active ChangePlan/SystemTestPlan/DeploymentPlan/Issue in `TASK_CONTROL.md`; controlled strength alone does not register work.
6. Put only independent, evidence-backed, uncommitted outcomes in `WORK_CANDIDATES.md`. Require a valid Basis, one actual Owner, a user-visible Trigger, and dependencies; omit status, execution type, environment, artifact, target and authorization. Promote only on an explicit user task by atomically removing the candidate, classifying the three axes, and registering only when durable recovery is required. Treat the inventory as known, not exhaustive; audit the declared scope before claiming completeness.
7. Repair all inbound links, Depends On paths, source comments, task entries and skill references before deleting or moving a path. Token savings require removing an unnecessary agent default route, not merely changing a directory.
8. When a user correction changes a durable rule, identify the mistaken assumption, choose the narrowest truthful generality (`case`, `capability`, `project`, or `workspace`), and replace the conflicting rule at its sole owner. Re-evaluate dependent documents, skills, task state, candidates, and links; update the same outcome instead of appending a caveat or creating a duplicate.

## Writing Rules

- Keep only current boundary, logic, invariant, failure/compatibility behavior, verification and next executable condition.
- Keep agent entries as short conditional routers plus non-negotiable gates. Move narrative, tutorials and command sequences to the human surface, but retain shared authoritative facts at their real owner.
- Human task-page filenames match their H1 exactly. Preserve standard entry filenames, numbered durable documents and activity IDs instead of renaming them as part of unrelated cleanup.
- Product behavior belongs to ProductContract; capability implementation belongs to CurrentDesign; irreversible choice belongs to Decision; agent-facing operational selection belongs to Runbook; human procedure belongs to `/人类-文档/` or an adjacent README; specialist method belongs to skill.
- A specific incident or conversation is evidence for a correction, not automatically a workspace-wide rule. Store only the reusable invariant at the owner and level where it remains true without the originating example.
- ChangePlan records one durable controlled Development A→B task; SystemTestPlan records one explicitly requested durable controlled candidate/environment campaign and its request source; DeploymentPlan records one explicitly requested artifact/target operation and is always durable. Temporary work has no activity document even when controlled. Each activity document is deleted when its own task concludes and must never absorb a different execution type. Issue exists only while a recoverable problem remains.
- Preserve detailed history only for a Decision, required Material or unresolved handoff. Code-reconstructible routes, DTOs, fields, test outputs and fixed defects do not become long-lived documentation.
- Read and write UTF-8. Use relative paths inside the workspace and never copy secrets, logs, customer data or generated artifacts.

## Validate and Close

When formal documents or governance entrypoints changed, run `npm run check:docs`, targeted inbound-reference scans and scoped `git diff --check`; confirm human pages are not mandatory agent context and that shared owners were linked rather than copied. Do not run a workspace-wide diff check solely for an unrelated dirty worktree. Execute WF-0004 conditionally: reconcile only real long-lived facts, remove this task's activity entry when its completion rule passes, and append a completion fact only for a registered result that will affect later choices. Never retain one task plan for a different execution type or Git closeout. Report this task's type, conclusion, evidence, and repository state; mention another task type only when the user requested or the workflow actually established it.
