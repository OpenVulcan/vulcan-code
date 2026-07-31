//! Safe Windows process-lifetime primitives for Vulcan hosts.
//! Vulcan 宿主使用的安全 Windows 进程生命周期原语。

#[cfg(windows)]
mod process_job;

#[cfg(windows)]
pub use process_job::WindowsKillOnCloseJob;
