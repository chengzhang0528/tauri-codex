use crate::delivery::DeliverySnapshot;
use serde::{Deserialize, Serialize};

pub const DEFAULT_MODEL_AUTO_COMPACT_TOKEN_LIMIT: u64 = 272_000;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexSettings {
    pub model: String,
    pub model_reasoning_effort: String,
    pub model_auto_compact_token_limit: u64,
    pub execution_mode: String,
    pub web_search: String,
    pub personality: String,
    pub config_error: Option<String>,
}

impl Default for CodexSettings {
    fn default() -> Self {
        Self {
            model: String::new(),
            model_reasoning_effort: String::new(),
            model_auto_compact_token_limit: DEFAULT_MODEL_AUTO_COMPACT_TOKEN_LIMIT,
            execution_mode: "default".to_string(),
            web_search: String::new(),
            personality: String::new(),
            config_error: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub sk: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServerSummary {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub has_sk: bool,
    pub is_default: bool,
}

impl From<&ServerProfile> for ServerSummary {
    fn from(value: &ServerProfile) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            base_url: value.base_url.clone(),
            has_sk: !value.sk.is_empty(),
            is_default: value.is_default,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TerminalInstance {
    pub id: String,
    pub window_label: String,
    pub workdir: String,
    pub server_id: Option<String>,
    pub resume: bool,
    pub codex_version: Option<String>,
    pub pid: Option<u32>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub app_version: String,
    pub codex_version: Option<String>,
    pub code_home: String,
    pub config_toml: String,
    pub codex_settings: CodexSettings,
    pub servers: Vec<ServerSummary>,
    pub terminals: Vec<TerminalInstance>,
    pub delivery: DeliverySnapshot,
}
