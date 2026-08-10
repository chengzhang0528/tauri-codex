import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";
import process from "node:process";

const appRoot = process.cwd();
const resourceRoot = path.join(appRoot, "src-tauri", "resources", "codex");
const buildVersions = JSON.parse(readFileSync(path.join(appRoot, "build-versions.json"), "utf8"));
const buildCacheRoot = path.resolve(process.env.TAURI_BUILD_CACHE ?? path.join(appRoot, "..", ".codex-build", "cache"));
const npmCache = path.resolve(process.env.npm_config_cache ?? path.join(buildCacheRoot, "npm"));
const npm = process.platform === "win32" ? (process.env.ComSpec ?? "cmd.exe") : "npm";
const npmPrefix = process.platform === "win32" ? ["/d", "/s", "/c", "npm.cmd"] : [];
const node = process.platform === "win32" ? "node.exe" : "node";
const versionFile = path.join(resourceRoot, ".prepared-version");

function run(args, options = {}) {
  const command = options.command ?? npm;
  const commandArgs = options.command ? args : [...npmPrefix, ...args];
  const result = spawnSync(command, commandArgs, {
      cwd: appRoot,
      env: { ...process.env, TAURI_BUILD_CACHE: buildCacheRoot, npm_config_cache: npmCache },
      stdio: options.quiet ? ["ignore", "pipe", "pipe"] : "inherit",
      encoding: "utf8",
  });
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`${command} exited with ${result.status}`);
  return result.stdout?.trim() ?? "";
}

const version = buildVersions.codexVersion;
if (!/^\d[\w.-]*$/.test(version)) throw new Error(`Invalid Codex version: ${version}`);
const packageFile = path.join(resourceRoot, "node_modules", "@openai", "codex", "package.json");
const installedVersion = existsSync(packageFile) ? JSON.parse(readFileSync(packageFile, "utf8")).version : "";
const preparedVersion = existsSync(versionFile) ? readFileSync(versionFile, "utf8").trim() : "";
if (installedVersion === version && preparedVersion === version) {
  console.log(`Bundled @openai/codex ${version} is already prepared.`);
} else {
  rmSync(resourceRoot, { recursive: true, force: true });
  mkdirSync(resourceRoot, { recursive: true });
  mkdirSync(npmCache, { recursive: true });
  run(["install", "--prefix", resourceRoot, "--cache", npmCache, "--prefer-offline", "--no-audit", "--no-fund", "--no-package-lock", `@openai/codex@${version}`]);
  if (!existsSync(packageFile)) throw new Error("Codex package was not installed into resources/codex");
  if (JSON.parse(readFileSync(packageFile, "utf8")).version !== version) throw new Error(`Installed Codex version does not match ${version}`);
  run([path.join(resourceRoot, "node_modules", "@openai", "codex", "bin", "codex.js"), "--version"], { command: node });
  writeFileSync(versionFile, `${version}\n`, "utf8");
  console.log(`Prepared bundled @openai/codex ${version}.`);
}
