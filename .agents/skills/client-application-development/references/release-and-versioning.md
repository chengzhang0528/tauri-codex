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

## GitHub Release And Installer Mirrors

When GitHub Release is the canonical first-install record, use one versioned tag and create the Release as a draft. Attach the complete frozen platform matrix with explicit OS/architecture names, one checksum manifest, signatures/provenance, and concise install/upgrade notes. Read the draft and every asset back through the API before making it public.

If first-install network reachability requires OSS or another mirror, upload the exact same final installer bytes under an immutable versioned key and compare size plus SHA-256 after public read-back. Never rebuild per channel or let a mirror become a different candidate. Publish the GitHub draft and commit mutable install indexes only after every promised channel and platform asset is readable.

These are first-install acquisition channels unless the product explicitly assigns update ownership to them. Subsequent in-app updates should use the official package manager, store, desktop framework updater, or established launcher that owns the installed component. Do not add a custom GitHub/OSS downloader merely because the installer is published there.

## Stable Installer Pattern

Calculate the next Installer version from stored metadata, build only when installer behavior changed, verify or reuse an existing immutable installer, publish it once, then update release metadata. Ordinary client releases build payloads and manifests without rebuilding the stable Installer. Apply this pattern to the project's own installer or store and keep version calculation independent from artifact upload.

For a thin installer, publish the complete immutable closure first: the installer or launcher asset, manifest, every required payload, and any missing third-party object. Read each object back through the same public path and compare size, SHA-256, schema, platform, architecture, and compatibility. Update the mutable bootstrap/index only as the final operation. A failed pre-commit upload must leave the old bootstrap usable; an uncertain post-commit read must be handled by re-reading, never by blind overwrite.

When the product needs more than one public origin, keep a single candidate identity and publish the same bytes to every origin. The release transaction is complete only after every required immutable object is readable from every promised origin; commit each origin's mutable pointer last. Distribution endpoints and transport policy belong to deployment or the platform owner, not application code. A Bootstrap compatibility change is an Installer/Launcher release, even when the visible Manager change is small.

Make publisher admission an entry gate, not a late upload error. Before a tag, public Release, store submission, or channel pointer exposes the candidate, verify the named publishing environment, the presence of required secret/configuration keys, and project-scoped write plus anonymous read-back access without logging secret values. Then run a resumable `build once -> stage secondary immutable objects -> publish primary -> verify all origins -> commit mutable pointers` transaction. A retry reuses the frozen candidate and already verified immutable objects; it never rebuilds a same-version candidate or advances a pointer past an incomplete origin.
