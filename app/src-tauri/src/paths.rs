use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

pub fn app_data_root(app: &AppHandle) -> Result<PathBuf, String> {
    let root = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

pub fn codex_home(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app_data_root(app)?.join("codex-home");
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
}

pub fn server_profile_name(id: &str) -> String {
    format!("server-{}", safe_filename(id))
}

pub fn server_env_key(id: &str) -> String {
    let suffix = safe_filename(id).to_ascii_uppercase();
    format!("TAURI_CODEX_SERVER_{}_SK", suffix)
}

pub fn current_codex_dir(app: &AppHandle) -> Result<PathBuf, String> {
    if let Some(release) = crate::delivery::current_release_path(&app_data_root(app)?)? {
        return Ok(release.join("codex"));
    }
    let resource_root = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?;
    let resource = [
        resource_root.join("codex"),
        resource_root.join("resources/codex"),
    ]
    .into_iter()
    .find(|candidate| {
        candidate
            .join("node_modules/@openai/codex/package.json")
            .is_file()
    })
    .ok_or_else(|| "尚无 current release，且开发资源缺少应用私有 Codex".to_string())?;
    Ok(resource)
}

#[cfg(test)]
fn codex_version_in(root: &Path) -> Option<semver::Version> {
    let package = root.join("node_modules/@openai/codex/package.json");
    let value: serde_json::Value = serde_json::from_slice(&fs::read(package).ok()?).ok()?;
    semver::Version::parse(value.get("version")?.as_str()?).ok()
}

pub fn codex_entry(app: &AppHandle) -> Result<PathBuf, String> {
    let current = current_codex_dir(app)?;
    let candidates = [
        current.join("node_modules/@openai/codex/bin/codex.js"),
        current.join("node_modules/@openai/codex/bin/codex"),
        current.join("node_modules/@openai/codex/dist/cli.js"),
    ];
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            format!(
                "应用私有 Codex 不完整，请重新运行 tauri-codex Setup 修复：{}",
                current.display()
            )
        })
}

pub fn codex_version(app: &AppHandle) -> Result<Option<String>, String> {
    let package = current_codex_dir(app)?.join("node_modules/@openai/codex/package.json");
    if !package.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(package).map_err(|error| error.to_string())?;
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| error.to_string())?;
    Ok(value
        .get("version")
        .and_then(|version| version.as_str())
        .map(str::to_owned))
}

pub fn system_node() -> Result<PathBuf, String> {
    if let Some(configured) = std::env::var_os("TAURI_CODEX_SYSTEM_NODE") {
        let configured = PathBuf::from(configured);
        return if command_succeeds(&configured, ["--version"]) {
            Ok(configured)
        } else {
            Err("Launcher 指定的系统 Node.js 不可运行".to_string())
        };
    }
    for candidate in system_node_candidates() {
        if command_succeeds(&candidate, ["--version"]) {
            return Ok(candidate);
        }
    }
    Err("未找到可运行的系统 Node.js；请先安装 Node.js LTS".to_string())
}

pub fn validate_node(node: &Path) -> Result<(), String> {
    let output = crate::job::background_command(node)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err("Node.js --version 执行失败".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let major = text
        .trim()
        .trim_start_matches('v')
        .split('.')
        .next()
        .and_then(|value| value.parse::<u64>().ok())
        .ok_or_else(|| format!("无法解析 Node.js 版本：{}", text.trim()))?;
    if major < 16 {
        return Err(format!("Node.js 至少需要 v16，当前为 {}", text.trim()));
    }
    Ok(())
}

pub(crate) fn system_node_candidates() -> Vec<PathBuf> {
    let mut candidates = command_candidates(if cfg!(windows) { "node.exe" } else { "node" });
    if cfg!(windows) {
        for (root, suffix) in [
            (std::env::var_os("ProgramFiles"), "nodejs/node.exe"),
            (std::env::var_os("ProgramW6432"), "nodejs/node.exe"),
            (std::env::var_os("LOCALAPPDATA"), "Programs/nodejs/node.exe"),
        ] {
            let Some(root) = root else { continue };
            let candidate = PathBuf::from(root).join(suffix);
            if candidate.is_file() && !candidates.contains(&candidate) {
                candidates.push(candidate);
            }
        }
    }
    candidates
}

pub(crate) fn npm_cli_for_node(node: &Path) -> Option<PathBuf> {
    let node_dir = node.parent()?;
    [
        node_dir.join("node_modules/npm/bin/npm-cli.js"),
        node_dir.join("../lib/node_modules/npm/bin/npm-cli.js"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

pub(crate) fn system_npm_for_node(node: &Path) -> Result<PathBuf, String> {
    let cli = npm_cli_for_node(node).ok_or_else(|| {
        format!(
            "系统 Node.js 缺少 npm CLI：{}",
            node.parent().unwrap_or_else(|| Path::new(".")).display()
        )
    })?;
    let output = crate::job::background_command(node)
        .arg(&cli)
        .arg("--version")
        .output()
        .map_err(|error| format!("无法运行系统 npm：{error}"))?;
    if output.status.success() {
        Ok(cli)
    } else {
        Err(format!(
            "系统 npm 不可运行：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn command_candidates(name: &str) -> Vec<PathBuf> {
    let locator = if cfg!(windows) { "where.exe" } else { "which" };
    let Ok(output) = crate::job::background_command(locator).arg(name).output() else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

fn command_succeeds<I, S>(program: &Path, args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    crate::job::background_command(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub fn delivery_root() -> Result<PathBuf, String> {
    let roaming = std::env::var_os("APPDATA").ok_or_else(|| "APPDATA 不可用".to_string())?;
    Ok(PathBuf::from(roaming).join("com.tauri.codex"))
}

pub fn atomic_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
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
            .map_err(std::io::Error::other)
        }
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, target)
    }
}

pub fn write_atomic(target: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| std::io::Error::other("原子写入目标缺少父目录"))?;
    fs::create_dir_all(parent)?;
    let name = target
        .file_name()
        .ok_or_else(|| std::io::Error::other("原子写入目标缺少文件名"))?
        .to_string_lossy();
    let temporary = parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        atomic_replace(&temporary, target)?;
        #[cfg(not(windows))]
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{codex_version_in, npm_cli_for_node, write_atomic};
    use std::fs;

    #[test]
    fn finds_npm_cli_next_to_node_even_when_path_contains_spaces() {
        let root = std::env::temp_dir().join(format!(
            "tauri codex npm path {}",
            uuid::Uuid::new_v4().simple()
        ));
        let node = root.join("Program Files/nodejs/node.exe");
        let cli = root.join("Program Files/nodejs/node_modules/npm/bin/npm-cli.js");
        fs::create_dir_all(cli.parent().expect("npm parent")).expect("create npm tree");
        fs::write(&node, []).expect("create node marker");
        fs::write(&cli, []).expect("create npm marker");

        assert_eq!(npm_cli_for_node(&node), Some(cli));
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn reads_a_managed_codex_version_without_running_it() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-version-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let package = root.join("node_modules/@openai/codex/package.json");
        fs::create_dir_all(package.parent().expect("package parent")).expect("create package tree");
        fs::write(&package, br#"{"version":"0.147.0"}"#).expect("write package");

        assert_eq!(
            codex_version_in(&root).expect("version").to_string(),
            "0.147.0"
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn atomically_replaces_a_fully_flushed_file() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-atomic-write-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&root).expect("create fixture");
        let target = root.join("state.json");
        write_atomic(&target, b"first").expect("write first state");
        write_atomic(&target, b"second").expect("replace state");
        assert_eq!(fs::read(&target).expect("read state"), b"second");
        assert_eq!(
            fs::read_dir(&root).expect("read fixture").count(),
            1,
            "temporary files must not remain"
        );
        fs::remove_dir_all(root).expect("remove fixture");
    }
}

pub fn servers_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_root(app)?.join("servers.json"))
}

pub fn config_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(codex_home(app)?.join("config.toml"))
}

pub fn safe_filename(value: &str) -> String {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download.bin")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
