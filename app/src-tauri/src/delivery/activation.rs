use super::contract::{Manifest, UpdateTarget};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;

const RELEASE_STATE_SCHEMA: u32 = 2;
const ACTIVATION_JOURNAL_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivationJournal {
    schema_version: u32,
    target_version: String,
    old_version: Option<String>,
    quarantine: String,
    phase: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CurrentState {
    schema_version: u32,
    current: String,
}

pub fn release_path(root: &Path, version: &str) -> PathBuf {
    root.join("releases").join(version)
}
pub fn staging_path(root: &Path, version: &str) -> PathBuf {
    root.join("releases").join(format!("staging-{version}"))
}

pub fn current_release_version(root: &Path) -> Result<Option<String>, String> {
    Ok(read_current(root)?.map(|state| state.current))
}

pub fn current_release_path(root: &Path) -> Result<Option<PathBuf>, String> {
    let Some(state) = read_current(root)? else {
        return Ok(None);
    };
    let path = release_path(root, &state.current);
    validate_installed(&path, &state.current)?;
    Ok(Some(path))
}

pub fn current_component_versions(root: &Path) -> Result<(Option<String>, Option<String>), String> {
    let Some(path) = current_release_path(root)? else {
        return Ok((None, None));
    };
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(path.join("release.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    super::contract::verify_envelope(&manifest)?;
    let codex = manifest
        .payload
        .components
        .iter()
        .find(|component| component.id.as_str() == "codex")
        .map(|component| component.version.clone());
    let node = manifest
        .payload
        .components
        .iter()
        .find(|component| component.id.as_str() == "node")
        .map(|component| component.version.clone());
    Ok((codex, node))
}

pub fn current_manager_version(root: &Path) -> Result<Option<String>, String> {
    let Some(path) = current_release_path(root)? else {
        return Ok(None);
    };
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(path.join("release.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    super::contract::verify_envelope(&manifest)?;
    Ok(manifest
        .payload
        .components
        .iter()
        .find(|component| {
            component.id == super::contract::ComponentId::Manager && component.required
        })
        .map(|component| component.version.clone()))
}

pub fn commit_staged(root: &Path, target: &UpdateTarget) -> Result<(), String> {
    let version = match target {
        UpdateTarget::Release { version } => version,
        UpdateTarget::Installer { .. } => {
            return Err("Installer 激活由 Launcher setup 负责".to_string())
        }
    };
    let staging = staging_path(root, version);
    let installed = release_path(root, version);
    if staging.join(".ready").is_file() {
        validate_installed(&staging, version)?;
        if installed.exists() {
            let quarantine = root.join("releases").join(format!(
                "repair-{version}-{}",
                uuid::Uuid::new_v4().simple()
            ));
            let old_version = read_current(root)?.map(|state| state.current);
            let mut journal = ActivationJournal {
                schema_version: ACTIVATION_JOURNAL_SCHEMA,
                target_version: version.clone(),
                old_version,
                quarantine: quarantine
                    .file_name()
                    .expect("quarantine has a file name")
                    .to_string_lossy()
                    .to_string(),
                phase: "prepared".to_string(),
            };
            write_journal(root, &journal)?;
            move_directory(&installed, &quarantine).map_err(|error| {
                let _ = clear_journal(root);
                format!("无法隔离待替换 release：{error}")
            })?;
            journal.phase = "old-quarantined".to_string();
            if let Err(error) = write_journal(root, &journal) {
                return Err(recover_after_failure(root, error));
            }
            if let Err(error) = move_directory(&staging, &installed) {
                return Err(recover_after_failure(
                    root,
                    format!("无法提交 release：{error}"),
                ));
            }
            journal.phase = "candidate-installed".to_string();
            if let Err(error) = write_journal(root, &journal) {
                return Err(recover_after_failure(root, error));
            }
            if let Err(error) = validate_installed(&installed, version) {
                return Err(recover_after_failure(root, error));
            }
            if let Err(error) = write_current(
                root,
                &CurrentState {
                    schema_version: RELEASE_STATE_SCHEMA,
                    current: version.clone(),
                },
            ) {
                return Err(recover_after_failure(root, error));
            }
            journal.phase = "current-committed".to_string();
            if let Err(error) = write_journal(root, &journal) {
                return Err(recover_after_failure(root, error));
            }
            clear_journal(root)?;
            return Ok(());
        }
        move_directory(&staging, &installed)
            .map_err(|error| format!("无法提交 release：{error}"))?;
        validate_installed(&installed, version)?;
    } else if installed.exists() {
        validate_installed(&installed, version)?;
    } else {
        return Err(format!("release {version} 尚未完成 stage"));
    }
    write_current(
        root,
        &CurrentState {
            schema_version: RELEASE_STATE_SCHEMA,
            current: version.clone(),
        },
    )
}

pub fn recover_pending(root: &Path) -> Result<(), String> {
    let path = journal_path(root);
    if !path.is_file() {
        return Ok(());
    }
    let journal: ActivationJournal =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("激活 journal 损坏：{error}"))?;
    if journal.schema_version != ACTIVATION_JOURNAL_SCHEMA {
        return Err("激活 journal schema 不受支持".to_string());
    }
    super::contract::validate_version(&journal.target_version)?;
    if Path::new(&journal.quarantine).components().count() != 1
        || !journal
            .quarantine
            .starts_with(&format!("repair-{}-", journal.target_version))
    {
        return Err("激活 journal quarantine 路径不安全".to_string());
    }
    if let Some(old_version) = &journal.old_version {
        super::contract::validate_version(old_version)?;
    }
    let releases = root.join("releases");
    let installed = release_path(root, &journal.target_version);
    let staging = staging_path(root, &journal.target_version);
    let quarantine = releases.join(&journal.quarantine);
    match journal.phase.as_str() {
        "prepared" => {
            if quarantine.is_dir() {
                if installed.exists() {
                    return Err(
                        "激活 journal prepared 状态同时存在 installed 与 quarantine".to_string()
                    );
                }
                move_directory(&quarantine, &installed)
                    .map_err(|error| format!("无法恢复已隔离的旧 release：{error}"))?;
            } else if !installed.is_dir() {
                return Err("激活 journal prepared 状态缺少旧 release".to_string());
            }
            clear_journal(root)?;
        }
        "old-quarantined" | "candidate-installed" => {
            restore_pre_activation_state(root, &journal, &installed, &staging, &quarantine)?;
            clear_journal(root)?;
        }
        "current-committed" => {
            let current_is_target = read_current(root)?
                .as_ref()
                .is_some_and(|state| state.current == journal.target_version);
            if !current_is_target
                || validate_installed(&installed, &journal.target_version).is_err()
            {
                restore_pre_activation_state(root, &journal, &installed, &staging, &quarantine)?;
            }
            clear_journal(root)?;
        }
        other => return Err(format!("激活 journal phase 不受支持：{other}")),
    }
    Ok(())
}

fn restore_pre_activation_state(
    root: &Path,
    journal: &ActivationJournal,
    installed: &Path,
    staging: &Path,
    quarantine: &Path,
) -> Result<(), String> {
    if !quarantine.is_dir() {
        return Err("激活 journal 缺少可恢复的旧 release".to_string());
    }
    if installed.exists() {
        if staging.exists() {
            fs::remove_dir_all(staging).map_err(|error| error.to_string())?;
        }
        move_directory(installed, staging)
            .map_err(|error| format!("无法恢复未提交 candidate：{error}"))?;
    }
    move_directory(quarantine, installed)
        .map_err(|error| format!("无法恢复旧 release：{error}"))?;
    match &journal.old_version {
        Some(version) => write_current(
            root,
            &CurrentState {
                schema_version: RELEASE_STATE_SCHEMA,
                current: version.clone(),
            },
        ),
        None => remove_current(root),
    }
}

fn recover_after_failure(root: &Path, error: String) -> String {
    match recover_pending(root) {
        Ok(()) => error,
        Err(recovery) => format!("{error}；激活恢复失败：{recovery}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        commit_staged, move_directory, recover_pending, release_path, staging_path, write_journal,
        ActivationJournal, ACTIVATION_JOURNAL_SCHEMA,
    };
    use crate::delivery::contract::UpdateTarget;
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use ed25519_dalek::{Signer, SigningKey};
    use serde_json::json;
    use std::fs;

    fn write_release(path: &Path, version: &str, marker: &str) {
        fs::create_dir_all(path).unwrap();
        fs::write(path.join(".ready"), b"ready\n").unwrap();
        fs::write(path.join("marker.txt"), marker).unwrap();
        let payload = json!({
            "product": "tauri-codex",
            "version": version,
            "platform": "windows",
            "architecture": "x86_64",
            "minimumLauncherVersion": "1.1.0",
            "minimumManagerVersion": version,
            "components": []
        });
        let signing = SigningKey::from_bytes(&[
            0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec,
            0x2c, 0xc4, 0x44, 0x49, 0xc5, 0x69, 0x7b, 0x32, 0x69, 0x19, 0x70, 0x3b, 0xac, 0x03,
            0x1c, 0xae, 0x7f, 0x60,
        ]);
        let signature = STANDARD.encode(
            signing
                .sign(&crate::delivery::contract::canonical_json(&payload))
                .to_bytes(),
        );
        fs::write(
            path.join("release.json"),
            serde_json::to_vec(&json!({
                "schemaVersion": 2,
                "keyId": "development-rfc8032",
                "payload": payload,
                "signature": signature
            }))
            .unwrap(),
        )
        .unwrap();
    }

    use std::path::Path;

    #[test]
    fn forward_repair_replaces_same_version_without_previous_state() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-forward-repair-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let version = "0.2.1";
        write_release(&staging_path(&root, version), version, "failed");
        commit_staged(
            &root,
            &UpdateTarget::Release {
                version: version.to_string(),
            },
        )
        .unwrap();
        write_release(&staging_path(&root, version), version, "repaired");

        commit_staged(
            &root,
            &UpdateTarget::Release {
                version: version.to_string(),
            },
        )
        .unwrap();

        assert_eq!(
            fs::read_to_string(release_path(&root, version).join("marker.txt")).unwrap(),
            "repaired"
        );
        let current: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("releases/current.json")).unwrap()).unwrap();
        assert_eq!(current, json!({"schemaVersion": 2, "current": version}));
        assert!(fs::read_dir(root.join("releases"))
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("repair-0.2.1-")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interrupted_same_version_activation_restores_old_current() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-activation-recovery-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let version = "0.2.1";
        write_release(&staging_path(&root, version), version, "old");
        commit_staged(
            &root,
            &UpdateTarget::Release {
                version: version.to_string(),
            },
        )
        .unwrap();
        write_release(&staging_path(&root, version), version, "candidate");
        let quarantine_name = format!("repair-{version}-test");
        let quarantine = root.join("releases").join(&quarantine_name);
        let mut journal = ActivationJournal {
            schema_version: ACTIVATION_JOURNAL_SCHEMA,
            target_version: version.to_string(),
            old_version: Some(version.to_string()),
            quarantine: quarantine_name,
            phase: "old-quarantined".to_string(),
        };
        write_journal(&root, &journal).unwrap();
        move_directory(&release_path(&root, version), &quarantine).unwrap();
        move_directory(&staging_path(&root, version), &release_path(&root, version)).unwrap();
        journal.phase = "candidate-installed".to_string();
        write_journal(&root, &journal).unwrap();

        recover_pending(&root).unwrap();

        assert_eq!(
            fs::read_to_string(release_path(&root, version).join("marker.txt")).unwrap(),
            "old"
        );
        assert_eq!(
            fs::read_to_string(staging_path(&root, version).join("marker.txt")).unwrap(),
            "candidate"
        );
        assert!(!root.join("activation-journal.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn prepared_journal_restores_old_release_after_quarantine_move() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-prepared-recovery-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let version = "0.2.1";
        write_release(&staging_path(&root, version), version, "old");
        commit_staged(
            &root,
            &UpdateTarget::Release {
                version: version.to_string(),
            },
        )
        .unwrap();
        write_release(&staging_path(&root, version), version, "candidate");
        let quarantine_name = format!("repair-{version}-prepared-test");
        let quarantine = root.join("releases").join(&quarantine_name);
        write_journal(
            &root,
            &ActivationJournal {
                schema_version: ACTIVATION_JOURNAL_SCHEMA,
                target_version: version.to_string(),
                old_version: Some(version.to_string()),
                quarantine: quarantine_name,
                phase: "prepared".to_string(),
            },
        )
        .unwrap();
        move_directory(&release_path(&root, version), &quarantine).unwrap();

        recover_pending(&root).unwrap();

        assert_eq!(
            fs::read_to_string(release_path(&root, version).join("marker.txt")).unwrap(),
            "old"
        );
        assert_eq!(
            fs::read_to_string(staging_path(&root, version).join("marker.txt")).unwrap(),
            "candidate"
        );
        assert!(!root.join("activation-journal.json").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn committed_journal_keeps_valid_candidate() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-committed-recovery-{}",
            uuid::Uuid::new_v4().simple()
        ));
        let version = "0.2.1";
        write_release(&staging_path(&root, version), version, "old");
        commit_staged(
            &root,
            &UpdateTarget::Release {
                version: version.to_string(),
            },
        )
        .unwrap();
        write_release(&staging_path(&root, version), version, "candidate");
        let quarantine_name = format!("repair-{version}-committed-test");
        let quarantine = root.join("releases").join(&quarantine_name);
        let journal = ActivationJournal {
            schema_version: ACTIVATION_JOURNAL_SCHEMA,
            target_version: version.to_string(),
            old_version: Some(version.to_string()),
            quarantine: quarantine_name,
            phase: "current-committed".to_string(),
        };
        write_journal(&root, &journal).unwrap();
        move_directory(&release_path(&root, version), &quarantine).unwrap();
        move_directory(&staging_path(&root, version), &release_path(&root, version)).unwrap();

        recover_pending(&root).unwrap();

        assert_eq!(
            fs::read_to_string(release_path(&root, version).join("marker.txt")).unwrap(),
            "candidate"
        );
        assert!(quarantine.is_dir());
        assert!(!root.join("activation-journal.json").exists());
        fs::remove_dir_all(root).unwrap();
    }
}

fn validate_installed(path: &Path, version: &str) -> Result<(), String> {
    if !path.join(".ready").is_file() {
        return Err(format!("release {version} 缺少 ready 标记"));
    }
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(path.join("release.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("release 元数据损坏：{error}"))?;
    super::contract::verify_envelope(&manifest)?;
    if manifest.payload.version != version {
        return Err(format!("release 版本不匹配：{version}"));
    }
    Ok(())
}

fn read_current(root: &Path) -> Result<Option<CurrentState>, String> {
    let path = root.join("releases/current.json");
    if !path.is_file() {
        return Ok(None);
    }
    let state: CurrentState =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("current 状态损坏：{error}"))?;
    if state.schema_version != RELEASE_STATE_SCHEMA {
        return Err("旧 current 状态不兼容，请重新运行新版 Installer".to_string());
    }
    super::contract::validate_version(&state.current)?;
    Ok(Some(state))
}

fn journal_path(root: &Path) -> PathBuf {
    root.join("activation-journal.json")
}

fn write_journal(root: &Path, journal: &ActivationJournal) -> Result<(), String> {
    let path = journal_path(root);
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let result = paths::atomic_replace(&temporary, &path).map_err(|error| error.to_string());
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn clear_journal(root: &Path) -> Result<(), String> {
    match fs::remove_file(journal_path(root)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn write_current(root: &Path, state: &CurrentState) -> Result<(), String> {
    let releases = root.join("releases");
    fs::create_dir_all(&releases).map_err(|error| error.to_string())?;
    let target = releases.join("current.json");
    let temporary = releases.join(format!(".current-{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(state).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let result = paths::atomic_replace(&temporary, &target).map_err(|error| error.to_string());
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn remove_current(root: &Path) -> Result<(), String> {
    match fs::remove_file(root.join("releases/current.json")) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn move_directory(source: &Path, target: &Path) -> io::Result<()> {
    let mut last = None;
    for attempt in 0..30 {
        match fs::rename(source, target) {
            Ok(()) => return Ok(()),
            Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) => {
                last = Some(error);
                if attempt < 29 {
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
            Err(error) => return Err(error),
        }
    }
    Err(last.expect("retry captures transient error"))
}
