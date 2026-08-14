---
name: develop-managed-client-capability
description: Develop, extend, maintain, debug, upgrade, package, release, or review trusted application-managed client capabilities, including technology selection, managed programs, narrow client built-ins, lifecycle/state, function-style tool declarations, Artifact output, doctor checks, installer construction, GitHub Release and OSS first-install distribution, official package-manager or framework updates, update UX, release packaging, and tests. Use for internal client plugins or capability extensions whose executable, version, health, resources, delivery, update behavior, or user experience the application must guarantee. Do not use for ordinary application features, arbitrary third-party drop-in plugins, manager-only UI, or installer work unrelated to a capability.
---

# Develop Managed Client Capability

Treat a request called "plugin" as a managed capability only when the current product owns its distribution and lifecycle. Never imply that a public plugin SDK, dynamic loader, or stable third-party ABI exists without source and contract evidence.

## Establish Facts

1. Read the workspace and matched project/source `AGENTS.md`, task control, source, types, tests, ProductContract, CurrentDesign, and Runbook routed by those entries.
2. Search the current executor, tool declaration, Artifact, lifecycle, doctor, installer, package manager, updater, release, and test owners before designing. Load [extension-point discovery](references/extension-point-discovery.md) completely for a code change.
3. Use `align-solution-direction` before proposing a solution and `technical-solution` for controlled coding work. Preserve unrelated worktree changes.
4. Treat source and tests as authority for current behavior. A reference or neighboring capability is a navigation aid, not an API promise.
5. For technology selection, installation, GitHub Release, OSS mirroring, or update behavior, load `../client-application-development/SKILL.md` and its [technology and update channels](../client-application-development/references/technology-and-update-channels.md) reference completely. Keep generic delivery rules there and capability-specific choices here.

## Classify The Form

| Form | Choose when | Action |
|---|---|---|
| Project CLI or Project Skill | Existing project-owned tooling solves the need | Improve that project workflow; do not add a managed capability |
| Managed capability program | The application must guarantee a cross-project executable, version, health, resource behavior, or stable UX | Add or maintain one focused program and only its real integration owners |
| Client built-in | The action must use client-owned state or handles and cannot be delegated | Add the smallest namespaced action; do not create an interpreter or plugin ABI |
| Public contract change | Existing command/progress/result/Artifact contracts cannot express the result | Stop and design the contract with every consumer |
| Third-party drop-in plugin | Independent install, discovery, permissions, or hot loading is required | Stop unless the product already owns a supported public plugin boundary |

Do not promote an executable found on project `PATH` into an application-managed capability without an explicit ownership decision.

## Define The Capability

Before implementation, record:

- stable identity, owner, user problem, and why an existing CLI or Skill is insufficient;
- profile: stateless command, Artifact producer, managed-process lease, logical resource/state, or a justified combination;
- argv/stdin/stdout/stderr, exit behavior, idempotency, deadline, cancellation, filesystem/process/network/device effects, and stable errors;
- state owner, absolute root, schema/version, limits, cleanup, restart, compatibility, and rollback behavior;
- technology stack and reuse evidence, platforms, dependencies, version/doctor behavior, release components, licenses, declarations, exact consumers, success criteria, and stop conditions;
- installation source, installer/package owner, first-install channels, canonical release, official update adapter, update interaction, active-work impact, restart behavior, hidden-process requirement, and rollback.

Select common and conditional acceptance from [capability acceptance](references/capability-acceptance.md). Do not add state, processes, release wiring, public APIs, or UI merely because another capability has them.

## Select Technology And Delivery

- Prefer the repository's existing language, runtime, UI framework, build graph, package manager, signing path, and release automation. Add a new framework only when the current stack cannot produce the required platform artifact and the solution records the permanent cost.
- Match the artifact to the product shape: publish a library or CLI through its native package registry; use the established desktop framework packager for a GUI client; build a native installer only for OS integration, prerequisites, repair/uninstall, shortcuts, or offline bootstrap that package delivery cannot provide.
- Keep one canonical version per independently releasable application, installer, and capability component, plus one immutable candidate for each published version; do not force unrelated components to share a version. Publish platform installers to a versioned GitHub Release. When first-install reachability requires OSS, mirror the exact same installer bytes and checksum; never rebuild a separate OSS candidate.
- Treat GitHub Release and OSS as first-install acquisition channels unless the product contract explicitly selects them as an updater. For later in-app updates, prefer the official mechanism that matches the actual installation source, such as an npm/pnpm package update, Python tool upgrade, system/store update, or the desktop framework's official updater. Do not build a second custom download protocol when that owner already exists.
- Keep capability delivery inside the application's existing installer/release. Do not create another installer, launcher, updater, frontend entry, or release stream for one capability unless it is independently installed and that product boundary is explicit.

## Preserve Update UX

- Default to an explicit `Check for updates` action. The first click only checks and reports `up to date`, `update available`, or a retryable failure; it must not install, restart, or interrupt work.
- After an update is available, expose a distinct `Update now` action with version, expected download/restart impact, and any active-work block. Keep the current version usable when the user defers or cancels.
- Run package-manager/updater work in a backend-owned hidden process. On Windows, suppress creation of a console window even when an official tool uses a `.cmd` or PowerShell shim; stream bounded progress into the existing UI instead of opening `cmd`, PowerShell, a second frontend, or a transient terminal.
- Drain work only when activation actually replaces an in-use process or component. Never force-close sessions, jobs, terminals, or unsaved work. If restart is required, stage first and request confirmation at the last responsible moment.
- Preserve one visible update operation, cancellation where safe, clear failure ownership, and the previous runnable version or the official package manager's recovery path.

## Separate Declarations From Enforcement

- Keep tool declarations function-style: availability/version, callable program or action, signature, inputs, outputs, effects, limits, and stable errors. Do not put orchestration, examples, retries, recovery, or cleanup workflows there.
- Put model-facing selection, composition, diagnostics timing, iteration, recovery, and cleanup in a concise native Skill only when needed.
- Keep identity/cwd routing, invocation shape, namespace protection, availability rejection, deadlines, cancellation, process-tree control, state/lease/Artifact ownership, validation, and limits enforced by code.
- Treat Tool and Skill text as guidance, never execution authority or a security boundary.
- Do not add HTTPS/TLS, certificate, scheme, Origin/Host, redirect, or network-source enforcement in the capability or host application. Transport policy belongs to deployment, the reverse proxy, OS/browser, or an explicitly named platform owner. Keep business authorization and input/output integrity separate.

## Preserve Narrow Ownership

- Let the capability own its business semantics, CLI, state schema, and lease behavior.
- Let the generic client executor own project cwd, deadline/cancel, process trees, project state partitioning, Artifact validation, and lifecycle hook invocation.
- Let document composition own deterministic tool declarations without running deep doctor checks in polling loops.
- Let the selected official package manager, framework updater, store, or existing launcher own discovery/install mechanics. Let the application backend adapt status, hidden execution, active-work drain, confirmation, and recovery; the renderer and running executable do not replace themselves.
- Let the manager own registrations and worker lifecycle, not invocation execution.
- Keep Server/Web generic unless the public result contract truly cannot express the capability.

## Implement And Verify

1. Re-run the discovery searches and identify exact consumers.
2. Reuse the generic executor and Artifact path. Add narrow dispatch, state, error, declaration, or lifecycle behavior only when the selected profile requires it.
3. Separate package/release doctor, cheap cached runtime availability, and explicit deep diagnostics.
4. Make `version` bounded and machine-readable; reject unknown commands and arguments.
5. Add release integration only when stable delivery is in scope, updating the complete enumerated consumer set without creating another installer. Verify GitHub Release assets and any OSS first-install mirror are byte-identical, while subsequent updates use the selected official update owner.
6. Run the common acceptance baseline and every triggered conditional row. Expand testing to other projects only when their contracts or behavior changed.
7. Update the unique CurrentDesign owner only for a real long-term capability boundary or invariant. Complete WF-0004 and do not close while required checks fail or capability resources remain live.

Stop when implementation would require server-side business parsing, model-selected project/client identity, arbitrary remote code installation, manager-side execution, unbounded resources, transport-security policy in application code, or unidentified public-contract consumers.
