import { createHash, createHmac, createPublicKey, randomUUID, verify } from "node:crypto";
import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { canonicalJson } from "./windows-pipeline.mjs";

export const OSS_BASE_URL = "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";
export const OSS_BUCKET = "shared-public-assets";
export const OSS_BOOTSTRAP_KEY = "bootstrap/windows-x64.json";

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

async function responseBytes(response, label) {
  if (!response.ok) throw new Error(`${label} returned HTTP ${response.status}`);
  return Buffer.from(await response.arrayBuffer());
}

async function getObject(fetchImpl, baseURL, key) {
  const response = await fetchImpl(`${baseURL}/${key}`, { headers: { "User-Agent": "tauri-codex-release-publisher" }, redirect: "error" });
  if (response.status === 404) return null;
  return responseBytes(response, `OSS ${key}`);
}

async function putObject(fetchImpl, config, key, bytes, immutable) {
  const date = new Date().toUTCString(); const base = new URL(config.baseURL); const type = contentType(key);
  const headers = { "Content-Type": type, Date: date, Authorization: ossAuthorization({ method: "PUT", contentType: type, date, key, secret: config.accessKeySecret, accessKeyId: config.accessKeyId, immutable, basePath: base.pathname.replace(/\/$/, ""), bucket: config.bucket }) };
  if (immutable) headers["x-oss-forbid-overwrite"] = "true";
  const response = await fetchImpl(`${config.baseURL}/${key}`, { method: "PUT", headers, body: bytes, redirect: "error" });
  if (!(response.ok || (immutable && response.status === 409))) throw new Error(`OSS PUT ${key} returned HTTP ${response.status}`);
}

async function deleteObject(fetchImpl, config, key) {
  const date = new Date().toUTCString(); const base = new URL(config.baseURL);
  const response = await fetchImpl(`${config.baseURL}/${key}`, { method: "DELETE", headers: { Date: date, Authorization: ossAuthorization({ method: "DELETE", date, key, secret: config.accessKeySecret, accessKeyId: config.accessKeyId, basePath: base.pathname.replace(/\/$/, ""), bucket: config.bucket }) }, redirect: "error" });
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

function loadCandidate(releaseRoot, trust) {
  const candidateEnvelope = JSON.parse(readFileSync(path.join(releaseRoot, "candidate.json"), "utf8"));
  const candidate = verifySignedEnvelope(candidateEnvelope, trust, "candidate");
  if (candidate.product !== "tauri-codex" || candidate.platform !== "windows" || candidate.architecture !== "x86_64" || !stableVersion.test(candidate.version) || !stableVersion.test(candidate.installerVersion) || !/^[a-f0-9]{40}$/.test(candidate.sourceCommit) || candidate.bootstrap?.objectKey !== OSS_BOOTSTRAP_KEY || candidate.bootstrap?.provenance !== "ed25519") throw new Error("candidate identity 无效");
  const bootstrapBytes = localBytes(releaseRoot, candidate.bootstrap.localPath, "Bootstrap");
  if (bootstrapBytes.length !== candidate.bootstrap.size || sha256(bootstrapBytes) !== candidate.bootstrap.sha256) throw new Error("frozen Bootstrap bytes 已变化");
  const bootstrap = verifySignedEnvelope(JSON.parse(bootstrapBytes.toString("utf8")), trust, "Bootstrap");
  if (bootstrap.product !== candidate.product || bootstrap.platform !== candidate.platform || bootstrap.architecture !== candidate.architecture || bootstrap.release?.version !== candidate.version || bootstrap.installer?.version !== candidate.installerVersion) throw new Error("Bootstrap 与 candidate identity 不一致");
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

export async function preflightPublisher({ accessKeyId, accessKeySecret, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET, probeId = randomUUID() }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const key = `probes/tauri-codex-${probeId}.txt`; const bytes = Buffer.from(`tauri-codex OSS probe ${probeId}\n`);
  await putObject(fetchImpl, config, key, bytes, true); const readback = await getObject(fetchImpl, baseURL, key); if (!readback?.equals(bytes)) throw new Error("OSS probe anonymous readback mismatch"); await deleteObject(fetchImpl, config, key); if (await getObject(fetchImpl, baseURL, key)) throw new Error("OSS probe cleanup failed"); return { admitted: true, key };
}

export async function stageRelease({ releaseRoot, accessKeyId, accessKeySecret, releaseKeyId, releasePublicKey, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const trust = releaseTrust(releaseKeyId, releasePublicKey); const release = loadCandidate(releaseRoot, trust);
  for (const item of release.immutable) {
    if (item.bytes) await putObject(fetchImpl, config, item.artifact.objectKey, item.bytes, true);
    await verifyObject(fetchImpl, config, item.artifact, item.role);
  }
  return { staged: true, release: release.candidate.version, objects: release.immutable.map((item) => item.artifact.objectKey) };
}

export async function commitRelease({ releaseRoot, accessKeyId, accessKeySecret, releaseKeyId, releasePublicKey, fetchImpl = fetch, baseURL = OSS_BASE_URL, bucket = OSS_BUCKET }) {
  const config = settings({ accessKeyId, accessKeySecret, baseURL, bucket }); const trust = releaseTrust(releaseKeyId, releasePublicKey); const release = loadCandidate(releaseRoot, trust);
  for (const item of release.immutable) await verifyObject(fetchImpl, config, item.artifact, item.role);
  const current = await getObject(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY);
  if (current) {
    const currentEnvelope = JSON.parse(current.toString("utf8")); const nextEnvelope = JSON.parse(release.bootstrapBytes.toString("utf8"));
    const currentVersion = verifySignedEnvelope(currentEnvelope, trust, "current Bootstrap")?.release?.version; const nextVersion = verifySignedEnvelope(nextEnvelope, trust, "next Bootstrap")?.release?.version;
    if (stableVersion.test(currentVersion) && stableVersion.test(nextVersion) && compareVersions(nextVersion, currentVersion) < 0) throw new Error(`refusing Bootstrap downgrade ${currentVersion} -> ${nextVersion}`);
  }
  await putObject(fetchImpl, config, OSS_BOOTSTRAP_KEY, release.bootstrapBytes, false);
  const confirmed = await getObject(fetchImpl, baseURL, OSS_BOOTSTRAP_KEY); if (!confirmed?.equals(release.bootstrapBytes)) throw new Error("OSS Bootstrap commit readback mismatch");
  return { committed: true, release: release.candidate.version, bootstrap: OSS_BOOTSTRAP_KEY };
}

export async function publishRelease(options) { await stageRelease(options); return commitRelease(options); }

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  const [mode, version] = process.argv.slice(2); const appVersion = JSON.parse(readFileSync(path.join(workspaceRoot, "app", "package.json"), "utf8")).version;
  try {
    if (!stableVersion.test(version) || version !== appVersion) throw new Error("version must match app/package.json");
    const common = { releaseRoot: path.join(workspaceRoot, ".codex-build", "releases", version, "windows-x64"), accessKeyId: process.env.ALIYUN_OSS_ACCESS_KEY_ID, accessKeySecret: process.env.ALIYUN_OSS_ACCESS_KEY_SECRET, releaseKeyId: process.env.TAURI_CODEX_RELEASE_KEY_ID, releasePublicKey: process.env.TAURI_CODEX_RELEASE_PUBLIC_KEY };
    const result = mode === "preflight" ? await preflightPublisher(common) : mode === "stage" ? await stageRelease(common) : mode === "commit" ? await commitRelease(common) : null;
    if (!result) throw new Error("usage: publish:release:oss -- <preflight|stage|commit> <version>");
    console.log(JSON.stringify(result, null, 2));
  } catch (error) { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1; }
}
