# Client Lifecycle Reference

Use this state model as a compatibility checklist, then map it to the project's actual state store:

`current -> checking -> up-to-date | available -> updating/downloading -> verifying -> staged -> waiting-for-drain -> restart-required/activating -> health-check -> ready`

Failure may occur in any state. Preserve `current` until pre-activation validation succeeds. A pending candidate may be discarded without affecting the running release. After activation, follow the project-owned automatic-rollback or forward-repair contract; retain `previous` only when that contract makes it a compatible recovery target.

| Responsibility | Owner | Required property |
|---|---|---|
| First install, prerequisites, repair, uninstall | Installer/store/package manager | Explicit platform contract and reversible failure behavior |
| Update discovery and compatibility | Launcher/updater/client shell | Manifest-driven, bounded, observable |
| Download and staging | Updater | Temporary files, size/digest/signature checks, atomic move |
| Session protection | Running client/session manager | No forced interruption; explicit drain condition |
| Activation | Launcher/store/platform updater | User confirmation unless contract allows no-session activation |
| Health and recovery | Launcher/updater | Explicit automatic-rollback or forward-repair contract with diagnosable results |
| Product-state migration | Manager/owning application | Explicit idempotent mappings; repair/upgrade preserves unrelated user state |
| Release publication | CI/release workflow | Immutable artifacts and final pointer/index update |

Do not add a state merely to represent a command. Add one only when it changes ownership, user action, safety, or recovery behavior.

For large desktop payloads, map the state machine across three owners: the Installer bootstraps the Launcher, the Launcher owns manifest-driven component preparation, activation, and project-owned recovery, and the running Manager owns intent, visibility, active-work drain, and explicit product-state migration. Check at Launcher startup and about every six hours while the Manager runs; download and stage in the background, but create no Service or scheduled task after exit. Activation remains behind an explicit user action. A launcher upgrade is a separate bootstrap path and must be completed before consuming a manifest it cannot understand.

Default a Manager-style desktop UI to one fixed dynamic primary control with two explicit intents: `Check for updates` is read-only, then the applicable update action becomes valid only for a compatible available or staged target. Disable it while loading, checking, or updating, and prevent periodic responses from unlocking foreground work early. The existing UI remains the only visual owner; official package-manager or updater processes run through a backend/launcher with hidden-console flags and bounded progress. Add no renderer shell execution, transient terminal, or duplicate update frontend.
