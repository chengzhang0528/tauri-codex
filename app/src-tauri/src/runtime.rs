use crate::paths;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn check_system_node() -> Result<(PathBuf, PathBuf), String> {
    if let (Ok(node), Ok(npm)) = (paths::system_node(), paths::system_npm()) {
        if paths::validate_node(&node).is_ok() {
            return Ok((node, npm));
        }
    }
    Err("系统 Node.js/npm 不满足 Codex 运行要求；请重新运行 tauri-codex Setup 修复".to_string())
}

pub fn ensure_system_node_from_install_dir() -> Result<(PathBuf, PathBuf), String> {
    if let Ok(runtime) = check_system_node() {
        return Ok(runtime);
    }
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let install_dir = executable
        .parent()
        .ok_or_else(|| "无法确定 tauri-codex 安装目录".to_string())?;
    ensure_system_node_from_resources(&install_dir.join("resources"))
}

fn ensure_system_node_from_resources(resource_root: &Path) -> Result<(PathBuf, PathBuf), String> {
    let installer = find_node_installer(resource_root).ok_or_else(|| {
        "系统 Node.js/npm 不满足要求，安装包未包含官方 Node.js LTS x64 MSI".to_string()
    })?;
    let status = Command::new("msiexec.exe")
        .args(["/i", &installer.to_string_lossy(), "/passive", "/norestart"])
        .status()
        .map_err(|error| format!("无法运行 Node.js 安装程序：{error}"))?;
    if !status.success() {
        return Err(format!(
            "Node.js 安装程序退出码 {}",
            status.code().unwrap_or(-1)
        ));
    }
    let node = paths::system_node()?;
    paths::validate_node(&node)?;
    let npm = paths::system_npm()?;
    Ok((node, npm))
}

fn find_node_installer(resource_root: &std::path::Path) -> Option<PathBuf> {
    let node_root = resource_root.join("node");
    std::fs::read_dir(node_root)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| {
            path.extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| extension.eq_ignore_ascii_case("msi"))
        })
}
