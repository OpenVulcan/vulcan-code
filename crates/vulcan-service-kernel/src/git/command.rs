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
    let hooks_path = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut command = Command::new("git");
    command
        .current_dir(working_directory)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-c")
        .arg("diff.external=")
        .arg("-c")
        .arg("core.quotepath=false")
        .arg("-c")
        .arg(format!("core.hooksPath={hooks_path}"));
    command.args(args);
    apply_windows_release_no_window_to_std_command(&mut command);
    command
        .output()
        .map_err(|error| format!("failed to run git command `{}`: {error}", args.join(" ")))
}

/// Runs one bounded Git command with stdin content.
/// 使用 stdin 内容运行单个有界 Git 命令。
pub(crate) fn run_webui_git_command_with_stdin(
    working_directory: &Path,
    args: &[&str],
    stdin: &[u8],
) -> Result<std::process::Output, String> {
    let hooks_path = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut command = Command::new("git");
    command
        .current_dir(working_directory)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_PAGER", "")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .arg("-c")
        .arg("diff.external=")
        .arg("-c")
        .arg("core.quotepath=false")
        .arg("-c")
        .arg(format!("core.hooksPath={hooks_path}"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_windows_release_no_window_to_std_command(&mut command);
    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to run git command `{}` with stdin: {error}",
            args.join(" ")
        )
    })?;
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin.write_all(stdin).map_err(|error| {
            format!(
                "failed to write stdin for git command `{}`: {error}",
                args.join(" ")
            )
        })?;
    }
    child.wait_with_output().map_err(|error| {
        format!(
            "failed to wait for git command `{}` with stdin: {error}",
            args.join(" ")
        )
    })
}

/// Runs one real Git action command without disabling repository hooks.
/// 在不禁用仓库 hooks 的情况下运行一条真实 Git 操作命令。
pub(crate) fn run_webui_git_mutating_command(
    working_directory: &Path,
    args: &[&str],
) -> Result<std::process::Output, String> {
    let mut command = Command::new("git");
    command
        .current_dir(working_directory)
        .env("GIT_PAGER", "")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .args(args);
    apply_windows_release_no_window_to_std_command(&mut command);
    command.output().map_err(|error| {
        format!(
            "failed to run git action command `{}`: {error}",
            args.join(" ")
        )
    })
}

/// Runs one real Git action command with stdin and without disabling repository hooks.
/// 在不禁用仓库 hooks 的情况下使用 stdin 运行一条真实 Git 操作命令。
pub(crate) fn run_webui_git_mutating_command_with_stdin(
    working_directory: &Path,
    args: &[&str],
    stdin: &[u8],
) -> Result<std::process::Output, String> {
    let mut command = Command::new("git");
    command
        .current_dir(working_directory)
        .env("GIT_PAGER", "")
        .arg("-c")
        .arg("core.fsmonitor=false")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    apply_windows_release_no_window_to_std_command(&mut command);
    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to run git action command `{}` with stdin: {error}",
            args.join(" ")
        )
    })?;
    if let Some(mut child_stdin) = child.stdin.take() {
        child_stdin.write_all(stdin).map_err(|error| {
            format!(
                "failed to write stdin for git action command `{}`: {error}",
                args.join(" ")
            )
        })?;
    }
    child.wait_with_output().map_err(|error| {
        format!(
            "failed to wait for git action command `{}` with stdin: {error}",
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
