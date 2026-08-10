use crate::model::{
    AppSnapshot, CodexSettings, CodexUpdateInfo, ServerProfile, ServerSummary, TerminalInstance,
    UpdateResult,
};
use crate::paths;
use crate::sessions::SessionManager;
use crate::updates;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tauri::{AppHandle, State};

#[derive(Clone, Default)]
pub struct AppState {
    pub sessions: SessionManager,
    #[cfg(debug_assertions)]
    pub dev_events: crate::dev_bridge::DevEventHub,
}

#[derive(Debug, Deserialize)]
pub struct StartTerminalRequest {
    pub workdir: String,
    pub server_id: Option<String>,
    #[serde(default)]
    pub resume: bool,
}

#[derive(Debug, Deserialize)]
pub struct ResizeRequest {
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

#[derive(Debug, Deserialize)]
pub struct RenderedRequest {
    pub sequence: u64,
}

#[derive(Debug, Deserialize)]
pub struct SaveCodexSettingsRequest {
    pub model: String,
    pub model_reasoning_effort: String,
    pub execution_mode: String,
    pub web_search: String,
    pub personality: String,
}

pub(crate) fn load_servers(app: &AppHandle) -> Result<Vec<ServerProfile>, String> {
    let file = paths::servers_file(app)?;
    if !file.is_file() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(file).map_err(|error| error.to_string())?;
    parse_servers(&text)
}

fn parse_servers(text: &str) -> Result<Vec<ServerProfile>, String> {
    let mut servers: Vec<ServerProfile> =
        serde_json::from_str(text).map_err(|error| format!("Server 配置损坏：{error}"))?;
    normalize_server_defaults(&mut servers);
    Ok(servers)
}

fn normalize_server_defaults(servers: &mut [ServerProfile]) {
    let default_index = servers
        .iter()
        .position(|server| server.is_default)
        .or_else(|| (!servers.is_empty()).then_some(0));
    for (index, server) in servers.iter_mut().enumerate() {
        server.is_default = Some(index) == default_index;
    }
}

fn set_server_default(servers: &mut [ServerProfile], id: &str) {
    for server in servers {
        server.is_default = server.id == id;
    }
}

fn save_servers(app: &AppHandle, servers: &[ServerProfile]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(servers).map_err(|error| error.to_string())?;
    fs::write(paths::servers_file(app)?, text).map_err(|error| error.to_string())
}

pub fn snapshot(app: &AppHandle, state: &AppState) -> Result<AppSnapshot, String> {
    let config_toml = read_config_toml(app)?;
    let codex_settings = parse_codex_settings(&config_toml);
    let servers = load_servers(app)?;
    Ok(AppSnapshot {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        codex_version: paths::codex_version(app)?,
        code_home: paths::codex_home(app)?.to_string_lossy().to_string(),
        config_toml,
        codex_settings,
        servers: servers.iter().map(ServerSummary::from).collect(),
        terminals: state.sessions.list()?,
        pending_codex_versions: paths::pending_codex_versions(app)?,
        staged_app_updates: updates::staged_app_updates(app)?,
    })
}

fn read_config_toml(app: &AppHandle) -> Result<String, String> {
    let config_file = paths::config_file(app)?;
    if config_file.is_file() {
        fs::read_to_string(config_file).map_err(|error| error.to_string())
    } else {
        Ok("# Codex configuration\n".to_string())
    }
}

#[tauri::command]
pub fn get_snapshot(app: AppHandle, state: State<'_, AppState>) -> Result<AppSnapshot, String> {
    snapshot(&app, &state)
}

#[tauri::command]
pub fn get_server(app: AppHandle, id: String) -> Result<ServerProfile, String> {
    load_servers(&app)?
        .into_iter()
        .find(|server| server.id == id)
        .ok_or_else(|| "Server 不存在".to_string())
}

#[tauri::command]
pub fn save_server(
    app: AppHandle,
    mut profile: ServerProfile,
) -> Result<Vec<ServerSummary>, String> {
    if profile.name.trim().is_empty()
        || profile.base_url.trim().is_empty()
        || profile.sk.trim().is_empty()
    {
        return Err("API 名称、URL 和 API Key 不能为空".to_string());
    }
    let url =
        reqwest::Url::parse(&profile.base_url).map_err(|_| "Base URL 不是合法 URL".to_string())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("Base URL 只支持 HTTP 或 HTTPS".to_string());
    }
    if profile.id.trim().is_empty() {
        profile.id = uuid::Uuid::new_v4().simple().to_string();
    }
    profile.name = profile.name.trim().to_string();
    profile.base_url = profile.base_url.trim_end_matches('/').to_string();
    profile.sk = profile.sk.trim().to_string();
    let make_default = profile.is_default;
    let profile_id = profile.id.clone();
    let mut servers = load_servers(&app)?;
    if let Some(existing) = servers.iter_mut().find(|server| server.id == profile.id) {
        *existing = profile;
    } else {
        servers.push(profile);
    }
    if make_default {
        set_server_default(&mut servers, &profile_id);
    }
    normalize_server_defaults(&mut servers);
    save_servers(&app, &servers)?;
    sync_server_profiles(&app)?;
    Ok(servers.iter().map(ServerSummary::from).collect())
}

#[tauri::command]
pub fn delete_server(app: AppHandle, id: String) -> Result<Vec<ServerSummary>, String> {
    let mut servers = load_servers(&app)?;
    servers.retain(|server| server.id != id);
    normalize_server_defaults(&mut servers);
    save_servers(&app, &servers)?;
    sync_server_profiles(&app)?;
    Ok(servers.iter().map(ServerSummary::from).collect())
}

pub(crate) fn sync_server_profiles(app: &AppHandle) -> Result<(), String> {
    let servers = load_servers(app)?;
    save_servers(app, &servers)?;
    let current = read_config_toml(app)?;
    let updated = remove_legacy_server_profiles(&current)?;
    write_config_toml(app, &updated)?;
    sync_server_profile_files(app, &servers)
}

fn sync_server_profile_files(app: &AppHandle, servers: &[ServerProfile]) -> Result<(), String> {
    let home = paths::codex_home(app)?;
    let desired = servers
        .iter()
        .map(|server| format!("{}.config.toml", paths::server_profile_name(&server.id)))
        .collect::<HashSet<_>>();

    for server in servers {
        let filename = format!("{}.config.toml", paths::server_profile_name(&server.id));
        write_toml_file(&home.join(filename), &build_server_profile(server)?)?;
    }

    for entry in fs::read_dir(&home).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let filename = entry.file_name().to_string_lossy().to_string();
        if filename.starts_with("server-")
            && filename.ends_with(".config.toml")
            && !desired.contains(&filename)
            && entry.path().is_file()
        {
            fs::remove_file(entry.path()).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[tauri::command]
pub fn save_config(app: AppHandle, config_toml: String) -> Result<CodexSettings, String> {
    config_toml
        .parse::<toml::Value>()
        .map_err(|error| format!("TOML 校验失败：{error}"))?;
    let updated = remove_legacy_server_profiles(&config_toml)?;
    write_config_toml(&app, &updated)?;
    sync_server_profile_files(&app, &load_servers(&app)?)?;
    Ok(parse_codex_settings(&updated))
}

#[tauri::command]
pub fn save_codex_settings(
    app: AppHandle,
    settings: SaveCodexSettingsRequest,
) -> Result<CodexSettings, String> {
    let current = read_config_toml(&app)?;
    let updated = merge_codex_settings(&current, &settings)?;
    let updated = remove_legacy_server_profiles(&updated)?;
    write_config_toml(&app, &updated)?;
    sync_server_profile_files(&app, &load_servers(&app)?)?;
    Ok(parse_codex_settings(&updated))
}

fn write_config_toml(app: &AppHandle, config_toml: &str) -> Result<(), String> {
    write_toml_file(&paths::config_file(app)?, config_toml)
}

fn write_toml_file(target: &std::path::Path, config_toml: &str) -> Result<(), String> {
    let temporary = target.with_extension(format!("toml.{}.tmp", std::process::id()));
    fs::write(&temporary, config_toml).map_err(|error| error.to_string())?;
    if target.exists() {
        fs::remove_file(&target).map_err(|error| error.to_string())?;
    }
    fs::rename(temporary, target).map_err(|error| error.to_string())
}

fn remove_legacy_server_profiles(config_toml: &str) -> Result<String, String> {
    let mut document = config_toml
        .parse::<toml_edit::Document>()
        .map_err(|error| format!("TOML 校验失败：{error}"))?;
    for key in ["model_providers", "profiles"] {
        let Some(table) = document
            .get_mut(key)
            .and_then(toml_edit::Item::as_table_mut)
        else {
            continue;
        };
        let app_owned = table
            .iter()
            .filter_map(|(name, _)| name.starts_with("server-").then(|| name.to_string()))
            .collect::<Vec<_>>();
        for name in app_owned {
            table.remove(&name);
        }
        if table.is_empty() {
            document.remove(key);
        }
    }

    let updated = document.to_string();
    updated
        .parse::<toml::Value>()
        .map_err(|error| format!("TOML 校验失败：{error}"))?;
    Ok(updated)
}

fn build_server_profile(server: &ServerProfile) -> Result<String, String> {
    let provider_id = paths::server_profile_name(&server.id);
    let mut document = toml_edit::Document::new();
    document.insert("model_provider", toml_edit::value(&provider_id));

    let providers = document
        .entry("model_providers")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| "Codex 配置中的 model_providers 必须是表格".to_string())?;
    let provider = providers
        .entry(&provider_id)
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(|| format!("Codex provider {provider_id} 必须是表格"))?;
    provider.insert("name", toml_edit::value(server.name.trim()));
    provider.insert(
        "base_url",
        toml_edit::value(server.base_url.trim_end_matches('/')),
    );
    provider.insert(
        "env_key",
        toml_edit::value(paths::server_env_key(&server.id)),
    );
    provider.insert("wire_api", toml_edit::value("responses"));

    let profile = document.to_string();
    profile
        .parse::<toml::Value>()
        .map_err(|error| format!("TOML 校验失败：{error}"))?;
    Ok(profile)
}

fn parse_codex_settings(config_toml: &str) -> CodexSettings {
    let value = match config_toml.parse::<toml::Value>() {
        Ok(value) => value,
        Err(error) => {
            return CodexSettings {
                execution_mode: "custom".to_string(),
                config_error: Some(format!("TOML 校验失败：{error}")),
                ..CodexSettings::default()
            }
        }
    };
    let approval_policy = string_value(&value, "approval_policy");
    let sandbox_mode = string_value(&value, "sandbox_mode");
    let execution_mode = match (approval_policy.as_str(), sandbox_mode.as_str()) {
        ("", "") => "default",
        ("on-request", "workspace-write") => "standard",
        ("never", "danger-full-access") => "automatic",
        ("on-request", "read-only") => "read-only",
        _ => "custom",
    };
    CodexSettings {
        model: string_value(&value, "model"),
        model_reasoning_effort: string_value(&value, "model_reasoning_effort"),
        execution_mode: execution_mode.to_string(),
        web_search: string_value(&value, "web_search"),
        personality: string_value(&value, "personality"),
        config_error: None,
    }
}

fn string_value(value: &toml::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(toml::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn merge_codex_settings(
    config_toml: &str,
    settings: &SaveCodexSettingsRequest,
) -> Result<String, String> {
    let mut document = config_toml
        .parse::<toml_edit::Document>()
        .map_err(|error| format!("TOML 校验失败：{error}"))?;
    set_string(&mut document, "model", &settings.model);
    set_choice(
        &mut document,
        "model_reasoning_effort",
        &settings.model_reasoning_effort,
        &["minimal", "low", "medium", "high", "xhigh"],
    )?;
    set_choice(
        &mut document,
        "web_search",
        &settings.web_search,
        &["disabled", "cached", "indexed", "live"],
    )?;
    set_choice(
        &mut document,
        "personality",
        &settings.personality,
        &["none", "friendly", "pragmatic"],
    )?;
    match settings.execution_mode.trim() {
        "default" => {
            document.remove("approval_policy");
            document.remove("sandbox_mode");
        }
        "standard" => {
            set_document_string(&mut document, "approval_policy", "on-request");
            set_document_string(&mut document, "sandbox_mode", "workspace-write");
        }
        "automatic" => {
            set_document_string(&mut document, "approval_policy", "never");
            set_document_string(&mut document, "sandbox_mode", "danger-full-access");
        }
        "read-only" => {
            set_document_string(&mut document, "approval_policy", "on-request");
            set_document_string(&mut document, "sandbox_mode", "read-only");
        }
        "custom" => {}
        _ => return Err("执行方式不是支持的 Codex 配置".to_string()),
    }
    let updated = document.to_string();
    updated
        .parse::<toml::Value>()
        .map_err(|error| format!("TOML 校验失败：{error}"))?;
    Ok(updated)
}

fn set_string(document: &mut toml_edit::Document, key: &str, requested: &str) {
    let requested = requested.trim();
    if requested.is_empty() {
        document.remove(key);
    } else {
        set_document_string(document, key, requested);
    }
}

fn set_choice(
    document: &mut toml_edit::Document,
    key: &str,
    requested: &str,
    allowed: &[&str],
) -> Result<(), String> {
    let requested = requested.trim();
    if requested.is_empty() {
        document.remove(key);
        return Ok(());
    }
    let current = document.get(key).and_then(toml_edit::Item::as_str);
    if !allowed.contains(&requested) && current != Some(requested) {
        return Err(format!("{key} 不是 Codex 支持的配置值"));
    }
    set_document_string(document, key, requested);
    Ok(())
}

fn set_document_string(document: &mut toml_edit::Document, key: &str, value: &str) {
    if let Some(existing) = document
        .get_mut(key)
        .and_then(toml_edit::Item::as_value_mut)
    {
        let decor = existing.decor().clone();
        let mut replacement = toml_edit::Value::from(value);
        *replacement.decor_mut() = decor;
        *existing = replacement;
    } else {
        document.insert(key, toml_edit::value(value));
    }
}

#[tauri::command]
pub fn start_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    request: StartTerminalRequest,
) -> Result<TerminalInstance, String> {
    start_terminal_inner(&app, &state, request)
}

pub(crate) fn start_terminal_inner(
    app: &AppHandle,
    state: &AppState,
    request: StartTerminalRequest,
) -> Result<TerminalInstance, String> {
    let server_id = selected_server_id(&request)?;
    let server = load_servers(&app)?
        .into_iter()
        .find(|server| server.id == server_id)
        .ok_or_else(|| "Server 不存在".to_string())?;
    let workdir = if request.workdir.trim().is_empty() {
        std::env::current_dir().map_err(|error| error.to_string())?
    } else {
        PathBuf::from(request.workdir)
    };
    state.sessions.start(app, &workdir, &server, request.resume)
}

fn selected_server_id(request: &StartTerminalRequest) -> Result<&str, String> {
    request
        .server_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "请选择模型实例".to_string())
}

#[tauri::command]
pub fn restart_terminal(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> Result<TerminalInstance, String> {
    let existing = state
        .sessions
        .list()?
        .into_iter()
        .find(|terminal| terminal.id == id)
        .ok_or_else(|| "终端实例不存在或已退出".to_string())?;
    let server_id = existing
        .server_id
        .as_deref()
        .ok_or_else(|| "该会话没有绑定模型实例，无法重新启动".to_string())?;
    let server = load_servers(&app)?
        .into_iter()
        .find(|server| server.id == server_id)
        .ok_or_else(|| "Server 不存在".to_string())?;
    state.sessions.restart(&app, &id, &server)
}

#[tauri::command]
pub fn terminal_input(state: State<'_, AppState>, id: String, data: String) -> Result<(), String> {
    state.sessions.input(&id, &data)
}

#[tauri::command]
pub fn terminal_ready(
    state: State<'_, AppState>,
    id: String,
    request: ResizeRequest,
) -> Result<(), String> {
    state.sessions.renderer_ready(
        &id,
        request.rows,
        request.cols,
        request.pixel_width,
        request.pixel_height,
    )
}

#[tauri::command]
pub fn terminal_rendered(
    state: State<'_, AppState>,
    id: String,
    request: RenderedRequest,
) -> Result<(), String> {
    state.sessions.renderer_rendered(&id, request.sequence)
}

#[tauri::command]
pub fn terminal_resize(
    state: State<'_, AppState>,
    id: String,
    request: ResizeRequest,
) -> Result<(), String> {
    state.sessions.resize(
        &id,
        request.rows,
        request.cols,
        request.pixel_width,
        request.pixel_height,
    )
}

#[tauri::command]
pub fn interrupt_terminal(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.sessions.interrupt(&id)
}

#[tauri::command]
pub fn terminate_terminal(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.sessions.terminate(&id)
}

#[tauri::command]
pub fn force_terminate_terminal(state: State<'_, AppState>, id: String) -> Result<(), String> {
    state.sessions.force_terminate(&id)
}

#[tauri::command]
pub fn check_app_update() -> Result<crate::model::ReleaseInfo, String> {
    updates::check_release()
}

#[tauri::command]
pub fn check_codex_update(app: AppHandle) -> Result<CodexUpdateInfo, String> {
    updates::check_codex_update(&app)
}

#[tauri::command]
pub fn download_app_update(
    app: AppHandle,
    url: String,
    filename: String,
    size: u64,
    digest: Option<String>,
    release_tag: String,
) -> Result<UpdateResult, String> {
    updates::download_release(&app, &url, &filename, size, digest.as_deref(), &release_tag)
}

#[tauri::command]
pub fn install_codex_update(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
) -> Result<UpdateResult, String> {
    install_codex_update_inner(&app, &state, version)
}

pub(crate) fn install_codex_update_inner(
    app: &AppHandle,
    state: &AppState,
    version: String,
) -> Result<UpdateResult, String> {
    let staged = updates::stage_codex(app, &version)?;
    if active_instance_count(state) == 0 {
        updates::activate_codex(app, &staged.version)
    } else {
        Ok(UpdateResult {
            kind: "codex-waiting".to_string(),
            ..staged
        })
    }
}

#[tauri::command]
pub fn activate_codex_update(
    app: AppHandle,
    state: State<'_, AppState>,
    version: String,
) -> Result<UpdateResult, String> {
    activate_codex_update_inner(&app, &state, version)
}

pub(crate) fn activate_codex_update_inner(
    app: &AppHandle,
    state: &AppState,
    version: String,
) -> Result<UpdateResult, String> {
    if active_instance_count(state) != 0 {
        return Err(format!(
            "仍有 {} 个活动任务；请先停止后再应用 Codex 更新",
            active_instance_count(state)
        ));
    }
    updates::activate_codex(app, &version)
}

#[tauri::command]
pub fn apply_app_update(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    apply_app_update_inner(&app, &state, path)
}

pub(crate) fn apply_app_update_inner(
    app: &AppHandle,
    state: &AppState,
    path: String,
) -> Result<(), String> {
    if active_instance_count(state) != 0 {
        return Err(format!(
            "仍有 {} 个活动任务；更新将在任务归零后应用",
            active_instance_count(state)
        ));
    }
    updates::launch_desktop_update(app, &path)
}

fn active_instance_count(state: &AppState) -> usize {
    state.sessions.active_count()
}

#[cfg(test)]
mod tests {
    use super::{
        build_server_profile, merge_codex_settings, normalize_server_defaults,
        parse_codex_settings, parse_servers, remove_legacy_server_profiles, selected_server_id,
        set_server_default, SaveCodexSettingsRequest, StartTerminalRequest,
    };
    use crate::model::ServerProfile;

    #[test]
    fn resume_keeps_the_selected_model_instance() {
        let request = StartTerminalRequest {
            workdir: "D:\\workspace".to_string(),
            server_id: Some("local-custom".to_string()),
            resume: true,
        };

        assert_eq!(selected_server_id(&request), Ok("local-custom"));
    }

    #[test]
    fn starting_without_a_model_instance_is_rejected() {
        let request = StartTerminalRequest {
            workdir: "D:\\workspace".to_string(),
            server_id: None,
            resume: false,
        };

        assert_eq!(
            selected_server_id(&request),
            Err("请选择模型实例".to_string())
        );
    }

    #[test]
    fn model_instance_defaults_are_normalized_to_one() {
        let mut servers = vec![
            ServerProfile {
                id: "one".to_string(),
                name: "One".to_string(),
                base_url: "https://one.example".to_string(),
                sk: "test-only".to_string(),
                is_default: false,
            },
            ServerProfile {
                id: "two".to_string(),
                name: "Two".to_string(),
                base_url: "https://two.example".to_string(),
                sk: "test-only".to_string(),
                is_default: true,
            },
            ServerProfile {
                id: "three".to_string(),
                name: "Three".to_string(),
                base_url: "https://three.example".to_string(),
                sk: "test-only".to_string(),
                is_default: true,
            },
        ];

        normalize_server_defaults(&mut servers);

        assert!(!servers[0].is_default);
        assert!(servers[1].is_default);
        assert!(!servers[2].is_default);
    }

    #[test]
    fn legacy_model_instance_file_ignores_removed_model_and_adds_default() {
        let servers = parse_servers(
            r#"[{"id":"legacy","name":"Legacy","base_url":"https://example.invalid/v1","sk":"test-only","default_model":"gpt-5"}]"#,
        )
        .expect("parse legacy model instance file");

        assert_eq!(servers.len(), 1);
        assert_eq!(servers[0].id, "legacy");
        assert!(servers[0].is_default);
    }

    #[test]
    fn selecting_a_model_instance_clears_the_previous_default() {
        let mut servers = vec![
            ServerProfile {
                id: "one".to_string(),
                name: "One".to_string(),
                base_url: "https://one.example".to_string(),
                sk: "test-only".to_string(),
                is_default: true,
            },
            ServerProfile {
                id: "two".to_string(),
                name: "Two".to_string(),
                base_url: "https://two.example".to_string(),
                sk: "test-only".to_string(),
                is_default: false,
            },
        ];

        set_server_default(&mut servers, "two");

        assert!(!servers[0].is_default);
        assert!(servers[1].is_default);
    }

    #[test]
    fn server_profile_uses_current_codex_profile_file_shape() {
        let profile = build_server_profile(&ServerProfile {
            id: "api-one".to_string(),
            name: "API One".to_string(),
            base_url: "https://api.example.com/v1/".to_string(),
            sk: "test-only".to_string(),
            is_default: false,
        })
        .expect("build profile");
        let value = profile.parse::<toml::Value>().expect("valid TOML");

        assert_eq!(value["model_provider"].as_str(), Some("server-api-one"));
        assert_eq!(
            value["model_providers"]["server-api-one"]["base_url"].as_str(),
            Some("https://api.example.com/v1")
        );
        assert!(value.get("profiles").is_none());
        assert!(value.get("model").is_none());
        assert!(value["model_providers"]["server-api-one"]
            .get("sk")
            .is_none());
    }

    #[test]
    fn removing_server_profiles_leaves_user_provider_intact() {
        let updated = remove_legacy_server_profiles(
            "# Keep this comment\n[model_providers.user-provider]\nname = \"User\"\nbase_url = \"https://user.example/v1\"\n\n[model_providers.server-old]\nname = \"Old\"\nbase_url = \"https://old.example/v1\"\n\n[profiles.server-old]\nmodel_provider = \"server-old\"\n",
        )
        .expect("remove legacy provider");
        let value = updated.parse::<toml::Value>().expect("valid TOML");

        assert!(updated.contains("# Keep this comment"));
        assert!(value["model_providers"].get("server-old").is_none());
        assert_eq!(
            value["model_providers"]["user-provider"]["name"].as_str(),
            Some("User")
        );
        assert!(value.get("profiles").is_none());
    }

    #[test]
    fn reads_guided_settings_from_codex_toml() {
        let settings = parse_codex_settings(
            r#"model = "gpt-5.6"
model_reasoning_effort = "high"
approval_policy = "on-request"
sandbox_mode = "workspace-write"
web_search = "cached"
personality = "pragmatic"
"#,
        );

        assert_eq!(settings.model, "gpt-5.6");
        assert_eq!(settings.model_reasoning_effort, "high");
        assert_eq!(settings.execution_mode, "standard");
        assert_eq!(settings.web_search, "cached");
        assert_eq!(settings.personality, "pragmatic");
        assert_eq!(settings.config_error, None);
    }

    #[test]
    fn guided_save_preserves_advanced_values_and_comments() {
        let original = r#"# Keep this comment
model = "old-model"

[features]
multi_agent = true
"#;
        let updated = merge_codex_settings(
            original,
            &SaveCodexSettingsRequest {
                model: "new-model".to_string(),
                model_reasoning_effort: "high".to_string(),
                execution_mode: "automatic".to_string(),
                web_search: "live".to_string(),
                personality: "friendly".to_string(),
            },
        )
        .expect("merge guided settings");
        let value = updated.parse::<toml::Value>().expect("valid TOML");

        assert!(updated.contains("# Keep this comment"));
        assert_eq!(value["model"].as_str(), Some("new-model"));
        assert_eq!(value["approval_policy"].as_str(), Some("never"));
        assert_eq!(value["sandbox_mode"].as_str(), Some("danger-full-access"));
        assert_eq!(value["features"]["multi_agent"].as_bool(), Some(true));
    }
}
