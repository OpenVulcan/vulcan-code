//! Audited Windows Debug console attachment for a GUI-launched Core.
//! GUI 启动 Core 时经过审计的 Windows Debug 控制台挂载。

#![allow(unsafe_code)]

use windows::Win32::Foundation::GENERIC_WRITE;
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Console::{
    AllocConsole, GetConsoleWindow, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE, SetConsoleOutputCP,
    SetConsoleTitleW, SetStdHandle,
};
use windows::core::w;

/// Attaches one visible console to a GUI-launched Debug Core and routes Rust diagnostics to it.
/// 给 GUI 启动的 Debug Core 挂载一个可见控制台，并将 Rust 诊断输出送入其中。
///
/// # Returns
/// 返回值
/// Returns an error when the OS cannot allocate or connect console output; no second console is attempted.
/// 系统无法分配或连接控制台输出时返回错误；不会尝试创建第二个控制台。
pub fn ensure_gui_debug_console() -> Result<(), String> {
    // A GUI launcher can pass NUL for stdout/stderr even when Windows has made a console window.
    // 即使 Windows 已创建控制台窗口，GUI 启动器仍可能把 stdout/stderr 指向 NUL。
    let needs_console = unsafe { GetConsoleWindow().0.is_null() };
    if needs_console {
        unsafe { AllocConsole() }.map_err(|error| format!("AllocConsole failed: {error}"))?;
        unsafe { SetConsoleTitleW(w!("Vulcan Code Debug Core")) }
            .map_err(|error| format!("SetConsoleTitleW failed: {error}"))?;
    }
    // output is a dedicated console handle because the GUI launcher deliberately detached stdio.
    // output 是专用控制台句柄，因为 GUI 启动器有意分离了标准输入输出。
    let output = unsafe {
        CreateFileW(
            w!("CONOUT$"),
            GENERIC_WRITE.0,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|error| format!("opening CONOUT$ failed: {error}"))?;
    // The process owns this handle until exit; both standard streams share the same console.
    // 该句柄由进程持有至退出；两个标准输出流共享同一个控制台。
    unsafe { SetStdHandle(STD_OUTPUT_HANDLE, output) }
        .map_err(|error| format!("routing stdout to console failed: {error}"))?;
    unsafe { SetStdHandle(STD_ERROR_HANDLE, output) }
        .map_err(|error| format!("routing stderr to console failed: {error}"))?;
    unsafe { SetConsoleOutputCP(65001) }
        .map_err(|error| format!("setting UTF-8 console output failed: {error}"))?;
    Ok(())
}
