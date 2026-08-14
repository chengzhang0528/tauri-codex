use super::contract::{artifact_url, digest, Artifact, Component, ComponentId, Manifest};
use super::health;
use crate::{job, runtime};
use reqwest::blocking::Client;
use sha2::Digest;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zip::ZipArchive;

const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);

pub fn stage_release(
    root: &Path,
    manifest: &Manifest,
    progress: &mut dyn FnMut(&str, &str, u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<PathBuf, String> {
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
        return Ok(staging);
    }
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|error| error.to_string())?;
    }
    fs::create_dir_all(&staging).map_err(|error| error.to_string())?;
    let result = (|| -> Result<PathBuf, String> {
        let cache = root.join("components");
        fs::create_dir_all(&cache).map_err(|error| error.to_string())?;
        let node = required(manifest, ComponentId::Node)?;
        if runtime::check_system_node_at_least(&node.version).is_err() {
            let asset = cache.join(format!("{}.msi", node.artifact.sha256));
            download(&node.artifact, &asset, "Node.js", progress, cancelled)?;
            progress("正在验证组件", "Node.js", 0, 0);
            health::verify_authenticode(&asset)?;
            ensure_not_cancelled(cancelled)?;
            progress("正在安装系统组件", "Node.js", 0, 0);
            install_node(&asset)?;
        } else {
            progress("已复用系统组件", "Node.js", 0, 0);
        }
        ensure_not_cancelled(cancelled)?;
        runtime::check_system_node_at_least(&node.version)?;
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
        }
        progress("正在检查组件", "Manager", 0, 0);
        ensure_not_cancelled(cancelled)?;
        health::doctor_manager(&staging.join("manager"))?;
        progress("正在检查组件", "Codex", 0, 0);
        ensure_not_cancelled(cancelled)?;
        health::doctor_codex(&staging.join("codex"))?;
        fs::write(
            staging.join("release.json"),
            serde_json::to_vec_pretty(manifest).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        fs::write(staging.join(".ready"), b"ready\n").map_err(|error| error.to_string())?;
        Ok(staging.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&staging);
    }
    result
}

pub fn stage_installer(
    root: &Path,
    version: &str,
    artifact: &Artifact,
    progress: &mut dyn FnMut(&str, &str, u64, u64),
    cancelled: &dyn Fn() -> bool,
) -> Result<PathBuf, String> {
    ensure_not_cancelled(cancelled)?;
    let dir = root.join("installer-updates").join(version);
    fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let target = dir.join(format!("tauri-codex_{version}_x64-setup.exe"));
    download(artifact, &target, "Installer", progress, cancelled)?;
    progress("正在验证组件", "Installer", 0, 0);
    ensure_not_cancelled(cancelled)?;
    health::verify_authenticode(&target)?;
    fs::write(dir.join(".ready"), b"ready\n").map_err(|error| error.to_string())?;
    Ok(target)
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
        let bytes = fs::read(destination).map_err(|error| error.to_string())?;
        if bytes.len() as u64 == artifact.size && digest(&bytes) == artifact.sha256 {
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
    for index in 0..archive.len() {
        ensure_not_cancelled(cancelled)?;
        let mut entry = archive.by_index(index).map_err(|error| error.to_string())?;
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| "组件归档包含不安全路径".to_string())?
            .to_owned();
        if entry.is_symlink() || (!entry.is_dir() && !entry.is_file()) {
            return Err(format!("组件归档包含不允许的文件类型：{}", entry.name()));
        }
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output).map_err(|error| error.to_string())?;
        } else {
            if let Some(parent) = output.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            let mut target = File::create(&output).map_err(|error| error.to_string())?;
            std::io::copy(&mut entry, &mut target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn ensure_not_cancelled(cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    if cancelled() {
        Err("更新准备已取消".to_string())
    } else {
        Ok(())
    }
}

fn install_node(installer: &Path) -> Result<(), String> {
    let status = job::background_command("msiexec.exe")
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
