use super::*;

pub(crate) fn terminal_transaction_command_text(
    inputs: &[TerminalSessionInput],
) -> Result<String, NativeToolError> {
    if inputs.len() != 1 {
        return Err(NativeToolError::new(
            "`until_exit_code` requires exactly one text input containing one complete shell command.",
        ));
    }
    let text = match &inputs[0] {
        TerminalSessionInput::Text { text } => text,
        TerminalSessionInput::Paste { .. } => {
            return Err(NativeToolError::new(
                "`until_exit_code` cannot be used with paste input. Use text for one shell command transaction or use until_quiet for pasted interactive content.",
            ));
        }
        TerminalSessionInput::Key { .. } | TerminalSessionInput::Chord { .. } => {
            return Err(NativeToolError::new(
                "`until_exit_code` cannot be used with key or chord input.",
            ));
        }
    };
    if !stdin_payload_has_trailing_newline(text) {
        return Err(NativeToolError::new(
            "`until_exit_code` requires a complete command ending with a newline.",
        ));
    }
    let command = text.trim().to_string();
    if command.is_empty() {
        return Err(NativeToolError::new(
            "`until_exit_code` requires a non-empty command.",
        ));
    }
    Ok(command)
}

/// Validates that a terminal is clean enough to start one command transaction.
/// 校验终端状态是否足够干净，可以启动一条命令事务。
pub(crate) fn validate_terminal_transaction_start(
    snapshot: &CommandSessionSnapshot,
) -> Result<(), NativeToolError> {
    if snapshot.pending_stdin_line {
        return Err(NativeToolError::new(
            "`until_exit_code` cannot run while the terminal has a pending input line. Send Ctrl+C or submit/clear the current line before starting a command transaction.",
        ));
    }
    Ok(())
}

/// Wraps one shell command so the host can observe the transaction exit code.
/// 包装一条 shell 命令，使宿主可以观测该事务退出码。
pub(crate) fn wrap_terminal_transaction_command(
    command: &str,
    shell_label: &str,
    transaction_id: &str,
    interactive_newline: &str,
) -> Result<String, NativeToolError> {
    let sentinel = format!("__VULCAN_TERMINAL_TX_EXIT__:{transaction_id}:");
    let lower_shell = shell_label.to_ascii_lowercase();
    if lower_shell.contains("pwsh") || lower_shell.contains("powershell") {
        // historyCommand adds the real user command after PSReadLine has already accepted the internal wrapper.
        // historyCommand 在 PSReadLine 已接收内部包装后追加真实用户命令。
        let history_command = powershell_single_quoted_literal(command);
        return Ok(format!(
            "$global:LASTEXITCODE = $null; $vulcanErrorCount = $Error.Count; & {{ {command} }}; $vulcanSuccess = $?; $vulcanNativeCode = $LASTEXITCODE; $vulcanNewErrors = $Error.Count - $vulcanErrorCount; if ($null -ne $vulcanNativeCode) {{ $vulcanCode = $vulcanNativeCode }} elseif ($vulcanSuccess -and $vulcanNewErrors -le 0) {{ $vulcanCode = 0 }} else {{ $vulcanCode = 1 }}; if ('Microsoft.PowerShell.PSConsoleReadLine' -as [type]) {{ try {{ [Microsoft.PowerShell.PSConsoleReadLine]::AddToHistory({history_command}) }} catch {{ }} }}; Write-Output \"{sentinel}$vulcanCode\"{interactive_newline}"
        ));
    }
    if lower_shell.contains("cmd") {
        return Ok(format!(
            "{command}{interactive_newline}echo {sentinel}%ERRORLEVEL%{interactive_newline}"
        ));
    }
    if lower_shell.contains("bash") || lower_shell.contains("sh") || lower_shell.contains("zsh") {
        return Ok(format!(
            "{{ {command}; vulcan_code=$?; printf '\\n{sentinel}%s\\n' \"$vulcan_code\"; }}{interactive_newline}"
        ));
    }
    Err(NativeToolError::new(format!(
        "`until_exit_code` is not supported for shell `{shell_label}`. Use until_quiet or an explicit supported shell."
    )))
}

/// Encodes one PowerShell single-quoted string literal without executing or re-parsing its content.
/// 编码一个 PowerShell 单引号字符串字面量，且不执行或重新解析其内容。
///
/// # Parameters / 参数
/// - `text`: Exact user command retained for PSReadLine history.
/// - `text`：为 PSReadLine 历史保留的精确用户命令。
///
/// # Returns / 返回值
/// Returns a single-quoted literal whose embedded apostrophes are doubled per PowerShell syntax.
/// 返回按 PowerShell 语法将内部单引号加倍后的单引号字面量。
fn powershell_single_quoted_literal(text: &str) -> String {
    format!("'{}'", text.replace('\'', "''"))
}

pub(crate) fn strip_terminal_transaction_sentinel(
    chunks: &[CommandSessionOutputChunk],
    transaction_id: &str,
) -> (Vec<CommandSessionOutputChunk>, Option<i32>) {
    let sentinel_prefixes = terminal_transaction_sentinel_prefixes(transaction_id);
    let mut exit_code = None;
    let mut stripped_chunks = Vec::new();

    for chunk in chunks {
        let mut stripped_text = String::new();
        for line in chunk.text.split_inclusive('\n') {
            let line_without_newline = line.trim_end_matches(['\r', '\n']);
            let detection_line = strip_simple_ansi_sequences(line_without_newline);
            if let Some((sentinel_start, sentinel_len)) =
                find_terminal_transaction_sentinel(&detection_line, &sentinel_prefixes)
            {
                let raw_code = &detection_line[sentinel_start + sentinel_len..];
                let raw_code = raw_code
                    .trim_start()
                    .chars()
                    .take_while(|ch| ch.is_ascii_digit() || *ch == '-' || *ch == '+')
                    .collect::<String>();
                if let Ok(parsed_code) = raw_code.parse::<i32>() {
                    exit_code = Some(parsed_code);
                }
                continue;
            }
            stripped_text.push_str(line);
        }
        if !stripped_text.is_empty() {
            let mut stripped_chunk = chunk.clone();
            stripped_chunk.text = stripped_text;
            stripped_chunks.push(stripped_chunk);
        }
    }

    (stripped_chunks, exit_code)
}

/// Returns transaction sentinel prefixes accepted for append logs and left-clipped screen frames.
/// 返回追加日志与左侧裁剪屏幕帧中可接受的事务哨兵前缀。
pub(crate) fn terminal_transaction_sentinel_prefixes(transaction_id: &str) -> Vec<String> {
    let visible_prefix = "__VULCAN_TERMINAL_TX_EXIT__:";
    let shortest_suffix_len = "EXIT__:".len();
    (0..=visible_prefix.len() - shortest_suffix_len)
        .map(|offset| format!("{}{transaction_id}:", &visible_prefix[offset..]))
        .collect()
}

/// Finds one exact transaction sentinel prefix inside a decoded terminal line.
/// 在已解码终端行中查找一个精确事务哨兵前缀。
pub(crate) fn find_terminal_transaction_sentinel(
    line: &str,
    sentinel_prefixes: &[String],
) -> Option<(usize, usize)> {
    sentinel_prefixes
        .iter()
        .filter_map(|prefix| line.find(prefix).map(|start| (start, prefix.len())))
        .min_by_key(|(start, _)| *start)
}
