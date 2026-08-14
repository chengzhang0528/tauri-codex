# Client Lifecycle Reference

Use this state model as a compatibility checklist, then map it to the project's actual state store:

`current -> checking -> up-to-date | available -> updating/downloading -> verifying -> staged -> waiting-for-drain -> restart-required/activating -> health-check -> ready`

Failure may occur in any state. Preserve `current` until activation and health checks succeed. Keep `previous` while the new release is under observation. A pending candidate may be discarded without affecting the running release.

| Responsibility | Owner | Required property |
|---|---|---|
| First install, prerequisites, repair, uninstall | Installer/store/package manager | Explicit platform contract and reversible failure behavior |
| Update discovery and compatibility | Launcher/updater/client shell | Manifest-driven, bounded, observable |
| Download and staging | Updater | Temporary files, size/digest/signature checks, atomic move |
| Session protection | Running client/session manager | No forced interruption; explicit drain condition |
| Activation | Launcher/store/platform updater | User confirmation unless contract allows no-session activation |
| Health and rollback | Launcher/updater | Previous release retained and diagnosable |
| Release publication | CI/release workflow | Immutable artifacts and final pointer/index update |

Do not add a state merely to represent a command. Add one only when it changes ownership, user action, safety, or recovery behavior.

For large desktop payloads, map the state machine across three owners: the Installer bootstraps the Launcher, the Launcher owns manifest-driven component preparation and activation, and the running Manager owns intent, visibility, and active-work drain. Automatic checks/downloads are disabled by default and require an explicit ProductContract override; activation remains behind an explicit user action. A launcher upgrade is a separate bootstrap path and must be completed before consuming a manifest it cannot understand.

Default UI interaction is two-step: `Check for updates` is read-only, then `Update now` becomes available only for a compatible target. The existing UI remains the only visual owner; official package-manager or updater processes run through a backend/launcher with hidden-console flags and bounded progress. Add no renderer shell execution, transient terminal, or duplicate update frontend.
