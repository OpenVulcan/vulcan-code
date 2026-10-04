//! Windows Job Object ownership for non-terminal service children.
//! 非终端服务子进程使用的 Windows Job Object 所有权实现。

use std::{io, mem::size_of, os::windows::io::AsRawHandle, process::Child, ptr};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
        QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
    },
};

/// Owns a Windows Job Object that terminates assigned service children when the guard is dropped.
/// 持有一个在守卫释放时终止已绑定服务子进程的 Windows Job Object。
#[derive(Debug)]
pub struct WindowsKillOnCloseJob {
    /// Native Job Object handle retained as an integer to keep the guard Send-safe.
    /// 以整数形式保留的原生 Job Object 句柄，用于保持守卫可安全跨线程传递。
    handle: usize,
}

impl WindowsKillOnCloseJob {
    /// Creates a private non-inheritable job before any process is launched.
    /// 在任何进程启动前创建私有不可继承 Job。
    /// No inputs; returns a unique kill-on-close owner or the exact Windows error.
    /// 无输入；返回唯一关闭即终止所有者，或精确 Windows 错误。
    pub fn create() -> io::Result<Self> {
        // SAFETY: Null attributes and name create a private non-inheritable job.
        // 安全性：空属性及名称创建私有不可继承 Job。
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        // The guard is established immediately, including all configuration failure paths.
        // 立即建立守卫，覆盖全部配置失败路径。
        let job = Self {
            handle: handle as usize,
        };
        // The documented zero default leaves breakaway privileges disabled.
        // 文档规定的零默认值保持脱离权限关闭。
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: This unique live handle and matching fixed-size structure remain valid for the call.
        // 安全性：唯一有效句柄及匹配定长结构在调用期间保持有效。
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    /// Assigns the supplied process to this exact private job without releasing the guard on failure.
    /// 将给定进程绑定到此精确私有 Job，失败时不释放守卫。
    /// Child supplies a live owned process handle; callers requiring pre-execution containment must suspend it first.
    /// child 提供有效自有进程句柄；要求执行前纳管的调用方必须先挂起它。
    pub fn assign_child(&self, child: &Child) -> io::Result<()> {
        // SAFETY: Both handles are owned and remain live throughout this synchronous assignment.
        // 安全性：两个句柄均为自有，并在同步绑定期间保持有效。
        if unsafe { AssignProcessToJobObject(self.handle as HANDLE, child.as_raw_handle().cast()) }
            == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Reads the actual active process count, including descendants, from this exact job.
    /// 从此精确 Job 读取包含派生进程的实际活动进程数。
    /// No inputs; returns Windows accounting rather than a cached root-process status.
    /// 无输入；返回 Windows 记账，而非缓存的根进程状态。
    pub fn active_process_count(&self) -> io::Result<u32> {
        // Query uses the native fixed layout matching JobObjectBasicAccountingInformation.
        // 查询使用匹配 JobObjectBasicAccountingInformation 的原生定长布局。
        let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: The live job and writable structure are valid for the matching query class and length.
        // 安全性：有效 Job 及可写结构与查询类别、长度匹配。
        if unsafe {
            QueryInformationJobObject(
                self.handle as HANDLE,
                JobObjectBasicAccountingInformation,
                (&raw mut accounting).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(accounting.ActiveProcesses)
    }

    /// Requests termination of every process in this job without pretending it is an exit receipt.
    /// 请求终止此 Job 的全部进程，不将其视为退出回执。
    /// Exit code is supplied by the owner; callers must separately confirm active_process_count reaches zero.
    /// 退出码由所有者提供；调用方必须独立确认 active_process_count 归零。
    pub fn terminate(&self, exit_code: u32) -> io::Result<()> {
        // SAFETY: This guard uniquely owns the live job handle; no PID lookup or external handle is used.
        // 安全性：此守卫唯一拥有有效 Job 句柄；不查询 PID 或使用外部句柄。
        if unsafe { TerminateJobObject(self.handle as HANDLE, exit_code) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Creates a kill-on-close Job Object and assigns the supplied service child process.
    /// 创建关闭即终止的 Job Object，并绑定给定的服务子进程。
    ///
    /// # Parameters / 参数
    /// - `child`: Already spawned child whose lifetime must not outlive its service owner.
    /// - `child`：已启动且生命周期不得超过服务所有者的子进程。
    ///
    /// # Returns / 返回值
    /// Returns the owning guard, or the exact operating-system error from creation, configuration, or assignment.
    /// 返回持有守卫；创建、配置或绑定失败时返回精确操作系统错误。
    pub fn attach(child: &Child) -> io::Result<Self> {
        // The guard owns the job before the fallible assignment, preserving cleanup on every path.
        // 可能失败的绑定之前，守卫已拥有 Job，使每条路径均保留清理。
        let job = Self::create()?;
        job.assign_child(child)?;
        Ok(job)
    }
}

impl Drop for WindowsKillOnCloseJob {
    /// Closes the Job Object so Windows terminates any surviving assigned service children.
    /// 关闭 Job Object，使 Windows 终止仍存活的已绑定服务子进程。
    fn drop(&mut self) {
        // SAFETY: The handle is uniquely owned by this guard and is closed exactly once.
        // 安全性：句柄由此守卫唯一持有，并且只关闭一次。
        unsafe {
            CloseHandle(self.handle as HANDLE);
        }
    }
}
