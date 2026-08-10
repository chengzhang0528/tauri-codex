---
name: replicate-workspace
description: Use when initializing or repairing a document-driven workspace control plane and its grounded human entry from an existing AGENTS.md/TASK_CONTROL/StructureContract/Workflow model. Trigger for 复刻工作空间, 初始化工作空间, 文档驱动开发, dual documentation surfaces, or control-plane upgrades. Do not infer product design, stack, APIs, source skeletons, services, or implementation directories.
---

# Replicate Workspace

## Principle

Replicate only the operating model. Treat the reference workspace as a source of governance patterns, never as the target's product truth.

Store repository skills in Codex's discovered `.agents/skills/` tree and copy only the methods the target control plane actually uses.

Keep the agent control plane and human task surface distinct. Replicate a root README and `/人类-文档/` only when the target has at least one verified human task or source entry; link target-owned facts instead of copying reference content.

## Workspace Isolation Gate

Before any write:

1. Resolve and record the canonical reference root and target root. Use `git -C <path> rev-parse --show-toplevel` for Git workspaces and `Resolve-Path` for the final filesystem path. Stop if the roots are equal unless the request explicitly repairs that workspace.
2. Capture scoped `git status --short` baselines for both roots, list the exact target-owned paths, and hash every existing reference file whose relative path collides with that manifest. The reference workspace is read-only during replication; do not use it as a staging area or template scratch space.
3. For every write, normalize the destination and assert that it starts with the canonical target root. For cross-root `apply_patch` on Windows, use absolute target paths with forward slashes; never rely on the process working directory, use relative patch paths while the working directory is the reference workspace, or mix reference and target paths in one patch.
4. Write one small target file first. Immediately assert that it exists under the canonical target root, that the matching reference hash and status are unchanged, and that target `git status` reports the expected path. Only then write the remaining target files.

Keep target creation and reference remediation as separate phases. Complete and validate the target before any reference repair. If an unexpected reference delta appears, freeze further reference writes, preserve concurrent work, and repair only changes attributable to the replication after the target passes or the user explicitly changes the order. Never use reset, checkout, a whole-commit revert, or a commit created after replication began as a substitute for the verified pre-task baseline.

## Minimal Control Plane

```text
<workspace>/
  AGENTS.md
  .agents/
    skills/
      <required-skill>/SKILL.md
  文档/
    TASK_CONTROL.md
    WORKSPACE_STRUCTURE.md
    工作流/
      WORKFLOW_CONTRACT.md
      WF-0001-日常变更.md
      WF-0002-技术变更.md
      WF-0003-调查与阻断.md
      WF-0004-任务收口.md
    工作空间/
      归档/DOC-0001-已完成任务事实.md
```

When a real project ID and source root are known, add only `文档/项目/项目_<id>/AGENTS.md` and the category directories required by grounded current documents. Do not create README inside the governed `文档/` tree, TASKS, relationship indexes or placeholder category files.

When a human surface is grounded, add only the real entries needed now:

```text
<workspace>/
  README.md
  人类-文档/
    README.md
    <task-category>/<task title>.md
```

The root README is the shortest human navigation, `/人类-文档/README.md` is the complete human entry, and task pages link ProductContract, CurrentDesign, Runbook or source owners. Do not create an empty task category or mirror the formal documentation tree.

## Workflow

1. Execute the Workspace Isolation Gate and keep the resolved roots visible in every write operation.
2. Read the reference root AGENTS, task control, structure contract, workflow contract, relevant project entry and validation script. If a human surface is requested, also read its human-documentation method and navigation pattern. Extract only startup order, task truth, Kind/lifecycle, safety gates, closeout, routing and audience separation.
3. Inspect the target and preserve `.git`, existing source roots, current tasks and user-created content. Never overwrite dirty or concurrent work.
4. Treat control-plane initialization and semantic repair as controlled work, then independently decide persistence. Same-turn work uses a current-task plan without TASK_CONTROL or ChangePlan; durable recovery registers one task and, for checker or other technical changes, creates an Active WF-0002 ChangePlan that depends on an actual durable formal owner. Require CurrentDesign only when capability-design facts change.
5. Create or repair the minimal control plane using only verified absolute target paths. Declare only real document locations and classify existing content by both authoritative owner and primary consumer, not by filename or reference-workspace layout. Create the human surface only from verified target commands, source entries and facts.
6. Do not seed an unrequested active task. When the reference provides evidence-backed future outcomes, create or update a lazily read WorkInventory without status or execution type; do not invent build, API, UI, database, deployment or source work.
7. Validate the target metadata, Kind/Status placement, Workflow/Action definitions, task/activity bindings, links, materials and UTF-8; run scoped whitespace checks that include untracked target files. Recheck both workspace statuses and all captured reference hashes; fail if the reference differs from its baseline.
8. Execute the target WF-0004. Remove any migration ChangePlan and task only after current facts are reconciled and all triggered checks pass; append one completion fact only when a registered result will affect later choices.

## Boundaries

- Do not modify, stage, commit or push the reference workspace as part of replication.
- Never invent `apps/`, `services/`, `packages/`, `src/`, framework files, lock files, APIs, data models, source roots or a literal root `项目/` source folder.
- Never copy reference product contracts, service names, credentials, connection strings, logs, customer data or generated artifacts.
- Do not retain initialization reports, formal-document README indexes, completed plans or duplicate directory maps. A grounded root README and `/人类-文档/README.md` are the explicit human-entry exception.
- Stop when a path cannot be proven to resolve under the target root, the reference baseline changes unexpectedly, a document owner is ambiguous, an active task depends on content proposed for deletion, or migration would change product/source behavior without authorization.

## Closeout

Report the resolved target root, control-plane files created or repaired, whether `TASK_CONTROL.md` changed, document moves/merges/deletions, target validation results, and whether the reference workspace matched its captured baseline. Report any reference repair as a separate scoped result.
