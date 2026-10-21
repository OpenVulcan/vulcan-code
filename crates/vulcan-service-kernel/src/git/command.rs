//! Git command helpers shared by WebUI Git services.
//! WebUI Git 服务共享的 Git 命令辅助函数。

use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};
use vulcan_host_tools::{
    output_decode::decode_shell_output_bytes, process_creation::new_background_std_command,
};

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
    // Git is a background service task even in Debug, where Core may have no inherited console.
    // 即使在 Debug 下，Git 仍是后台服务任务；Core 此时可能没有可继承的控制台。
    let mut command = new_background_std_command("git");
    // Git receives a standard host path so Windows child-process startup never inherits a verbatim prefix.
    // Git 接收标准宿主路径，避免 Windows 子进程启动继承逐字路径前缀。
    let native_working_directory = vulcan_runtime_assets::native_host_path(working_directory);
    command
        .current_dir(native_working_directory)
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
    let mut child = command.spawn().map_err(|error| {
        format!(
            "failed to run {command_label} `{}` with stdin: {error}",
            args.join(" ")
        )
    })?;
    let child_stdin = child.stdin.take();
    // Git can emit output while it still reads stdin, so stdin is written concurrently with draining
    // stdout and stderr; writing it first deadlocks once both pipe buffers fill.
    // Git 可能在仍读取 stdin 时输出，因此 stdin 与 stdout、stderr 的排空并发进行；先写完 stdin
    // 会在两端管道缓冲区写满后死锁。
    let (written, output) = std::thread::scope(|scope| {
        let writer = child_stdin.map(|mut child_stdin| {
            scope.spawn(move || {
                // Dropping the pipe at the end of the closure signals end of input.
                // 闭包结束时释放管道以发出输入结束信号。
                child_stdin.write_all(stdin)
            })
        });
        let output = child.wait_with_output();
        let written = writer.map_or(Ok(()), |writer| {
            writer
                .join()
                .unwrap_or_else(|_| Err(std::io::Error::other("stdin writer panicked")))
        });
        (written, output)
    });
    written.map_err(|error| {
        format!(
            "failed to write stdin for {command_label} `{}`: {error}",
            args.join(" ")
        )
    })?;
    output.map_err(|error| {
        format!(
            "failed to wait for {command_label} `{}` with stdin: {error}",
            args.join(" ")
        )
    })
}

/// Formats a Git command failure with stderr preserved for diagnostics.
/// 格式化 Git 命令失败信息，并保留 stderr 方便诊断。
pub(crate) fn format_webui_git_error(label: &str, output: &std::process::Output) -> String {
    let stderr = decode_shell_output_bytes(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        format!("{label} failed with status {}", output.status)
    } else {
        format!("{label} failed with status {}: {stderr}", output.status)
    }
}

/// Upper bound on Git processes one WebUI request runs at the same time.
/// 单个 WebUI 请求同时运行的 Git 进程上限。
const WEBUI_GIT_PARALLEL_PROCESSES: usize = 8;

/// Runs `task` over `items` on up to `WEBUI_GIT_PARALLEL_PROCESSES` scoped threads and returns the
/// results in input order. Each Git process costs about a hundred milliseconds to start on Windows,
/// so independent queries run side by side instead of one after another.
/// 在最多 `WEBUI_GIT_PARALLEL_PROCESSES` 个作用域线程上对 `items` 运行 `task`，按输入顺序返回结果。
/// Windows 上每个 Git 进程启动约需百毫秒，因此互不依赖的查询并行执行而非逐个串行。
pub(crate) fn map_webui_git_parallel<T: Sync, R: Send>(
    items: &[T],
    task: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let workers = items.len().min(WEBUI_GIT_PARALLEL_PROCESSES);
    if workers <= 1 {
        return items.iter().map(task).collect();
    }
    let next = AtomicUsize::new(0);
    let (next, task) = (&next, &task);
    let mut indexed: Vec<(usize, R)> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(move || {
                    let mut done = Vec::new();
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break done;
                        };
                        done.push((index, task(item)));
                    }
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
            })
            .collect()
    });
    indexed.sort_by_key(|(index, _)| *index);
    indexed.into_iter().map(|(_, result)| result).collect()
}

#[cfg(test)]
mod tests {
    use super::map_webui_git_parallel;

    // Results keep input order however the workers interleave.
    // 无论工作线程如何交错，结果都保持输入顺序。
    #[test]
    fn parallel_map_keeps_input_order() {
        let items: Vec<u64> = (0..40).collect();
        let doubled = map_webui_git_parallel(&items, |item| {
            std::thread::sleep(std::time::Duration::from_millis(40 - item));
            item * 2
        });
        assert_eq!(
            doubled,
            items.iter().map(|item| item * 2).collect::<Vec<_>>()
        );
        assert!(map_webui_git_parallel(&[] as &[u64], |item| *item).is_empty());
    }
}
