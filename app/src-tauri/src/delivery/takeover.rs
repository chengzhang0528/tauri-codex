use std::path::Path;

pub fn run(install_root: &Path, silent: bool) -> Result<(), String> {
    #[cfg(windows)]
    {
        run_windows(install_root, silent)
    }
    #[cfg(not(windows))]
    {
        let _ = (install_root, silent);
        Err("Installer 进程接管只支持 Windows".to_string())
    }
}

#[cfg(windows)]
#[derive(Debug, Clone)]
struct OwnedProcess {
    pid: u32,
    path: std::path::PathBuf,
}

#[cfg(windows)]
fn run_windows(install_root: &Path, silent: bool) -> Result<(), String> {
    let install_root = std::fs::canonicalize(install_root)
        .map_err(|error| format!("无法验证产品安装目录：{error}"))?;
    let executable =
        std::fs::canonicalize(std::env::current_exe().map_err(|error| error.to_string())?)
            .map_err(|error| format!("无法验证 Installer takeover helper：{error}"))?;
    let executable_root = executable
        .parent()
        .ok_or_else(|| "Installer takeover helper 缺少父目录".to_string())?;
    if normalize_path(executable_root) != normalize_path(&install_root) {
        return Err("Installer takeover 只能由现有安装目录内的 Launcher 执行".to_string());
    }

    let mut roots = vec![normalize_path(&install_root)];
    if let Ok(delivery_root) = crate::paths::delivery_root() {
        if delivery_root.is_dir() {
            let delivery_root = std::fs::canonicalize(&delivery_root)
                .map_err(|error| format!("无法验证产品交付目录：{error}"))?;
            roots.push(normalize_path(&delivery_root));
        }
    }

    let current_pid = std::process::id();
    let processes = owned_processes(&roots, current_pid)?;
    if processes.is_empty() {
        return Ok(());
    }
    if !silent && !confirm_takeover(&processes)? {
        return Err("用户取消了 Installer 进程接管".to_string());
    }

    request_normal_shutdown(&processes)?;
    std::thread::sleep(std::time::Duration::from_secs(5));

    let remaining = owned_processes(&roots, current_pid)?;
    for process in &remaining {
        terminate_verified_process(process, &roots)?;
    }
    if !remaining.is_empty() {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let survivors = owned_processes(&roots, current_pid)?;
    if survivors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "仍有产品进程无法关闭：{}",
            survivors
                .iter()
                .map(|process| format!("{} ({})", process.pid, process.path.display()))
                .collect::<Vec<_>>()
                .join("，")
        ))
    }
}

#[cfg(windows)]
fn owned_processes(roots: &[String], current_pid: u32) -> Result<Vec<OwnedProcess>, String> {
    use std::collections::{HashMap, HashSet};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
        TH32CS_SNAPPROCESS,
    };

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }
        .map_err(|error| error.to_string())?;
    let mut entry = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut entries = Vec::new();
    if unsafe { Process32FirstW(snapshot, &mut entry) }.is_ok() {
        loop {
            let name_length = entry
                .szExeFile
                .iter()
                .position(|character| *character == 0)
                .unwrap_or(entry.szExeFile.len());
            entries.push((
                entry.th32ProcessID,
                entry.th32ParentProcessID,
                String::from_utf16_lossy(&entry.szExeFile[..name_length]).to_lowercase(),
            ));
            if unsafe { Process32NextW(snapshot, &mut entry) }.is_err() {
                break;
            }
        }
    }
    unsafe { CloseHandle(snapshot) }.map_err(|error| error.to_string())?;

    let parents = entries
        .iter()
        .map(|(pid, parent, _)| (*pid, *parent))
        .collect::<HashMap<_, _>>();
    let mut excluded = HashSet::from([current_pid]);
    if let Some(parent) = parents.get(&current_pid).copied() {
        if parent != 0 {
            excluded.insert(parent);
        }
    }

    let mut result = Vec::new();
    let launcher_name = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_lowercase())
        })
        .ok_or_else(|| "无法确定 Launcher 进程名".to_string())?;
    for (pid, _, name) in entries {
        if excluded.contains(&pid) {
            continue;
        }
        let path = query_process_path(pid);
        let launcher_path_is_owned = path
            .as_ref()
            .map(|path| path_is_owned(path, roots))
            .unwrap_or(false);
        if name == launcher_name && !launcher_path_is_owned {
            return Err(format!("检测到无法安全接管的同名 Launcher 进程：PID {pid}"));
        }
        if name == "tauri-codex-manager.exe" && path.is_none() {
            return Err(format!("无法验证正在运行的 Manager 进程：PID {pid}"));
        }
        let Some(path) = path else {
            continue;
        };
        if path_is_owned(&path, roots) {
            result.push(OwnedProcess { pid, path });
        }
    }
    result.sort_by_key(|process| process.pid);
    Ok(result)
}

#[cfg(windows)]
fn query_process_path(pid: u32) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    let path = std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize]));
    Some(std::fs::canonicalize(&path).unwrap_or(path))
}

#[cfg(windows)]
fn request_normal_shutdown(processes: &[OwnedProcess]) -> Result<(), String> {
    use std::collections::HashSet;
    use windows::core::BOOL;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
    };

    unsafe extern "system" fn close_window(
        window: windows::Win32::Foundation::HWND,
        context: LPARAM,
    ) -> BOOL {
        let targets = unsafe { &*(context.0 as *const HashSet<u32>) };
        let mut pid = 0_u32;
        unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
        if targets.contains(&pid) {
            let _ = unsafe { PostMessageW(Some(window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        }
        BOOL(1)
    }

    let targets = processes
        .iter()
        .map(|process| process.pid)
        .collect::<HashSet<_>>();
    unsafe {
        EnumWindows(
            Some(close_window),
            LPARAM(&targets as *const HashSet<u32> as isize),
        )
    }
    .map_err(|error| format!("无法请求产品进程正常退出：{error}"))
}

#[cfg(windows)]
fn terminate_verified_process(process: &OwnedProcess, roots: &[String]) -> Result<(), String> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, TerminateProcess, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE,
    };

    let handle = unsafe {
        OpenProcess(
            PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE,
            false,
            process.pid,
        )
    }
    .map_err(|error| format!("无法打开待关闭进程 {}：{error}", process.pid))?;
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    let query = unsafe {
        QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    };
    let result = match query {
        Ok(()) => {
            use std::os::windows::ffi::OsStringExt;
            let actual =
                std::path::PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize]));
            let actual = std::fs::canonicalize(&actual).unwrap_or(actual);
            if !path_is_owned(&actual, roots) {
                Err(format!("进程 {} 的可执行路径已发生变化", process.pid))
            } else {
                unsafe { TerminateProcess(handle, 1) }
                    .map_err(|error| format!("无法终止产品进程 {}：{error}", process.pid))
            }
        }
        Err(error) => Err(format!("无法重新验证进程 {}：{error}", process.pid)),
    };
    let _ = unsafe { CloseHandle(handle) };
    result
}

#[cfg(windows)]
fn confirm_takeover(processes: &[OwnedProcess]) -> Result<bool, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDOK, MB_DEFBUTTON2, MB_ICONWARNING, MB_OKCANCEL,
    };

    let mut lines = processes
        .iter()
        .take(12)
        .map(|process| format!("PID {}  {}", process.pid, process.path.display()))
        .collect::<Vec<_>>();
    if processes.len() > lines.len() {
        lines.push(format!("另有 {} 个产品进程", processes.len() - lines.len()));
    }
    let message = format!(
        "安装程序需要关闭以下 tauri-codex 进程。请先保存正在进行的工作，然后选择“确定”。\n\n{}\n\n选择“取消”将停止安装。",
        lines.join("\n")
    );
    let message = std::ffi::OsStr::new(&message)
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let title = std::ffi::OsStr::new("tauri-codex Installer")
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OKCANCEL | MB_ICONWARNING | MB_DEFBUTTON2,
        )
    };
    Ok(result == IDOK)
}

#[cfg(windows)]
fn normalize_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('/', "\\");
    value
        .strip_prefix(r"\\?\")
        .unwrap_or(&value)
        .trim_end_matches('\\')
        .to_lowercase()
}

#[cfg(windows)]
fn path_is_owned(path: &Path, roots: &[String]) -> bool {
    let path = normalize_path(path);
    roots
        .iter()
        .any(|root| path == *root || path.starts_with(&format!("{root}\\")))
}

#[cfg(all(test, windows))]
mod tests {
    use super::path_is_owned;
    use std::path::Path;

    #[test]
    fn takeover_path_check_requires_a_component_boundary() {
        let roots = vec![r"c:\program files\tauri-codex".to_string()];
        assert!(path_is_owned(
            Path::new(r"C:\Program Files\tauri-codex\tauri-codex.exe"),
            &roots
        ));
        assert!(!path_is_owned(
            Path::new(r"C:\Program Files\tauri-codex-other\tool.exe"),
            &roots
        ));
    }
}
