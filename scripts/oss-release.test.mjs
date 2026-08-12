import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import {
  commitRelease,
  compareVersions,
  OSS_BOOTSTRAP_KEY,
  ossAuthorization,
  preflightPublisher,
  publishRelease,
  safeObjectKey,
  stageRelease,
} from "./oss-release.mjs";

const baseURL = "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";

function digest(bytes) {
  return createHash("sha256").update(bytes).digest("hex");
}

function artifact(url, objectKey, bytes) {
  return { url, objectKey, size: bytes.length, sha256: digest(bytes) };
}

function fixture(version = "0.1.8", installerVersion = "1.0.3") {
  const root = mkdtempSync(path.join(os.tmpdir(), "tauri-codex-oss-release-"));
  const components = path.join(root, "components");
  mkdirSync(components);
  const manager = Buffer.from("manager");
  const codex = Buffer.from("codex");
  const node = Buffer.from("node");
  const installer = Buffer.from("installer");
  const managerName = `tauri-codex-manager-${version}-windows-x64.zip`;
  const codexName = "tauri-codex-codex-0.147.0-windows-x64.zip";
  const nodeName = "node-v24.19.0-x64.msi";
  const installerName = `tauri-codex_${installerVersion}_x64-setup.exe`;
  const releaseURL = `https://github.com/chengzhang0528/tauri-codex/releases/download/v${version}`;
  const manifest = {
    schemaVersion: 1, product: "tauri-codex", version, platform: "windows", architecture: "x86_64",
    components: [
      { id: "manager", version, required: true, artifact: artifact(`${releaseURL}/${managerName}`, `releases/${version}/windows-x64/components/${managerName}`, manager) },
      { id: "codex", version: "0.147.0", required: true, artifact: artifact(`${releaseURL}/${codexName}`, `releases/${version}/windows-x64/components/${codexName}`, codex) },
      { id: "node", version: "24.19.0", required: true, artifact: artifact(`${releaseURL}/${nodeName}`, `releases/${version}/windows-x64/components/${nodeName}`, node) },
    ],
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  const bootstrap = {
    schemaVersion: 1, product: "tauri-codex", platform: "windows", architecture: "x86_64",
    installer: { version: installerVersion, artifact: artifact(
      `https://github.com/chengzhang0528/tauri-codex/releases/download/v0.1.8/${installerName}`,
      `installers/${installerVersion}/windows-x64/${installerName}`, installer,
    ) },
    release: { version, manifest: artifact(`${releaseURL}/manifest.json`, `releases/${version}/windows-x64/manifest.json`, manifestBytes) },
  };
  writeFileSync(path.join(root, "bootstrap.json"), `${JSON.stringify(bootstrap, null, 2)}\n`);
  writeFileSync(path.join(root, "manifest.json"), manifestBytes);
  writeFileSync(path.join(root, installerName), installer);
  writeFileSync(path.join(components, managerName), manager);
  writeFileSync(path.join(components, codexName), codex);
  writeFileSync(path.join(components, nodeName), node);
  const github = new Map([
    [`${releaseURL}/bootstrap.json`, Buffer.from(`${JSON.stringify(bootstrap, null, 2)}\n`)],
    [bootstrap.installer.artifact.url, installer],
    [bootstrap.release.manifest.url, manifestBytes],
    ...manifest.components.map((component, index) => [component.artifact.url, [manager, codex, node][index]]),
  ]);
  return { root, bootstrap, manifest, github };
}

function fakeFetch(github, options = {}) {
  const objects = new Map(options.objects ?? []);
  const events = [];
  const fetchImpl = async (input, init = {}) => {
    const url = new URL(input);
    const method = init.method ?? "GET";
    const key = url.href.startsWith(`${baseURL}/`) ? url.href.slice(baseURL.length + 1) : null;
    if (url.hostname === "github.com") {
      events.push(`github:${url.pathname}`);
      const bytes = github.get(url.href);
      return new Response(bytes ?? "missing", { status: bytes ? 200 : 404 });
    }
    if (key && method === "GET") {
      events.push(`get:${key}`);
      if (options.failGet === key) return new Response("injected", { status: 500 });
      const bytes = objects.get(key);
      return new Response(bytes ?? "missing", { status: bytes ? 200 : 404 });
    }
    if (key && method === "PUT") {
      events.push(`put:${key}`);
      if (options.failPut === key) return new Response("injected", { status: 500 });
      if (init.headers["x-oss-forbid-overwrite"] === "true" && objects.has(key)) {
        return new Response("exists", { status: 409 });
      }
      objects.set(key, Buffer.from(init.body));
      return new Response("", { status: 200 });
    }
    if (key && method === "DELETE") {
      events.push(`delete:${key}`);
      objects.delete(key);
      return new Response(null, { status: 204 });
    }
    return new Response("unexpected", { status: 404 });
  };
  return { fetchImpl, objects, events };
}

test("validates object keys, versions, and OSS signatures", () => {
  assert.equal(safeObjectKey("releases/0.1.8/windows-x64/manifest.json"), true);
  assert.equal(safeObjectKey("releases/0.1.8/../secret"), false);
  assert.equal(compareVersions("0.1.8", "0.1.7"), 1);
  assert.match(ossAuthorization({ method: "PUT", contentType: "application/json", date: "Thu, 13 Aug 2026 00:00:00 GMT", key: OSS_BOOTSTRAP_KEY, secret: "secret", accessKeyId: "id" }), /^OSS id:/);
});

test("publisher admission proves public readback and removes its exact probe", async () => {
  const remote = fakeFetch(new Map());
  const result = await preflightPublisher({
    accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl, probeId: "run-1",
  });
  assert.equal(result.admitted, true);
  assert.deepEqual(remote.events, [
    "put:probes/tauri-codex-run-1.txt",
    "get:probes/tauri-codex-run-1.txt",
    "delete:probes/tauri-codex-run-1.txt",
    "get:probes/tauri-codex-run-1.txt",
  ]);
  assert.equal(remote.objects.size, 0);
});

test("stages immutable objects without publishing Bootstrap", async () => {
  const release = fixture();
  const oldBootstrap = Buffer.from(JSON.stringify({ installer: { version: "1.0.2" }, release: { version: "0.1.7" } }));
  const remote = fakeFetch(release.github, { objects: [[OSS_BOOTSTRAP_KEY, oldBootstrap]] });
  try {
    await stageRelease({ releaseRoot: release.root, accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl });
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), oldBootstrap);
    assert.equal(remote.events.includes(`put:${OSS_BOOTSTRAP_KEY}`), false);
    assert.equal(remote.events.some((event) => event.startsWith("github:")), false);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("publishes immutable closure, reads it back, and commits Bootstrap last", async () => {
  const release = fixture();
  const remote = fakeFetch(release.github);
  try {
    const result = await publishRelease({ releaseRoot: release.root, accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl });
    assert.equal(result.release, "0.1.8");
    const puts = remote.events.filter((event) => event.startsWith("put:"));
    assert.equal(puts.at(-1), `put:${OSS_BOOTSTRAP_KEY}`);
    for (const component of release.manifest.components) assert.deepEqual(remote.objects.get(component.artifact.objectKey), release.github.get(component.artifact.url));
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("does not commit Bootstrap after an immutable object failure", async () => {
  const release = fixture();
  const managerKey = release.manifest.components[0].artifact.objectKey;
  const oldBootstrap = Buffer.from(JSON.stringify({
    installer: { version: "1.0.2" },
    release: { version: "0.1.7" },
  }));
  const remote = fakeFetch(release.github, { objects: [[OSS_BOOTSTRAP_KEY, oldBootstrap]], failPut: managerKey });
  try {
    await assert.rejects(() => publishRelease({ releaseRoot: release.root, accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl }));
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), oldBootstrap);
    assert.equal(remote.events.includes(`put:${managerKey}`), true);
    assert.equal(remote.events.includes(`put:${OSS_BOOTSTRAP_KEY}`), false);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});

test("does not commit Bootstrap unless GitHub exposes the frozen candidate", async () => {
  const release = fixture();
  const oldBootstrap = Buffer.from(JSON.stringify({ installer: { version: "1.0.2" }, release: { version: "0.1.7" } }));
  const remote = fakeFetch(release.github, { objects: [[OSS_BOOTSTRAP_KEY, oldBootstrap]] });
  try {
    await stageRelease({ releaseRoot: release.root, accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl });
    release.github.delete(`https://github.com/chengzhang0528/tauri-codex/releases/download/v0.1.8/bootstrap.json`);
    await assert.rejects(() => commitRelease({
      releaseRoot: release.root, accessKeyId: "id", accessKeySecret: "secret", fetchImpl: remote.fetchImpl,
    }), /GitHub Bootstrap/);
    assert.deepEqual(remote.objects.get(OSS_BOOTSTRAP_KEY), oldBootstrap);
  } finally {
    rmSync(release.root, { recursive: true, force: true });
  }
});
