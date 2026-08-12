//! Platform process-lifecycle guards shared by Vulcan service runtimes.
//! Vulcan 服务运行时共享的平台进程生命周期守卫。

#[cfg(windows)]
mod windows_process_job;

#[cfg(windows)]
pub use windows_process_job::WindowsKillOnCloseJob;
