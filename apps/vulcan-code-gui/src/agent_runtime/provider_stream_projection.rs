//! Provider stream projection helpers for event mapping and output normalization.
//! 面向事件映射与输出规范化的 Provider 流式投影辅助模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::model::SessionId;
use vulcan_core::types::{
    CoreEvent as CoreControllerEvent, CoreEventKind as CoreControllerEventKind,
};

/// Returns whether one provider runtime event represents a real tool boundary that should seal the current floating thinking phase.
/// 返回某条 Provider 运行时事件是否代表真实工具边界，从而应封口当前浮动思考阶段。
pub(crate) fn is_provider_tool_boundary_runtime_event(event: &RuntimeEvent) -> bool {
    match event.kind {
        RuntimeEventKind::ToolStarted => event.title.starts_with("工具调用 · "),
        RuntimeEventKind::ToolFinished => event.title.starts_with("工具返回 · "),
        _ => false,
    }
}

/// Converts the currently accumulated thinking phase into one completion event exactly once when visible content exists.
/// 当当前累计的思考阶段存在可见内容时，将其恰好一次转换为完成事件。
pub(crate) fn take_current_thinking_phase_completed_event(
    session_id: SessionId,
    thinking_visible_output: &str,
    thinking_completed_emitted: &mut bool,
) -> Option<ExecutionStreamEvent> {
    if *thinking_completed_emitted || thinking_visible_output.trim().is_empty() {
        return None;
    }

    *thinking_completed_emitted = true;
    Some(ExecutionStreamEvent::ThinkingCompleted { session_id })
}

/// Clears the current thinking-phase buffers after one tool boundary so later reasoning starts a fresh floating segment.
/// 在一次工具边界后清空当前思考阶段缓冲，使后续推理从新的浮动片段开始。
pub(crate) fn reset_current_thinking_phase_tracking(
    thinking_raw_output: &mut String,
    thinking_visible_output: &mut String,
    thinking_completed_emitted: &mut bool,
) {
    thinking_raw_output.clear();
    thinking_visible_output.clear();
    *thinking_completed_emitted = false;
}

/// Converts one runtime event into one controller-owned event for execution tracing and later audit projection.
/// 将一条运行时事件转换为一条控制器自有事件，供执行追踪与后续审计投影使用。
pub(crate) fn runtime_event_to_core_event(event: &RuntimeEvent) -> Option<CoreControllerEvent> {
    let kind = match event.kind {
        RuntimeEventKind::ToolStarted => CoreControllerEventKind::ToolStarted,
        RuntimeEventKind::ToolFinished => CoreControllerEventKind::ToolFinished,
        _ => return None,
    };

    Some(CoreControllerEvent {
        kind,
        title: event.title.clone(),
        body: event.detail.clone(),
        metadata: event.metadata.clone(),
    })
}

/// Normalizes assistant output so empty or whitespace-only responses become one readable fallback.
/// 规范化助手输出，确保空白响应会被替换成一条可读的回退文本。
pub(crate) fn normalize_assistant_output(output: String) -> String {
    let trimmed = output.trim();

    if trimmed.is_empty() {
        String::from("模型未返回可见文本输出。")
    } else {
        trimmed.to_string()
    }
}

/// Normalizes one optional streamed text buffer so blank values become `None`.
/// 规范化一段可选流式文本缓冲，使纯空白值转换为 `None`。
pub(crate) fn normalize_optional_stream_output(output: String) -> Option<String> {
    let trimmed = output.trim();

    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Classifies whether the current raw thinking buffer is still pending one synthetic preamble match, has fully matched it, or should be preserved verbatim.
/// 判断当前原始思考缓冲是仍在等待合成前导语完整匹配、已经完整命中该前导语，还是应按原样保留。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntheticThinkingPreambleState {
    /// The current buffer only matches a prefix of the synthetic preamble and should not emit visible content yet.
    /// 当前缓冲仅匹配到合成前导语的前缀，因此暂时不应产出可见内容。
    Pending,
    /// The synthetic preamble has been fully matched and visible content should start from the provided byte offset.
    /// 合成前导语已经被完整匹配，可见内容应从给定字节偏移开始。
    Matched { visible_start_byte: usize },
    /// The current buffer is not the synthetic preamble and should be preserved exactly as received.
    /// 当前缓冲并非合成前导语，应按接收结果原样保留。
    Rejected,
}

/// Normalizes one streamed thinking buffer so provider-added synthetic preamble text never enters transcript storage or copies.
/// 规范化一段流式思考缓冲，使 Provider 额外插入的合成前导语不会进入转录存储或复制内容。
pub(crate) fn normalize_streamed_thinking_output(raw_output: &str) -> String {
    match classify_synthetic_thinking_preamble(raw_output) {
        SyntheticThinkingPreambleState::Pending => String::new(),
        SyntheticThinkingPreambleState::Matched { visible_start_byte } => {
            raw_output[visible_start_byte..].to_string()
        }
        SyntheticThinkingPreambleState::Rejected => raw_output.to_string(),
    }
}

/// Detects whether the current raw thinking buffer starts with the synthetic "Here's a thinking process:" preamble emitted by some providers.
/// 检测当前原始思考缓冲是否以部分 Provider 额外插入的 “Here's a thinking process:” 合成前导语开头。
pub(crate) fn classify_synthetic_thinking_preamble(
    raw_output: &str,
) -> SyntheticThinkingPreambleState {
    let expected = "here's a thinking process:";
    let mut characters = raw_output.char_indices().peekable();

    while let Some((_, character)) = characters.peek().copied() {
        if matches!(character, ' ' | '\t' | '\r' | '\n' | '·' | '•') {
            characters.next();
        } else {
            break;
        }
    }

    for expected_character in expected.chars() {
        let Some((_, actual_character)) = characters.next() else {
            return SyntheticThinkingPreambleState::Pending;
        };

        if normalize_synthetic_thinking_preamble_character(actual_character)
            != Some(expected_character)
        {
            return SyntheticThinkingPreambleState::Rejected;
        }
    }

    let mut visible_start_byte = characters
        .peek()
        .map(|(byte_index, _)| *byte_index)
        .unwrap_or(raw_output.len());

    while let Some((byte_index, character)) = characters.peek().copied() {
        if matches!(character, ' ' | '\t' | '·' | '•') {
            characters.next();
            visible_start_byte = byte_index + character.len_utf8();
        } else {
            break;
        }
    }

    while let Some((byte_index, character)) = characters.peek().copied() {
        if matches!(character, '\r' | '\n' | ' ' | '\t') {
            characters.next();
            visible_start_byte = byte_index + character.len_utf8();
        } else {
            break;
        }
    }

    SyntheticThinkingPreambleState::Matched { visible_start_byte }
}

/// Normalizes one candidate preamble character into the lowercase comparable form expected by the synthetic-thinking matcher.
/// 将一个候选前导语字符规范化为合成思考匹配器期望的小写可比较形式。
pub(crate) fn normalize_synthetic_thinking_preamble_character(character: char) -> Option<char> {
    match character {
        '’' => Some('\''),
        '\'' | ':' | ' ' => Some(character),
        _ if character.is_ascii() => Some(character.to_ascii_lowercase()),
        _ => None,
    }
}

/// Truncates one preview string while preserving a readable Unicode-safe suffix.
/// 在保留可读 Unicode 后缀的前提下截断一段预览文本。
pub(crate) fn truncate_preview(value: &str, max_chars: usize) -> String {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        compact
    } else {
        let truncated = compact
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}…")
    }
}
