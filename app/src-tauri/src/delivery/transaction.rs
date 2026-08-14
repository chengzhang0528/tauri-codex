use super::activation;
use super::contract::{CheckTrigger, DeliverySnapshot, UpdateState, UpdateTarget};
use crate::paths;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

const TRANSACTION_SCHEMA: u32 = 2;
static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Transaction {
    pub schema_version: u32,
    pub operation_id: String,
    pub state: UpdateState,
    pub trigger: CheckTrigger,
    pub target: Option<UpdateTarget>,
    pub phase: String,
    pub component: String,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
    pub checked_at: Option<u64>,
}

impl Default for Transaction {
    fn default() -> Self {
        Self {
            schema_version: TRANSACTION_SCHEMA,
            operation_id: String::new(),
            state: UpdateState::Idle,
            trigger: CheckTrigger::Manual,
            target: None,
            phase: "idle".to_string(),
            component: String::new(),
            downloaded: 0,
            total: 0,
            error: None,
            checked_at: None,
        }
    }
}

pub fn transaction_path(root: &Path) -> std::path::PathBuf {
    root.join("delivery-transaction.json")
}

pub fn load(root: &Path) -> Result<Transaction, String> {
    let path = transaction_path(root);
    if !path.is_file() {
        return Ok(Transaction::default());
    }
    let transaction: Transaction =
        serde_json::from_slice(&fs::read(&path).map_err(|error| error.to_string())?)
            .map_err(|error| format!("交付事务损坏：{error}"))?;
    if transaction.schema_version != TRANSACTION_SCHEMA {
        return Err(format!(
            "交付事务 schema {} 不受支持",
            transaction.schema_version
        ));
    }
    Ok(transaction)
}

pub fn save(root: &Path, transaction: &Transaction) -> Result<(), String> {
    let path = transaction_path(root);
    let temporary = path.with_extension(format!("json.{}.tmp", std::process::id()));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(transaction).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    let result = paths::atomic_replace(&temporary, &path).map_err(|error| error.to_string());
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub fn recover(
    root: &Path,
    mut transaction: Transaction,
    installer_version: &str,
) -> Result<Transaction, String> {
    let original = transaction.clone();
    match transaction.state {
        UpdateState::Checking => {
            transaction = Transaction::default();
        }
        UpdateState::Downloading | UpdateState::Verifying => {
            finish(
                &mut transaction,
                UpdateState::Failed,
                "interrupted-prepare",
                Some("上次更新准备被中断，可以重试".to_string()),
            );
        }
        UpdateState::WaitingForDrain => {
            transaction.state = UpdateState::Staged;
            transaction.phase = "staged".to_string();
            transaction.error = None;
        }
        UpdateState::Activating | UpdateState::HealthCheck => {
            let activated = match transaction.target.as_ref() {
                Some(UpdateTarget::Release { version }) => {
                    activation::current_release_version(root)?.as_deref() == Some(version)
                }
                Some(UpdateTarget::Installer { version }) => {
                    !super::contract::newer(version, installer_version)?
                }
                None => false,
            };
            if activated {
                transaction.state = UpdateState::Ready;
                transaction.phase = "ready".to_string();
                transaction.error = None;
            } else {
                finish(
                    &mut transaction,
                    UpdateState::Failed,
                    "interrupted-activation",
                    Some("上次激活未完成，可以重新准备目标".to_string()),
                );
            }
        }
        _ => {}
    }
    if transaction != original {
        save(root, &transaction)?;
    }
    Ok(transaction)
}

pub fn record_ready(root: &Path, target: UpdateTarget) -> Result<(), String> {
    let mut transaction = begin(
        CheckTrigger::Automatic,
        Some(target),
        UpdateState::Ready,
        "ready",
    );
    transaction.checked_at = Some(now_millis());
    save(root, &transaction)
}

pub fn snapshot(root: &Path, active_sessions: usize) -> Result<DeliverySnapshot, String> {
    let transaction = load(root)?;
    let current_version = activation::current_release_version(root)?;
    let (codex, node) = activation::current_component_versions(root)?;
    Ok(DeliverySnapshot {
        state: transaction.state,
        target: transaction.target,
        current_version,
        current_codex_version: codex,
        current_node_version: node,
        active_sessions,
        phase: transaction.phase,
        component: transaction.component,
        downloaded: transaction.downloaded,
        total: transaction.total,
        error: transaction.error,
        checked_at: transaction.checked_at,
        operation_id: (!transaction.operation_id.is_empty()).then_some(transaction.operation_id),
    })
}

pub fn begin(
    trigger: CheckTrigger,
    target: Option<UpdateTarget>,
    state: UpdateState,
    phase: &str,
) -> Transaction {
    let id = format!(
        "delivery-{}-{}",
        now_millis(),
        NEXT_OPERATION.fetch_add(1, Ordering::Relaxed)
    );
    Transaction {
        schema_version: TRANSACTION_SCHEMA,
        operation_id: id,
        state,
        trigger,
        target,
        phase: phase.to_string(),
        component: String::new(),
        downloaded: 0,
        total: 0,
        error: None,
        checked_at: None,
    }
}

pub fn finish(
    transaction: &mut Transaction,
    state: UpdateState,
    phase: &str,
    error: Option<String>,
) {
    transaction.state = state;
    transaction.phase = phase.to_string();
    transaction.error = error;
    transaction.checked_at = Some(now_millis());
}

#[cfg(test)]
mod tests {
    use super::{begin, load, recover, save, CheckTrigger, UpdateState, UpdateTarget};
    use std::fs;

    fn fixture() -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-transaction-{}",
            uuid::Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn persists_and_recovers_an_interrupted_prepare() {
        let root = fixture();
        let transaction = begin(
            CheckTrigger::Automatic,
            Some(UpdateTarget::Release {
                version: "0.2.1".to_string(),
            }),
            UpdateState::Downloading,
            "downloading",
        );
        save(&root, &transaction).unwrap();
        let recovered = recover(&root, load(&root).unwrap(), "1.1.0").unwrap();
        assert_eq!(recovered.state, UpdateState::Failed);
        assert_eq!(recovered.target, transaction.target);
        assert_eq!(load(&root).unwrap(), recovered);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn drain_wait_requires_a_fresh_activation_confirmation() {
        let root = fixture();
        let transaction = begin(
            CheckTrigger::Manual,
            Some(UpdateTarget::Release {
                version: "0.2.1".to_string(),
            }),
            UpdateState::WaitingForDrain,
            "waiting-for-drain",
        );
        let recovered = recover(&root, transaction, "1.1.0").unwrap();
        assert_eq!(recovered.state, UpdateState::Staged);
        fs::remove_dir_all(root).unwrap();
    }
}

fn now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}
