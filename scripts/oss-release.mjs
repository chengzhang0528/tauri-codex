import { createHash, createHmac, createPublicKey, randomUUID, verify } from "node:crypto";
import { spawnSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { canonicalJson } from "./windows-pipeline.mjs";

export const OSS_BASE_URL = "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";
export const OSS_BUCKET = "shared-public-assets";
export const OSS_BOOTSTRAP_KEY = "bootstrap/windows-x64.json";
export const ROLLBACK_SNAPSHOT_NAME = "bootstrap-rollback-snapshot.json";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(scriptRoot, "..");
const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;
const digestPattern = /^[a-f0-9]{64}$/;

export function safeObjectKey(key) {
  return typeof key === "string" && key.length > 0 && !key.startsWith("/") && !key.includes("\\") && key.split("/").every((part) => part && part !== "." && part !== ".." && /^[A-Za-z0-9._@-]+$/.test(part));
}

export function compareVersions(left, right) {
  if (!stableVersion.test(left) || !stableVersion.test(right)) throw new Error("stable three-part versions are required");
  const a = left.split(".").map(Number); const b = right.split(".").map(Number);
  for (let index = 0; index < 3; index += 1) if (a[index] !== b[index]) return a[index] < b[index] ? -1 : 1;
  return 0;
}

export function ossAuthorization({ method, contentType = "", date, key, secret, accessKeyId, immutable = false, basePath = "/project-tauri-codex", bucket = OSS_BUCKET }) {
  if (!safeObjectKey(key)) throw new Error(`unsafe OSS object key: ${key}`);
  const canonicalHeaders = immutable ? "x-oss-forbid-overwrite:true\n" : "";
  const canonicalResource = `/${bucket}${basePath}/${key}`;
  const input = [method, "", contentType, date].join("\n") + `\n${canonicalHeaders}${canonicalResource}`;
  return `OSS ${accessKeyId}:${createHmac("sha1", secret).update(input).digest("base64")}`;
}

function sha256(bytes) { return createHash("sha256").update(bytes).digest("hex"); }
function contentType(key) { return key.endsWith(".json") ? "application/json" : "application/octet-stream"; }

function settings({ accessKeyId, accessKeySecret, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  if (!accessKeyId || !accessKeySecret) throw new Error("ALIYUN_OSS_ACCESS_KEY_ID and ALIYUN_OSS_ACCESS_KEY_SECRET are required");
  return { accessKeyId, accessKeySecret, baseURL, bucket };
}

function frozenSourceCommit() {
  const status = spawnSync("git", ["-C", workspaceRoot, "status", "--porcelain"], { encoding: "utf8", windowsHide: true });
  if (status.error || status.status !== 0) throw new Error(`无法检查 Git worktree：${status.error?.message ?? status.stderr.trim()}`);
  if (status.stdout.trim()) throw new Error("OSS 发布必须从 clean Git worktree 执行");
  const head = spawnSync("git", ["-C", workspaceRoot, "rev-parse", "HEAD"], { encoding: "utf8", windowsHide: true });
  const commit = head.stdout.trim();
  if (head.error || head.status !== 0 || !/^[a-f0-9]{40}$/.test(commit)) throw new Error("无法固定 OSS 发布 source commit");
  return commit;
}

async function responseBytes(response, label) {
  if (!response.ok) throw new Error(`${label} returned HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

async function getObjectRecord(fetchImpl, baseURL, key) {
  const response = await fetchImpl(`${baseURL}/${key}`, { headers: { "User-Agent": "tauri-codex-release-publisher" }, redirect: "error" });
  if (response.status === 404) return null;
  return { bytes: await responseBytes(response, `OSS ${key}`), etag: response.headers.get("etag") };
}

async function getObject(fetchImpl, baseURL, key) {
  const record = await getObjectRecord(fetchImpl, baseURL, key);
  return record?.bytes ?? null;
}

async function putObject(fetchImpl, config, key, bytes, immutable, conditions = {}) {
  const date = new Date().toUTCString(); const base = new URL(config.baseURL); const type = contentType(key);
  const headers = { "Content-Type": type, Date: date, Authorization: ossAuthorization({ method: "PUT", contentType: type, date, key, secret: config.accessKeySecret, accessKeyId: config.accessKeyId, immutable, basePath: base.pathname.replace(/\/$/, ""), bucket: config.bucket }) };
  if (immutable) headers["x-oss-forbid-overwrite"] = "true";
  if (conditions.ifMatch) headers["If-Match"] = conditions.ifMatch;
  if (conditions.ifNoneMatch) headers["If-None-Match"] = "*";
  const response = await fetchImpl(`${config.baseURL}/${key}`, { method: "PUT", headers, body: bytes, redirect: "error" });
  if (!(response.ok || (immutable && response.status === 409))) throw new Error(`OSS PUT ${key} returned HTTP ${response.status}`);
}

async function deleteObject(fetchImpl, config, key, conditions = {}) {
  const date = new Date().toUTCString(); const base = new URL(config.baseURL);
  const headers = { Date: date, Authorization: ossAuthorization({ method: "DELETE", date, key, secret: config.accessKeySecret, accessKeyId: config.accessKeyId, basePath: base.pathname.replace(/\/$/, ""), bucket: config.bucket }) };
  if (conditions.ifMatch) headers["If-Match"] = conditions.ifMatch;
  const response = await fetchImpl(`${config.baseURL}/${key}`, { method: "DELETE", headers, redirect: "error" });
  if (!(response.ok || response.status === 204 || response.status === 404)) throw new Error(`OSS DELETE ${key} returned HTTP ${response.status}`);
}

function validateArtifact(artifact, expectedPrefix) {
  if (!artifact || !safeObjectKey(artifact.objectKey) || !artifact.objectKey.startsWith(expectedPrefix) || !Number.isSafeInteger(artifact.size) || artifact.size <= 0 || !digestPattern.test(artifact.sha256) || typeof artifact.provenance !== "string" || !artifact.provenance) throw new Error(`invalid candidate artifact under ${expectedPrefix}`);
}

function releaseTrust(releaseKeyId, releasePublicKey) {
  const keyId = releaseKeyId?.trim();
  const raw = Buffer.from(releasePublicKey?.trim() ?? "", "base64");
  if (!keyId || raw.length !== 32) throw new Error("TAURI_CODEX_RELEASE_KEY_ID and TAURI_CODEX_RELEASE_PUBLIC_KEY are required");
  const publicKey = createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), raw]), format: "der", type: "spki" });
  return { keyId, publicKey };
}

function verifySignedEnvelope(envelope, trust, label) {
  if (envelope?.schemaVersion !== 2 || envelope.keyId !== trust.keyId || !envelope.payload || typeof envelope.signature !== "string") throw new Error(`${label} signed envelope identity 无效`);
  let signature;
  try { signature = Buffer.from(envelope.signature, "base64"); } catch { throw new Error(`${label} signature 不是 base64`); }
  if (signature.length !== 64 || !verify(null, Buffer.from(canonicalJson(envelope.payload)), trust.publicKey, signature)) throw new Error(`${label} Ed25519 signature 校验失败`);
  return envelope.payload;
}

function localBytes(releaseRoot, localPath, label) {
  if (typeof localPath !== "string" || !localPath || path.isAbsolute(localPath)) throw new Error(`${label} localPath 无效`);
  const root = path.resolve(releaseRoot);
  const resolved = path.resolve(root, localPath);
  if (!resolved.startsWith(`${root}${path.sep}`)) throw new Error(`${label} localPath 越界`);
  return readFileSync(resolved);
}

function sameArtifact(left, right) {
  return left?.objectKey === right?.objectKey && left?.size === right?.size && left?.sha256 === right?.sha256 && left?.provenance === right?.provenance;
}

function validateManifestPayload(manifest, candidate) {
  if (!stableVersion.test(manifest.minimumLauncherVersion) || !stableVersion.test(manifest.minimumManagerVersion) || !Array.isArray(manifest.components)) throw new Error("manifest compatibility fields 无效");
  if (compareVersions(manifest.minimumLauncherVersion, candidate.installerVersion) > 0) throw new Error("manifest minimumLauncherVersion 超出 candidate Installer");
  const ids = new Set();
  const expected = { manager: ["archive", "zip", "manager"], codex: ["archive", "zip", "codex"], node: ["system", "msi", "system"] };
  for (const component of manifest.components) {
    if (!component || !expected[component.id] || ids.has(component.id)) throw new Error("manifest component ID 不唯一或未知");
    ids.add(component.id);
    const [kind, archive, installPath] = expected[component.id];
    if (!stableVersion.test(component.version) || component.kind !== kind || component.archive !== archive || component.installPath !== installPath || component.required !== true || component.provenance !== "authenticode+ed25519") throw new Error(`manifest ${component.id} 规则无效`);
    if ((component.id === "manager" || component.id === "codex") && !digestPattern.test(component.installedTreeSha256)) throw new Error(`manifest ${component.id} 安装树 SHA-256 无效`);
    if (component.id === "node" && component.installedTreeSha256 != null) throw new Error("manifest node 不得声明安装树 SHA-256");
    validateArtifact(component.artifact, `releases/${candidate.version}/windows-x64/components/`);
    if (component.artifact.provenance !== component.provenance) throw new Error(`manifest ${component.id} provenance 不一致`);
    if (component.id === "manager" && component.version !== candidate.version) throw new Error("manifest Manager version 与 release 不一致");
  }
  for (const id of Object.keys(expected)) if (!ids.has(id)) throw new Error(`manifest 缺少必需 ${id}`);
  if (compareVersions(manifest.minimumManagerVersion, manifest.components.find((component) => component.id === "manager").version) > 0) throw new Error("manifest minimumManagerVersion 超出 Manager");
}

function validatePublishedBootstrap(bootstrap) {
  if (bootstrap?.product !== "tauri-codex" || bootstrap.platform !== "windows" || bootstrap.architecture !== "x86_64" || !stableVersion.test(bootstrap.minimumLauncherVersion)) throw new Error("Bootstrap platform/compatibility 规则无效");
  if (!bootstrap.installer || !stableVersion.test(bootstrap.installer.version) || compareVersions(bootstrap.minimumLauncherVersion, bootstrap.installer.version) > 0) throw new Error("Bootstrap Installer version 无效");
  validateArtifact(bootstrap.installer.artifact, `installers/${bootstrap.installer.version}/windows-x64/`);
  if (bootstrap.installer.artifact.provenance !== "authenticode+ed25519") throw new Error("Bootstrap Installer provenance 无效");
  if (!bootstrap.release || !stableVersion.test(bootstrap.release.version)) throw new Error("Bootstrap release version 无效");
  validateArtifact(bootstrap.release.manifest, `releases/${bootstrap.release.version}/windows-x64/`);
  if (bootstrap.release.manifest.objectKey !== `releases/${bootstrap.release.version}/windows-x64/manifest.json` || bootstrap.release.manifest.provenance !== "ed25519") throw new Error("Bootstrap manifest identity 无效");
}

function validateLegacyBootstrap(bootstrap) {
  if (bootstrap?.schemaVersion !== 1 || bootstrap.product !== "tauri-codex" || bootstrap.platform !== "windows" || bootstrap.architecture !== "x86_64") throw new Error("legacy Bootstrap identity 无效");
  if (!stableVersion.test(bootstrap.installer?.version) || !stableVersion.test(bootstrap.release?.version)) throw new Error("legacy Bootstrap version 无效");
  validateArtifact({ ...bootstrap.installer.artifact, provenance: "legacy-authenticode" }, `installers/${bootstrap.installer.version}/windows-x64/`);
  validateArtifact({ ...bootstrap.release.manifest, provenance: "legacy-manifest" }, `releases/${bootstrap.release.version}/windows-x64/`);
}

function previousBootstrapPayload(bytes, trust) {
  let envelope;
  try { envelope = JSON.parse(bytes.toString("utf8")); } catch { throw new Error("previous Bootstrap JSON 无效"); }
  if (envelope?.schemaVersion === 1) {
    validateLegacyBootstrap(envelope);
    return envelope;
  }
  const payload = verifySignedEnvelope(envelope, trust, "previous Bootstrap");
  validatePublishedBootstrap(payload);
  return payload;
}

function validateBootstrapPayload(bootstrap, candidate) {
  validatePublishedBootstrap(bootstrap);
  if (bootstrap.product !== candidate.product || bootstrap.platform !== candidate.platform || bootstrap.architecture !== candidate.architecture || !stableVersion.test(bootstrap.minimumLauncherVersion) || compareVersions(bootstrap.minimumLauncherVersion, candidate.installerVersion) > 0) throw new Error("Bootstrap compatibility 规则无效");
  if (!bootstrap.installer || bootstrap.installer.version !== candidate.installerVersion) throw new Error("Bootstrap Installer identity 无效");
  validateArtifact(bootstrap.installer.artifact, `installers/${candidate.installerVersion}/windows-x64/`);
  if (bootstrap.installer.artifact.provenance !== "authenticode+ed25519") throw new Error("Bootstrap Installer provenance 无效");
  if (!bootstrap.release || bootstrap.release.version !== candidate.version) throw new Error("Bootstrap release identity 无效");
  validateArtifact(bootstrap.release.manifest, `releases/${candidate.version}/windows-x64/`);
  if (bootstrap.release.manifest.objectKey !== `releases/${candidate.version}/windows-x64/manifest.json` || bootstrap.release.manifest.provenance !== "ed25519") throw new Error("Bootstrap manifest identity 无效");
}

function loadCandidate(releaseRoot, trust, expectedSourceCommit) {
  const candidateEnvelope = JSON.parse(readFileSync(path.join(releaseRoot, "candidate.json"), "utf8"));
  const candidate = verifySignedEnvelope(candidateEnvelope, trust, "candidate");
  if (candidate.product !== "tauri-codex" || candidate.platform !== "windows" || candidate.architecture !== "x86_64" || !stableVersion.test(candidate.version) || !stableVersion.test(candidate.installerVersion) || !/^[a-f0-9]{40}$/.test(candidate.sourceCommit) || candidate.bootstrap?.objectKey !== OSS_BOOTSTRAP_KEY || candidate.bootstrap?.provenance !== "ed25519") throw new Error("candidate identity 无效");
  if (!/^[a-f0-9]{40}$/.test(expectedSourceCommit) || candidate.sourceCommit !== expectedSourceCommit) throw new Error("candidate source commit 与当前冻结源码不一致");
  const bootstrapBytes = localBytes(releaseRoot, candidate.bootstrap.localPath, "Bootstrap");
  if (bootstrapBytes.length !== candidate.bootstrap.size || sha256(bootstrapBytes) !== candidate.bootstrap.sha256) throw new Error("frozen Bootstrap bytes 已变化");
  const bootstrap = verifySignedEnvelope(JSON.parse(bootstrapBytes.toString("utf8")), trust, "Bootstrap");
  if (bootstrap.product !== candidate.product || bootstrap.platform !== candidate.platform || bootstrap.architecture !== candidate.architecture || bootstrap.release?.version !== candidate.version || bootstrap.installer?.version !== candidate.installerVersion) throw new Error("Bootstrap 与 candidate identity 不一致");
  validateBootstrapPayload(bootstrap, candidate);
  const roles = candidate.immutable.map((item) => item.role);
  if (roles.length !== 5 || new Set(roles).size !== roles.length || !["manifest", "manager", "codex", "node", "installer"].every((role) => roles.includes(role))) throw new Error("candidate immutable roles 不完整");
  const immutable = candidate.immutable.map((item) => {
    const prefix = item.role === "installer" ? `installers/${candidate.installerVersion}/windows-x64/` : item.role === "manifest" ? `releases/${candidate.version}/windows-x64/` : `releases/${candidate.version}/windows-x64/components/`;
    validateArtifact(item.artifact, prefix);
    const bytes = item.localPath ? localBytes(releaseRoot, item.localPath, item.role) : null;
    if (!bytes && item.role !== "installer") throw new Error(`${item.role} 必须冻结本地 bytes`);
    if (bytes && (bytes.length !== item.artifact.size || sha256(bytes) !== item.artifact.sha256)) throw new Error(`frozen ${item.role} bytes 已变化`);
    return { ...item, bytes };
  });
  const manifestItem = immutable.find((item) => item.role === "manifest");
  const installerItem = immutable.find((item) => item.role === "installer");
  if (bootstrap.release.manifest.provenance !== "ed25519" || bootstrap.installer.artifact.provenance !== "authenticode+ed25519") throw new Error("Bootstrap artifact provenance 不满足发布身份要求");
  if (!sameArtifact(bootstrap.release.manifest, manifestItem.artifact) || !sameArtifact(bootstrap.installer.artifact, installerItem.artifact)) throw new Error("Bootstrap artifact closure 与 candidate 不一致");
  const manifest = verifySignedEnvelope(JSON.parse(manifestItem.bytes.toString("utf8")), trust, "manifest");
  if (manifest.product !== candidate.product || manifest.version !== candidate.version || manifest.platform !== candidate.platform || manifest.architecture !== candidate.architecture) throw new Error("manifest 与 candidate identity 不一致");
  validateManifestPayload(manifest, candidate);
  for (const role of ["manager", "codex", "node"]) {
    const item = immutable.find((entry) => entry.role === role);
    const component = manifest.components?.find((entry) => entry.id === role && entry.required === true);
    if (!component || component.artifact.provenance !== "authenticode+ed25519" || !sameArtifact(component.artifact, item.artifact)) throw new Error(`manifest ${role} closure 与 candidate 不一致`);
  }
  return { candidate, bootstrapBytes, immutable };
}

async function verifyObject(fetchImpl, config, artifact, label) {
  const bytes = await getObject(fetchImpl, config.baseURL, artifact.objectKey);
  if (!bytes || bytes.length !== artifact.size || sha256(bytes) !== artifact.sha256) throw new Error(`OSS ${label} identity mismatch`);
  return bytes;
}

function snapshotFile(releaseRoot, snapshotPath) {
  return path.resolve(snapshotPath ?? path.join(releaseRoot, ROLLBACK_SNAPSHOT_NAME));
}

function validEtag(etag) {
  return typeof etag === "string" && /^"[A-Za-z0-9-]{16,128}"$/.test(etag);
}

function loadRollbackSnapshot(releaseRoot, snapshotPath, release) {
  const file = snapshotFile(releaseRoot, snapshotPath);
  let snapshot;
  try { snapshot = JSON.parse(readFileSync(file, "utf8")); } catch { throw new Error("Bootstrap rollback snapshot 无效或缺失"); }
  if (snapshot?.schemaVersion !== 1 || snapshot.objectKey !== OSS_BOOTSTRAP_KEY || snapshot.target?.size !== release.bootstrapBytes.length || snapshot.target?.sha256 !== sha256(release.bootstrapBytes)) throw new Error("Bootstrap rollback snapshot target 不匹配");
  if (snapshot.previous === null) return { file, snapshot, previousBytes: null };
  if (!snapshot.previous || !validEtag(snapshot.previous.etag) || !Number.isSafeInteger(snapshot.previous.size) || snapshot.previous.size <= 0 || !digestPattern.test(snapshot.previous.sha256) || typeof snapshot.previous.bytesBase64 !== "string") throw new Error("Bootstrap rollback snapshot previous record 无效");
  let previousBytes;
  try { previousBytes = Buffer.from(snapshot.previous.bytesBase64, "base64"); } catch { throw new Error("Bootstrap rollback snapshot bytes 无效"); }
  if (previousBytes.length !== snapshot.previous.size || sha256(previousBytes) !== snapshot.previous.sha256 || previousBytes.toString("base64") !== snapshot.previous.bytesBase64) throw new Error("Bootstrap rollback snapshot bytes 已被篡改");
  return { file, snapshot, previousBytes };
}

function assertCurrentMatchesSnapshot(current, snapshotRecord) {
  if (snapshotRecord.previousBytes === null) {
    if (current) throw new Error("current Bootstrap 与空 rollback snapshot 不一致");
    return;
  }
  if (!current || !current.bytes.equals(snapshotRecord.previousBytes) || current.etag !== snapshotRecord.snapshot.previous.etag) throw new Error("current Bootstrap 已在 snapshot 后变化");
}

export async function preflightPublisher({ accessKeyId, accessKeySecret, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET, probeId = randomUUID() }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const key = `probes/tauri-codex-${probeId}.txt`; const bytes = Buffer.from(`tauri-codex OSS probe ${probeId}\n`);
  await putObject(fetchImpl, config, key, bytes, true);
  let admissionError;
  try {
    const readback = await getObject(fetchImpl, baseURL, key);
    if (!readback?.equals(bytes)) throw new Error("OSS probe anonymous readback mismatch");
  } catch (error) {
    admissionError = error;
  }
  await deleteObject(fetchImpl, config, key);
  if (await getObject(fetchImpl, baseURL, key)) throw new Error("OSS probe cleanup failed");
  if (admissionError) throw admissionError;
  return { admitted: true, key };
}

export async function stageRelease({ releaseRoot, expectedSourceCommit, accessKeyId, accessKeySecret, releaseKeyId, releasePublicKey, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const trust = releaseTrust(releaseKeyId, releasePublicKey); const release = loadCandidate(releaseRoot, trust, expectedSourceCommit);
  for (const item of release.immutable) {
    if (item.bytes) await putObject(fetchImpl, config, item.artifact.objectKey, item.bytes, true);
    await verifyObject(fetchImpl, config, item.artifact, item.role);
  }
  return { staged: true, release: release.candidate.version, objects: release.immutable.map((item) => item.artifact.objectKey) };
}

export async function snapshotRelease({ releaseRoot, snapshotPath, expectedSourceCommit, accessKeyId, accessKeySecret, releaseKeyId, releasePublicKey, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  settings({ accessKeyId, accessKeySecret, baseURL, bucket });
  const trust = releaseTrust(releaseKeyId, releasePublicKey); const release = loadCandidate(releaseRoot, trust, expectedSourceCommit);
  const current = await getObjectRecord(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (current && !validEtag(current.etag)) throw new Error("current Bootstrap 缺少可用于条件恢复的 ETag");
  if (current) previousBootstrapPayload(current.bytes, trust);
  const snapshot = {
    schemaVersion: 1,
    objectKey: OSS_BOOTSTRAP_KEY,
    target: { size: release.bootstrapBytes.length, sha256: sha256(release.bootstrapBytes) },
    previous: current ? { etag: current.etag, size: current.bytes.length, sha256: sha256(current.bytes), bytesBase64: current.bytes.toString("base64") } : null,
  };
  const file = snapshotFile(releaseRoot, snapshotPath);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, `${JSON.stringify(snapshot, null, 2)}\n`, { flag: "wx" });
  return { snapshotted: true, release: release.candidate.version, snapshot: file, previous: current ? snapshot.previous.sha256 : null };
}

export async function commitRelease({ releaseRoot, snapshotPath, expectedSourceCommit, accessKeyId, accessKeySecret, releaseKeyId, releasePublicKey, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const trust = releaseTrust(releaseKeyId, releasePublicKey); const release = loadCandidate(releaseRoot, trust, expectedSourceCommit);
  for (const item of release.immutable) await verifyObject(fetchImpl, config, item.artifact, item.role);
  const snapshot = loadRollbackSnapshot(releaseRoot, snapshotPath, release);
  const current = await getObjectRecord(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  assertCurrentMatchesSnapshot(current, snapshot);
  const nextEnvelope = JSON.parse(release.bootstrapBytes.toString("utf8"));
  const nextPayload = verifySignedEnvelope(nextEnvelope, trust, "next Bootstrap");
  if (current) {
    const currentPayload = previousBootstrapPayload(current.bytes, trust);
    const currentVersion = currentPayload?.release?.version;
    const nextVersion = nextPayload?.release?.version;
    const currentInstallerVersion = currentPayload?.installer?.version;
    const nextInstallerVersion = nextPayload?.installer?.version;
    if (stableVersion.test(currentVersion) && stableVersion.test(nextVersion) && compareVersions(nextVersion, currentVersion) < 0) throw new Error(`refusing Bootstrap downgrade ${currentVersion} -> ${nextVersion}`);
    if (currentInstallerVersion && nextInstallerVersion && stableVersion.test(currentInstallerVersion) && stableVersion.test(nextInstallerVersion) && compareVersions(nextInstallerVersion, currentInstallerVersion) < 0) throw new Error(`refusing Installer downgrade ${currentInstallerVersion} -> ${nextInstallerVersion}`);
    if (stableVersion.test(currentVersion) && stableVersion.test(nextVersion) && compareVersions(nextVersion, currentVersion) === 0 && !current.bytes.equals(release.bootstrapBytes)) throw new Error(`refusing same-version Bootstrap replacement ${nextVersion}`);
    if (current.bytes.equals(release.bootstrapBytes)) return { committed: true, release: release.candidate.version, bootstrap: OSS_BOOTSTRAP_KEY, idempotent: true };
    await putObject(fetchImpl, config, OSS_BOOTSTRAP_KEY, release.bootstrapBytes, false, { ifMatch: snapshot.snapshot.previous.etag });
  } else {
    await putObject(fetchImpl, config, OSS_BOOTSTRAP_KEY, release.bootstrapBytes, false, { ifNoneMatch: true });
  }
  const confirmed = await getObject(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY); if (!confirmed?.equals(release.bootstrapBytes)) throw new Error("OSS Bootstrap commit readback mismatch");
  return { committed: true, release: release.candidate.version, bootstrap: OSS_BOOTSTRAP_KEY };
}

export async function confirmRelease({ releaseRoot, expectedSourceCommit, accessKeyId, accessKeySecret, releaseKeyId, releasePublicKey, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const trust = releaseTrust(releaseKeyId, releasePublicKey); const release = loadCandidate(releaseRoot, trust, expectedSourceCommit);
  for (const item of release.immutable) await verifyObject(fetchImpl, config, item.artifact, item.role);
  const current = await getObject(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (!current?.equals(release.bootstrapBytes)) throw new Error("public OSS Bootstrap 与 candidate 不一致");
  return { confirmed: true, release: release.candidate.version, bootstrap: OSS_BOOTSTRAP_KEY };
}

export async function rollbackRelease({ releaseRoot, snapshotPath, expectedSourceCommit, accessKeyId, accessKeySecret, releaseKeyId, releasePublicKey, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const trust = releaseTrust(releaseKeyId, releasePublicKey); const release = loadCandidate(releaseRoot, trust, expectedSourceCommit);
  const snapshot = loadRollbackSnapshot(releaseRoot, snapshotPath, release);
  const current = await getObjectRecord(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (!current?.bytes.equals(release.bootstrapBytes) || !validEtag(current.etag)) throw new Error("rollback 拒绝覆盖非本候选 Bootstrap");
  if (snapshot.previousBytes) {
    await putObject(fetchImpl, config, OSS_BOOTSTRAP_KEY, snapshot.previousBytes, false, { ifMatch: current.etag });
  } else {
    await deleteObject(fetchImpl, config, OSS_BOOTSTRAP_KEY, { ifMatch: current.etag });
  }
  const restored = await getObject(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (snapshot.previousBytes ? !restored?.equals(snapshot.previousBytes) : restored !== null) throw new Error("OSS Bootstrap rollback readback mismatch");
  return { rolledBack: true, release: release.candidate.version, restored: snapshot.previousBytes ? snapshot.snapshot.previous.sha256 : null };
}

export async function publishRelease(options) { await stageRelease(options); await snapshotRelease(options); return commitRelease(options); }

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  const [mode, version] = process.argv.slice(2); const appVersion = JSON.parse(readFileSync(path.join(workspaceRoot, "app", "package.json"), "utf8")).version;
  try {
    if (!stableVersion.test(version) || version !== appVersion) throw new Error("version must match app/package.json");
    const common = { releaseRoot: path.join(workspaceRoot, ".codex-build", "releases", version, "windows-x64"), expectedSourceCommit: mode === "preflight" ? undefined : frozenSourceCommit(), accessKeyId: process.env.ALIYUN_OSS_ACCESS_KEY_ID, accessKeySecret: process.env.ALIYUN_OSS_ACCESS_KEY_SECRET, releaseKeyId: process.env.TAURI_CODEX_RELEASE_KEY_ID, releasePublicKey: process.env.TAURI_CODEX_RELEASE_PUBLIC_KEY };
    const result = mode === "preflight" ? await preflightPublisher(common) : mode === "stage" ? await stageRelease(common) : mode === "snapshot" ? await snapshotRelease(common) : mode === "commit" ? await commitRelease(common) : mode === "confirm" ? await confirmRelease(common) : mode === "rollback" ? await rollbackRelease(common) : null;
    if (!result) throw new Error("usage: publish:release:oss -- <preflight|stage|snapshot|commit|confirm|rollback> <version>");
    console.log(JSON.stringify(result, null, 2));
  } catch (error) { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1; }
}
