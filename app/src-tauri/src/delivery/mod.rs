mod activation;
pub(crate) mod broker;
mod component;
mod contract;
mod health;
mod ipc;
mod takeover;
mod transaction;

pub use broker::{
    current_release_ready_for_launcher, run_launcher_action, start_launcher_setup, LauncherState,
};
pub use contract::{CheckTrigger, DeliverySnapshot, UpdateIntent, UpdateResult, UpdateState};
pub(crate) use ipc::InstanceGuard;

pub(crate) fn acquire_launcher_instance() -> Result<Option<InstanceGuard>, String> {
    ipc::acquire_instance()
}

pub fn manager_snapshot() -> Result<DeliverySnapshot, String> {
    match ipc::request(ipc::Request::GetSnapshot)? {
        ipc::Response::Snapshot(snapshot) => Ok(snapshot),
        ipc::Response::Error(error) => Err(error),
        other => Err(format!("Broker 返回意外响应：{other:?}")),
    }
}

pub fn manager_intent(intent: UpdateIntent) -> Result<DeliverySnapshot, String> {
    match ipc::request(ipc::Request::Intent(intent))? {
        ipc::Response::Snapshot(snapshot) => Ok(snapshot),
        ipc::Response::Error(error) => Err(error),
        other => Err(format!("Broker 返回意外响应：{other:?}")),
    }
}

pub fn validate_installer_bootstrap() -> Result<(), String> {
    broker::validate_installer_bootstrap()
}

pub fn verify_release_authenticode(path: &std::path::Path) -> Result<(), String> {
    health::verify_authenticode(path)
}

pub fn current_release_path(root: &std::path::Path) -> Result<Option<std::path::PathBuf>, String> {
    activation::current_release_path(root)
}

pub(crate) fn run_manager_broker(
    root: std::path::PathBuf,
    instance: InstanceGuard,
) -> Result<(), String> {
    broker::run_manager_broker(root, instance)
}
