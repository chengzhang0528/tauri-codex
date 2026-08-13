use crate::host::{HostCommand, HostConfig, HostEvent};
use crate::model::{ServerProfile, TerminalInstance};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::AppHandle;

const OUTPUT_QUEUE_CAPACITY: usize = 256;
const OUTPUT_BATCH_LIMIT: usize = 32;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(2);
const HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(8);
const TERMINATE_TIMEOUT: Duration = Duration::from_secs(3);
const RENDERER_READY_TIMEOUT: Duration = Duration::from_secs(30);
const RENDER_ACK_TIMEOUT: Duration = Duration::from_secs(8);

struct RuntimeSession {
    instance: TerminalInstance,
    status: Arc<Mutex<String>>,
    last_pong: Arc<Mutex<Instant>>,
    unresponsive: Arc<AtomicBool>,
    alive: Arc<AtomicBool>,
    renderer_ready: Arc<AtomicBool>,
    renderer_ack: Arc<(Mutex<u64>, Condvar)>,
    host_stdin: Arc<Mutex<ChildStdin>>,
    host: Arc<Mutex<Child>>,
}

#[derive(Clone, Default)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Arc<RuntimeSession>>>>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TerminalOutput {
    sequence: u64,
    data: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TerminalExit {
    code: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize)]
struct TerminalHeartbeat {
    responsive: bool,
}

impl SessionManager {
    pub fn list(&self) -> Result<Vec<TerminalInstance>, String> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| "会话锁已损坏".to_string())?;
        let mut result = sessions
            .values()
            .map(|session| {
                let mut instance = session.instance.clone();
                if let Ok(status) = session.status.lock() {
                    instance.status = status.clone();
                }
                instance
            })
            .collect::<Vec<_>>();
        result.sort_by(|left, right| left.id.cmp(&right.id));
        Ok(result)
    }

    pub fn active_count(&self) -> usize {
        self.sessions
            .lock()
            .map(|sessions| sessions.len())
            .unwrap_or(0)
    }

    pub fn start(
        &self,
        app: &AppHandle,
        workdir: &Path,
        server: &ServerProfile,
        resume: bool,
    ) -> Result<TerminalInstance, String> {
        if !workdir.is_dir() {
            return Err(format!("工作目录不存在：{}", workdir.display()));
        }

        let id = uuid::Uuid::new_v4().simple().to_string();
        let label = format!("terminal-{id}");
        let home = crate::paths::codex_home(app)?;
        let entry = crate::paths::codex_entry(app)?;
        let codex_version = crate::paths::codex_version(app)?;
        let node = crate::paths::system_node()?;
        let config = HostConfig {
            node: node.to_string_lossy().to_string(),
            entry: entry.to_string_lossy().to_string(),
            home: home.to_string_lossy().to_string(),
            workdir: workdir.to_string_lossy().to_string(),
            resume,
            profile: Some(crate::paths::server_profile_name(&server.id)),
            env_key: Some(crate::paths::server_env_key(&server.id)),
            sk: Some(server.sk.clone()),
            rows: 36,
            cols: 120,
            pixel_width: 0,
            pixel_height: 0,
        };

        let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
        let mut host = crate::job::background_command(current_exe);
        host.arg("--session-host")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut host = host
            .spawn()
            .map_err(|error| format!("无法启动 Session Host：{error}"))?;
        let host_pid = host.id();
        let mut host_stdin = host
            .stdin
            .take()
            .ok_or_else(|| "Session Host stdin 不可用".to_string())?;
        let stdout = host
            .stdout
            .take()
            .ok_or_else(|| "Session Host stdout 不可用".to_string())?;
        let stderr = host.stderr.take();
        write_host_message(&mut host_stdin, &config)?;

        if let Some(stderr) = stderr {
            let stderr_label = label.clone();
            let stderr_app = app.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if !line.trim().is_empty() {
                        crate::emit_to_terminal(&stderr_app, &stderr_label, "terminal-error", line);
                    }
                }
            });
        }

        let instance = TerminalInstance {
            id: id.clone(),
            window_label: label.clone(),
            workdir: workdir.to_string_lossy().to_string(),
            server_id: Some(server.id.clone()),
            resume,
            codex_version,
            pid: Some(host_pid),
            status: "running".to_string(),
        };
        let runtime = Arc::new(RuntimeSession {
            instance: instance.clone(),
            status: Arc::new(Mutex::new("running".to_string())),
            last_pong: Arc::new(Mutex::new(Instant::now())),
            unresponsive: Arc::new(AtomicBool::new(false)),
            alive: Arc::new(AtomicBool::new(true)),
            renderer_ready: Arc::new(AtomicBool::new(false)),
            renderer_ack: Arc::new((Mutex::new(0), Condvar::new())),
            host_stdin: Arc::new(Mutex::new(host_stdin)),
            host: Arc::new(Mutex::new(host)),
        });
        self.sessions
            .lock()
            .map_err(|_| "会话锁已损坏".to_string())?
            .insert(id.clone(), Arc::clone(&runtime));

        let (output_tx, output_rx) = sync_channel::<String>(OUTPUT_QUEUE_CAPACITY);
        let overflowed = Arc::new(AtomicBool::new(false));
        let exit_emitted = Arc::new(AtomicBool::new(false));
        spawn_host_reader(
            stdout,
            &label,
            app,
            output_tx,
            Arc::clone(&overflowed),
            Arc::clone(&exit_emitted),
            Arc::clone(&runtime.status),
            Arc::clone(&runtime.last_pong),
            Arc::clone(&runtime.unresponsive),
        );

        let dispatch_label = label.clone();
        let dispatch_app = app.clone();
        let dispatch_ready = Arc::clone(&runtime.renderer_ready);
        let dispatch_ack = Arc::clone(&runtime.renderer_ack);
        let dispatch_overflowed = Arc::clone(&overflowed);
        let dispatch_status = Arc::clone(&runtime.status);
        thread::spawn(move || {
            let ready_started = Instant::now();
            while !dispatch_ready.load(Ordering::Acquire)
                && !dispatch_overflowed.load(Ordering::Acquire)
                && ready_started.elapsed() < RENDERER_READY_TIMEOUT
            {
                thread::sleep(Duration::from_millis(10));
            }
            if !dispatch_ready.load(Ordering::Acquire) {
                mark_render_overflow(
                    &dispatch_overflowed,
                    &dispatch_status,
                    &dispatch_label,
                    &dispatch_app,
                );
                return;
            }

            let mut sequence = 0_u64;
            loop {
                if dispatch_overflowed.load(Ordering::Acquire) {
                    break;
                }
                let first = match output_rx.recv_timeout(Duration::from_millis(16)) {
                    Ok(data) => data,
                    Err(RecvTimeoutError::Timeout) => continue,
                    Err(RecvTimeoutError::Disconnected) => break,
                };
                let mut batch = first;
                for _ in 0..OUTPUT_BATCH_LIMIT {
                    match output_rx.try_recv() {
                        Ok(data) => batch.push_str(&data),
                        Err(_) => break,
                    }
                }
                sequence = sequence.wrapping_add(1);
                crate::emit_to_terminal(
                    &dispatch_app,
                    &dispatch_label,
                    "terminal-output",
                    TerminalOutput {
                        sequence,
                        data: batch,
                    },
                );
                if !wait_for_render_ack(&dispatch_ack, sequence, RENDER_ACK_TIMEOUT) {
                    mark_render_overflow(
                        &dispatch_overflowed,
                        &dispatch_status,
                        &dispatch_label,
                        &dispatch_app,
                    );
                    break;
                }
            }
        });

        spawn_heartbeat(Arc::clone(&runtime), &label, app);

        let manager = self.clone();
        let monitor_label = label.clone();
        let monitor_id = id.clone();
        let monitor_app = app.clone();
        let monitor_host = Arc::clone(&runtime.host);
        let monitor_exit = Arc::clone(&exit_emitted);
        let monitor_alive = Arc::clone(&runtime.alive);
        thread::spawn(move || {
            let status = loop {
                let status = monitor_host
                    .lock()
                    .ok()
                    .and_then(|mut host| host.try_wait().ok())
                    .flatten();
                if status.is_some() {
                    break status;
                }
                thread::sleep(Duration::from_millis(100));
            };
            monitor_alive.store(false, Ordering::Release);
            if !monitor_exit.swap(true, Ordering::AcqRel) {
                crate::emit_to_terminal(
                    &monitor_app,
                    &monitor_label,
                    "terminal-exit",
                    TerminalExit {
                        code: status.and_then(|status| status.code().map(|code| code as u32)),
                    },
                );
            }
            manager.remove(&monitor_id);
            crate::emit_to_main(&monitor_app, "terminal-state-changed", ());
        });

        Ok(instance)
    }

    pub fn restart(
        &self,
        app: &AppHandle,
        id: &str,
        server: &ServerProfile,
    ) -> Result<TerminalInstance, String> {
        let session = self.session(id)?;
        let workdir = PathBuf::from(session.instance.workdir.clone());
        let resume = session.instance.resume;
        self.terminate(id).ok();
        self.remove(id);
        self.start(app, &workdir, server, resume)
    }

    fn remove(&self, id: &str) {
        if let Ok(mut sessions) = self.sessions.lock() {
            sessions.remove(id);
        }
    }

    pub fn input(&self, id: &str, data: &str) -> Result<(), String> {
        let session = self.session(id)?;
        if session
            .status
            .lock()
            .map_err(|_| "终端状态锁已损坏".to_string())?
            .as_str()
            == "render-overflow"
        {
            return Err("终端渲染已溢出；只允许中断、停止或重新启动".to_string());
        }
        send_host_command(
            &session,
            &HostCommand::Input {
                data: data.to_string(),
            },
        )
    }

    pub fn renderer_ready(
        &self,
        id: &str,
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) -> Result<(), String> {
        let session = self.session(id)?;
        session.renderer_ready.store(true, Ordering::Release);
        send_host_command(
            &session,
            &HostCommand::Resize {
                rows,
                cols,
                pixel_width,
                pixel_height,
            },
        )
    }

    pub fn renderer_rendered(&self, id: &str, sequence: u64) -> Result<(), String> {
        let session = self.session(id)?;
        let (ack, ready) = &*session.renderer_ack;
        let mut last = ack.lock().map_err(|_| "终端渲染确认锁已损坏".to_string())?;
        if sequence > *last {
            *last = sequence;
            ready.notify_all();
        }
        Ok(())
    }

    pub fn resize(
        &self,
        id: &str,
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    ) -> Result<(), String> {
        self.send(
            id,
            &HostCommand::Resize {
                rows,
                cols,
                pixel_width,
                pixel_height,
            },
        )
    }

    pub fn interrupt(&self, id: &str) -> Result<(), String> {
        self.send(id, &HostCommand::Interrupt)
    }

    pub fn terminate(&self, id: &str) -> Result<(), String> {
        let session = self.session(id)?;
        terminate_session(&session)
    }

    #[cfg(debug_assertions)]
    pub fn terminate_if_running(&self, id: &str) -> Result<(), String> {
        let session = self
            .sessions
            .lock()
            .map_err(|_| "会话锁已损坏".to_string())?
            .get(id)
            .cloned();
        let Some(session) = session else {
            return Ok(());
        };
        terminate_session(&session)
    }

    pub fn force_terminate(&self, id: &str) -> Result<(), String> {
        let session = self.session(id)?;
        force_session(&session)
    }

    pub fn force_terminate_all(&self) {
        let sessions = self
            .sessions
            .lock()
            .map(|sessions| sessions.values().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for session in sessions {
            let _ = force_session(&session);
        }
    }

    fn send(&self, id: &str, command: &HostCommand) -> Result<(), String> {
        let session = self.session(id)?;
        self.send_command(&session, command)
    }

    fn send_command(&self, session: &RuntimeSession, command: &HostCommand) -> Result<(), String> {
        send_host_command(session, command)
    }

    fn session(&self, id: &str) -> Result<Arc<RuntimeSession>, String> {
        self.sessions
            .lock()
            .map_err(|_| "会话锁已损坏".to_string())?
            .get(id)
            .cloned()
            .ok_or_else(|| "终端实例不存在或已退出".to_string())
    }
}

fn terminate_session(session: &RuntimeSession) -> Result<(), String> {
    if send_host_command(&session, &HostCommand::Terminate).is_err() {
        return force_session(&session);
    }
    if wait_for_host_exit(&session, TERMINATE_TIMEOUT)? {
        Ok(())
    } else {
        force_session(&session)
    }
}

fn write_host_message<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), String> {
    serde_json::to_writer(&mut *writer, value).map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn send_host_command(session: &RuntimeSession, command: &HostCommand) -> Result<(), String> {
    let mut stdin = session
        .host_stdin
        .lock()
        .map_err(|_| "Session Host stdin 锁已损坏".to_string())?;
    write_host_message(&mut *stdin, command)
}

fn wait_for_host_exit(session: &RuntimeSession, timeout: Duration) -> Result<bool, String> {
    let started = Instant::now();
    loop {
        if session
            .host
            .lock()
            .map_err(|_| "Session Host 进程锁已损坏".to_string())?
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_some()
        {
            return Ok(true);
        }
        if started.elapsed() >= timeout {
            return Ok(false);
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn force_session(session: &RuntimeSession) -> Result<(), String> {
    if let Some(pid) = session.instance.pid {
        crate::job::terminate_process_tree(pid);
    }
    {
        let mut host = session
            .host
            .lock()
            .map_err(|_| "Session Host 进程锁已损坏".to_string())?;
        if host
            .try_wait()
            .map_err(|error| error.to_string())?
            .is_none()
        {
            host.kill().map_err(|error| error.to_string())?;
        }
    }
    if wait_for_host_exit(session, TERMINATE_TIMEOUT)? {
        Ok(())
    } else {
        Err("无法强制终止 Session Host 进程树".to_string())
    }
}

fn wait_for_render_ack(
    state: &Arc<(Mutex<u64>, Condvar)>,
    sequence: u64,
    timeout: Duration,
) -> bool {
    let (ack, ready) = &**state;
    let Ok(last) = ack.lock() else { return false };
    let Ok((last, result)) = ready.wait_timeout_while(last, timeout, |last| *last < sequence)
    else {
        return false;
    };
    !result.timed_out() && *last >= sequence
}

fn mark_render_overflow(
    overflowed: &AtomicBool,
    status: &Arc<Mutex<String>>,
    label: &str,
    app: &AppHandle,
) {
    if overflowed.swap(true, Ordering::AcqRel) {
        return;
    }
    set_status(status, "render-overflow");
    crate::emit_to_terminal(app, label, "terminal-overflow", ());
    crate::emit_to_main(app, "terminal-state-changed", ());
}

fn spawn_heartbeat(session: Arc<RuntimeSession>, label: &str, app: &AppHandle) {
    let label = label.to_string();
    let app = app.clone();
    thread::spawn(move || {
        let mut nonce = 0_u64;
        while session.alive.load(Ordering::Acquire) {
            thread::sleep(HEARTBEAT_INTERVAL);
            if !session.alive.load(Ordering::Acquire) {
                break;
            }
            nonce = nonce.wrapping_add(1);
            if send_host_command(&session, &HostCommand::Ping { nonce }).is_err() {
                mark_unresponsive(&session, &label, &app);
                continue;
            }
            let timed_out = session
                .last_pong
                .lock()
                .map(|last| last.elapsed() >= HEARTBEAT_TIMEOUT)
                .unwrap_or(true);
            if timed_out {
                mark_unresponsive(&session, &label, &app);
            }
        }
    });
}

fn mark_unresponsive(session: &RuntimeSession, label: &str, app: &AppHandle) {
    if session.unresponsive.swap(true, Ordering::AcqRel) {
        return;
    }
    set_status(&session.status, "host-unresponsive");
    crate::emit_to_terminal(
        app,
        label,
        "terminal-heartbeat",
        TerminalHeartbeat { responsive: false },
    );
    crate::emit_to_main(app, "terminal-state-changed", ());
}

fn set_status(status: &Arc<Mutex<String>>, value: &str) {
    if let Ok(mut status) = status.lock() {
        value.clone_into(&mut status);
    }
}

fn spawn_host_reader(
    stdout: impl std::io::Read + Send + 'static,
    label: &str,
    app: &AppHandle,
    output_tx: std::sync::mpsc::SyncSender<String>,
    overflowed: Arc<AtomicBool>,
    exit_emitted: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
    last_pong: Arc<Mutex<Instant>>,
    unresponsive: Arc<AtomicBool>,
) {
    let label = label.to_string();
    let app = app.clone();
    thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            let Ok(line) = line else { break };
            let Ok(event) = serde_json::from_str::<HostEvent>(&line) else {
                crate::emit_to_terminal(&app, &label, "terminal-error", "Session Host 输出无效");
                continue;
            };
            match event {
                HostEvent::Ready { .. } => {
                    if let Ok(mut last) = last_pong.lock() {
                        *last = Instant::now();
                    }
                }
                HostEvent::Pong { .. } => {
                    if let Ok(mut last) = last_pong.lock() {
                        *last = Instant::now();
                    }
                    if unresponsive.swap(false, Ordering::AcqRel) {
                        set_status(&status, "running");
                        crate::emit_to_terminal(
                            &app,
                            &label,
                            "terminal-heartbeat",
                            TerminalHeartbeat { responsive: true },
                        );
                        crate::emit_to_main(&app, "terminal-state-changed", ());
                    }
                }
                HostEvent::Output { data } => {
                    if overflowed.load(Ordering::Acquire) {
                        continue;
                    }
                    if output_tx.try_send(data).is_err() {
                        mark_render_overflow(&overflowed, &status, &label, &app);
                    }
                }
                HostEvent::Overflow => {
                    mark_render_overflow(&overflowed, &status, &label, &app);
                }
                HostEvent::Error { message } => {
                    crate::emit_to_terminal(&app, &label, "terminal-error", message);
                }
                HostEvent::Exited { code } => {
                    if !exit_emitted.swap(true, Ordering::AcqRel) {
                        crate::emit_to_terminal(
                            &app,
                            &label,
                            "terminal-exit",
                            TerminalExit { code },
                        );
                    }
                }
            }
        }
    });
}
