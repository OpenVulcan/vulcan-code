//! Host-native RTK command router: bundled executable lookup, rewrite execution, and route parsing.
//! 宿主原生 RTK 命令路由：随包可执行文件查找、改写执行与路由解析。
//!
//! The rewrite protocol belongs to upstream `src/hooks/rewrite_cmd.rs` at the release locked in
//! `scripts/build-inputs.lock.json` `tools.rtk`; re-read it whenever that release changes.
//! 改写协议归属于 `scripts/build-inputs.lock.json` `tools.rtk` 锁定发行版的上游
//! `src/hooks/rewrite_cmd.rs`；该发行版变化时必须重新核对。

use crate::process_creation::new_background_tokio_command;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

/// File stem of the bundled RTK executable inside `tools/bin`.
/// `tools/bin` 中随包 RTK 可执行文件的文件名主干。
pub const RTK_BINARY_STEM: &str = "rtk";

/// Upper bound for one `rtk rewrite` inspection before the original command runs unchanged.
/// 单次 `rtk rewrite` 检查的上限；超时后原命令保持不变地执行。
pub const RTK_REWRITE_TIMEOUT: Duration = Duration::from_millis(2_000);

/// Upper bound for each RTK Git command that compresses commit-summary context.
/// 压缩提交总结上下文的每条 RTK Git 命令的上限。
pub const RTK_GIT_CONTEXT_TIMEOUT: Duration = Duration::from_millis(5_000);

/// Environment variable that keeps RTK output free of ANSI colors; set only for routed commands.
/// 使 RTK 输出不含 ANSI 颜色的环境变量；只为经路由的命令设置。
pub const RTK_PLAIN_OUTPUT_VARIABLE: &str = "NO_COLOR";

/// Environment variable that suppresses RTK's `rtk init` hook-installation warning.
/// 抑制 RTK `rtk init` Hook 安装提示的环境变量。
pub const RTK_SUPPRESS_HOOK_WARNING_VARIABLE: &str = "RTK_SUPPRESS_HOOK_WARNING";

/// Environment variable that disables RTK telemetry.
/// 关闭 RTK 遥测的环境变量。
pub const RTK_TELEMETRY_DISABLED_VARIABLE: &str = "RTK_TELEMETRY_DISABLED";

/// Returns the bundled RTK executable when the active output root ships one.
/// 当前 output 根目录随包提供 RTK 时返回其可执行文件。
///
/// # Returns
/// 返回值
/// The existing `<tools/bin>/rtk[.exe]` path, or `None` for hosts without a locked RTK artifact.
/// 已存在的 `<tools/bin>/rtk[.exe]` 路径；锁文件未提供 RTK 制品的宿主返回 `None`。
pub fn resolve_rtk_executable() -> Option<PathBuf> {
    let directory = crate::runtime_assets::resolve_bundled_tool_bin_directory()?;
    let executable = directory.join(format!("{RTK_BINARY_STEM}{}", std::env::consts::EXE_SUFFIX));
    executable.is_file().then_some(executable)
}

/// Builds the RTK variables the host injects into every process it launches for the model.
/// 构建宿主为模型启动的每个进程注入的 RTK 变量。
///
/// # Parameters
/// 参数
/// `telemetry_disabled` is the user's RTK telemetry setting.
/// `telemetry_disabled` 是用户的 RTK 遥测设置。
///
/// # Returns
/// 返回值
/// Ordered name/value pairs; later process layers may override them.
/// 有序名称与值；后续进程层可覆盖它们。
pub fn rtk_host_environment(telemetry_disabled: bool) -> Vec<(String, String)> {
    let mut environment = vec![(
        String::from(RTK_SUPPRESS_HOOK_WARNING_VARIABLE),
        String::from("1"),
    )];
    if telemetry_disabled {
        environment.push((
            String::from(RTK_TELEMETRY_DISABLED_VARIABLE),
            String::from("1"),
        ));
    }
    environment
}

/// Builds the variables for an RTK process the host runs itself: the host set plus plain output.
/// 构建宿主自行运行 RTK 进程所用的变量：宿主变量集合加纯文本输出。
///
/// # Parameters
/// 参数
/// `telemetry_disabled` is the user's RTK telemetry setting.
/// `telemetry_disabled` 是用户的 RTK 遥测设置。
pub fn rtk_invocation_environment(telemetry_disabled: bool) -> Vec<(String, String)> {
    let mut environment = rtk_host_environment(telemetry_disabled);
    environment.push((String::from(RTK_PLAIN_OUTPUT_VARIABLE), String::from("1")));
    environment
}

/// Builds one hidden, bounded RTK child process with piped output.
/// 构建一个隐藏、有界且输出走管道的 RTK 子进程。
fn rtk_command(
    executable: &Path,
    arguments: &[&str],
    cwd: &Path,
    environment: &[(String, String)],
) -> tokio::process::Command {
    let mut command = new_background_tokio_command(executable);
    command
        .args(arguments)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for (name, value) in environment {
        command.env(name, value);
    }
    command
}

/// Runs one RTK command and returns its stdout when it exits successfully within `timeout`.
/// 运行一条 RTK 命令，在 `timeout` 内成功退出时返回其标准输出。
///
/// # Parameters
/// 参数
/// `arguments` follow the executable; `cwd` and `environment` configure the hidden child.
/// `arguments` 跟在可执行文件之后；`cwd` 与 `environment` 配置隐藏子进程。
///
/// # Returns
/// 返回值
/// Decoded stdout, or an English reason for a timeout, launch failure, or non-zero exit.
/// 解码后的标准输出；超时、启动失败或非零退出时返回英文原因。
pub async fn run_rtk_capture(
    executable: &Path,
    arguments: &[&str],
    cwd: &Path,
    environment: &[(String, String)],
    timeout: Duration,
) -> Result<String, String> {
    let child = rtk_command(executable, arguments, cwd, environment)
        .spawn()
        .map_err(|error| format!("failed to launch rtk: {error}"))?;
    let output = tokio::time::timeout(timeout, child.wait_with_output())
        .await
        .map_err(|_| format!("rtk timed out after {} ms", timeout.as_millis()))?
        .map_err(|error| format!("rtk failed: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "rtk exited with {}: {}",
            output
                .status
                .code()
                .map_or_else(|| String::from("a signal"), |code| format!("code {code}")),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(crate::output_decode::decode_shell_output_bytes(
        &output.stdout,
    ))
}

/// Removes RTK's own `[rtk]` notice lines and surrounding whitespace from compressed output.
/// 从压缩输出中移除 RTK 自身的 `[rtk]` 提示行及两端空白。
pub fn strip_rtk_notices(stdout: &str) -> String {
    stdout
        .lines()
        .filter(|line| !line.trim_start().starts_with("[rtk]"))
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Result of asking RTK whether one shell command has an RTK equivalent.
/// 询问 RTK 某条 shell 命令是否有 RTK 等价命令的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RtkRewriteOutcome {
    /// Exit 0 or 3 with the complete rewritten command.
    /// 退出码 0 或 3，并携带完整改写命令。
    Rewritten(String),
    /// Exit 1: RTK has no rewrite for this command.
    /// 退出码 1：RTK 对此命令没有改写。
    NoRewrite,
    /// Exit 2: an RTK deny rule matched, so the command stays with Vulcan's own permission pipeline.
    /// 退出码 2：命中 RTK 拒绝规则，命令留给 Vulcan 自身的权限管线。
    Denied,
    /// Timeout, launch failure, unknown exit, or unusable output; never executed.
    /// 超时、启动失败、未知退出码或不可用输出；绝不执行。
    Failed(String),
}

/// Classifies one finished `rtk rewrite` process.
/// 对一个已结束的 `rtk rewrite` 进程分类。
///
/// # Parameters
/// 参数
/// `exit_code` is the process exit code (`None` when killed by a signal) and `stdout` its raw bytes.
/// `exit_code` 是进程退出码（被信号终止时为 `None`），`stdout` 是其原始字节。
pub fn classify_rtk_rewrite(exit_code: Option<i32>, stdout: &[u8]) -> RtkRewriteOutcome {
    match exit_code {
        Some(0 | 3) => {
            let Ok(text) = std::str::from_utf8(stdout) else {
                return RtkRewriteOutcome::Failed(String::from(
                    "rtk rewrite returned non-UTF-8 output",
                ));
            };
            let rewritten = text.trim();
            if rewritten.is_empty() {
                RtkRewriteOutcome::Failed(String::from("rtk rewrite returned an empty command"))
            } else {
                RtkRewriteOutcome::Rewritten(rewritten.to_string())
            }
        }
        Some(1) => RtkRewriteOutcome::NoRewrite,
        Some(2) => RtkRewriteOutcome::Denied,
        Some(code) => RtkRewriteOutcome::Failed(format!("rtk rewrite exited with code {code}")),
        None => RtkRewriteOutcome::Failed(String::from("rtk rewrite was terminated")),
    }
}

/// Runs `rtk rewrite` for one command in a hidden, bounded child process.
/// 在隐藏且有界的子进程中为一条命令运行 `rtk rewrite`。
///
/// # Parameters
/// 参数
/// `executable` is the bundled RTK path, `command_text` the model's command, `cwd` its working
/// directory, and `environment` the variables added to the inherited environment.
/// `executable` 是随包 RTK 路径，`command_text` 是模型命令，`cwd` 是其工作目录，
/// `environment` 是追加到继承环境上的变量。
///
/// # Returns
/// 返回值
/// The classified outcome; partial output from a timed-out run is discarded.
/// 分类后的结果；超时运行产生的部分输出被丢弃。
pub async fn run_rtk_rewrite(
    executable: &Path,
    command_text: &str,
    cwd: &Path,
    environment: &[(String, String)],
) -> RtkRewriteOutcome {
    let child = match rtk_command(executable, &["rewrite", command_text], cwd, environment).spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return RtkRewriteOutcome::Failed(format!("failed to launch rtk rewrite: {error}"));
        }
    };
    match tokio::time::timeout(RTK_REWRITE_TIMEOUT, child.wait_with_output()).await {
        Ok(Ok(output)) => classify_rtk_rewrite(output.status.code(), &output.stdout),
        Ok(Err(error)) => RtkRewriteOutcome::Failed(format!("rtk rewrite failed: {error}")),
        Err(_) => RtkRewriteOutcome::Failed(format!(
            "rtk rewrite timed out after {} ms",
            RTK_REWRITE_TIMEOUT.as_millis()
        )),
    }
}

/// One shell token produced by the route scanner.
/// 路由扫描器产生的一个 shell Token。
#[derive(Debug, PartialEq, Eq)]
enum ShellToken {
    /// A word with its quotes removed.
    /// 去除引号后的单词。
    Word(String),
    /// One of `;`, `&`, `|`, `&&`, `||`.
    /// `;`、`&`、`|`、`&&`、`||` 之一。
    Operator,
}

/// Splits a command into words and control operators, keeping quoted text inside one word.
/// 将命令拆分为单词与控制操作符，引号内的文本保留在同一单词中。
fn tokenize_shell_command(command: &str) -> Vec<ShellToken> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut characters = command.chars().peekable();
    let flush = |current: &mut String, tokens: &mut Vec<ShellToken>| {
        if !current.is_empty() {
            tokens.push(ShellToken::Word(std::mem::take(current)));
        }
    };
    while let Some(character) = characters.next() {
        if let Some(active) = quote {
            if character == active {
                quote = None;
            } else {
                current.push(character);
            }
            continue;
        }
        match character {
            '"' | '\'' => quote = Some(character),
            ';' | '&' | '|' => {
                flush(&mut current, &mut tokens);
                if character != ';' && characters.peek() == Some(&character) {
                    characters.next();
                }
                tokens.push(ShellToken::Operator);
            }
            character if character.is_whitespace() => flush(&mut current, &mut tokens),
            character => current.push(character),
        }
    }
    flush(&mut current, &mut tokens);
    tokens
}

/// Returns whether one word names the RTK executable, with or without a directory or `.exe`.
/// 返回一个单词是否指向 RTK 可执行文件（可带目录或 `.exe`）。
fn is_rtk_executable(word: &str) -> bool {
    let basename = word.rsplit(['/', '\\']).next().unwrap_or(word);
    basename.eq_ignore_ascii_case(RTK_BINARY_STEM)
        || basename.eq_ignore_ascii_case(&format!("{RTK_BINARY_STEM}.exe"))
}

/// Collects the unique RTK routes referenced anywhere in a rewritten command, in first-seen order.
/// 按首次出现顺序收集改写命令中任意位置引用的唯一 RTK 路由。
///
/// # Parameters
/// 参数
/// `command` is the complete rewrite output, including every line.
/// `command` 是包含全部行的完整改写输出。
///
/// # Returns
/// 返回值
/// Lowercase route names such as `git` or `cargo`; flags directly after `rtk` are not routes.
/// 小写路由名，例如 `git` 或 `cargo`；紧跟 `rtk` 的参数标志不算路由。
pub fn rtk_command_routes(command: &str) -> Vec<String> {
    let mut routes: Vec<String> = Vec::new();
    let mut expect_route = false;
    for token in tokenize_shell_command(command) {
        let ShellToken::Word(word) = token else {
            expect_route = false;
            continue;
        };
        if expect_route && !word.starts_with('-') {
            let route = word.to_ascii_lowercase();
            if !routes.contains(&route) {
                routes.push(route);
            }
        }
        expect_route = is_rtk_executable(&word);
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Routes are found on every line and after every operator, and quoted operators stay text.
    /// 每一行、每个操作符之后的路由都会被找到，引号中的操作符保持为文本。
    #[test]
    fn rtk_routes_cover_every_command_segment() {
        assert_eq!(
            rtk_command_routes("rtk git status --short\nrtk cargo check && rtk.exe GIT diff"),
            vec![String::from("git"), String::from("cargo")]
        );
        assert_eq!(
            rtk_command_routes("rtk git log | C:\\tools\\rtk.exe grep x;rtk ls"),
            vec![
                String::from("git"),
                String::from("grep"),
                String::from("ls")
            ]
        );
        assert_eq!(
            rtk_command_routes("echo \"rtk && docker\""),
            Vec::<String>::new()
        );
        assert_eq!(rtk_command_routes("rtk --version"), Vec::<String>::new());
        assert_eq!(
            rtk_command_routes("rtk && git status"),
            Vec::<String>::new()
        );
    }

    /// Only exits 0 and 3 with non-empty UTF-8 output become an executable rewrite.
    /// 只有带非空 UTF-8 输出的退出码 0 与 3 会成为可执行改写。
    #[test]
    fn rtk_rewrite_classification_never_executes_unusable_output() {
        assert_eq!(
            classify_rtk_rewrite(Some(3), b"  rtk git status\n"),
            RtkRewriteOutcome::Rewritten(String::from("rtk git status"))
        );
        assert_eq!(
            classify_rtk_rewrite(Some(0), b"rtk cargo test"),
            RtkRewriteOutcome::Rewritten(String::from("rtk cargo test"))
        );
        assert_eq!(
            classify_rtk_rewrite(Some(1), b"rtk git"),
            RtkRewriteOutcome::NoRewrite
        );
        assert_eq!(
            classify_rtk_rewrite(Some(2), b"rtk git"),
            RtkRewriteOutcome::Denied
        );
        for (code, stdout) in [
            (Some(0), b" \n".as_slice()),
            (Some(3), b"\xff\xfe".as_slice()),
            (Some(9), b"rtk git".as_slice()),
            (None, b"rtk git".as_slice()),
        ] {
            assert!(matches!(
                classify_rtk_rewrite(code, stdout),
                RtkRewriteOutcome::Failed(_)
            ));
        }
    }

    /// The init warning is always suppressed and telemetry follows the user setting.
    /// 始终抑制 init 提示，遥测跟随用户设置。
    #[test]
    fn rtk_host_environment_follows_telemetry_setting() {
        let names = |telemetry_disabled| {
            rtk_host_environment(telemetry_disabled)
                .into_iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(true),
            vec![
                String::from(RTK_SUPPRESS_HOOK_WARNING_VARIABLE),
                String::from(RTK_TELEMETRY_DISABLED_VARIABLE)
            ]
        );
        assert_eq!(
            names(false),
            vec![String::from(RTK_SUPPRESS_HOOK_WARNING_VARIABLE)]
        );
    }

    /// Only `[rtk]` notice lines are dropped; indented diff content stays intact.
    /// 只丢弃 `[rtk]` 提示行；缩进的 diff 内容保持不变。
    #[test]
    fn rtk_notice_lines_are_stripped() {
        assert_eq!(
            strip_rtk_notices("\n[rtk] notice\nM src/a.rs\n  [rtk] hint\n  +line\n\n"),
            "M src/a.rs\n  +line"
        );
        assert_eq!(strip_rtk_notices("[rtk] only\n"), "");
    }
}
