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
  const lock = JSON.parse(readFileSync(new URL("../app/package-lock.json", import.meta.url), "utf8"));
  const cargo = readFileSync(new URL("../app/src-tauri/Cargo.toml", import.meta.url), "utf8");
  const installer = JSON.parse(readFileSync(new URL("../app/installer-versions.json", import.meta.url), "utf8"));
  const tauri = JSON.parse(readFileSync(new URL("../app/src-tauri/tauri.conf.json", import.meta.url), "utf8"));
  assert.equal(lock.version, app.version);
  assert.equal(lock.packages[""].version, app.version);
  assert.match(cargo, new RegExp(`^version = "${app.version.replaceAll(".", "\\.")}"$`, "m"));
  assert.equal(installer.installerVersion, "1.0.4");
  assert.equal(installer.releaseTag, "v0.1.9");
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
  const managerMain = readFileSync(new URL("../app/src-tauri/src/manager_main.rs", import.meta.url), "utf8");
  const pipeline = readFileSync(new URL("./windows-pipeline.mjs", import.meta.url), "utf8");
  assert.match(vite, /launcher:\s*resolve/);
  assert.match(cargo, /^default-run\s*=\s*"tauri-codex"$/m);
  assert.match(cargo, /^custom-protocol\s*=\s*\["tauri\/custom-protocol"\]$/m);
  assert.match(cargo, /^required-features\s*=\s*\["custom-protocol"\]$/m);
  assert.match(pipeline, /--bin",\s*"tauri-codex-manager"/);
  assert.match(pipeline, /--features",\s*"custom-protocol"/);
  assert.match(pipeline, /WebView2Loader\.dll/);
  assert.match(pipeline, /verifyManagerArchive\(managerArchive\)/);
  assert.match(managerMain, /not\(feature = "custom-protocol"\)/);
  assert.match(managerMain, /compile_error!/);
  assert.match(pipeline, /writeFileSync\(bootstrapResource[\s\S]*tauri",\s*"--",\s*"bundle"/);
});

test("manager doctor and launch use the complete runtime directory", () => {
  const thin = readFileSync(new URL("../app/src-tauri/src/thin.rs", import.meta.url), "utf8");
  assert.match(thin, /root\.join\("WebView2Loader\.dll"\)/);
  assert.match(thin, /\.arg\("--runtime-check"\)[\s\S]*\.current_dir\(root\)/);
  assert.match(thin, /Command::new\(&manager\)[\s\S]*\.current_dir\(manager\.parent\(\)/);
});

test("release admits and stages OSS before GitHub publication, then commits Bootstrap last", () => {
  const workflow = readFileSync(new URL("../.github/workflows/windows-release.yml", import.meta.url), "utf8");
  const pipeline = readFileSync(new URL("./windows-pipeline.mjs", import.meta.url), "utf8");
  assert.match(workflow, /oss-preflight:[\s\S]*environment: oss-release[\s\S]*publish:release:oss -- preflight/);
  assert.match(workflow, /oss-stage:[\s\S]*needs: \[oss-preflight, build\][\s\S]*publish:release:oss -- stage/);
  assert.match(workflow, /github-release:[\s\S]*needs: \[build, oss-stage\][\s\S]*Publish GitHub Release assets/);
  assert.match(workflow, /oss-commit:[\s\S]*needs: \[build, github-release\][\s\S]*publish:release:oss -- commit/);
  assert.match(workflow, /environment: oss-release/);
  assert.match(workflow, /ALIYUN_OSS_ACCESS_KEY_ID: \$\{\{ secrets\.ALIYUN_OSS_ACCESS_KEY_ID \}\}/);
  assert.match(pipeline, /objectKey/);
  assert.match(pipeline, /releases\/\$\{appVersion\}\/windows-x64\/components/);
  assert.match(pipeline, /installers\/\$\{installerVersion\}\/windows-x64/);
});
