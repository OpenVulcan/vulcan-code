use super::*;

pub(crate) fn encode_terminal_inputs(
    inputs: &[TerminalSessionInput],
    interactive_newline: &str,
    key_encoding: TerminalSessionKeyEncoding,
) -> Result<Vec<u8>, NativeToolError> {
    let mut encoded = Vec::new();
    for input in inputs {
        match input {
            TerminalSessionInput::Text { text } | TerminalSessionInput::Paste { text } => {
                encoded.extend_from_slice(text.as_bytes());
            }
            TerminalSessionInput::Key { key } => {
                encoded.extend_from_slice(&terminal_key_sequence(
                    key,
                    interactive_newline,
                    key_encoding,
                )?);
            }
            TerminalSessionInput::Chord {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => {
                encoded.extend_from_slice(&terminal_chord_sequence(
                    key,
                    ctrl.unwrap_or(false),
                    alt.unwrap_or(false),
                    shift.unwrap_or(false),
                    meta.unwrap_or(false),
                    interactive_newline,
                    key_encoding,
                )?);
            }
        }
    }
    Ok(encoded)
}

/// Converts terminal-session inputs into backend-neutral structured input events.
/// 将终端会话输入转换为后端无关的结构化输入事件。
pub(crate) fn terminal_input_events_from_inputs(
    inputs: &[TerminalSessionInput],
) -> Vec<CommandSessionInputEvent> {
    inputs
        .iter()
        .map(|input| match input {
            TerminalSessionInput::Text { text } => {
                CommandSessionInputEvent::Text { text: text.clone() }
            }
            TerminalSessionInput::Paste { text } => {
                CommandSessionInputEvent::Paste { text: text.clone() }
            }
            TerminalSessionInput::Key { key } => CommandSessionInputEvent::Key { key: key.clone() },
            TerminalSessionInput::Chord {
                key,
                ctrl,
                alt,
                shift,
                meta,
            } => CommandSessionInputEvent::Chord {
                key: key.clone(),
                ctrl: ctrl.unwrap_or(false),
                alt: alt.unwrap_or(false),
                shift: shift.unwrap_or(false),
                meta: meta.unwrap_or(false),
            },
        })
        .collect()
}

/// Converts terminal-session inputs into WindowsConsole events for the requested key encoding.
/// 根据请求的按键编码将终端会话输入转换为 WindowsConsole 事件。
pub(crate) fn windows_console_input_events_from_inputs(
    inputs: &[TerminalSessionInput],
    interactive_newline: &str,
    requested_key_encoding: Option<TerminalSessionKeyEncoding>,
) -> Result<Vec<CommandSessionInputEvent>, NativeToolError> {
    match requested_key_encoding {
        None => Ok(terminal_input_events_from_inputs(inputs)),
        Some(TerminalSessionKeyEncoding::AnsiVt) => {
            let encoded = encode_terminal_inputs(
                inputs,
                interactive_newline,
                TerminalSessionKeyEncoding::AnsiVt,
            )?;
            let text = String::from_utf8(encoded).map_err(|error| {
                NativeToolError::new(format!(
                    "ANSI VT terminal input contained invalid UTF-8: {error}"
                ))
            })?;
            Ok(vec![CommandSessionInputEvent::Text { text }])
        }
        Some(TerminalSessionKeyEncoding::WindowsConsoleGetch) => Err(NativeToolError::new(
            "`windows_console_getch` is only for byte-oriented compatibility backends. Omit `key_encoding` for Windows native console sessions, or use `ansi_vt` for stdin/VT readers.",
        )),
    }
}

/// Returns the effective key encoding label reported for one send action.
/// 返回一次 send 动作报告的有效按键编码标签。
pub(crate) fn terminal_effective_key_encoding_label(
    backend: CommandSessionBackend,
    requested_key_encoding: Option<TerminalSessionKeyEncoding>,
) -> &'static str {
    if backend == CommandSessionBackend::WindowsConsole && requested_key_encoding.is_none() {
        "native_console"
    } else {
        requested_key_encoding
            .unwrap_or(TerminalSessionKeyEncoding::AnsiVt)
            .as_str()
    }
}

/// Returns the terminal sequence for one standardized key.
/// 返回一个标准化按键对应的终端序列。
pub(crate) fn terminal_key_sequence(
    key: &str,
    interactive_newline: &str,
    key_encoding: TerminalSessionKeyEncoding,
) -> Result<Vec<u8>, NativeToolError> {
    if key_encoding == TerminalSessionKeyEncoding::WindowsConsoleGetch {
        return terminal_windows_getch_key_sequence(key, interactive_newline);
    }
    match key {
        "Enter" => Ok(interactive_newline.as_bytes().to_vec()),
        "Tab" => Ok(b"\t".to_vec()),
        "Esc" | "Escape" => Ok(vec![0x1b]),
        "Backspace" => Ok(vec![0x08]),
        "Delete" => Ok(b"\x1b[3~".to_vec()),
        "ArrowUp" => Ok(b"\x1b[A".to_vec()),
        "ArrowDown" => Ok(b"\x1b[B".to_vec()),
        "ArrowRight" => Ok(b"\x1b[C".to_vec()),
        "ArrowLeft" => Ok(b"\x1b[D".to_vec()),
        "Home" => Ok(b"\x1b[H".to_vec()),
        "End" => Ok(b"\x1b[F".to_vec()),
        "PageUp" => Ok(b"\x1b[5~".to_vec()),
        "PageDown" => Ok(b"\x1b[6~".to_vec()),
        "Insert" => Ok(b"\x1b[2~".to_vec()),
        "F1" => Ok(b"\x1bOP".to_vec()),
        "F2" => Ok(b"\x1bOQ".to_vec()),
        "F3" => Ok(b"\x1bOR".to_vec()),
        "F4" => Ok(b"\x1bOS".to_vec()),
        "F5" => Ok(b"\x1b[15~".to_vec()),
        "F6" => Ok(b"\x1b[17~".to_vec()),
        "F7" => Ok(b"\x1b[18~".to_vec()),
        "F8" => Ok(b"\x1b[19~".to_vec()),
        "F9" => Ok(b"\x1b[20~".to_vec()),
        "F10" => Ok(b"\x1b[21~".to_vec()),
        "F11" => Ok(b"\x1b[23~".to_vec()),
        "F12" => Ok(b"\x1b[24~".to_vec()),
        _ => Err(NativeToolError::new(format!(
            "Unsupported terminal key `{key}`. Call skill(\"terminal-session\") and use a documented key name."
        ))),
    }
}

/// Returns the Windows console getch byte sequence for one standardized key.
/// 返回一个标准化按键对应的 Windows 控制台 getch 字节序列。
pub(crate) fn terminal_windows_getch_key_sequence(
    key: &str,
    interactive_newline: &str,
) -> Result<Vec<u8>, NativeToolError> {
    match key {
        "Enter" => Ok(interactive_newline.as_bytes().to_vec()),
        "Tab" => Ok(vec![b'\t']),
        "Esc" | "Escape" => Ok(vec![0x1b]),
        "Backspace" => Ok(vec![0x08]),
        "ArrowUp" | "ArrowDown" | "ArrowRight" | "ArrowLeft" | "Home" | "End" | "PageUp"
        | "PageDown" | "Insert" | "Delete" | "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7"
        | "F8" | "F9" | "F10" | "F11" | "F12"
            if cfg!(windows) =>
        {
            Err(NativeToolError::new(format!(
                "The current byte-oriented terminal backend cannot synthesize console extended key `{key}` for msvcrt.getch(). Use the Windows native console backend or send a documented text shortcut such as 1/2/3, Enter, Esc, or q."
            )))
        }
        "ArrowUp" => Ok(vec![0xe0, b'H']),
        "ArrowDown" => Ok(vec![0xe0, b'P']),
        "ArrowRight" => Ok(vec![0xe0, b'M']),
        "ArrowLeft" => Ok(vec![0xe0, b'K']),
        "Home" => Ok(vec![0xe0, b'G']),
        "End" => Ok(vec![0xe0, b'O']),
        "PageUp" => Ok(vec![0xe0, b'I']),
        "PageDown" => Ok(vec![0xe0, b'Q']),
        "Insert" => Ok(vec![0xe0, b'R']),
        "Delete" => Ok(vec![0xe0, b'S']),
        "F1" => Ok(vec![0x00, b';']),
        "F2" => Ok(vec![0x00, b'<']),
        "F3" => Ok(vec![0x00, b'=']),
        "F4" => Ok(vec![0x00, b'>']),
        "F5" => Ok(vec![0x00, b'?']),
        "F6" => Ok(vec![0x00, b'@']),
        "F7" => Ok(vec![0x00, b'A']),
        "F8" => Ok(vec![0x00, b'B']),
        "F9" => Ok(vec![0x00, b'C']),
        "F10" => Ok(vec![0x00, b'D']),
        "F11" => Ok(vec![0xe0, 0x85]),
        "F12" => Ok(vec![0xe0, 0x86]),
        _ => Err(NativeToolError::new(format!(
            "Unsupported Windows console getch key `{key}`. Use Enter, Tab, Esc, Backspace, or text shortcuts."
        ))),
    }
}

/// Returns the terminal sequence for one standardized key chord.
/// 返回一个标准化组合键对应的终端序列。
pub(crate) fn terminal_chord_sequence(
    key: &str,
    ctrl: bool,
    alt: bool,
    shift: bool,
    meta: bool,
    interactive_newline: &str,
    key_encoding: TerminalSessionKeyEncoding,
) -> Result<Vec<u8>, NativeToolError> {
    if shift && key == "Tab" && !ctrl && !alt && !meta {
        return Ok(b"\x1b[Z".to_vec());
    }

    let mut sequence = Vec::new();
    if alt || meta {
        sequence.push(0x1b);
    }
    if ctrl {
        let normalized = key.trim().to_ascii_lowercase();
        let mut chars = normalized.chars();
        let Some(ch) = chars.next() else {
            return Err(NativeToolError::new("Chord key cannot be empty."));
        };
        if chars.next().is_none() && ch.is_ascii_alphabetic() {
            sequence.push((ch as u8) & 0x1f);
            return Ok(sequence);
        }
        return Err(NativeToolError::new(format!(
            "Unsupported Ctrl chord `{key}`. Use a single alphabetic key such as c, d, z, l, or r."
        )));
    }
    sequence.extend_from_slice(&terminal_key_sequence(
        key,
        interactive_newline,
        key_encoding,
    )?);
    Ok(sequence)
}

/// Returns whether the input list requests a foreground-process interrupt.
/// 返回输入列表是否请求中断前台进程。
pub(crate) fn terminal_inputs_request_interrupt(inputs: &[TerminalSessionInput]) -> bool {
    inputs.iter().any(|input| {
        matches!(
            input,
            TerminalSessionInput::Chord {
                key,
                ctrl: Some(true),
                alt,
                shift,
                meta,
            } if key.eq_ignore_ascii_case("c")
                && !alt.unwrap_or(false)
                && !shift.unwrap_or(false)
                && !meta.unwrap_or(false)
        )
    })
}

/// Returns the visible label for one terminal input item.
/// 返回一条终端输入项的可见标签。
pub(crate) fn terminal_input_label(input: &TerminalSessionInput) -> String {
    match input {
        TerminalSessionInput::Text { text } => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                String::from("text")
            } else {
                trimmed.chars().take(80).collect()
            }
        }
        TerminalSessionInput::Paste { text } => {
            format!("paste {} chars", text.chars().count())
        }
        TerminalSessionInput::Key { key } => key.clone(),
        TerminalSessionInput::Chord {
            key,
            ctrl,
            alt,
            shift,
            meta,
        } => {
            let mut parts = Vec::new();
            if ctrl.unwrap_or(false) {
                parts.push("Ctrl");
            }
            if alt.unwrap_or(false) {
                parts.push("Alt");
            }
            if shift.unwrap_or(false) {
                parts.push("Shift");
            }
            if meta.unwrap_or(false) {
                parts.push("Meta");
            }
            parts.push(key.as_str());
            parts.join("+")
        }
    }
}
