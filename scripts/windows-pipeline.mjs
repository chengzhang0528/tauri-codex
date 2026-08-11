import { copyFileSync, existsSync, mkdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(scriptRoot, "..");
const appRoot = path.join(workspaceRoot, "app");
const appScript = path.join(appRoot, "scripts", "run-tauri-windows.mjs");
const config = JSON.parse(readFileSync(path.join(appRoot, "build-versions.json"), "utf8"));
const appPackage = JSON.parse(readFileSync(path.join(appRoot, "package.json"), "utf8"));
const tauriConfig = JSON.parse(readFileSync(path.join(appRoot, "src-tauri", "tauri.conf.json"), "utf8"));
const appVersion = appPackage.version;
const buildRoot = path.join(workspaceRoot, ".codex-build");
const releaseRoot = path.join(buildRoot, "releases", appVersion, "windows-x64");
const targetReleaseRoot = path.join(appRoot, "src-tauri", "target", config.rustTarget, "release");
const installerSource = path.join(targetReleaseRoot, "bundle", "nsis", `tauri-codex_${appVersion}_x64-setup.exe`);
const installerOutput = path.join(releaseRoot, `tauri-codex_${appVersion}_x64-setup.exe`);
const installerManifest = path.join(releaseRoot, "installer.json");

if (process.env.TAURI_RELEASE_VERSION && process.env.TAURI_RELEASE_VERSION !== appVersion) {
  fail(`TAURI_RELEASE_VERSION ${process.env.TAURI_RELEASE_VERSION} 与 app/package.json ${appVersion} 不一致。`);
}

function fail(message) {
  throw new Error(message);
}

function run(command, args, options = {}) {
  const result = spawnSync(command, args, {
    cwd: options.cwd ?? workspaceRoot,
    env: options.env ?? process.env,
    stdio: options.capture ? ["ignore", "pipe", "pipe"] : "inherit",
    encoding: "utf8",
    windowsHide: false,
  });
  if (result.error) throw new Error(`${command} unavailable: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`${path.basename(command)} exited with ${result.status}`);
  return result;
}

function npmCommand() {
  const node = process.env.npm_node_execpath ?? process.execPath;
  const cli = process.env.npm_execpath;
  if (!cli) fail("必须通过 npm run 调用根脚本，以便定位 npm CLI。");
  return { node, cli };
}

function runNpm(args, env) {
  const { node, cli } = npmCommand();
  return run(node, [cli, ...args], { env });
}

function toolchainEnvironment() {
  if (process.platform !== "win32") fail("Windows 构建脚本只能在 Windows 上运行。");
  const userProfile = process.env.USERPROFILE;
  const localAppData = process.env.LOCALAPPDATA;
  const cargoBin = userProfile ? path.join(userProfile, ".cargo", "bin") : "";
  const rustup = cargoBin ? path.join(cargoBin, "rustup.exe") : "";
  const mingwCandidates = [
    process.env.TAURI_MINGW_BIN,
    localAppData && path.join(localAppData, "Programs", "msys64", "ucrt64", "bin"),
    "C:\\msys64\\ucrt64\\bin",
    "C:\\Program Files\\msys64\\ucrt64\\bin",
  ].filter(Boolean);
  const mingwBin = mingwCandidates.find((candidate) =>
    existsSync(path.join(candidate, "windres.exe")) &&
    existsSync(path.join(candidate, "gcc.exe")) &&
    existsSync(path.join(candidate, "ar.exe")),
  );
  if (!rustup || !existsSync(rustup)) fail(`未找到 Rustup: ${rustup || "USERPROFILE 未设置"}`);
  if (!mingwBin) fail("未找到 MSYS2 UCRT64 工具链（需要 windres.exe、gcc.exe 和 ar.exe）。");

  const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === "path") ?? "PATH";
  const cacheRoot = path.join(buildRoot, "cache");
  const npmCache = path.join(cacheRoot, "npm");
  mkdirSync(cacheRoot, { recursive: true });
  const env = {
    ...process.env,
    RUSTUP_TOOLCHAIN: config.rustToolchain,
    TAURI_BUILD_CACHE: cacheRoot,
    npm_config_cache: npmCache,
    npm_config_prefer_offline: "true",
    [pathKey]: [mingwBin, cargoBin, process.env[pathKey]].filter(Boolean).join(";"),
  };
  const compiler = run(rustup, ["run", config.rustToolchain, "rustc", "-Vv"], { env, capture: true });
  const installedTargets = run(rustup, ["target", "list", "--installed", "--toolchain", config.rustToolchain], { env, capture: true });
  if (!installedTargets.stdout.split(/\r?\n/).includes(config.rustTarget)) {
    fail(`Rust target ${config.rustTarget} 未安装。`);
  }
  return { env, rustup, mingwBin, compiler: compiler.stdout.trim() };
}

function readPreparedVersion(relativePath) {
  const value = readFileSync(path.join(appRoot, relativePath), "utf8").trim();
  return value;
}

function sha256(filePath) {
  return createHash("sha256").update(readFileSync(filePath)).digest("hex");
}

function artifactRecord(filePath) {
  const stat = statSync(filePath);
  return {
    path: path.relative(workspaceRoot, filePath).replaceAll(path.sep, "/"),
    size: stat.size,
    sha256: sha256(filePath),
  };
}

function writeManifest(filePath, artifact) {
  mkdirSync(path.dirname(filePath), { recursive: true });
  writeFileSync(filePath, `${JSON.stringify({
    schemaVersion: 1,
    appVersion,
    target: config.rustTarget,
    codexVersion: config.codexVersion,
    nodeVersion: config.nodeVersion,
    nodeSha256: config.nodeSha256,
    artifact,
  }, null, 2)}\n`, "utf8");
}

function runTauri(mode, extraArgs, env) {
  return run(process.execPath, [appScript, mode, ...extraArgs], { env });
}

function bootstrap() {
  const toolchain = toolchainEnvironment();
  const dependencies = path.join(appRoot, "node_modules");
  const tauriBin = path.join(dependencies, ".bin", process.platform === "win32" ? "tauri.cmd" : "tauri");
  const viteEntry = path.join(dependencies, "vite", "bin", "vite.js");
  if (!existsSync(dependencies)) {
    runNpm(["--prefix", appRoot, "ci", "--no-audit", "--no-fund"], toolchain.env);
  } else if (!existsSync(tauriBin) || !existsSync(viteEntry)) {
    console.log("app/node_modules 不完整，执行 npm install 修复缺失依赖，不删除现有目录。");
    runNpm(["--prefix", appRoot, "install", "--no-audit", "--no-fund", "--no-package-lock"], toolchain.env);
  } else {
    console.log("复用已存在的 app/node_modules，跳过 npm ci 以避免打断正在运行的开发进程。");
  }
  runNpm(["--prefix", appRoot, "run", "prepare:codex"], toolchain.env);
  runNpm(["--prefix", appRoot, "run", "prepare:node"], toolchain.env);
  console.log(JSON.stringify({ bootstrapped: true, target: config.rustTarget, codexVersion: config.codexVersion, nodeVersion: config.nodeVersion }, null, 2));
}

function build() {
  const toolchain = toolchainEnvironment();
  runTauri("build", ["--no-bundle"], toolchain.env);
  const binary = path.join(targetReleaseRoot, "tauri-codex.exe");
  if (!existsSync(binary)) fail(`应用构建没有生成 ${binary}`);
  const output = path.join(buildRoot, "build", appVersion, "windows-x64", "tauri-codex.exe");
  mkdirSync(path.dirname(output), { recursive: true });
  copyFileSync(binary, output);
  console.log(JSON.stringify({ built: true, artifact: artifactRecord(output) }, null, 2));
}

function buildInstaller() {
  const toolchain = toolchainEnvironment();
  runTauri("build", [], toolchain.env);
  if (!existsSync(installerSource)) fail(`Tauri 没有生成 ${installerSource}`);
  mkdirSync(releaseRoot, { recursive: true });
  copyFileSync(installerSource, installerOutput);
  const artifact = artifactRecord(installerOutput);
  writeManifest(installerManifest, artifact);
  console.log(JSON.stringify({ built: true, installer: artifact, manifest: path.relative(workspaceRoot, installerManifest).replaceAll(path.sep, "/") }, null, 2));
}

function verifyInstaller() {
  if (!existsSync(installerOutput)) fail(`找不到安装包 ${installerOutput}，先运行 npm run installer:build`);
  if (!existsSync(installerManifest)) fail(`找不到安装包清单 ${installerManifest}`);
  const manifest = JSON.parse(readFileSync(installerManifest, "utf8"));
  const artifact = artifactRecord(installerOutput);
  if (manifest.schemaVersion !== 1 || manifest.appVersion !== appVersion || manifest.target !== config.rustTarget ||
      manifest.codexVersion !== config.codexVersion || manifest.nodeVersion !== config.nodeVersion ||
      manifest.nodeSha256 !== config.nodeSha256) {
    fail("安装包清单中的版本或目标与当前固定构建配置不一致。");
  }
  if (JSON.stringify(manifest.artifact) !== JSON.stringify(artifact)) fail("安装包清单与实际文件大小或 SHA-256 不一致。");
  if (artifact.size < 1_000_000) fail("安装包体积异常，拒绝作为候选制品。");
  if (readPreparedVersion("src-tauri/resources/codex/.prepared-version") !== config.codexVersion ||
      readPreparedVersion("src-tauri/resources/node/.prepared-version") !== config.nodeVersion) {
    fail("内置 Codex 或 Node 资源版本与固定构建配置不一致。");
  }
  const nodeMsi = path.join(appRoot, "src-tauri", "resources", "node", `node-v${config.nodeVersion}-x64.msi`);
  const codexPackage = path.join(appRoot, "src-tauri", "resources", "codex", "node_modules", "@openai", "codex", "package.json");
  if (!existsSync(nodeMsi) || !existsSync(codexPackage)) fail("安装包输入资源不完整。");
  if (sha256(nodeMsi) !== config.nodeSha256) fail("内置 Node.js MSI 的 SHA-256 与固定构建配置不一致。");
  console.log(JSON.stringify({ verified: true, installer: artifact, codexVersion: config.codexVersion, nodeVersion: config.nodeVersion }, null, 2));
}

function buildRelease() {
  bootstrap();
  buildInstaller();
  verifyInstaller();
}

function verifyRelease() {
  verifyInstaller();
  console.log(JSON.stringify({ releaseVerified: true, version: appVersion, target: config.rustTarget }, null, 2));
}

function rustTest() {
  const toolchain = toolchainEnvironment();
  const manifest = path.join(appRoot, "src-tauri", "Cargo.toml");
  run(toolchain.rustup, ["run", config.rustToolchain, "cargo", "fmt", "--manifest-path", manifest, "--", "--check"], { env: toolchain.env });
  run(toolchain.rustup, ["run", config.rustToolchain, "cargo", "check", "--manifest-path", manifest, "--tests"], { env: toolchain.env });
  run(toolchain.rustup, ["run", config.rustToolchain, "cargo", "test", "--release", "--manifest-path", manifest, "--lib", "--target", config.rustTarget], { env: toolchain.env });
}

const mode = process.argv[2];
try {
  switch (mode) {
    case "bootstrap": bootstrap(); break;
    case "build": build(); break;
    case "installer-build": buildInstaller(); break;
    case "installer-verify": verifyInstaller(); break;
    case "release-build": buildRelease(); break;
    case "release-verify": verifyRelease(); break;
    case "rust-test": rustTest(); break;
    default: fail("用法: npm run <bootstrap|build|installer:build|installer:verify|build:release|verify:release>");
  }
} catch (error) {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
}
