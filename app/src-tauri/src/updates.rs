use crate::model::{
    CodexUpdateInfo, GithubReleaseResponse, ReleaseAsset, ReleaseInfo, UpdateResult,
};
use crate::paths;
use reqwest::blocking::Client;
use reqwest::StatusCode;
use semver::Version;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tauri::AppHandle;

const CODEX_MIN_NODE_MAJOR: u64 = 16;
const NPM_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const NPM_VIEW_TIMEOUT: Duration = Duration::from_secs(30);
const CODEX_SMOKE_TIMEOUT: Duration = Duration::from_secs(30);

pub fn check_release() -> Result<ReleaseInfo, String> {
    let client = Client::builder()
        .user_agent("tauri-codex-updater")
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        paths::GITHUB_REPOSITORY
    );
    let response = client.get(url).send().map_err(|error| error.to_string())?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(no_release_info());
    }
    let response: GithubReleaseResponse = response
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json()
        .map_err(|error| error.to_string())?;
    let update_available = is_newer_version(&response.tag_name, env!("CARGO_PKG_VERSION"))?;
    Ok(ReleaseInfo {
        tag_name: response.tag_name,
        name: response.name.unwrap_or_default(),
        html_url: response.html_url,
        published_at: response.published_at,
        update_available,
        assets: response
            .assets
            .into_iter()
            .map(|asset| ReleaseAsset {
                name: asset.name,
                download_url: asset.browser_download_url,
                size: asset.size,
                digest: asset.digest,
            })
            .collect(),
    })
}

fn no_release_info() -> ReleaseInfo {
    ReleaseInfo {
        tag_name: String::new(),
        name: String::new(),
        html_url: String::new(),
        published_at: None,
        update_available: false,
        assets: Vec::new(),
    }
}

pub fn download_release(
    app: &AppHandle,
    url: &str,
    filename: &str,
    expected_size: u64,
    expected_digest: Option<&str>,
    release_tag: &str,
) -> Result<UpdateResult, String> {
    let parsed = reqwest::Url::parse(url).map_err(|error| error.to_string())?;
    if !matches!(
        parsed.host_str(),
        Some("github.com")
            | Some("objects.githubusercontent.com")
            | Some("release-assets.githubusercontent.com")
    ) {
        return Err("更新资产必须来自 GitHub Releases".to_string());
    }
    let tag = paths::safe_filename(release_tag.trim());
    if tag.is_empty() {
        return Err("GitHub Release tag 不能为空".to_string());
    }
    let directory = paths::updates_dir(app)?.join(tag);
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let name = paths::safe_filename(filename);
    let target = directory.join(&name);
    let partial = directory.join(format!(".{name}.part"));
    if target.is_file() {
        match verify_download(&target, expected_size, expected_digest) {
            Ok((bytes, digest)) => {
                write_download_metadata(&directory, release_tag, &name, bytes, &digest)?;
                return Ok(UpdateResult {
                    version: release_tag.to_string(),
                    path: target.to_string_lossy().to_string(),
                    kind: "desktop-staged".to_string(),
                });
            }
            Err(_) => fs::remove_file(&target).map_err(|error| error.to_string())?,
        }
    }
    let client = Client::builder()
        .user_agent("tauri-codex-updater")
        .connect_timeout(Duration::from_secs(15))
        .build()
        .map_err(|error| error.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut file = fs::File::create(&partial).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    let result = (|| -> Result<(), String> {
        loop {
            let read = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            hasher.update(&buffer[..read]);
            bytes += read as u64;
        }
        if expected_size != 0 && bytes != expected_size {
            return Err(format!(
                "更新资产大小不匹配：期望 {expected_size} 字节，收到 {bytes} 字节"
            ));
        }
        let actual_digest = format!("sha256:{:x}", hasher.finalize());
        if let Some(expected) = expected_digest.filter(|value| !value.trim().is_empty()) {
            let normalized = normalize_digest(expected)?;
            if actual_digest != normalized {
                return Err(format!(
                    "更新资产 SHA-256 不匹配：期望 {normalized}，收到 {actual_digest}"
                ));
            }
        }
        Ok(())
    })();
    drop(file);
    if let Err(error) = result {
        let _ = fs::remove_file(&partial);
        return Err(error);
    }
    if target.exists() {
        fs::remove_file(&target).map_err(|error| error.to_string())?;
    }
    fs::rename(&partial, &target).map_err(|error| error.to_string())?;
    let digest = format!(
        "{:x}",
        Sha256::digest(fs::read(&target).map_err(|error| error.to_string())?)
    );
    write_download_metadata(&directory, release_tag, &name, bytes, &digest)?;
    Ok(UpdateResult {
        version: release_tag.to_string(),
        path: target.to_string_lossy().to_string(),
        kind: "desktop-staged".to_string(),
    })
}

fn verify_download(
    path: &Path,
    expected_size: u64,
    expected_digest: Option<&str>,
) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    if expected_size != 0 && size != expected_size {
        return Err(format!(
            "更新资产大小不匹配：期望 {expected_size} 字节，收到 {size} 字节"
        ));
    }
    let digest = format!("sha256:{:x}", hasher.finalize());
    if let Some(expected) = expected_digest.filter(|value| !value.trim().is_empty()) {
        let normalized = normalize_digest(expected)?;
        if digest != normalized {
            return Err(format!(
                "更新资产 SHA-256 不匹配：期望 {normalized}，收到 {digest}"
            ));
        }
    }
    Ok((size, digest.trim_start_matches("sha256:").to_string()))
}

fn write_download_metadata(
    directory: &Path,
    release_tag: &str,
    name: &str,
    size: u64,
    digest: &str,
) -> Result<(), String> {
    let metadata = serde_json::json!({
        "release": release_tag,
        "name": name,
        "size": size,
        "sha256": digest,
    });
    fs::write(
        directory.join(format!("{name}.json")),
        serde_json::to_vec_pretty(&metadata).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub fn check_codex_update(app: &AppHandle) -> Result<CodexUpdateInfo, String> {
    let mut npm = paths::npm_command()?;
    npm.args(["view", "@openai/codex", "version", "--json"]);
    let output = run_output_with_timeout(&mut npm, NPM_VIEW_TIMEOUT)
        .map_err(|error| format!("无法运行系统 npm：{error}"))?;
    if !output.status.success() {
        return Err(format!(
            "npm 查询 @openai/codex 失败：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let latest =
        serde_json::from_str::<String>(&raw).unwrap_or_else(|_| raw.trim_matches('"').to_string());
    validate_version(&latest)?;
    let current = paths::codex_version(app)?;
    let update_available = match current.as_deref() {
        Some(current) => is_newer_version(&latest, current)?,
        None => true,
    };
    Ok(CodexUpdateInfo {
        current_version: current,
        latest_version: latest,
        update_available,
    })
}

pub fn stage_codex(app: &AppHandle, version: &str) -> Result<UpdateResult, String> {
    let version = validate_version(version)?;
    let node = paths::system_node()?;
    ensure_node_major(&node)?;
    let mut npm = paths::npm_command()?;
    let root = paths::codex_root(app)?;
    let staging = root.join(format!("staging-{version}"));
    if staging
        .join("node_modules/@openai/codex/package.json")
        .is_file()
    {
        smoke_codex(&node, app, &staging)?;
        return Ok(UpdateResult {
            version,
            path: staging.to_string_lossy().to_string(),
            kind: "codex-staged".to_string(),
        });
    }
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let status = run_status_with_timeout(
        npm.arg("install")
            .arg("--prefix")
            .arg(&staging)
            .arg("--no-package-lock")
            .arg(format!("@openai/codex@{version}"))
            .current_dir(&staging),
        NPM_TIMEOUT,
    );
    if let Err(error) = status {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("Codex 安装失败：{error}"));
    }
    if let Err(error) = smoke_codex(&node, app, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(format!("Codex 安装后 smoke 失败：{error}"));
    }
    Ok(UpdateResult {
        version,
        path: staging.to_string_lossy().to_string(),
        kind: "codex-staged".to_string(),
    })
}

pub fn activate_codex(app: &AppHandle, version: &str) -> Result<UpdateResult, String> {
    let version = validate_version(version)?;
    let node = paths::system_node()?;
    ensure_node_major(&node)?;
    let root = paths::codex_root(app)?;
    let staging = root.join(format!("staging-{version}"));
    if !staging
        .join("node_modules/@openai/codex/package.json")
        .is_file()
    {
        return Err(format!(
            "Codex staging 不存在或不完整：{}",
            staging.display()
        ));
    }
    smoke_codex(&node, app, &staging)?;
    let current = root.join("current");
    let previous = root.join("previous");
    if previous.exists() {
        fs::remove_dir_all(&previous).map_err(|error| error.to_string())?;
    }
    if current.exists() {
        fs::rename(&current, &previous).map_err(|error| error.to_string())?;
    }
    if let Err(error) = fs::rename(&staging, &current) {
        if previous.exists() && !current.exists() {
            let _ = fs::rename(&previous, &current);
        }
        return Err(error.to_string());
    }
    Ok(UpdateResult {
        version,
        path: current.to_string_lossy().to_string(),
        kind: "codex".to_string(),
    })
}

pub fn staged_app_updates(app: &AppHandle) -> Result<Vec<String>, String> {
    let root = paths::updates_dir(app)?;
    let mut files = Vec::new();
    for release in fs::read_dir(root).map_err(|error| error.to_string())? {
        let release = release.map_err(|error| error.to_string())?;
        if !release.path().is_dir() {
            continue;
        }
        for file in fs::read_dir(release.path()).map_err(|error| error.to_string())? {
            let file = file.map_err(|error| error.to_string())?.path();
            if file.is_file()
                && matches!(
                    file.extension().and_then(|extension| extension.to_str()),
                    Some("msi") | Some("exe")
                )
            {
                files.push(file.to_string_lossy().to_string());
            }
        }
    }
    files.sort();
    Ok(files)
}

pub fn stage_latest_release(app: &AppHandle) -> Result<UpdateResult, String> {
    let release = check_release()?;
    if !release.update_available {
        return Err("当前已是最新桌面版本".to_string());
    }
    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name.ends_with(".exe"))
        .or_else(|| {
            release
                .assets
                .iter()
                .find(|asset| asset.name.ends_with(".msi"))
        })
        .ok_or_else(|| "最新 GitHub Release 未包含 Windows 安装资产".to_string())?;
    download_release(
        app,
        &asset.download_url,
        &asset.name,
        asset.size,
        asset.digest.as_deref(),
        &release.tag_name,
    )
}

pub fn launch_desktop_update(app: &AppHandle, path: &str) -> Result<(), String> {
    let candidate = PathBuf::from(path);
    let updates = paths::updates_dir(app)?;
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("更新资产不可用：{error}"))?;
    let root = updates
        .canonicalize()
        .map_err(|error| format!("更新目录不可用：{error}"))?;
    if !canonical.starts_with(&root) {
        return Err("更新资产必须来自应用更新目录".to_string());
    }
    match canonical
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some(extension) if extension.eq_ignore_ascii_case("msi") => {
            Command::new("msiexec.exe")
                .args(["/i", &canonical.to_string_lossy(), "/passive", "/norestart"])
                .spawn()
                .map_err(|error| format!("无法启动 MSI 更新：{error}"))?;
        }
        Some(extension) if extension.eq_ignore_ascii_case("exe") => {
            Command::new(&canonical)
                .arg("/S")
                .spawn()
                .map_err(|error| format!("无法启动桌面更新：{error}"))?;
        }
        _ => return Err("桌面更新只支持 MSI 或 EXE 资产".to_string()),
    }
    app.exit(0);
    Ok(())
}

fn validate_version(value: &str) -> Result<String, String> {
    let version = value.trim().trim_start_matches('v');
    if version.is_empty()
        || version.chars().any(|character| {
            !(character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_'))
        })
    {
        return Err("Codex 版本号不合法".to_string());
    }
    Ok(version.to_string())
}

fn is_newer_version(candidate: &str, current: &str) -> Result<bool, String> {
    let candidate = Version::parse(candidate.trim().trim_start_matches('v'))
        .map_err(|error| format!("无法解析候选版本 {candidate}：{error}"))?;
    let current = Version::parse(current.trim().trim_start_matches('v'))
        .map_err(|error| format!("无法解析当前版本 {current}：{error}"))?;
    Ok(candidate > current)
}

fn ensure_node_major(node: &Path) -> Result<(), String> {
    let output = Command::new(node)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    let version = String::from_utf8_lossy(&output.stdout);
    let major = version
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| format!("无法解析 Node.js 版本：{version}"))?;
    if major < CODEX_MIN_NODE_MAJOR {
        return Err(format!(
            "Node.js 至少需要 v{CODEX_MIN_NODE_MAJOR}，当前为 {}",
            version.trim()
        ));
    }
    Ok(())
}

fn smoke_codex(node: &Path, app: &AppHandle, root: &Path) -> Result<(), String> {
    let entry = codex_entry_in(root)?;
    let smoke_home =
        paths::codex_root(app)?.join(format!(".smoke-home-{}", uuid::Uuid::new_v4().simple()));
    std::fs::create_dir_all(&smoke_home).map_err(|error| error.to_string())?;
    let result = run_status_with_timeout(
        Command::new(node)
            .arg(entry)
            .arg("--version")
            .env("CODEX_HOME", &smoke_home)
            .current_dir(root),
        CODEX_SMOKE_TIMEOUT,
    );
    let _ = std::fs::remove_dir_all(&smoke_home);
    result
}

fn codex_entry_in(root: &Path) -> Result<PathBuf, String> {
    let candidates = [
        root.join("node_modules/@openai/codex/bin/codex.js"),
        root.join("node_modules/@openai/codex/bin/codex"),
        root.join("node_modules/@openai/codex/dist/cli.js"),
    ];
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| format!("Codex 入口不存在：{}", root.display()))
}

fn run_status_with_timeout(command: &mut Command, timeout: Duration) -> Result<(), String> {
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => return Err(format!("进程退出码 {}", status.code().unwrap_or(-1))),
            None if started.elapsed() >= timeout => {
                let pid = child.id();
                let _ = child.kill();
                terminate_process_tree(pid);
                return Err(format!("进程超过 {:?} 未退出", timeout));
            }
            None => thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn run_output_with_timeout(
    command: &mut Command,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| error.to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(_) => return child.wait_with_output().map_err(|error| error.to_string()),
            None if started.elapsed() >= timeout => {
                let pid = child.id();
                let _ = child.kill();
                terminate_process_tree(pid);
                return Err(format!("进程超过 {:?} 未退出", timeout));
            }
            None => thread::sleep(Duration::from_millis(100)),
        }
    }
}

fn terminate_process_tree(pid: u32) {
    if cfg!(windows) {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn normalize_digest(value: &str) -> Result<String, String> {
    let value = value.trim().to_ascii_lowercase();
    let hex = value.strip_prefix("sha256:").unwrap_or(&value);
    if hex.len() != 64 || !hex.chars().all(|character| character.is_ascii_hexdigit()) {
        return Err(format!("GitHub asset digest 不合法：{value}"));
    }
    Ok(format!("sha256:{hex}"))
}

#[cfg(test)]
mod tests {
    use super::{is_newer_version, no_release_info, verify_download};
    use sha2::{Digest, Sha256};
    use std::fs;

    #[test]
    fn compares_github_and_npm_versions_without_downgrading() {
        assert!(is_newer_version("v0.148.0", "0.147.0").unwrap());
        assert!(!is_newer_version("v0.147.0", "0.147.0").unwrap());
        assert!(!is_newer_version("v0.146.0", "0.147.0").unwrap());
    }

    #[test]
    fn missing_github_release_is_a_normal_empty_result() {
        let release = no_release_info();
        assert!(release.tag_name.is_empty());
        assert!(!release.update_available);
        assert!(release.assets.is_empty());
    }

    #[test]
    fn verifies_and_rejects_existing_staged_downloads() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-update-verify-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&root).unwrap();
        let candidate = root.join("candidate.exe");
        fs::write(&candidate, b"verified candidate").unwrap();
        let digest = format!("sha256:{:x}", Sha256::digest(b"verified candidate"));

        let verified = verify_download(&candidate, 18, Some(&digest)).unwrap();
        assert_eq!(verified.0, 18);
        assert_eq!(verified.1, digest.trim_start_matches("sha256:"));
        assert!(verify_download(&candidate, 17, Some(&digest)).is_err());
        assert!(verify_download(
            &candidate,
            18,
            Some("sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        )
        .is_err());

        fs::remove_dir_all(root).unwrap();
    }
}
