import { copyFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import path from "node:path";
import process from "node:process";

const appRoot = process.cwd();
const resourceRoot = path.join(appRoot, "src-tauri", "resources", "node");
const buildVersions = JSON.parse(readFileSync(path.join(appRoot, "build-versions.json"), "utf8"));
const versionFile = path.join(resourceRoot, ".prepared-version");
const version = buildVersions.nodeVersion;
const sha256 = String(buildVersions.nodeSha256 ?? "").toLowerCase();
const buildCacheRoot = path.resolve(process.env.TAURI_BUILD_CACHE ?? path.join(appRoot, "..", ".codex-build", "cache"));
const cacheRoot = path.join(buildCacheRoot, "node", sha256);

if (!/^\d+\.\d+\.\d+$/.test(version)) throw new Error(`Invalid Node.js version: ${version}`);
if (!/^[a-f0-9]{64}$/.test(sha256)) throw new Error(`Invalid Node.js SHA-256: ${sha256}`);

function fileSha256(filePath) {
  return createHash("sha256").update(readFileSync(filePath)).digest("hex");
}

function isVerified(filePath) {
  return existsSync(filePath) && statSync(filePath).isFile() && fileSha256(filePath) === sha256;
}

function removeIfPresent(filePath) {
  rmSync(filePath, { recursive: true, force: true });
}

function copyVerified(source, destination) {
  mkdirSync(path.dirname(destination), { recursive: true });
  const partial = `${destination}.partial`;
  removeIfPresent(partial);
  copyFileSync(source, partial);
  try {
    if (!isVerified(partial)) throw new Error(`SHA-256 mismatch for ${source}`);
    removeIfPresent(destination);
    renameSync(partial, destination);
  } finally {
    removeIfPresent(partial);
  }
}

function downloadVerified(url, destination) {
  mkdirSync(path.dirname(destination), { recursive: true });
  const partial = `${destination}.partial`;
  removeIfPresent(partial);
  const systemCurl = process.env.SystemRoot ? path.join(process.env.SystemRoot, "System32", "curl.exe") : "";
  const curl = systemCurl && existsSync(systemCurl) ? systemCurl : "curl.exe";
  const result = spawnSync(curl, [
    "--location", "--fail", "--retry", "4", "--connect-timeout", "20", "--ssl-no-revoke",
    "--output", partial, url,
  ], { cwd: appRoot, stdio: "inherit", windowsHide: true });
  try {
    if (result.error) throw new Error(`curl.exe unavailable: ${result.error.message}`);
    if (result.status !== 0) throw new Error(`Node.js MSI download exited with ${result.status}`);
    if (!isVerified(partial)) throw new Error(`Node.js MSI SHA-256 mismatch; expected ${sha256}`);
    removeIfPresent(destination);
    renameSync(partial, destination);
  } finally {
    removeIfPresent(partial);
  }
}

function preparedVersion() {
  return existsSync(versionFile) ? readFileSync(versionFile, "utf8").trim() : "";
}

if (process.platform !== "win32") {
  console.log(`Skipping Windows Node.js MSI preparation on ${process.platform}.`);
} else {
  const fileName = `node-v${version}-x64.msi`;
  const target = path.join(resourceRoot, fileName);
  const cached = path.join(cacheRoot, fileName);
  const targetReady = isVerified(target) && preparedVersion() === version;
  if (targetReady) {
    if (!isVerified(cached)) copyVerified(target, cached);
    console.log(`Bundled Node.js ${version} MSI is already prepared.`);
  } else {
    rmSync(resourceRoot, { recursive: true, force: true });
    mkdirSync(resourceRoot, { recursive: true });
    if (!isVerified(cached)) {
      removeIfPresent(cached);
      downloadVerified(`https://nodejs.org/dist/v${version}/${fileName}`, cached);
      console.log(`Downloaded and cached official Node.js ${version} x64 MSI.`);
    } else {
      console.log(`Reusing verified cached Node.js ${version} x64 MSI.`);
    }
    copyVerified(cached, target);
    writeFileSync(versionFile, `${version}\n`, "utf8");
    console.log(`Prepared official Node.js ${version} x64 MSI from verified cache.`);
  }
}
