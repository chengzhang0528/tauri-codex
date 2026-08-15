import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import test from "node:test";
import { dirtyWorktreeMessage, selfUseEnvelope, verifySelfUseEnvelope, withRestoredFileBytes } from "./windows-pipeline.mjs";
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

test("schema v3 makes the unsigned self-use policy explicit", () => {
  const payload = { z: [3, { b: true, a: null }], a: "value" };
  const envelope = selfUseEnvelope(payload);
  assert.deepEqual(envelope, { schemaVersion: 3, releaseMode: "self-use", payload });
  assert.deepEqual(verifySelfUseEnvelope(envelope), payload);
  assert.throws(() => verifySelfUseEnvelope({ ...envelope, schemaVersion: 2 }), /identity/);
  assert.throws(() => verifySelfUseEnvelope({ ...envelope, releaseMode: "production" }), /identity/);
  assert.throws(() => verifySelfUseEnvelope({ ...envelope, unexpected: true }), /identity/);
});

test("frozen source diagnostics identify every dirty path without file contents", () => {
  const status = " M app/src-tauri/resources/bootstrap.json\n?? app/generated/output.txt";
  const message = dirtyWorktreeMessage(status);
  assert.match(message, / M app\/src-tauri\/resources\/bootstrap\.json/);
  assert.match(message, /\?\? app\/generated\/output\.txt/);
  assert.doesNotMatch(message, /schemaVersion|releaseMode/);
  assert.equal(dirtyWorktreeMessage("\n"), null);
});

test("Tauri manifest rewrites restore the exact original bytes on failure", () => {
  const root = mkdtempSync(path.join(tmpdir(), "tauri-codex-manifest-"));
  const manifest = path.join(root, "Cargo.toml");
  const original = Buffer.from("[package]\r\nname = \"tauri-codex\"\r\n", "utf8");
  try {
    writeFileSync(manifest, original);
    assert.throws(() => withRestoredFileBytes(manifest, () => {
      writeFileSync(manifest, "[package]\nname = \"rewritten\"\n", "utf8");
      throw new Error("bundle failed");
    }), /bundle failed/);
    assert.deepEqual(readFileSync(manifest), original);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
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
  assert.equal(pipeline.match(/withRestoredFileBytes\(cargoManifest/g)?.length, 2);
  assert.ok(pipeline.indexOf("doctorFinalComponentArchives(managerArchive, codexArchive);") < pipeline.indexOf("const payload = {"));
  assert.match(pipeline, /tauri-codex-manager\.exe"\), \["--runtime-check"\]/);
  assert.match(pipeline, /codexEntry, "--version"/);
  assert.match(pipeline, /rgEntry, \["--version"\]/);
  assert.match(pipeline, /managerSource, \["--verify-authenticode", filePath\]/);
  assert.match(pipeline, /managerSource, \["--verify-codex-component", root\]/);
  assert.doesNotMatch(pipeline, /signtool|findSignTool/i);
  assert.doesNotMatch(pipeline, /Get-AuthenticodeSignature|Microsoft\.PowerShell\.Security/);
  assert.doesNotMatch(pipeline, /probePublishedInstaller\(\)[\s\S]{0,800}catch\s*\{/);
  assert.match(managerMain, /compile_error!/);
});

test("Launcher owns doctor, hidden Manager launch, automatic staging, and Named Pipe IPC", () => {
  const broker = readFileSync(new URL("../app/src-tauri/src/delivery/broker.rs", import.meta.url), "utf8");
  const health = readFileSync(new URL("../app/src-tauri/src/delivery/health.rs", import.meta.url), "utf8");
  const ipc = readFileSync(new URL("../app/src-tauri/src/delivery/ipc.rs", import.meta.url), "utf8");
  const launcher = readFileSync(new URL("../app/src-tauri/src/lib.rs", import.meta.url), "utf8");
  assert.match(health, /root\.join\("WebView2Loader\.dll"\)/);
  assert.match(health, /verify_authenticode\(&root\.join\("WebView2Loader\.dll"\)\)/);
  assert.match(health, /doctor_codex[\s\S]*verify_codex_executable_provenance\(root\)/);
  assert.match(health, /CODEX_PACKAGE_EXECUTABLES[\s\S]*codex-path\/rg\.exe", false/);
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
  assert.match(workflow, /- candidate\s+- publish\s+- finalize\s+- rollback/);
  assert.match(workflow, /candidate-build:[\s\S]*if: inputs\.operation == 'candidate'/);
  assert.match(workflow, /publish-stage:[\s\S]*needs: \[resolve, publish-preflight\]/);
  assert.match(workflow, /publish-commit:[\s\S]*needs: \[resolve, publish-stage\]/);
  assert.match(workflow, /finalize-release:[\s\S]*if: inputs\.operation == 'finalize'/);
  assert.match(workflow, /rollback-bootstrap:[\s\S]*if: inputs\.operation == 'rollback'/);
  assert.match(workflow, /run-id: \$\{\{ inputs\.candidate_run_id \}\}/);
  assert.match(workflow, /publish:release:oss -- snapshot/);
  assert.match(workflow, /publish:release:oss -- confirm/);
  assert.match(workflow, /publish:release:oss -- rollback/);
  for (const secret of [
    "TAURI_CODEX_AUTHENTICODE_PFX_BASE64",
    "TAURI_CODEX_AUTHENTICODE_PFX_PASSWORD",
    "TAURI_CODEX_RELEASE_KEY_ID",
    "TAURI_CODEX_RELEASE_PRIVATE_KEY",
    "TAURI_CODEX_RELEASE_PUBLIC_KEY",
    "TAURI_CODEX_AUTHENTICODE_TIMESTAMP_URL",
  ]) assert.doesNotMatch(workflow, new RegExp(secret));
  assert.match(workflow, /ALIYUN_OSS_ACCESS_KEY_ID/);
  assert.match(workflow, /shared-public-assets\.oss-cn-beijing\.aliyuncs\.com\/project-tauri-codex/);
  assert.doesNotMatch(workflow, /^\s+files:/m);
  assert.doesNotMatch(workflow, /Publish GitHub Release assets/);
});

test("public and human documentation match canonical versions and OSS-only delivery", () => {
  const app = JSON.parse(readFileSync(new URL("../app/package.json", import.meta.url), "utf8"));
  const installer = JSON.parse(readFileSync(new URL("../app/installer-versions.json", import.meta.url), "utf8"));
  const readme = readFileSync(new URL("../README.md", import.meta.url), "utf8");
  const guide = readFileSync(new URL("../人类-文档/开发/构建Windows桌面应用.md", import.meta.url), "utf8");
  const decision = readFileSync(new URL("../文档/项目/项目_tauri-codex/决策/DEC-0001-Windows-x64-Codex桌面封装方案.md", import.meta.url), "utf8");
  assert.match(readme, new RegExp(`当前源码候选版本为 \`v${app.version.replaceAll(".", "\\.")}\``));
  assert.match(readme, new RegExp(`project-tauri-codex/installers/${installer.installerVersion.replaceAll(".", "\\.")}/windows-x64/tauri-codex_${installer.installerVersion.replaceAll(".", "\\.")}_x64-setup\\.exe`));
  assert.doesNotMatch(readme, /\[下载[^\]]*Windows[^\]]*\]\(https:\/\/github\.com\/[^)]+\/releases/i);
  assert.doesNotMatch(readme, /暂存 GitHub Releases/);
  assert.match(guide, new RegExp(`\\.codex-build/build/${app.version.replaceAll(".", "\\.")}/windows-x64/tauri-codex\\.exe`));
  assert.match(guide, new RegExp(`\\.codex-build/releases/${app.version.replaceAll(".", "\\.")}/windows-x64/`));
  assert.doesNotMatch(guide, /推送[^。\n]*tag|首次发布 tag/);
  assert.match(decision, /setup-required/);
  assert.doesNotMatch(decision, /自动准备完整 release 或独立 Installer|Launcher\/Installer 更新运行/);
});

test("runtime and tooling contain no legacy delivery owner or fallback", () => {
  const deliveryRoot = new URL("../app/src-tauri/src/delivery/", import.meta.url);
  const delivery = readdirSync(deliveryRoot)
    .filter((name) => name.endsWith(".rs"))
    .map((name) => readFileSync(new URL(name, deliveryRoot), "utf8"))
    .join("\n");
  const publisher = readFileSync(new URL("./oss-release.mjs", import.meta.url), "utf8");
  const seed = JSON.parse(readFileSync(new URL("../app/src-tauri/resources/bootstrap.json", import.meta.url), "utf8"));
  assert.equal(seed.schemaVersion, 3);
  assert.equal(seed.releaseMode, "self-use");
  assert.equal(seed.payload.release.manifest.provenance, "self-use+sha256");
  assert.doesNotMatch(delivery, /github\.com|npm install|installer@version|"previous"/i);
  assert.doesNotMatch(publisher, /github\.com|retireRelease/);
  assert.equal(existsSync(new URL("../app/src-tauri/src/thin.rs", import.meta.url)), false);
  assert.equal(existsSync(new URL("../app/src-tauri/src/updates.rs", import.meta.url)), false);
  assert.equal(existsSync(new URL("../.github/workflows/retire-windows-release.yml", import.meta.url)), false);
});
