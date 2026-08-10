use portable_pty::{Child, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, Sender, TryRecvError, TrySendError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const GRACEFUL_STOP_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostConfig {
    pub node: String,
    pub entry: String,
    pub home: String,
    pub workdir: String,
    pub resume: bool,
    pub profile: Option<String>,
    pub env_key: Option<String>,
    pub sk: Option<String>,
    pub rows: u16,
    pub cols: u16,
    pub pixel_width: u16,
    pub pixel_height: u16,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostCommand {
    Input {
        data: String,
    },
    Resize {
        rows: u16,
        cols: u16,
        pixel_width: u16,
        pixel_height: u16,
    },
    Interrupt,
    Ping {
        nonce: u64,
    },
    Terminate,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum HostEvent {
    Ready { codex_pid: Option<u32> },
    Pong { nonce: u64 },
    Output { data: String },
    Overflow,
    Error { message: String },
    Exited { code: Option<u32> },
}

enum HostStatus {
    Overflow,
    Error(String),
    Exited(Option<u32>),
}

struct FallbackProcessGuard(Option<u32>);

impl Drop for FallbackProcessGuard {
    fn drop(&mut self) {
        if let Some(pid) = self.0 {
            crate::job::terminate_process_tree(pid);
        }
    }
}

pub fn run_from_stdin() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_line(&mut input)
        .map_err(|error| format!("读取 Session Host 配置失败：{error}"))?;
    if input.trim().is_empty() {
        return Err("Session Host 配置为空".to_string());
    }
    let config: HostConfig =
        serde_json::from_str(&input).map_err(|error| format!("Session Host 配置无效：{error}"))?;
    run(config)
}

fn run(config: HostConfig) -> Result<(), String> {
    let pty = NativePtySystem::default();
    let pair = pty
        .openpty(PtySize {
            rows: config.rows,
            cols: config.cols,
            pixel_width: config.pixel_width,
            pixel_height: config.pixel_height,
        })
        .map_err(|error| format!("无法创建 ConPTY：{error:?}"))?;

    let mut command = CommandBuilder::new(config.node);
    command.arg(config.entry);
    command.cwd(config.workdir);
    command.env("CODEX_HOME", config.home);
    for argument in codex_cli_arguments(config.profile.as_deref(), config.resume) {
        command.arg(argument);
    }
    if let (Some(key), Some(sk)) = (config.env_key, config.sk) {
        command.env(key, sk);
    }

    let mut child = pair
        .slave
        .spawn_command(command)
        .map_err(|error| format!("无法启动 Codex：{error}"))?;
    drop(pair.slave);
    let writer = Arc::new(Mutex::new(
        pair.master
            .take_writer()
            .map_err(|error| format!("ConPTY writer 不可用：{error}"))?,
    ));
    let reader = pair
        .master
        .try_clone_reader()
        .map_err(|error| format!("ConPTY reader 不可用：{error}"))?;
    let master: Arc<Mutex<Box<dyn MasterPty + Send>>> = Arc::new(Mutex::new(pair.master));
    let pid = child.process_id();
    let job = match pid {
        Some(pid) => crate::job::JobObject::attach(pid).ok(),
        None => {
            let _ = child.kill();
            return Err("Codex 进程没有 PID".to_string());
        }
    };
    let _fallback_guard = FallbackProcessGuard(job.is_none().then_some(pid.unwrap()));
    let child = Arc::new(Mutex::new(child));

    let (output_tx, output_rx) = sync_channel::<String>(256);
    let (status_tx, status_rx) = std::sync::mpsc::channel::<HostStatus>();
    let overflowed = Arc::new(AtomicBool::new(false));

    spawn_output_reader(
        reader,
        output_tx,
        status_tx.clone(),
        Arc::clone(&overflowed),
    );
    spawn_child_monitor(Arc::clone(&child), status_tx.clone());

    let (command_tx, command_rx) = std::sync::mpsc::channel::<HostCommand>();
    spawn_command_reader(command_tx);

    let stdout = io::stdout();
    let mut output = BufWriter::new(stdout.lock());
    write_event(&mut output, &HostEvent::Ready { codex_pid: pid })?;

    let mut stop_deadline = None;
    loop {
        while let Ok(status) = status_rx.try_recv() {
            match status {
                HostStatus::Overflow => write_event(&mut output, &HostEvent::Overflow)?,
                HostStatus::Error(message) => {
                    write_event(&mut output, &HostEvent::Error { message })?
                }
                HostStatus::Exited(code) => {
                    write_event(&mut output, &HostEvent::Exited { code })?;
                    return Ok(());
                }
            }
        }

        loop {
            match command_rx.try_recv() {
                Ok(HostCommand::Input { data }) => {
                    if let Err(error) = writer
                        .lock()
                        .map_err(|_| "终端写入锁已损坏".to_string())
                        .and_then(|mut writer| {
                            writer
                                .write_all(data.as_bytes())
                                .and_then(|_| writer.flush())
                                .map_err(|error| error.to_string())
                        })
                    {
                        write_event(&mut output, &HostEvent::Error { message: error })?;
                    }
                }
                Ok(HostCommand::Resize {
                    rows,
                    cols,
                    pixel_width,
                    pixel_height,
                }) => {
                    if let Err(error) = master
                        .lock()
                        .map_err(|_| "ConPTY 锁已损坏".to_string())
                        .and_then(|master| {
                            master
                                .resize(PtySize {
                                    rows,
                                    cols,
                                    pixel_width,
                                    pixel_height,
                                })
                                .map_err(|error| error.to_string())
                        })
                    {
                        write_event(&mut output, &HostEvent::Error { message: error })?;
                    }
                }
                Ok(HostCommand::Interrupt) => {
                    if let Err(error) = writer
                        .lock()
                        .map_err(|_| "终端写入锁已损坏".to_string())
                        .and_then(|mut writer| {
                            writer
                                .write_all(b"\x03")
                                .and_then(|_| writer.flush())
                                .map_err(|error| error.to_string())
                        })
                    {
                        write_event(&mut output, &HostEvent::Error { message: error })?;
                    }
                }
                Ok(HostCommand::Ping { nonce }) => {
                    write_event(&mut output, &HostEvent::Pong { nonce })?;
                }
                Ok(HostCommand::Terminate) => {
                    if stop_deadline.is_none() {
                        if let Err(error) = writer
                            .lock()
                            .map_err(|_| "终端写入锁已损坏".to_string())
                            .and_then(|mut writer| {
                                writer
                                    .write_all(b"\x03")
                                    .and_then(|_| writer.flush())
                                    .map_err(|error| error.to_string())
                            })
                        {
                            write_event(&mut output, &HostEvent::Error { message: error })?;
                        }
                        stop_deadline = Some(Instant::now() + GRACEFUL_STOP_TIMEOUT);
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    terminate_codex(job.as_ref(), pid);
                    return Ok(());
                }
            }
        }
        if stop_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            terminate_codex(job.as_ref(), pid);
            return Ok(());
        }

        match output_rx.recv_timeout(Duration::from_millis(20)) {
            Ok(data) => write_event(&mut output, &HostEvent::Output { data })?,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

fn codex_cli_arguments(profile: Option<&str>, resume: bool) -> Vec<String> {
    let mut arguments = Vec::new();
    if let Some(profile) = profile {
        arguments.push("--profile".to_string());
        arguments.push(profile.to_string());
    }
    if resume {
        arguments.push("resume".to_string());
    }
    arguments
}

fn terminate_codex(job: Option<&crate::job::JobObject>, pid: Option<u32>) {
    if let Some(job) = job {
        if job.terminate().is_ok() {
            return;
        }
    }
    if let Some(pid) = pid {
        crate::job::terminate_process_tree(pid);
    }
}

fn spawn_output_reader(
    mut reader: Box<dyn Read + Send>,
    output_tx: std::sync::mpsc::SyncSender<String>,
    status_tx: Sender<HostStatus>,
    overflowed: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        let mut pending_utf8 = Vec::new();
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let data = drain_utf8(&mut pending_utf8, true);
                    if !data.is_empty() && !overflowed.load(Ordering::Acquire) {
                        let _ = output_tx.try_send(data);
                    }
                    break;
                }
                Ok(size) => {
                    if overflowed.load(Ordering::Acquire) {
                        continue;
                    }
                    pending_utf8.extend_from_slice(&buffer[..size]);
                    let data = drain_utf8(&mut pending_utf8, false);
                    if data.is_empty() {
                        continue;
                    }
                    if let Err(TrySendError::Full(_)) = output_tx.try_send(data) {
                        if !overflowed.swap(true, Ordering::AcqRel) {
                            let _ = status_tx.send(HostStatus::Overflow);
                        }
                    }
                }
                Err(error) => {
                    let _ = status_tx.send(HostStatus::Error(error.to_string()));
                    break;
                }
            }
        }
    });
}

fn drain_utf8(buffer: &mut Vec<u8>, end_of_stream: bool) -> String {
    let mut output = String::new();
    loop {
        match std::str::from_utf8(buffer) {
            Ok(valid) => {
                output.push_str(valid);
                buffer.clear();
                break;
            }
            Err(error) => {
                let valid = error.valid_up_to();
                if valid > 0 {
                    output.push_str(std::str::from_utf8(&buffer[..valid]).unwrap_or_default());
                    buffer.drain(..valid);
                }
                match error.error_len() {
                    Some(length) => {
                        output.push('\u{fffd}');
                        let consumed = length.min(buffer.len());
                        buffer.drain(..consumed);
                    }
                    None if end_of_stream => {
                        output.push_str(&String::from_utf8_lossy(buffer));
                        buffer.clear();
                        break;
                    }
                    None => break,
                }
            }
        }
    }
    output
}

fn spawn_child_monitor(
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    status_tx: Sender<HostStatus>,
) {
    thread::spawn(move || {
        let result = child.lock().ok().and_then(|mut child| child.wait().ok());
        let _ = status_tx.send(HostStatus::Exited(result.map(|status| status.exit_code())));
    });
}

fn spawn_command_reader(command_tx: Sender<HostCommand>) {
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in BufReader::new(stdin.lock()).lines() {
            let Ok(line) = line else { break };
            match serde_json::from_str::<HostCommand>(&line) {
                Ok(command) => {
                    if command_tx.send(command).is_err() {
                        break;
                    }
                }
                Err(error) => {
                    let _ = command_tx.send(HostCommand::Terminate);
                    let _ = error;
                    break;
                }
            }
        }
    });
}

fn write_event(output: &mut BufWriter<impl Write>, event: &HostEvent) -> Result<(), String> {
    serde_json::to_writer(&mut *output, event).map_err(|error| error.to_string())?;
    output.write_all(b"\n").map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{codex_cli_arguments, drain_utf8};

    #[test]
    fn resume_places_the_selected_profile_before_the_subcommand() {
        assert_eq!(
            codex_cli_arguments(Some("server-local-custom"), true),
            vec!["--profile", "server-local-custom", "resume"]
        );
    }

    #[test]
    fn preserves_utf8_split_across_pty_reads() {
        let bytes = "终端输出".as_bytes();
        let mut pending = bytes[..2].to_vec();
        assert_eq!(drain_utf8(&mut pending, false), "");
        pending.extend_from_slice(&bytes[2..5]);
        assert_eq!(drain_utf8(&mut pending, false), "终");
        pending.extend_from_slice(&bytes[5..]);
        assert_eq!(drain_utf8(&mut pending, false), "端输出");
        assert!(pending.is_empty());
    }

    #[test]
    fn replaces_only_invalid_utf8_bytes() {
        let mut pending = b"ok\xffdone".to_vec();
        assert_eq!(drain_utf8(&mut pending, true), "ok\u{fffd}done");
        assert!(pending.is_empty());
    }
}
