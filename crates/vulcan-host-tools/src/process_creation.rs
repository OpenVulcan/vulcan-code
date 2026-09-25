//! Shared child-process creation helpers for host-owned tools.
//! 宿主自有工具复用的子进程创建辅助能力。

use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command as StdCommand;
use tokio::process::Command as TokioCommand;

/// Windows process creation flag that prevents a child console window from being shown.
/// 防止子进程控制台窗口显示的 Windows 进程创建标志。
pub const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Constructs one host-owned background command with the shared Windows no-window policy.
/// 使用共享 Windows 无窗口策略构造一个宿主自有后台命令。
///
/// # Parameters
/// 参数
/// `program` is the exact executable passed to the standard process runtime.
/// `program` 是传递给标准进程运行时的准确可执行文件。
///
/// # Returns
/// 返回值
/// Returns a standard command that is already safe to configure and spawn as a non-visible child.
/// 返回一个已可安全继续配置并作为不可见子进程启动的标准命令。
pub fn new_background_std_command<S>(program: S) -> StdCommand
where
    S: AsRef<OsStr>,
{
    // command receives the process-visibility contract at construction so callers cannot omit it.
    // command 在构造时即获得进程可见性契约，调用方无法遗漏该策略。
    let mut command = StdCommand::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    command
}

/// Constructs one Tokio background command from the shared standard-process factory.
/// 通过共享标准进程工厂构造一个 Tokio 后台命令。
///
/// # Parameters
/// 参数
/// `program` is the exact executable passed to the asynchronous process runtime.
/// `program` 是传递给异步进程运行时的准确可执行文件。
///
/// # Returns
/// 返回值
/// Returns a Tokio command carrying the same non-visible Windows creation policy.
/// 返回一个携带相同 Windows 不可见创建策略的 Tokio 命令。
pub fn new_background_tokio_command<S>(program: S) -> TokioCommand
where
    S: AsRef<OsStr>,
{
    TokioCommand::from(new_background_std_command(program))
}

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

#[cfg(test)]
mod tests {
    use super::WINDOWS_CREATE_NO_WINDOW;

    /// Verifies every shell-tool process entrypoint constructs children through the shared factory.
    /// 验证每个 Shell 工具进程入口都通过共享工厂构造子进程。
    #[test]
    fn shell_tool_process_entrypoints_use_shared_background_factories() {
        // shellCommandSource owns the one-shot shell process entrypoint.
        // shellCommandSource 持有一次性 Shell 进程入口。
        let shell_command_source =
            include_str!("../../vulcan-agent-runtime/src/tooling/native/shell_command.rs");
        // commandSessionSource owns the pipe-backed long-lived process entrypoint.
        // commandSessionSource 持有管道后端长生命周期进程入口。
        let command_session_source = include_str!("command_session_runtime/runtime.rs");
        // processTreeSource owns the Windows process-tree stop helper entrypoints.
        // processTreeSource 持有 Windows 进程树停止辅助入口。
        let process_tree_source = include_str!("process_tree.rs");
        // terminalHostSource owns the isolated PTY host process entrypoint.
        // terminalHostSource 持有隔离 PTY 宿主进程入口。
        let terminal_host_source = include_str!("terminal_host_client.rs");

        assert!(shell_command_source.contains("new_background_tokio_command(&program)"));
        assert!(
            command_session_source
                .contains("new_background_tokio_command(&request.launcher_program)")
        );
        assert!(process_tree_source.contains("new_background_tokio_command(\"taskkill\")"));
        assert!(process_tree_source.contains("new_background_std_command(\"taskkill\")"));
        assert!(
            terminal_host_source
                .contains("new_background_tokio_command(crate::runtime_assets::native_host_path(")
        );

        assert!(!shell_command_source.contains("TokioCommand::new(&program)"));
        assert!(!command_session_source.contains("TokioCommand::new(&request.launcher_program)"));
        assert!(!process_tree_source.contains("TokioCommand::new(\"taskkill\")"));
        assert!(!process_tree_source.contains("StdCommand::new(\"taskkill\")"));
        assert!(
            !terminal_host_source.contains("Command::new(crate::runtime_assets::native_host_path(")
        );
    }

    /// Verifies the shared Windows background policy uses the operating-system no-window flag.
    /// 验证共享 Windows 后台策略使用操作系统无窗口标志。
    #[cfg(windows)]
    #[test]
    fn windows_background_policy_uses_create_no_window() {
        assert_eq!(WINDOWS_CREATE_NO_WINDOW, 0x0800_0000);
    }

    /// Keeps the platform-specific constant referenced in non-Windows test builds.
    /// 在非 Windows 测试构建中保持平台专用常量被引用。
    #[cfg(not(windows))]
    #[test]
    fn non_windows_build_keeps_windows_policy_constant_stable() {
        assert_eq!(WINDOWS_CREATE_NO_WINDOW, 0x0800_0000);
    }
}
