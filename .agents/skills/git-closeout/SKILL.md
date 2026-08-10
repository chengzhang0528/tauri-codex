---
name: git-closeout
description: Use automatically after one independent task has a passing conclusion, required activity cleanup is finished, and file changes can be attributed exactly. Runs the only add/commit/fetch/push path in a child process, preserves unrelated changes, reports Git separately, and never treats a commit or push as SystemTest evidence or Deployment authorization.
---

# Git Closeout

## Goal

Turn a verified independent result into one exact commit on its current branch and push it without absorbing another task's changes. This is a repository closeout gate, not a functional completion gate, SystemTest, Deployment, recovery, merge, release, or history-rewrite tool. A successful push never authorizes package publication or target-environment change.

Use `scripts/run-git-closeout.mjs` as the only path for `git add`, `git commit`, `git fetch`, and `git push`. Invoke it with `node` so the Git operations run in a dedicated child process. Do not reproduce those write operations manually when this skill applies.

## Decide the scenario

| Scenario | Action |
|---|---|
| Independent work is complete, required validation passed, lifecycle cleanup is complete, and changed files have exact ownership | Run `commit-push` before the final report. |
| The work is complete but produces no repository change | Return `skip_no_changes`; do not create an empty commit. |
| The current checkout is an unborn `main` and `origin` has no branches | Allow one exact initial commit and push; recheck that the remote is still empty immediately before pushing. |
| The user explicitly forbids commit or push for this request | Skip and report the explicit override. |
| Validation failed, required validation was not run, or this work's active task/ChangePlan/SystemTestPlan/DeploymentPlan still exists | Stop before Git mutation. |
| Unrelated changes are unstaged and disjoint from every owned file | Keep them unstaged and continue with explicit owned file paths only. |
| One owned file contains changes from multiple tasks or its ownership is uncertain | Stop; do not stage the whole file or guess hunks. |
| Any content was already staged before this run | Stop; do not unstage, amend, or include it. |
| The current branch, linked-worktree state, or remote branch differs from the default | Stop unless the current user request explicitly authorizes that exact existing branch or linked worktree. Never create either. |
| Local and remote history are not exactly synchronized before the commit | Stop; do not merge, rebase, reset, or force push. The only exception is an unborn local `main` with a completely empty `origin`. |
| The remote changes after the local commit or push fails | Keep the local commit, stop, and report its SHA and the failure. Do not rewrite history. |

Run once per independently complete work item. A successful run leaves no owned change, so it must not recursively trigger itself.

## Build the owned path set

1. Start from files actually created, changed, deleted, or renamed for this independent work item.
2. Include its final lifecycle edits, such as removing its `TASK_CONTROL.md` row, deleting its Active ChangePlan, and updating a durable owner when required.
3. Review `git diff -- <path>` for every selected file. A path is eligible only when the entire current file change belongs to this work item.
4. Pass files individually with repeated `--path`. Do not pass directories, globs, `.`, or a repository-wide path.
5. Never include secrets, credentials, logs, generated output, dependency directories, IDE state, or agent runtime state. The runner also blocks common unsafe path classes, but that check does not replace ownership review.

Other unstaged files may remain in the worktree. Any pre-existing staged file is an ambiguous ownership state and must stop the run.

## Run the child process

Use the repository Node runtime and provide concise, meaningful evidence rather than the word `passed` alone:

```powershell
node .agents/skills/git-closeout/scripts/run-git-closeout.mjs `
  --mode commit-push `
  --repo . `
  --message "chore: add verified git closeout" `
  --verification "node --test ...; npm run check:docs; git diff --check -- <owned paths>" `
  --path AGENTS.md `
  --path .agents/skills/git-closeout/SKILL.md
```

The default target is `origin/main`, and the current checkout must be the primary checkout of `main`. Only when the current user explicitly requests another already-existing branch may `--branch <name>` be passed. Only when the current user explicitly requests an already-existing linked worktree may `--allow-linked-worktree` be passed. The runner never creates a branch or worktree.

Use `--mode check` to diagnose readiness without staging or committing. A successful `check` is not completion; rerun `commit-push` after all lifecycle edits are final.

## Interpret the result

The process emits one JSON result and uses these exit classes:

- Exit `0`: `pushed`, `ready`, or `skip_no_changes`.
- Exit `2`: a controlled stop before commit, such as invalid ownership, existing staged content, wrong checkout, or divergence.
- Exit `3`: a local commit exists but synchronization or push did not complete.
- Exit `1`: invalid invocation or an unexpected runner failure.

Report Git closeout as complete only for `pushed`, or for an explicit `skip_no_changes`/user override. A controlled stop changes only the Git status: it must not reverse an already established functional `通过`, and the caller must not restore or retain a task/ChangePlan solely for commit or push. When exit `3` occurs, report the local commit SHA and do not retry through merge, rebase, reset, amend, or force push.

## Verification contract

Maintain scenario tests in `scripts/run-git-closeout.test.mjs`. They must cover at least:

- exact commit and push while preserving an unrelated unstaged file;
- no-change skip;
- missing verification evidence;
- pre-existing staged content;
- non-default branch without explicit authorization;
- remote-ahead or divergent history before commit;
- exact initialization of an empty remote and rejection when another remote branch already exists;
- cleanup of owned staging when an unborn repository cannot create its first commit;
- unsafe or non-file path rejection.

Run the focused Node tests, skill metadata validation, required document gate, and scoped whitespace check before using this skill to close its own changes.
