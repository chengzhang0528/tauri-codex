import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { installedTreeSha256, selfUseEnvelope } from "./windows-pipeline.mjs";
import {
  commitRelease,
  compareVersions,
  OSS_BOOTSTRAP_KEY,
  ossAuthorization,
  preflightPublisher,
  publishRelease,
  rollbackRelease,
  safeObjectKey,
  snapshotRelease,
  stageRelease,
} from "./oss-release.mjs";

const baseURL = "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";
function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function artifact(objectKey, bytes, provenance) {
  return { objectKey, size: bytes.length, sha256: digest(bytes), provenance };
}

function jsonBytes(value) {
  return Buffer.from(`${JSON.stringify(value, null, 2)}\n`);
}

function legacyBootstrapBytes(version = "0.1.11", installerVersion = "1.0.4") {
  return jsonBytes({
    schemaVersion: 1,
    product: "tauri-codex",
    platform: "windows",
    architecture: "x86_64",
    installer: {
      version: installerVersion,
      artifact: { objectKey: `installers/${installerVersion}/windows-x64/tauri-codex_${installerVersion}_x64-setup.exe`, size: 100, sha256: "1".repeat(64) },
    },
    release: {
      version,
      manifest: { objectKey: `releases/${version}/windows-x64/manifest.json`, size: 100, sha256: "2".repeat(64) },
    },
  });
}

function fixture(version = "0.2.0", installerVersion = "1.1.0", options = {}) {
  const root = mkdtempSync(path.join(os.tmpdir(), "tauri-codex-oss-v3-"));
  const componentsRoot = path.join(root, "components");
  mkdirSync(componentsRoot);
  const blobs = {
    manager: Buffer.from(`manager-${version}`),
    codex: Buffer.from("codex-0.147.0"),
    node: Buffer.from("node-24.19.0"),
    installer: Buffer.from(`installer-${installerVersion}`),
  };
  const names = {
    manager: `tauri-codex-manager-${version}-windows-x64.zip`,
    codex: "tauri-codex-codex-0.147.0-windows-x64.zip",
    node: "node-v24.19.0-x64.msi",
    installer: `tauri-codex_${installerVersion}_x64-setup.exe`,
  };
  const records = {
    manager: artifact(`releases/${version}/windows-x64/components/${names.manager}`, blobs.manager, "unsigned-self-use+sha256"),
    codex: artifact(`releases/${version}/windows-x64/components/${names.codex}`, blobs.codex, options.codexProvenance ?? "upstream-authenticode+sha256"),
    node: artifact(`releases/${version}/windows-x64/components/${names.node}`, blobs.node, "upstream-authenticode+sha256"),
    installer: artifact(`installers/${installerVersion}/windows-x64/${names.installer}`, blobs.installer, "unsigned-self-use+sha256"),
  };
  const manifestPayload = {
    product: "tauri-codex", version, platform: "windows", architecture: "x86_64",
    minimumLauncherVersion: installerVersion, minimumManagerVersion: version,
    components: [
      { id: "manager", version, kind: "archive", archive: "zip", required: true, installPath: "manager", provenance: records.manager.provenance, installedTreeSha256: digest(Buffer.from(`manager-tree-${version}`)), artifact: records.manager },
      { id: "codex", version: "0.147.0", kind: "archive", archive: "zip", required: true, installPath: "codex", provenance: records.codex.provenance, installedTreeSha256: digest(Buffer.from("codex-tree-0.147.0")), artifact: records.codex },
      { id: "node", version: "24.19.0", kind: "system", archive: "msi", required: true, installPath: "system", provenance: records.node.provenance, installedTreeSha256: null, artifact: records.node },
    ],
  };
  const manifestBytes = jsonBytes(selfUseEnvelope(manifestPayload));
  records.manifest = artifact(`releases/${version}/windows-x64/manifest.json`, manifestBytes, "self-use+sha256");
  const bootstrapPayload = {
    product: "tauri-codex", platform: "windows", architecture: "x86_64", minimumLauncherVersion: installerVersion,
    installer: { version: installerVersion, artifact: records.installer },
    release: { version, manifest: records.manifest },
  };
  const bootstrapBytes = jsonBytes(selfUseEnvelope(bootstrapPayload));
  writeFileSync(path.join(root, "manifest.json"), manifestBytes);
  writeFileSync(path.join(root, "bootstrap.json"), bootstrapBytes);
  for (const role of ["manager", "codex", "node"]) writeFileSync(path.join(componentsRoot, names[role]), blobs[role]);
  if (!options.reuseInstaller) writeFileSync(path.join(root, names.installer), blobs.installer);
  const immutable = [
    { role: "manifest", localPath: "manifest.json", artifact: records.manifest },
    ...["manager", "codex", "node"].map((role) => ({ role, localPath: `components/${names[role]}`, artifact: records[role] })),
    { role: "installer", localPath: options.reuseInstaller ? null : names.installer, artifact: records.installer },
  ];
  const candidatePayload = {
    product: "tauri-codex", version, installerVersion, platform: "windows", architecture: "x86_64", sourceCommit: "a".repeat(40),
    bootstrap: { localPath: "bootstrap.json", objectKey: OSS_BOOTSTRAP_KEY, size: bootstrapBytes.length, sha256: digest(bootstrapBytes), provenance: "self-use+sha256" },
    immutable,
  };
  writeFileSync(path.join(root, "candidate.json"), jsonBytes(selfUseEnvelope(candidatePayload)));
  return { root, records, blobs, bootstrapBytes, candidatePayload };
}

function fakeFetch(options = {}) {
  const objects = new Map(options.objects ?? []);
  const events = [];
  const fetchImpl = async (input, init = {}) => {
    const url = new URL(input);
    const method = init.method ?? "GET";
    const key = url.href.startsWith(`${baseURL}/`) ? url.href.slice(baseURL.length + 1) : null;
    if (key && method === "GET") {
      events.push(`get:${key}`);
      if (options.failGet === key) return new Response("injected", { status: 500 });
      const bytes = objects.get(key);
      return new Response(bytes ?? "missing", { status: bytes ? 200 : 404, headers: bytes ? { etag: `"${digest(bytes)}"` } : undefined });
    }
    if (key && method === "PUT") {
      events.push(`put:${key}`);
      if (options.failPut === key) return new Response("injected", { status: 500 });
      if (options.beforeConditionalPut === key) {
        objects.set(key, options.raceBytes);
        options.beforeConditionalPut = null;
      }
      if (init.headers["x-oss-forbid-overwrite"] === "true" && objects.has(key)) return new Response("exists", { status: 409 });
      if (init.headers["If-Match"] && (!objects.has(key) || init.headers["If-Match"] !== `"${digest(objects.get(key))}"`)) return new Response("changed", { status: 412 });
      if (init.headers["If-None-Match"] === "*" && objects.has(key)) return new Response("exists", { status: 412 });
      objects.set(key, Buffer.from(init.body));
      return new Response("", { status: 200 });
    }
    if (key && method === "DELETE") {
      events.push(`delete:${key}`);
      if (init.headers["If-Match"] && (!objects.has(key) || init.headers["If-Match"] !== `"${digest(objects.get(key))}"`)) return new Response("changed", { status: 412 });
      objects.delete(key);
      return new Response(null, { status: 204 });
    }
    return new Response("unexpected", { status: 404 });
  };
  return { fetchImpl, objects, events };
}

function publishOptions(release, remote) {
  return { releaseRoot: release.root, expectedSourceCommit: "a".repeat(40), accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl };
}

async function snapshotAndCommit(release, remote) {
  const options = publishOptions(release, remote);
  await snapshotRelease(options);
  return commitRelease(options);
}

test("validates object keys, versions, and OSS authorization", () => {
  assert.equal(safeObjectKey("releases/0.2.0/windows-x64/manifest.json"), true);
  assert.equal(safeObjectKey("releases/0.2.0/../secret"), false);
  assert.equal(compareVersions("0.2.0", "0.1.9"), 1);
  assert.match(ossAuthorization({ method: "PUT", contentType: "application/json", date: "Thu, 13 Aug 2026 00:00:00 GMT", key: OSS_BOOTSTRAP_KEY, secret: "secret", accessKeyId: "id" }), /^OSS id:/);
});

test("build and runtime share the fixed installed-tree digest format", () => {
  const root = mkdtempSync(path.join(os.tmpdir(), "tauri-codex-tree-v3-"));
  try {
    mkdirSync(path.join(root, "Bin"));
    writeFileSync(path.join(root, "Bin", "app.exe"), "app");
    writeFileSync(path.join(root, "readme.txt"), "docs");
    assert.equal(installedTreeSha256(root), "7df47593086e0fceca3c8194935fee043384498c3ec6f31d86aebdf599eae4db");
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test("publisher admission proves public readback and removes its exact probe", async () => {
  const remote = fakeFetch();
  const result = await preflightPublisher({ accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl, probeId: "run-1" });
  assert.equal(result.admitted, true);
  assert.deepEqual(remote.events, [
    "put:probes/tauri-codex-run-1.txt",
    "get:probes/tauri-codex-run-1.txt",
    "delete:probes/tauri-codex-run-1.txt",
    "get:probes/tauri-codex-run-1.txt",
  ]);
  assert.equal(remote.objects.size, 0);
});

test("publisher admission removes its exact probe after readback failure", async () => {
  const key = "probes/tauri-codex-run-failed.txt";
  const remote = fakeFetch({ failGet: key });

  await assert.rejects(
    () => preflightPublisher({ accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl, probeId: "run-failed" }),
    /HTTP 500/,
  );

  assert.equal(remote.objects.has(key), false);
  assert.equal(remote.events.includes(`delete:${key}`), true);
});

test("stages immutable objects without moving Bootstrap", async () => {
  const release = fixture();
  const old = fixture("0.1.9", "1.0.9");
  const remote = fakeFetch({ objects: [[OSS_BOOTSTRAP_KEY, old.bootstrapBytes]] });
  try {
    await stageRelease(publishOptions(release, remote));
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), old.bootstrapBytes);
    assert.equal(remote.events.includes(`put:${OSS_BOOTSTRAP_KEY}`), false);
    for (const item of release.candidatePayload.immutable) assert.deepEqual(remote.objects.get(item.artifact.objectKey), release.blobs[item.role] ?? readFileSync(path.join(release.root, item.localPath)));
  } finally {
    rmSync(release.root, { recursive: true, force: true });
    rmSync(old.root, { recursive: true, force: true });
  }
});

test("self-use policy does not weaken third-party provenance", async () => {
  const release = fixture("0.2.0", "1.1.0", { codexProvenance: "unsigned-self-use+sha256" });
  const remote = fakeFetch();
  try {
    await assert.rejects(() => stageRelease(publishOptions(release, remote)), /manifest codex 规则无效/);
    assert.equal(remote.events.length, 0);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("reuses a stable Installer only after OSS identity readback", async () => {
  const release = fixture("0.2.1", "1.1.0", { reuseInstaller: true });
  const remote = fakeFetch({ objects: [[release.records.installer.objectKey, release.blobs.installer]] });
  try {
    await stageRelease(publishOptions(release, remote));
    assert.equal(remote.events.includes(`put:${release.records.installer.objectKey}`), false);
    assert.equal(remote.events.includes(`get:${release.records.installer.objectKey}`), true);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("publishes the immutable closure and commits Bootstrap last", async () => {
  const release = fixture();
  const remote = fakeFetch();
  try {
    const result = await publishRelease(publishOptions(release, remote));
    assert.equal(result.release, "0.2.0");
    assert.equal(remote.events.filter((event) => event.startsWith("put:")).at(-1), `put:${OSS_BOOTSTRAP_KEY}`);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), release.bootstrapBytes);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("snapshot captures the exact previous Bootstrap bytes and ETag", async () => {
  const release = fixture();
  const previous = fixture("0.1.9", "1.0.9");
  const remote = fakeFetch({ objects: [[OSS_BOOTSTRAP_KEY, previous.bootstrapBytes]] });
  try {
    const result = await snapshotRelease(publishOptions(release, remote));
    const snapshot = JSON.parse(readFileSync(result.snapshot, "utf8"));
    assert.equal(snapshot.previous.bytesBase64, previous.bootstrapBytes.toString("base64"));
    assert.equal(snapshot.previous.sha256, digest(previous.bootstrapBytes));
    assert.equal(snapshot.previous.etag, `"${digest(previous.bootstrapBytes)}"`);
    assert.equal(snapshot.target.sha256, digest(release.bootstrapBytes));
  } finally {
    rmSync(release.root, { recursive: true, force: true });
    rmSync(previous.root, { recursive: true, force: true });
  }
});

test("first v3 publication conditionally migrates a validated schema v1 Bootstrap", async () => {
  const release = fixture();
  const previous = legacyBootstrapBytes();
  const remote = fakeFetch({ objects: [[OSS_BOOTSTRAP_KEY, previous]] });
  try {
    await publishRelease(publishOptions(release, remote));
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), release.bootstrapBytes);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("immutable upload failure leaves the old Bootstrap untouched", async () => {
  const release = fixture();
  const old = fixture("0.1.9", "1.0.9");
  const remote = fakeFetch({ objects: [[OSS_BOOTSTRAP_KEY, old.bootstrapBytes]], failPut: release.records.manager.objectKey });
  try {
    await assert.rejects(() => publishRelease(publishOptions(release, remote)));
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), old.bootstrapBytes);
    assert.equal(remote.events.includes(`put:${OSS_BOOTSTRAP_KEY}`), false);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
    rmSync(old.root, { recursive: true, force: true });
  }
});

test("commit refuses an incomplete OSS closure", async () => {
  const release = fixture();
  const remote = fakeFetch();
  try {
    await assert.rejects(() => snapshotAndCommit(release, remote), /identity mismatch/);
    assert.equal(remote.events.includes(`put:${OSS_BOOTSTRAP_KEY}`), false);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("candidate identity and frozen bytes reject mutation", async () => {
  const release = fixture();
  const remote = fakeFetch();
  try {
    const candidatePath = path.join(release.root, "candidate.json");
    const candidate = JSON.parse(readFileSync(candidatePath, "utf8"));
    candidate.payload.version = "0.2.1";
    writeFileSync(candidatePath, jsonBytes(candidate));
    await assert.rejects(() => stageRelease(publishOptions(release, remote)), /identity/);

    writeFileSync(candidatePath, jsonBytes(selfUseEnvelope(release.candidatePayload)));
    writeFileSync(path.join(release.root, "components", path.basename(release.records.manager.objectKey)), "mutated");
    await assert.rejects(() => stageRelease(publishOptions(release, remote)), /bytes/);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("publisher rejects a candidate from a different source commit", async () => {
  const release = fixture();
  const remote = fakeFetch();
  try {
    await assert.rejects(
      () => stageRelease({ ...publishOptions(release, remote), expectedSourceCommit: "b".repeat(40) }),
      /source commit/,
    );
    assert.equal(remote.events.length, 0);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("commit rejects a self-use Bootstrap downgrade", async () => {
  const current = fixture("0.2.1", "1.1.0");
  const older = fixture("0.2.0", "1.1.0");
  const objects = [[OSS_BOOTSTRAP_KEY, current.bootstrapBytes]];
  for (const item of older.candidatePayload.immutable) objects.push([item.artifact.objectKey, item.localPath ? readFileSync(path.join(older.root, item.localPath)) : older.blobs.installer]);
  const remote = fakeFetch({ objects });
  try {
    await assert.rejects(() => snapshotAndCommit(older, remote), /downgrade/);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), current.bootstrapBytes);
  } finally {
    rmSync(current.root, { recursive: true, force: true });
    rmSync(older.root, { recursive: true, force: true });
  }
});

test("commit rejects an Installer downgrade even when release advances", async () => {
  const current = fixture("0.2.0", "1.2.0");
  const next = fixture("0.2.1", "1.1.0");
  const objects = [[OSS_BOOTSTRAP_KEY, current.bootstrapBytes]];
  for (const item of next.candidatePayload.immutable) objects.push([item.artifact.objectKey, item.localPath ? readFileSync(path.join(next.root, item.localPath)) : next.blobs.installer]);
  const remote = fakeFetch({ objects });
  try {
    await assert.rejects(() => snapshotAndCommit(next, remote), /Installer downgrade/);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), current.bootstrapBytes);
  } finally {
    rmSync(current.root, { recursive: true, force: true });
    rmSync(next.root, { recursive: true, force: true });
  }
});

test("commit rejects different Bootstrap bytes for the same release version", async () => {
  const current = fixture("0.2.0", "1.1.0");
  const replacement = fixture("0.2.0", "1.2.0");
  const objects = [[OSS_BOOTSTRAP_KEY, current.bootstrapBytes]];
  for (const item of replacement.candidatePayload.immutable) objects.push([item.artifact.objectKey, item.localPath ? readFileSync(path.join(replacement.root, item.localPath)) : replacement.blobs.installer]);
  const remote = fakeFetch({ objects });
  try {
    await assert.rejects(() => snapshotAndCommit(replacement, remote), /same-version Bootstrap replacement/);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), current.bootstrapBytes);
  } finally {
    rmSync(current.root, { recursive: true, force: true });
    rmSync(replacement.root, { recursive: true, force: true });
  }
});

test("conditional Bootstrap commit rejects a concurrent writer", async () => {
  const current = fixture("0.1.9", "1.0.9");
  const next = fixture("0.2.0", "1.1.0");
  const racing = fixture("0.3.0", "1.2.0");
  const objects = [[OSS_BOOTSTRAP_KEY, current.bootstrapBytes]];
  for (const item of next.candidatePayload.immutable) objects.push([item.artifact.objectKey, item.localPath ? readFileSync(path.join(next.root, item.localPath)) : next.blobs.installer]);
  const remote = fakeFetch({ objects, beforeConditionalPut: OSS_BOOTSTRAP_KEY, raceBytes: racing.bootstrapBytes });
  try {
    await assert.rejects(() => snapshotAndCommit(next, remote), /HTTP 412/);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), racing.bootstrapBytes);
  } finally {
    rmSync(current.root, { recursive: true, force: true });
    rmSync(next.root, { recursive: true, force: true });
    rmSync(racing.root, { recursive: true, force: true });
  }
});

test("commit rejects a malformed self-use current Bootstrap", async () => {
  const current = fixture("0.1.9", "1.0.9");
  const next = fixture("0.2.0", "1.1.0");
  const currentPayload = JSON.parse(current.bootstrapBytes.toString("utf8")).payload;
  currentPayload.release.version = "invalid";
  const malformed = jsonBytes(selfUseEnvelope(currentPayload));
  const objects = [[OSS_BOOTSTRAP_KEY, malformed]];
  for (const item of next.candidatePayload.immutable) objects.push([item.artifact.objectKey, item.localPath ? readFileSync(path.join(next.root, item.localPath)) : next.blobs.installer]);
  const remote = fakeFetch({ objects });
  try {
    await assert.rejects(() => snapshotAndCommit(next, remote), /Bootstrap release version/);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), malformed);
  } finally {
    rmSync(current.root, { recursive: true, force: true });
    rmSync(next.root, { recursive: true, force: true });
  }
});

test("rollback restores the exact previous Bootstrap bytes", async () => {
  const previous = fixture("0.1.9", "1.0.9");
  const release = fixture();
  const remote = fakeFetch({ objects: [[OSS_BOOTSTRAP_KEY, previous.bootstrapBytes]] });
  try {
    const options = publishOptions(release, remote);
    await stageRelease(options);
    await snapshotRelease(options);
    await commitRelease(options);
    const result = await rollbackRelease(options);
    assert.equal(result.rolledBack, true);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), previous.bootstrapBytes);
  } finally {
    rmSync(previous.root, { recursive: true, force: true });
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("rollback rejects a changed current Bootstrap", async () => {
  const previous = fixture("0.1.9", "1.0.9");
  const release = fixture();
  const other = fixture("0.3.0", "1.2.0");
  const remote = fakeFetch({ objects: [[OSS_BOOTSTRAP_KEY, previous.bootstrapBytes]] });
  try {
    const options = publishOptions(release, remote);
    await snapshotRelease(options);
    remote.objects.set(OSS_BOOTSTRAP_KEY, other.bootstrapBytes);
    await assert.rejects(() => rollbackRelease(options), /非本候选/);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), other.bootstrapBytes);
  } finally {
    rmSync(previous.root, { recursive: true, force: true });
    rmSync(release.root, { recursive: true, force: true });
    rmSync(other.root, { recursive: true, force: true });
  }
});

test("rollback rejects a tampered snapshot", async () => {
  const previous = fixture("0.1.9", "1.0.9");
  const release = fixture();
  const remote = fakeFetch({ objects: [[OSS_BOOTSTRAP_KEY, previous.bootstrapBytes]] });
  try {
    const options = publishOptions(release, remote);
    const result = await snapshotRelease(options);
    const snapshot = JSON.parse(readFileSync(result.snapshot, "utf8"));
    snapshot.previous.bytesBase64 = Buffer.from("tampered").toString("base64");
    writeFileSync(result.snapshot, jsonBytes(snapshot));
    remote.objects.set(OSS_BOOTSTRAP_KEY, release.bootstrapBytes);
    await assert.rejects(() => rollbackRelease(options), /篡改/);
  } finally {
    rmSync(previous.root, { recursive: true, force: true });
    rmSync(release.root, { recursive: true, force: true });
  }
});
