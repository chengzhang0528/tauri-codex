use std::ffi::OsStr;
#[cfg(windows)]
use std::mem::size_of;
use std::process::Command;
#[cfg(windows)]
use std::process::Stdio;

#[cfg(windows)]
use windows::Win32::Foundation::{CloseHandle, HANDLE};
#[cfg(windows)]
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE};

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn background_command<S: AsRef<OsStr>>(program: S) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(windows)]
pub struct JobObject(HANDLE);

#[cfg(windows)]
unsafe impl Send for JobObject {}

#[cfg(windows)]
unsafe impl Sync for JobObject {}

#[cfg(windows)]
impl JobObject {
    pub fn attach(pid: u32) -> Result<Self, String> {
        unsafe {
            let job = CreateJobObjectW(None, None).map_err(|error| error.to_string())?;
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const _,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .map_err(|error| error.to_string())?;
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)
                .map_err(|error| error.to_string())?;
            let assigned = AssignProcessToJobObject(job, process).is_ok();
            let _ = CloseHandle(process);
            if !assigned {
                let _ = CloseHandle(job);
                return Err("无法将 Codex 进程加入 Job Object".to_string());
            }
            Ok(Self(job))
        }
    }

    pub fn terminate(&self) -> Result<(), String> {
        unsafe { TerminateJobObject(self.0, 1).map_err(|error| error.to_string()) }
    }
}

#[cfg(windows)]
pub fn terminate_process_tree(pid: u32) {
    let _ = background_command("taskkill.exe")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(windows)]
impl Drop for JobObject {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

#[cfg(not(windows))]
pub struct JobObject;

#[cfg(not(windows))]
impl JobObject {
    pub fn attach(_pid: u32) -> Result<Self, String> {
        Ok(Self)
    }
    pub fn terminate(&self) -> Result<(), String> {
        Err("Job Object 只在 Windows 上可用".to_string())
    }
}

#[cfg(not(windows))]
pub fn terminate_process_tree(_pid: u32) {}

#[cfg(all(test, windows))]
mod tests {
    use super::background_command;

    #[test]
    fn background_commands_still_capture_output() {
        let output = background_command("cmd.exe")
            .args(["/D", "/C", "echo", "background-output"])
            .output()
            .expect("run background command");

        assert!(output.status.success());
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            "background-output"
        );
    }

    #[test]
    fn background_commands_do_not_receive_a_console_window() {
        let status = background_command("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "Add-Type 'using System; using System.Runtime.InteropServices; public static class ConsoleWindow { [DllImport(\"kernel32.dll\")] public static extern IntPtr GetConsoleWindow(); }'; if ([ConsoleWindow]::GetConsoleWindow() -ne [IntPtr]::Zero) { exit 1 }",
            ])
            .status()
            .expect("inspect background console window");

        assert!(status.success());
    }
}
