use crate::commands::{self, AppState};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

const BRIDGE_ADDRESS: &str = "127.0.0.1:1422";
const EVENT_LIMIT: usize = 512;
const EVENT_WAIT: Duration = Duration::from_secs(20);

#[derive(Clone, Default)]
pub struct DevEventHub {
    inner: Arc<(Mutex<EventState>, Condvar)>,
}

#[derive(Default)]
struct EventState {
    sequence: u64,
    events: VecDeque<BridgeEvent>,
}

#[derive(Clone, Serialize)]
struct BridgeEvent {
    sequence: u64,
    event: String,
    payload: Value,
}

#[derive(Deserialize)]
struct BridgeCall {
    command: String,
    #[serde(default)]
    args: Value,
}

impl DevEventHub {
    pub fn publish<T: Serialize>(&self, event: &str, payload: &T) {
        let Ok(payload) = serde_json::to_value(payload) else {
            return;
        };
        let (lock, ready) = &*self.inner;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        state.sequence = state.sequence.wrapping_add(1);
        let sequence = state.sequence;
        state.events.push_back(BridgeEvent {
            sequence,
            event: event.to_string(),
            payload,
        });
        while state.events.len() > EVENT_LIMIT {
            state.events.pop_front();
        }
        ready.notify_all();
    }

    fn wait_after(&self, after: Option<u64>, timeout: Duration) -> (u64, Vec<BridgeEvent>) {
        let (lock, ready) = &*self.inner;
        let mut state = lock.lock().unwrap_or_else(|error| error.into_inner());
        let Some(after) = after else {
            return (state.sequence, Vec::new());
        };
        if state.sequence <= after {
            state = ready
                .wait_timeout_while(state, timeout, |value| value.sequence <= after)
                .unwrap_or_else(|error| error.into_inner())
                .0;
        }
        let events = state
            .events
            .iter()
            .filter(|event| event.sequence > after)
            .cloned()
            .collect::<Vec<_>>();
        let cursor = events
            .last()
            .map(|event| event.sequence)
            .unwrap_or(after.max(state.sequence));
        (cursor, events)
    }
}

pub fn start(app: AppHandle, state: AppState) -> Result<(), String> {
    let server = Server::http(BRIDGE_ADDRESS)
        .map_err(|error| format!("无法启动浏览器开发桥接 {BRIDGE_ADDRESS}：{error}"))?;
    thread::Builder::new()
        .name("tauri-codex-dev-bridge".to_string())
        .spawn(move || {
            for request in server.incoming_requests() {
                let app = app.clone();
                let state = state.clone();
                thread::spawn(move || handle_request(request, app, state));
            }
        })
        .map_err(|error| format!("无法启动浏览器开发桥接线程：{error}"))?;
    Ok(())
}

fn handle_request(mut request: Request, app: AppHandle, state: AppState) {
    let url = request.url().to_string();
    let response = match (request.method(), url.split('?').next().unwrap_or_default()) {
        (&Method::Get, "/__tauri_codex__/health") => success(json!({ "ready": true })),
        (&Method::Get, "/__tauri_codex__/events") => match query_after(&url) {
            Ok(after) => {
                let (cursor, events) = state.dev_events.wait_after(after, EVENT_WAIT);
                success(json!({ "cursor": cursor, "events": events }))
            }
            Err(error) => failure(StatusCode(400), error),
        },
        (&Method::Post, "/__tauri_codex__/call") => {
            let mut body = String::new();
            match request
                .as_reader()
                .read_to_string(&mut body)
                .map_err(|error| error.to_string())
                .and_then(|_| {
                    serde_json::from_str::<BridgeCall>(&body).map_err(|error| error.to_string())
                })
                .and_then(|call| dispatch(&app, &state, call))
            {
                Ok(result) => success(result),
                Err(error) => failure(StatusCode(400), error),
            }
        }
        _ => failure(StatusCode(404), "浏览器开发桥接路由不存在".to_string()),
    };
    let _ = request.respond(response);
}

fn dispatch(app: &AppHandle, state: &AppState, call: BridgeCall) -> Result<Value, String> {
    match call.command.as_str() {
        "get_snapshot" => value(commands::snapshot(app, state)?),
        "start_terminal" => value(commands::start_terminal_inner(
            app,
            state,
            argument(&call.args, "request")?,
        )?),
        "terminal_input" => {
            let id: String = argument(&call.args, "id")?;
            let data: String = argument(&call.args, "data")?;
            value(state.sessions.input(&id, &data)?)
        }
        "restart_terminal" => {
            let id: String = argument(&call.args, "id")?;
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
            let server = commands::load_servers(app)?
                .into_iter()
                .find(|server| server.id == server_id)
                .ok_or_else(|| "Server 不存在".to_string())?;
            value(state.sessions.restart(app, &id, &server)?)
        }
        "terminal_ready" => {
            let id: String = argument(&call.args, "id")?;
            let request: commands::ResizeRequest = argument(&call.args, "request")?;
            value(state.sessions.renderer_ready(
                &id,
                request.rows,
                request.cols,
                request.pixel_width,
                request.pixel_height,
            )?)
        }
        "terminal_rendered" => {
            let id: String = argument(&call.args, "id")?;
            let request: commands::RenderedRequest = argument(&call.args, "request")?;
            value(state.sessions.renderer_rendered(&id, request.sequence)?)
        }
        "terminal_resize" => {
            let id: String = argument(&call.args, "id")?;
            let request: commands::ResizeRequest = argument(&call.args, "request")?;
            value(state.sessions.resize(
                &id,
                request.rows,
                request.cols,
                request.pixel_width,
                request.pixel_height,
            )?)
        }
        "interrupt_terminal" => {
            let id: String = argument(&call.args, "id")?;
            value(state.sessions.interrupt(&id)?)
        }
        "terminate_terminal" => {
            let id: String = argument(&call.args, "id")?;
            value(state.sessions.terminate_if_running(&id)?)
        }
        "force_terminate_terminal" => {
            let id: String = argument(&call.args, "id")?;
            value(state.sessions.force_terminate(&id)?)
        }
        "get_server" => value(commands::get_server(
            app.clone(),
            argument(&call.args, "id")?,
        )?),
        "save_server" => value(commands::save_server(
            app.clone(),
            argument(&call.args, "profile")?,
        )?),
        "delete_server" => value(commands::delete_server(
            app.clone(),
            argument(&call.args, "id")?,
        )?),
        "save_config" => value(commands::save_config(
            app.clone(),
            argument(&call.args, "configToml")?,
        )?),
        "save_codex_settings" => value(commands::save_codex_settings(
            app.clone(),
            argument(&call.args, "settings")?,
        )?),
        "check_update" => value(commands::check_update()?),
        "prepare_update" => value(commands::prepare_update()?),
        "activate_update" => value(commands::activate_update_inner(state)?),
        "cancel_update" => value(commands::cancel_update_inner(state)?),
        _ => Err(format!("浏览器开发桥接不支持命令：{}", call.command)),
    }
}

fn argument<T: DeserializeOwned>(args: &Value, key: &str) -> Result<T, String> {
    serde_json::from_value(args.get(key).cloned().unwrap_or(Value::Null))
        .map_err(|error| format!("参数 {key} 无效：{error}"))
}

fn value<T: Serialize>(result: T) -> Result<Value, String> {
    serde_json::to_value(result).map_err(|error| error.to_string())
}

fn query_after(url: &str) -> Result<Option<u64>, String> {
    let Some(query) = url.split_once('?').map(|(_, query)| query) else {
        return Ok(None);
    };
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        if key == "after" {
            return value
                .parse::<u64>()
                .map(Some)
                .map_err(|_| "事件游标无效".to_string());
        }
    }
    Ok(None)
}

fn success(result: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(StatusCode(200), json!({ "ok": true, "result": result }))
}

fn failure(status: StatusCode, error: String) -> Response<std::io::Cursor<Vec<u8>>> {
    json_response(status, json!({ "ok": false, "error": error }))
}

fn json_response(status: StatusCode, body: Value) -> Response<std::io::Cursor<Vec<u8>>> {
    let mut response = Response::from_string(body.to_string()).with_status_code(status);
    response.add_header(
        Header::from_bytes("content-type", "application/json; charset=utf-8")
            .expect("static response header is valid"),
    );
    response.add_header(
        Header::from_bytes("cache-control", "no-store").expect("static response header is valid"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::{query_after, DevEventHub};
    use serde_json::json;
    use std::time::Duration;

    #[test]
    fn event_poll_starts_at_current_cursor_and_returns_only_new_events() {
        let hub = DevEventHub::default();
        hub.publish("before", &json!({ "value": 1 }));
        let (cursor, existing) = hub.wait_after(None, Duration::ZERO);
        assert_eq!(cursor, 1);
        assert!(existing.is_empty());

        hub.publish("after", &json!({ "value": 2 }));
        let (cursor, events) = hub.wait_after(Some(cursor), Duration::ZERO);
        assert_eq!(cursor, 2);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event, "after");
        assert_eq!(events[0].payload, json!({ "value": 2 }));
    }

    #[test]
    fn parses_optional_event_cursor() {
        assert_eq!(query_after("/__tauri_codex__/events").unwrap(), None);
        assert_eq!(
            query_after("/__tauri_codex__/events?after=42").unwrap(),
            Some(42)
        );
        assert!(query_after("/__tauri_codex__/events?after=nope").is_err());
    }
}
