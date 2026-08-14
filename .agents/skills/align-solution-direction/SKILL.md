---
name: align-solution-direction
description: Use before producing or materially revising any solution, architecture proposal, implementation/refactor/integration/migration plan, rollout plan, or reviewer decision in this workspace. Establish a domain-independent direction contract from the latest user intent, verified facts, and prior corrections; separate outcome, boundary, invariants, completion rules, evidence, and lifecycle state; then pass silently, correct the framing, or stop only for a material decision before downstream skills add concrete detail.
---

# Align Solution Direction

## Goal

Establish the decision frame that every downstream solution must preserve. Align what outcome is being pursued, what belongs in scope, which rules must remain true, what counts as complete, and which choices are genuinely unresolved.

Do not solve the concrete problem here. Do not produce an architecture inventory, layer checklist, file plan, test campaign, or document lifecycle. Route those details to the appropriate downstream skill after the direction is stable.

If current intent and facts already agree, pass silently. Use `../technical-solution/SKILL.md` only when the user needs a concrete technical solution or coding-ready plan.

## Establish Authority

1. Treat the latest explicit user instruction or correction as the desired direction. It supersedes incompatible earlier framing.
2. Treat source, types, tests, measured behavior, and the routed formal owner as evidence about the current state and feasibility, not as authority over the user's desired outcome.
3. Treat conversation history as evidence of decisions and corrections, never as product or code truth. Preserve settled choices unless the user supersedes them or verified facts make them impossible.
4. Verify facts that can be discovered. Ask only when alternatives materially change the outcome or ownership and cannot be resolved from current authority.

Read root `AGENTS.md`, `文档/TASK_CONTROL.md`, the matched project `AGENTS.md`, and only sources needed to resolve a direction-changing fact. Read `WORK_CANDIDATES.md` under `文档/` only when the user asks about later, remaining, next, or roadmap work. Follow `WORKFLOW_CONTRACT.md` only for workspace changes or cross-session recovery.

## Build the Direction Contract

Keep the contract compact and domain-independent:

- **Outcome**: the observable value the user wants now.
- **Boundary**: what is included now, what is explicitly excluded, and the execution type of each task explicitly requested by the user. Use `Development`, `SystemTest`, or `Deployment` per task when workspace delivery work is involved; then independently choose normal or controlled execution and temporary or durable recovery. Never declare a project/session-wide current phase or infer another task from validation breadth, completion, candidates, environment, CI configuration, release gates, or repository state.
- **Invariants**: rules downstream work must not reinterpret or trade away.
- **Ownership**: who owns each retained responsibility or decision.
- **Completion rule**: what determines that the requested work is complete, separated from confidence evidence, later verification, rollout, and repository state.
- **Open decisions**: only unresolved choices that materially change the preceding fields.

Do not fill missing sections with speculative detail. Do not enumerate empty technical layers. A downstream implementation concern becomes part of the direction only when it changes one of these fields.

## Guard Requirement Authority

Classify every input that could change the direction contract:

- **Confirmed requirement**: an explicit user instruction, correction, acceptance, or bounded delegation. Only this class defines the desired outcome and hard requirements.
- **Verified fact**: source, types, tests, measured behavior, or the routed formal owner establish the current state or feasibility. A fact may constrain a truthful solution but does not redefine the user's desired outcome.
- **Working assumption**: an inference, recommendation, preference, or default that has neither authority above. Keep it visible and reversible at its first consequential use.

Silence, repetition, prior-plan inclusion, and downstream copying do not confirm an assumption. Promote it only through explicit user acceptance, or reclassify it as a verified fact when evidence supports it; never blur those two authorities. If it changes the outcome, boundary, invariants, ownership, public or persisted commitments, completion rule, or an irreversible or external action, expose it as an open decision. If it affects only a reversible, non-material implementation choice, state the default and proceed within the confirmed boundary.

Preserve this provenance across every revision and handoff. Do not turn a working assumption into an invariant, acceptance criterion, non-goal, prohibition, ownership decision, compatibility promise, or stop condition merely because an earlier artifact used it. For example, an inferred companion capability remains a suggestion, while a source-proven public consumer is a verified compatibility fact; explicit delegation lets the agent choose only within the delegated boundary.

## Learn From Corrections

When the user corrects an answer or repeatedly rejects the same framing:

1. Identify the incorrect assumption or conflation that produced the error, not just the sentence that was rejected.
2. Determine the correction's proper generality: one case, one class of work, one project, or workspace-wide. Do not turn a concrete example into a universal rule without evidence that the user intends that scope.
3. Replace the old rule at its owner. Do not append a caveat while leaving the conflicting default active.
4. Re-evaluate dependent scope, completion rules, plans, skills, documents, and task state. Remove stale consequences rather than preserving them as history.
5. Apply an abstraction check: the resulting rule must remain understandable and useful after removing the concrete example that exposed it.
6. Keep one outcome identity across corrections. Update the same direction contract, task, plan, or candidate instead of creating a parallel item merely because the user refined acceptance or rejected an assumption.

Corrections may change the direction contract even when no product or code fact changes. They must not be buried as implementation notes.

## Calibrate the Direction

Test the proposed direction with these questions:

1. Does every retained item directly support the current outcome, or is it process residue, speculative hardening, duplicated ownership, or a later phase?
2. Are constraints genuine requirements or merely properties of the first implementation idea?
3. Are separate Development, SystemTest, or Deployment tasks being conflated with one global phase, governance strength, Git/lifecycle state, or tests attached to implementation acceptance?
4. Does a proposed compatibility or operational burden protect a real promise, retained state, or supported consumer?
5. Can an existing owner or mechanism satisfy the outcome with less permanent complexity?
6. Would a downstream skill be able to add concrete detail without changing the outcome, boundary, invariants, ownership, or completion rule?
7. Is a known future result being mistaken for an authorized task, or a finite candidate inventory being mistaken for a complete audit? Candidates remain untyped and uncommitted until the user explicitly starts them; completeness requires a declared audit scope.
8. Can every user acceptance scenario be reached through a supported product entrypoint? Move internal fault injection to focused tests and remove obsolete or impossible scenarios.
9. Are independently changing facts being compressed into one enum or behavior flag, or are new tables, APIs, caches, workers, containers, and UI states being proposed without a proven owner and consumer?
10. Are scale, retention, concurrency, idempotency, recursion, recovery, compatibility, and remote-failure machinery supported by current evidence, or are they speculative operating cost?

Prefer removal, reuse, deferral, or reassignment when they preserve the outcome with less lasting cost. Use external products and general best practices only as candidate generators, never as authority.

Choose the fastest verification that covers the retained risk. Preserve a broader or slower verification path only when a real shared consumer, release promise, or failure boundary makes focused evidence insufficient.

## Decide and Respond

Choose one outcome:

- **Silent pass**: the direction contract matches current intent and evidence. Do not expose this skill or restate the contract; continue directly to the requested work.
- **Correct and continue**: the framing, scope, ownership, completion rule, or task execution type was wrong, but no material user decision remains. State only the changed decisions and continue from the corrected contract.
- **Stop for decision**: unresolved alternatives materially change the outcome, invariants, ownership, persisted/public commitments, or completion rule. Present two or three exclusive options, recommend one, and state the consequence.

Do not stop for discoverable facts, non-material assumptions, implementation preferences, later verification, or repository state. Do not repeat concrete impact matrices, file plans, or verification commands owned by downstream skills.

## Hand Off Without Drift

Pass only the direction contract to downstream skills. They may add concrete design, code boundaries, validation, operations, or documentation, but must not silently redefine the contract.

If downstream evidence makes the direction impossible, return to this skill with the conflicting fact. If it only affects implementation choice, validation confidence, rollout, or Git state, keep the direction unchanged and handle it in the owning skill.

Persist a correction only at its proper owner: product behavior in ProductContract, capability design in CurrentDesign, workspace gates in AGENTS/Workflow, reusable method in a skill, recoverable execution state in its active task or plan, and an evidence-backed uncommitted result in `WORK_CANDIDATES.md`. Controlled work does not by itself require persistence. Do not store conversation narrative or a concrete incident in this skill.
