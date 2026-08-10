import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawn } from 'node:child_process';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const appDir = resolve(scriptDir, '..');
const buildVersions = JSON.parse(readFileSync(join(appDir, 'build-versions.json'), 'utf8'));
const mode = process.argv[2];
const extraArgs = process.argv.slice(3);
const target = buildVersions.rustTarget;
const toolchain = buildVersions.rustToolchain;

if (process.platform !== 'win32') {
  console.error('tauri-codex 的 Windows 构建入口只能在 Windows 上运行。');
  process.exit(1);
}

if (mode !== 'dev' && mode !== 'build') {
  console.error('用法: npm run tauri:windows -- <dev|build> [Tauri 参数]');
  process.exit(1);
}

const userProfile = process.env.USERPROFILE;
const localAppData = process.env.LOCALAPPDATA;
const cargoBin = userProfile ? join(userProfile, '.cargo', 'bin') : null;
const rustup = cargoBin ? join(cargoBin, 'rustup.exe') : null;
const mingwCandidates = [
  process.env.TAURI_MINGW_BIN,
  localAppData && join(localAppData, 'Programs', 'msys64', 'ucrt64', 'bin'),
  'C:\\msys64\\ucrt64\\bin',
  'C:\\Program Files\\msys64\\ucrt64\\bin',
].filter(Boolean);
const mingwBin = mingwCandidates.find((candidate) =>
  existsSync(join(candidate, 'windres.exe')) && existsSync(join(candidate, 'gcc.exe')),
);

if (!rustup || !existsSync(rustup)) {
  console.error(`未找到 Rustup: ${rustup ?? 'USERPROFILE 未设置'}`);
  process.exit(1);
}

if (!mingwBin) {
  console.error('未找到 MSYS2 UCRT64 工具链（需要 windres.exe 和 gcc.exe）。');
  console.error('可设置 TAURI_MINGW_BIN 指向 ucrt64\\bin。');
  process.exit(1);
}

const pathKey = Object.keys(process.env).find((key) => key.toLowerCase() === 'path') ?? 'PATH';
const pathEntries = [mingwBin, cargoBin, process.env[pathKey]].filter(Boolean);
const childEnv = {
  ...process.env,
  RUSTUP_TOOLCHAIN: toolchain,
  [pathKey]: pathEntries.join(';'),
};

const npmCommand = process.env.npm_node_execpath ?? process.execPath;
const npmArgs = process.env.npm_execpath ? [process.env.npm_execpath] : null;
if (!npmArgs) {
  console.error('必须通过 npm run 调用 tauri:windows，以便定位 npm CLI。');
  process.exit(1);
}
const args = ['run', 'tauri', '--', mode, '--target', target, ...extraArgs];
const child = spawn(npmCommand, [...npmArgs, ...args], {
  cwd: appDir,
  env: childEnv,
  stdio: 'inherit',
  windowsHide: false,
});

child.once('error', (error) => {
  console.error(`无法启动 npm: ${error.message}`);
  process.exit(1);
});

child.once('exit', (code, signal) => {
  if (signal) {
    console.error(`Tauri 进程收到信号 ${signal}`);
    process.exit(1);
  }
  process.exit(code ?? 1);
});
