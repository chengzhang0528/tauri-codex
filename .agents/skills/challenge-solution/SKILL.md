---
name: challenge-solution
description: Use when the user explicitly asks to challenge, question, rebut, stress-test, or find flaws in an existing solution, technical proposal, architecture plan, migration plan, implementation plan, or governance proposal. Review the exact supplied proposal as a read-only artifact, expose unsupported assumptions, scope expansion, ownership mistakes, missing evidence, and avoidable complexity, then recommend the user's next step without rewriting, executing, persisting, or treating the review as approval.
---

# Challenge Solution

## Goal

Act as a reviewer of one existing proposal. Test whether it is necessary, correctly scoped, evidence-backed, owned by the right mechanisms, and proportionate to the user's outcome.

Do not author the missing proposal, revise the reviewed artifact, implement it, create task state, or turn the review into an approval workflow. When a proposal must first be created, route that work to `../technical-solution/SKILL.md` and review its returned artifact only after it exists.

## Fix the Review Target

1. Identify the exact proposal, version, or bounded text under review. If no stable review target exists, state that finding and recommend producing one; do not invent it inside this skill.
2. Recover the latest user outcome and explicit exclusions. Treat prior conversation as decision evidence, not as product or code truth.
3. Read only the sources required to test material claims. For workspace proposals, use source, types, tests, and the routed formal owner; do not scan unrelated documentation.
4. Keep the proposal unchanged during review. A materially revised proposal is a new review target.

## Challenge the Proposal

Lead with concrete findings, ordered by impact:

- **Necessity**: Can the outcome be achieved by an existing owner or simpler mechanism?
- **Scope**: Does the proposal change unrelated tasks, projects, users, phases, skills, documents, or runtime behavior?
- **Assumptions and evidence**: Which decisions depend on unverified facts, stale conversation, convention, or inferred authority?
- **Ownership**: Are product facts, implementation design, reusable method, workflow authorization, task state, and operations assigned to their actual owners?
- **Complexity and lifecycle**: Does it introduce duplicate rules, recursion, permanent state, compatibility burden, or process ceremony without a durable need?
- **Completion and failure**: Are success, stopping conditions, validation, rollback, and unresolved decisions observable and proportionate?
- **Counterexamples**: Test at least one nearby case that should be covered and one that must remain outside the proposal.

Do not manufacture objections for balance. Preserve parts supported by evidence and distinguish blocking findings from optional improvements.

## Return the Review

Return:

1. the review target;
2. findings first, with evidence or a concrete counterexample;
3. retained decisions that survived the challenge;
4. one verdict: `pass`, `revise`, or `reject`;
5. one recommended next user action.

End the current task after the review. The user may later request revision, implementation, or no further action. That later request receives its own authorization and task classification.

## Boundaries

- Never modify workspace files, external state, task control, candidates, plans, or Git state.
- Never infer implementation authority from `pass`, from the existence of a proposal, or from earlier conversation.
- Do not review the review recursively. The review is evidence about the proposal, not a replacement proposal.
- If another skill or the user creates a revised proposal, review that new artifact once when explicitly requested or required by its owning method.
- Do not make this skill an automatic gate for ordinary questions, normal Development, or every technical solution. Trigger only on an explicit challenge request or a direct handoff from a method that requires proposal review.
