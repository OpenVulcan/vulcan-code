use serde_json::Value;
use vulcan_context::{
    ContextCompactionPolicy, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
    estimate_mixed_language_text_tokens,
};
pub(super) fn recent_tail_start_index(
    messages: &[SessionMessageEntry],
    policy: ContextCompactionPolicy,
) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let target_rounds = policy.tail_turns.max(1usize);
    let segments = session_api_round_segments(messages);
    let mut retained_start = messages.len().saturating_sub(1);
    let mut retained_tokens = 0usize;
    let mut retained_rounds = 0usize;

    for segment in segments.iter().rev() {
        if retained_rounds >= target_rounds {
            break;
        }
        let segment_tokens = estimated_session_tail_tokens(&messages[segment.start..segment.end]);
        if retained_rounds == 0 || retained_tokens + segment_tokens <= policy.preserve_recent_tokens
        {
            retained_start = segment.start;
            retained_tokens = retained_tokens.saturating_add(segment_tokens);
            retained_rounds += 1;
        } else {
            break;
        }
    }

    retained_start
}

/// Transcript-index segment representing one API round plus its following tool-visible entries.
/// 表示一个 API 执行轮次及其后续工具可见条目的转录索引段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SessionTailSegment {
    /// Inclusive segment start index.
    /// 段起始索引，包含该位置。
    pub(super) start: usize,
    /// Exclusive segment end index.
    /// 段结束索引，不包含该位置。
    pub(super) end: usize,
}

/// Builds transcript tail segments at assistant API-round boundaries.
/// 按 Assistant API 执行轮次边界构建转录尾部段。
pub(super) fn session_api_round_segments(
    messages: &[SessionMessageEntry],
) -> Vec<SessionTailSegment> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut round_starts = Vec::new();
    let mut last_round_key: Option<String> = None;
    for (index, message) in messages.iter().enumerate() {
        let Some(round_key) = message_api_round_key(message) else {
            continue;
        };
        if last_round_key.as_deref() != Some(round_key.as_str()) {
            round_starts.push(index);
            last_round_key = Some(round_key);
        }
    }
    if round_starts.is_empty() {
        return vec![SessionTailSegment {
            start: 0,
            end: messages.len(),
        }];
    }
    if round_starts[0] != 0 {
        round_starts.insert(0, 0);
    }

    round_starts
        .iter()
        .enumerate()
        .map(|(position, start)| SessionTailSegment {
            start: *start,
            end: round_starts
                .get(position + 1)
                .copied()
                .unwrap_or(messages.len()),
        })
        .collect()
}

/// Returns the stable API-round key for one assistant transcript entry.
/// 返回一条 Assistant 转录项的稳定 API 执行轮次键。
fn message_api_round_key(message: &SessionMessageEntry) -> Option<String> {
    if message.kind != SessionMessageKind::Assistant
        || !matches!(
            message.record_kind,
            SessionMessageRecordKind::AssistantOutput | SessionMessageRecordKind::ToolCallOutput
        )
    {
        return None;
    }
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("provider_message_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(message.id.clone()))
}

/// Estimates the approximate token footprint of retained transcript entries.
/// 估算保留转录项的近似 Token 占用。
pub(super) fn estimated_session_tail_tokens(messages: &[SessionMessageEntry]) -> usize {
    messages.iter().map(estimate_session_message_tokens).sum()
}

/// Estimates one transcript entry with a lightweight text heuristic.
/// 使用轻量文本启发式估算一条转录项。
fn estimate_session_message_tokens(message: &SessionMessageEntry) -> usize {
    estimate_text_tokens(&message.title)
        + estimate_text_tokens(&message.body)
        + message
            .metadata
            .as_ref()
            .and_then(|metadata| serde_json::to_string(metadata).ok())
            .map(|metadata| estimate_text_tokens(&metadata))
            .unwrap_or_default()
        + 16usize
}

/// Estimates plain text using the same character heuristic as the context budget path.
/// 使用与上下文预算路径一致的字符启发式估算纯文本。
fn estimate_text_tokens(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    estimate_mixed_language_text_tokens(trimmed).max(1usize)
}
