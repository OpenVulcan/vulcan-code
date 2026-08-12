//! Platform-specific ownership guard for one terminal process tree.
//! 单个终端进程树的平台专用所有权守卫。

#[cfg(not(windows))]
use std::io;

/// Owns the operating-system resource that isolates one terminal process tree.
/// 持有用于隔离单个终端进程树的操作系统资源。
#[cfg(unix)]
pub struct TerminalProcessJob {
    /// Process group created by portable-pty for the terminal child session.
    /// portable-pty 为终端子会话创建的进程组。
    process_group_id: i32,
}

#[cfg(unix)]
impl TerminalProcessJob {
    /// Accepts the portable-pty process identifier on Unix-like systems.
    /// 在类 Unix 系统上接收 portable-pty 进程标识符。
    ///
    /// `portable-pty` creates a dedicated session whose leader pid is also its process-group id.
    /// The guard kills that group on drop so descendants cannot keep the PTY or application alive.
    /// `portable-pty` 会创建独立会话，其领导进程 pid 同时也是进程组 id；守卫在丢弃时终止该组，避免后代进程继续占用 PTY 或应用生命周期。
    ///
    /// # Parameters / 参数
    /// - `_process_id`: Optional terminal child process identifier.
    /// - `_process_id`：可选的终端子进程标识符。
    ///
    /// # Returns / 返回值
    /// Returns the lifetime marker for the isolated terminal session.
    /// 返回隔离终端会话的生命周期标记。
    pub fn attach(process_id: Option<u32>) -> io::Result<Self> {
        let process_id = process_id.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "portable-pty did not expose the Unix terminal process identifier",
            )
        })?;
        let process_group_id = i32::try_from(process_id).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "portable-pty returned a Unix terminal process identifier outside i32 range",
            )
        })?;
        Ok(Self { process_group_id })
    }

    /// Terminates the isolated Unix process group immediately.
    /// 立即终止隔离的 Unix 进程组。
    ///
    /// # Returns / 返回值
    /// Returns success after SIGKILL is accepted or when the group has already exited.
    /// SIGKILL 被接受或进程组已经退出后返回成功。
    pub fn terminate(&self) -> io::Result<()> {
        // SAFETY: portable-pty creates a dedicated session/process group with the child pid;
        // a negative pid targets only that exact process group.
        // 安全性：portable-pty 使用子进程 pid 创建独立会话/进程组；负 pid 只定位该精确进程组。
        let result = unsafe { libc::kill(-self.process_group_id, libc::SIGKILL) };
        if result == 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            return Ok(());
        }
        Err(error)
    }
}

#[cfg(unix)]
impl Drop for TerminalProcessJob {
    /// Terminates the isolated Unix process group, including surviving descendants.
    /// 终止隔离的 Unix 进程组，包括仍存活的后代进程。
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

/// Owns no extra process-tree resource on unsupported non-Unix, non-Windows targets.
/// 在不支持的非 Unix、非 Windows 目标上不持有额外进程树资源。
#[cfg(all(not(unix), not(windows)))]
pub struct TerminalProcessJob;

#[cfg(all(not(unix), not(windows)))]
impl TerminalProcessJob {
    /// Returns an explicit unsupported-platform error instead of pretending isolation exists.
    /// 返回明确的不支持平台错误，而不是假装已存在隔离能力。
    pub fn attach(_process_id: Option<u32>) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "terminal process-tree isolation is unsupported on this platform",
        ))
    }

    /// Reports unsupported process-tree termination on this platform.
    /// 报告当前平台不支持进程树终止。
    pub fn terminate(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "terminal process-tree termination is unsupported on this platform",
        ))
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::mem::{size_of, zeroed};
    use std::ptr;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA, PROCESS_TERMINATE,
    };

    /// Owns a kill-on-close Windows Job Object assigned to one terminal process tree.
    /// 持有绑定到单个终端进程树的关闭即终止 Windows Job Object。
    pub struct TerminalProcessJob {
        /// Contains the uniquely owned native Job Object handle.
        /// 包含唯一持有的原生 Job Object 句柄。
        handle: HANDLE,
    }

    impl TerminalProcessJob {
        /// Creates a kill-on-close Job Object and assigns the exact terminal child process.
        /// 创建关闭即终止的 Job Object，并绑定精确的终端子进程。
        ///
        /// # Parameters / 参数
        /// - `process_id`: Process identifier returned by `portable-pty`.
        /// - `process_id`：由 `portable-pty` 返回的进程标识符。
        ///
        /// # Returns / 返回值
        /// Returns an owning guard, or the exact Windows error when setup fails.
        /// 返回所有权守卫；设置失败时返回精确的 Windows 错误。
        pub fn attach(process_id: Option<u32>) -> io::Result<Self> {
            let process_id = process_id.ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "portable-pty did not expose the Windows terminal process identifier",
                )
            })?;
            // SAFETY: Null security attributes and name request a private unnamed Job Object.
            // 安全性：空安全属性与空名称用于请求一个私有匿名 Job Object。
            let job_handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
            if job_handle.is_null() {
                return Err(io::Error::last_os_error());
            }

            // SAFETY: The all-zero value is the documented initialization for this POD type.
            // 安全性：全零值是该 POD 类型有文档支持的初始化方式。
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            // SAFETY: The handle is live and the information class matches the supplied structure.
            // 安全性：句柄有效，并且信息类别与传入结构匹配。
            let configured = unsafe {
                SetInformationJobObject(
                    job_handle,
                    JobObjectExtendedLimitInformation,
                    (&raw const limits).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            };
            if configured == 0 {
                return Err(close_job_after_error(job_handle));
            }

            // SAFETY: The requested rights are limited to Job Object assignment and lifecycle cleanup.
            // 安全性：请求的权限仅用于 Job Object 绑定与生命周期清理。
            let process_handle = unsafe {
                OpenProcess(
                    PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
                    0,
                    process_id,
                )
            };
            if process_handle.is_null() {
                return Err(close_job_after_error(job_handle));
            }
            // SAFETY: Both handles are live for the duration of this call.
            // 安全性：两个句柄在此次调用期间均保持有效。
            let assigned = unsafe { AssignProcessToJobObject(job_handle, process_handle) };
            // SAFETY: The process handle is uniquely owned by this function and no longer needed.
            // 安全性：进程句柄由此函数唯一持有，并且调用后不再需要。
            unsafe { CloseHandle(process_handle) };
            if assigned == 0 {
                return Err(close_job_after_error(job_handle));
            }
            Ok(Self { handle: job_handle })
        }

        /// Terminates every process assigned to this isolated Job Object.
        /// 终止分配到当前隔离 Job Object 的全部进程。
        ///
        /// # Returns / 返回值
        /// Returns success after Windows accepts complete process-tree termination.
        /// Windows 接受完整进程树终止后返回成功。
        pub fn terminate(&self) -> io::Result<()> {
            // SAFETY: The guard uniquely owns a live Job Object handle.
            // 安全性：守卫唯一持有有效的 Job Object 句柄。
            let terminated = unsafe { TerminateJobObject(self.handle, 1) };
            if terminated == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    impl Drop for TerminalProcessJob {
        /// Closes the Job Object and thereby terminates any surviving assigned descendants.
        /// 关闭 Job Object，从而终止仍存活的已绑定后代进程。
        fn drop(&mut self) {
            let _ = self.terminate();
            // SAFETY: This guard uniquely owns the handle and closes it exactly once after termination.
            // 安全性：此守卫在终止进程树后唯一持有句柄，并且只关闭一次。
            unsafe { CloseHandle(self.handle) };
        }
    }

    /// Preserves the current Windows error while closing a partially initialized Job Object.
    /// 在关闭部分初始化的 Job Object 时保留当前 Windows 错误。
    ///
    /// # Parameters / 参数
    /// - `job_handle`: Uniquely owned Job Object handle that must be closed.
    /// - `job_handle`：必须关闭的唯一持有 Job Object 句柄。
    ///
    /// # Returns / 返回值
    /// Returns the operating-system error captured before cleanup.
    /// 返回清理前捕获的操作系统错误。
    fn close_job_after_error(job_handle: HANDLE) -> io::Error {
        let error = io::Error::last_os_error();
        // SAFETY: The handle is uniquely owned on every caller's error path.
        // 安全性：在每个调用方错误路径中，该句柄均由当前函数唯一持有。
        unsafe { CloseHandle(job_handle) };
        error
    }
}

#[cfg(windows)]
pub use windows::TerminalProcessJob;
