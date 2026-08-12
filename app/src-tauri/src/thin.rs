use crate::{paths, runtime};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};
use zip::ZipArchive;

const BOOTSTRAP_FILE: &str = "bootstrap.json";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/chengzhang0528/tauri-codex/releases/latest";
const MAX_MANIFEST_BYTES: u64 = 2 * 1024 * 1024;
const MAX_COMPONENT_BYTES: u64 = 1024 * 1024 * 1024;
const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
const DOCTOR_TIMEOUT: Duration = Duration::from_secs(30);
const RELEASE_STATE_SCHEMA_VERSION: u32 = 1;
const WINDOWS_MOVE_RETRY_ATTEMPTS: usize = 30;
const WINDOWS_MOVE_RETRY_DELAY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Deserialize)]
pub struct Bootstrap {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub product: String,
    pub platform: String,
    pub architecture: String,
    #[serde(default)]
    pub installer: Option<BootstrapInstaller>,
    pub release: BootstrapRelease,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BootstrapInstaller {
    pub version: String,
    pub artifact: Artifact,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BootstrapRelease {
    pub version: String,
    pub manifest: Artifact,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Manifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: u32,
    pub product: String,
    pub version: String,
    pub platform: String,
    pub architecture: String,
    pub components: Vec<ComponentArtifact>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ComponentArtifact {
    pub id: String,
    pub version: String,
    pub kind: String,
    pub artifact: Artifact,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub archive: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Artifact {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct CurrentReleaseState {
    schema_version: u32,
    current: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    previous: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherStatus {
    pub phase: String,
    pub component: String,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
    pub running: bool,
}

impl Default for LauncherStatus {
    fn default() -> Self {
        Self {
            phase: "正在读取发布信息".to_string(),
            component: "初始化".to_string(),
            downloaded: 0,
            total: 0,
            error: None,
            running: false,
        }
    }
}

#[derive(Default)]
pub struct LauncherState {
    status: Mutex<LauncherStatus>,
    running: AtomicBool,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

pub fn run_launcher_action() -> Result<bool, String> {
    let args = std::env::args().collect::<Vec<_>>();
    if args.get(1).map(String::as_str) == Some("--activate-release") {
        let version = args
            .get(2)
            .ok_or_else(|| "缺少待激活 release 版本".to_string())?;
        let pid = args
            .get(3)
            .and_then(|value| value.parse::<u32>().ok())
            .ok_or_else(|| "缺少 Manager PID".to_string())?;
        wait_for_process_exit(pid, Duration::from_secs(30))?;
        activate_release(&launcher_data_root()?, version)?;
        launch_current_manager(&launcher_data_root()?)?;
        return Ok(true);
    }
    if args.get(1).map(String::as_str) == Some("--thin-setup") {
        validate_installer_bootstrap()?;
        return Ok(true);
    }
    Ok(false)
}

pub fn launch_current_if_ready() -> Result<bool, String> {
    let root = launcher_data_root()?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    if let Some(release) = current_release_path(&root)? {
        if doctor_manager(&release.join("manager")).is_err()
            || doctor_codex(&release.join("codex")).is_err()
        {
            return Ok(false);
        }
        launch_current_manager(&root)?;
        return Ok(true);
    }
    Ok(false)
}

#[tauri::command]
pub fn get_launcher_status(state: State<'_, LauncherState>) -> Result<LauncherStatus, String> {
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "Launcher 状态锁已损坏".to_string())
}

#[tauri::command]
pub fn retry_launcher_setup(app: AppHandle, state: State<'_, LauncherState>) -> Result<(), String> {
    start_launcher_setup(app, state.inner())
}

pub fn start_launcher_setup(app: AppHandle, state: &LauncherState) -> Result<(), String> {
    if state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    update_launcher_status(
        &app,
        state,
        LauncherStatus {
            running: true,
            ..LauncherStatus::default()
        },
    );
    let app_handle = app.clone();
    std::thread::spawn(move || {
        let result = setup_first_release(&app_handle);
        let state = app_handle.state::<LauncherState>();
        state.running.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => {
                update_launcher_status(
                    &app_handle,
                    state.inner(),
                    LauncherStatus {
                        phase: "准备完成，正在启动应用".to_string(),
                        component: "桌面应用".to_string(),
                        running: false,
                        ..LauncherStatus::default()
                    },
                );
                match launcher_data_root().and_then(|root| launch_current_manager(&root)) {
                    Ok(()) => app_handle.exit(0),
                    Err(error) => set_launcher_error(&app_handle, state.inner(), error),
                }
            }
            Err(error) => set_launcher_error(&app_handle, state.inner(), error),
        }
    });
    Ok(())
}

fn setup_first_release(app: &AppHandle) -> Result<(), String> {
    let root = launcher_data_root()?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    report_launcher(app, "正在读取发布信息", "Bootstrap", 0, 0);
    let bootstrap = resolve_bootstrap()?;
    report_launcher(app, "正在校验组件清单", "Release manifest", 0, 0);
    let manifest = load_manifest(&bootstrap)?;
    let mut progress = |phase: &str, component: &str, downloaded: u64, total: u64| {
        report_launcher(app, phase, component, downloaded, total);
    };
    stage_release(&root, &manifest, &mut progress)?;
    report_launcher(app, "正在激活可运行版本", "桌面应用", 0, 0);
    activate_release(&root, &manifest.version)?;
    Ok(())
}

fn report_launcher(app: &AppHandle, phase: &str, component: &str, downloaded: u64, total: u64) {
    let state = app.state::<LauncherState>();
    update_launcher_status(
        app,
        state.inner(),
        LauncherStatus {
            phase: phase.to_string(),
            component: component.to_string(),
            downloaded,
            total,
            error: None,
            running: true,
        },
    );
}

fn set_launcher_error(app: &AppHandle, state: &LauncherState, error: String) {
    let current = state
        .status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default();
    update_launcher_status(
        app,
        state,
        LauncherStatus {
            error: Some(error),
            running: false,
            ..current
        },
    );
}

fn update_launcher_status(app: &AppHandle, state: &LauncherState, status: LauncherStatus) {
    if let Ok(mut current) = state.status.lock() {
        *current = status.clone();
    }
    let _ = app.emit("launcher-status", status);
}

pub fn validate_installer_bootstrap() -> Result<(), String> {
    validate_bootstrap(&read_installed_bootstrap()?)
}

pub fn staged_release_path(version: &str) -> Result<PathBuf, String> {
    validate_version(version)?;
    Ok(launcher_data_root()?
        .join("releases")
        .join(format!("staging-{version}")))
}

pub fn launch_release_activation(target: &str) -> Result<(), String> {
    if let Some(version) = target.strip_prefix("installer@") {
        validate_version(version)?;
        let installer = staged_installer_path(&launcher_data_root()?, version)?;
        Command::new(&installer)
            .arg("/S")
            .spawn()
            .map_err(|error| format!("无法启动 Installer {}：{error}", installer.display()))?;
        return Ok(());
    }
    validate_version(target)?;
    let launcher = std::env::var_os("TAURI_CODEX_LAUNCHER")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .ok_or_else(|| "当前 Manager 不是由已安装 Launcher 启动".to_string())?;
    let pid = std::process::id().to_string();
    Command::new(launcher)
        .args(["--activate-release", target, &pid])
        .spawn()
        .map_err(|error| format!("无法启动 release 激活器：{error}"))?;
    Ok(())
}

pub fn stage_latest_release() -> Result<String, String> {
    let root = launcher_data_root()?;
    let bootstrap = resolve_bootstrap()?;
    let manifest = load_manifest(&bootstrap)?;
    if current_release_version(&root)?.as_deref() != Some(manifest.version.as_str()) {
        stage_release(&root, &manifest, &mut |_, _, _, _| {})?;
        return Ok(manifest.version);
    }
    if let Some(installer) = &bootstrap.installer {
        if installer_is_newer(&installer.version)? {
            return stage_installer(&root, installer);
        }
    }
    Err("当前桌面应用和 Launcher 已是最新版本".to_string())
}

pub fn installer_update_available() -> Result<bool, String> {
    let bootstrap = resolve_bootstrap()?;
    match bootstrap.installer {
        Some(installer) => installer_is_newer(&installer.version),
        None => Ok(false),
    }
}

fn stage_installer(root: &Path, installer: &BootstrapInstaller) -> Result<String, String> {
    let directory = root.join("installer-updates").join(&installer.version);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let destination = directory.join(format!("tauri-codex_{}_x64-setup.exe", installer.version));
    download_artifact(
        &installer.artifact,
        &destination,
        "Installer",
        &mut |_, _, _, _| {},
    )?;
    fs::write(directory.join(".ready"), b"ready\n").map_err(|error| error.to_string())?;
    Ok(format!("installer@{}", installer.version))
}

pub fn current_release_version(root: &Path) -> Result<Option<String>, String> {
    Ok(read_current_release_state(root)?.map(|state| state.current))
}

pub fn current_release_path(root: &Path) -> Result<Option<PathBuf>, String> {
    let Some(state) = read_current_release_state(root)? else {
        return Ok(None);
    };
    let release = installed_release_path(root, &state.current);
    validate_installed_release(&release, &state.current)?;
    Ok(Some(release))
}

pub fn staged_releases() -> Result<Vec<String>, String> {
    let data_root = launcher_data_root()?;
    let root = data_root.join("releases");
    let mut releases = Vec::new();
    if root.is_dir() {
        releases.extend(
            fs::read_dir(root)
                .map_err(|error| error.to_string())?
                .filter_map(Result::ok)
                .filter(|entry| entry.path().join(".ready").is_file())
                .filter_map(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .strip_prefix("staging-")
                        .map(str::to_owned)
                }),
        );
    }
    let installers = data_root.join("installer-updates");
    if installers.is_dir() {
        releases.extend(
            fs::read_dir(installers)
                .map_err(|error| error.to_string())?
                .filter_map(Result::ok)
                .filter(|entry| entry.path().join(".ready").is_file())
                .map(|entry| format!("installer@{}", entry.file_name().to_string_lossy())),
        );
    }
    releases.sort();
    Ok(releases)
}

fn staged_installer_path(root: &Path, version: &str) -> Result<PathBuf, String> {
    let directory = root.join("installer-updates").join(version);
    let installer = directory.join(format!("tauri-codex_{version}_x64-setup.exe"));
    if !directory.join(".ready").is_file() || !installer.is_file() {
        return Err(format!("Installer {version} 尚未完整 stage"));
    }
    Ok(installer)
}

fn installer_is_newer(candidate: &str) -> Result<bool, String> {
    let current: serde_json::Value =
        serde_json::from_str(include_str!("../../installer-versions.json"))
            .map_err(|error| format!("内置 Installer 版本无法解析：{error}"))?;
    let current = current
        .get("installerVersion")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "内置 Installer 版本缺失".to_string())?;
    let candidate = semver::Version::parse(candidate).map_err(|error| error.to_string())?;
    let current = semver::Version::parse(current).map_err(|error| error.to_string())?;
    Ok(candidate > current)
}

fn resolve_bootstrap() -> Result<Bootstrap, String> {
    match read_latest_bootstrap() {
        Ok(bootstrap) => Ok(bootstrap),
        Err(network_error) => read_installed_bootstrap().map_err(|installed_error| {
            format!("无法读取线上或安装内 Bootstrap：{network_error}; {installed_error}")
        }),
    }
}

fn read_latest_bootstrap() -> Result<Bootstrap, String> {
    let client = http_client()?;
    let release: GithubRelease = client
        .get(LATEST_RELEASE_API)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| error.to_string())?;
    let asset = release
        .assets
        .into_iter()
        .find(|asset| asset.name == BOOTSTRAP_FILE)
        .ok_or_else(|| "最新 GitHub Release 缺少 bootstrap.json".to_string())?;
    let digest = asset
        .digest
        .ok_or_else(|| "GitHub bootstrap 资产缺少 SHA-256".to_string())?;
    let artifact = Artifact {
        url: asset.browser_download_url,
        size: asset.size,
        sha256: digest,
    };
    read_json_artifact(&artifact, MAX_MANIFEST_BYTES, "Bootstrap")
}

fn read_installed_bootstrap() -> Result<Bootstrap, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let install_dir = executable
        .parent()
        .ok_or_else(|| "无法确定安装目录".to_string())?;
    let candidates = [
        install_dir.join(BOOTSTRAP_FILE),
        install_dir.join("resources").join(BOOTSTRAP_FILE),
    ];
    let path = candidates
        .iter()
        .find(|path| path.is_file())
        .ok_or_else(|| "薄安装器缺少 bootstrap.json".to_string())?;
    serde_json::from_slice(&fs::read(path).map_err(|error| error.to_string())?)
        .map_err(|error| format!("Bootstrap 无法解析：{error}"))
}

fn load_manifest(bootstrap: &Bootstrap) -> Result<Manifest, String> {
    validate_bootstrap(bootstrap)?;
    let manifest = read_json_artifact(
        &bootstrap.release.manifest,
        MAX_MANIFEST_BYTES,
        "release manifest",
    )?;
    validate_manifest(&manifest, bootstrap)?;
    Ok(manifest)
}

fn validate_bootstrap(bootstrap: &Bootstrap) -> Result<(), String> {
    if bootstrap.schema_version != 1
        || bootstrap.product != "tauri-codex"
        || bootstrap.platform != "windows"
        || bootstrap.architecture != "x86_64"
    {
        return Err("Bootstrap 与当前 Windows x64 Launcher 不兼容".to_string());
    }
    validate_version(&bootstrap.release.version)?;
    if let Some(installer) = &bootstrap.installer {
        validate_version(&installer.version)?;
        validate_artifact(&installer.artifact, MAX_COMPONENT_BYTES)?;
    }
    validate_artifact(&bootstrap.release.manifest, MAX_MANIFEST_BYTES)
}

fn validate_manifest(manifest: &Manifest, bootstrap: &Bootstrap) -> Result<(), String> {
    if manifest.schema_version != 1
        || manifest.product != "tauri-codex"
        || manifest.version != bootstrap.release.version
        || manifest.platform != "windows"
        || manifest.architecture != "x86_64"
    {
        return Err("组件清单与 Bootstrap 不一致".to_string());
    }
    for id in ["manager", "codex", "node"] {
        if !manifest
            .components
            .iter()
            .any(|component| component.id == id && component.required)
        {
            return Err(format!("组件清单缺少必需 {id} 组件"));
        }
    }
    for component in &manifest.components {
        validate_version(&component.version)?;
        validate_artifact(&component.artifact, MAX_COMPONENT_BYTES)?;
        match component.id.as_str() {
            "manager" | "codex"
                if component.kind != "archive" || component.archive.as_deref() != Some("zip") =>
            {
                return Err(format!("{} 组件归档规则不合法", component.id));
            }
            "node" if component.kind != "system-msi" => {
                return Err("Node 组件类型不合法".to_string());
            }
            _ => {}
        }
    }
    Ok(())
}

fn stage_release(
    root: &Path,
    manifest: &Manifest,
    progress: &mut dyn FnMut(&str, &str, u64, u64),
) -> Result<PathBuf, String> {
    let releases = root.join("releases");
    let staging = releases.join(format!("staging-{}", manifest.version));
    let ready = staging.join(".ready");
    if ready.is_file() {
        let existing: Manifest = serde_json::from_slice(
            &fs::read(staging.join("release.json")).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        if existing == *manifest {
            return Ok(staging);
        }
    }
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let cache = root.join("components");
    fs::create_dir_all(&cache).map_err(|error| error.to_string())?;

    if runtime::check_system_node().is_err() {
        let node = required_component(manifest, "node")?;
        let installer = cache.join(format!("node-{}.msi", node.version));
        download_artifact(&node.artifact, &installer, "Node.js", progress)?;
        progress("正在安装系统依赖", "Node.js", 0, 0);
        install_node(&installer)?;
    } else {
        progress("已复用系统组件", "Node.js", 0, 0);
    }
    runtime::check_system_node()?;

    for id in ["manager", "codex"] {
        let component = required_component(manifest, id)?;
        let archive = cache.join(format!("{id}-{}.zip", component.version));
        let label = if id == "manager" { "Manager" } else { "Codex" };
        download_artifact(&component.artifact, &archive, label, progress)?;
        let destination = staging.join(id);
        fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        progress("正在安全解包", label, 0, 0);
        unpack_zip(&archive, &destination)?;
    }
    progress("正在检查组件", "Manager", 0, 0);
    doctor_manager(&staging.join("manager"))?;
    progress("正在检查组件", "Codex", 0, 0);
    doctor_codex(&staging.join("codex"))?;
    fs::write(
        staging.join("release.json"),
        serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    fs::write(ready, b"ready\n").map_err(|error| error.to_string())?;
    Ok(staging)
}

fn required_component<'a>(
    manifest: &'a Manifest,
    id: &str,
) -> Result<&'a ComponentArtifact, String> {
    manifest
        .components
        .iter()
        .find(|component| component.id == id && component.required)
        .ok_or_else(|| format!("清单缺少必需 {id} 组件"))
}

fn activate_release(root: &Path, version: &str) -> Result<(), String> {
    validate_version(version)?;
    let releases = root.join("releases");
    let staging = releases.join(format!("staging-{version}"));
    let installed = installed_release_path(root, version);
    let candidate = if staging.join(".ready").is_file() {
        staging.as_path()
    } else if installed.join(".ready").is_file() {
        installed.as_path()
    } else {
        return Err(format!("release {version} 尚未完整 stage"));
    };
    doctor_manager(&candidate.join("manager"))
        .map_err(|error| format!("激活前检查 Manager 失败：{error}"))?;
    doctor_codex(&candidate.join("codex"))
        .map_err(|error| format!("激活前检查 Codex 失败：{error}"))?;
    commit_staged_release(root, version)
}

fn launch_current_manager(root: &Path) -> Result<(), String> {
    let release =
        current_release_path(root)?.ok_or_else(|| "尚无可运行的当前桌面 release".to_string())?;
    let manager = release.join("manager").join("tauri-codex-manager.exe");
    doctor_manager(manager.parent().unwrap_or_else(|| Path::new(".")))?;
    let launcher = std::env::current_exe().map_err(|error| error.to_string())?;
    Command::new(&manager)
        .env("TAURI_CODEX_LAUNCHER", launcher)
        .spawn()
        .map_err(|error| format!("无法启动 Manager {}：{error}", manager.display()))?;
    Ok(())
}

fn commit_staged_release(root: &Path, version: &str) -> Result<(), String> {
    validate_version(version)?;
    let releases = root.join("releases");
    let staging = releases.join(format!("staging-{version}"));
    let installed = installed_release_path(root, version);

    if installed.exists() {
        validate_installed_release(&installed, version)?;
        if staging.exists() {
            fs::remove_dir_all(&staging)
                .map_err(|error| format!("无法清理重复 staging {}：{error}", staging.display()))?;
        }
    } else {
        if !staging.join(".ready").is_file() {
            return Err(format!("release {version} 尚未完整 stage"));
        }
        move_release_directory(&staging, &installed).map_err(|error| {
            format!(
                "无法落位 release 目录 {} -> {}：{error}",
                staging.display(),
                installed.display()
            )
        })?;
        validate_installed_release(&installed, version)?;
    }

    let current_state = read_current_release_state(root)?;
    if current_state
        .as_ref()
        .is_some_and(|state| state.current == version)
    {
        return Ok(());
    }
    let previous = current_state.map(|state| state.current);
    write_current_release_state(
        root,
        &CurrentReleaseState {
            schema_version: RELEASE_STATE_SCHEMA_VERSION,
            current: version.to_string(),
            previous,
        },
    )
}

fn installed_release_path(root: &Path, version: &str) -> PathBuf {
    root.join("releases").join(version)
}

fn validate_installed_release(path: &Path, version: &str) -> Result<(), String> {
    if !path.join(".ready").is_file() {
        return Err(format!(
            "release {version} 缺少 ready 标记：{}",
            path.display()
        ));
    }
    let manifest_path = path.join("release.json");
    let manifest: Manifest =
        serde_json::from_slice(&fs::read(&manifest_path).map_err(|error| {
            format!(
                "无法读取 release 元数据 {}：{error}",
                manifest_path.display()
            )
        })?)
        .map_err(|error| format!("release 元数据损坏 {}：{error}", manifest_path.display()))?;
    if manifest.version != version {
        return Err(format!(
            "release 目录版本不匹配：期望 {version}，实际 {}",
            manifest.version
        ));
    }
    Ok(())
}

fn read_current_release_state(root: &Path) -> Result<Option<CurrentReleaseState>, String> {
    let path = root.join("releases").join("current.json");
    if !path.is_file() {
        return Ok(None);
    }
    let state: CurrentReleaseState = serde_json::from_slice(
        &fs::read(&path)
            .map_err(|error| format!("无法读取当前 release 状态 {}：{error}", path.display()))?,
    )
    .map_err(|error| format!("当前 release 状态损坏 {}：{error}", path.display()))?;
    if state.schema_version != RELEASE_STATE_SCHEMA_VERSION {
        return Err(format!(
            "当前 release 状态版本不受支持：{}",
            state.schema_version
        ));
    }
    validate_version(&state.current)?;
    if let Some(previous) = &state.previous {
        validate_version(previous)?;
    }
    Ok(Some(state))
}

fn write_current_release_state(root: &Path, state: &CurrentReleaseState) -> Result<(), String> {
    let releases = root.join("releases");
    fs::create_dir_all(&releases)
        .map_err(|error| format!("无法创建 release 状态目录 {}：{error}", releases.display()))?;
    let target = releases.join("current.json");
    let temporary = releases.join(format!(".current-{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let bytes = serde_json::to_vec_pretty(state)
            .map_err(|error| format!("无法序列化当前 release 状态：{error}"))?;
        let mut file = File::create(&temporary).map_err(|error| {
            format!("无法创建临时 release 状态 {}：{error}", temporary.display())
        })?;
        file.write_all(&bytes).map_err(|error| {
            format!("无法写入临时 release 状态 {}：{error}", temporary.display())
        })?;
        file.sync_all().map_err(|error| {
            format!("无法刷新临时 release 状态 {}：{error}", temporary.display())
        })?;
        replace_file_atomic(&temporary, &target).map_err(|error| {
            format!(
                "无法提交当前 release 状态 {} -> {}：{error}",
                temporary.display(),
                target.display()
            )
        })
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn move_release_directory(source: &Path, target: &Path) -> io::Result<()> {
    retry_windows_move(
        || fs::rename(source, target),
        |delay| std::thread::sleep(delay),
    )
}

fn retry_windows_move<F, W>(mut operation: F, mut wait: W) -> io::Result<()>
where
    F: FnMut() -> io::Result<()>,
    W: FnMut(Duration),
{
    let mut last_error = None;
    for attempt in 0..WINDOWS_MOVE_RETRY_ATTEMPTS {
        match operation() {
            Ok(()) => return Ok(()),
            Err(error) if is_transient_windows_move_error(&error) => last_error = Some(error),
            Err(error) => return Err(error),
        }
        if attempt + 1 < WINDOWS_MOVE_RETRY_ATTEMPTS {
            wait(WINDOWS_MOVE_RETRY_DELAY);
        }
    }
    Err(last_error.expect("retry loop always records a transient error"))
}

fn is_transient_windows_move_error(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(5 | 32 | 33))
}

#[cfg(windows)]
fn replace_file_atomic(source: &Path, target: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let target = target
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        MoveFileExW(
            PCWSTR(source.as_ptr()),
            PCWSTR(target.as_ptr()),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
        .map_err(io::Error::other)
    }
}

#[cfg(not(windows))]
fn replace_file_atomic(source: &Path, target: &Path) -> io::Result<()> {
    fs::rename(source, target)
}

fn launcher_data_root() -> Result<PathBuf, String> {
    let roaming = std::env::var_os("APPDATA").ok_or_else(|| "APPDATA 不可用".to_string())?;
    Ok(PathBuf::from(roaming).join("com.tauri.codex"))
}

fn validate_artifact(artifact: &Artifact, max_size: u64) -> Result<(), String> {
    let url = reqwest::Url::parse(&artifact.url).map_err(|error| error.to_string())?;
    let trusted_host = matches!(
        url.host_str(),
        Some("github.com")
            | Some("objects.githubusercontent.com")
            | Some("release-assets.githubusercontent.com")
    );
    if url.scheme() != "https" || !trusted_host {
        return Err("组件来源必须是固定 HTTPS GitHub Releases 地址".to_string());
    }
    if artifact.size == 0 || artifact.size > max_size {
        return Err(format!("组件大小不在允许范围内：{}", artifact.size));
    }
    normalize_digest(&artifact.sha256).map(|_| ())
}

fn read_json_artifact<T: for<'de> Deserialize<'de>>(
    artifact: &Artifact,
    max_size: u64,
    label: &str,
) -> Result<T, String> {
    validate_artifact(artifact, max_size)?;
    let response = http_client()?
        .get(&artifact.url)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let bytes = response.bytes().map_err(|error| error.to_string())?;
    if bytes.len() as u64 != artifact.size {
        return Err(format!(
            "{label} 大小不匹配：期望 {}，收到 {}",
            artifact.size,
            bytes.len()
        ));
    }
    verify_digest(&bytes, &artifact.sha256)?;
    serde_json::from_slice(&bytes).map_err(|error| format!("{label} 无法解析：{error}"))
}

fn download_artifact(
    artifact: &Artifact,
    destination: &Path,
    component: &str,
    progress: &mut dyn FnMut(&str, &str, u64, u64),
) -> Result<(), String> {
    validate_artifact(artifact, MAX_COMPONENT_BYTES)?;
    if destination.is_file() {
        let bytes = fs::read(destination).map_err(|error| error.to_string())?;
        if bytes.len() as u64 == artifact.size && verify_digest(&bytes, &artifact.sha256).is_ok() {
            progress(
                "已复用校验通过的组件",
                component,
                artifact.size,
                artifact.size,
            );
            return Ok(());
        }
        fs::remove_file(destination).map_err(|error| error.to_string())?;
    }
    let partial = destination.with_extension("part");
    if partial.exists() {
        fs::remove_file(&partial).map_err(|error| error.to_string())?;
    }
    let mut response = http_client()?
        .get(&artifact.url)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut file = File::create(&partial).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    progress("正在下载组件", component, 0, artifact.size);
    loop {
        let read = response
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > MAX_COMPONENT_BYTES {
            let _ = fs::remove_file(&partial);
            return Err("组件下载超过大小上限".to_string());
        }
        file.write_all(&buffer[..read])
            .map_err(|error| error.to_string())?;
        hasher.update(&buffer[..read]);
        progress("正在下载组件", component, size, artifact.size);
    }
    drop(file);
    progress("正在校验组件", component, size, artifact.size);
    let digest = format!("{:x}", hasher.finalize());
    if size != artifact.size || digest != normalize_digest(&artifact.sha256)? {
        let _ = fs::remove_file(&partial);
        return Err("组件大小或 SHA-256 不匹配".to_string());
    }
    fs::rename(partial, destination).map_err(|error| error.to_string())
}

fn unpack_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "组件归档包含不安全路径".to_string())?
            .to_owned();
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut file = File::create(&output).map_err(|error| error.to_string())?;
            std::io::copy(&mut entry, &mut file).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn doctor_manager(root: &Path) -> Result<(), String> {
    let executable = root.join("tauri-codex-manager.exe");
    if executable.is_file()
        && fs::metadata(&executable)
            .map_err(|error| error.to_string())?
            .len()
            > 0
    {
        Ok(())
    } else {
        Err(format!("Manager 入口不存在：{}", executable.display()))
    }
}

fn doctor_codex(root: &Path) -> Result<(), String> {
    let entry = [
        root.join("node_modules/@openai/codex/bin/codex.js"),
        root.join("node_modules/@openai/codex/bin/codex"),
        root.join("node_modules/@openai/codex/dist/cli.js"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
    .ok_or_else(|| format!("Codex 入口不存在：{}", root.display()))?;
    let smoke_home = std::env::temp_dir().join(format!(
        "tauri-codex-doctor-{}",
        uuid::Uuid::new_v4().simple()
    ));
    fs::create_dir_all(&smoke_home).map_err(|error| error.to_string())?;
    let mut child = Command::new(paths::system_node()?)
        .arg(entry)
        .arg("--version")
        .env("CODEX_HOME", &smoke_home)
        .current_dir(root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) if status.success() => break,
            Some(status) => {
                return Err(format!(
                    "Codex doctor 退出码 {}",
                    status.code().unwrap_or(-1)
                ))
            }
            None if started.elapsed() >= DOCTOR_TIMEOUT => {
                let _ = child.kill();
                return Err("Codex doctor 超时".to_string());
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let _ = fs::remove_dir_all(smoke_home);
    Ok(())
}

fn install_node(installer: &Path) -> Result<(), String> {
    let status = Command::new("msiexec.exe")
        .args(["/i", &installer.to_string_lossy(), "/passive", "/norestart"])
        .status()
        .map_err(|error| format!("无法运行 Node.js 安装程序：{error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Node.js 安装程序退出码 {}",
            status.code().unwrap_or(-1)
        ))
    }
}

fn wait_for_process_exit(pid: u32, timeout: Duration) -> Result<(), String> {
    let started = Instant::now();
    while started.elapsed() < timeout {
        let output = Command::new("tasklist.exe")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .map_err(|error| error.to_string())?;
        let text = String::from_utf8_lossy(&output.stdout);
        if !text.contains(&pid.to_string()) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    Err(format!("等待 Manager {pid} 退出超时"))
}

fn http_client() -> Result<Client, String> {
    Client::builder()
        .user_agent("tauri-codex-thin-launcher")
        .connect_timeout(Duration::from_secs(15))
        .timeout(NETWORK_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())
}

fn verify_digest(bytes: &[u8], expected: &str) -> Result<(), String> {
    let expected = normalize_digest(expected)?;
    let actual = format!("{:x}", Sha256::digest(bytes));
    if actual == expected {
        Ok(())
    } else {
        Err(format!("SHA-256 不匹配：期望 {expected}，收到 {actual}"))
    }
}

fn normalize_digest(value: &str) -> Result<String, String> {
    let digest = value
        .trim()
        .trim_start_matches("sha256:")
        .to_ascii_lowercase();
    if digest.len() == 64
        && digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        Ok(digest)
    } else {
        Err("SHA-256 不合法".to_string())
    }
}

fn validate_version(value: &str) -> Result<(), String> {
    semver::Version::parse(value.trim().trim_start_matches('v'))
        .map(|_| ())
        .map_err(|error| format!("版本号不合法：{value} ({error})"))
}

#[cfg(test)]
mod tests {
    use super::{
        commit_staged_release, current_release_path, current_release_version, installer_is_newer,
        retry_windows_move, validate_artifact, verify_digest, Artifact, CurrentReleaseState,
        WINDOWS_MOVE_RETRY_ATTEMPTS, WINDOWS_MOVE_RETRY_DELAY,
    };
    use sha2::Digest;
    use std::fs;
    use std::io;

    #[test]
    fn rejects_non_github_artifact_sources() {
        let artifact = Artifact {
            url: "https://example.com/file".to_string(),
            size: 1,
            sha256: "00".repeat(32),
        };
        assert!(validate_artifact(&artifact, 10).is_err());
    }

    #[test]
    fn verifies_component_digest() {
        let bytes = b"thin-installer";
        let digest = format!("{:x}", sha2::Sha256::digest(bytes));
        assert!(verify_digest(bytes, &digest).is_ok());
        assert!(verify_digest(bytes, &"00".repeat(32)).is_err());
    }

    #[test]
    fn compares_installer_version_independently_from_manager_version() {
        assert!(installer_is_newer("1.0.3").unwrap());
        assert!(!installer_is_newer("1.0.2").unwrap());
        assert!(!installer_is_newer("0.1.2").unwrap());
    }

    #[test]
    fn retries_transient_windows_move_errors_until_success() {
        let mut attempts = 0;
        let mut waits = Vec::new();
        retry_windows_move(
            || {
                attempts += 1;
                if attempts < 3 {
                    Err(io::Error::from_raw_os_error(5))
                } else {
                    Ok(())
                }
            },
            |delay| waits.push(delay),
        )
        .unwrap();
        assert_eq!(attempts, 3);
        assert_eq!(waits, vec![WINDOWS_MOVE_RETRY_DELAY; 2]);
    }

    #[test]
    fn does_not_retry_permanent_move_errors() {
        let mut attempts = 0;
        let error = retry_windows_move(
            || {
                attempts += 1;
                Err(io::Error::from_raw_os_error(3))
            },
            |_| panic!("permanent move error must not wait"),
        )
        .unwrap_err();
        assert_eq!(attempts, 1);
        assert_eq!(error.raw_os_error(), Some(3));
    }

    #[test]
    fn returns_last_transient_error_after_retry_exhaustion() {
        let mut attempts = 0;
        let mut waits = 0;
        let error = retry_windows_move(
            || {
                attempts += 1;
                Err(io::Error::from_raw_os_error(32))
            },
            |_| waits += 1,
        )
        .unwrap_err();
        assert_eq!(attempts, WINDOWS_MOVE_RETRY_ATTEMPTS);
        assert_eq!(waits, WINDOWS_MOVE_RETRY_ATTEMPTS - 1);
        assert_eq!(error.raw_os_error(), Some(32));
    }

    #[test]
    fn activates_immutable_releases_through_an_atomic_current_state() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-release-state-{}",
            uuid::Uuid::new_v4().simple()
        ));
        write_staged_release(&root, "0.1.4");
        commit_staged_release(&root, "0.1.4").unwrap();
        assert_eq!(
            current_release_version(&root).unwrap().as_deref(),
            Some("0.1.4")
        );
        assert_eq!(
            current_release_path(&root).unwrap().unwrap(),
            root.join("releases/0.1.4")
        );

        write_staged_release(&root, "0.1.5");
        commit_staged_release(&root, "0.1.5").unwrap();
        let state: CurrentReleaseState =
            serde_json::from_slice(&fs::read(root.join("releases/current.json")).unwrap()).unwrap();
        assert_eq!(state.current, "0.1.5");
        assert_eq!(state.previous.as_deref(), Some("0.1.4"));
        assert!(root.join("releases/0.1.4/.ready").is_file());
        assert!(root.join("releases/0.1.5/.ready").is_file());

        write_staged_release(&root, "0.1.5");
        commit_staged_release(&root, "0.1.5").unwrap();
        let repeated: CurrentReleaseState =
            serde_json::from_slice(&fs::read(root.join("releases/current.json")).unwrap()).unwrap();
        assert_eq!(repeated.previous.as_deref(), Some("0.1.4"));

        fs::remove_file(root.join("releases/current.json")).unwrap();
        commit_staged_release(&root, "0.1.5").unwrap();
        assert_eq!(
            current_release_version(&root).unwrap().as_deref(),
            Some("0.1.5")
        );
        fs::remove_dir_all(root).unwrap();
    }

    fn write_staged_release(root: &std::path::Path, version: &str) {
        let staging = root.join("releases").join(format!("staging-{version}"));
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join(".ready"), b"ready\n").unwrap();
        fs::write(
            staging.join("release.json"),
            format!(
                r#"{{"schemaVersion":1,"product":"tauri-codex","version":"{version}","platform":"windows","architecture":"x86_64","components":[]}}"#
            ),
        )
        .unwrap();
    }
}
