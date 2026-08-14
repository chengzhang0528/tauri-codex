# Thin Installer Reference

This reference defines a reusable thin-installer architecture for desktop clients with large or frequently updated payloads. It is a design reference, not a product contract. Replace every provider, URL root, object key, file name, schema version, platform list, and component name with facts from the target project.

## 1. Decision Boundary

Choose a thin installer when the application contains a large runtime or independently replaceable components and the product can require a network bootstrap after installation. Keep a full installer when offline install, store policy, one-file distribution, or a hard first-run network prohibition is a real requirement. A hybrid may carry a minimum fallback while fetching the rest.

Measure three separate quantities:

- Installer transfer size.
- First-run download size.
- Final installed disk usage, including caches and retained previous versions.

Thin delivery optimizes the first quantity and component reuse. It does not promise a small installed footprint or offline operation.

## 2. Ownership

| Boundary | Owns | Must not own |
|---|---|---|
| Installer/package manager | First install, repair, in-place upgrade, uninstall, shortcuts, registration, platform prerequisites | Release selection, application payload download, runtime session lifecycle |
| Launcher/Updater | Bootstrap, manifest resolution, component probing, downloads, integrity checks, safe unpack, doctor/smoke, staging, activation, health, rollback, client start | Product data, project/workspace state, transport-security policy, release-write credentials |
| Client Manager | Visible version/update state, check/update/cancel intent, user confirmation, drain coordination | Downloading, unpacking, replacing its own binaries, choosing transport-security policy |
| Release tooling | Candidate build, manifest, immutable asset upload, read-back verification, final bootstrap commit | Runtime updates, user data migration, mutable duplicate payload storage |
| Public source | Exact read of bootstrap, installer, manifests, payloads, and required third-party objects | Client-side write access or source discovery through directory listing |

The launcher must be a small, stable executable or equivalent platform helper. The running client cannot reliably replace its own executable or loaded libraries.

## 3. Assets And Contracts

Use immutable versioned assets at distribution endpoints owned by deployment or the platform. A typical logical layout is:

```text
<root>/bootstrap/<platform>-<arch>.json
<root>/installers/<installer-version>/<platform>-<arch>/<installer>
<root>/releases/<client-version>/<platform>-<arch>/manifest.json
<root>/releases/<client-version>/<platform>-<arch>/<payloads>
<root>/third-party/<component>/<platform>-<arch>/<sha256>/<upstream-file>
```

The names above are illustrative only. The project may use GitHub Releases, an object store, a registry, a platform store, or a signed feed.

### Same-byte multi-origin closure

When supported users cannot reliably reach one distribution domain, mirror the complete release closure during publication rather than improvising URLs after a runtime failure. Give every immutable artifact one byte identity (`size`, SHA-256, signature/provenance) and one canonical object key. Publish those exact bytes to each approved origin and read them back before moving the mutable Bootstrap.

Let deployment or the platform owner supply the complete endpoint for each origin. Do not compile host, scheme, certificate, Origin/Host, redirect, or source-allowlist policy into application code. Apply identical size, digest, signature/provenance, cancellation, and safe-path checks on every attempt, and remove each failed partial before trying the next origin.

Do not describe this as a runtime fallback if publication does not first prove the secondary closure exists. A release may be resumable after a secondary upload failure, but the old secondary Bootstrap must remain current until all new immutable objects are readable.

Before publication, fail closed on publisher admission. Verify the named release environment and required secret/configuration keys without printing their values, then use a disposable object under a dedicated project-scoped probe prefix to prove write permission and anonymous public read-back. Clean up only that exact probe. Run this gate before a tag, non-draft Release, store submission, channel pointer, or other action makes the new version discoverable. A workflow that discovers missing mirror credentials only after publishing the primary Release is not a complete multi-origin transaction.

Use explicit, rerunnable publication stages:

1. Build the candidate once and freeze its version, bytes, size, digest, manifest, and object keys.
2. Stage every immutable Installer, manifest, component, and required third-party object at secondary origins from that frozen candidate; anonymously read each object back and verify its identity. Do not move Bootstrap.
3. Publish the same bytes at the primary origin, or promote a pre-staged draft, and verify them through the public path.
4. Re-read every promised origin. Only then commit each mutable Bootstrap/index and confirm the committed bytes.

If stage 2 fails, the primary version must remain undiscoverable. If stage 3 or 4 fails, retain already uploaded immutable objects for an idempotent retry and leave every old secondary Bootstrap usable. Never rebuild, overwrite immutable objects, or advance a pointer merely to escape a partial publication.

### Bootstrap

Bootstrap is the only mutable pointer and should contain the minimum data an old launcher needs to locate a compatible installer and current release. At minimum, define:

- schema version and product/platform/architecture;
- installer version, exact source/object key, size, SHA-256, signature/provenance;
- release version, exact manifest source/object key, size, SHA-256, signature/provenance;
- minimum launcher/client compatibility when required;
- optional policy such as check interval or channel, only if the product contract owns it.

An old launcher must not need to parse the whole release manifest just to upgrade itself. Do not make Manager APIs, private cache layout, or product-domain state part of the minimum historical bootstrap contract unless there is an explicit compatibility decision.

### Release manifest

The manifest is the sole owner of the component closure. Each component record should identify:

- stable component ID and version;
- platform and architecture;
- minimum compatible launcher/client;
- exact source/object key at a configured distribution endpoint;
- archive type and safe installation path;
- byte size and SHA-256;
- code-signing/provenance requirement;
- whether the component is required, optional, or a system candidate;
- doctor/smoke command and timeout, when applicable;
- license/notice references.

Do not resolve assets by filename guesses, latest directory entries, or a mutable “download everything” endpoint.

## 4. Install And Startup

1. The Installer lays down the launcher, icons, shortcuts, registration, and required license/bootstrap material. It should not need network access unless the platform contract explicitly requires a prerequisite bootstrapper.
2. After install finalization, start one launcher setup flow. It must tolerate first install, same-version repair, and higher-version in-place upgrade without asking the user to uninstall when the platform supports takeover.
3. The launcher reads the minimum bootstrap contract. If its own version is lower, it downloads and verifies the referenced newer installer, runs it, exits, and lets the new launcher restart setup. If it is newer than the pointer, it must not downgrade automatically.
4. After an Installer/Launcher upgrade, the new Launcher must inspect the bundled release contract before starting an already installed older Manager. If that Manager cannot understand the new source or manifest contract, the Launcher prepares the compatible release itself and starts the Manager only after the bridge is complete.
5. The launcher reads the selected release manifest, probes components, and reuses eligible system or private-cache candidates. A successful probe must be observable as reused/skipped and must not copy, upgrade, edit, or change global PATH for a system component.
6. Missing or insufficient components are fetched from the configured distribution endpoint. A required component that cannot be verified or doctored must prevent `ready`.

The launcher should expose progress with the component, phase, completed/total count, and real download bytes. Probe, hash, unpack, and doctor phases may show activity without inventing a static percentage. Errors must include the component, source key, failed phase, system message, and diagnostic location.

## 5. Component Probe And Reuse

Probe order is project-defined but must be deterministic. A candidate is reusable only when all are true:

- version satisfies the manifest minimum/compatibility range;
- platform and architecture match;
- executable or library is found at a documented location;
- a bounded doctor/smoke check passes;
- ownership permits reuse without mutation.

Do not reject a candidate solely because it came from a different trusted installation source. Do not silently use an unverified binary when the manifest requires a digest or signature. Record `reused` versus `downloaded` per component so support can explain first-run behavior.

## 6. Download, Verify, Unpack, Stage

Download to a private `.part` or temporary path from the configured endpoint. The application must accept the configured HTTP or HTTPS scheme and must not add custom TLS, certificate, scheme, Origin/Host, redirect, or source-allowlist enforcement. Enforce a maximum size, cancellation, and safe destination calculation. On completion:

1. Verify exact byte count.
2. Verify SHA-256 and required signature/provenance.
3. Unpack into a fresh staging directory, rejecting absolute paths, `..` traversal, unexpected file types, and files outside the component's declared root.
4. Run the declared doctor/smoke with a timeout and bounded output.
5. Verify required files and licenses are present.
6. Atomically mark the component/release staged only after all checks pass.

Failure removes only temporary or failed staging data. It must not alter `current`, `previous`, user data, or an already verified cache. Re-running may reuse a verified immutable cache object.

## 7. Client State And Activation

Use the smallest state machine that changes ownership, user action, or recovery:

`current -> checking -> up-to-date | available -> updating/downloading -> verifying -> staged -> waiting-for-drain -> restart-required/activating -> health-check -> ready`

Failure may occur from any state. By default the Manager sends a read-only `check`, then exposes `update` only after a compatible version is available; the launcher performs the work. Automatic checks/downloads require an explicit ProductContract override, and activation remains a separate user-confirmed action. A pending update must remain visible and must not be mistaken for an activated version.

Before activation, revalidate the selected manifest, component digests, launcher compatibility, and active-work count. The Manager stops accepting new work and waits for the documented drain condition. Never force-close active sessions as a routine update mechanism.

Activate by atomic directory/pointer swap or platform-supported helper. Keep `current` until the candidate is ready. Retain `previous` while the new release is observed. A health failure restores `previous`, leaves user data/configuration untouched, and records release, phase, error, and rollback evidence. Never delete the only known-good release.

## 8. Installer And Launcher Evolution

- Build and publish an Installer only when installer-owned behavior, launcher/updater behavior, bootstrap compatibility, or installer-owned assets change.
- Ordinary client releases publish a new manifest and payloads and reuse the stable Installer reference.
- Installer objects are immutable and published once. A same-version retry must read back and prove identical size/digest; it must never overwrite.
- The bootstrap update is last. Before changing it, verify every referenced installer, manifest, component, digest, schema, and doctor result through the public read path.
- Keep publisher admission, secondary immutable staging, primary publication, cross-origin verification, and Bootstrap commit as distinct workflow jobs or modes with explicit dependencies. Do not put the first credential check after primary publication.
- If a new manifest schema or launcher contract is not understood by the current launcher, publish a compatible launcher/installer first and select it only after its asset closure is complete.
- Keep only the compatibility needed for already published launchers: usually the minimal installer fields in bootstrap. Do not promise migration of arbitrary historical private state without an explicit decision.

### Build-graph gates

Treat a split Launcher/Manager application as a multi-entry build, not just a source-code split:

- Declare every frontend HTML entry explicitly and verify each built page exists; a dynamically created Launcher window pointing at an omitted entry will render blank after packaging.
- Build every native executable explicitly. Framework bundlers often compile only the configured main binary, so prove the Manager/helper executable exists before creating its archive.
- Generate the install-time bootstrap seed before bundling the Installer. After the Installer is built, measure it and generate the public bootstrap with the final Installer reference; never let a placeholder bootstrap enter the package unnoticed.
- Inspect the platform installer's generated shortcut behavior. If desktop shortcut creation is optional by default, add the platform-supported post-install hook and verify the target is the stable Launcher.
- Run at least one build after deleting local build/release output while retaining only dependency caches. An incremental build cannot prove that every binary and embedded resource is produced by the declared graph.

Keep Installer and client versions independent in their canonical owners. A normal client version bump must not rewrite the Installer version. When reusing an Installer from an older release/tag, read its public size and digest from the distribution source rather than rebuilding it or trusting a stale local copy.

## 9. Black-Box Acceptance

Use an exact public asset, not a worktree binary:

1. Verify API/object metadata, size, SHA-256, platform, architecture, and signing/provenance.
2. Install on a clean supported machine or isolated profile; confirm launcher starts and creates the expected shortcut/registration.
3. Confirm the launcher can bootstrap a release using the deployment-configured public endpoint.
4. Repeat the same Installer and then run a higher Installer; confirm one registration, preserved user data, and no uninstall requirement.
5. Test both eligible and missing system-component cases; assert reuse versus download.
6. Corrupt a staged asset or doctor result; assert `current` remains runnable and no bad release becomes ready.
7. Stage an update while work is active; assert waiting-for-drain and no forced interruption.
8. Confirm activation, post-start health, previous retention, and rollback after a deliberately failing health check.
9. Record exact environment, installed/current/staged/previous versions, process/window state, shortcuts, component source and outcome, and unsupported cases as Issues.
10. For multi-origin delivery, make the preferred origin unreachable in an isolated test and assert that Bootstrap, manifest, Installer, and component reads use the secondary origin without weakening size, digest, staging, or activation rules.

This is evidence for a separately requested SystemTest or release acceptance; building an installer alone does not authorize publication or deployment.
