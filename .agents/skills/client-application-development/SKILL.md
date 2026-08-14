---
name: client-application-development
description: Standardize client application technology selection, installer construction, GitHub Release and mirror publication, official package-manager/framework updates, update UX, signing, rollback, and lifecycle validation across desktop, launcher, updater, native, mobile, and web clients. Use for Chinese requests about 技术选型、开发客户端、安装包、GitHub Release、OSS、检测更新、应用内更新、自动更新、版本、发布、回滚 or similar client delivery work.
---

# Client Application Development

Use one lifecycle method across desktop, launcher, updater, native, mobile, web, store, package-manager, and CDN clients while preserving each project's framework and distribution contract. Frameworks are adapters, not mandates.

Keep product facts in the project's ProductContract, Decision, CurrentDesign, source, tests, and Runbook. This Skill stores reusable method only. Do not copy another project's product names, cloud provider, object paths, installer format, or credentials.

## 1. Establish The Contract

Before implementation, identify the client variant, current technology stack, supported OS/device/browser and architectures, installation source, distribution target, canonical version source, installer/package boundary, first-install channels, update owner, component owners, user confirmation boundary, active-work drain rule, hidden-process requirement, signing/provenance requirement, and this request's execution type. Resolve each fact from the project's formal owner and code evidence. If owners conflict, repair the single fact owner before expanding scope.

Never infer SystemTest or Deployment from build success, CI, a tag, a candidate, a release, or Git push. A feature plus its focused verification remains Development unless the user separately asks for an independent test or deployment result.

Load [technology-and-update-channels.md](references/technology-and-update-channels.md) completely when choosing a stack or installer, publishing GitHub Release/OSS first-install assets, selecting an official updater, or designing update interaction and process behavior.

## 2. Choose The Delivery Shape

For a client with a large runtime, bundled CLI, browser engine, language runtime, model, SDK, or other replaceable payload, evaluate a thin-installer architecture before adding payloads to MSI/NSIS/AppImage/DMG:

- **Thin installer**: installs a stable Launcher/Updater, product icon, shortcuts, uninstall/repair registration, and only the licenses/bootstrap data needed to start. It is a low-frequency bootstrap and normally changes only when launcher or installer behavior changes.
- **Full installer**: carries the client and required payloads for one-shot or offline delivery. Choose it only when offline installation, store policy, one-file distribution, or a proven compatibility requirement outweighs the larger download.
- **Hybrid**: keep a small fallback payload in the installer and fetch optional/replaceable components after install. Document exactly which assets are offline and which are network-dependent.

Do not call a large self-contained package “thin”. Measure installer bytes separately from installed disk usage and first-run download. A thin installer reduces initial transfer and allows payload reuse; it does not remove the need to manage the final runtime footprint.

When the project chooses thin delivery, load [thin-installer.md](references/thin-installer.md) before designing or coding. Adapt its contracts to the project; never copy its example provider, path, schema version, or names verbatim.

## 3. Resolve Release Intent

Make normal release requests human-friendly and deterministic:

1. Read the canonical current version and immutable release metadata.
2. Infer the bump from explicit intent; default to the next patch when unspecified. Use minor/major only when project policy or clear intent supports it.
3. Synchronize verified version consumers and calculate the next version automatically.
4. Build and verify an immutable candidate; derive tag, manifest, metadata, and artifact names from that same version.
5. Show the resolved version before irreversible publication.

`发布` means calculate and prepare the next release, not require the user to type a version. `发布 vX.Y.Z` is an optional constraint: validate monotonicity, uniqueness, and policy; never overwrite an existing tag or immutable asset. Release intent is not Deployment authorization. Named-target publication still needs the project's controlled Deployment workflow, target, admission evidence, authorization, and rollback.

## 4. Common Lifecycle

Apply only the stages relevant to the client variant:

`discover -> resolve release -> build -> verify -> publish immutable assets -> commit bootstrap/index last -> check -> probe/reuse -> download -> verify -> unpack/doctor -> stage -> drain active work -> confirm -> activate -> health check -> retain previous/rollback`

### Build and publish

- Use the existing framework's official package/installer builder when it satisfies the platform contract. Build every declared frontend, native, and helper entry for supported platforms from one frozen source/version; keep installer, launcher, client, and third-party components independently attributable, then calculate size and SHA-256 after final signing.
- For split desktop clients, verify the packaging graph explicitly: every HTML entry is emitted, every native binary is built, the install-time bootstrap exists before bundling, and the final public bootstrap is generated only after the Installer digest is known. Require one clean-output build before first publication.
- Generate a manifest containing release identity, platform, architecture, minimum compatible launcher/client, component version, source/object key, archive/installation rule, byte size, SHA-256, and signature/provenance.
- When one public network domain is not reliable for all supported users, build a same-byte multi-origin closure during publication. Keep one canonical artifact identity and let the deployment or platform owner provide the configured distribution endpoints; the application must not invent transport-security policy.
- Before exposing a new release or moving any public pointer, verify every promised origin's publisher admission: the named environment exists, required secret/configuration keys are non-empty, the credential can write and anonymously read back a disposable project-scoped probe, and secret values never enter logs. A missing secondary-origin credential blocks publication before the primary release becomes public.
- Model multi-origin publication as resumable stages: build once; stage and read back immutable objects at secondary origins; publish the same candidate at the primary origin; verify every public origin; then commit each mutable bootstrap/index last. Keep these stages separately rerunnable so a late failure never rebuilds or substitutes the candidate.
- Upload immutable assets first. Read every object back and verify size, digest, schema, and launcher compatibility. Update one mutable bootstrap/index pointer only after the complete closure is readable. A failed pre-commit publication must leave the old pointer usable.
- Preserve third-party licenses and notices with the component that requires them.
- Ordinary client releases must not rebuild a stable installer unless installer/launcher behavior or installer-owned assets changed. Reuse the already published installer reference.
- A launcher, updater, source-policy, or bootstrap-compatibility change requires a new Installer/Launcher version. Complete that upgrade bridge before starting or delegating to an older running client that cannot consume the new release contract.
- Keep client and Installer versions in separate canonical files. Version automation for an ordinary client release must not rewrite the Installer version; resolve a reused Installer's public size/digest from its immutable published asset.
- Use one versioned GitHub Release as the canonical human-download record when the project selects GitHub. If OSS is required for reliable first installation, mirror the exact signed installer bytes and checksum under an immutable versioned key, read both back, and publish any mutable install pointer last. Keep these acquisition channels separate from later in-app updates; prefer the official package manager, store, framework updater, or established launcher matching the actual installation source.

### Installer, launcher, and running client

- **Installer/store** owns first install, prerequisites, repair, upgrade, uninstall, platform integration, and shortcut policy. It must support repeat execution and in-place upgrade without requiring uninstall when the platform permits.
- **Launcher/Updater** owns bootstrap/manifest reads, compatible release selection, component probing, download, integrity/signature verification, safe unpack, doctor/smoke, staging, activation, health check, rollback, and starting the client. It consumes distribution endpoints supplied by the deployment or platform owner and does not impose HTTPS/TLS, certificate, scheme, Origin/Host, redirect, or source-allowlist policy.
- **Running client/manager** owns user intent, visible state, active-work draining, and confirmation. It must not directly download, unpack, replace its own files, or hold release-write credentials. A helper is required to replace a running executable safely.
- **System prerequisites** remain separate from app-managed components. Probe and reuse an eligible system component without copying, upgrading, editing, or changing global PATH; install a missing/insufficient prerequisite only through the owner defined by the product contract.

### Runtime update

- Default to a user-invoked `Check for updates` action that performs a read-only version check and shows `up to date`, `update available`, or a retryable failure without mutation. Only after availability, expose a separate `Update now` action with target version, expected download/restart impact, and any active-work block.
- Match the update owner to the installation source. Use the official package-manager command/API, platform store, desktop framework updater, or existing launcher; `npm update` is only applicable after proving npm owns that installed component and its version semantics are correct.
- When an existing launcher owns manifest-driven updates, resolve compatible artifacts from the manifest rather than filenames or directory listings. Download to a private temporary location with bounded size, safe paths, cancellation, and resumability only if explicitly designed. Use the configured endpoint without rejecting HTTP or adding custom certificate, scheme, Origin/Host, redirect, or source-allowlist checks. Verify byte count, SHA-256, signature/provenance, platform, architecture, and compatibility before atomic staging. Unpack defensively and run component doctor/smoke before readiness.
- When a package manager, store, or framework updater owns discovery and installation, do not duplicate its download, unpack, signature, staging, or activation implementation. Adapt its official status/result while retaining the application's active-work, confirmation, hidden-process, and user-visible recovery contract.
- Keep automatic checks/downloads disabled by default; only an explicit ProductContract may opt in. Never force-close work, show the remaining action, and require explicit confirmation for activation/restart/version switching unless the contract permits no-session activation; recheck the target and lock immediately before activation.
- Execute update tooling from a backend-owned hidden process and stream bounded progress to the existing UI. On Windows, suppress console creation for `.cmd`, `.bat`, PowerShell, package-manager, installer, and helper processes. Never flash a terminal or create a second frontend/window for update progress.

### Activation and rollback

- Keep `current` runnable until the candidate passes validation. Activate with an atomic directory/pointer swap or platform-supported updater.
- Retain a `previous` known-good release while the new one is observed. Never delete the only runnable release or user data.
- Run a minimal process/UI/runtime health check after activation. On failure, restore `previous`, preserve diagnostics, and report the failed phase, release, component, and rollback result.

## 5. Security, Compatibility, Verification

- Transport security belongs to the deployment edge, reverse proxy, operating system/browser, or an explicitly named platform owner. Application code must neither add custom HTTPS/TLS, certificate, scheme, Origin/Host, redirect, or network-source enforcement nor override the runtime's transport behavior. Keep business authentication/authorization and artifact integrity as separate application responsibilities.
- SHA-256 proves integrity, not publisher identity. Require code signing, notarization, store provenance, or an explicitly documented unsigned-candidate policy.
- Reject invalid versions/manifests, downgrade, mismatched platform/architecture, unsafe paths, wrong size/digest, truncated downloads, invalid signatures, and incompatible components.
- Keep binaries, configuration, credentials, user data, and migrations in separate ownership boundaries. Never commit keys, tokens, customer data, logs, or generated secrets.
- For Development, run affected source/type checks and focused tests, including version resolution, manifest validation, component reuse, staging, active-work waiting, cancellation, activation failure, health failure, and rollback. Run independent SystemTest or Deployment only when explicitly requested/authorized.
- Verify the two-step check/update contract, repeated-click exclusion, stale-result handling, hidden background execution, no terminal/duplicate-window flash, defer/cancel boundaries, active-work preservation, restart confirmation, and official-source installed version.

## 6. Release Candidate Acceptance

For desktop clients, source checks are not sufficient. After a target Release is public, validate the exact published asset as a user would:

- Download the installer from that Release and verify API-reported size, SHA-256, platform, architecture, and signing/provenance status.
- Install or upgrade using the downloaded installer; launch the installed binary, never the worktree or debug build.
- Assert startup view, navigation, key controls, no blank/error/partial page, and every changed user-visible update state.
- For thin installers, additionally verify: blank machine bootstraps successfully; eligible system components are shown as reused; missing components are fetched from the fixed source; digest/doctor failures keep the old current release; repeat MSI execution preserves registration and user data; activation leaves previous and supports rollback.
- For a multi-origin closure, block the preferred origin and prove discovery, manifest, Installer, and component reads fall back to the secondary origin with the same size/digest assertions. A normal online test that happens to use the preferred origin is not fallback evidence.
- Record installed version, process/window health, registration, shortcuts, component versions, current/staged/previous state, and remaining user action. Worktree/debug UI checks are supplementary only.

## 7. Report And Stop

Report the resolved version and calculation, delivery shape and measured installer/installed sizes, artifact/platform evidence, manifest and digest, current/staged/activated/previous state, remaining user action, exact verified environment, unsupported cases (recommend an Issue), and Development/SystemTest/Deployment/Git results separately.

Stop if the canonical version owner, trusted source, signing authority, compatibility rule, confirmation boundary, active-work drain condition, or rollback path is missing and cannot be discovered safely. A local installer is not a published release.

Load references only when needed:

- [thin-installer.md](references/thin-installer.md): thin-installer component model, state machine, release transaction, and black-box acceptance.
- [release-and-versioning.md](references/release-and-versioning.md): version resolution and immutable publication rules.
- [lifecycle.md](references/lifecycle.md): state ownership and adapter matrix.
- [technology-and-update-channels.md](references/technology-and-update-channels.md): stack and installer selection, GitHub Release/OSS first-install channels, official update owners, two-step update UX, and hidden process behavior.
