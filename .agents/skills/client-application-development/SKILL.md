---
name: client-application-development
description: Standardize client application technology selection, installer construction, fixed Aliyun OSS binary publication, official package-manager/framework updates, update UX, signing, post-activation recovery, and lifecycle validation across desktop, launcher, updater, native, mobile, and web clients. Use for Chinese requests about 技术选型、开发客户端、安装包、GitHub Release、OSS、检测更新、应用内更新、自动更新、版本、发布、回滚 or similar client delivery work.
---

# Client Application Development
Use one lifecycle method across desktop, launcher, updater, native, mobile, web, store, and package-manager clients while preserving each project's framework contract. Frameworks are adapters, not mandates.

Use the `https` scheme with the fixed OSS host `shared-public-assets.oss-cn-beijing.aliyuncs.com`; publish workspace-owned client binaries only below `<project-prefix>/`. The bucket is fixed by this Skill; each project owns only its collision-free prefix and product-specific asset names. GitHub may hold source, tags, release notes, and optional automation, but no Installer, manifest, payload, checksum bundle, or other release binary.

Keep other product facts in the project's ProductContract, Decision, CurrentDesign, source, tests, and Runbook. Never copy another project's product name, prefix, component names, installer format, or credentials.

## 1. Establish The Contract
Before implementation, identify the client variant, current technology stack, supported OS/device/browser and architectures, installation source, distribution mode (`self-use` or `production`), OSS project prefix, canonical version source, installer/package boundary, update owner, component owners, any explicit in-place compatibility promise, user confirmation boundary, active-work drain rule, post-activation recovery contract, hidden-process requirement, signing/provenance requirement by component owner, and this request's execution type. Resolve each project fact from the formal owner and code evidence. If owners conflict, repair the single fact owner before expanding scope.

Never infer SystemTest or Deployment from build success, CI, a tag, a candidate, a release, or Git push. A feature plus its focused verification remains Development unless the user separately asks for an independent test or deployment result.

Load [technology-and-update-channels.md](references/technology-and-update-channels.md) completely when choosing a stack or installer, publishing OSS first-install assets, selecting an official updater, or designing update interaction and process behavior.

## 2. Choose The Delivery Shape
For a managed desktop client with a large runtime, bundled CLI, browser engine, language runtime, model, SDK, or other replaceable payload, choose a thin-installer architecture by default:

- **Thin installer**: installs a stable Launcher/Updater, product icon, shortcuts, uninstall/repair registration, and only the licenses/bootstrap data needed to start. It is a low-frequency bootstrap and normally changes only when launcher or installer behavior changes.
- **Full installer**: carries the client and required payloads for one-shot or offline delivery. Use it only when an explicit offline-install, store, one-file, or compatibility contract requires it.
- **Hybrid**: keeps a minimum fallback payload in the installer. Use it only when the formal contract identifies exactly which assets must remain offline.

Do not call a large self-contained package “thin”. Measure installer bytes separately from installed disk usage and first-run download. A thin installer reduces initial transfer and allows payload reuse; it does not remove the need to manage the final runtime footprint.

On Windows, default a thin client without a Service, driver, machine-wide shared resource, or privileged prerequisite to a current-user MSI. Escalate to machine-wide installation only when one of those platform requirements is verified. Load [thin-installer.md](references/thin-installer.md) before designing or coding; adapt only its project prefix, schema version, component names, and platform details.

## 3. Resolve Release Intent
Make normal release requests human-friendly and deterministic:

1. Read the canonical current version and immutable release metadata.
2. Infer the bump from explicit intent; default to the next patch when unspecified. Use minor/major only when project policy or clear intent supports it.
3. Synchronize verified version consumers and calculate the next version automatically.
4. Build and verify an immutable candidate; derive tag, manifest, metadata, and artifact names from that same version.
5. Show the resolved version before irreversible publication.

`发布` means calculate and prepare the next release, not require the user to type a version. `发布 vX.Y.Z` is an optional constraint: validate monotonicity, uniqueness, and policy; never overwrite an existing tag or immutable asset. Release intent is not Deployment authorization. Named-target publication still needs the project's controlled Deployment workflow, target, admission evidence, authorization, and rollback.

Keep Installer and client versions independent. Store the current Installer version and verified candidate identity in one project-local canonical metadata file; initialize it on the first build, default later unspecified releases to a patch bump, and reuse an already verified same-version candidate after interruption.

## 4. Common Lifecycle
Apply only the stages relevant to the client variant:

`discover -> resolve release -> build -> verify -> publish immutable assets -> commit bootstrap/index last -> check -> probe/reuse -> download -> verify -> unpack/doctor -> stage -> drain active work -> confirm -> activate -> health check -> project-owned recovery`

### Build and publish
- Use the existing framework's official package/installer builder when it satisfies the platform contract. Build every declared frontend, native, and helper entry for supported platforms from one frozen source/version; keep installer, launcher, client, and third-party components independently attributable, then calculate size and SHA-256 from the final bytes after signing when the selected mode or component owner requires it.
- Before freezing manifest or candidate identity, run the cheapest final runtime checks against the component trees that will actually be archived or installed, such as a Manager runtime check and bundled CLI `--version`. Dependency preparation or cache-hit checks do not replace this final candidate doctor.
- For split desktop clients, verify the packaging graph explicitly: every HTML entry is emitted, every native binary is built, the install-time bootstrap exists before bundling, and the final public bootstrap is generated only after the Installer digest is known. Require one clean-output build before first publication.
- Generate a manifest containing release identity, platform, architecture, minimum compatible launcher/client, component version, source/object key, archive/installation rule, byte size, SHA-256, and signature/provenance.
- Publish Installer, Bootstrap, manifests, component payloads, checksums, and third-party fallback objects only to the fixed Aliyun OSS bucket under the project's prefix. Do not publish release binaries to GitHub or any second permanent origin.
- Before exposing a release or moving Bootstrap, verify the named OSS publishing environment, non-empty required configuration, project-prefix write access, and anonymous public read-back with a disposable project-scoped probe. Never print credential values.
- Model publication as resumable stages: build once; freeze version, bytes, object keys, sizes, digests, and provenance; upload immutable OSS objects; anonymously read each object back; then commit the one mutable Bootstrap last. A retry reuses the frozen candidate and verified objects; it never rebuilds the same version or substitutes another origin.
- Upload immutable assets first. Read every object back and verify size, digest, schema, and launcher compatibility. Update one mutable bootstrap/index pointer only after the complete closure is readable. A failed pre-commit publication must leave the old pointer usable.
- Preserve third-party licenses and notices with the component that requires them.
- Ordinary client releases must not rebuild a stable installer unless installer/launcher behavior or installer-owned assets changed. Reuse the already published installer reference.
- A launcher, updater, source-policy, or bootstrap-compatibility change requires a new Installer/Launcher version. Treat historical in-place compatibility as opt-in: unless the ProductContract explicitly promises it, consider older launcher protocols, schemas, and private installer state unsupported and use the installation source's official replacement path. Use a newer Setup for a standalone desktop client and the platform store, package manager, framework updater, or managed reprovision path for other clients; use desktop uninstall/reinstall only after normal Setup/upgrade fails. Repair an unreachable official entry or misplaced state at its owner instead of turning either defect into an implicit compatibility promise. Add only a bounded bridge or migration when the ProductContract explicitly promises in-place compatibility or the official replacement path cannot preserve and re-establish required user-owned state. Do not add dual protocols, dual schemas, compound upgrade closures, rollback machinery, or historical-state migration solely to avoid reinstall.
- Keep client and Installer versions in separate canonical files. Version automation for an ordinary client release must not rewrite the Installer version; resolve a reused Installer's public size/digest from its immutable published asset.
- GitHub tags and release notes may identify the source revision, but their download links must point to anonymously readable OSS objects and must not attach binaries. A GitHub Workflow may invoke the same local publisher but must not become the sole release owner.

### Installer, launcher, and running client
- **Installer/store** owns first install, prerequisites, repair, upgrade, uninstall, platform integration, and shortcut policy. Prefer the platform's normal repeat/in-place path when it is supported and compatible, but do not turn that preference into an application-level compatibility promise.
- **Launcher/Updater** owns bootstrap/manifest reads, compatible release selection, component probing, download, integrity and required signature/provenance verification, safe unpack, doctor/smoke, staging, activation, health check, the project-owned post-activation recovery contract, and starting the client. It consumes distribution endpoints supplied by the deployment or platform owner and does not impose HTTPS/TLS, certificate, scheme, Origin/Host, redirect, or source-allowlist policy.
- **Running client/manager** owns user intent, visible state, active-work draining, and confirmation. It must not directly download, unpack, replace its own files, or hold release-write credentials. A helper is required to replace a running executable safely.
- **System prerequisites** remain separate from app-managed components. Probe and reuse an eligible system component without copying, upgrading, editing, or changing global PATH; install a missing/insufficient prerequisite only through the owner defined by the product contract.

For Windows MSI first install, repair, or in-place upgrade, start exactly one Launcher setup after `InstallFinalize`. If an old install still owns running product processes, list them and ask the user to save work, request normal shutdown, and show a waiting state. After five seconds, terminate only processes whose verified executable path is inside the product installation root. Honor a longer shutdown deadline only when source or a formal platform contract already defines one. Never terminate reused system components or unrelated processes. Repair and upgrade must preserve one product registration, user settings, and application-owned business state.

### Runtime update
- Default a Manager-style desktop update surface to one fixed primary control. It performs a read-only `Check for updates` intent while idle or current, then changes to the applicable explicit update intent only after a compatible target is available or staged. Keep it disabled while loading, checking, or updating; periodic checks must not unlock an active foreground request early.
- Match the update owner to the installation source. Use the official package-manager command/API, platform store, desktop framework updater, or existing launcher; `npm update` is only applicable after proving npm owns that installed component and its version semantics are correct.
- When an existing launcher owns manifest-driven updates, resolve compatible artifacts from the manifest rather than filenames or directory listings. Download to a private temporary location with bounded size, safe paths, cancellation, and resumability only if explicitly designed. Use the configured endpoint without rejecting HTTP or adding custom certificate, scheme, Origin/Host, redirect, or source-allowlist checks. Verify byte count, SHA-256, signature/provenance, platform, architecture, and compatibility before atomic staging. Unpack defensively and run component doctor/smoke before readiness.
- When a package manager, store, or framework updater owns discovery and installation, do not duplicate its download, unpack, signature, staging, or activation implementation. Adapt its official status/result while retaining the application's active-work, confirmation, hidden-process, and user-visible recovery contract.
- For launcher-managed desktop clients, enable automatic check and verified background download/staging of compatible application components by default: check at Launcher startup and about every six hours while the Manager is running. An incompatible Installer/Launcher target must remain a user-action-required state and must not be automatically prepared or activated. Create no Service or scheduled task after the client exits. Automatic work must not activate, restart, or force-close active work; show the remaining action and require explicit confirmation for activation/restart/version switching unless an existing no-session contract permits activation. Recheck the target and lock immediately before activation.
- Execute update tooling from a backend-owned hidden process and stream bounded progress to the existing UI. On Windows, suppress console creation for `.cmd`, `.bat`, PowerShell, package-manager, installer, and helper processes. Never flash a terminal or create a second frontend/window for update progress.

### Activation and recovery
- Keep `current` runnable until the candidate passes pre-activation validation. Activate with an atomic directory/pointer swap or platform-supported updater without deleting the only runnable release or user data.
- Resolve one post-activation recovery contract from the project's formal owner: automatic rollback or forward repair. Use automatic rollback only when binaries, configuration, and persisted data are backward-compatible and the rollback path is tested; retain a known-good `previous` release while the new one is observed. For forward repair, do not mark an incompatible prior release as a rollback target.
- Run a minimal process/UI/runtime health check after activation. On failure, execute the selected recovery contract, preserve diagnostics and user data, and report the failed phase, release, component, recovery mode, remaining runnable state, and result.

### Product state migration
- Keep product registrations, workspace/project identity, settings, credential references, results, and audit data outside Installer/Launcher ownership. Repair and in-place upgrade preserve them.
- Let the Manager or owning application migrate only explicitly enumerated retired official identifiers, derived registration IDs, and owned state paths. Make migration idempotent and atomic; preserve unrelated state and fail on unknown sources, corruption, or old/new conflicts instead of guessing.
- Uninstall removes installer-owned binaries, staging, shortcuts, registration, and update policy. It must not modify reused system programs or silently delete user-created business data.

## 5. Security, Compatibility, Verification
- Transport security belongs to the deployment edge, reverse proxy, operating system/browser, or an explicitly named platform owner. Application code must neither add custom HTTPS/TLS, certificate, scheme, Origin/Host, redirect, or network-source enforcement nor override the runtime's transport behavior. Keep business authentication/authorization and artifact integrity as separate application responsibilities.
- Default workspace-owned delivery to `self-use` unless the user explicitly requests large-scale public distribution or promotion, or selects a channel whose admission requires publisher signing. Personal, internal, and limited controlled distribution remain self-use; Installer, OSS, GitHub Actions, Release, candidate, or version metadata do not imply production. Application-owned Launcher, Manager, Installer, updater, and helper binaries may use explicit unsigned self-use provenance, but bind final bytes to exact object key, size, SHA-256, frozen source commit, and applicable doctor; missing publisher credentials are not a blocker. Third-party and platform-owned binaries retain required upstream, package-manager, framework, or store provenance and must never inherit this allowance. Use a distinct production mode with required publisher signing, notarization, or store provenance for an explicit public-promotion release. SHA-256 proves integrity, not publisher identity.
- Reject invalid versions/manifests, downgrade, mismatched platform/architecture, unsafe paths, wrong size/digest, truncated downloads, invalid signatures, and incompatible components.
- Default to no historical compatibility bridge. The absence of an explicit in-place promise selects the installation source's official replacement path; it does not require a separate declaration that old versions are unsupported and is not a reason to stop. Verify that this path is reachable and restores the complete required state, but fix failures in that path or its state ownership before considering compatibility machinery. Add a bounded bridge or migration only for an explicit in-place promise or required user-owned state that the official replacement path cannot preserve and re-establish.
- Keep binaries, configuration, credentials, user data, and migrations in separate ownership boundaries. Never commit keys, tokens, customer data, logs, or generated secrets.
- For Development, run affected source/type checks and focused tests, including version resolution, manifest validation, component reuse, staging, active-work waiting, cancellation, activation failure, health failure, and the selected post-activation recovery mode. Run independent SystemTest or Deployment only when explicitly requested/authorized.
- Verify the single dynamic control and two-step intent contract, repeated-click exclusion, stale-result handling, startup/six-hour automatic checks, absence of Service/scheduled-task persistence, hidden background execution, no terminal/duplicate-window flash, defer/cancel boundaries, active-work preservation, restart confirmation, selected recovery mode, process-takeover boundary, state preservation/migration, and official-source installed version.

## 6. Release Candidate Acceptance
For desktop clients, source checks are not sufficient. After a target release is public in OSS, validate the exact published asset as a user would:

- Download the installer from its public OSS object key and verify object metadata, size, SHA-256, platform, architecture, and signing/provenance status.
- Install or upgrade using the downloaded installer; launch the installed binary, never the worktree or debug build.
- Assert startup view, navigation, key controls, no blank/error/partial page, and every changed user-visible update state.
- For thin installers, additionally verify: blank machine bootstraps successfully; eligible system components are shown as reused; missing components are fetched from the fixed source; digest/doctor failures keep the old current release; the contract-selected Setup, upgrade, or reinstall path preserves user-owned state; activation follows the selected post-activation recovery contract.
- Make the OSS root unreachable and prove the client fails with a diagnosable source error while leaving `current` and user data intact; it must not fall back to GitHub, a registry, or another binary origin.
- Record installed version, process/window health, registration, shortcuts, component versions, current/staged state, any contract-owned previous state, recovery result, and remaining user action. Worktree/debug UI checks are supplementary only.

## 7. Report And Stop
Report the resolved version and calculation, delivery shape and measured installer/installed sizes, artifact/platform evidence, manifest and digest, current/staged/activated state, any contract-owned previous state, recovery mode/result, remaining user action, exact verified environment, unsupported cases (recommend an Issue), and Development/SystemTest/Deployment/Git results separately.

Stop if the project prefix, canonical version owner, a signing authority required by the selected production/channel/component contract, confirmation boundary, active-work drain condition, or post-activation recovery contract is missing and cannot be discovered safely. Missing self-use publisher credentials are not a blocker for application-owned binaries. Missing compatibility policy uses the no-historical-compatibility default above; a claimed in-place promise must come from the ProductContract. A local installer is not a published release, and an OSS outage does not authorize a fallback origin.

Load references only when needed:

- [thin-installer.md](references/thin-installer.md): thin-installer component model, state machine, release transaction, and black-box acceptance.
- [release-and-versioning.md](references/release-and-versioning.md): version resolution and immutable publication rules.
- [lifecycle.md](references/lifecycle.md): state ownership and adapter matrix.
- [technology-and-update-channels.md](references/technology-and-update-channels.md): stack and installer selection, fixed OSS publication, official update owners, single-control two-step UX, and hidden process behavior.
