use crate::{job, paths};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DOCTOR_TIMEOUT: Duration = Duration::from_secs(30);

pub fn doctor_manager(root: &Path) -> Result<(), String> {
    require_nonempty(&root.join("tauri-codex-manager.exe"), "Manager 入口")?;
    require_nonempty(
        &root.join("WebView2Loader.dll"),
        "Manager WebView2 运行时依赖",
    )?;
    verify_authenticode(&root.join("tauri-codex-manager.exe"))?;
    run_success(
        job::background_command(root.join("tauri-codex-manager.exe")).arg("--runtime-check"),
        DOCTOR_TIMEOUT,
        "Manager doctor",
    )
}

pub fn doctor_codex(root: &Path) -> Result<(), String> {
    let entry = codex_entry(root)?;
    verify_authenticode_tree(root)?;
    let smoke_home = root.join(format!(".doctor-home-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&smoke_home).map_err(|error| error.to_string())?;
    let result = run_success(
        job::background_command(paths::system_node()?)
            .arg(entry)
            .arg("--version")
            .env("CODEX_HOME", &smoke_home)
            .current_dir(root),
        DOCTOR_TIMEOUT,
        "Codex doctor",
    );
    let _ = fs::remove_dir_all(&smoke_home);
    result
}

pub fn verify_authenticode_tree(root: &Path) -> Result<(), String> {
    let mut pending = vec![root.to_path_buf()];
    let mut executable_count = 0usize;
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let file_type = entry.file_type().map_err(|error| error.to_string())?;
            if file_type.is_symlink() {
                return Err(format!(
                    "组件目录不允许符号链接：{}",
                    entry.path().display()
                ));
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if entry
                .path()
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("exe") || extension.eq_ignore_ascii_case("dll")
                })
            {
                executable_count += 1;
                verify_authenticode(&entry.path())?;
            }
        }
    }
    if executable_count == 0 {
        return Err(format!(
            "Codex 组件不包含 Windows executable：{}",
            root.display()
        ));
    }
    Ok(())
}

pub fn require_nonempty(path: &Path, label: &str) -> Result<(), String> {
    if !path.is_file() || fs::metadata(path).map_err(|error| error.to_string())?.len() == 0 {
        return Err(format!("{label}不存在或为空：{}", path.display()));
    }
    Ok(())
}

pub fn run_success(command: &mut Command, timeout: Duration, label: &str) -> Result<(), String> {
    command.stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{label} 启动失败：{error}"))?;
    let started = Instant::now();
    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(status) if status.success() => return Ok(()),
            Some(status) => return Err(format!("{label} 退出码 {}", status.code().unwrap_or(-1))),
            None if started.elapsed() >= timeout => {
                let pid = child.id();
                let _ = child.kill();
                crate::job::terminate_process_tree(pid);
                return Err(format!("{label} 超时"));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

pub fn codex_entry(root: &Path) -> Result<std::path::PathBuf, String> {
    [
        root.join("node_modules/@openai/codex/bin/codex.js"),
        root.join("node_modules/@openai/codex/bin/codex"),
        root.join("node_modules/@openai/codex/dist/cli.js"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| format!("Codex 入口不存在：{}", root.display()))
}

pub fn verify_authenticode(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        verify_authenticode_windows(path)
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(windows)]
fn verify_authenticode_windows(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::Security::WinTrust::{
        WinVerifyTrust, WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_FILE_INFO,
        WTD_CHOICE_FILE, WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY,
        WTD_UI_NONE,
    };
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: std::mem::size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: PCWSTR(wide.as_ptr()),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: std::mem::size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            Anonymous: windows::Win32::Security::WinTrust::WINTRUST_DATA_0 { pFile: &mut file },
            dwStateAction: WTD_STATEACTION_VERIFY,
            ..Default::default()
        };
        let status = WinVerifyTrust(
            windows::Win32::Foundation::HWND::default(),
            &WINTRUST_ACTION_GENERIC_VERIFY_V2 as *const _ as *mut _,
            &mut data as *mut _ as *mut _,
        );
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        let _ = WinVerifyTrust(
            windows::Win32::Foundation::HWND::default(),
            &WINTRUST_ACTION_GENERIC_VERIFY_V2 as *const _ as *mut _,
            &mut data as *mut _ as *mut _,
        );
        if status == 0 {
            Ok(())
        } else {
            Err(format!(
                "Authenticode 校验失败 {}：{}",
                path.display(),
                status
            ))
        }
    }
}
