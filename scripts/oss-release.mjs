import { createHash, createHmac, randomUUID } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

export const OSS_BASE_URL = "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";
export const OSS_BUCKET = "shared-public-assets";
export const OSS_BOOTSTRAP_KEY = "bootstrap/windows-x64.json";

const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const digestPattern = /^[a-f0-9]{64}$/;

export function safeObjectKey(key) {
  return typeof key === "string" && key.length > 0 && !key.startsWith("/") && !key.includes("\\") &&
    key.split("/").every((part) => part.length > 0 && part !== "." && part !== ".." && /^[A-Za-z0-9._@-]+$/.test(part));
}

export function compareVersions(left, right) {
  if (!stableVersion.test(left) || !stableVersion.test(right)) throw new Error("stable three-part versions are required");
  const a = left.split(".").map(Number);
  const b = right.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) {
    if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  }
  return 0;
}

export function ossAuthorization({ method, contentType = "", date, key, secret, accessKeyId, immutable = false, basePath = "/project-tauri-codex", bucket = OSS_BUCKET }) {
  if (!safeObjectKey(key)) throw new Error(`unsafe OSS object key: ${key}`);
  const canonicalHeaders = immutable ? "x-oss-forbid-overwrite:true\n" : "";
  const canonicalResource = `/${bucket}${basePath}/${key}`;
  const input = [method, "", contentType, date].join("\n") + `\n${canonicalHeaders}${canonicalResource}`;
  const signature = createHmac("sha1", secret).update(input).digest("base64");
  return `OSS ${accessKeyId}:${signature}`;
}

function sha256(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function validateArtifact(artifact, expectedPrefix) {
  if (!artifact || !Number.isSafeInteger(artifact.size) || artifact.size <= 0 || !digestPattern.test(artifact.sha256) ||
      !safeObjectKey(artifact.objectKey) || !artifact.objectKey.startsWith(expectedPrefix)) {
    throw new Error(`invalid release artifact under ${expectedPrefix}`);
  }
  const source = new URL(artifact.url);
  if (source.protocol !== "https:" || source.hostname !== "github.com" ||
      !source.pathname.startsWith("/chengzhang0528/tauri-codex/releases/download/") || source.search || source.hash ||
      path.posix.basename(source.pathname) !== path.posix.basename(artifact.objectKey)) {
    throw new Error(`untrusted GitHub release URL: ${artifact.url}`);
  }
}

async function readResponse(response, label) {
  if (!response.ok) throw new Error(`${label} returned HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

async function readVerified(fetchImpl, url, artifact, label) {
  const bytes = await readResponse(await fetchImpl(url, {
    headers: { "User-Agent": "tauri-codex-release-publisher", Accept: "application/octet-stream" },
    redirect: "follow",
  }), label);
  const digest = sha256(bytes);
  if (bytes.length !== artifact.size || digest !== artifact.sha256) {
    throw new Error(`${label} identity mismatch: ${bytes.length}/${digest}`);
  }
  return bytes;
}

async function readExact(fetchImpl, url, expected, label) {
  const bytes = await readResponse(await fetchImpl(url, {
    headers: { "User-Agent": "tauri-codex-release-publisher", Accept: "application/octet-stream" },
    redirect: "follow",
  }), label);
  if (!bytes.equals(expected)) throw new Error(`${label} differs from the frozen release candidate`);
}

async function getOSS(fetchImpl, baseURL, key) {
  const response = await fetchImpl(`${baseURL}/${key}`, {
    headers: { "User-Agent": "tauri-codex-release-publisher" },
    redirect: "error",
  });
  if (response.status === 404) return null;
  return readResponse(response, `OSS ${key}`);
}

async function putOSS(fetchImpl, { baseURL, bucket, accessKeyId, accessKeySecret }, key, bytes, contentType, immutable) {
  const base = new URL(baseURL);
  const date = new Date().toUTCString();
  const headers = {
    "Content-Type": contentType,
    Date: date,
    Authorization: ossAuthorization({
      method: "PUT", contentType, date, key, secret: accessKeySecret, accessKeyId, immutable,
      basePath: base.pathname.replace(/\/$/, ""), bucket,
    }),
  };
  if (immutable) headers["x-oss-forbid-overwrite"] = "true";
  const response = await fetchImpl(`${baseURL}/${key}`, { method: "PUT", headers, body: bytes, redirect: "error" });
  if (!(response.ok || (immutable && response.status === 409))) {
    throw new Error(`OSS PUT ${key} returned HTTP ${response.status}`);
  }
}

async function deleteOSS(fetchImpl, { baseURL, bucket, accessKeyId, accessKeySecret }, key) {
  const base = new URL(baseURL);
  const date = new Date().toUTCString();
  const response = await fetchImpl(`${baseURL}/${key}`, {
    method: "DELETE",
    headers: {
      Date: date,
      Authorization: ossAuthorization({
        method: "DELETE", date, key, secret: accessKeySecret, accessKeyId,
        basePath: base.pathname.replace(/\/$/, ""), bucket,
      }),
    },
    redirect: "error",
  });
  if (!(response.ok || response.status === 204 || response.status === 404)) {
    throw new Error(`OSS DELETE ${key} returned HTTP ${response.status}`);
  }
}

function contentType(key) {
  return key.endsWith(".json") ? "application/json" : "application/octet-stream";
}

async function publishImmutable(fetchImpl, settings, artifact, localBytes, label) {
  const bytes = localBytes ?? await readVerified(fetchImpl, artifact.url, artifact, `GitHub ${label}`);
  if (bytes.length !== artifact.size || sha256(bytes) !== artifact.sha256) {
    throw new Error(`local ${label} identity differs from the release manifest`);
  }
  await putOSS(fetchImpl, settings, artifact.objectKey, bytes, contentType(artifact.objectKey), true);
  const confirmed = await getOSS(fetchImpl, settings.baseURL, artifact.objectKey);
  if (!confirmed || confirmed.length !== artifact.size || sha256(confirmed) !== artifact.sha256) {
    throw new Error(`OSS ${label} readback differs from the release manifest`);
  }
}

function readRelease(releaseRoot) {
  const bootstrapPath = path.join(releaseRoot, "bootstrap.json");
  const manifestPath = path.join(releaseRoot, "manifest.json");
  const bootstrapBytes = readFileSync(bootstrapPath);
  const manifestBytes = readFileSync(manifestPath);
  const bootstrap = JSON.parse(bootstrapBytes);
  const manifest = JSON.parse(manifestBytes);
  if (bootstrap.schemaVersion !== 1 || bootstrap.product !== "tauri-codex" || bootstrap.platform !== "windows" ||
      bootstrap.architecture !== "x86_64" || !stableVersion.test(bootstrap.release?.version) ||
      !stableVersion.test(bootstrap.installer?.version) || manifest.version !== bootstrap.release.version) {
    throw new Error("release Bootstrap or manifest identity is invalid");
  }
  validateArtifact(bootstrap.installer.artifact, `installers/${bootstrap.installer.version}/windows-x64/`);
  validateArtifact(bootstrap.release.manifest, `releases/${manifest.version}/windows-x64/`);
  if (bootstrap.release.manifest.objectKey !== `releases/${manifest.version}/windows-x64/manifest.json`) {
    throw new Error("release manifest object key is not canonical");
  }
  for (const component of manifest.components ?? []) {
    validateArtifact(component.artifact, `releases/${manifest.version}/windows-x64/components/`);
  }
  const installerName = path.posix.basename(bootstrap.installer.artifact.objectKey);
  const installerPath = path.join(releaseRoot, installerName);
  const components = manifest.components.map((component) => ({
    component,
    bytes: readFileSync(path.join(releaseRoot, "components", path.posix.basename(component.artifact.objectKey))),
  }));
  return {
    bootstrap, bootstrapBytes, manifest, manifestBytes,
    installerBytes: existsSync(installerPath) ? readFileSync(installerPath) : null, components,
  };
}

function publisherSettings({ accessKeyId, accessKeySecret, baseURL, bucket }) {
  if (!accessKeyId || !accessKeySecret) throw new Error("ALIYUN_OSS_ACCESS_KEY_ID and ALIYUN_OSS_ACCESS_KEY_SECRET are required");
  return { baseURL, bucket, accessKeyId, accessKeySecret };
}

async function rejectDowngrade(fetchImpl, baseURL, release) {
  const currentBytes = await getOSS(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (!currentBytes) return;
  const current = JSON.parse(currentBytes);
  if (compareVersions(release.manifest.version, current.release?.version) < 0 ||
      compareVersions(release.bootstrap.installer.version, current.installer?.version) < 0) {
    throw new Error("OSS Bootstrap downgrade is disabled");
  }
}

export async function preflightPublisher({ accessKeyId, accessKeySecret, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET, probeId = randomUUID() }) {
  const settings = publisherSettings({ accessKeyId, accessKeySecret, baseURL, bucket });
  if (!/^[A-Za-z0-9._-]+$/.test(probeId)) throw new Error("unsafe OSS publisher probe id");
  const key = `probes/tauri-codex-${probeId}.txt`;
  const bytes = Buffer.from(`tauri-codex publisher probe ${probeId}\n`);
  try {
    await putOSS(fetchImpl, settings, key, bytes, "text/plain", true);
    const confirmed = await getOSS(fetchImpl, baseURL, key);
    if (!confirmed || !confirmed.equals(bytes)) throw new Error("OSS publisher probe public readback differs");
  } finally {
    await deleteOSS(fetchImpl, settings, key);
  }
  if (await getOSS(fetchImpl, baseURL, key)) throw new Error("OSS publisher probe cleanup was not observable");
  return { admitted: true, bucket, baseURL };
}

export async function stageRelease({ releaseRoot, accessKeyId, accessKeySecret, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const settings = publisherSettings({ accessKeyId, accessKeySecret, baseURL, bucket });
  const release = readRelease(releaseRoot);
  await rejectDowngrade(fetchImpl, baseURL, release);
  await publishImmutable(fetchImpl, settings, release.bootstrap.installer.artifact, release.installerBytes, "Installer");
  for (const { component, bytes } of release.components) {
    await publishImmutable(fetchImpl, settings, component.artifact, bytes, component.id);
  }
  await publishImmutable(fetchImpl, settings, release.bootstrap.release.manifest, release.manifestBytes, "manifest");
  return { staged: true, release: release.manifest.version, installerVersion: release.bootstrap.installer.version };
}

export async function commitRelease({ releaseRoot, accessKeyId, accessKeySecret, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const settings = publisherSettings({ accessKeyId, accessKeySecret, baseURL, bucket });
  const release = readRelease(releaseRoot);
  await rejectDowngrade(fetchImpl, baseURL, release);
  const artifacts = [
    [release.bootstrap.installer.artifact, "Installer"],
    ...release.components.map(({ component }) => [component.artifact, component.id]),
    [release.bootstrap.release.manifest, "manifest"],
  ];
  for (const [artifact, label] of artifacts) {
    await readVerified(fetchImpl, artifact.url, artifact, `GitHub ${label}`);
    const mirrored = await getOSS(fetchImpl, baseURL, artifact.objectKey);
    if (!mirrored || mirrored.length !== artifact.size || sha256(mirrored) !== artifact.sha256) {
      throw new Error(`OSS ${label} is missing or differs before Bootstrap commit`);
    }
  }
  await readExact(fetchImpl,
    `https://github.com/chengzhang0528/tauri-codex/releases/download/v${release.manifest.version}/bootstrap.json`,
    release.bootstrapBytes, "GitHub Bootstrap");
  await putOSS(fetchImpl, settings, OSS_BOOTSTRAP_KEY, release.bootstrapBytes, "application/json", false);
  const committed = await getOSS(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (!committed || !committed.equals(release.bootstrapBytes)) {
    throw new Error("OSS Bootstrap commit confirmation differs from local bytes");
  }
  return { published: true, release: release.manifest.version, installerVersion: release.bootstrap.installer.version, bootstrapKey: OSS_BOOTSTRAP_KEY };
}

function parseReleaseManifest(bytes, version) {
  const manifest = JSON.parse(bytes);
  if (manifest.schemaVersion !== 1 || manifest.product !== "tauri-codex" || manifest.version !== version ||
      manifest.platform !== "windows" || manifest.architecture !== "x86_64" || !Array.isArray(manifest.components)) {
    throw new Error(`release ${version} manifest identity is invalid`);
  }
  for (const component of manifest.components) {
    validateArtifact(component.artifact, `releases/${version}/windows-x64/components/`);
  }
  return manifest;
}

async function confirmDeleted(fetchImpl, baseURL, key) {
  if (await getOSS(fetchImpl, baseURL, key)) {
    throw new Error(`OSS retirement did not remove ${key}`);
  }
}

export async function retireRelease({
  oldVersion,
  oldInstallerVersion,
  replacementVersion,
  accessKeyId,
  accessKeySecret,
  fetchImpl = fetch,
  baseURL = OSS_BASE_URL,
  bucket = OSS_BUCKET,
}) {
  if (![oldVersion, oldInstallerVersion, replacementVersion].every((version) => stableVersion.test(version ?? "")) ||
      compareVersions(replacementVersion, oldVersion) <= 0) {
    throw new Error("retirement requires stable old/installer/replacement versions and a newer replacement");
  }
  const settings = publisherSettings({ accessKeyId, accessKeySecret, baseURL, bucket });
  const currentBytes = await getOSS(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (!currentBytes) throw new Error("OSS Bootstrap is missing; refusing retirement");
  const current = JSON.parse(currentBytes);
  if (current.release?.version !== replacementVersion || compareVersions(current.installer?.version, oldInstallerVersion) < 0) {
    throw new Error("OSS Bootstrap has not activated the requested replacement release and installer");
  }
  await readExact(
    fetchImpl,
    `https://github.com/chengzhang0528/tauri-codex/releases/download/v${replacementVersion}/bootstrap.json`,
    currentBytes,
    "GitHub replacement Bootstrap",
  );

  const manifestKey = `releases/${oldVersion}/windows-x64/manifest.json`;
  const manifestBytes = await getOSS(fetchImpl, baseURL, manifestKey);
  if (!manifestBytes) {
    return { retired: true, alreadyRetired: true, oldVersion, replacementVersion, deletedKeys: [] };
  }
  const manifest = parseReleaseManifest(manifestBytes, oldVersion);
  const oldBootstrapBytes = await readResponse(await fetchImpl(
    `https://github.com/chengzhang0528/tauri-codex/releases/download/v${oldVersion}/bootstrap.json`,
    { headers: { "User-Agent": "tauri-codex-release-publisher" }, redirect: "follow" },
  ), "GitHub old Bootstrap");
  const oldBootstrap = JSON.parse(oldBootstrapBytes);
  const installerKey = `installers/${oldInstallerVersion}/windows-x64/tauri-codex_${oldInstallerVersion}_x64-setup.exe`;
  if (oldBootstrap.release?.version !== oldVersion || oldBootstrap.installer?.version !== oldInstallerVersion ||
      oldBootstrap.release?.manifest?.objectKey !== manifestKey || oldBootstrap.installer?.artifact?.objectKey !== installerKey) {
    throw new Error("old GitHub Bootstrap does not match the requested retirement identity");
  }

  const protectedKeys = new Set([
    current.release?.manifest?.objectKey,
    current.installer?.artifact?.objectKey,
  ]);
  const componentKeys = manifest.components.map((component) => component.artifact.objectKey);
  const deleteKeys = [...componentKeys];
  if (!protectedKeys.has(installerKey)) deleteKeys.push(installerKey);
  if (deleteKeys.some((key) => protectedKeys.has(key)) || protectedKeys.has(manifestKey)) {
    throw new Error("retirement attempted to delete an object referenced by the current Bootstrap");
  }

  for (const key of deleteKeys) {
    await deleteOSS(fetchImpl, settings, key);
    await confirmDeleted(fetchImpl, baseURL, key);
  }
  await deleteOSS(fetchImpl, settings, manifestKey);
  await confirmDeleted(fetchImpl, baseURL, manifestKey);
  return {
    retired: true,
    alreadyRetired: false,
    oldVersion,
    replacementVersion,
    deletedKeys: [...deleteKeys, manifestKey],
  };
}

export async function publishRelease(options) {
  await stageRelease(options);
  return commitRelease(options);
}

async function main() {
  const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
  const workspaceRoot = path.resolve(scriptRoot, "..");
  const [mode, version, installerVersion, replacementVersion] = process.argv.slice(2);
  if (!new Set(["preflight", "stage", "commit", "retire"]).has(mode) || !stableVersion.test(version ?? "")) {
    throw new Error("usage: node scripts/oss-release.mjs <preflight|stage|commit> <version> or retire <old-version> <old-installer-version> <replacement-version>");
  }
  const options = {
    releaseRoot: path.join(workspaceRoot, ".codex-build", "releases", version, "windows-x64"),
    accessKeyId: process.env.ALIYUN_OSS_ACCESS_KEY_ID,
    accessKeySecret: process.env.ALIYUN_OSS_ACCESS_KEY_SECRET,
    probeId: `${process.env.GITHUB_RUN_ID ?? "local"}-${process.env.GITHUB_RUN_ATTEMPT ?? randomUUID()}`,
  };
  const result = mode === "preflight" ? await preflightPublisher(options) :
    mode === "stage" ? await stageRelease(options) :
    mode === "commit" ? await commitRelease(options) : await retireRelease({
      ...options,
      oldVersion: version,
      oldInstallerVersion: installerVersion,
      replacementVersion,
    });
  console.log(JSON.stringify(result, null, 2));
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
