//! Shared child-process creation helpers for host-owned tools.
//! 宿主自有工具复用的子进程创建辅助能力。

use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

/// Windows process creation flag that prevents a child console window from being shown.
/// 防止子进程控制台窗口显示的 Windows 进程创建标志。
pub const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Hides one Tokio child process window only in Windows release builds.
/// 仅在 Windows release 构建中隐藏单个 Tokio 子进程窗口。
///
/// # Parameters
/// 参数
/// `command` is the Tokio command being prepared before spawn.
/// `command` 是启动前正在配置的 Tokio 命令。
pub fn apply_windows_release_no_window(command: &mut TokioCommand) {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    #[cfg(any(not(windows), debug_assertions))]
    {
        let _ = command;
    }
}

/// Hides one standard child process window only in Windows release builds.
/// 仅在 Windows release 构建中隐藏单个标准库子进程窗口。
///
/// # Parameters
/// 参数
/// `command` is the standard command being prepared before spawn.
/// `command` 是启动前正在配置的标准库命令。
pub fn apply_windows_release_no_window_to_std_command(command: &mut StdCommand) {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    #[cfg(any(not(windows), debug_assertions))]
    {
        let _ = command;
    }
}
