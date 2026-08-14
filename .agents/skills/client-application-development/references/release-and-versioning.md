# Release And Versioning Reference

## Resolution rule

Use one canonical version source. A release request resolves in this order:

1. Read the current stable version from the project's authoritative file or release metadata.
2. Infer the bump from explicit intent: patch for fixes, minor for compatible features, major for an intentional breaking contract.
3. If no impact is stated, use the project's default, normally patch.
4. Calculate the next version and show it before publication.
5. Synchronize verified consumers, build, verify, tag, and publish.

The normal user interface is `发布` or a similarly clear release request. Never require `发布 vX.Y.Z` as a ceremony. An explicitly supplied version is an optional constraint that must be validated for monotonicity, uniqueness, and project policy.

## Candidate contract

Each candidate should have:

- stable version and commit identity;
- target platform and architecture;
- artifact name, URL/object key, byte size, SHA-256, and signature/provenance;
- release manifest schema and minimum compatible launcher/client version;
- build and verification evidence;
- immutable publication identity.

Do not overwrite an existing tag or asset. Do not publish a pointer before all referenced immutable assets are readable and verified. A release workflow may be triggered by a version tag, a protected manual dispatch, or an approved API action, but the trigger must not bypass deployment authorization.

## Fixed Aliyun OSS Publication

Use the `https` scheme with the fixed OSS host `shared-public-assets.oss-cn-beijing.aliyuncs.com`; publish workspace-owned Installer, manifest, payload, checksum, and third-party fallback objects only below `<project-prefix>/`. Give every immutable artifact one versioned object key, size, SHA-256, platform, architecture, and signing/provenance identity, then read it back anonymously before exposing it.

GitHub may hold source, tags, release notes, and optional automation. Do not attach release binaries or preserve duplicate binary assets there. Release-note download links point to the immutable public OSS objects. A GitHub Workflow may invoke the same local publisher but must not become the sole release owner.

OSS is the sole client-visible binary origin for first install and launcher-managed updates. Package-manager/store/framework-managed components continue to use their established official owner; do not duplicate that owner's update implementation merely because an Installer exists.

## Stable Installer Pattern

Calculate the next Installer version from stored metadata, build only when installer behavior changed, verify or reuse an existing immutable installer, publish it once, then update release metadata. Ordinary client releases build payloads and manifests without rebuilding the stable Installer. Apply this pattern to the project's own installer or store and keep version calculation independent from artifact upload.

For a thin installer, publish the complete immutable closure first: the installer or launcher asset, manifest, every required payload, and any missing third-party object. Read each object back through the same public path and compare size, SHA-256, schema, platform, architecture, and compatibility. Update the mutable bootstrap/index only as the final operation. A failed pre-commit upload must leave the old bootstrap usable; an uncertain post-commit read must be handled by re-reading, never by blind overwrite.

Do not add a mirror, fallback origin, or second permanent binary store. An OSS outage blocks publication, first install, or update without authorizing GitHub or registry fallback. The fixed endpoint remains deployment configuration; application code must not add custom host, scheme, certificate, Origin/Host, redirect, or source-allowlist enforcement. A Bootstrap compatibility change is an Installer/Launcher release, even when the visible Manager change is small.

Make OSS publisher admission an entry gate, not a late upload error. Before a tag, release note, or Bootstrap exposes the candidate, verify the named publishing environment, required configuration, project-prefix write access, and anonymous read-back without logging secret values. Then run a resumable `build once -> upload immutable OSS objects -> public read-back -> commit Bootstrap` transaction. A retry reuses the frozen candidate and verified objects; it never rebuilds a same-version candidate or advances Bootstrap past an incomplete closure.
