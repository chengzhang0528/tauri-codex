---
name: system-testing
description: Run or govern a SystemTest task only when the user explicitly asks for an independent integration, system, full-suite, candidate-acceptance, release-gate, or regression result against existing code. Choose control strength and persistence independently; never infer authority from a candidate list, CI, a release gate, test tooling, or suite size.
---

# System Testing

## Goal

Produce an assertion-backed conclusion for an explicitly identified workspace or immutable candidate without changing product code or deploying it to a formal target.

SystemTest is a user-controlled task type, not a project/session state or an automatic validation tail of Development. A completed feature, available test command, browser, real database, large suite, or desire for more confidence does not authorize this skill.

## Classify control and persistence

Read root `AGENTS.md`, `文档/工作流/WORKFLOW_CONTRACT.md`, WF-0005, the matched project `AGENTS.md`, and only the selected Runbook and test sources. Read a SystemTestPlan only for durable controlled execution. `WORK_CANDIDATES.md` never authorizes this skill.

First confirm that the current user asks for an independent test conclusion against existing code. A request to implement or fix something and run tests as acceptance remains Development, even when those tests are broad. CI configuration or a release gate describes scope only after the user authorizes this test task.

Use **normal SystemTest** only when all are true:

- the tested subject is the current workspace or another explicitly identified local subject;
- the environment is the current workspace or a user-specified isolated local environment, not shared or formal;
- no immutable release candidate decision or durable campaign evidence is required.

Use **controlled SystemTest** when any normal condition fails, including an immutable candidate, shared or named test environment, or user-requested CI/release-gate qualification. Fix the user request source, Candidate, Environment, scenario scope, assertions, entry conditions, exit conditions and non-goals before execution.

Then independently choose persistence. Temporary testing has no TASK_CONTROL row or SystemTestPlan, even when controlled; keep its fixed campaign in the current task and report identity, environment, scope and assertions. Use durable recovery for a multi-session campaign, persistent blocker, or shared mutable environment state. Proceed only with:

- an independent task labelled `Phase: SystemTest`;
- a SystemTestPlan whose `Requested By` records the current user request identifier or verifiable intent summary, never CI, a release gate, or automation;
- one unambiguous Candidate such as a commit, build, package, image, or immutable bundle;
- one named Environment isolated from formal deployment targets;
- scenario scope, assertions, entry conditions, exit conditions, and non-goals;
- plan status `Active` after all entry conditions are fixed.

If request authority is absent, do not create a task or plan. Candidate and Environment must be fixed before any controlled test; do not register `pending` placeholders.

## Run the campaign

1. Record the tested subject and verify that the executed code or artifact matches it. A dirty workspace is valid only for normal local testing when it is identified as the subject; never present it as an immutable release candidate.
2. Verify environment ownership, credentials by configuration key, data isolation, cleanup boundary, and the selected Runbook.
3. Execute only the approved campaign. Use existing runners and reports; do not create a parallel harness unless the task explicitly includes test infrastructure development.
4. Assert observable behavior and material side effects. Separate product assertions from environment, credential, startup, migration, fixture, and runner failures.
5. Keep the tested subject read-only. On a product or test implementation defect, retain the minimum reproduction and stop the affected campaign path; create or link an independent Development task only after the user authorizes the fix.
6. For controlled testing, rerun against a new Candidate only after the authorized Development task closes and the SystemTestPlan is updated. Never silently substitute the artifact.

## Boundaries

- Preparing an isolated test environment from the Candidate is allowed; publishing a formal package or changing a production/staging deployment target is not.
- Do not edit product source, test source, migrations, fixtures, or configuration tracked by the product during the campaign. Such changes belong to Development.
- Do not turn Development evidence into SystemTest because it uses integration boundaries, a browser, real database, real service, multiple roles, or a large suite. Test scope and environment are execution inputs, not substitutes for the user's explicit test goal.
- A passing controlled candidate campaign may provide evidence for a later Deployment task. Neither normal nor controlled testing creates or authorizes Deployment.
- An unavailable environment means SystemTest evidence is unavailable. It does not reverse a previously completed Development result.

## Closeout

Report `Task type: SystemTest`, control strength, persistence, tested workspace or Candidate, Environment, selected scenarios, exact commands or runners, assertion results, environment failures, skipped scope, retained safe evidence, and residual risk. Report deployment eligibility only for a controlled candidate campaign. Remove a durable SystemTestPlan and task when its candidate receives a final conclusion; for either strength, record defects without editing them and wait for user authorization before creating a Development task. Do not run deployment or imply deployment authorization.
