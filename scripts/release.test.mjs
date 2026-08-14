import assert from "node:assert/strict";
import { generateKeyPairSync } from "node:crypto";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import test from "node:test";
import { canonicalJson, signEnvelope, verifyEnvelope } from "./windows-pipeline.mjs";
import { nextPatchVersion, replaceVersion } from "./release.mjs";

test("increments a stable patch version", () => {
  assert.equal(nextPatchVersion("0.2.0"), "0.2.1");
  assert.equal(nextPatchVersion("1.9.9"), "1.9.10");
});

test("updates one version marker without changing surrounding text", () => {
  assert.equal(replaceVersion('{"version":"0.2.0"}', "0.2.0", "0.2.1", "fixture"), '{"version":"0.2.1"}');
  assert.throws(() => replaceVersion('{"version":"0.2.0","other":"0.2.0"}', "0.2.0", "0.2.1", "fixture"));
});

test("Manager and Installer versions have independent canonical owners", () => {
  const app = JSON.parse(readFileSync(new URL("../app/package.json", import.meta.url), "utf8"));
  const lock = JSON.parse(readFileSync(new URL("../app/package-lock.json", import.meta.url), "utf8"));
  const cargo = readFileSync(new URL("../app/src-tauri/Cargo.toml", import.meta.url), "utf8");
  const installer = JSON.parse(readFileSync(new URL("../app/installer-versions.json", import.meta.url), "utf8"));
  const tauri = JSON.parse(readFileSync(new URL("../app/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(app.version, "0.2.0");
  assert.equal(lock.version, app.version);
  assert.equal(lock.packages[""].version, app.version);
  assert.match(cargo, new RegExp(`^version = "${app.version.replaceAll(".", "\\.")}"$`, "m"));
  assert.deepEqual(installer, { schemaVersion: 2, installerVersion: "1.1.0", minimumManagerVersion: "0.2.0", publishedArtifact: null });
  assert.equal(tauri.version, installer.installerVersion);
  assert.equal(installer.minimumManagerVersion, "0.2.0");
  assert.deepEqual(Object.keys(tauri.bundle.resources).sort(), [
    "../../LICENSES/Apache-2.0.txt",
    "../../THIRD_PARTY_NOTICES.md",
    "resources/bootstrap.json",
  ]);
});

test("schema v2 canonical JSON and Ed25519 envelopes reject mutation", () => {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const trust = { keyId: "test-key", privateKey, publicKey };
  const payload = { z: [3, { b: true, a: null }], a: "value" };
  assert.equal(canonicalJson(payload), '{"a":"value","z":[3,{"a":null,"b":true}]}');
  const envelope = signEnvelope(payload, trust);
  assert.deepEqual(verifyEnvelope(envelope, trust), payload);
  envelope.payload.a = "mutated";
  assert.throws(() => verifyEnvelope(envelope, trust), /signature/);
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

test("split Launcher and Manager are explicit clean build outputs", () => {
  const vite = readFileSync(new URL("../app/vite.config.ts", import.meta.url), "utf8");
  const cargo = readFileSync(new URL("../app/src-tauri/Cargo.toml", import.meta.url), "utf8");
  const managerMain = readFileSync(new URL("../app/src-tauri/src/manager_main.rs", import.meta.url), "utf8");
  const pipeline = readFileSync(new URL("./windows-pipeline.mjs", import.meta.url), "utf8");
  assert.match(vite, /launcher:\s*resolve/);
  assert.match(cargo, /^default-run\s*=\s*"tauri-codex"$/m);
  assert.match(cargo, /^required-features\s*=\s*\["custom-protocol"\]$/m);
  assert.match(pipeline, /CARGO_TARGET_DIR:\s*releaseCargoRoot/);
  assert.match(pipeline, /rmSync\(releaseCargoRoot, \{ recursive: true, force: true \}\)/);
  assert.match(pipeline, /--bin",\s*"tauri-codex-manager"/);
  assert.match(pipeline, /WebView2Loader\.dll/);
  assert.match(pipeline, /dist", "launcher\.html"/);
  assert.match(pipeline, /status === "404"/);
  assert.match(pipeline, /minimumManagerVersion:\s*minimumManagerVersion/);
  assert.doesNotMatch(pipeline, /minimumManagerVersion:\s*appVersion/);
  assert.match(pipeline, /doctorFinalComponentArchives\(managerArchive, codexArchive\)/);
  assert.ok(pipeline.indexOf("doctorFinalComponentArchives(managerArchive, codexArchive);") < pipeline.indexOf("const payload = {"));
  assert.match(pipeline, /tauri-codex-manager\.exe"\), \["--runtime-check"\]/);
  assert.match(pipeline, /codexEntry, "--version"/);
  assert.doesNotMatch(pipeline, /probePublishedInstaller\(\)[\s\S]{0,800}catch\s*\{/);
  assert.match(managerMain, /compile_error!/);
});

test("Launcher owns doctor, hidden Manager launch, automatic staging, and Named Pipe IPC", () => {
  const broker = readFileSync(new URL("../app/src-tauri/src/delivery/broker.rs", import.meta.url), "utf8");
  const health = readFileSync(new URL("../app/src-tauri/src/delivery/health.rs", import.meta.url), "utf8");
  const ipc = readFileSync(new URL("../app/src-tauri/src/delivery/ipc.rs", import.meta.url), "utf8");
  const launcher = readFileSync(new URL("../app/src-tauri/src/lib.rs", import.meta.url), "utf8");
  assert.match(health, /root\.join\("WebView2Loader\.dll"\)/);
  assert.match(health, /verify_authenticode_tree\(root\)/);
  assert.match(broker, /automatic_cycle\(&automatic\)/);
  assert.match(broker, /UpdateIntent::Prepare/);
  assert.match(broker, /UpdateState::SetupRequired/);
  assert.doesNotMatch(broker, /stage_installer|verify_staged_installer/);
  assert.doesNotMatch(broker, /background_command\(installer\)/);
  assert.match(broker, /job::background_command\(&manager\)/);
  assert.match(ipc, /PIPE_REJECT_REMOTE_CLIENTS/);
  assert.match(ipc, /D:P\(A;;GA;;;/);
  assert.match(ipc, /acquire_instance/);
  assert.ok(launcher.indexOf("acquire_launcher_instance") < launcher.indexOf("current_release_ready_for_launcher"));
  assert.match(broker, /_instance: ipc::InstanceGuard/);
  assert.doesNotMatch(broker, /startup_bridge|read_bootstrap_remote_for_startup|manager_bridge_required/);
});

test("release workflow commits OSS before creating OSS-only GitHub Release Notes", () => {
  const workflow = readFileSync(new URL("../.github/workflows/windows-release.yml", import.meta.url), "utf8");
  assert.match(workflow, /workflow_dispatch:/);
  assert.doesNotMatch(workflow, /\n\s+push:/);
  assert.match(workflow, /build-once:[\s\S]*needs: \[oss-preflight\]/);
  assert.match(workflow, /oss-stage:[\s\S]*needs: \[build-once\]/);
  assert.match(workflow, /oss-commit:[\s\S]*needs: \[build-once, oss-stage\]/);
  assert.match(workflow, /github-release-notes:[\s\S]*needs: \[build-once, oss-commit\]/);
  assert.match(workflow, /TAURI_CODEX_RELEASE_PRIVATE_KEY/);
  assert.match(workflow, /TAURI_CODEX_AUTHENTICODE_PFX_BASE64/);
  assert.match(workflow, /shared-public-assets\.oss-cn-beijing\.aliyuncs\.com\/project-tauri-codex/);
  assert.doesNotMatch(workflow, /^\s+files:/m);
  assert.doesNotMatch(workflow, /Publish GitHub Release assets/);
});

test("runtime and tooling contain no legacy delivery owner or fallback", () => {
  const deliveryRoot = new URL("../app/src-tauri/src/delivery/", import.meta.url);
  const delivery = readdirSync(deliveryRoot)
    .filter((name) => name.endsWith(".rs"))
    .map((name) => readFileSync(new URL(name, deliveryRoot), "utf8"))
    .join("\n");
  const publisher = readFileSync(new URL("./oss-release.mjs", import.meta.url), "utf8");
  const seed = JSON.parse(readFileSync(new URL("../app/src-tauri/resources/bootstrap.json", import.meta.url), "utf8"));
  assert.equal(seed.schemaVersion, 2);
  assert.equal(seed.keyId, "development-rfc8032");
  assert.doesNotMatch(delivery, /github\.com|npm install|installer@version|"previous"/i);
  assert.doesNotMatch(publisher, /github\.com|retireRelease/);
  assert.equal(existsSync(new URL("../app/src-tauri/src/thin.rs", import.meta.url)), false);
  assert.equal(existsSync(new URL("../app/src-tauri/src/updates.rs", import.meta.url)), false);
  assert.equal(existsSync(new URL("../.github/workflows/retire-windows-release.yml", import.meta.url)), false);
});
