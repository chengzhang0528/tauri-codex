import { copyFileSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createHash, createPrivateKey, createPublicKey, randomUUID, sign, verify } from "node:crypto";
import { spawnSync } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(scriptRoot, "..");
const appRoot = path.join(workspaceRoot, "app");
const appScript = path.join(appRoot, "scripts", "run-tauri-windows.mjs");
const versions = JSON.parse(readFileSync(path.join(appRoot, "build-versions.json"), "utf8"));
const installerVersions = JSON.parse(readFileSync(path.join(appRoot, "installer-versions.json"), "utf8"));
const appPackage = JSON.parse(readFileSync(path.join(appRoot, "package.json"), "utf8"));
const appVersion = appPackage.version;
const installerVersion = installerVersions.installerVersion;
const minimumManagerVersion = installerVersions.minimumManagerVersion;
const buildRoot = path.join(workspaceRoot, ".codex-build");
const releaseRoot = path.join(buildRoot, "releases", appVersion, "windows-x64");
const componentRoot = path.join(releaseRoot, "components");
const releaseCargoRoot = path.join(buildRoot, "cargo-release");
const targetRoot = path.join(releaseCargoRoot, versions.rustTarget, "release");
const launcherSource = path.join(targetRoot, "tauri-codex.exe");
const managerSource = path.join(targetRoot, "tauri-codex-manager.exe");
const webviewLoaderSource = path.join(targetRoot, "WebView2Loader.dll");
const installerSource = path.join(targetRoot, "bundle", "nsis", `tauri-codex_${installerVersion}_x64-setup.exe`);
const installerOutput = path.join(releaseRoot, path.basename(installerSource));
const manifestOutput = path.join(releaseRoot, "manifest.json");
const bootstrapOutput = path.join(releaseRoot, "bootstrap.json");
const bootstrapResource = path.join(appRoot, "src-tauri", "resources", "bootstrap.json");
const candidateOutput = path.join(releaseRoot, "candidate.json");
const OSS_ROOT = "https://shared-public-assets.oss-cn-beijing.aliyuncs.com/project-tauri-codex";
const MANAGER_FILES = ["tauri-codex-manager.exe", "WebView2Loader.dll"];

export function canonicalJson(value) {
  if (value === null || typeof value !== "object") return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(",")}]`;
  return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(",")}}`;
}

export function signEnvelope(payload, { keyId, privateKey }) {
  const signature = sign(null, Buffer.from(canonicalJson(payload)), privateKey).toString("base64");
  return { schemaVersion: 2, keyId, payload, signature };
}

export function verifyEnvelope(envelope, { keyId, publicKey }) {
  if (envelope?.schemaVersion !== 2 || envelope.keyId !== keyId) throw new Error("signed envelope identity 不匹配");
  if (!verify(null, Buffer.from(canonicalJson(envelope.payload)), publicKey, Buffer.from(envelope.signature, "base64"))) throw new Error("Ed25519 signature 校验失败");
  return envelope.payload;
}

function fail(message) { throw new Error(message); }

function run(command, args, options = {}) {
  const result = spawnSync(command, args, { cwd: options.cwd ?? workspaceRoot, env: options.env ?? process.env, stdio: options.capture ? ["ignore", "pipe", "pipe"] : "inherit", encoding: "utf8", windowsHide: true });
  if (result.error) fail(`${command} unavailable: ${result.error.message}`);
  if (result.status !== 0) fail(`${path.basename(command)} exited with ${result.status}${options.capture && result.stderr ? `: ${result.stderr.trim()}` : ""}`);
  return result;
}

function gitOutput(args) {
  return run("git", ["-C", workspaceRoot, ...args], { capture: true }).stdout.trim();
}

function frozenSource() {
  if (gitOutput(["status", "--porcelain"])) fail("生产候选必须从 clean Git worktree 构建。");
  const commit = gitOutput(["rev-parse", "HEAD"]);
  if (!/^[a-f0-9]{40}$/.test(commit)) fail("无法固定 source commit。");
  return commit;
}

function assertFrozenSource(expectedCommit) {
  const actual = frozenSource();
  if (actual !== expectedCommit) fail(`构建期间 source commit 已变化：${expectedCommit} -> ${actual}`);
}

function npmCommand() {
  const cli = process.env.npm_execpath;
  if (!cli) fail("必须通过 npm run 调用根脚本，以便定位 npm CLI。");
  return { node: process.env.npm_node_execpath ?? process.execPath, cli };
}

function runNpm(args, env) { const { node, cli } = npmCommand(); return run(node, [cli, ...args], { env }); }

function toolchainEnvironment() {
  if (process.platform !== "win32") fail("Windows 构建脚本只能在 Windows 上运行。");
  const cargoBin = process.env.USERPROFILE ? path.join(process.env.USERPROFILE, ".cargo", "bin") : "";
  const rustup = cargoBin ? path.join(cargoBin, "rustup.exe") : "";
  const candidates = [process.env.TAURI_MINGW_BIN, process.env.LOCALAPPDATA && path.join(process.env.LOCALAPPDATA, "Programs", "msys64", "ucrt64", "bin"), "C:\\msys64\\ucrt64\\bin", "C:\\Program Files\\msys64\\ucrt64\\bin"].filter(Boolean);
  const mingwBin = candidates.find((candidate) => ["windres.exe", "gcc.exe", "ar.exe"].every((name) => existsSync(path.join(candidate, name))));
  if (!existsSync(rustup)) fail("未找到 Rustup。");
  if (!mingwBin) fail("未找到 MSYS2 UCRT64 工具链。");
  const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";
  const env = { ...process.env, RUSTUP_TOOLCHAIN: versions.rustToolchain, npm_config_cache: path.join(buildRoot, "cache", "npm"), npm_config_prefer_offline: "true", [pathKey]: [mingwBin, cargoBin, process.env[pathKey]].filter(Boolean).join(";") };
  mkdirSync(path.join(buildRoot, "cache"), { recursive: true });
  const installed = run(rustup, ["target", "list", "--installed", "--toolchain", versions.rustToolchain], { env, capture: true }).stdout.split(/\r?\n/);
  if (!installed.includes(versions.rustTarget)) fail(`Rust target ${versions.rustTarget} 未安装。`);
  return { env, rustup };
}

function releaseSigning() {
  const keyId = process.env.TAURI_CODEX_RELEASE_KEY_ID?.trim();
  const privateBytes = process.env.TAURI_CODEX_RELEASE_PRIVATE_KEY?.trim();
  const publicBytes = process.env.TAURI_CODEX_RELEASE_PUBLIC_KEY?.trim();
  if (!keyId || !privateBytes || !publicBytes) fail("生产候选缺少 TAURI_CODEX_RELEASE_KEY_ID/PRIVATE_KEY/PUBLIC_KEY。");
  let privateKey;
  try { privateKey = createPrivateKey({ key: Buffer.from(privateBytes, "base64"), format: "der", type: "pkcs8" }); } catch (error) { fail(`Ed25519 private key 无效：${error.message}`); }
  const derived = createPublicKey(privateKey).export({ format: "der", type: "spki" }).subarray(-32);
  if (!derived.equals(Buffer.from(publicBytes, "base64"))) fail("Ed25519 public key 与 private key 不匹配。");
  const publicKey = createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), derived]), format: "der", type: "spki" });
  return { keyId, privateKey, publicKey, publicRaw: publicBytes };
}

function publicSigning() {
  const keyId = process.env.TAURI_CODEX_RELEASE_KEY_ID?.trim();
  const bytes = Buffer.from(process.env.TAURI_CODEX_RELEASE_PUBLIC_KEY?.trim() ?? "", "base64");
  if (!keyId || bytes.length !== 32) fail("候选验证缺少可信 Ed25519 key ID/public key。");
  return { keyId, publicKey: createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), bytes]), format: "der", type: "spki" }) };
}

function findSignTool() {
  if (process.env.TAURI_CODEX_SIGNTOOL && existsSync(process.env.TAURI_CODEX_SIGNTOOL)) return process.env.TAURI_CODEX_SIGNTOOL;
  const result = spawnSync("where.exe", ["signtool.exe"], { encoding: "utf8", windowsHide: true });
  const found = result.status === 0 ? result.stdout.split(/\r?\n/).map((line) => line.trim()).find(Boolean) : undefined;
  if (!found) fail("未找到 signtool.exe。");
  return found;
}

function signAuthenticode(filePath) {
  const thumbprint = process.env.TAURI_CODEX_AUTHENTICODE_THUMBPRINT?.trim();
  const timestamp = process.env.TAURI_CODEX_AUTHENTICODE_TIMESTAMP_URL?.trim();
  if (!thumbprint || !timestamp) fail("生产候选缺少 Authenticode thumbprint 或 timestamp URL。");
  const signtool = findSignTool();
  run(signtool, ["sign", "/sha1", thumbprint, "/fd", "SHA256", "/tr", timestamp, "/td", "SHA256", filePath]);
  verifyAuthenticode(filePath);
}

function verifyAuthenticode(filePath) { run(findSignTool(), ["verify", "/pa", "/all", filePath]); }
function ensureAuthenticode(filePath) {
  const verified = spawnSync(findSignTool(), ["verify", "/pa", "/all", filePath], { stdio: "ignore", windowsHide: true });
  if (verified.status !== 0) signAuthenticode(filePath);
}
function filesBelow(root) {
  const files = [];
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    const resolved = path.join(root, entry.name);
    if (entry.isDirectory()) files.push(...filesBelow(resolved));
    else if (entry.isFile()) files.push(resolved);
    else fail(`候选资源不允许链接或特殊文件：${resolved}`);
  }
  return files;
}
function sha256(filePath) { return createHash("sha256").update(readFileSync(filePath)).digest("hex"); }
export function installedTreeSha256(root, selectedFiles = filesBelow(root)) {
  const entries = selectedFiles.map((filePath) => {
    const metadata = lstatSync(filePath);
    if (!metadata.isFile() || metadata.isSymbolicLink()) fail(`安装树不允许链接或特殊文件：${filePath}`);
    const relative = path.relative(root, filePath);
    if (!relative || relative.startsWith(`..${path.sep}`) || path.isAbsolute(relative)) fail(`安装树文件越界：${filePath}`);
    return { relative: relative.replaceAll(path.sep, "/").toLowerCase(), filePath, size: metadata.size };
  }).sort((left, right) => left.relative < right.relative ? -1 : left.relative > right.relative ? 1 : 0);
  for (let index = 1; index < entries.length; index += 1) if (entries[index - 1].relative === entries[index].relative) fail(`安装树包含大小写冲突路径：${entries[index].relative}`);
  const tree = createHash("sha256");
  for (const entry of entries) tree.update(entry.relative).update("\0").update(String(entry.size)).update("\0").update(sha256(entry.filePath)).update("\n");
  return tree.digest("hex");
}
function artifactRecord(filePath) { return { path: path.relative(workspaceRoot, filePath).replaceAll(path.sep, "/"), size: statSync(filePath).size, sha256: sha256(filePath) }; }
function objectArtifact(filePath, objectKey, provenance) { const measured = artifactRecord(filePath); return { objectKey, size: measured.size, sha256: measured.sha256, provenance }; }
function componentKey(name) { return `releases/${appVersion}/windows-x64/components/${name}`; }
function releaseKey(name) { return `releases/${appVersion}/windows-x64/${name}`; }
function installerKey(name) { return `installers/${installerVersion}/windows-x64/${name}`; }
function writeJson(filePath, value) { mkdirSync(path.dirname(filePath), { recursive: true }); writeFileSync(filePath, `${JSON.stringify(value, null, 2)}\n`, "utf8"); }

function createArchive(output, cwd, entries) {
  const result = spawnSync("tar.exe", ["-a", "-c", "-f", output, "-C", cwd, ...entries], { stdio: "inherit", windowsHide: true });
  if (result.error || result.status !== 0) fail(`无法创建归档 ${output}`);
}

function archiveEntries(filePath) {
  const result = run("tar.exe", ["-tf", filePath], { capture: true });
  return result.stdout.split(/\r?\n/).map((entry) => entry.replaceAll("\\", "/").replace(/^\.\//, "")).filter(Boolean).sort();
}

function verifyManagerArchive(filePath) {
  const actual = archiveEntries(filePath);
  const expected = [...MANAGER_FILES].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) fail(`Manager archive 不完整：${actual.join(", ")}`);
}

function verifyArchiveTree(filePath, expectedDigest) {
  const root = path.join(buildRoot, `verify-tree-${randomUUID()}`);
  mkdirSync(root, { recursive: true });
  try {
    run("tar.exe", ["-xf", filePath, "-C", root]);
    if (installedTreeSha256(root) !== expectedDigest) fail(`archive 安装树摘要不匹配：${filePath}`);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function doctorFinalComponentArchives(managerArchive, codexArchive) {
  const root = path.join(buildRoot, `candidate-doctor-${randomUUID()}`);
  const managerRoot = path.join(root, "manager");
  const codexRoot = path.join(root, "codex");
  const doctorHome = path.join(root, "codex-home");
  mkdirSync(managerRoot, { recursive: true });
  mkdirSync(codexRoot, { recursive: true });
  mkdirSync(doctorHome, { recursive: true });
  try {
    run("tar.exe", ["-xf", managerArchive, "-C", managerRoot]);
    run("tar.exe", ["-xf", codexArchive, "-C", codexRoot]);
    run(path.join(managerRoot, "tauri-codex-manager.exe"), ["--runtime-check"], {
      env: { ...process.env, TAURI_CODEX_SYSTEM_NODE: process.execPath },
      capture: true,
    });
    const codexEntry = [
      path.join(codexRoot, "node_modules", "@openai", "codex", "bin", "codex.js"),
      path.join(codexRoot, "node_modules", "@openai", "codex", "bin", "codex"),
      path.join(codexRoot, "node_modules", "@openai", "codex", "dist", "cli.js"),
    ].find((entry) => existsSync(entry));
    if (!codexEntry) fail("最终 Codex component 缺少 CLI 入口。");
    const output = run(process.execPath, [codexEntry, "--version"], {
      cwd: codexRoot,
      env: { ...process.env, CODEX_HOME: doctorHome },
      capture: true,
    }).stdout.trim();
    if (!output.split(/\s+/).includes(versions.codexVersion)) {
      fail(`最终 Codex component 版本不匹配：${output || "无输出"}`);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}

function bootstrap() {
  const toolchain = toolchainEnvironment();
  const dependencies = path.join(appRoot, "node_modules");
  if (!existsSync(dependencies)) runNpm(["--prefix", appRoot, "ci", "--no-audit", "--no-fund"], toolchain.env);
  runNpm(["--prefix", appRoot, "run", "prepare:codex"], toolchain.env);
  runNpm(["--prefix", appRoot, "run", "prepare:node"], toolchain.env);
  console.log(JSON.stringify({ bootstrapped: true, target: versions.rustTarget, codexVersion: versions.codexVersion, nodeVersion: versions.nodeVersion }, null, 2));
}

function buildBinaries(toolchain, signing) {
  const env = { ...toolchain.env, CARGO_TARGET_DIR: releaseCargoRoot, TAURI_CODEX_RELEASE_KEY_ID: signing.keyId, TAURI_CODEX_RELEASE_PUBLIC_KEY: signing.publicRaw };
  run(process.execPath, [appScript, "build", "--no-bundle"], { env });
  run(toolchain.rustup, ["run", versions.rustToolchain, "cargo", "build", "--manifest-path", path.join(appRoot, "src-tauri", "Cargo.toml"), "--release", "--target", versions.rustTarget, "--bin", "tauri-codex-manager", "--features", "custom-protocol"], { env });
  for (const binary of [launcherSource, managerSource]) { if (!existsSync(binary)) fail(`构建产物不存在：${binary}`); signAuthenticode(binary); }
  if (!existsSync(webviewLoaderSource)) fail("Manager 缺少 WebView2Loader.dll。");
  verifyAuthenticode(webviewLoaderSource);
  return env;
}

function prepareComponents(signing) {
  mkdirSync(componentRoot, { recursive: true });
  const codexRoot = path.join(appRoot, "src-tauri", "resources", "codex");
  const nodeMsi = path.join(appRoot, "src-tauri", "resources", "node", `node-v${versions.nodeVersion}-x64.msi`);
  if (!existsSync(path.join(codexRoot, "node_modules", "@openai", "codex", "package.json")) || !existsSync(nodeMsi)) fail("Codex/Node 构建输入不完整，请先运行 bootstrap。");
  const codexExecutables = filesBelow(codexRoot).filter((file) => path.extname(file).toLowerCase() === ".exe");
  if (codexExecutables.length === 0) fail("Codex 构建输入不包含 Windows executable。");
  for (const executable of codexExecutables) ensureAuthenticode(executable);
  verifyAuthenticode(nodeMsi);
  const managerArchive = path.join(componentRoot, `tauri-codex-manager-${appVersion}-windows-x64.zip`);
  const codexArchive = path.join(componentRoot, `tauri-codex-codex-${versions.codexVersion}-windows-x64.zip`);
  const nodeAsset = path.join(componentRoot, path.basename(nodeMsi));
  createArchive(managerArchive, targetRoot, MANAGER_FILES);
  createArchive(codexArchive, codexRoot, ["."]);
  copyFileSync(nodeMsi, nodeAsset);
  verifyManagerArchive(managerArchive);
  const managerTreeSha256 = installedTreeSha256(targetRoot, MANAGER_FILES.map((name) => path.join(targetRoot, name)));
  const codexTreeSha256 = installedTreeSha256(codexRoot);
  verifyArchiveTree(managerArchive, managerTreeSha256);
  verifyArchiveTree(codexArchive, codexTreeSha256);
  doctorFinalComponentArchives(managerArchive, codexArchive);
  const payload = {
    product: "tauri-codex", version: appVersion, platform: "windows", architecture: "x86_64",
    minimumLauncherVersion: installerVersion, minimumManagerVersion: minimumManagerVersion,
    components: [
      { id: "manager", version: appVersion, kind: "archive", archive: "zip", required: true, installPath: "manager", provenance: "authenticode+ed25519", installedTreeSha256: managerTreeSha256, artifact: objectArtifact(managerArchive, componentKey(path.basename(managerArchive)), "authenticode+ed25519") },
      { id: "codex", version: versions.codexVersion, kind: "archive", archive: "zip", required: true, installPath: "codex", provenance: "authenticode+ed25519", installedTreeSha256: codexTreeSha256, artifact: objectArtifact(codexArchive, componentKey(path.basename(codexArchive)), "authenticode+ed25519") },
      { id: "node", version: versions.nodeVersion, kind: "system", archive: "msi", required: true, installPath: "system", provenance: "authenticode+ed25519", installedTreeSha256: null, artifact: objectArtifact(nodeAsset, componentKey(path.basename(nodeAsset)), "authenticode+ed25519") },
    ],
  };
  const envelope = signEnvelope(payload, signing);
  writeJson(manifestOutput, envelope);
  return { managerArchive, codexArchive, nodeAsset, payload, envelope, manifest: objectArtifact(manifestOutput, releaseKey("manifest.json"), "ed25519") };
}

let probedInstaller;

function downloadOptionalInstaller(key, destination) {
  const result = spawnSync("curl.exe", ["--silent", "--show-error", "--output", destination, "--write-out", "%{http_code}", `${OSS_ROOT}/${key}`], { cwd: workspaceRoot, encoding: "utf8", windowsHide: true });
  if (result.error) fail(`curl.exe unavailable: ${result.error.message}`);
  const status = result.stdout.trim();
  if (status === "404") return false;
  if (result.status !== 0) fail(`OSS Installer 探测失败（HTTP ${status || "unknown"}）：${result.stderr.trim()}`);
  if (status !== "200") fail(`OSS Installer 探测返回意外 HTTP ${status || "unknown"}。`);
  return true;
}

function probePublishedInstaller() {
  if (probedInstaller !== undefined) return probedInstaller;
  const key = installerKey(`tauri-codex_${installerVersion}_x64-setup.exe`);
  const downloaded = path.join(releaseRoot, `.probe-installer-${installerVersion}.exe`);
  try {
    if (!downloadOptionalInstaller(key, downloaded)) {
      probedInstaller = null;
      return probedInstaller;
    }
    const measured = artifactRecord(downloaded);
    verifyAuthenticode(downloaded);
    probedInstaller = { objectKey: key, size: measured.size, sha256: measured.sha256, provenance: "authenticode+ed25519" };
  } finally {
    rmSync(downloaded, { force: true });
  }
  return probedInstaller;
}

function shouldBuildInstaller() { return process.env.TAURI_BUILD_INSTALLER === "1" || (!installerVersions.publishedArtifact && !probePublishedInstaller()); }

function publishedInstaller() {
  const artifact = installerVersions.publishedArtifact ?? probePublishedInstaller();
  if (!artifact || artifact.objectKey !== installerKey(`tauri-codex_${installerVersion}_x64-setup.exe`) || !Number.isSafeInteger(artifact.size) || artifact.size <= 0 || !/^[a-f0-9]{64}$/.test(artifact.sha256) || artifact.provenance !== "authenticode+ed25519") fail("installer-versions.json 缺少可复用 OSS Installer identity。");
  const downloaded = path.join(releaseRoot, `.reused-installer-${installerVersion}.exe`);
  try {
    run("curl.exe", ["--fail", "--silent", "--show-error", "--output", downloaded, `${OSS_ROOT}/${artifact.objectKey}`]);
    if (statSync(downloaded).size !== artifact.size || sha256(downloaded) !== artifact.sha256) fail("OSS Installer 匿名回读 identity 不匹配。");
    verifyAuthenticode(downloaded);
  } finally {
    rmSync(downloaded, { force: true });
  }
  return artifact;
}

function buildReleaseCandidate() {
  const signing = releaseSigning();
  const toolchain = toolchainEnvironment();
  const sourceCommit = frozenSource();
  rmSync(releaseRoot, { recursive: true, force: true });
  rmSync(releaseCargoRoot, { recursive: true, force: true });
  mkdirSync(releaseRoot, { recursive: true });
  const buildEnv = buildBinaries(toolchain, signing);
  const components = prepareComponents(signing);
  const seedPayload = { product: "tauri-codex", platform: "windows", architecture: "x86_64", minimumLauncherVersion: installerVersion, installer: null, release: { version: appVersion, manifest: components.manifest } };
  let installer;
  let installerLocalPath = null;
  if (shouldBuildInstaller()) {
    const originalBootstrap = readFileSync(bootstrapResource);
    try {
      writeJson(bootstrapResource, signEnvelope(seedPayload, signing));
      runNpm(["--prefix", appRoot, "run", "tauri", "--", "bundle", "--target", versions.rustTarget, "--bundles", "nsis"], buildEnv);
    } finally {
      writeFileSync(bootstrapResource, originalBootstrap);
    }
    if (!existsSync(installerSource)) fail(`Tauri 未生成 ${installerSource}`);
    for (const binary of [launcherSource, managerSource, webviewLoaderSource]) verifyAuthenticode(binary);
    signAuthenticode(installerSource); copyFileSync(installerSource, installerOutput); installer = objectArtifact(installerOutput, installerKey(path.basename(installerOutput)), "authenticode+ed25519"); installerLocalPath = path.relative(releaseRoot, installerOutput).replaceAll(path.sep, "/");
  } else installer = publishedInstaller();
  const bootstrapPayload = { ...seedPayload, installer: { version: installerVersion, artifact: installer } };
  writeJson(bootstrapOutput, signEnvelope(bootstrapPayload, signing));
  const manifestComponents = components.payload.components;
  const immutable = [
    { role: "manifest", localPath: "manifest.json", artifact: components.manifest },
    { role: "manager", localPath: path.relative(releaseRoot, components.managerArchive).replaceAll(path.sep, "/"), artifact: manifestComponents.find((component) => component.id === "manager").artifact },
    { role: "codex", localPath: path.relative(releaseRoot, components.codexArchive).replaceAll(path.sep, "/"), artifact: manifestComponents.find((component) => component.id === "codex").artifact },
    { role: "node", localPath: path.relative(releaseRoot, components.nodeAsset).replaceAll(path.sep, "/"), artifact: manifestComponents.find((component) => component.id === "node").artifact },
    { role: "installer", localPath: installerLocalPath, artifact: installer },
  ];
  const candidate = { product: "tauri-codex", version: appVersion, installerVersion, platform: "windows", architecture: "x86_64", sourceCommit, bootstrap: { localPath: "bootstrap.json", objectKey: "bootstrap/windows-x64.json", size: statSync(bootstrapOutput).size, sha256: sha256(bootstrapOutput), provenance: "ed25519" }, immutable };
  assertFrozenSource(sourceCommit);
  writeJson(candidateOutput, signEnvelope(candidate, signing));
  console.log(JSON.stringify({ built: true, version: appVersion, installerVersion, installerReused: !installerLocalPath, candidate: artifactRecord(candidateOutput) }, null, 2));
}

function verifyReleaseCandidate() {
  if (!existsSync(candidateOutput)) fail("候选不存在，先运行 installer:build。");
  const trust = publicSigning(); const candidateEnvelope = JSON.parse(readFileSync(candidateOutput, "utf8"));
  const candidate = verifyEnvelope(candidateEnvelope, trust);
  if (candidate.version !== appVersion || candidate.installerVersion !== installerVersion || candidate.sourceCommit !== frozenSource()) fail("candidate identity 与版本源不一致。");
  const manifest = JSON.parse(readFileSync(manifestOutput, "utf8")); const bootstrapEnvelope = JSON.parse(readFileSync(bootstrapOutput, "utf8"));
  const manifestPayload = verifyEnvelope(manifest, trust); const bootstrapPayload = verifyEnvelope(bootstrapEnvelope, trust);
  if (manifestPayload.version !== appVersion || bootstrapPayload.release.version !== appVersion || bootstrapPayload.release.manifest.sha256 !== sha256(manifestOutput) || bootstrapPayload.installer.version !== installerVersion) fail("signed closure 版本或摘要不一致。");
  for (const item of candidate.immutable) {
    if (!item.localPath) continue;
    const filePath = path.join(releaseRoot, item.localPath); if (!existsSync(filePath)) fail(`candidate localPath 缺失：${item.localPath}`);
    if (statSync(filePath).size !== item.artifact.size || sha256(filePath) !== item.artifact.sha256) fail(`candidate bytes 已变化：${item.localPath}`);
  }
  const manager = candidate.immutable.find((item) => item.role === "manager"); verifyManagerArchive(path.join(releaseRoot, manager.localPath));
  const codex = candidate.immutable.find((item) => item.role === "codex");
  verifyArchiveTree(path.join(releaseRoot, manager.localPath), manifestPayload.components.find((component) => component.id === "manager").installedTreeSha256);
  verifyArchiveTree(path.join(releaseRoot, codex.localPath), manifestPayload.components.find((component) => component.id === "codex").installedTreeSha256);
  for (const entry of [path.join(appRoot, "dist", "index.html"), path.join(appRoot, "dist", "launcher.html")]) if (!existsSync(entry)) fail(`Vite 构建入口缺失：${entry}`);
  console.log(JSON.stringify({ verified: true, version: appVersion, installerVersion, schemaVersion: 2, source: OSS_ROOT }, null, 2));
}

function build() {
  const signing = releaseSigning(); const toolchain = toolchainEnvironment(); buildBinaries(toolchain, signing);
  const output = path.join(buildRoot, "build", appVersion, "windows-x64", "tauri-codex.exe"); mkdirSync(path.dirname(output), { recursive: true }); copyFileSync(launcherSource, output); console.log(JSON.stringify({ built: true, artifact: artifactRecord(output) }, null, 2));
}

function rustTest() {
  const toolchain = toolchainEnvironment(); const manifest = path.join(appRoot, "src-tauri", "Cargo.toml"); const env = { ...toolchain.env, CARGO_TARGET_DIR: path.join(buildRoot, "rust-test") };
  run(toolchain.rustup, ["run", versions.rustToolchain, "cargo", "fmt", "--manifest-path", manifest, "--", "--check"], { env });
  run(toolchain.rustup, ["run", versions.rustToolchain, "cargo", "check", "--manifest-path", manifest, "--tests"], { env });
  run(toolchain.rustup, ["run", versions.rustToolchain, "cargo", "test", "--release", "--manifest-path", manifest, "--lib", "--target", versions.rustTarget], { env });
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  const mode = process.argv[2];
  try {
    if (process.env.TAURI_RELEASE_VERSION && process.env.TAURI_RELEASE_VERSION !== appVersion) fail("TAURI_RELEASE_VERSION 与 app/package.json 不一致。");
    if (installerVersions.schemaVersion !== 2 || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(installerVersion)) fail("installer-versions.json 无效。");
    if (!/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(minimumManagerVersion)) fail("installer-versions.json 缺少有效 minimumManagerVersion。");
    switch (mode) {
      case "bootstrap": bootstrap(); break;
      case "build": build(); break;
      case "installer-build": buildReleaseCandidate(); break;
      case "installer-verify": verifyReleaseCandidate(); break;
      case "release-build": bootstrap(); buildReleaseCandidate(); verifyReleaseCandidate(); break;
      case "release-verify": verifyReleaseCandidate(); break;
      case "rust-test": rustTest(); break;
      default: fail("用法: npm run <bootstrap|build|installer:build|installer:verify|build:release|verify:release>");
    }
  } catch (error) { console.error(error instanceof Error ? error.message : String(error)); process.exitCode = 1; }
}
