//! Suspended startup keeps plugin code from running before its exact process job is assigned.
//! 挂起启动使插件代码在绑定精确进程 Job 前不能运行。

use std::{
    io,
    mem::{offset_of, size_of},
    os::windows::{
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
        process::CommandExt,
    },
    process::{Child, Command},
};
use windows_sys::Win32::{
    Foundation::{ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        Threading::{
            CREATE_SUSPENDED, GetProcessIdOfThread, OpenThread, ResumeThread,
            THREAD_QUERY_LIMITED_INFORMATION, THREAD_SUSPEND_RESUME,
        },
    },
};

/// Adds suspended startup to the caller's explicit Windows creation flags.
/// 为调用方明确的 Windows 创建标志添加挂起启动。
/// Command retains its arguments and environment; flags must include every caller-required flag.
/// command 保留参数与环境；flags 必须包含调用方要求的全部标志。
/// Returns no value; the caller must assign the child to its job before resuming it.
/// 无返回值；调用方必须先将子进程绑定到 Job，再恢复它。
pub fn configure_suspended_child(command: &mut Command, flags: u32) {
    command.creation_flags(flags | CREATE_SUSPENDED);
}

/// Resumes the suspended threads belonging to the supplied live child process.
/// 恢复属于所给有效子进程的挂起线程。
/// Child must have been created suspended and already assigned to the caller's containment job.
/// child 必须已挂起创建，并已绑定到调用方的纳管 Job。
/// Returns an explicit error on enumeration, identity, open or resume failure; ownership stays with the caller.
/// 枚举、身份核对、打开或恢复失败均明确返回错误；所有权仍由调用方保留。
pub fn resume_suspended_child(child: &Child) -> io::Result<()> {
    // SAFETY: Thread snapshots take no borrowed process pointers and return an owned native handle.
    // 安全性：线程快照不借用进程指针，并返回自有原生句柄。
    let raw_snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if raw_snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: The successful snapshot is a unique valid handle closed exactly once by this owner.
    // 安全性：成功快照是唯一有效句柄，由此所有者仅关闭一次。
    let snapshot = unsafe { OwnedHandle::from_raw_handle(raw_snapshot.cast()) };
    // The native structure size is required by ToolHelp on each enumeration step.
    // ToolHelp 要求每个枚举步骤提供原生结构大小。
    let mut entry = THREADENTRY32 {
        dwSize: size_of::<THREADENTRY32>() as u32,
        ..Default::default()
    };
    // SAFETY: The owned snapshot and writable entry remain live for the synchronous call.
    // 安全性：自有快照及可写条目在同步调用期间保持有效。
    let mut found = unsafe { Thread32First(snapshot.as_raw_handle().cast(), &raw mut entry) };
    // At least one genuinely suspended child thread must be resumed for startup to succeed.
    // 启动成功至少需要恢复一个真正挂起的子进程线程。
    let mut resumed = false;
    loop {
        if found == 0 {
            // Capture the error before any handle destructor can overwrite the thread's last-error slot.
            // 在任何句柄析构可能覆盖线程最后错误槽之前捕获错误。
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_NO_MORE_FILES as i32) {
                break;
            }
            return Err(error);
        }
        if entry.dwSize < (offset_of!(THREADENTRY32, th32OwnerProcessID) + size_of::<u32>()) as u32
        {
            return Err(io::Error::other(
                "Thread snapshot omitted the process identity.",
            ));
        }
        if entry.th32OwnerProcessID == child.id() {
            // SAFETY: The snapshot supplies an OS thread ID; the opened handle is checked again below.
            // 安全性：快照提供操作系统线程 ID；下方再次核对打开的句柄。
            let raw_thread = unsafe {
                OpenThread(
                    THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
                    0,
                    entry.th32ThreadID,
                )
            };
            if raw_thread.is_null() {
                return Err(io::Error::last_os_error());
            }
            // SAFETY: OpenThread succeeded with a unique, non-inheritable owned handle.
            // 安全性：OpenThread 成功返回唯一且不可继承的自有句柄。
            let thread = unsafe { OwnedHandle::from_raw_handle(raw_thread.cast()) };
            // SAFETY: This handle has query rights and remains live through the identity check.
            // 安全性：此句柄有查询权限，并在身份核对期间保持有效。
            let owner_id = unsafe { GetProcessIdOfThread(thread.as_raw_handle().cast()) };
            if owner_id == 0 {
                return Err(io::Error::last_os_error());
            }
            if owner_id != child.id() {
                return Err(io::Error::other("Suspended child thread identity changed."));
            }
            // SAFETY: The checked thread belongs to the retained child and has suspend/resume rights.
            // 安全性：核对后的线程属于保留子进程，且句柄具备挂起及恢复权限。
            let previous_count = unsafe { ResumeThread(thread.as_raw_handle().cast()) };
            if previous_count == u32::MAX {
                return Err(io::Error::last_os_error());
            }
            if previous_count != 1 {
                return Err(io::Error::other(
                    "Child thread did not have the expected initial suspension count.",
                ));
            }
            resumed = true;
        }
        entry.dwSize = size_of::<THREADENTRY32>() as u32;
        // SAFETY: The same owned snapshot and correctly sized writable entry remain valid.
        // 安全性：相同自有快照及大小正确的可写条目保持有效。
        found = unsafe { Thread32Next(snapshot.as_raw_handle().cast(), &raw mut entry) };
    }
    if !resumed {
        return Err(io::Error::other("No suspended child thread was found."));
    }
    Ok(())
}
