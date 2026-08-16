import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { spawnSync } from "node:child_process";

const scriptRoot = path.dirname(fileURLToPath(import.meta.url));
const workspaceRoot = path.resolve(scriptRoot, "..");
const appRoot = path.join(workspaceRoot, "app");
const stableVersion = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/;

export function nextPatchVersion(version) {
  if (!stableVersion.test(version)) throw new Error(`invalid stable version: ${version}`);
  const [major, minor, patch] = version.split(".").map(Number);
  return `${major}.${minor}.${patch + 1}`;
}

export function replaceVersion(text, current, next, file) {
  if (!stableVersion.test(current) || !stableVersion.test(next)) {
    throw new Error(`invalid version for ${file}`);
  }
  const marker = `"${current}"`;
  const matches = text.split(marker).length - 1;
  if (matches !== 1) throw new Error(`${file} must contain exactly one version marker for ${current}`);
  return text.replace(marker, `"${next}"`);
}

export function replaceExact(text, marker, replacement, file, expected = 1) {
  const matches = text.split(marker).length - 1;
  if (matches !== expected) throw new Error(`${file} must contain exactly ${expected} expected marker(s)`);
  return text.replaceAll(marker, replacement);
}

export function prepareInstallerVersionValues(installer, tauri, appVersion) {
  if (installer?.schemaVersion !== 2 || !stableVersion.test(installer.installerVersion) || !stableVersion.test(installer.minimumManagerVersion)) {
    throw new Error("app/installer-versions.json is invalid");
  }
  if (!stableVersion.test(appVersion)) throw new Error("app/package.json has an invalid version");
  if (tauri?.version !== installer.installerVersion) throw new Error("Tauri and Installer versions have drifted");
  const current = installer.installerVersion;
  const next = nextPatchVersion(current);
  return {
    current,
    next,
    installer: { ...installer, installerVersion: next, minimumManagerVersion: appVersion, publishedArtifact: null },
    tauri: { ...tauri, version: next },
  };
}

export function replaceCargoPackageVersion(text, current, next) {
  if (!stableVersion.test(current) || !stableVersion.test(next)) throw new Error("invalid Cargo package version");
  const escaped = current.replaceAll(".", "\\.");
  const marker = new RegExp(`(name = "tauri-codex"\\r?\\nversion = ")${escaped}(")`, "g");
  if ([...text.matchAll(marker)].length !== 1) throw new Error("app/src-tauri/Cargo.lock has an unexpected tauri-codex version count");
  return text.replace(marker, `$1${next}$2`);
}

function runGit(args, options = {}) {
  const result = spawnSync("git", ["-C", workspaceRoot, ...args], {
    encoding: "utf8",
    stdio: options.capture ? ["ignore", "pipe", "pipe"] : "inherit",
    windowsHide: false,
  });
  if (result.error) throw new Error(`git unavailable: ${result.error.message}`);
  if (result.status !== 0) throw new Error(`git ${args.join(" ")} failed: ${(result.stderr || "").trim()}`);
  return options.capture ? result.stdout.trim() : "";
}

function updateVersions(current, next) {
  const files = [
    [path.join(appRoot, "package.json"), (text) => replaceVersion(text, current, next, "app/package.json")],
    [path.join(appRoot, "package-lock.json"), (text) => {
      const marker = `"version": "${current}"`;
      const matches = text.split(marker).length - 1;
      if (matches < 2) throw new Error("app/package-lock.json is missing its root version entries");
      let remaining = 2;
      return text.replaceAll(marker, () => remaining-- > 0 ? `"version": "${next}"` : marker);
    }],
    [path.join(appRoot, "src-tauri", "Cargo.toml"), (text) => {
      const marker = `version = "${current}"`;
      if (text.split(marker).length - 1 !== 1) throw new Error("app/src-tauri/Cargo.toml has an unexpected version count");
      return text.replace(marker, `version = "${next}"`);
    }],
    [path.join(appRoot, "src-tauri", "Cargo.lock"), (text) => replaceCargoPackageVersion(text, current, next)],
    [path.join(workspaceRoot, "README.md"), (text) => replaceExact(text, `\`v${current}\``, `\`v${next}\``, "README.md")],
    [path.join(workspaceRoot, "人类-文档", "开发", "构建Windows桌面应用.md"), (text) => {
      let updated = replaceExact(text, `.codex-build/build/${current}/`, `.codex-build/build/${next}/`, "构建Windows桌面应用.md build path");
      updated = replaceExact(updated, `.codex-build/releases/${current}/`, `.codex-build/releases/${next}/`, "构建Windows桌面应用.md release path");
      return updated;
    }],
  ];
  const updates = files.map(([file, transform]) => [file, transform(readFileSync(file, "utf8"))]);
  for (const [file, text] of updates) writeFileSync(file, text, "utf8");
}

function preparePatch() {
  if (runGit(["branch", "--show-current"], { capture: true }) !== "main") throw new Error("release preparation must start from main");
  if (runGit(["status", "--porcelain", "--", "app/package.json", "app/package-lock.json", "app/src-tauri/Cargo.toml", "app/src-tauri/Cargo.lock", "app/src-tauri/tauri.conf.json", "README.md", "人类-文档/开发/构建Windows桌面应用.md"], { capture: true })) {
    throw new Error("canonical version files must be clean before release preparation");
  }
  const packagePath = path.join(appRoot, "package.json");
  const current = JSON.parse(readFileSync(packagePath, "utf8")).version;
  const next = nextPatchVersion(current);
  updateVersions(current, next);
  console.log(JSON.stringify({ prepared: true, current, version: next, tag: `v${next}` }, null, 2));
}

function prepareInstallerPatch() {
  if (runGit(["branch", "--show-current"], { capture: true }) !== "main") throw new Error("release preparation must start from main");
  if (runGit(["status", "--porcelain", "--", "app/installer-versions.json", "app/src-tauri/tauri.conf.json"], { capture: true })) {
    throw new Error("canonical Installer version files must be clean before release preparation");
  }
  const appVersion = JSON.parse(readFileSync(path.join(appRoot, "package.json"), "utf8")).version;
  const installerPath = path.join(appRoot, "installer-versions.json");
  const tauriPath = path.join(appRoot, "src-tauri", "tauri.conf.json");
  const installer = JSON.parse(readFileSync(installerPath, "utf8"));
  const tauri = JSON.parse(readFileSync(tauriPath, "utf8"));
  const { current, next, installer: updatedInstaller, tauri: updatedTauri } = prepareInstallerVersionValues(installer, tauri, appVersion);
  writeFileSync(installerPath, `${JSON.stringify(updatedInstaller, null, 2)}\n`, "utf8");
  writeFileSync(tauriPath, `${JSON.stringify(updatedTauri, null, 2)}\n`, "utf8");
  console.log(JSON.stringify({ prepared: true, current, installerVersion: next, minimumManagerVersion: appVersion }, null, 2));
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try {
    if (process.argv[2] === "patch") preparePatch();
    else if (process.argv[2] === "installer-patch") prepareInstallerPatch();
    else throw new Error("usage: node scripts/release.mjs <patch|installer-patch>");
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
