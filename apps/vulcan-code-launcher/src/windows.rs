//! The few Windows calls the launcher needs; the only unsafe code in this crate.
//! 启动器所需的少量 Windows 调用；本 crate 中唯一的 unsafe 代码。
#![allow(unsafe_code)]

use windows_sys::Win32::System::Console::{CTRL_BREAK_EVENT, CTRL_C_EVENT, SetConsoleCtrlHandler};
use windows_sys::Win32::System::Environment::GetCommandLineW;
use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
use windows_sys::core::BOOL;

/// Returns this process's raw command line as UTF-16 without the terminator.
/// 以不含终止符的 UTF-16 返回本进程的原始命令行。
pub(crate) fn command_line() -> Vec<u16> {
    // SAFETY: GetCommandLineW returns a pointer to a NUL-terminated string that lives as long as
    // the process; it is read up to the terminator and copied.
    // SAFETY：GetCommandLineW 返回在进程生命周期内有效、以 NUL 结尾的字符串指针；读取到终止符为止并复制。
    unsafe {
        let line = GetCommandLineW();
        if line.is_null() {
            return Vec::new();
        }
        let mut length = 0;
        while *line.add(length) != 0 {
            length += 1;
        }
        std::slice::from_raw_parts(line, length).to_vec()
    }
}

/// Swallows Ctrl+C and Ctrl+Break for this process only; other console events keep their default.
/// 只为本进程吞掉 Ctrl+C 与 Ctrl+Break；其他控制台事件保持默认处理。
unsafe extern "system" fn ignore_interrupt(event: u32) -> BOOL {
    BOOL::from(event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT)
}

/// Keeps the forwarder alive when the console sends Ctrl+C or Ctrl+Break to every attached process.
/// 控制台向所有附加进程发送 Ctrl+C 或 Ctrl+Break 时保持转发器存活。
///
/// A handler routine is used instead of `SetConsoleCtrlHandler(NULL, TRUE)`, because that flag is
/// inherited and would stop the child from seeing Ctrl+C as well.
/// 使用处理函数而不是 `SetConsoleCtrlHandler(NULL, TRUE)`，因为后者的标志会被子进程继承，
/// 使子进程也收不到 Ctrl+C。
pub(crate) fn ignore_console_interrupts() {
    // SAFETY: the handler is a plain function that touches no state and stays valid for the process lifetime.
    // SAFETY：处理函数是不访问任何状态的普通函数，在进程生命周期内始终有效。
    unsafe {
        SetConsoleCtrlHandler(Some(ignore_interrupt), 1);
    }
}

/// Shows an error message box, the only channel a GUI-subsystem program has.
/// 显示错误消息框，这是 GUI 子系统程序唯一的输出途径。
pub(crate) fn show_error(message: &str) {
    let text: Vec<u16> = message.encode_utf16().chain([0]).collect();
    let caption: Vec<u16> = "Vulcan Code".encode_utf16().chain([0]).collect();
    // SAFETY: both strings are NUL-terminated and outlive the modal call; no owner window is passed.
    // SAFETY：两个字符串都以 NUL 结尾且在模态调用期间有效；不传入所有者窗口。
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}
