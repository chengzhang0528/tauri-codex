import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import { nextPatchVersion, replaceVersion } from "./release.mjs";

test("increments a stable patch version", () => {
  assert.equal(nextPatchVersion("0.1.0"), "0.1.1");
  assert.equal(nextPatchVersion("1.9.9"), "1.9.10");
});

test("updates one version marker without changing surrounding text", () => {
  assert.equal(replaceVersion('{"version":"0.1.0"}', "0.1.0", "0.1.1", "fixture"), '{"version":"0.1.1"}');
  assert.throws(() => replaceVersion('{"version":"0.1.0","other":"0.1.0"}', "0.1.0", "0.1.1", "fixture"));
});

test("application version updates do not require an installer version change", () => {
  const installer = { schemaVersion: 1, installerVersion: "1.0.0" };
  assert.equal(nextPatchVersion("0.1.2"), "0.1.3");
  assert.equal(installer.installerVersion, "1.0.0");
});

test("desktop packaging keeps the stable installer independent and thin", () => {
  const app = JSON.parse(readFileSync(new URL("../app/package.json", import.meta.url), "utf8"));
  const installer = JSON.parse(readFileSync(new URL("../app/installer-versions.json", import.meta.url), "utf8"));
  const tauri = JSON.parse(readFileSync(new URL("../app/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(app.version, "0.1.4");
  assert.equal(installer.installerVersion, "1.0.1");
  assert.equal(installer.releaseTag, "v0.1.4");
  assert.equal(tauri.version, installer.installerVersion);
  assert.deepEqual(Object.keys(tauri.bundle.resources).sort(), [
    "../../LICENSES/Apache-2.0.txt",
    "../../THIRD_PARTY_NOTICES.md",
    "resources/bootstrap.json",
  ]);
});

test("launcher event subscriptions are covered by the default capability", () => {
  const capability = JSON.parse(readFileSync(new URL("../app/src-tauri/capabilities/default.json", import.meta.url), "utf8"));
  const runtime = readFileSync(new URL("../app/src-tauri/src/lib.rs", import.meta.url), "utf8");
  const launcher = readFileSync(new URL("../app/src/launcher.ts", import.meta.url), "utf8");
  assert.match(runtime, /WebviewWindowBuilder::new\(app, "launcher"/);
  assert.ok(capability.windows.includes("launcher"));
  assert.ok(capability.permissions.includes("core:default"));
  assert.match(launcher, /listen<LauncherStatus>\("launcher-status"/);
});

test("split launcher and manager are explicit build outputs", () => {
  const vite = readFileSync(new URL("../app/vite.config.ts", import.meta.url), "utf8");
  const cargo = readFileSync(new URL("../app/src-tauri/Cargo.toml", import.meta.url), "utf8");
  const pipeline = readFileSync(new URL("./windows-pipeline.mjs", import.meta.url), "utf8");
  assert.match(vite, /launcher:\s*resolve/);
  assert.match(cargo, /^default-run\s*=\s*"tauri-codex"$/m);
  assert.match(pipeline, /--bin",\s*"tauri-codex-manager"/);
  assert.match(pipeline, /writeFileSync\(bootstrapResource[\s\S]*tauri",\s*"--",\s*"bundle"/);
});
