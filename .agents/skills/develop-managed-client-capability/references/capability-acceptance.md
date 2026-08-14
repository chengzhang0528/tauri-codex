# Capability Acceptance

Run the common baseline for every code change. Add only conditional rows whose trigger is true, and report skipped high-risk rows with their reason.

## Common Baseline

- Test argv and stdin bytes, unknown arguments, nonzero exit, deadline, cancellation, and process-tree termination when a subprocess exists.
- Test project cwd and prove model input cannot select project/client identity, credentials, or application-owned absolute state roots.
- Bound stdout/stderr and diagnostics; exclude secrets, credentials, signed URLs, and customer data.
- Verify deterministic function-style tool declarations and explicit available/degraded behavior when declaration surfaces change.
- Run affected source/type tests and build, skill validation, and `git diff --check`; run document checks when formal documents or governance routes change.

## Conditional Checks

| Trigger | Required evidence |
|---|---|
| Produces Artifact | File count and byte limits, supported media, real decode when applicable, link/path escape, identity change, digest, upload failure, and ownership |
| Persists state | Workspace-external project-scoped root, schema/version, atomic write, corruption, restart, stale state, and previous-version readability |
| Owns logical lease | Opaque ID, project/session ownership, TTL, idempotent release, restart reconciliation, and cross-boundary rejection |
| Owns process lease | Logical-lease checks plus process-tree cleanup, verified OS identity, PID reuse, client exit, and session cleanup |
| Uses network/database/device | Configured endpoint or device boundary, timeout, cancellation, credential source, redaction, least OS privilege, and unavailable behavior; no application transport-security policy |
| Adds managed executable | Bounded machine-readable version, separated doctor levels, unknown-command rejection, supported builds, and direct CLI contract tests |
| Ships in stable release | Manifest/path/source, size/digest/signature, licenses, private environment, release doctor, atomic activation, mixed-version coexistence, and rollback |
| Publishes first-install channels | GitHub Release tag/version/assets/notes plus API read-back; any OSS mirror has identical bytes, size, digest, platform, and version and cannot drift into a separate build |
| First run needs a package registry | Configured registry identity and reachability, exact package/version, credential boundary when private, timeout/failure UX, and no false offline-install claim; or an immutable bundled package installed through the official package manager |
| Uses an official update adapter | Installation-source detection, exact official command/API, compatible version resolution, bounded progress/error output, cancellation boundary, and no custom release-asset downloader when the official owner suffices |
| Adds update UI | First action checks only; second action updates only after availability; up-to-date, available, checking, updating, waiting, restart-required, cancelled, failed, and recovered states; repeated-click exclusion and retry |
| Spawns update tooling | Backend-only execution, hidden console/process flags on Windows, no new renderer/frontend window, bounded stdout/stderr, process-tree cancellation, and no visible terminal flash |
| Changes tool declaration | Function signature, input/output/effects/errors, stable ordering, cached availability, hash update, unavailable behavior, and no workflow duplication |
| Changes managed Skill | Valid frontmatter/trigger, concise workflow, deterministic materialization, namespace collision handling, project Skill coexistence, and no reliance on text for enforcement |
| Adds client built-in | Narrow namespace, no interpreter behavior, deadline/cancel, trusted-state ownership, and invalid-subcommand tests |
| Changes public contract | Enumerated client/server/web/generated consumers, contract generation/no-drift, and all affected tests |
| Changes agent-visible flow | Real supported Agent execution through the product path; direct binary execution alone is insufficient |
| Uses project/session identity or persistent resources | Two distinct projects and clients with unique markers proving no state, process, output, or Artifact crossing |
| Changes visual output | Native image inspection and required viewport checks in addition to functional tests |

Complete only when every selected requirement passes, capability-owned resources are cleaned, rollback claims have a real previous-version test when applicable, and the long-term CurrentDesign owner matches actual behavior.
