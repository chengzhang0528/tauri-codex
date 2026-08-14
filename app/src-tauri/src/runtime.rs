use crate::paths;
use semver::Version;
use std::path::PathBuf;

pub fn check_system_node() -> Result<(PathBuf, PathBuf), String> {
    if let (Ok(node), Ok(npm)) = (paths::system_node(), paths::system_npm()) {
        if paths::validate_node(&node).is_ok() {
            return Ok((node, npm));
        }
    }
    Err("系统 Node.js/npm 不满足 Codex 运行要求；请重新运行 tauri-codex Setup 修复".to_string())
}

pub fn check_system_node_at_least(required: &str) -> Result<(PathBuf, PathBuf), String> {
    let (node, npm) = check_system_node()?;
    let required = Version::parse(required).map_err(|error| error.to_string())?;
    let output = crate::job::background_command(&node)
        .arg("--version")
        .output()
        .map_err(|error| error.to_string())?;
    let actual = Version::parse(
        String::from_utf8_lossy(&output.stdout)
            .trim()
            .trim_start_matches('v'),
    )
    .map_err(|error| format!("无法解析 Node.js 版本：{error}"))?;
    if actual < required {
        return Err(format!("系统 Node.js {actual} 低于清单要求 {required}"));
    }
    Ok((node, npm))
}
