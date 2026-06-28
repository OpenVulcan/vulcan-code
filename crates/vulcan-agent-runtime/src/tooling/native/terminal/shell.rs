use super::*;

pub(crate) fn shell_specific_newline(shell_label: &str) -> &'static str {
    HostCommandShell::parse(shell_label)
        .map(HostCommandShell::interactive_newline)
        .unwrap_or(if cfg!(windows) { "\r\n" } else { "\n" })
}

/// Normalizes one optional shell selector and falls back to `auto` when the caller omitted it.
/// 规范化一份可选 shell 选择器，并在调用方省略时回退到 `auto`。
pub(crate) fn normalize_shell_selector(raw_shell_selector: Option<&str>) -> &str {
    raw_shell_selector
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("auto")
}

/// Reports whether one shell selector still relies on host-side `auto` resolution.
/// 判断一个 shell 选择器是否仍依赖宿主侧 `auto` 解析。
pub(crate) fn shell_selector_uses_auto(raw_shell_selector: Option<&str>) -> bool {
    normalize_shell_selector(raw_shell_selector).eq_ignore_ascii_case("auto")
}

/// Reports whether one stdin payload already ends with a line terminator.
/// 判断一段标准输入载荷是否已经以行终止符结束。
pub(crate) fn stdin_payload_has_trailing_newline(text: &str) -> bool {
    text.ends_with(['\n', '\r'])
}

pub(crate) fn terminal_generated_command_newline(shell_label: &str) -> &'static str {
    terminal_session_pty_newline(shell_label)
}

/// Returns the PTY-facing newline sequence used by generated terminal key/control input.
/// 返回宿主生成的终端按键或控制输入面向 PTY 使用的换行序列。
pub(crate) fn terminal_session_pty_newline(shell_label: &str) -> &'static str {
    let lower_shell = shell_label.to_ascii_lowercase();
    if cfg!(windows)
        && (lower_shell.contains("pwsh")
            || lower_shell.contains("powershell")
            || lower_shell.contains("cmd"))
    {
        "\r"
    } else {
        shell_specific_newline(shell_label)
    }
}

/// Returns the default backend used by terminal_session on the current host.
/// 返回当前宿主上 terminal_session 使用的默认后端。
pub(crate) fn default_terminal_session_backend() -> CommandSessionBackend {
    if cfg!(windows) {
        CommandSessionBackend::WindowsConsole
    } else {
        CommandSessionBackend::Pty
    }
}

#[cfg(test)]
pub(crate) fn effective_start_append_newline(
    requested_append_newline: Option<bool>,
    initial_stdin: Option<&str>,
) -> bool {
    requested_append_newline.unwrap_or(initial_stdin.is_some())
}

/// Validates whether `start` stdin is safe to submit without hidden concatenation across later writes.
/// 校验 `start` 的标准输入是否能安全提交，避免与后续写入发生隐式拼接。
#[cfg(test)]
pub(crate) fn validate_shell_session_start_stdin_policy(
    initial_stdin: Option<&str>,
    append_newline: bool,
) -> Result<(), NativeToolError> {
    if let Some(stdin_text) = initial_stdin {
        if !append_newline && !stdin_payload_has_trailing_newline(stdin_text) {
            return Err(NativeToolError::new(
                "shell_session start rejected initial stdin without a trailing newline. Either set `append_newline: true`, include the trailing newline inside `stdin`, or start without `stdin` and use `write` for partial interactive input.",
            ));
        }
    }
    Ok(())
}

/// Validates whether one requested shell selector may safely rely on `auto` for the resolved session behavior.
/// 校验一条请求的 shell 选择器是否可以安全依赖 `auto` 来驱动当前会话行为。
pub(crate) fn validate_terminal_session_start_shell_policy(
    raw_shell_selector: Option<&str>,
    shell_behavior: &ResolvedSessionShellBehavior,
) -> Result<(), NativeToolError> {
    if cfg!(windows)
        && shell_selector_uses_auto(raw_shell_selector)
        && shell_behavior.nested_shell_inferred
        && matches!(
            HostCommandShell::parse(shell_behavior.interactive_shell_label.as_str()),
            Some(HostCommandShell::Bash | HostCommandShell::Sh | HostCommandShell::Zsh)
        )
    {
        return Err(NativeToolError::new(
            "terminal_session start requires an explicit `shell` when the command directly launches `bash`, `sh`, or `zsh` on Windows. `auto` resolves only the outer launcher and is rejected here to keep interactive newline behavior deterministic.",
        ));
    }
    Ok(())
}

/// Extracts the leading program token from a raw command string for best-effort nested interactive shell detection.
/// 从原始命令字符串中提取起始程序标记，用于尽力检测嵌套交互 shell。
pub(crate) fn leading_command_program_token(command_text: &str) -> Option<String> {
    command_text
        .trim()
        .split_whitespace()
        .next()
        .map(|token| token.trim_matches(|character| character == '"' || character == '\''))
        .filter(|token| !token.is_empty())
        .map(|token| token.to_string())
}

/// Attempts to infer a nested interactive shell directly launched by the command text.
/// 尝试根据命令文本直接推断被启动的嵌套交互 shell。
pub(crate) fn infer_nested_interactive_shell(command_text: &str) -> Option<HostCommandShell> {
    let token = leading_command_program_token(command_text)?;
    let token = token
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(token.as_str())
        .to_ascii_lowercase();
    let token = token
        .strip_suffix(".exe")
        .or_else(|| token.strip_suffix(".cmd"))
        .or_else(|| token.strip_suffix(".bat"))
        .or_else(|| token.strip_suffix(".com"))
        .unwrap_or(token.as_str());
    HostCommandShell::parse(token)
}

/// Resolves the effective interactive shell behavior for a command session, including nested-shell handoff when it can be inferred safely.
/// 解析命令会话的有效交互 shell 行为，并在能够安全推断时处理嵌套 shell 切换。
pub(crate) fn resolve_session_shell_behavior(
    command_text: &str,
    launcher_shell_label: &str,
) -> ResolvedSessionShellBehavior {
    let nested_shell = infer_nested_interactive_shell(command_text);
    let interactive_shell_label = nested_shell
        .map(HostCommandShell::id)
        .unwrap_or(launcher_shell_label)
        .to_string();
    let nested_shell_inferred = interactive_shell_label != launcher_shell_label;

    ResolvedSessionShellBehavior {
        interactive_shell_label,
        launcher_shell_label: launcher_shell_label.to_string(),
        nested_shell_inferred,
    }
}
