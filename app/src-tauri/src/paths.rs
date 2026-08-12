use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use tauri::{AppHandle, Manager};

pub const GITHUB_REPOSITORY: &str = "chengzhang0528/tauri-codex";

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

pub fn codex_root(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app_data_root(app)?.join("codex");
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
    let independently_managed = codex_root(app)?.join("current");
    let managed = app_data_root(app)?.join("releases/current/codex");
    match (
        codex_version_in(&independently_managed),
        codex_version_in(&managed),
    ) {
        (Some(independent), Some(release)) if release > independent => Ok(managed),
        (Some(_), _) => Ok(independently_managed),
        (None, Some(_)) => Ok(managed),
        (None, None) => Ok(independently_managed),
    }
}

fn codex_version_in(root: &Path) -> Option<semver::Version> {
    let package = root.join("node_modules/@openai/codex/package.json");
    let value: serde_json::Value = serde_json::from_slice(&fs::read(package).ok()?).ok()?;
    semver::Version::parse(value.get("version")?.as_str()?).ok()
}

pub fn codex_entry(app: &AppHandle) -> Result<PathBuf, String> {
    ensure_bundled_codex(app)?;
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
                "应用私有 Codex 尚未安装，请在更新页安装 @openai/codex 到 {}",
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

pub fn pending_codex_versions(app: &AppHandle) -> Result<Vec<String>, String> {
    let root = codex_root(app)?;
    let mut versions = fs::read_dir(root)
        .map_err(|error| error.to_string())?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_prefix("staging-")
                .filter(|version| !version.is_empty())
                .map(str::to_string)
        })
        .collect::<Vec<_>>();
    versions.sort();
    Ok(versions)
}

pub fn system_node() -> Result<PathBuf, String> {
    let candidates = node_candidates();
    for candidate in candidates {
        if command_succeeds(&candidate, ["--version"]) {
            return Ok(candidate);
        }
    }
    Err("未找到可运行的系统 Node.js；请先安装 Node.js LTS".to_string())
}

pub fn system_npm() -> Result<PathBuf, String> {
    if cfg!(windows) {
        let node = system_node()?;
        let cli = npm_cli_for_node(&node).ok_or_else(|| {
            format!(
                "系统 Node.js 缺少 npm CLI：{}",
                node.parent().unwrap_or_else(|| Path::new(".")).display()
            )
        })?;
        let output = Command::new(&node)
            .arg(&cli)
            .arg("--version")
            .output()
            .map_err(|error| format!("无法运行系统 npm：{error}"))?;
        if output.status.success() {
            return Ok(cli);
        }
        return Err(format!(
            "系统 npm 不可运行：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }

    for candidate in command_candidates("npm") {
        if command_succeeds(&candidate, ["--version"]) {
            return Ok(candidate);
        }
    }
    Err("未找到可运行的系统 npm；请先安装 Node.js LTS".to_string())
}

pub fn validate_node(node: &Path) -> Result<(), String> {
    let output = Command::new(node)
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

pub fn npm_command() -> Result<Command, String> {
    let npm = system_npm()?;
    if cfg!(windows) {
        let node = system_node()?;
        let mut command = Command::new(node);
        command.arg(npm);
        Ok(command)
    } else {
        Ok(Command::new(npm))
    }
}

fn node_candidates() -> Vec<PathBuf> {
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

fn npm_cli_for_node(node: &Path) -> Option<PathBuf> {
    let node_dir = node.parent()?;
    [
        node_dir.join("node_modules/npm/bin/npm-cli.js"),
        node_dir.join("../lib/node_modules/npm/bin/npm-cli.js"),
    ]
    .into_iter()
    .find(|candidate| candidate.is_file())
}

fn command_candidates(name: &str) -> Vec<PathBuf> {
    let locator = if cfg!(windows) { "where.exe" } else { "which" };
    let Ok(output) = Command::new(locator).arg(name).output() else {
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
    Command::new(program)
        .args(args)
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

pub(crate) fn ensure_bundled_codex(app: &AppHandle) -> Result<(), String> {
    let current = current_codex_dir(app)?;
    if current
        .join("node_modules/@openai/codex/package.json")
        .is_file()
    {
        return Ok(());
    }

    let resource_root = app
        .path()
        .resource_dir()
        .map_err(|error| error.to_string())?;
    let candidates = [
        resource_root.join("codex"),
        resource_root.join("resources/codex"),
    ];
    let source = candidates
        .iter()
        .find(|candidate| candidate.join("node_modules/@openai/codex/package.json").is_file())
        .ok_or_else(|| {
            format!(
                "应用私有 Codex 尚未安装；请准备安装包内 resources/codex 或在更新页安装 @openai/codex 到 {}",
                current.display()
            )
        })?;
    if current.exists() {
        fs::remove_dir_all(&current).map_err(|error| error.to_string())?;
    }
    copy_dir(source, &current)?;
    Ok(())
}

fn copy_dir(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir(&source_path, &destination_path)?;
        } else {
            fs::copy(&source_path, &destination_path).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{codex_version_in, npm_cli_for_node};
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
}

pub fn servers_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(app_data_root(app)?.join("servers.json"))
}

pub fn config_file(app: &AppHandle) -> Result<PathBuf, String> {
    Ok(codex_home(app)?.join("config.toml"))
}

pub fn updates_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let path = app_data_root(app)?.join("updates");
    fs::create_dir_all(&path).map_err(|error| error.to_string())?;
    Ok(path)
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
