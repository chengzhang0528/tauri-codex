use super::contract::{DeliverySnapshot, UpdateIntent};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::Arc;

pub const PIPE_NAME: &str = r"\\.\pipe\tauri-codex-delivery-v2";
const MAX_MESSAGE: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Request {
    GetSnapshot,
    Intent(UpdateIntent),
    Ping,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Snapshot(DeliverySnapshot),
    Error(String),
    Pong,
}

pub fn serve(handler: Arc<dyn Fn(Request) -> Response + Send + Sync>) -> Result<(), String> {
    #[cfg(windows)]
    {
        serve_windows(handler)
    }
    #[cfg(not(windows))]
    {
        let _ = handler;
        Err("Launcher Broker 只支持 Windows Named Pipe".to_string())
    }
}

pub fn request(request: Request) -> Result<Response, String> {
    #[cfg(windows)]
    {
        request_windows(request)
    }
    #[cfg(not(windows))]
    {
        let _ = request;
        Err("Manager Broker 只支持 Windows Named Pipe".to_string())
    }
}

#[cfg(windows)]
fn serve_windows(handler: Arc<dyn Fn(Request) -> Response + Send + Sync>) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, ERROR_PIPE_CONNECTED, HANDLE};
    use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
    use windows::Win32::System::Pipes::{
        ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, PIPE_READMODE_MESSAGE,
        PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_MESSAGE, PIPE_WAIT,
    };
    let security = security_attributes()?;
    let name = std::ffi::OsStr::new(PIPE_NAME)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    loop {
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(name.as_ptr()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_MESSAGE | PIPE_READMODE_MESSAGE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                MAX_MESSAGE as u32,
                MAX_MESSAGE as u32,
                0,
                Some(&security.attributes),
            )
        };
        if handle.is_invalid() {
            return Err(format!(
                "创建 Named Pipe 失败：{}",
                windows::core::Error::from_win32()
            ));
        }
        let connected = unsafe { ConnectNamedPipe(handle, None) };
        if let Err(error) = connected {
            if error.code().0 as u32 != ERROR_PIPE_CONNECTED.0 {
                unsafe { CloseHandle(handle) }.ok();
                return Err(format!("连接 Named Pipe 失败：{error}"));
            }
        }
        let mut stream = unsafe { std::fs::File::from_raw_handle(handle.0 as _) };
        let response = match read_json::<Request>(&mut stream) {
            Ok(request) => (handler)(request),
            Err(error) => Response::Error(error),
        };
        let _ = write_json(&mut stream, &response);
        let handle = HANDLE(stream.as_raw_handle() as _);
        unsafe {
            DisconnectNamedPipe(handle).ok();
        }
        drop(stream);
    }
}

#[cfg(windows)]
fn request_windows(request: Request) -> Result<Response, String> {
    use std::fs::OpenOptions;
    use std::thread;
    for _ in 0..20 {
        match OpenOptions::new().read(true).write(true).open(PIPE_NAME) {
            Ok(mut stream) => {
                write_json(&mut stream, &request)?;
                return read_json(&mut stream);
            }
            Err(error) => {
                thread::sleep(std::time::Duration::from_millis(100));
                if error.kind() != std::io::ErrorKind::NotFound {
                    continue;
                }
            }
        }
    }
    Err("Launcher Broker Named Pipe 不可达".to_string())
}

#[cfg(windows)]
struct SecurityAttributes {
    attributes: windows::Win32::Security::SECURITY_ATTRIBUTES,
    descriptor: windows::Win32::Security::PSECURITY_DESCRIPTOR,
}

#[cfg(windows)]
impl Drop for SecurityAttributes {
    fn drop(&mut self) {
        unsafe {
            windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
                self.descriptor.0 as _,
            )));
        }
    }
}

#[cfg(windows)]
fn security_attributes() -> Result<SecurityAttributes, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::Authorization::{
        ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    };
    use windows::Win32::Security::{
        GetTokenInformation, TokenUser, SECURITY_ATTRIBUTES, TOKEN_QUERY, TOKEN_USER,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    let mut token = HANDLE::default();
    unsafe {
        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)
            .map_err(|error| error.to_string())?;
    }
    let mut length = 0u32;
    unsafe {
        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut length);
    }
    let mut bytes = vec![0u8; length as usize];
    let token_info = unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(bytes.as_mut_ptr() as _),
            length,
            &mut length,
        )
    };
    if let Err(error) = token_info {
        unsafe { windows::Win32::Foundation::CloseHandle(token).ok() };
        return Err(error.to_string());
    }
    let sid = unsafe {
        std::ptr::read_unaligned(bytes.as_ptr() as *const TOKEN_USER)
            .User
            .Sid
    };
    let mut sid_text = windows::core::PWSTR::null();
    unsafe {
        ConvertSidToStringSidW(sid, &mut sid_text).map_err(|error| error.to_string())?;
    }
    let sid_string = unsafe { sid_text.to_string().map_err(|error| error.to_string())? };
    unsafe {
        windows::Win32::Foundation::LocalFree(Some(windows::Win32::Foundation::HLOCAL(
            sid_text.0 as _,
        )));
        windows::Win32::Foundation::CloseHandle(token).ok();
    }
    let sddl = format!("D:P(A;;GA;;;{sid_string})");
    let wide = std::ffi::OsStr::new(&sddl)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let mut descriptor = windows::Win32::Security::PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            PCWSTR(wide.as_ptr()),
            1,
            &mut descriptor,
            None,
        )
        .map_err(|error| error.to_string())?;
    }
    let attributes = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    Ok(SecurityAttributes {
        attributes,
        descriptor,
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(stream: &mut impl Read) -> Result<T, String> {
    let mut length = [0u8; 4];
    stream
        .read_exact(&mut length)
        .map_err(|error| error.to_string())?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE {
        return Err("IPC 消息长度不合法".to_string());
    }
    let mut bytes = vec![0u8; length];
    stream
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    serde_json::from_slice(&bytes).map_err(|error| format!("IPC JSON 无法解析：{error}"))
}

fn write_json<T: Serialize>(stream: &mut impl Write, value: &T) -> Result<(), String> {
    let bytes = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if bytes.len() > MAX_MESSAGE {
        return Err("IPC 消息过大".to_string());
    }
    stream
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .map_err(|error| error.to_string())?;
    stream
        .write_all(&bytes)
        .map_err(|error| error.to_string())?;
    stream.flush().map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{read_json, write_json, Request, MAX_MESSAGE};
    use crate::delivery::contract::{CheckTrigger, UpdateIntent};
    use std::io::Cursor;

    #[test]
    fn framed_ipc_round_trips_typed_intent() {
        let request = Request::Intent(UpdateIntent::Check {
            trigger: CheckTrigger::Automatic,
        });
        let mut bytes = Cursor::new(Vec::new());
        write_json(&mut bytes, &request).unwrap();
        bytes.set_position(0);
        let decoded: Request = read_json(&mut bytes).unwrap();
        assert!(matches!(
            decoded,
            Request::Intent(UpdateIntent::Check {
                trigger: CheckTrigger::Automatic
            })
        ));
    }

    #[test]
    fn framed_ipc_rejects_zero_and_oversized_messages() {
        assert!(read_json::<Request>(&mut Cursor::new(0u32.to_le_bytes())).is_err());
        assert!(
            read_json::<Request>(&mut Cursor::new(((MAX_MESSAGE + 1) as u32).to_le_bytes()))
                .is_err()
        );
    }
}
