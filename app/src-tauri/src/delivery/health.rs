use crate::{job, paths};
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const DOCTOR_TIMEOUT: Duration = Duration::from_secs(30);
const CODEX_VENDOR_ROOT: &str =
    "node_modules/@openai/codex-win32-x64/vendor/x86_64-pc-windows-msvc";
const CODEX_PACKAGE_EXECUTABLES: &[(&str, bool)] = &[
    ("bin/codex-code-mode-host.exe", true),
    ("bin/codex.exe", true),
    ("codex-path/rg.exe", false),
    ("codex-resources/codex-command-runner.exe", true),
    ("codex-resources/codex-windows-sandbox-setup.exe", true),
];

pub fn doctor_system_node(minimum: &str) -> Result<std::path::PathBuf, String> {
    let mut failures = Vec::new();
    for node in paths::system_node_candidates() {
        if let Err(error) = verify_authenticode(&node) {
            failures.push(error);
            continue;
        }
        match crate::runtime::check_system_node_candidate_at_least(&node, minimum) {
            Ok((node, _)) => return Ok(node),
            Err(error) => failures.push(format!("{}：{error}", node.display())),
        }
    }
    Err(if failures.is_empty() {
        format!("未找到满足版本 {minimum} 且通过 Authenticode 的系统 Node.js/npm")
    } else {
        format!(
            "没有合格的系统 Node.js/npm（要求 {minimum}）：{}",
            failures.join("；")
        )
    })
}

pub fn doctor_manager(root: &Path, system_node: &Path) -> Result<(), String> {
    require_nonempty(&root.join("tauri-codex-manager.exe"), "Manager 入口")?;
    require_nonempty(
        &root.join("WebView2Loader.dll"),
        "Manager WebView2 运行时依赖",
    )?;
    verify_authenticode(&root.join("WebView2Loader.dll"))?;
    let mut command = job::background_command(root.join("tauri-codex-manager.exe"));
    command
        .arg("--runtime-check")
        .env("TAURI_CODEX_SYSTEM_NODE", system_node);
    run_success(&mut command, DOCTOR_TIMEOUT, "Manager doctor")
}

pub fn doctor_codex(root: &Path, system_node: &Path) -> Result<(), String> {
    let entry = codex_entry(root)?;
    verify_codex_executable_provenance(root)?;
    let rg = root.join(CODEX_VENDOR_ROOT).join("codex-path/rg.exe");
    run_success(
        job::background_command(rg).arg("--version"),
        DOCTOR_TIMEOUT,
        "Codex rg doctor",
    )?;
    let smoke_home = root.join(format!(".doctor-home-{}", uuid::Uuid::new_v4().simple()));
    fs::create_dir_all(&smoke_home).map_err(|error| error.to_string())?;
    let result = run_success(
        job::background_command(system_node)
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

fn codex_signed_executables(root: &Path) -> Result<Vec<std::path::PathBuf>, String> {
    let mut pending = vec![root.to_path_buf()];
    let mut actual = Vec::new();
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
                actual.push(entry.path());
            }
        }
    }
    actual.sort();
    let mut expected = CODEX_PACKAGE_EXECUTABLES
        .iter()
        .map(|(relative, _)| root.join(CODEX_VENDOR_ROOT).join(relative))
        .collect::<Vec<_>>();
    expected.sort();
    if actual != expected {
        let relative = |path: &Path| {
            path.strip_prefix(root)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/")
        };
        return Err(format!(
            "Codex Windows executable 闭包不匹配，expected=[{}] actual=[{}]",
            expected
                .iter()
                .map(|path| relative(path))
                .collect::<Vec<_>>()
                .join(", "),
            actual
                .iter()
                .map(|path| relative(path))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for path in &expected {
        require_nonempty(path, "Codex Windows executable")?;
    }
    Ok(CODEX_PACKAGE_EXECUTABLES
        .iter()
        .filter(|(_, requires_authenticode)| *requires_authenticode)
        .map(|(relative, _)| root.join(CODEX_VENDOR_ROOT).join(relative))
        .collect())
}

pub fn verify_codex_executable_provenance(root: &Path) -> Result<(), String> {
    for executable in codex_signed_executables(root)? {
        verify_authenticode(&executable)?;
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

#[cfg(test)]
mod tests {
    use super::{codex_signed_executables, CODEX_PACKAGE_EXECUTABLES, CODEX_VENDOR_ROOT};
    use std::fs;

    #[test]
    fn codex_executable_inventory_is_exact_and_keeps_unsigned_rg_out_of_signature_checks() {
        let root = std::env::temp_dir().join(format!(
            "tauri-codex-health-{}",
            uuid::Uuid::new_v4().simple()
        ));
        for (relative, _) in CODEX_PACKAGE_EXECUTABLES {
            let path = root.join(CODEX_VENDOR_ROOT).join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, b"fixture").unwrap();
        }

        let signed = codex_signed_executables(&root).unwrap();
        assert_eq!(signed.len(), 4);
        assert!(signed.iter().all(|path| path.is_file()));
        assert!(signed.iter().all(|path| !path.ends_with("rg.exe")));

        let unexpected = root.join(CODEX_VENDOR_ROOT).join("bin/unexpected.dll");
        fs::write(&unexpected, b"fixture").unwrap();
        assert!(codex_signed_executables(&root)
            .unwrap_err()
            .contains("闭包不匹配"));
        fs::remove_file(unexpected).unwrap();

        let rg = root.join(CODEX_VENDOR_ROOT).join("codex-path/rg.exe");
        fs::remove_file(rg).unwrap();
        assert!(codex_signed_executables(&root)
            .unwrap_err()
            .contains("闭包不匹配"));
        fs::remove_dir_all(root).unwrap();
    }
}
