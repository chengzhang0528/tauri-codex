# Technology And Update Channels

Use this reference when selecting a client stack, building an installer, publishing fixed Aliyun OSS assets, or designing in-app updates. Resolve the actual project contract first, then apply the fixed workspace delivery defaults for every workspace-owned client binary.

## Select The Stack

Prefer the current repository stack and its official packaging/update path. Compare alternatives only on requirements that change the result:

| Product shape | Prefer | Require before choosing |
|---|---|---|
| CLI or library installed by a language ecosystem | Native registry and package manager | A published package, one canonical package/version owner, supported global/tool installation, and official upgrade semantics |
| Desktop web UI with existing Electron/Tauri/Wails/native shell | Existing framework packager and updater | Supported OS/architecture output, signing, hidden helper execution, restart/activation behavior, and post-activation recovery evidence |
| Native desktop client | Existing OS-native installer/toolchain | Real OS integration, prerequisites, service/helper, repair/uninstall, or offline requirements |
| Store-managed client | Platform store | Store rules, signing, staged rollout, update ownership, and recovery limitations |
| Large multi-component client | Stable thin launcher plus manifest components | Proven need for independent payload reuse, first-run network behavior, compatibility bridge, and recovery |

Do not introduce another runtime, UI shell, installer framework, updater service, or release stream for familiarity alone. Record build size, installed size, prerequisites, signing support, cross-platform cost, update ownership, process behavior, and long-term maintenance for a material choice.

Use the project's established builder first. Common candidates to verify against current source and official documentation include Electron Forge makers or electron-builder for Electron, the built-in bundle/update facilities for Tauri, Wails' build output plus the project's platform installer, MSIX/WiX for applicable Windows-native or .NET clients, Xcode packaging/notarization for macOS, and the platform store toolchain when the store owns installation. These names are candidates, not defaults; never add two packagers for the same artifact.

## Build The Installer

1. Freeze one source commit and canonical version before building.
2. Build every declared frontend/native/helper entry from a clean output directory.
3. Generate platform-specific packages with the established framework or installer tool. Do not wrap a native package manager solely to claim an installer exists.
4. Include only installer-owned bootstrap, shortcuts, registration, uninstall/repair data, licenses, and required prerequisites. Keep replaceable application payloads separate when thin delivery is selected.
5. Sign/notarize where the product contract requires it. Calculate size and SHA-256 after final signing because signing changes bytes.
6. Install the exact artifact on a clean supported environment; launch the installed binary and verify registration, shortcut, repair/repeat install, uninstall boundary, first-run bootstrap, and no transient terminal or second frontend window.

On Windows, use a current-user MSI by default when the product owns no Service, driver, machine-wide shared resource, or privileged prerequisite. Escalate to machine-wide installation only for a verified platform requirement. After `InstallFinalize`, run one Launcher setup; request normal shutdown for old product processes, wait five seconds, and terminate only executables verified inside the installation root. Preserve registration, settings, and business state across repair and in-place upgrade.

## Publish Fixed Aliyun OSS Assets

Use the `https` scheme with the fixed OSS host `shared-public-assets.oss-cn-beijing.aliyuncs.com`; publish every workspace-owned Installer, Bootstrap, manifest, component payload, checksum, and third-party fallback below `<project-prefix>/`. GitHub may hold source, tags, release notes, and optional automation, but it must not store release binaries.

Use one frozen candidate and one OSS publication transaction:

1. Verify the named OSS publishing environment, required configuration, project-prefix write access, and anonymous read-back with a disposable project-scoped probe without logging secret values.
2. Freeze version, bytes, object keys, size, SHA-256, platform, architecture, and signature/provenance.
3. Upload immutable versioned objects and read each back anonymously.
4. Commit the one mutable Bootstrap only after the complete closure is readable.
5. On failure, retain immutable objects for retry and keep the old Bootstrap; never rebuild the same version or substitute another origin.

Release-note download links point to OSS. A GitHub Workflow may invoke the same local publisher but cannot become its sole owner. Do not create an OSS mirror, GitHub fallback, or second permanent binary origin.

If first run would otherwise fetch an application-managed capability from npm or another registry, publish the exact package/archive to OSS and let the official package manager install it from that local immutable input when supported. Do not silently replace global registry configuration or claim offline first run when network dependencies remain. Independently package-managed or store-owned components keep their official update owner.

## Select The Update Owner

Match updates to the actual installation source and official ecosystem mechanism:

| Installed through | Preferred later update owner | Typical adapter |
|---|---|---|
| npm/pnpm/yarn global or tool package | Same package manager and configured registry | Resolve installed/latest package versions, then run the package manager's official update/install command |
| pipx/uv tool | Same Python tool manager/index | Use its official upgrade command/API |
| Cargo install | Cargo or an established project-approved updater | Use the supported crate/update path; do not assume self-update exists |
| .NET global tool | `dotnet tool update` | Preserve source and tool-path rules |
| Homebrew/Winget/Chocolatey/store | That system/store owner | Deep-link or invoke only the supported noninteractive update path |
| Electron/Tauri/Wails/native packaged app | Existing framework/platform updater or stable launcher | Use its signed feed, staging, activation, restart, and project-owned recovery contract |
| Thin multi-component client | Existing launcher/updater | Manifest-driven component preparation and atomic activation |

`npm update` is an example, not a universal rule. For a global CLI, the correct operation may be `npm install -g <package>@latest`; for a locally pinned dependency, changing the project lockfile may be a Development operation rather than an end-user client update. Verify the official command and its version semantics from the project's package manager and current install layout.

Prefer a non-mutating official metadata/API check, then update the exact version the user accepted so the target cannot change between check and install. For npm this may mean resolving the registry version first and installing `<package>@<resolved-version>` in the same global or private prefix that owns the current package. Use `npm update` only when its documented scope, dependency range, prefix, and target semantics match the installed component.

Do not add a custom release-asset downloader when the official package manager, store, framework updater, or launcher already owns update discovery and installation. If first install came from an installer but runtime components are officially package-managed, record that split explicitly and keep one owner per component.

Keep application, installer/launcher, and managed capability versions separate when they release independently. Each has one canonical version owner and compatibility relation; a normal capability update must not rewrite the installer or application version unless their owned behavior or compatibility bridge changes.

## Design The User Interaction

Default to a deliberate two-step interaction:

1. `Check for updates` performs a bounded read-only check. Show current version and one result: up to date, update available with target version, or failed with retry. It must not install, restart, or alter active work.
2. For a Manager-style desktop UI, reuse the same fixed primary control for the applicable explicit update intent only when an update is available or staged. Before starting, show download/restart requirements and whether active work must finish. Disable the control while loading, checking, or updating; periodic responses must not unlock foreground work early. Allow defer and cancellation until the updater's safe commit boundary.

Represent only states that change user action or recovery:

`idle -> checking -> up-to-date | available -> updating -> verifying/staged -> waiting-for-drain -> restart-required/activating -> ready | failed`

Keep the existing window as the single visual owner. The renderer sends intent to a backend/launcher/updater and receives bounded progress; it does not run shell commands directly. On Windows, create the updater/package-manager process with console-window suppression and a hidden window style as required by the runtime. A `.cmd`, `.bat`, or PowerShell shim must not flash a terminal. Do not create a new frontend process or window merely to display update progress.

Use the current backend runtime's native hidden-process controls and test the packaged build:

| Runtime owner | Windows requirement |
|---|---|
| Node/Electron main process | Spawn outside the renderer with `windowsHide: true`; avoid a shell when possible, and when a `.cmd`/`.bat` shim requires `cmd.exe`, keep the shell hidden and pass arguments structurally |
| Go backend | Set `SysProcAttr.HideWindow` or the appropriate no-window creation flag in Windows-only code while retaining process-tree cancellation |
| .NET backend | Use `UseShellExecute = false` and `CreateNoWindow = true`; keep status/progress in the existing UI |
| Rust/Tauri backend | Use Windows process creation flags or the framework's sidecar API from the Rust/backend side, never renderer shell execution |

Do not blindly copy a flag across runtimes. Read the actual process API and adjacent tests, preserve argument boundaries, cancellation, exit code, bounded stdout/stderr, and child-process cleanup.

Do not interrupt sessions, jobs, terminals, uploads, or unsaved work. Download/stage while the app remains usable when the owner supports it. Drain new work only before activation, then ask for restart confirmation. If live activation is safe and officially supported, preserve UI state and report completion without inventing a restart.

For launcher-managed desktop clients, check at Launcher startup and about every six hours while the Manager runs. Download, verify, and stage automatically, but create no Service or scheduled task after exit and never activate without the required user confirmation.

## Verify The User Contract

- Repeated check clicks coalesce or disable correctly; a stale response cannot overwrite a newer one.
- Check-only never mutates installed state.
- Update cannot start without an available compatible target.
- Progress, cancellation, failure, retry, waiting-for-drain, restart, health, and the selected recovery result are visible in the existing UI.
- Background execution produces no `cmd`, PowerShell, terminal, installer shell, or duplicate frontend flash.
- Active work and user data survive failure, defer, restart, and the selected recovery behavior.
- Installed version after success matches the official registry/feed/store and the UI's reported version.
- Installer, manifest, payload, and fallback objects are anonymously readable from their exact OSS keys; GitHub has no release binary assets.
- An OSS outage produces a diagnosable source failure without fallback and leaves the current version and user data intact.
