use crate::paths;
use semver::Version;
use std::path::PathBuf;

pub fn check_system_node() -> Result<(PathBuf, PathBuf), String> {
    if let Ok(node) = paths::system_node() {
        if let Ok(npm) = paths::system_npm_for_node(&node) {
            if paths::validate_node(&node).is_ok() {
                return Ok((node, npm));
            }
        }
    }
    Err("系统 Node.js/npm 不满足 Codex 运行要求；请重新运行 tauri-codex Setup 修复".to_string())
}

pub(crate) fn check_system_node_candidate_at_least(
    node: &std::path::Path,
    required: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let npm = paths::system_npm_for_node(node)?;
    check_system_node_candidate_at_least_with_npm(node, npm, required)
}

fn check_system_node_candidate_at_least_with_npm(
    node: &std::path::Path,
    npm: PathBuf,
    required: &str,
) -> Result<(PathBuf, PathBuf), String> {
    let required = Version::parse(required).map_err(|error| error.to_string())?;
    let output = crate::job::background_command(node)
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
    Ok((node.to_path_buf(), npm))
}
