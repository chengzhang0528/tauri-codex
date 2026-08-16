# Thin Installer Reference

This reference defines the default thin-installer architecture for managed desktop clients with large or frequently updated payloads. Use the `https` scheme with the fixed OSS host `shared-public-assets.oss-cn-beijing.aliyuncs.com` and publish all workspace-owned binaries below `<project-prefix>/`; adapt only the project prefix, object names, schema version, platform list, and component identities.

## 1. Decision Boundary
Choose a thin installer when the application contains a large runtime or independently replaceable components and can require a network bootstrap after installation. Use a full installer only when an explicit offline-install, store, one-file, or first-run network prohibition requires it. Use a hybrid only when the formal contract identifies the minimum offline fallback.

On Windows, default to a current-user MSI when the product owns no Service, driver, machine-wide shared resource, or privileged prerequisite. Escalate to machine-wide installation only for a verified platform requirement.

Measure three separate quantities:

- Installer transfer size.
- First-run download size.
- Final installed disk usage, including caches and any prior versions retained by the recovery contract.

Thin delivery optimizes the first quantity and component reuse. It does not promise a small installed footprint or offline operation.

## 2. Ownership
| Boundary | Owns | Must not own |
|---|---|---|
| Installer/package manager | First install, repair, in-place upgrade, uninstall, shortcuts, registration, platform prerequisites | Release selection, application payload download, runtime session lifecycle |
| Launcher/Updater | Bootstrap, manifest resolution, component probing, downloads, integrity checks, safe unpack, doctor/smoke, staging, activation, health, project-owned recovery, client start | Product data, project/workspace state, transport-security policy, release-write credentials |
| Client Manager | Visible version/update state, check/update/cancel intent, user confirmation, drain coordination | Downloading, unpacking, replacing its own binaries, choosing transport-security policy |
| Release tooling | Candidate build, manifest, immutable asset upload, read-back verification, final bootstrap commit | Runtime updates, user data migration, mutable duplicate payload storage |
| Public source | Exact read of bootstrap, installer, manifests, payloads, and required third-party objects | Client-side write access or source discovery through directory listing |

The launcher must be a small, stable executable or equivalent platform helper. The running client cannot reliably replace its own executable or loaded libraries.

## 3. Assets And Contracts
Use immutable versioned assets under the fixed OSS bucket and one collision-free project prefix:

```text
scheme: https
host: shared-public-assets.oss-cn-beijing.aliyuncs.com
root: <project-prefix>/
  bootstrap/<platform>-<arch>.json
  installers/<installer-version>/<platform>-<arch>/<installer>
  releases/<client-version>/<platform>-<arch>/manifest.json
  releases/<client-version>/<platform>-<arch>/<component-payloads>
  third-party/<component>/<platform>-<arch>/<sha256>/<upstream-file>
```

Each release contains one manifest plus one payload per independently managed component. Do not add another mutable latest pointer or duplicate a payload under multiple permanent keys. GitHub may hold source, tags, release notes, and optional automation, but no release binaries.

### Single-origin OSS closure
Give every immutable artifact one identity (`objectKey`, size, SHA-256, signature/provenance) under the project prefix. The Launcher performs exact reads from this closure and never discovers objects through List, filename guesses, mutable latest endpoints, GitHub, registries, or runtime fallback URLs.

Treat the fixed OSS endpoint as deployment configuration, not application transport-security policy. Do not add custom host, scheme, certificate, Origin/Host, redirect, or source-allowlist enforcement. Apply size, digest, signature/provenance, cancellation, and safe-path checks to the downloaded artifact.

Before publication, fail closed on OSS publisher admission. Verify the named environment and required configuration without printing secret values, then use a disposable object under a dedicated project-scoped probe prefix to prove write permission and anonymous public read-back. Run this gate before any tag, release note, or Bootstrap change makes the version discoverable.

Use explicit, rerunnable publication stages:

1. Build the candidate once and freeze its version, bytes, size, digest, manifest, and object keys.
2. Upload every immutable Installer, manifest, component, and required third-party object from that frozen candidate; anonymously read each object back and verify its identity. Do not move Bootstrap.
3. Re-read the complete OSS closure. Only then commit the one mutable Bootstrap and confirm its bytes.

If upload or verification fails, retain already uploaded immutable objects for an idempotent retry and leave the old Bootstrap usable. Never rebuild, overwrite immutable objects, advance Bootstrap, or substitute GitHub or another origin to escape a partial publication.

### Bootstrap
Bootstrap is the only mutable pointer and should contain the minimum data an old launcher needs to locate a compatible installer and current release. At minimum, define:

- schema version and product/platform/architecture;
- installer version, exact source/object key, size, SHA-256, signature/provenance;
- release version, exact manifest source/object key, size, SHA-256, signature/provenance;
- minimum launcher/client compatibility when required;
- optional policy such as check interval or channel, only when an explicit project formal owner owns it.

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
2. After install finalization, start one launcher setup flow. Preserve one product registration, settings, and business state. Use same-version repair or higher-version in-place Setup when the selected platform path supports it; replacing installer-owned files through that platform path does not create a historical application-protocol compatibility promise.
3. Apply the no-historical-compatibility default before designing self-update behavior:
   - Only when the ProductContract explicitly promises in-place compatibility, or the official replacement path cannot preserve and re-establish required user-owned state, ensure a compatible Launcher and Manager before starting an older client. Implement and test only the bounded bridge or migration needed for that exception.
   - Otherwise treat the older Launcher protocol, schema, and private installer state as unsupported. Do not download, stage, execute, or activate an incompatible Installer from the running client. Report `setup-required`, direct the user to the newer official Setup, offer uninstall/reinstall only after normal Setup/upgrade fails, and leave the current installation and user data untouched. Repair an unreachable Setup or misplaced state at its owner; neither defect creates an implicit compatibility promise.
4. An externally run newer Setup may install the new Launcher. Before that Launcher starts an existing Manager, compare the Manager with the Launcher's compiled minimum version; when incompatible, enter the normal setup/bootstrap flow and prepare a compatible release. This is a startup admission check, not a historical compatibility bridge.
5. The launcher reads the selected release manifest, probes components, and reuses eligible system or private-cache candidates. A successful probe must be observable as reused/skipped and must not copy, upgrade, edit, or change global PATH for a system component.
6. Missing or insufficient components are fetched from the configured distribution endpoint. A required component that cannot be verified or doctored must prevent `ready`.

The launcher should expose progress with the component, phase, completed/total count, and real download bytes. Probe, hash, unpack, and doctor phases may show activity without inventing a static percentage. Errors must include the component, source key, failed phase, system message, and diagnostic location.

For a Windows takeover, list running product processes and ask the user to save work before requesting normal shutdown. Show the waiting state. After five seconds, terminate only a process whose executable path has been verified inside the product installation root; use a longer deadline only when source or a formal platform contract already defines one. Never terminate reused system components or unrelated processes.

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

Failure removes only temporary or failed staging data. It must not alter `current`, user data, any prior release retained by the recovery contract, or an already verified cache. Re-running may reuse a verified immutable cache object.

## 7. Client State And Activation
Use the smallest state machine that changes ownership, user action, or recovery:

`current -> checking -> up-to-date | setup-required`

`current -> checking -> available -> updating/downloading -> verifying -> staged -> waiting-for-drain -> restart-required/activating -> health-check -> ready`

Failure may occur from any state. Use one fixed Manager primary control: it sends a read-only `check` while idle/current and changes to the applicable explicit update intent only after a compatible version is available or staged. Disable it while loading, checking, or updating, and prevent a periodic response from unlocking a foreground request early.

Check automatically at Launcher startup and about every six hours while the Manager runs. Download, verify, and stage compatible application components automatically in the background, but never prepare an Installer target that requires external Setup. Create no Service or scheduled task after the client exits. Activation remains separately user-confirmed, and a pending update must remain visible rather than being mistaken for an activated version.

Before activation, revalidate the selected manifest, component digests, launcher compatibility, and active-work count. The Manager stops accepting new work and waits for the documented drain condition. Never force-close active sessions as a routine update mechanism.

Activate by atomic directory/pointer swap or platform-supported helper. Keep `current` until the candidate is ready, and never delete the only runnable release or user data. Resolve either automatic rollback or forward repair from the project's formal owner. Use automatic rollback only with tested binary, configuration, and persisted-data backward compatibility; retain `previous` while the new release is observed. Under forward repair, do not mark an incompatible prior release as a rollback target. A health failure executes the selected recovery contract and records release, phase, error, recovery mode, remaining runnable state, and result.

## 8. Product State Migration
Installer and Launcher must not read or migrate product registrations, workspace/project identity, settings, credential references, results, or audit data. Let the Manager or owning application apply only explicitly enumerated retired official identifier, derived registration ID, and state-path migrations. Make them atomic and idempotent, preserve unrelated state, and fail on unknown sources, corruption, or old/new conflicts. Repair and in-place upgrade preserve business state; uninstall must not modify reused programs or silently delete user-created data.

## 9. Installer And Launcher Evolution
- Build and publish an Installer only when installer-owned behavior, launcher/updater behavior, bootstrap compatibility, or installer-owned assets change.
- Ordinary client releases publish a new manifest and payloads and reuse the stable Installer reference.
- Installer objects are immutable and published once. A same-version retry must read back and prove identical size/digest; it must never overwrite.
- The bootstrap update is last. Before changing it, verify every referenced installer, manifest, component, digest, schema, and doctor result through the public read path.
- Keep OSS publisher admission, immutable upload/read-back, and Bootstrap commit as distinct workflow jobs or modes with explicit dependencies. Do not put the credential check after publication.
- If a new manifest schema or launcher contract is not understood by the current launcher, use the default replacement path: make the running client stop at `setup-required` until the user runs the newer official Setup. Publish a bridge only for an explicit in-place promise or required user-owned state that Setup/reinstall cannot preserve and re-establish.
- Default to no historical protocol, schema, rollback, or private-state migration. Do not infer compatibility from already published launchers, an existing update channel, or the absence of a separate declaration that old versions are unsupported.

### Build-graph gates
Treat a split Launcher/Manager application as a multi-entry build, not just a source-code split:

- Declare every frontend HTML entry explicitly and verify each built page exists; a dynamically created Launcher window pointing at an omitted entry will render blank after packaging.
- Build every native executable explicitly. Framework bundlers often compile only the configured main binary, so prove the Manager/helper executable exists before creating its archive.
- Create component archives from explicit normalized file-relative entries, never a dot/root input whose stored path semantics depend on the archive tool. Before freezing artifact identity, invoke the candidate client's own production parser/safe-unpack path against each final archive; a successful generic `tar`/`zip` extraction is insufficient evidence.
- Generate the install-time bootstrap seed before bundling the Installer. After the Installer is built, measure it and generate the public bootstrap with the final Installer reference; never let a placeholder bootstrap enter the package unnoticed.
- Inspect the platform installer's generated shortcut behavior. If desktop shortcut creation is optional by default, add the platform-supported post-install hook and verify the target is the stable Launcher.
- Run at least one build after deleting local build/release output while retaining only dependency caches. An incremental build cannot prove that every binary and embedded resource is produced by the declared graph.

Keep Installer and client versions independent in their canonical owners. A normal client version bump must not rewrite the Installer version. When reusing an Installer from an older release/tag, read its public size and digest from the distribution source rather than rebuilding it or trusting a stale local copy.
Keep the current Installer version and verified candidate identity in one project-local metadata owner. Initialize it on first build, default unspecified later changes to a patch bump, and reuse an already verified same-version candidate after interruption.

## 10. Black-Box Acceptance
Use an exact public asset, not a worktree binary:

1. Verify public OSS object metadata, size, SHA-256, platform, architecture, and signing/provenance.
2. Install on a clean supported machine or isolated profile; confirm launcher starts and creates the expected shortcut/registration.
3. Confirm the launcher can bootstrap a release using the deployment-configured public endpoint.
4. Exercise the Installer evolution path. For an explicit in-place compatibility promise, repeat the same Installer and then run a higher Installer without losing registration or user data. Otherwise prove the running client stops at `setup-required`, normal external Setup succeeds, fallback uninstall/reinstall remains available, the complete required state is restored, and no incompatible partial mutation occurs.
5. Test eligible, missing, and below-minimum system-component cases; assert reuse versus manifest-frozen official installation, elevation/reboot handling, and post-install re-probe.
6. Corrupt a staged asset or doctor result; assert `current` remains runnable and no bad release becomes ready.
7. Stage an update while work is active; assert waiting-for-drain and no forced interruption.
8. Confirm activation, post-start health, and the configured recovery mode. For automatic rollback, deliberately fail health and prove restoration; for forward repair, prove no incompatible prior release is selected and the repair state remains diagnosable.
9. Verify startup and six-hour checks, background staging without Service/scheduled-task persistence, the single dynamic control, Windows owned-process takeover, and repair/upgrade state preservation.
10. Make the OSS endpoint unreachable and assert a diagnosable source failure, no fallback to GitHub/registry/another origin, and no change to `current` or user data.
11. Record exact environment, installed/current/staged versions, any contract-owned previous version, recovery result, process/window state, shortcuts, component source and outcome, and unsupported cases as Issues.

This is evidence for a separately requested SystemTest or release acceptance; building an installer alone does not authorize publication or deployment.
