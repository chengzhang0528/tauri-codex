# Extension-Point Discovery

Use this reference after selecting a managed capability program or narrow client built-in. Discover real names from the target repository; do not assume a generic adapter, registry, manifest, loader, or SDK exists.

## Search Before Design

Use `rg` to locate only the relevant owners:

- generic command execution, cwd, stdin/stdout/stderr, deadline, cancellation, and process-tree handling;
- Artifact output directories, validation, upload, and ownership;
- built-in dispatch, stable error mapping, and capability-specific environment;
- startup, exit, restart reconciliation, Session cleanup, and polling loops;
- function-style tool declaration and runtime availability snapshots;
- model-facing managed Skills and reserved namespaces;
- package doctor, runtime probe, package-manager adapter, updater manifest, managed paths, installer target generation, GitHub Release workflow, OSS mirror job, packaging, and integration scripts;
- update UI command/state ownership, active-work drain, restart confirmation, backend process flags, console-window suppression, progress, cancellation, and failure reporting.

Read adjacent tests for every selected owner. Record the exact consumer set before changing a command, environment variable, state field, release component, tool declaration, or managed Skill.

## Select Only Required Connections

| Concern | Add only when |
|---|---|
| Capability-specific state | The generic invocation lifecycle cannot hold the required durable state |
| Lifecycle hook | A verified logical or process resource outlives one invocation |
| Artifact integration | The capability produces files that existing stdout/progress cannot represent |
| Stable capability error | A user or consumer must distinguish it from generic process failure |
| Tool declaration | The model needs a callable function contract |
| Managed Skill | Reliable use requires selection, orchestration, recovery, or cleanup guidance |
| Updater/release integration | The capability ships as a stable application-managed component |
| Installer integration | The capability changes prerequisites, platform registration, shortcuts, launcher/bootstrap behavior, or installer-owned files |
| Update UI | The user can check, start, cancel, defer, restart, or recover an update |
| Public API or UI | Existing generic command/progress/result/Artifact surfaces cannot represent the user result |

For stateful programs, define the exact client-to-program state transport and expose only the capability's project-scoped root. Keep deep doctor checks out of document synchronization and other hot loops.

When maintaining an existing capability, begin at the failing behavior and preserve its released CLI/state compatibility unless the task explicitly authorizes a break. A neighboring capability may illustrate source navigation but does not define a universal lifecycle.

Trace the installed artifact back to its source before choosing an updater. Inspect package manifests, lockfiles, installer configuration, desktop framework config, release workflows, registry metadata, and current install layout. Do not infer npm, GitHub Release, OSS, or a custom launcher merely from the language used by one component.
