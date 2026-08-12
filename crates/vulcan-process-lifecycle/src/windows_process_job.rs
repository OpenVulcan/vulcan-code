//! Windows Job Object ownership for non-terminal service children.
//! 非终端服务子进程使用的 Windows Job Object 所有权实现。

use std::{
    io,
    mem::{size_of, zeroed},
    os::windows::io::AsRawHandle,
    process::Child,
    ptr,
};
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
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
        // SAFETY: Null security attributes and name request a private unnamed Job Object.
        // 安全性：空安全属性和名称用于请求一个私有匿名 Job Object。
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }

        // SAFETY: The all-zero value is the documented initialization for this POD Win32 type.
        // 安全性：全零值是该 POD Win32 类型的文档化初始化方式。
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: The handle is live, the structure remains valid for the call, and the
        // information class matches the supplied structure.
        // 安全性：句柄有效、结构在调用期间保持有效，且信息类别与传入结构匹配。
        let configured = unsafe {
            SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                (&raw const limits).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };
        if configured == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: This function uniquely owns the live handle on this error path.
            // 安全性：此错误路径中该函数唯一持有这个有效句柄。
            unsafe {
                CloseHandle(handle);
            }
            return Err(error);
        }

        // SAFETY: `Child` owns a live process handle and the Job Object remains alive in the
        // returned guard after assignment succeeds.
        // 安全性：`Child` 持有有效进程句柄，绑定成功后 Job Object 由返回的守卫继续持有。
        let assigned = unsafe { AssignProcessToJobObject(handle, child.as_raw_handle().cast()) };
        if assigned == 0 {
            let error = io::Error::last_os_error();
            // SAFETY: This function uniquely owns the live handle on this error path.
            // 安全性：此错误路径中该函数唯一持有这个有效句柄。
            unsafe {
                CloseHandle(handle);
            }
            return Err(error);
        }

        Ok(Self {
            handle: handle as usize,
        })
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
