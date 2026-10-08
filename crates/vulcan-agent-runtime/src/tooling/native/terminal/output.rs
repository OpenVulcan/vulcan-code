use super::*;

pub(crate) fn terminal_output_chunks_contain_pattern(
    chunks: &[CommandSessionOutputChunk],
    pattern: &str,
) -> bool {
    chunks.iter().any(|chunk| chunk.text.contains(pattern))
        || chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<String>()
            .contains(pattern)
}

pub(crate) fn terminal_events_from_chunks(chunks: &[CommandSessionOutputChunk]) -> Value {
    Value::Array(
        chunks
            .iter()
            .map(|chunk| {
                json!({
                    "seq": chunk.seq,
                    "kind": "output",
                    "stream": chunk.stream.as_str(),
                    "text": chunk.text,
                })
            })
            .collect(),
    )
}

/// Coalesces adjacent output chunks from the same stream for compact model-visible output.
/// 将同一输出流的相邻分片合并为更紧凑的模型可见输出。
pub(crate) fn coalesce_terminal_output_chunks(
    chunks: Vec<CommandSessionOutputChunk>,
) -> Vec<CommandSessionOutputChunk> {
    let mut coalesced: Vec<CommandSessionOutputChunk> = Vec::new();
    for chunk in chunks {
        if let Some(last_chunk) = coalesced.last_mut()
            && last_chunk.stream == chunk.stream
            && last_chunk.text.len().saturating_add(chunk.text.len())
                <= TERMINAL_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES
        {
            last_chunk.text.push_str(&chunk.text);
            continue;
        }
        coalesced.push(chunk);
    }
    coalesced
}

pub(crate) fn detect_terminal_output_mode(
    chunks: &[CommandSessionOutputChunk],
) -> (&'static str, f64, String) {
    let combined = chunks
        .iter()
        .map(|chunk| chunk.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    if combined.contains("\x1b[?1049h")
        || combined.contains("\x1b[?47h")
        || combined.contains("\x1b[2J")
        || combined.contains("\x1b[H")
    {
        return (
            "screen",
            0.82,
            String::from("ANSI alternate-screen or cursor-control sequence detected."),
        );
    }
    let escape_count = combined.matches('\x1b').count();
    if escape_count >= 8 {
        return (
            "mixed",
            0.64,
            String::from("Multiple ANSI control sequences detected."),
        );
    }
    (
        "log",
        0.9,
        String::from("Output appears to be append-style text."),
    )
}

/// Removes simple ANSI control sequences for screen-text fallback rendering.
/// 移除简单 ANSI 控制序列以便生成屏幕文本回退内容。
pub(crate) fn strip_simple_ansi_sequences(text: &str) -> String {
    let mut output = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\x1b' {
            output.push(ch);
            continue;
        }
        if chars.peek() == Some(&'[') {
            let _ = chars.next();
            for next in chars.by_ref() {
                if next.is_ascii_alphabetic() || matches!(next, '~' | '@') {
                    break;
                }
            }
        }
    }
    output
}

/// Returns whether a drained interrupt chunk contains model-visible content.
/// 返回中断排空分片是否包含模型应看见的内容。
pub(crate) fn terminal_interrupt_drain_chunk_is_visible(chunk: &CommandSessionOutputChunk) -> bool {
    let text_without_ansi = strip_simple_ansi_sequences(&chunk.text);
    let visible_text =
        text_without_ansi.trim_matches(|ch: char| ch.is_control() || ch.is_whitespace());
    !visible_text.is_empty()
}

/// Builds plain screen text for screen-like terminal output when available.
/// 在存在屏幕型终端输出时构建纯文本屏幕内容。
pub(crate) fn terminal_screen_text_from_chunks(
    output_mode: &str,
    chunks: &[CommandSessionOutputChunk],
) -> Option<String> {
    if output_mode != "screen" && output_mode != "mixed" {
        return None;
    }
    let screen_text = strip_simple_ansi_sequences(
        &chunks
            .iter()
            .map(|chunk| chunk.text.as_str())
            .collect::<Vec<_>>()
            .join(""),
    );
    (!screen_text.trim().is_empty()).then_some(screen_text)
}

/// Builds a compact controls hint for terminal-session host metadata.
/// 为终端会话宿主元数据构建紧凑控制提示。
pub(crate) fn terminal_controls_hint(output_mode: &str) -> Value {
    if output_mode == "screen" || output_mode == "mixed" {
        json!({
            "preferred_input": "keys",
            "text_input_supported": true,
            "keys_supported": true,
            "paste_supported": true,
            "common_keys": ["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Tab", "Enter", "Esc"],
            "common_chords": [{ "key": "c", "ctrl": true }],
            "note": "Screen-like output detected. Prefer keys/chords and snapshots for navigation."
        })
    } else {
        json!({
            "preferred_input": "text",
            "text_input_supported": true,
            "keys_supported": true,
            "paste_supported": true,
            "common_keys": ["Enter", "Tab", "ArrowUp", "ArrowDown"],
            "common_chords": [{ "key": "c", "ctrl": true }]
        })
    }
}

/// Returns a compact model-visible controls hint line for screen-like output.
/// 返回一条面向模型的紧凑屏幕控制提示。
pub(crate) fn terminal_controls_hint_line(output_mode: &str) -> Option<&'static str> {
    if output_mode == "screen" || output_mode == "mixed" {
        Some(
            "Prefer snapshot/screen_stable with key or chord inputs; avoid treating ANSI output as an append-only log.",
        )
    } else {
        None
    }
}

/// Builds backend-specific terminal input capability metadata.
/// 构建后端特定的终端输入能力元数据。
pub(crate) fn terminal_input_capabilities(snapshot: &CommandSessionSnapshot) -> Value {
    match snapshot.backend {
        CommandSessionBackend::Pty => json!({
            "text": true,
            "paste": true,
            "key": true,
            "chord": true,
            "encoding": "ansi_vt",
            "common_keys": ["Enter", "Tab", "Esc", "ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight", "Delete", "F1"],
        }),
    }
}
