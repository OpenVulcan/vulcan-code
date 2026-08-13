//! Shared child-process creation helpers for host-owned tools.
//! 宿主自有工具复用的子进程创建辅助能力。

use std::path::PathBuf;
use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

/// Windows process creation flag that prevents a child console window from being shown.
/// 防止子进程控制台窗口显示的 Windows 进程创建标志。
pub const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Hides one background Tokio child process window in every Windows build profile.
/// 在所有 Windows 构建配置中隐藏单个后台 Tokio 子进程窗口。
///
/// # Parameters
/// 参数
/// `command` is the non-interactive Tokio command being prepared before spawn.
/// `command` 是启动前正在配置的非交互 Tokio 命令。
pub fn apply_windows_no_window(command: &mut TokioCommand) {
    #[cfg(windows)]
    {
        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

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

/// Prepends host-resolved directories to one Tokio child process search path.
/// 把宿主解析出的目录前置到一个 Tokio 子进程搜索路径。
///
/// # Parameters
/// 参数
/// `command` is the child command being prepared; `path_prefixes` preserves precedence order.
/// `command` 是正在准备的子命令；`path_prefixes` 保持优先级顺序。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after applying PATH, or an error when the platform cannot encode the joined value.
/// 应用 PATH 后返回 `Ok(())`；平台无法编码拼接值时返回错误。
pub fn apply_child_path_prefixes(
    command: &mut TokioCommand,
    path_prefixes: &[PathBuf],
) -> Result<(), String> {
    if path_prefixes.is_empty() {
        return Ok(());
    }
    // paths starts with unique host-owned prefixes in caller-declared order.
    // paths 首先按调用方声明顺序保存唯一的宿主自有前缀。
    let mut paths = Vec::new();
    // pathPrefix iterates each caller-owned prefix before inherited PATH entries are appended.
    // pathPrefix 在追加继承 PATH 条目前遍历每个调用方持有的前缀。
    for path_prefix in path_prefixes {
        if !paths.contains(path_prefix) {
            paths.push(path_prefix.clone());
        }
    }
    // existingPath contains the parent search path inherited only by this child.
    // existingPath 保存仅由当前子进程继承的父进程搜索路径。
    if let Some(existing_path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&existing_path));
    }
    // joinedPath is the exact child-only PATH value and never mutates the parent environment.
    // joinedPath 是精确的子进程专属 PATH 值，绝不修改父进程环境。
    let joined_path = std::env::join_paths(paths)
        .map_err(|error| format!("Failed to construct the host-owned child PATH: {error}"))?;
    command.env("PATH", joined_path);
    Ok(())
}
