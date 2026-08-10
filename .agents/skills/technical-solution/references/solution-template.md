# Review-first technical solution template

Use this short outline by default. Do not expand it into a file-by-file recipe unless the reviewer approves the direction or explicitly asks for a coding-ready supplement.

## 1. Direction contract

- Outcome
- Boundary
- This task's execution type
- Invariants
- Ownership
- Completion rule
- Material open decisions

Carry these from `align-solution-direction`; do not redefine them in the solution.

## 2. Review summary

State the recommended decision, scope, and whether the plan is ready to code.

## 3. Decision table

| Outcome / decision | Current support | Required change | Owner | Evidence | Open decision |
|---|---|---|---|---|---|

Use only the evidence needed to support each decision. Add UI, API, data/schema, security, operations, migration, or compatibility notes only for dimensions that the outcome actually affects; name concrete assets only when verified. Group related surfaces and actions.

Every unresolved product rule must become two or three labelled choices, with one marked recommended. Do not leave a reviewer with an open-ended “define/confirm rule” request.

## 4. Recommended order and boundaries

- Ordered slices with the one prerequisite that matters
- Explicit rejected/non-goal behavior
- Questions that require reviewer direction

## 5. Evidence and verification

- Key verified sources and material assumptions
- Scoped white-box evidence that satisfies the completion rule
- SystemTest or Deployment task status only when the user requested or the workflow established it
- Repository state, reported separately from Development completion

---

# Coding-ready supplement

Add the following only after approval or an explicit request for implementation detail. Headings may be translated or combined only when all content remains explicit.

## 1. Goal and Scope

- Problem and expected outcome
- Included repositories/modules/contracts
- This task's execution boundary
- Explicit exclusions

## 2. Facts and Sources

| Type | Fact | Exact source/evidence |
|---|---|---|
| Verified |  |  |
| Assumption/inference |  | Why it is not yet verified |

## 3. Plan and Change Boundary

- Ordered implementation slices
- Exact code/document/contract touch points
- Prerequisites and downstream handoff
- Consumer or migration impact

## 4. Agent Constraints

- Read first
- May change
- Must not change
- Behavior to preserve
- Required validation
- Secret/access limits

## 5. Success Criteria

Use observable outcomes only and preserve the completion rule from the direction contract:

- UI: named route, interaction, state, and visible result
- Backend/infrastructure: named tests, API assertions, CLI output, generated artifact, or direct database query/state

## 6. Stop Implementation Conditions

Stop when a required fact conflicts or remains unverified, scope needs unauthorized expansion, a forbidden dependency must change, required access is unavailable, no observable acceptance surface exists, or a product/ownership decision remains blocking.

## 7. Verification

- Exact scoped build/typecheck/test commands that provide white-box completion evidence
- Selected Development-scoped API/CLI/browser/database checks; route broad candidate testing to a separate SystemTestPlan
- Evidence or report required by current runbook
- Risk-based regression checks
- SystemTest, Deployment, and repository state, without using them to reverse the Development verdict

## 8. Open Questions and Non-Goals

Keep only non-blocking questions here. List non-goals explicitly; move every blocking question into section 6.

## Gate

Before handoff, confirm the preserved direction contract, explicit boundary, grounded facts, direct constraints, observable criteria, stop conditions, concrete verification, and non-goals. When a correction changes the direction contract, rewrite all dependent sections and delete incompatible wording rather than appending an exception.
