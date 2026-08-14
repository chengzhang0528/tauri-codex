use super::contract::{artifact_url, Artifact, Component, ComponentId, Manifest};
use super::health;
use crate::job;
use reqwest::blocking::Client;
use sha2::Digest;
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zip::ZipArchive;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_UNPACKED_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_UNPACKED_FILE_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_UNPACKED_ENTRIES: usize = 100_000;
const MAX_UNPACKED_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageReleaseOutcome {
    Staged,
    RebootRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NodeInstallOutcome {
    Ready,
    RebootRequired,
}

pub fn stage_release(
    root: &Path,
    manifest: &Manifest,
    progress: &mut dyn FnMut(&str, &str, u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<StageReleaseOutcome, String> {
    ensure_not_cancelled(cancelled)?;
    let version = &manifest.payload.version;
    let staging = root.join("releases").join(format!("staging-{version}"));
    if staging.join(".ready").is_file()
        && fs::read(staging.join("release.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Manifest>(&bytes).ok())
            .as_ref()
            == Some(manifest)
    {
        if verify_staged_release(root, manifest).is_ok() {
            return Ok(StageReleaseOutcome::Staged);
        }
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let result = (|| -> Result<StageReleaseOutcome, String> {
        let cache = root.join("components");
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        let node = required(manifest, ComponentId::Node)?;
        let system_node = match health::doctor_system_node(&node.version) {
            Ok(system_node) => {
                progress("已复用系统组件", "Node.js", 0, 0);
                system_node
            }
            Err(_) => {
                let asset = cache.join(format!("{}.msi", node.artifact.sha256));
                download(&node.artifact, &asset, "Node.js", progress, cancelled)?;
                progress("正在验证组件", "Node.js", 0, 0);
                health::verify_authenticode(&asset)?;
                ensure_not_cancelled(cancelled)?;
                progress("正在安装系统组件", "Node.js", 0, 0);
                ensure_not_cancelled(cancelled)?;
                match install_node(&asset)? {
                    NodeInstallOutcome::Ready => health::doctor_system_node(&node.version)?,
                    NodeInstallOutcome::RebootRequired => {
                        return Ok(StageReleaseOutcome::RebootRequired)
                    }
                }
            }
        };
        for id in [ComponentId::Manager, ComponentId::Codex] {
            ensure_not_cancelled(cancelled)?;
            let component = required(manifest, id.clone())?;
            let asset = cache.join(format!("{}.zip", component.artifact.sha256));
            let label = id.as_str();
            download(&component.artifact, &asset, label, progress, cancelled)?;
            let destination = staging.join(&component.install_path);
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            progress("正在安全解包", label, 0, 0);
            unpack_zip(&asset, &destination, cancelled)?;
            verify_component_tree(component, &destination)?;
        }
        progress("正在检查组件", "Manager", 0, 0);
        ensure_not_cancelled(cancelled)?;
        health::doctor_manager(&staging.join("manager"), &system_node)?;
        progress("正在检查组件", "Codex", 0, 0);
        ensure_not_cancelled(cancelled)?;
        health::doctor_codex(&staging.join("codex"), &system_node)?;
        fs::write(
            staging.join("release.json"),
            serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::write(staging.join(".ready"), b"ready\n").map_err(|error| error.to_string())?;
        Ok(StageReleaseOutcome::Staged)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn verify_staged_release(root: &Path, manifest: &Manifest) -> Result<(), String> {
    let release = root
        .join("releases")
        .join(format!("staging-{}", manifest.payload.version));
    verify_release(manifest, &release, "staged")
}

pub fn verify_installed_release(root: &Path, manifest: &Manifest) -> Result<(), String> {
    let release = root.join("releases").join(&manifest.payload.version);
    verify_release(manifest, &release, "installed")
}

fn verify_release(manifest: &Manifest, release: &Path, state: &str) -> Result<(), String> {
    let stored: Manifest = serde_json::from_slice(
        &fs::read(release.join("release.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("{state} manifest 损坏：{error}"))?;
    super::contract::verify_envelope(&stored)?;
    if stored != *manifest || !release.join(".ready").is_file() {
        return Err(format!("{state} release manifest 或 ready 标记不匹配"));
    }
    for id in [ComponentId::Manager, ComponentId::Codex] {
        let component = required(manifest, id.clone())?;
        verify_component_tree(component, &release.join(&component.install_path))?;
    }
    let node = required(manifest, ComponentId::Node)?;
    health::doctor_system_node(&node.version)?;
    Ok(())
}

fn required<'a>(manifest: &'a Manifest, id: ComponentId) -> Result<&'a Component, String> {
    manifest
        .payload
        .components
        .iter()
        .find(|component| component.id == id && component.required)
        .ok_or_else(|| format!("manifest 缺少必需 {}", id.as_str()))
}

fn download(
    artifact: &Artifact,
    destination: &Path,
    label: &str,
    progress: &mut dyn FnMut(&str, &str, u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    ensure_not_cancelled(cancelled)?;
    if destination.is_file() {
        if verify_cached_artifact(destination, artifact).is_ok() {
            progress("已复用校验通过的组件", label, artifact.size, artifact.size);
            return Ok(());
        }
        let _ = fs::remove_file(destination);
    }
    let partial = destination.with_extension("part");
    let mut response = Client::builder()
        .user_agent("tauri-codex-launcher")
        .timeout(NETWORK_TIMEOUT)
        .build()
        .map_err(|error| error.to_string())?
        .get(artifact_url(&artifact.object_key)?)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut file = File::create(&partial).map_err(|error| error.to_string())?;
    let mut hasher = sha2::Sha256::new();
    let mut size = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    progress("正在下载组件", label, 0, artifact.size);
    let result = (|| -> Result<(), String> {
        loop {
            ensure_not_cancelled(cancelled)?;
            let read = response
                .read(&mut buffer)
                .map_err(|error| error.to_string())?;
            if read == 0 {
                break;
            }
            size += read as u64;
            if size > artifact.size {
                return Err(format!("{label} 下载超过清单大小"));
            }
            file.write_all(&buffer[..read])
                .map_err(|error| error.to_string())?;
            hasher.update(&buffer[..read]);
            progress("正在下载组件", label, size, artifact.size);
        }
        if size != artifact.size || format!("{:x}", hasher.finalize()) != artifact.sha256 {
            return Err(format!("{label} size/SHA-256 不匹配"));
        }
        file.sync_all().map_err(|error| error.to_string())?;
        Ok(())
    })();
    drop(file);
    if result.is_err() {
        let _ = fs::remove_file(&partial);
        return result;
    }
    fs::rename(&partial, destination).map_err(|error| error.to_string())
}

fn unpack_zip(
    archive_path: &Path,
    destination: &Path,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), String> {
    let file = File::open(archive_path).map_err(|error| error.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|error| error.to_string())?;
    if archive.len() > MAX_UNPACKED_ENTRIES {
        return Err("组件归档文件数量超过上限".to_string());
    }
    let mut seen = HashSet::new();
    let mut unpacked = 0_u64;
    for index in 0..archive.len() {
        ensure_not_cancelled(cancelled)?;
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "组件归档包含不安全路径".to_string())?
            .to_owned();
        let normalized = validate_archive_path(&relative)?;
        if relative.components().count() > MAX_UNPACKED_DEPTH {
            return Err("组件归档目录深度超过上限".to_string());
        }
        if !seen.insert(normalized) {
            return Err(format!("组件归档包含重复路径：{}", entry.name()));
        }
        if entry.is_symlink() || (!entry.is_dir() && !entry.is_file()) {
            return Err(format!("组件归档包含不允许的文件类型：{}", entry.name()));
        }
        let declared_size = entry.size();
        let declared_total = unpacked.checked_add(declared_size);
        if declared_size > MAX_UNPACKED_FILE_BYTES
            || declared_total.is_none()
            || declared_total.unwrap_or(u64::MAX) > MAX_UNPACKED_BYTES
        {
            return Err("组件归档解压大小超过上限".to_string());
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut target = File::create(&output).map_err(|error| error.to_string())?;
            let copied = std::io::copy(&mut (&mut entry).take(declared_size + 1), &mut target)
                .map_err(|error| error.to_string())?;
            if copied != declared_size {
                return Err(format!("组件归档文件大小不匹配：{}", entry.name()));
            }
            unpacked = unpacked
                .checked_add(copied)
                .ok_or_else(|| "组件归档解压大小溢出".to_string())?;
        }
    }
    Ok(())
}

fn validate_archive_path(path: &Path) -> Result<String, String> {
    let mut normalized = Vec::new();
    for component in path.components() {
        let std::path::Component::Normal(component) = component else {
            return Err("组件归档包含不安全路径".to_string());
        };
        let value = component.to_string_lossy();
        if value.is_empty() || value.contains(':') || value.ends_with('.') || value.ends_with(' ') {
            return Err(format!(
                "组件归档包含 Windows 不安全路径：{}",
                path.display()
            ));
        }
        let stem = value
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        let reserved = matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit()
                && stem.as_bytes()[3] != b'0');
        if reserved {
            return Err(format!("组件归档包含 Windows 保留路径：{}", path.display()));
        }
        normalized.push(value.to_lowercase());
    }
    if normalized.is_empty() {
        return Err("组件归档包含空路径".to_string());
    }
    Ok(normalized.join("/"))
}

fn verify_cached_artifact(path: &Path, artifact: &Artifact) -> Result<(), String> {
    let metadata = fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file()
        || metadata.len() != artifact.size
        || sha256_file(path)? != artifact.sha256
    {
        return Err(format!("缓存组件 identity 不匹配：{}", path.display()));
    }
    Ok(())
}

fn verify_component_tree(component: &Component, root: &Path) -> Result<(), String> {
    let expected = component
        .installed_tree_sha256
        .as_deref()
        .ok_or_else(|| format!("{} 缺少安装树 SHA-256", component.id.as_str()))?;
    let actual = installed_tree_sha256(root)?;
    if actual != expected {
        return Err(format!("{} 安装树 SHA-256 不匹配", component.id.as_str()));
    }
    Ok(())
}

fn installed_tree_sha256(root: &Path) -> Result<String, String> {
    if !root.is_dir() {
        return Err(format!("组件安装目录缺失：{}", root.display()));
    }
    let mut files = Vec::new();
    let mut entries = 0usize;
    let mut total_bytes = 0u64;
    collect_tree_files(root, root, 0, &mut entries, &mut total_bytes, &mut files)?;
    files.sort_by(|left, right| left.0.cmp(&right.0));
    for pair in files.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(format!("组件安装树包含大小写冲突路径：{}", pair[0].0));
        }
    }
    let mut tree = sha2::Sha256::new();
    for (relative, path, size) in files {
        tree.update(relative.as_bytes());
        tree.update([0]);
        tree.update(size.to_string().as_bytes());
        tree.update([0]);
        tree.update(sha256_file(&path)?.as_bytes());
        tree.update(b"\n");
    }
    Ok(format!("{:x}", tree.finalize()))
}

fn collect_tree_files(
    root: &Path,
    directory: &Path,
    depth: usize,
    entries: &mut usize,
    total_bytes: &mut u64,
    files: &mut Vec<(String, PathBuf, u64)>,
) -> Result<(), String> {
    if depth > MAX_UNPACKED_DEPTH {
        return Err("组件安装树目录深度超过上限".to_string());
    }
    for entry in fs::read_dir(directory).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        *entries = entries
            .checked_add(1)
            .ok_or_else(|| "组件安装树条目数量溢出".to_string())?;
        if *entries > MAX_UNPACKED_ENTRIES {
            return Err("组件安装树条目数量超过上限".to_string());
        }
        if metadata.file_type().is_symlink() {
            return Err(format!("组件安装树不允许符号链接：{}", path.display()));
        }
        if metadata.is_dir() {
            collect_tree_files(root, &path, depth + 1, entries, total_bytes, files)?;
            continue;
        }
        if !metadata.is_file() {
            return Err(format!("组件安装树包含特殊文件：{}", path.display()));
        }
        if metadata.len() > MAX_UNPACKED_FILE_BYTES {
            return Err("组件安装树单文件大小超过上限".to_string());
        }
        *total_bytes = total_bytes
            .checked_add(metadata.len())
            .ok_or_else(|| "组件安装树总大小溢出".to_string())?;
        if *total_bytes > MAX_UNPACKED_BYTES {
            return Err("组件安装树总大小超过上限".to_string());
        }
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .components()
            .map(|component| component.as_os_str().to_string_lossy().to_lowercase())
            .collect::<Vec<_>>()
            .join("/");
        files.push((relative, path, metadata.len()));
    }
    Ok(())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = sha2::Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn ensure_not_cancelled(cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    if cancelled() {
        Err("更新准备已取消".to_string())
    } else {
        Ok(())
    }
}

fn install_node(installer: &Path) -> Result<NodeInstallOutcome, String> {
    let status = job::background_command("msiexec.exe")
        .args(["/i", &installer.to_string_lossy(), "/passive", "/norestart"])
        .status()
        .map_err(|error| format!("无法运行 Node.js 安装程序：{error}"))?;
    classify_node_installer_exit(status.code())
}

fn classify_node_installer_exit(code: Option<i32>) -> Result<NodeInstallOutcome, String> {
    match code {
        Some(0) => Ok(NodeInstallOutcome::Ready),
        Some(3010) => Ok(NodeInstallOutcome::RebootRequired),
        Some(code) => Err(format!("Node.js 安装程序退出码 {code}")),
        None => Err("Node.js 安装程序被终止".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_node_installer_exit, installed_tree_sha256, validate_archive_path,
        NodeInstallOutcome,
    };
    use std::fs;
    use std::path::Path;

    #[test]
    fn archive_paths_reject_windows_aliases_and_reserved_names() {
        assert_eq!(
            validate_archive_path(Path::new("Manager/Bin.exe")).unwrap(),
            "manager/bin.exe"
        );
        assert!(validate_archive_path(Path::new("manager/data:stream")).is_err());
        assert!(validate_archive_path(Path::new("manager/CON.txt")).is_err());
        assert!(validate_archive_path(Path::new("manager/file. ")).is_err());
    }

    #[test]
    fn installed_tree_digest_is_stable_and_content_sensitive() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-tree-digest-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(root.join("Bin")).unwrap();
        fs::write(root.join("Bin/app.exe"), b"app").unwrap();
        fs::write(root.join("readme.txt"), b"docs").unwrap();
        assert_eq!(
            installed_tree_sha256(&root).unwrap(),
            "7df47593086e0fceca3c8194935fee043384498c3ec6f31d86aebdf599eae4db"
        );
        assert_eq!(
            installed_tree_sha256(&root).unwrap(),
            installed_tree_sha256(&root).unwrap()
        );
        let before = installed_tree_sha256(&root).unwrap();
        fs::write(root.join("readme.txt"), b"changed").unwrap();
        assert_ne!(before, installed_tree_sha256(&root).unwrap());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn node_installer_exit_3010_requires_reboot_without_failing() {
        assert_eq!(
            classify_node_installer_exit(Some(0)).unwrap(),
            NodeInstallOutcome::Ready
        );
        assert_eq!(
            classify_node_installer_exit(Some(3010)).unwrap(),
            NodeInstallOutcome::RebootRequired
        );
        assert!(classify_node_installer_exit(Some(1603)).is_err());
        assert!(classify_node_installer_exit(None).is_err());
    }
}
