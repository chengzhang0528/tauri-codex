use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CodexSettings {
    pub model: String,
    pub model_reasoning_effort: String,
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
    pub pending_codex_versions: Vec<String>,
    pub staged_app_updates: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub download_url: String,
    pub size: u64,
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReleaseInfo {
    pub tag_name: String,
    pub name: String,
    pub html_url: String,
    pub published_at: Option<String>,
    pub update_available: bool,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateResult {
    pub version: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Deserialize)]
pub struct GithubReleaseResponse {
    pub tag_name: String,
    pub name: Option<String>,
    pub html_url: String,
    pub published_at: Option<String>,
    #[serde(default)]
    pub assets: Vec<GithubAssetResponse>,
}

#[derive(Debug, Deserialize)]
pub struct GithubAssetResponse {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
    #[serde(default)]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CodexUpdateInfo {
    pub current_version: Option<String>,
    pub latest_version: String,
    pub update_available: bool,
}
