#[cfg(windows)]
use std::mem::size_of;
#[cfg(windows)]
use std::process::{Command, Stdio};

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
    let _ = Command::new("taskkill.exe")
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
