use super::activation;
use super::component;
use super::contract::{
    self, Bootstrap, CheckTrigger, DeliverySnapshot, Manifest, UpdateIntent, UpdateState,
    UpdateTarget, BOOTSTRAP_KEY, OSS_ROOT,
};
use super::health;
use super::ipc::{self, Request, Response};
use super::transaction::{self, TargetIdentity, Transaction};
use crate::{job, paths};
use reqwest::blocking::Client;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LauncherStatus {
    pub phase: String,
    pub component: String,
    pub downloaded: u64,
    pub total: u64,
    pub error: Option<String>,
    pub running: bool,
}

impl Default for LauncherStatus {
    fn default() -> Self {
        Self {
            phase: "正在读取 signed Bootstrap".to_string(),
            component: "初始化".to_string(),
            downloaded: 0,
            total: 0,
            error: None,
            running: false,
        }
    }
}

#[derive(Default)]
pub struct LauncherState {
    pub status: Mutex<LauncherStatus>,
    pub running: AtomicBool,
}

pub struct Broker {
    root: PathBuf,
    inner: Mutex<BrokerInner>,
    prepare_lock: Mutex<()>,
    cancel_requested: AtomicBool,
    cancel_allowed: AtomicBool,
}

struct BrokerInner {
    transaction: Transaction,
    active_sessions: usize,
    activation_requested: bool,
}

impl Broker {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&root).map_err(|error| error.to_string())?;
        activation::recover_pending(&root)?;
        let stored = transaction::load(&root)?;
        let interrupted_activation = matches!(
            stored.state,
            UpdateState::Activating | UpdateState::HealthCheck
        );
        let transaction = transaction::recover(
            &root,
            stored,
            &include_installer_version()?,
            !interrupted_activation || activation_health(&root).is_ok(),
        )?;
        Ok(Self {
            root: root.clone(),
            inner: Mutex::new(BrokerInner {
                transaction,
                active_sessions: 0,
                activation_requested: false,
            }),
            prepare_lock: Mutex::new(()),
            cancel_requested: AtomicBool::new(false),
            cancel_allowed: AtomicBool::new(true),
        })
    }

    pub fn snapshot(&self) -> Result<DeliverySnapshot, String> {
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        transaction::snapshot(&self.root, &inner.transaction, inner.active_sessions)
    }

    fn commit_transaction(&self, inner: &mut BrokerInner, next: Transaction) -> Result<(), String> {
        transaction::commit(&self.root, &mut inner.transaction, next)
    }

    fn commit_background_transaction(
        &self,
        inner: &mut BrokerInner,
        next: Transaction,
    ) -> Result<(), String> {
        transaction::commit_background(&self.root, &mut inner.transaction, next)
    }

    pub fn handle(self: &Arc<Self>, intent: UpdateIntent) -> Result<DeliverySnapshot, String> {
        match intent {
            UpdateIntent::Check { trigger } => self.check(trigger),
            UpdateIntent::Prepare => self.prepare(),
            UpdateIntent::Activate { active_sessions } => self.activate(active_sessions),
            UpdateIntent::Cancel => self.cancel(),
        }
    }

    fn check(&self, trigger: CheckTrigger) -> Result<DeliverySnapshot, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        if matches!(
            inner.transaction.state,
            UpdateState::Checking
                | UpdateState::Downloading
                | UpdateState::Verifying
                | UpdateState::Staged
                | UpdateState::WaitingForDrain
                | UpdateState::Activating
                | UpdateState::HealthCheck
                | UpdateState::RebootRequired
                | UpdateState::RepairRequired
        ) {
            return transaction::snapshot(&self.root, &inner.transaction, inner.active_sessions);
        }
        let next = transaction::begin(trigger, None, UpdateState::Checking, "checking");
        let operation_id = next.operation_id.clone();
        self.commit_transaction(&mut inner, next)?;
        drop(inner);
        let result = self.resolve_target();
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        if inner.transaction.operation_id != operation_id {
            return transaction::snapshot(&self.root, &inner.transaction, inner.active_sessions);
        }
        let mut next = inner.transaction.clone();
        match result {
            Ok((target, identity)) => {
                next.target = target.clone();
                next.target_identity = identity;
                (next.state, next.phase) = match target {
                    Some(UpdateTarget::Installer { .. }) => {
                        (UpdateState::SetupRequired, "setup-required".to_string())
                    }
                    Some(UpdateTarget::Release { .. }) => {
                        (UpdateState::Available, "available".to_string())
                    }
                    None => (UpdateState::UpToDate, "up-to-date".to_string()),
                };
                next.checked_at = Some(now());
                next.error = None;
            }
            Err(error) => {
                transaction::finish(&mut next, UpdateState::Failed, "check", Some(error));
            }
        }
        self.commit_background_transaction(&mut inner, next)?;
        transaction::snapshot(&self.root, &inner.transaction, inner.active_sessions)
    }

    fn prepare(self: &Arc<Self>) -> Result<DeliverySnapshot, String> {
        let (target, target_identity, operation_id, active_sessions) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| "Broker 状态锁已损坏".to_string())?;
            if !matches!(
                inner.transaction.state,
                UpdateState::Available
                    | UpdateState::SetupRequired
                    | UpdateState::Failed
                    | UpdateState::RebootRequired
                    | UpdateState::RepairRequired
            ) {
                return transaction::snapshot(
                    &self.root,
                    &inner.transaction,
                    inner.active_sessions,
                );
            }
            let target = inner
                .transaction
                .target
                .clone()
                .ok_or_else(|| "没有可准备的兼容更新".to_string())?;
            if let UpdateTarget::Installer { version } = &target {
                return Err(format!(
                    "Installer {version} 不能在应用内准备，请运行新版 Setup；若升级失败，请卸载后重装"
                ));
            }
            let target_identity = inner.transaction.target_identity.clone();
            let mut next = transaction::begin(
                inner.transaction.trigger.clone(),
                Some(target.clone()),
                UpdateState::Downloading,
                "downloading",
            );
            next.target_identity = target_identity.clone();
            let operation_id = next.operation_id.clone();
            self.commit_transaction(&mut inner, next)?;
            (target, target_identity, operation_id, inner.active_sessions)
        };
        self.cancel_requested.store(false, Ordering::SeqCst);
        self.cancel_allowed.store(true, Ordering::SeqCst);
        let worker = self.clone();
        thread::spawn(move || worker.run_prepare(operation_id, target, target_identity));
        let inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        transaction::snapshot(&self.root, &inner.transaction, active_sessions)
    }

    fn run_prepare(
        self: Arc<Self>,
        operation_id: String,
        target: UpdateTarget,
        target_identity: Option<TargetIdentity>,
    ) {
        let _prepare = self
            .prepare_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let progress_owner = self.clone();
        let progress_id = operation_id.clone();
        let progress_failure = Arc::new(Mutex::new(None::<String>));
        let progress_failure_writer = progress_failure.clone();
        let mut progress = move |phase: &str, component: &str, downloaded: u64, total: u64| {
            if let Err(error) =
                progress_owner.update_progress(&progress_id, phase, component, downloaded, total)
            {
                if let Ok(mut failure) = progress_failure_writer.lock() {
                    *failure = Some(format!("无法持久化更新进度：{error}"));
                }
            }
        };
        let progress_failure_reader = progress_failure.clone();
        let cancelled = || {
            self.cancel_requested.load(Ordering::SeqCst)
                || progress_failure_reader
                    .lock()
                    .map(|failure| failure.is_some())
                    .unwrap_or(true)
                || self
                    .inner
                    .lock()
                    .map(|inner| inner.transaction.operation_id != operation_id)
                    .unwrap_or(true)
        };
        let mut result =
            self.prepare_target(&target, target_identity.as_ref(), &mut progress, &cancelled);
        if let Some(error) = progress_failure
            .lock()
            .ok()
            .and_then(|failure| failure.clone())
        {
            result = Err(error);
        }
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.transaction.operation_id != operation_id {
            return;
        }
        let mut next = inner.transaction.clone();
        match result {
            Ok(component::StageReleaseOutcome::Staged) => {
                next.state = UpdateState::Staged;
                next.phase = "staged".to_string();
                next.error = None;
            }
            Ok(component::StageReleaseOutcome::RebootRequired) => {
                next.state = UpdateState::RebootRequired;
                next.phase = "reboot-required".to_string();
                next.error = Some("Node.js 安装完成，请重启 Windows 后继续".to_string());
            }
            Err(error) => {
                transaction::finish(&mut next, UpdateState::Failed, "prepare", Some(error));
            }
        }
        let _ = self.commit_background_transaction(&mut inner, next);
        self.cancel_allowed.store(true, Ordering::SeqCst);
    }

    fn update_progress(
        &self,
        operation_id: &str,
        phase: &str,
        component: &str,
        downloaded: u64,
        total: u64,
    ) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        if inner.transaction.operation_id != operation_id {
            return Ok(());
        }
        let mut next = inner.transaction.clone();
        next.state = if phase.contains("下载") {
            UpdateState::Downloading
        } else {
            UpdateState::Verifying
        };
        next.phase = phase.to_string();
        next.component = component.to_string();
        next.downloaded = downloaded;
        next.total = total;
        self.commit_transaction(&mut inner, next)?;
        self.cancel_allowed
            .store(phase != "正在安装系统组件", Ordering::SeqCst);
        Ok(())
    }

    fn activate(&self, active_sessions: usize) -> Result<DeliverySnapshot, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        if !matches!(
            inner.transaction.state,
            UpdateState::Staged | UpdateState::WaitingForDrain
        ) {
            return Err("更新尚未进入 staged".to_string());
        }
        if active_sessions != 0 {
            let mut next = inner.transaction.clone();
            next.state = UpdateState::WaitingForDrain;
            next.phase = "waiting-for-drain".to_string();
            next.error = Some(format!("仍有 {active_sessions} 个活动会话"));
            self.commit_transaction(&mut inner, next)?;
            inner.active_sessions = active_sessions;
            return transaction::snapshot(&self.root, &inner.transaction, active_sessions);
        }
        let operation_id = inner.transaction.operation_id.clone();
        let target = inner
            .transaction
            .target
            .clone()
            .ok_or_else(|| "更新缺少激活目标".to_string())?;
        let target_identity = inner.transaction.target_identity.clone();
        drop(inner);
        self.revalidate_target(&target, target_identity.as_ref())?;
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        if inner.transaction.operation_id != operation_id
            || !matches!(
                inner.transaction.state,
                UpdateState::Staged | UpdateState::WaitingForDrain
            )
        {
            return Err("更新事务在激活前已发生变化，请重新检查".to_string());
        }
        let mut next = inner.transaction.clone();
        next.state = UpdateState::Activating;
        next.phase = "activating".to_string();
        next.error = None;
        self.commit_transaction(&mut inner, next)?;
        inner.active_sessions = active_sessions;
        let snapshot = transaction::snapshot(&self.root, &inner.transaction, active_sessions)?;
        inner.activation_requested = true;
        Ok(snapshot)
    }

    fn cancel(&self) -> Result<DeliverySnapshot, String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        if !matches!(
            inner.transaction.state,
            UpdateState::Checking
                | UpdateState::Available
                | UpdateState::Downloading
                | UpdateState::Verifying
                | UpdateState::Staged
                | UpdateState::WaitingForDrain
                | UpdateState::RebootRequired
                | UpdateState::Failed
                | UpdateState::RepairRequired
        ) || !self.cancel_allowed.load(Ordering::SeqCst)
        {
            return Err("更新已跨过可取消边界".to_string());
        }
        let next = transaction::begin(CheckTrigger::Manual, None, UpdateState::Idle, "idle");
        self.commit_transaction(&mut inner, next)?;
        self.cancel_requested.store(true, Ordering::SeqCst);
        inner.active_sessions = 0;
        inner.activation_requested = false;
        transaction::snapshot(&self.root, &inner.transaction, inner.active_sessions)
    }

    fn resolve_target(&self) -> Result<(Option<UpdateTarget>, Option<TargetIdentity>), String> {
        let (bootstrap, bootstrap_sha256) = read_bootstrap_remote_with_digest()?;
        contract::validate_bootstrap(&bootstrap)?;
        let current = activation::current_release_version(&self.root)?;
        let target = select_target(
            &bootstrap,
            current.as_deref(),
            &include_installer_version()?,
        )?;
        let identity = target.as_ref().map(|target| TargetIdentity {
            bootstrap_sha256: bootstrap_sha256.clone(),
            artifact_sha256: match target {
                UpdateTarget::Installer { .. } => bootstrap
                    .payload
                    .installer
                    .as_ref()
                    .expect("select_target returned installer without Bootstrap installer")
                    .artifact
                    .sha256
                    .clone(),
                UpdateTarget::Release { .. } => bootstrap.payload.release.manifest.sha256.clone(),
            },
        });
        Ok((target, identity))
    }

    fn prepare_target(
        &self,
        target: &UpdateTarget,
        target_identity: Option<&TargetIdentity>,
        progress: &mut dyn FnMut(&str, &str, u64, u64),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<component::StageReleaseOutcome, String> {
        match target {
            UpdateTarget::Installer { version } => Err(format!(
                "Installer {version} 不能在应用内准备，请运行新版 Setup"
            )),
            UpdateTarget::Release { version } => {
                let (bootstrap, bootstrap_sha256) = read_bootstrap_remote_with_digest()?;
                verify_target_bootstrap(target_identity, &bootstrap_sha256)?;
                let manifest = read_manifest(&bootstrap)?;
                if &manifest.payload.version != version {
                    return Err("release 目标与 manifest 不一致".to_string());
                }
                verify_target_artifact(
                    target_identity,
                    &bootstrap.payload.release.manifest.sha256,
                )?;
                component::stage_release(&self.root, &manifest, progress, cancelled)
            }
        }
    }

    fn revalidate_target(
        &self,
        target: &UpdateTarget,
        target_identity: Option<&TargetIdentity>,
    ) -> Result<(), String> {
        let (bootstrap, bootstrap_sha256) = read_bootstrap_remote_with_digest()?;
        contract::validate_bootstrap(&bootstrap)?;
        verify_target_bootstrap(target_identity, &bootstrap_sha256)?;
        match target {
            UpdateTarget::Installer { version } => Err(format!(
                "Installer {version} 不能在应用内激活，请运行新版 Setup"
            )),
            UpdateTarget::Release { version } => {
                let manifest = read_manifest(&bootstrap)?;
                if &manifest.payload.version != version {
                    return Err("激活前 release 目标已变化".to_string());
                }
                verify_target_artifact(
                    target_identity,
                    &bootstrap.payload.release.manifest.sha256,
                )?;
                component::verify_staged_release(&self.root, &manifest)
            }
        }
    }

    fn take_activation(&self) -> Option<(UpdateTarget, Option<TargetIdentity>)> {
        self.inner.lock().ok().and_then(|mut inner| {
            if inner.activation_requested {
                inner.activation_requested = false;
                inner
                    .transaction
                    .target
                    .clone()
                    .map(|target| (target, inner.transaction.target_identity.clone()))
            } else {
                None
            }
        })
    }

    fn activation_failed(&self, error: String) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        let mut next = inner.transaction.clone();
        transaction::finish(
            &mut next,
            UpdateState::RepairRequired,
            "repair-required",
            Some(error),
        );
        self.commit_background_transaction(&mut inner, next)
    }

    fn mark_health_check(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        let mut next = inner.transaction.clone();
        next.state = UpdateState::HealthCheck;
        next.phase = "health-check".to_string();
        next.error = None;
        self.commit_background_transaction(&mut inner, next)
    }

    fn activation_succeeded(&self) -> Result<(), String> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "Broker 状态锁已损坏".to_string())?;
        let mut next = inner.transaction.clone();
        next.state = UpdateState::Ready;
        next.phase = "ready".to_string();
        next.error = None;
        self.commit_transaction(&mut inner, next)
    }
}

pub fn run_manager_broker(root: PathBuf, _instance: ipc::InstanceGuard) -> Result<(), String> {
    let broker = Arc::new(Broker::new(root.clone())?);
    let handler = {
        let broker = broker.clone();
        Arc::new(move |request: Request| -> Response {
            match request {
                Request::GetSnapshot => broker
                    .snapshot()
                    .map(Response::Snapshot)
                    .unwrap_or_else(Response::Error),
                Request::Intent(intent) => broker
                    .handle(intent)
                    .map(Response::Snapshot)
                    .unwrap_or_else(Response::Error),
                Request::Ping => Response::Pong,
            }
        })
    };
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let mut first = Some(ready_tx);
        loop {
            let result = match first.take() {
                Some(ready) => ipc::serve_with_ready(handler.clone(), ready),
                None => ipc::serve(handler.clone()),
            };
            eprintln!(
                "Launcher Broker Named Pipe 服务退出：{}；1 秒后重建",
                result.unwrap_err()
            );
            thread::sleep(Duration::from_secs(1));
        }
    });
    match ready_rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(error),
        Err(error) => return Err(format!("Launcher Broker 启动超时：{error}")),
    }
    let automatic = broker.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        automatic_cycle(&automatic);
        loop {
            thread::sleep(Duration::from_secs(6 * 60 * 60));
            automatic_cycle(&automatic);
        }
    });
    loop {
        let manager = launch_manager(&root)?;
        let status = wait_manager(manager)?;
        if let Some((target, target_identity)) = broker.take_activation() {
            broker.revalidate_target(&target, target_identity.as_ref())?;
            match target {
                UpdateTarget::Installer { version } => {
                    return Err(format!(
                        "Installer {version} 不能由运行中的 Manager 激活，请运行新版 Setup"
                    ));
                }
                UpdateTarget::Release { .. } => match activation::commit_staged(&root, &target)
                    .and_then(|_| {
                        broker.mark_health_check()?;
                        activation_health(&root)
                    }) {
                    Ok(()) => {
                        broker.activation_succeeded()?;
                        continue;
                    }
                    Err(error) => {
                        broker.activation_failed(error)?;
                        return Err("激活后健康检查失败，已进入 forward-repair".to_string());
                    }
                },
            }
        }
        if !status.success() {
            return Err(format!("Manager 退出码 {}", status.code().unwrap_or(-1)));
        }
        break;
    }
    Ok(())
}

fn automatic_cycle(broker: &Arc<Broker>) {
    let before = broker.snapshot().ok();
    let retry_existing = before.as_ref().is_some_and(|snapshot| {
        snapshot.target.is_some()
            && matches!(
                snapshot.state,
                UpdateState::Failed | UpdateState::RepairRequired
            )
    });
    let snapshot = if retry_existing {
        before
    } else {
        broker
            .handle(UpdateIntent::Check {
                trigger: CheckTrigger::Automatic,
            })
            .ok()
    };
    if snapshot.as_ref().is_some_and(|snapshot| {
        matches!(
            snapshot.state,
            UpdateState::Available | UpdateState::Failed | UpdateState::RepairRequired
        ) && matches!(snapshot.target, Some(UpdateTarget::Release { .. }))
    }) {
        let _ = broker.handle(UpdateIntent::Prepare);
    }
}

fn launch_manager(root: &Path) -> Result<Child, String> {
    let release = activation::current_release_path(root)?
        .ok_or_else(|| "尚无可运行的 current release".to_string())?;
    let manager = release.join("manager/tauri-codex-manager.exe");
    let (_, node_version) = activation::current_component_versions(root)?;
    let node_version = node_version.ok_or_else(|| "current manifest 缺少 Node 组件".to_string())?;
    let system_node = health::doctor_system_node(&node_version)?;
    health::doctor_manager(
        manager.parent().unwrap_or_else(|| Path::new(".")),
        &system_node,
    )?;
    let launcher = std::env::current_exe().map_err(|error| error.to_string())?;
    job::background_command(&manager)
        .env("TAURI_CODEX_LAUNCHER", launcher)
        .env("TAURI_CODEX_SYSTEM_NODE", system_node)
        .current_dir(manager.parent().unwrap_or_else(|| Path::new(".")))
        .spawn()
        .map_err(|error| format!("无法启动 Manager：{error}"))
}
fn wait_manager(mut child: Child) -> Result<std::process::ExitStatus, String> {
    child.wait().map_err(|error| error.to_string())
}
fn activation_health(root: &Path) -> Result<(), String> {
    let release =
        activation::current_release_path(root)?.ok_or_else(|| "激活后缺少 current".to_string())?;
    let manifest: Manifest = serde_json::from_slice(
        &fs::read(release.join("release.json")).map_err(|error| error.to_string())?,
    )
    .map_err(|error| format!("current manifest 损坏：{error}"))?;
    contract::verify_envelope(&manifest)?;
    component::verify_installed_release(root, &manifest)?;
    let node = manifest
        .payload
        .components
        .iter()
        .find(|component| component.id == contract::ComponentId::Node && component.required)
        .ok_or_else(|| "current manifest 缺少 Node 组件".to_string())?;
    let system_node = health::doctor_system_node(&node.version)?;
    health::doctor_manager(&release.join("manager"), &system_node)?;
    health::doctor_codex(&release.join("codex"), &system_node)
}

pub fn current_release_ready_for_launcher(root: &Path) -> bool {
    activation::recover_pending(root)
        .and_then(|_| {
            ensure_manager_version_supported(activation::current_release_version(root)?.as_deref())
        })
        .and_then(|_| activation_health(root))
        .is_ok()
}

pub fn run_launcher_action() -> Result<bool, String> {
    let mut arguments = std::env::args().skip(1);
    match arguments.next().as_deref() {
        Some("--thin-setup") => {
            validate_installer_bootstrap()?;
            Ok(true)
        }
        Some("--installer-takeover") => {
            let install_root = arguments
                .next()
                .ok_or_else(|| "Installer takeover 缺少安装目录".to_string())?;
            let silent = arguments.any(|argument| argument == "--silent");
            super::takeover::run(Path::new(&install_root), silent)?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

pub fn validate_installer_bootstrap() -> Result<(), String> {
    let bootstrap = read_bootstrap_installed()?;
    contract::validate_bootstrap(&bootstrap)?;
    let current =
        semver::Version::parse(&include_installer_version()?).map_err(|error| error.to_string())?;
    let required = semver::Version::parse(&bootstrap.payload.minimum_launcher_version)
        .map_err(|error| error.to_string())?;
    if required > current {
        return Err(format!(
            "Installer Bootstrap 需要 Launcher {required}，当前为 {current}"
        ));
    }
    Ok(())
}

#[tauri::command]
pub fn get_launcher_status(state: State<'_, LauncherState>) -> Result<LauncherStatus, String> {
    state
        .status
        .lock()
        .map(|status| status.clone())
        .map_err(|_| "Launcher 状态锁已损坏".to_string())
}
#[tauri::command]
pub fn retry_launcher_setup(app: AppHandle, state: State<'_, LauncherState>) -> Result<(), String> {
    start_launcher_setup(app, state.inner())
}

pub fn start_launcher_setup(app: AppHandle, state: &LauncherState) -> Result<(), String> {
    if state
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Ok(());
    }
    set_status(
        &app,
        state,
        LauncherStatus {
            running: true,
            ..Default::default()
        },
    );
    let handle = app.clone();
    thread::spawn(move || {
        let result = initial_setup(&handle);
        let state = handle.state::<LauncherState>();
        state.running.store(false, Ordering::SeqCst);
        match result {
            Ok(()) => {
                set_status(
                    &handle,
                    state.inner(),
                    LauncherStatus {
                        phase: "准备完成，Launcher Broker 将启动 Manager".to_string(),
                        component: "release".to_string(),
                        running: false,
                        ..Default::default()
                    },
                );
                handle.exit(0);
            }
            Err(error) => set_error(&handle, state.inner(), error),
        }
    });
    Ok(())
}

fn initial_setup(app: &AppHandle) -> Result<(), String> {
    let root = paths::delivery_root()?;
    activation::recover_pending(&root)?;
    let bootstrap = read_bootstrap_remote()?;
    contract::validate_bootstrap(&bootstrap)?;
    let current = activation::current_release_version(&root)?;
    let selected = select_target(
        &bootstrap,
        current.as_deref(),
        &include_installer_version()?,
    )?;
    let version = match selected {
        Some(UpdateTarget::Installer { version }) => {
            return Err(format!(
                "当前 Launcher 需要先运行 Installer {version}，请重新运行新版 Setup"
            ))
        }
        Some(UpdateTarget::Release { version }) => version,
        None => bootstrap.payload.release.version.clone(),
    };
    ensure_manager_version_supported(Some(&version))?;
    if current.as_deref() == Some(version.as_str()) && current_release_ready_for_launcher(&root) {
        transaction::record_ready(
            &root,
            UpdateTarget::Release {
                version: version.clone(),
            },
        )?;
        return Ok(());
    }
    let manifest = read_manifest(&bootstrap)?;
    if manifest.payload.version != version {
        return Err("Bootstrap release 与安装目标不一致".to_string());
    }
    let cancelled = || false;
    let outcome = component::stage_release(
        &root,
        &manifest,
        &mut |phase, component, downloaded, total| {
            set_status_from_app(app, phase, component, downloaded, total)
        },
        &cancelled,
    )?;
    if outcome == component::StageReleaseOutcome::RebootRequired {
        return Err("Node.js 安装完成，请重启 Windows 后继续".to_string());
    }
    let target = UpdateTarget::Release {
        version: manifest.payload.version.clone(),
    };
    activation::commit_staged(&root, &target)?;
    activation_health(&root)?;
    transaction::record_ready(&root, target)
}

fn set_status_from_app(app: &AppHandle, phase: &str, component: &str, downloaded: u64, total: u64) {
    let state = app.state::<LauncherState>();
    set_status(
        app,
        state.inner(),
        LauncherStatus {
            phase: phase.to_string(),
            component: component.to_string(),
            downloaded,
            total,
            running: true,
            ..Default::default()
        },
    );
}
fn set_status(app: &AppHandle, state: &LauncherState, status: LauncherStatus) {
    if let Ok(mut current) = state.status.lock() {
        *current = status.clone();
    }
    let _ = app.emit("launcher-status", status);
}
fn set_error(app: &AppHandle, state: &LauncherState, error: String) {
    let current = state
        .status
        .lock()
        .map(|status| status.clone())
        .unwrap_or_default();
    set_status(
        app,
        state,
        LauncherStatus {
            error: Some(error),
            running: false,
            ..current
        },
    );
}

fn read_bootstrap_remote() -> Result<Bootstrap, String> {
    read_bootstrap_remote_with_digest().map(|(bootstrap, _)| bootstrap)
}

fn read_bootstrap_remote_with_digest() -> Result<(Bootstrap, String), String> {
    let client = Client::builder()
        .user_agent("tauri-codex-launcher")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let response = client
        .get(format!("{OSS_ROOT}/{BOOTSTRAP_KEY}"))
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    if response
        .content_length()
        .is_some_and(|length| length > contract::MAX_MANIFEST_BYTES)
    {
        return Err("Bootstrap 超过大小上限".to_string());
    }
    let bytes = response.bytes().map_err(|error| error.to_string())?;
    if bytes.len() as u64 > contract::MAX_MANIFEST_BYTES {
        return Err("Bootstrap 超过大小上限".to_string());
    }
    let digest = contract::digest(&bytes);
    Ok((contract::parse_signed(&bytes, "Bootstrap")?, digest))
}

fn verify_target_bootstrap(identity: Option<&TargetIdentity>, actual: &str) -> Result<(), String> {
    let identity = identity.ok_or_else(|| "更新事务缺少冻结的目标 identity".to_string())?;
    if identity.bootstrap_sha256 != actual {
        return Err("Bootstrap 在更新操作期间发生变化，请重新检查".to_string());
    }
    Ok(())
}

fn verify_target_artifact(identity: Option<&TargetIdentity>, actual: &str) -> Result<(), String> {
    let identity = identity.ok_or_else(|| "更新事务缺少冻结的目标 identity".to_string())?;
    if identity.artifact_sha256 != actual {
        return Err("更新目标 artifact 在操作期间发生变化，请重新检查".to_string());
    }
    Ok(())
}
fn read_bootstrap_installed() -> Result<Bootstrap, String> {
    let executable = std::env::current_exe().map_err(|error| error.to_string())?;
    let dir = executable
        .parent()
        .ok_or_else(|| "无法确定安装目录".to_string())?;
    let path = [
        dir.join("bootstrap.json"),
        dir.join("resources/bootstrap.json"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| "Installer 缺少 signed Bootstrap seed".to_string())?;
    contract::parse_signed(
        &fs::read(path).map_err(|error| error.to_string())?,
        "installed Bootstrap",
    )
}
fn read_manifest(bootstrap: &Bootstrap) -> Result<Manifest, String> {
    contract::validate_bootstrap(bootstrap)?;
    let client = Client::builder()
        .user_agent("tauri-codex-launcher")
        .timeout(Duration::from_secs(120))
        .build()
        .map_err(|error| error.to_string())?;
    let artifact = &bootstrap.payload.release.manifest;
    let response = client
        .get(contract::artifact_url(&artifact.object_key)?)
        .send()
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    if response
        .content_length()
        .is_some_and(|length| length > artifact.size || length > contract::MAX_MANIFEST_BYTES)
    {
        return Err("manifest 超过清单大小".to_string());
    }
    let bytes = response.bytes().map_err(|error| error.to_string())?;
    if bytes.len() as u64 != artifact.size || contract::digest(&bytes) != artifact.sha256 {
        return Err("manifest size/SHA-256 不匹配".to_string());
    }
    let manifest = contract::parse_signed(&bytes, "release manifest")?;
    contract::validate_manifest(&manifest, &bootstrap.payload)?;
    let launcher =
        semver::Version::parse(&include_installer_version()?).map_err(|error| error.to_string())?;
    let required = semver::Version::parse(&manifest.payload.minimum_launcher_version)
        .map_err(|error| error.to_string())?;
    if required > launcher {
        return Err(format!(
            "release 需要 Launcher {}，当前为 {}",
            required, launcher
        ));
    }
    Ok(manifest)
}

fn include_installer_version() -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../../installer-versions.json"))
            .map_err(|error| format!("installer-versions.json 无效：{error}"))?;
    if value
        .get("schemaVersion")
        .and_then(|schema| schema.as_u64())
        != Some(2)
    {
        return Err("installer-versions.json 必须使用 schema v2".to_string());
    }
    let version = value
        .get("installerVersion")
        .and_then(|version| version.as_str())
        .ok_or_else(|| "installer-versions.json 缺少 installerVersion".to_string())?;
    contract::validate_version(version)?;
    Ok(version.to_string())
}

fn include_minimum_manager_version() -> Result<String, String> {
    let value: serde_json::Value =
        serde_json::from_str(include_str!("../../../installer-versions.json"))
            .map_err(|error| format!("installer-versions.json 无效：{error}"))?;
    let version = value
        .get("minimumManagerVersion")
        .and_then(|version| version.as_str())
        .ok_or_else(|| "installer-versions.json 缺少 minimumManagerVersion".to_string())?;
    contract::validate_version(version)?;
    Ok(version.to_string())
}

fn ensure_manager_version_supported(current: Option<&str>) -> Result<(), String> {
    let current = current.ok_or_else(|| "尚无可运行的 current release".to_string())?;
    let minimum = include_minimum_manager_version()?;
    if contract::newer(&minimum, current)? {
        return Err(format!(
            "当前 Manager {current} 低于 Launcher 要求 {minimum}，必须先完成 Launcher setup"
        ));
    }
    Ok(())
}

fn select_target(
    bootstrap: &Bootstrap,
    current_release: Option<&str>,
    current_installer: &str,
) -> Result<Option<UpdateTarget>, String> {
    contract::validate_version(current_installer)?;
    let installed_launcher =
        semver::Version::parse(current_installer).map_err(|error| error.to_string())?;
    let required_launcher = semver::Version::parse(&bootstrap.payload.minimum_launcher_version)
        .map_err(|error| error.to_string())?;
    if required_launcher > installed_launcher {
        let installer = bootstrap
            .payload
            .installer
            .as_ref()
            .ok_or_else(|| "Bootstrap 要求新版 Launcher，但未提供 Installer".to_string())?;
        let candidate =
            semver::Version::parse(&installer.version).map_err(|error| error.to_string())?;
        if candidate < required_launcher || candidate <= installed_launcher {
            return Err("Bootstrap Installer 无法满足 Launcher compatibility".to_string());
        }
        return Ok(Some(UpdateTarget::Installer {
            version: installer.version.clone(),
        }));
    }
    if let Some(installer) = &bootstrap.payload.installer {
        if semver::Version::parse(&installer.version).map_err(|error| error.to_string())?
            > installed_launcher
        {
            return Ok(Some(UpdateTarget::Installer {
                version: installer.version.clone(),
            }));
        }
    }
    let candidate = &bootstrap.payload.release.version;
    let Some(current) = current_release else {
        return Ok(Some(UpdateTarget::Release {
            version: candidate.clone(),
        }));
    };
    contract::validate_version(current)?;
    match semver::Version::parse(candidate)
        .map_err(|error| error.to_string())?
        .cmp(&semver::Version::parse(current).map_err(|error| error.to_string())?)
    {
        std::cmp::Ordering::Greater => Ok(Some(UpdateTarget::Release {
            version: candidate.clone(),
        })),
        std::cmp::Ordering::Equal => Ok(None),
        std::cmp::Ordering::Less => Err(format!(
            "拒绝 release downgrade：current {current}，candidate {candidate}"
        )),
    }
}
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{ensure_manager_version_supported, select_target};
    use crate::delivery::contract::{
        Artifact, Bootstrap, BootstrapPayload, InstallerRef, ReleaseRef, UpdateTarget,
    };

    fn bootstrap(installer: &str, release: &str, minimum: &str) -> Bootstrap {
        Bootstrap {
            schema_version: 2,
            key_id: "test".to_string(),
            payload: BootstrapPayload {
                product: "tauri-codex".to_string(),
                platform: "windows".to_string(),
                architecture: "x86_64".to_string(),
                minimum_launcher_version: minimum.to_string(),
                installer: Some(InstallerRef {
                    version: installer.to_string(),
                    artifact: Artifact {
                        object_key: format!(
                            "installers/{installer}/windows-x64/tauri-codex_{installer}_x64-setup.exe"
                        ),
                        size: 1,
                        sha256: "a".repeat(64),
                        provenance: "authenticode+ed25519".to_string(),
                    },
                }),
                release: ReleaseRef {
                    version: release.to_string(),
                    manifest: Artifact {
                        object_key: format!("releases/{release}/windows-x64/manifest.json"),
                        size: 1,
                        sha256: "b".repeat(64),
                        provenance: "ed25519".to_string(),
                    },
                },
            },
            signature: "test".to_string(),
        }
    }

    #[test]
    fn stable_launcher_compares_release_against_current_pointer() {
        let value = bootstrap("1.1.0", "0.3.0", "1.1.0");
        assert_eq!(select_target(&value, Some("0.3.0"), "1.1.0").unwrap(), None);
        assert_eq!(
            select_target(&value, Some("0.2.0"), "1.1.0").unwrap(),
            Some(UpdateTarget::Release {
                version: "0.3.0".to_string()
            })
        );
    }

    #[test]
    fn launcher_upgrade_precedes_release_and_downgrade_is_rejected() {
        let value = bootstrap("1.2.0", "0.3.0", "1.2.0");
        assert_eq!(
            select_target(&value, Some("0.2.0"), "1.1.0").unwrap(),
            Some(UpdateTarget::Installer {
                version: "1.2.0".to_string()
            })
        );
        let older = bootstrap("1.1.0", "0.1.9", "1.1.0");
        assert!(select_target(&older, Some("0.2.0"), "1.1.0").is_err());
    }

    #[test]
    fn compiled_launcher_minimum_rejects_an_older_manager() {
        assert!(ensure_manager_version_supported(Some("0.1.9")).is_err());
        assert!(ensure_manager_version_supported(Some("0.2.0")).is_ok());
    }
}
