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
  ];
  for (const [file, transform] of files) writeFileSync(file, transform(readFileSync(file, "utf8")), "utf8");
}

function preparePatch() {
  if (runGit(["branch", "--show-current"], { capture: true }) !== "main") throw new Error("release preparation must start from main");
  if (runGit(["status", "--porcelain", "--", "app/package.json", "app/package-lock.json", "app/src-tauri/Cargo.toml", "app/src-tauri/tauri.conf.json"], { capture: true })) {
    throw new Error("canonical version files must be clean before release preparation");
  }
  const packagePath = path.join(appRoot, "package.json");
  const current = JSON.parse(readFileSync(packagePath, "utf8")).version;
  const next = nextPatchVersion(current);
  updateVersions(current, next);
  console.log(JSON.stringify({ prepared: true, current, version: next, tag: `v${next}` }, null, 2));
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try {
    if (process.argv[2] !== "patch") throw new Error("usage: npm run release:patch");
    preparePatch();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
