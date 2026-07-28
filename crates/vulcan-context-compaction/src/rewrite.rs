//! Canonical conversation compaction rewrite helpers shared by WebUI state and persistence flows.
//! 供WebUI状态与持久化流程共享的规范会话压缩改写辅助模块。

use serde_json::Value;
use vulcan_context::protocol::{
    SessionConversationMessageIdentityError, validate_session_conversation_message_identities,
};
use vulcan_context::{
    ContextCompactionPolicy, SessionConversationMessage, SessionConversationPayload,
    SessionConversationRecord, SessionConversationState, SessionMessageKind,
    SessionMessageRecordKind, estimate_mixed_language_text_tokens,
};
use vulcan_core::types::{ConversationAttachment, ConversationPayload};

/// Number of latest canonical records retained verbatim after one compaction rewrite.
/// 一次压缩改写后按原样保留的最新规范记录条数。
pub const PROTOCOL_RECORD_COMPACTION_RETAINED_TAIL_COUNT: usize = 50;

/// Maximum body length retained for old historical records after record-level compaction.
/// 记录级压缩后为旧历史记录保留的最大正文长度。
const PROTOCOL_RECORD_COMPACTED_BODY_CHARS: usize = 12 * 1024;

/// Placeholder text used when a retained recent tool result is too large to keep verbatim after compaction.
/// 压缩后最近工具结果过大而无法原样保留时使用的占位文本。
const COMPACTED_TOOL_RESULT_PLACEHOLDER: &str = "Tool output omitted during context compaction to preserve provider protocol integrity. The full output remains available in the transcript.";

/// Function pointer used to compact one `change_set` host result into its minimal transcript-safe form.
/// 用于将一份 `change_set` 宿主结果压缩为最小可转录安全形态的函数指针。
pub type ChangeSetCompactor = fn(&Value) -> Option<Value>;

/// Message-index segment representing one API round plus its following tool responses.
/// 表示一个 API 执行轮次及其后续工具响应的消息索引段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanonicalTailSegment {
    /// Inclusive start index of the retained segment.
    /// 保留段的起始索引，包含该位置。
    start: usize,
    /// Exclusive end index of the retained segment.
    /// 保留段的结束索引，不包含该位置。
    end: usize,
}

/// Rewrites the active canonical window using one compaction summary plus the retained recent tail.
/// 使用一条压缩摘要与保留的最近尾部消息改写当前活动规范窗口。
///
/// # Returns
/// 返回值
/// Returns success after one validated atomic rewrite, or an identity error with messages and continuation unchanged.
/// 完成一次经过校验的原子改写后返回成功；身份错误时消息与 continuation 保持不变。
pub fn compact_conversation_with_summary(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_kind: SessionMessageKind,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
    policy: ContextCompactionPolicy,
) -> Result<(), SessionConversationMessageIdentityError> {
    validate_session_conversation_message_identities(&state.messages)?;
    let retained_tail = canonical_recent_tail_messages(&state.messages, policy);
    let mut compacted_messages = Vec::with_capacity(retained_tail.len() + 1);
    let summary_text = summary_body.into();
    let summary_message = SessionConversationMessage {
        id: summary_message_id.into(),
        payload: match summary_kind {
            SessionMessageKind::User => SessionConversationPayload::UserText {
                text: summary_text.clone(),
                attachments: Vec::new(),
            },
            SessionMessageKind::Assistant | SessionMessageKind::Error => {
                SessionConversationPayload::Assistant {
                    text: summary_text,
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                }
            }
        },
    };
    compacted_messages.push(summary_message);
    compacted_messages.extend(retained_tail);
    validate_session_conversation_message_identities(&compacted_messages)?;
    state.messages = compacted_messages;
    // continuation is fully reset because a rewritten canonical window invalidates every provider and local coverage proof.
    // continuation 会被完整重置，因为改写后的规范窗口会使全部 Provider 与本地覆盖证明失效。
    state.continuation = Default::default();
    state.continuation.updated_at = Some(updated_at.into());
    Ok(())
}

/// Rewrites the active canonical window using the planner's exact covered-source boundary.
/// 使用规划器给出的精确源覆盖边界改写当前活动规范窗口。
///
/// # Parameters
/// # 参数
/// `state` is the canonical conversation window to rewrite.
/// `state` 是需要改写的规范会话窗口。
/// `summary_message_id` is the hidden summary message identifier inserted as the new context anchor.
/// `summary_message_id` 是作为新上下文锚点插入的隐藏摘要消息标识。
/// `summary_kind` controls whether the synthetic summary is projected as user or assistant text.
/// `summary_kind` 控制合成摘要按用户文本还是助手文本投影。
/// `summary_body` is the durable continuation summary.
/// `summary_body` 是持久续写摘要。
/// `updated_at` is the continuation timestamp written after the rewrite.
/// `updated_at` 是改写后写入 continuation 的时间戳。
/// `source_until_message_id` is the inclusive source boundary covered by the summary.
/// `source_until_message_id` 是摘要覆盖的闭区间源边界。
/// `policy` is the fallback policy used when the boundary is absent or stale.
/// `policy` 是边界缺失或过期时使用的回退策略。
///
/// # Returns
/// 返回值
/// Returns success after one validated atomic rewrite, or an identity error with messages and continuation unchanged.
/// 完成一次经过校验的原子改写后返回成功；身份错误时消息与 continuation 保持不变。
pub fn compact_conversation_with_summary_until(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_kind: SessionMessageKind,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
    source_until_message_id: Option<&str>,
    policy: ContextCompactionPolicy,
) -> Result<(), SessionConversationMessageIdentityError> {
    validate_session_conversation_message_identities(&state.messages)?;
    let summary_text = summary_body.into();
    let summary_message = summary_conversation_message(
        summary_message_id.into(),
        summary_kind,
        summary_text.clone(),
    );
    let retained_tail = source_until_message_id
        .and_then(|source_until_message_id| {
            state
                .messages
                .iter()
                .position(|message| message.id == source_until_message_id)
                .map(|index| state.messages[index.saturating_add(1)..].to_vec())
        })
        .unwrap_or_else(|| canonical_recent_tail_messages(&state.messages, policy));
    let mut compacted_messages = Vec::with_capacity(retained_tail.len() + 1);
    compacted_messages.push(summary_message);
    compacted_messages.extend(retained_tail);
    validate_session_conversation_message_identities(&compacted_messages)?;
    state.messages = compacted_messages;
    // continuation is fully reset because a rewritten canonical window invalidates every provider and local coverage proof.
    // continuation 会被完整重置，因为改写后的规范窗口会使全部 Provider 与本地覆盖证明失效。
    state.continuation = Default::default();
    state.continuation.updated_at = Some(updated_at.into());
    Ok(())
}

/// Builds the synthetic canonical summary message inserted by one compaction rewrite.
/// 构建一次压缩改写插入的合成规范摘要消息。
fn summary_conversation_message(
    id: String,
    summary_kind: SessionMessageKind,
    summary_text: String,
) -> SessionConversationMessage {
    SessionConversationMessage {
        id,
        payload: match summary_kind {
            SessionMessageKind::User => SessionConversationPayload::UserText {
                text: summary_text,
                attachments: Vec::new(),
            },
            SessionMessageKind::Assistant | SessionMessageKind::Error => {
                SessionConversationPayload::Assistant {
                    text: summary_text,
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                }
            }
        },
    }
}

/// Compacts canonical records after summary creation so old tool details stop inflating persistence.
/// 在摘要创建后压缩规范记录，避免旧工具细节继续放大持久化体积。
pub fn compact_conversation_records_after_summary(
    state: &mut SessionConversationState,
    summary_record_id: &str,
    policy: ContextCompactionPolicy,
    compact_change_set_host_result: ChangeSetCompactor,
) {
    let retained_tail_count = (policy.tail_turns.max(1usize) * 12usize)
        .max(PROTOCOL_RECORD_COMPACTION_RETAINED_TAIL_COUNT);
    if state.records.len() <= retained_tail_count {
        return;
    }

    let tail_start = state.records.len().saturating_sub(retained_tail_count);
    let mut retained_records = Vec::new();
    let mut retained_ids = std::collections::BTreeSet::new();

    for (index, record) in state.records.iter().cloned().enumerate() {
        let keep_recent_tail = index >= tail_start;
        let keep_summary = record.id == summary_record_id;
        let keep_historical_dialogue = historical_record_survives_compaction(&record);
        if !(keep_recent_tail || keep_summary || keep_historical_dialogue) {
            continue;
        }

        let retained_record = if keep_recent_tail || keep_summary {
            record
        } else {
            compact_historical_record(record, policy.prune, compact_change_set_host_result)
        };
        if retained_ids.insert(retained_record.id.clone()) {
            retained_records.push(retained_record);
        }
    }

    state.records = retained_records;
}

/// Selects the canonical message tail retained after summary insertion without splitting API rounds.
/// 在不切断 API 执行轮次的前提下，选择摘要插入后保留的规范消息尾部。
fn canonical_recent_tail_messages(
    messages: &[SessionConversationMessage],
    policy: ContextCompactionPolicy,
) -> Vec<SessionConversationMessage> {
    if messages.is_empty() {
        return Vec::new();
    }

    let target_rounds = policy.tail_turns.max(1usize);
    let budget = policy.preserve_recent_tokens;
    let segments = canonical_api_round_segments(messages);
    let mut retained_tail = Vec::new();
    let mut retained_tokens = 0usize;
    let mut retained_rounds = 0usize;

    for segment in segments.iter().rev() {
        if retained_rounds >= target_rounds {
            break;
        }

        let remaining_budget = budget.saturating_sub(retained_tokens);
        let segment_messages = &messages[segment.start..segment.end];
        let mut candidate = segment_messages.to_vec();
        let candidate_tokens = canonical_tail_token_estimate(&candidate);
        if candidate_tokens > remaining_budget {
            candidate = canonical_messages_with_compacted_tool_results(segment_messages);
        }
        let candidate_tokens = canonical_tail_token_estimate(&candidate);
        if retained_tail.is_empty() || retained_tokens + candidate_tokens <= budget {
            retained_tokens = retained_tokens.saturating_add(candidate_tokens);
            candidate.extend(retained_tail);
            retained_tail = candidate;
            retained_rounds += 1;
        } else {
            break;
        }
    }

    if retained_tail.is_empty() {
        canonical_messages_with_compacted_tool_results(
            &messages[messages.len().saturating_sub(1)..],
        )
    } else {
        retained_tail
    }
}

/// Builds safe tail segments at assistant API-round boundaries so tool calls and tool results stay together.
/// 按 Assistant API 执行轮次边界构建安全尾部段，确保工具调用与工具结果保持在同一组内。
fn canonical_api_round_segments(
    messages: &[SessionConversationMessage],
) -> Vec<CanonicalTailSegment> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut round_starts = Vec::new();
    let mut last_assistant_round_key: Option<String> = None;
    for (index, message) in messages.iter().enumerate() {
        let Some(round_key) = canonical_assistant_round_key(message) else {
            continue;
        };
        if last_assistant_round_key.as_deref() != Some(round_key.as_str()) {
            round_starts.push(index);
            last_assistant_round_key = Some(round_key);
        }
    }

    if round_starts.is_empty() {
        return vec![CanonicalTailSegment {
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
        .map(|(position, start)| CanonicalTailSegment {
            start: *start,
            end: round_starts
                .get(position + 1)
                .copied()
                .unwrap_or(messages.len()),
        })
        .collect()
}

/// Returns the stable assistant API-round key for one canonical message.
/// 返回一条规范消息的稳定 Assistant API 执行轮次键。
fn canonical_assistant_round_key(message: &SessionConversationMessage) -> Option<String> {
    let SessionConversationPayload::Assistant {
        provider_message_id,
        ..
    } = &message.payload
    else {
        return None;
    };
    provider_message_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(message.id.clone()))
}

/// Returns a cloned message slice where tool results are compacted into protocol-safe placeholders.
/// 返回一份克隆后的消息切片，其中工具结果被压缩为协议安全的占位文本。
fn canonical_messages_with_compacted_tool_results(
    messages: &[SessionConversationMessage],
) -> Vec<SessionConversationMessage> {
    messages
        .iter()
        .cloned()
        .map(|mut message| {
            if let SessionConversationPayload::ToolResult { text, .. } = &mut message.payload {
                *text = COMPACTED_TOOL_RESULT_PLACEHOLDER.to_string();
            }
            message
        })
        .collect()
}

/// Estimates the canonical tail size with the same conservative shape used by compaction retention.
/// 使用与压缩保留一致的保守形态估算规范尾部大小。
fn canonical_tail_token_estimate(messages: &[SessionConversationMessage]) -> usize {
    messages
        .iter()
        .map(canonical_message_token_estimate)
        .sum::<usize>()
}

/// Estimates one canonical message without consulting provider-specific tokenizers.
/// 在不依赖 Provider 专用 tokenizer 的情况下估算一条规范消息。
fn canonical_message_token_estimate(message: &SessionConversationMessage) -> usize {
    match &message.payload {
        SessionConversationPayload::UserText { text, attachments } => {
            estimate_compaction_text_tokens(text)
                + attachments
                    .iter()
                    .map(canonical_attachment_token_estimate)
                    .sum::<usize>()
                + 12usize
        }
        SessionConversationPayload::Assistant {
            text,
            reasoning,
            tool_calls,
            ..
        } => {
            estimate_compaction_text_tokens(text)
                + estimate_compaction_text_tokens(reasoning)
                + tool_calls
                    .iter()
                    .map(|tool_call| {
                        estimate_compaction_text_tokens(&tool_call.tool_name)
                            + serde_json::to_string(&tool_call.arguments)
                                .map(|arguments| estimate_compaction_text_tokens(&arguments))
                                .unwrap_or_default()
                            + 24usize
                    })
                    .sum::<usize>()
                + 16usize
        }
        SessionConversationPayload::ToolResult { text, .. } => {
            estimate_compaction_text_tokens(text) + 12usize
        }
        SessionConversationPayload::AgentMessage {
            message_type,
            task_name,
            sender,
            payload,
            ..
        } => {
            let text = ConversationPayload::render_agent_message_text(
                message_type,
                task_name,
                sender,
                payload,
            );
            estimate_compaction_text_tokens(&text) + 16usize
        }
    }
}

/// Estimates one attachment placeholder for compaction-retention decisions.
/// 为压缩保留决策估算一个附件占位成本。
fn canonical_attachment_token_estimate(attachment: &ConversationAttachment) -> usize {
    let kind_reserve = match attachment.kind {
        vulcan_core::types::ConversationAttachmentKind::Audio => 512usize,
        vulcan_core::types::ConversationAttachmentKind::Image => 384usize,
        vulcan_core::types::ConversationAttachmentKind::Video => 1024usize,
        vulcan_core::types::ConversationAttachmentKind::Pdf => 768usize,
    };
    kind_reserve
        + estimate_compaction_text_tokens(&attachment.file_name)
        + estimate_compaction_text_tokens(&attachment.mime_type)
}

/// Estimates text tokens with the local lightweight compaction heuristic.
/// 使用本地轻量压缩启发式估算文本 Token。
fn estimate_compaction_text_tokens(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    estimate_mixed_language_text_tokens(trimmed).max(1usize)
}

/// Returns whether one older record should survive record-level compaction.
/// 返回一条旧记录是否应在记录级压缩后保留。
fn historical_record_survives_compaction(record: &SessionConversationRecord) -> bool {
    matches!(record.kind, SessionMessageKind::User)
        || matches!(
            record.record_kind,
            SessionMessageRecordKind::AssistantOutput | SessionMessageRecordKind::CompactionSummary
        )
        || generated_attachment_tool_result_should_survive_compaction(record)
        || change_set_tool_result_should_survive_compaction(record)
}

/// Reduces an old surviving dialogue record to compact text and minimal metadata.
/// 将一条保留下来的旧对话记录降级为紧凑正文与最小元数据。
fn compact_historical_record(
    mut record: SessionConversationRecord,
    prune_tools: bool,
    compact_change_set_host_result: ChangeSetCompactor,
) -> SessionConversationRecord {
    if generated_attachment_tool_result_should_survive_compaction(&record) {
        record.body.clear();
        record.metadata = compact_generated_attachment_tool_result_metadata(record.metadata.take());
        return record;
    }
    if change_set_tool_result_should_survive_compaction(&record) {
        record.body.clear();
        record.metadata = compact_change_set_tool_result_metadata(
            record.metadata.take(),
            compact_change_set_host_result,
        );
        return record;
    }
    record.body = if prune_tools
        && matches!(
            record.record_kind,
            SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
        ) {
        String::from(
            "Tool output was compacted. Re-run or open the original session detail if needed.",
        )
    } else {
        truncate_compacted_record_body(&record.body)
    };
    record.metadata = None;
    record
}

/// Returns whether one canonical tool-result record carries generated-media metadata that must remain transcript-visible after compaction.
/// 返回一条规范工具结果记录是否携带必须在压缩后继续保留为转录可见的生成媒体元数据。
fn generated_attachment_tool_result_should_survive_compaction(
    record: &SessionConversationRecord,
) -> bool {
    matches!(
        record.record_kind,
        SessionMessageRecordKind::ToolResultOutput
    ) && record
        .metadata
        .as_ref()
        .and_then(resolve_generated_attachment_host_result_from_metadata)
        .is_some()
}

/// Returns whether one canonical tool-result record carries `change_set` metadata that must remain transcript-visible after compaction.
/// 返回一条规范工具结果记录是否携带必须在压缩后继续保留为转录可见的 `change_set` 元数据。
fn change_set_tool_result_should_survive_compaction(record: &SessionConversationRecord) -> bool {
    matches!(
        record.record_kind,
        SessionMessageRecordKind::ToolResultOutput
    ) && record
        .metadata
        .as_ref()
        .and_then(resolve_change_set_host_result_from_metadata)
        .is_some()
}

/// Resolves the generated-attachments host-result payload from transcript metadata, accepting nested and flattened shapes.
/// 从转录元数据中解析 generated-attachments 宿主结果载荷，同时兼容嵌套与扁平两种结构。
fn resolve_generated_attachment_host_result_from_metadata(metadata: &Value) -> Option<&Value> {
    let nested_host_result = metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .filter(|value| value.get("kind").and_then(Value::as_str) == Some("generated_attachments"));
    nested_host_result.or_else(|| {
        (metadata.get("kind").and_then(Value::as_str) == Some("generated_attachments")
            && metadata.get("payload").is_some())
        .then_some(metadata)
    })
}

/// Resolves the `change_set` host-result payload from transcript metadata, accepting nested and flattened shapes.
/// 从转录元数据中解析 `change_set` 宿主结果载荷，同时兼容嵌套与扁平两种结构。
fn resolve_change_set_host_result_from_metadata(metadata: &Value) -> Option<&Value> {
    let nested_host_result = metadata
        .get("host_result")
        .filter(|value| value.is_object())
        .filter(|value| value.get("kind").and_then(Value::as_str) == Some("change_set"));
    nested_host_result.or_else(|| {
        (metadata.get("kind").and_then(Value::as_str) == Some("change_set")
            && metadata.get("payload").is_some())
        .then_some(metadata)
    })
}

/// Compacts one generated-media tool-result metadata object down to the minimum host payload required to rebuild inline transcript cards later.
/// 将一份生成媒体工具结果元数据压缩到后续重建内联转录卡片所需的最小宿主载荷。
fn compact_generated_attachment_tool_result_metadata(metadata: Option<Value>) -> Option<Value> {
    let metadata = metadata?;
    let host_result = resolve_generated_attachment_host_result_from_metadata(&metadata)?;
    let compact_host_result = compact_generated_attachment_host_result(host_result)?;
    let mut metadata_object = serde_json::Map::new();
    metadata_object.insert(String::from("host_result"), compact_host_result);
    Some(Value::Object(metadata_object))
}

/// Compacts one `change_set` tool-result metadata object down to the minimum host payload required to rebuild inline change summaries later.
/// 将一份 `change_set` 工具结果元数据压缩到后续重建内联变更摘要所需的最小宿主载荷。
fn compact_change_set_tool_result_metadata(
    metadata: Option<Value>,
    compact_change_set_host_result: ChangeSetCompactor,
) -> Option<Value> {
    let metadata = metadata?;
    let host_result = resolve_change_set_host_result_from_metadata(&metadata)?;
    let compact_host_result = compact_change_set_host_result(host_result)?;
    let mut metadata_object = serde_json::Map::new();
    metadata_object.insert(String::from("host_result"), compact_host_result);
    Some(Value::Object(metadata_object))
}

/// Compacts one generated-attachments host-result payload to the minimal structure required by transcript media rendering and host actions.
/// 将一份 generated-attachments 宿主结果载荷压缩到转录媒体渲染与宿主动作所需的最小结构。
fn compact_generated_attachment_host_result(host_result: &Value) -> Option<Value> {
    let outputs = host_result
        .get("payload")
        .and_then(|payload| payload.get("outputs"))
        .and_then(Value::as_array)?;
    let compact_outputs = outputs
        .iter()
        .filter_map(compact_generated_attachment_output)
        .collect::<Vec<_>>();
    if compact_outputs.is_empty() {
        return None;
    }

    let mut payload_object = serde_json::Map::new();
    payload_object.insert(String::from("outputs"), Value::Array(compact_outputs));

    let mut host_result_object = serde_json::Map::new();
    host_result_object.insert(
        String::from("kind"),
        Value::String(String::from("generated_attachments")),
    );
    host_result_object.insert(String::from("payload"), Value::Object(payload_object));
    Some(Value::Object(host_result_object))
}

/// Compacts one generated attachment output to the fields required by transcript restoration, previews, and file actions.
/// 将单条生成附件输出压缩到转录恢复、预览显示与文件动作所需的字段集合。
fn compact_generated_attachment_output(output: &Value) -> Option<Value> {
    let output_object = output.as_object()?;
    let kind = output_object
        .get("kind")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let file_name = output_object
        .get("file_name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let local_path = output_object
        .get("local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;

    let mut compact_output = serde_json::Map::new();
    compact_output.insert(String::from("kind"), Value::String(kind.to_string()));
    compact_output.insert(
        String::from("file_name"),
        Value::String(file_name.to_string()),
    );
    compact_output.insert(
        String::from("local_path"),
        Value::String(local_path.to_string()),
    );

    if let Some(mime_type) = output_object
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("mime_type"),
            Value::String(mime_type.to_string()),
        );
    }
    if let Some(byte_len) = output_object.get("byte_len").cloned() {
        compact_output.insert(String::from("byte_len"), byte_len);
    }
    if let Some(source_path) = output_object
        .get("source_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("source_path"),
            Value::String(source_path.to_string()),
        );
    }
    if let Some(preview_local_path) = output_object
        .get("preview_local_path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("preview_local_path"),
            Value::String(preview_local_path.to_string()),
        );
    }
    if let Some(asset_id) = output_object
        .get("asset_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        compact_output.insert(
            String::from("asset_id"),
            Value::String(asset_id.to_string()),
        );
    }

    Some(Value::Object(compact_output))
}

/// Truncates one old record body to keep reconstructed history useful but bounded.
/// 截断旧记录正文，使重建历史保持有用但大小有界。
fn truncate_compacted_record_body(body: &str) -> String {
    if body.chars().count() <= PROTOCOL_RECORD_COMPACTED_BODY_CHARS {
        return body.to_string();
    }

    let truncated = body
        .chars()
        .take(PROTOCOL_RECORD_COMPACTED_BODY_CHARS.saturating_sub(1))
        .collect::<String>();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    //! Rewrite regression tests covering retained tool turns and transcript-safe metadata preservation.
    //! 覆盖保留工具轮次与转录安全元数据保留的改写回归测试。

    use super::*;
    use serde_json::json;
    use vulcan_context::SessionConversationToolCall;

    /// Builds one compact `change_set` host-result payload used by rewrite tests.
    /// 构建供改写测试使用的紧凑 `change_set` 宿主结果载荷。
    fn test_compact_change_set_host_result(host_result: &Value) -> Option<Value> {
        let files = host_result
            .get("payload")
            .and_then(|payload| payload.get("files"))
            .and_then(Value::as_array)?;
        let file_count = files.len() as u64;

        Some(json!({
            "kind": "change_set",
            "payload": {
                "stats": {
                    "file_count": file_count
                },
                "verification": {
                    "status": "skipped"
                }
            }
        }))
    }

    /// Verifies planner source boundaries control the retained canonical tail exactly.
    /// 验证规划器源边界会精确控制保留的规范尾部。
    #[test]
    fn compact_conversation_with_summary_until_keeps_messages_after_source_boundary() {
        let mut state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("old-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("旧问题"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("old-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("旧回答"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("tail-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("继续任务"),
                        attachments: Vec::new(),
                    },
                },
            ],
            ..SessionConversationState::default()
        };

        compact_conversation_with_summary_until(
            &mut state,
            "summary-1",
            SessionMessageKind::Assistant,
            "旧上下文摘要",
            "1780000000000",
            Some("old-assistant"),
            ContextCompactionPolicy::default(),
        )
        .expect("valid compacted message identities");

        let ids = state
            .messages
            .iter()
            .map(|message| message.id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["summary-1", "tail-user"]);
    }

    /// Verifies compaction keeps the newest tool-call turn structurally replayable even when the result is large.
    /// 验证压缩在工具结果很大时仍会保持最新工具调用轮次的可回放结构。
    #[test]
    fn compact_conversation_keeps_tool_turn_intact_when_tail_is_large() {
        let mut state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("old-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("旧问题"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("old-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("旧回答"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("recent-user"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("读取文件"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("recent-assistant"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::new(),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: vec![SessionConversationToolCall {
                            id: String::from("tool-call-local"),
                            tool_call_id: Some(String::from("tool-call-provider")),
                            tool_name: String::from("read_file"),
                            arguments: json!({"path": "Cargo.toml"}),
                        }],
                    },
                },
                SessionConversationMessage {
                    id: String::from("recent-tool-result"),
                    payload: SessionConversationPayload::ToolResult {
                        tool_call_id: Some(String::from("tool-call-provider")),
                        is_error: false,
                        text: "很大的工具输出".repeat(200),
                    },
                },
            ],
            records: Vec::new(),
            continuation: Default::default(),
        };
        let policy = ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 16,
            ..ContextCompactionPolicy::default()
        };

        compact_conversation_with_summary(
            &mut state,
            "summary",
            SessionMessageKind::Assistant,
            "摘要",
            "2",
            policy,
        )
        .expect("valid compacted message identities");

        assert_eq!(state.messages.len(), 3);
        assert_eq!(state.messages[0].id, "summary");
        assert!(matches!(
            &state.messages[1].payload,
            SessionConversationPayload::Assistant { tool_calls, .. } if tool_calls.len() == 1
        ));
        assert!(matches!(
            &state.messages[2].payload,
            SessionConversationPayload::ToolResult { text, .. }
                if text == COMPACTED_TOOL_RESULT_PLACEHOLDER
        ));
    }

    /// Verifies record-level compaction keeps generated-media tool results visible with the minimal host metadata needed to rebuild inline cards later.
    /// 验证记录级压缩会保留生成媒体工具结果的可见性，并保留后续重建内联卡片所需的最小宿主元数据。
    #[test]
    fn record_compaction_preserves_generated_media_tool_result_metadata() {
        let mut records = vec![SessionConversationRecord {
            id: String::from("tool-result-media-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 image_generation 工具"),
            body: String::from("## 工具返回\n\n媒体已生成。"),
            metadata: Some(json!({
                "tool_call_id": "call-media-1",
                "host_result": {
                    "kind": "generated_attachments",
                    "payload": {
                        "outputs": [{
                            "kind": "image",
                            "file_name": "demo.jpg",
                            "local_path": r"D:\demo.jpg",
                            "mime_type": "image/jpeg",
                            "byte_len": 1024,
                            "asset_id": "asset-1",
                            "ignored": "drop-me"
                        }]
                    }
                },
                "noise": {
                    "large": "drop-me-too"
                }
            })),
            created_at: String::from("1"),
        }];
        for index in 0..50 {
            records.push(SessionConversationRecord {
                id: format!("recent-{index}"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::new(),
                body: format!("最近消息 {index}"),
                metadata: None,
                created_at: (index + 2).to_string(),
            });
        }

        let mut state = SessionConversationState {
            messages: Vec::new(),
            records,
            continuation: Default::default(),
        };
        compact_conversation_records_after_summary(
            &mut state,
            "missing-summary",
            ContextCompactionPolicy {
                tail_turns: 1,
                ..ContextCompactionPolicy::default()
            },
            test_compact_change_set_host_result,
        );

        assert_eq!(state.records.len(), 51);
        let preserved_record = state
            .records
            .iter()
            .find(|record| record.id == "tool-result-media-1")
            .expect("generated-media tool result should survive compaction");
        assert!(preserved_record.body.is_empty());
        let metadata = preserved_record
            .metadata
            .as_ref()
            .expect("generated-media tool result should preserve metadata");
        let outputs = metadata
            .get("host_result")
            .and_then(|host_result| host_result.get("payload"))
            .and_then(|payload| payload.get("outputs"))
            .and_then(Value::as_array)
            .expect("generated-media outputs should remain available");
        assert_eq!(outputs.len(), 1);
        assert_eq!(
            outputs[0].get("kind").and_then(Value::as_str),
            Some("image")
        );
        assert_eq!(
            outputs[0].get("file_name").and_then(Value::as_str),
            Some("demo.jpg")
        );
        assert_eq!(
            outputs[0].get("local_path").and_then(Value::as_str),
            Some(r"D:\demo.jpg")
        );
        assert!(outputs[0].get("ignored").is_none());
        assert!(metadata.get("noise").is_none());
    }

    /// Verifies record-level compaction keeps `change_set` tool results visible with the compact host payload needed to rebuild inline file-change summaries later.
    /// 验证记录级压缩会保留 `change_set` 工具结果的可见性，并保留后续重建内联文件变更摘要所需的紧凑宿主载荷。
    #[test]
    fn record_compaction_preserves_change_set_tool_result_metadata() {
        let mut records = vec![SessionConversationRecord {
            id: String::from("tool-result-change-set-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("调用 write_file 工具"),
            body: String::from("## 工具返回\n\n文件变更已应用。"),
            metadata: Some(json!({
                "tool_call_id": "call-change-set-1",
                "host_result": {
                    "kind": "change_set",
                    "payload": {
                        "mode": "preview",
                        "summary": "Previewed file edit.",
                        "files": [{
                            "change": "create",
                            "path": r"D:\openvulcan\vulcan-code\src\demo.lua",
                            "content": "return {}\n"
                        }]
                    }
                },
                "noise": {
                    "large": "drop-me-too"
                }
            })),
            created_at: String::from("1"),
        }];
        for index in 0..50 {
            records.push(SessionConversationRecord {
                id: format!("recent-{index}"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::new(),
                body: format!("最近消息 {index}"),
                metadata: None,
                created_at: (index + 2).to_string(),
            });
        }

        let mut state = SessionConversationState {
            messages: Vec::new(),
            records,
            continuation: Default::default(),
        };
        compact_conversation_records_after_summary(
            &mut state,
            "missing-summary",
            ContextCompactionPolicy {
                tail_turns: 1,
                ..ContextCompactionPolicy::default()
            },
            test_compact_change_set_host_result,
        );

        assert_eq!(state.records.len(), 51);
        let metadata = state
            .records
            .iter()
            .find(|record| record.id == "tool-result-change-set-1")
            .and_then(|record| record.metadata.as_ref())
            .expect("change-set tool result should preserve metadata");
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("kind"))
                .and_then(Value::as_str),
            Some("change_set")
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("stats"))
                .and_then(|stats| stats.get("file_count"))
                .and_then(Value::as_u64),
            Some(1)
        );
        assert_eq!(
            metadata
                .get("host_result")
                .and_then(|host_result| host_result.get("payload"))
                .and_then(|payload| payload.get("verification"))
                .and_then(|verification| verification.get("status"))
                .and_then(Value::as_str),
            Some("skipped")
        );
        assert!(metadata.get("noise").is_none());
    }

    /// Verifies a summary identity collision leaves messages and continuation completely unchanged.
    /// 验证摘要身份冲突会使消息与 continuation 完全保持不变。
    #[test]
    fn compaction_rewrite_is_atomic_on_summary_identity_collision() {
        // state retains `summary-1` after the selected source boundary, creating an exact collision.
        // state 会在所选源边界后保留 `summary-1`，从而形成准确冲突。
        let mut state = SessionConversationState {
            messages: vec![
                SessionConversationMessage {
                    id: String::from("source-1"),
                    payload: SessionConversationPayload::UserText {
                        text: String::from("source"),
                        attachments: Vec::new(),
                    },
                },
                SessionConversationMessage {
                    id: String::from("summary-1"),
                    payload: SessionConversationPayload::Assistant {
                        text: String::from("retained"),
                        reasoning: String::new(),
                        provider_message_id: None,
                        provider_message_state: None,
                        usage: None,
                        tool_calls: Vec::new(),
                    },
                },
            ],
            records: Vec::new(),
            continuation: vulcan_context::SessionProviderContinuationState {
                provider_id: Some(String::from("provider")),
                model_id: Some(String::from("model")),
                last_provider_message_id: Some(String::from("response-1")),
                updated_at: Some(String::from("before")),
                ..vulcan_context::SessionProviderContinuationState::default()
            },
        };
        // original_state captures both the message window and continuation before validation.
        // original_state 捕获校验前的消息窗口与 continuation。
        let original_state = serde_json::to_value(&state).expect("state should serialize");

        // error proves the candidate window is rejected before either canonical field is assigned.
        // error 证明候选窗口会在任何规范字段赋值前被拒绝。
        let error = compact_conversation_with_summary_until(
            &mut state,
            "summary-1",
            SessionMessageKind::Assistant,
            "compacted",
            "after",
            Some("source-1"),
            ContextCompactionPolicy::default(),
        )
        .expect_err("summary identity collision must fail");

        assert!(matches!(
            error,
            SessionConversationMessageIdentityError::Duplicate {
                message_index: 1,
                ..
            }
        ));
        assert_eq!(
            serde_json::to_value(&state).expect("state should serialize after failure"),
            original_state
        );
    }
}
