//! Target process, console attachment, and lifecycle helpers.
//! 目标进程、控制台附着与生命周期辅助。

use std::io;
use std::mem::zeroed;
use std::ptr::{null, null_mut};
use std::thread;
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows_sys::Win32::System::Console::{AttachConsole, FreeConsole};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, CreateProcessW, OpenProcess,
    PROCESS_INFORMATION, PROCESS_TERMINATE, STARTF_USESHOWWINDOW, STARTUPINFOW, TerminateProcess,
    WaitForSingleObject,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::screen::resize_console;
use super::util::{last_os_error, wide_null, windows_command_line};

/// Target process and attached console handles owned by the helper.
/// helper 持有的目标进程与已附着控制台句柄。
pub(super) struct ConsoleSession {
    /// Target process handle.
    /// 目标进程句柄。
    pub(super) process: HANDLE,
    /// Target thread handle.
    /// 目标线程句柄。
    pub(super) thread: HANDLE,
    /// Console input handle.
    /// 控制台输入句柄。
    pub(super) input: HANDLE,
    /// Console output handle.
    /// 控制台输出句柄。
    pub(super) output: HANDLE,
    /// Target process identifier.
    /// 目标进程标识。
    pub(super) pid: u32,
    /// Whether stop was requested.
    /// 是否已请求停止。
    pub(super) stopped: bool,
}

impl Drop for ConsoleSession {
    /// Releases target process and console handles.
    /// 释放目标进程与控制台句柄。
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.input);
            CloseHandle(self.output);
            CloseHandle(self.thread);
            CloseHandle(self.process);
            FreeConsole();
        }
    }
}
/// Starts the target process in a new Windows console and opens console handles.
/// 在新的 Windows 控制台中启动目标进程并打开控制台句柄。
pub(super) fn start_console_session(
    program: &str,
    args: &[String],
    cwd: &str,
    rows: u16,
    cols: u16,
) -> Result<ConsoleSession, String> {
    let mut command_line = wide_null(&windows_command_line(program, args));
    let mut cwd_wide = wide_null(cwd);
    let mut startup_info = unsafe { zeroed::<STARTUPINFOW>() };
    startup_info.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup_info.dwFlags = STARTF_USESHOWWINDOW;
    startup_info.wShowWindow = SW_HIDE as u16;
    let mut process_info = unsafe { zeroed::<PROCESS_INFORMATION>() };
    let created = unsafe {
        CreateProcessW(
            null(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            0,
            CREATE_NEW_CONSOLE | CREATE_UNICODE_ENVIRONMENT,
            null(),
            cwd_wide.as_mut_ptr(),
            &startup_info,
            &mut process_info,
        )
    };
    if created == 0 {
        return Err(last_os_error("CreateProcessW failed"));
    }
    if let Err(error) = attach_console_with_retry(process_info.dwProcessId) {
        cleanup_failed_console_start(&process_info, None, None);
        return Err(error);
    }
    let input = match open_console_handle("CONIN$") {
        Ok(input) => input,
        Err(error) => {
            cleanup_failed_console_start(&process_info, None, None);
            return Err(error);
        }
    };
    let output = match open_console_handle("CONOUT$") {
        Ok(output) => output,
        Err(error) => {
            cleanup_failed_console_start(&process_info, Some(input), None);
            return Err(error);
        }
    };
    if let Err(error) = resize_console(output, rows, cols) {
        cleanup_failed_console_start(&process_info, Some(input), Some(output));
        return Err(error);
    }
    Ok(ConsoleSession {
        process: process_info.hProcess,
        thread: process_info.hThread,
        input,
        output,
        pid: process_info.dwProcessId,
        stopped: false,
    })
}

/// Cleans up a target process when console attachment or handle initialization fails.
/// 当控制台附着或句柄初始化失败时清理目标进程。
fn cleanup_failed_console_start(
    process_info: &PROCESS_INFORMATION,
    input: Option<HANDLE>,
    output: Option<HANDLE>,
) {
    unsafe {
        if let Some(input) = input {
            CloseHandle(input);
        }
        if let Some(output) = output {
            CloseHandle(output);
        }
    }
    let _ = terminate_target_process_tree(process_info.dwProcessId, process_info.hProcess, 1);
    unsafe {
        CloseHandle(process_info.hThread);
        CloseHandle(process_info.hProcess);
        FreeConsole();
    }
}

/// Terminates descendants of one target process before terminating the root process handle.
/// 先终止目标进程的后代进程，再终止根进程句柄。
pub(super) fn terminate_target_process_tree(
    root_pid: u32,
    root_process: HANDLE,
    exit_code: u32,
) -> Result<(), String> {
    let mut cleanup_diagnostics = Vec::new();
    match collect_descendant_process_ids(root_pid) {
        Ok(descendants) => {
            for pid in descendants.into_iter().rev() {
                if let Err(error) = terminate_process_by_pid(pid, exit_code) {
                    cleanup_diagnostics.push(error);
                }
            }
        }
        Err(error) => cleanup_diagnostics.push(error),
    }
    // Root may exit naturally while the stop request is waiting in the helper queue.
    // 当停止请求在 helper 队列中等待时，根进程可能已经自然退出。
    if unsafe { WaitForSingleObject(root_process, 0) } == WAIT_OBJECT_0 {
        return Ok(());
    }
    let root_terminated = unsafe { TerminateProcess(root_process, exit_code) } != 0;
    if !root_terminated {
        let mut error = last_os_error(&format!("TerminateProcess({root_pid}) failed"));
        if !cleanup_diagnostics.is_empty() {
            error.push_str("; child cleanup diagnostics: ");
            error.push_str(&cleanup_diagnostics.join("; "));
        }
        return Err(error);
    }
    Ok(())
}

/// Collects descendant process ids for one root process from a ToolHelp snapshot.
/// 从 ToolHelp 快照中收集一个根进程的后代进程标识。
fn collect_descendant_process_ids(root_pid: u32) -> Result<Vec<u32>, String> {
    let entries = process_entries_snapshot()?;
    let mut descendants = Vec::new();
    let mut stack = vec![root_pid];
    while let Some(parent_pid) = stack.pop() {
        for entry in &entries {
            if entry.parent_pid == parent_pid && entry.pid != root_pid {
                descendants.push(entry.pid);
                stack.push(entry.pid);
            }
        }
    }
    Ok(descendants)
}

/// One compact process entry captured from a ToolHelp process snapshot.
/// 从 ToolHelp 进程快照中捕获的一条紧凑进程记录。
#[derive(Debug, Clone, Copy)]
struct ProcessSnapshotEntry {
    /// Process identifier.
    /// 进程标识。
    pub(super) pid: u32,
    /// Parent process identifier.
    /// 父进程标识。
    parent_pid: u32,
}

/// Returns all process id and parent id pairs from one ToolHelp snapshot.
/// 从 ToolHelp 快照返回全部进程标识与父进程标识对。
fn process_entries_snapshot() -> Result<Vec<ProcessSnapshotEntry>, String> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(last_os_error("CreateToolhelp32Snapshot failed"));
    }
    let mut entries = Vec::new();
    let mut entry = unsafe { zeroed::<PROCESSENTRY32W>() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut ok = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while ok {
        entries.push(ProcessSnapshotEntry {
            pid: entry.th32ProcessID,
            parent_pid: entry.th32ParentProcessID,
        });
        ok = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe {
        CloseHandle(snapshot);
    }
    Ok(entries)
}

/// Terminates one process by id when the helper can open it with terminate permission.
/// 当 helper 可用终止权限打开进程时按进程标识终止该进程。
fn terminate_process_by_pid(pid: u32, exit_code: u32) -> Result<(), String> {
    let handle = unsafe { OpenProcess(PROCESS_TERMINATE, 0, pid) };
    if handle.is_null() {
        return Err(last_os_error(&format!("OpenProcess({pid}) failed")));
    }
    let terminated = unsafe { TerminateProcess(handle, exit_code) } != 0;
    unsafe {
        CloseHandle(handle);
    }
    if terminated {
        Ok(())
    } else {
        Err(last_os_error(&format!("TerminateProcess({pid}) failed")))
    }
}

/// Attaches the helper process to the target console, retrying while the console initializes.
/// 将 helper 进程附着到目标控制台，并在控制台初始化期间重试。
fn attach_console_with_retry(process_id: u32) -> Result<(), String> {
    unsafe {
        FreeConsole();
    }
    let mut last_error = None;
    for _ in 0..20 {
        if unsafe { AttachConsole(process_id) } != 0 {
            return Ok(());
        }
        last_error = Some(io::Error::last_os_error());
        thread::sleep(Duration::from_millis(50));
    }
    Err(format!(
        "AttachConsole({process_id}) failed: {}",
        last_error.unwrap_or_else(io::Error::last_os_error)
    ))
}

/// Opens one console pseudo-file handle.
/// 打开一个控制台伪文件句柄。
fn open_console_handle(name: &str) -> Result<HANDLE, String> {
    let mut wide = wide_null(name);
    let handle = unsafe {
        CreateFileW(
            wide.as_mut_ptr(),
            0x8000_0000u32 | 0x4000_0000u32,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null(),
            OPEN_EXISTING,
            0,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        Err(last_os_error(&format!("CreateFileW({name}) failed")))
    } else {
        Ok(handle)
    }
}
