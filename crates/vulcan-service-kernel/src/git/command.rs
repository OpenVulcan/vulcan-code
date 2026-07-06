//! Git command helpers shared by WebUI Git services.
//! WebUI Git 服务共享的 Git 命令辅助函数。

use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};
use vulcan_host_tools::process_creation::apply_windows_release_no_window_to_std_command;

/// Runs one bounded Git command with filters and pagers disabled.
/// 在禁用过滤器与分页器的情况下运行单个有界 Git 命令。
pub(crate) fn run_webui_git_command(
    working_directory: &Path,
    args: &[&str],
) -> Result<std::process::Output, String> {
    run_webui_git_command_without_stdin(working_directory, args, true, "git command")
}

/// Runs one bounded Git command with stdin content.
/// 使用 stdin 内容运行单个有界 Git 命令。
pub(crate) fn run_webui_git_command_with_stdin(
    working_directory: &Path,
    args: &[&str],
    stdin: &[u8],
) -> Result<std::process::Output, String> {
    run_webui_git_command_with_stdin_context(working_directory, args, stdin, true, "git command")
}

/// Runs one real Git action command without disabling repository hooks.
/// 在不禁用仓库 hooks 的情况下运行一条真实 Git 操作命令。
pub(crate) fn run_webui_git_mutating_command(
    working_directory: &Path,
    args: &[&str],
) -> Result<std::process::Output, String> {
    run_webui_git_command_without_stdin(working_directory, args, false, "git action command")
}

/// Runs one real Git action command with stdin and without disabling repository hooks.
/// 在不禁用仓库 hooks 的情况下使用 stdin 运行一条真实 Git 操作命令。
pub(crate) fn run_webui_git_mutating_command_with_stdin(
    working_directory: &Path,
    args: &[&str],
    stdin: &[u8],
) -> Result<std::process::Output, String> {
    run_webui_git_command_with_stdin_context(
        working_directory,
        args,
        stdin,
        false,
        "git action command",
    )
}

/// Builds one WebUI Git command with the requested safety profile.
/// 使用指定安全配置构建单个 WebUI Git 命令。
///
/// # Parameters
/// 参数
/// `working_directory` is the repository or workspace directory where Git should run.
/// `working_directory` 是 Git 应运行的仓库或工作区目录。
/// `disable_repository_hooks` indicates whether filters, hooks, and optional locks are disabled.
/// `disable_repository_hooks` 表示是否禁用过滤器、hooks 与可选锁。
///
/// # Returns
/// 返回值
/// A configured `git` command without caller-specific arguments or stdio configuration.
/// 返回尚未附加调用方参数或 stdio 配置的 `git` 命令。
fn webui_git_command(working_directory: &Path, disable_repository_hooks: bool) -> Command {
    let mut command = Command::new("git");
    command
        .current_dir(working_directory)
        .env("GIT_PAGER", "")
        .arg("-c")
        .arg("core.fsmonitor=false");
    if disable_repository_hooks {
        let hooks_path = if cfg!(windows) { "NUL" } else { "/dev/null" };
        command
            .env("GIT_OPTIONAL_LOCKS", "0")
            .arg("-c")
            .arg("diff.external=")
            .arg("-c")
            .arg("core.quotepath=false")
            .arg("-c")
            .arg(format!("core.hooksPath={hooks_path}"));
    }
    command
}

/// Runs a configured WebUI Git command without stdin.
/// 运行不带 stdin 的已配置 WebUI Git 命令。
///
/// # Parameters
/// 参数
/// `working_directory` is the repository or workspace directory where Git should run.
/// `working_directory` 是 Git 应运行的仓库或工作区目录。
/// `args` are the Git arguments appended after shared configuration.
/// `args` 是追加在共享配置后的 Git 参数。
/// `disable_repository_hooks` indicates whether filters, hooks, and optional locks are disabled.
/// `disable_repository_hooks` 表示是否禁用过滤器、hooks 与可选锁。
/// `command_label` is used in failure diagnostics.
/// `command_label` 用于失败诊断。
///
/// # Returns
/// 返回值
/// The completed process output, or a command-spawn diagnostic.
/// 返回完成后的进程输出，或命令启动诊断。
fn run_webui_git_command_without_stdin(
    working_directory: &Path,
    args: &[&str],
    disable_repository_hooks: bool,
    command_label: &str,
) -> Result<std::process::Output, String> {
    let mut command = webui_git_command(working_directory, disable_repository_hooks);
    command.args(args);
    apply_windows_release_no_window_to_std_command(&mut command);
    command.output().map_err(|error| {
        format!(
            "failed to run {command_label} `{}`: {error}",
            args.join(" ")
        )
    })
}

/// Runs a configured WebUI Git command with stdin.
/// 使用 stdin 运行已配置的 WebUI Git 命令。
///
/// # Parameters
/// 参数
/// `working_directory` is the repository or workspace directory where Git should run.
/// `working_directory` 是 Git 应运行的仓库或工作区目录。
/// `args` are the Git arguments appended after shared configuration.
/// `args` 是追加在共享配置后的 Git 参数。
/// `stdin` is the bytes written to the child process stdin.
/// `stdin` 是写入子进程 stdin 的字节。
/// `disable_repository_hooks` indicates whether filters, hooks, and optional locks are disabled.
/// `disable_repository_hooks` 表示是否禁用过滤器、hooks 与可选锁。
/// `command_label` is used in failure diagnostics.
/// `command_label` 用于失败诊断。
///
/// # Returns
/// 返回值
/// The completed process output, or a spawn, write, or wait diagnostic.
/// 返回完成后的进程输出，或启动、写入、等待诊断。
fn run_webui_git_command_with_stdin_context(
    working_directory: &Path,
    args: &[&str],
    stdin: &[u8],
    disable_repository_hooks: bool,
    command_label: &str,
) -> Result<std::process::Output, String> {
    let mut command = webui_git_command(working_directory, disable_repository_hooks);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_windows_release_no_window_to_std_command(&mut command);
    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to run {command_label} `{}` with stdin: {error}",
            args.join(" ")
        )
    })?;
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin.write_all(stdin).map_err(|error| {
            format!(
                "failed to write stdin for {command_label} `{}`: {error}",
                args.join(" ")
            )
        })?;
    }
    child.wait_with_output().map_err(|error| {
        format!(
            "failed to wait for {command_label} `{}` with stdin: {error}",
            args.join(" ")
        )
    })
}

/// Formats a Git command failure with stderr preserved for diagnostics.
/// 格式化 Git 命令失败信息，并保留 stderr 方便诊断。
pub(crate) fn format_webui_git_error(label: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{label} failed with status {}", output.status)
    } else {
        format!("{label} failed with status {}: {stderr}", output.status)
    }
}
