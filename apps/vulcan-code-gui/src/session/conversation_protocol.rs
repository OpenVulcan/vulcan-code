//! Canonical conversation protocol models shared by the GUI, persistence, and runtime bridge.
//! 供图形界面、持久化层与运行时桥接共享的规范会话协议模型。

use crate::context_budget::ContextCompactionPolicy;
use crate::model::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_core::types::{ConversationAttachment, ProviderUsageSnapshot};

/// Number of latest canonical records retained verbatim after one compaction rewrite.
/// 一次压缩改写后按原样保留的最新规范记录条数。
pub const PROTOCOL_RECORD_COMPACTION_RETAINED_TAIL_COUNT: usize = 50;

/// Maximum body length retained for old historical records after record-level compaction.
/// 记录级压缩后为旧历史记录保留的最大正文长度。
const PROTOCOL_RECORD_COMPACTED_BODY_CHARS: usize = 12 * 1024;

/// Placeholder text used when a retained recent tool result is too large to keep verbatim after compaction.
/// 压缩后最近工具结果过大而无法原样保留时使用的占位文本。
const COMPACTED_TOOL_RESULT_PLACEHOLDER: &str = "Tool output omitted during context compaction to preserve provider protocol integrity. The full output remains available in the transcript.";

/// Message-index segment representing one user turn plus its following assistant/tool responses.
/// 表示一个用户轮次及其后续助手/工具响应的消息索引段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanonicalTailSegment {
    /// Inclusive start index of the retained segment.
    /// 保留段的起始索引，包含该位置。
    start: usize,
    /// Exclusive end index of the retained segment.
    /// 保留段的结束索引，不包含该位置。
    end: usize,
}

/// Session-scoped canonical conversation state persisted independently from the visible transcript.
/// 独立于可见转录流持久化保存的按会话归属规范会话状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionConversationState {
    /// Active canonical message window that will be forwarded to the next runtime call.
    /// 将被转发给下一次运行时调用的活动规范消息窗口。
    pub messages: Vec<SessionConversationMessage>,
    /// Canonical session record stream retained as the single source for transcript and hidden-node projection.
    /// 作为转录与隐藏节点投影唯一真相源保留的规范会话记录流。
    #[serde(default)]
    pub records: Vec<SessionConversationRecord>,
    /// Provider-scoped continuation cursor snapshot prepared for future request chaining.
    /// 为未来请求串联准备的按 Provider 归属 continuation 游标快照。
    pub continuation: SessionProviderContinuationState,
}

/// Provider-scoped continuation cursor retained beside the canonical message window.
/// 与规范消息窗口并存保存的按 Provider 归属 continuation 游标。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionProviderContinuationState {
    /// Provider identifier that produced the latest canonical runtime result.
    /// 产出最近一次规范运行结果的 Provider 标识。
    pub provider_id: Option<String>,
    /// Model identifier that produced the latest canonical runtime result.
    /// 产出最近一次规范运行结果的模型标识。
    pub model_id: Option<String>,
    /// Latest provider-visible assistant message identifier returned by the upstream API.
    /// 上游接口返回的最近一次 Provider 可见助手消息标识。
    pub last_provider_message_id: Option<String>,
    /// Unix-millis text capturing when the continuation cursor was last refreshed.
    /// 捕获 continuation 游标最近刷新时间的 Unix 毫秒文本。
    pub updated_at: Option<String>,
}

/// Canonical conversation message stored inside the session protocol window.
/// 存储在会话协议窗口中的规范会话消息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConversationMessage {
    /// Stable protocol message identifier owned by the local workbench.
    /// 由本地工作台持有的稳定协议消息标识。
    pub id: String,
    /// Structured canonical payload that can be losslessly projected back into provider-visible request history.
    /// 可以稳定投影回 Provider 可见请求历史的结构化规范载荷。
    pub payload: SessionConversationPayload,
}

/// Canonical session record stored independently from the UI transcript projection.
/// 独立于界面转录投影保存的规范会话记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConversationRecord {
    /// Stable record identifier shared by projection caches and compaction checkpoints.
    /// 被投影缓存与压缩检查点共享的稳定记录标识。
    pub id: String,
    /// High-level record role used by transcript surfaces.
    /// 被转录表面使用的高层记录角色。
    pub kind: SessionMessageKind,
    /// Durable record kind that separates visible transcript rows from hidden internal nodes.
    /// 用于区分可见转录行与隐藏内部节点的持久化记录类型。
    pub record_kind: SessionMessageRecordKind,
    /// Whether the record should be shown in the visible transcript projection.
    /// 该记录是否应显示在可见转录投影中。
    pub visible: bool,
    /// Optional title projected into the transcript card header.
    /// 投影到转录卡片头部的可选标题。
    pub title: String,
    /// Primary body payload projected into transcript and modal surfaces.
    /// 被投影到转录与弹层表面的主要正文载荷。
    pub body: String,
    /// Optional structured metadata reserved for transcript-only enrichments and host-side audit data.
    /// 为仅限转录的增强信息与宿主侧审计数据保留的可选结构化元数据。
    pub metadata: Option<Value>,
    /// Unix-millis text captured when the record was created.
    /// 捕获该记录创建时间的 Unix 毫秒文本。
    pub created_at: String,
}

/// Structured canonical payload variants used by the local session protocol.
/// 本地会话协议使用的结构化规范载荷变体。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionConversationPayload {
    /// Plain user-authored text input.
    /// 纯文本用户输入。
    UserText {
        /// Visible user text content.
        /// 用户可见文本内容。
        text: String,
        /// Structured attachments that should stay attached to the user-authored text.
        /// 应与用户文本保持绑定的结构化附件集合。
        #[serde(default)]
        attachments: Vec<ConversationAttachment>,
    },
    /// Assistant turn that may contain visible text, structured reasoning, and tool calls.
    /// 可能同时包含可见文本、结构化思考与工具调用的助手回合。
    Assistant {
        /// Visible assistant text content.
        /// 助手可见文本内容。
        text: String,
        /// Flattened reasoning text extracted from provider-native reasoning blocks.
        /// 从 Provider 原生 reasoning 块中提取的扁平化思考文本。
        reasoning: String,
        /// Optional provider-native assistant message identifier.
        /// 可选的 Provider 原生助手消息标识。
        provider_message_id: Option<String>,
        /// Optional provider-native assistant state preserved for future provider-specific history replays.
        /// 为未来 Provider 专属历史回放保留的可选 Provider 原生助手状态。
        provider_message_state: Option<Value>,
        /// Optional provider usage snapshot retained for host-side budget calibration.
        /// 为宿主侧预算校准保留的可选 Provider 用量快照。
        #[serde(default)]
        usage: Option<ProviderUsageSnapshot>,
        /// Structured tool-call list emitted by the assistant turn.
        /// 由该助手回合发出的结构化工具调用列表。
        tool_calls: Vec<SessionConversationToolCall>,
    },
    /// Tool-result payload returned to the model after one assistant tool call.
    /// 在一次助手工具调用后回传给模型的工具结果载荷。
    ToolResult {
        /// Stable tool-result identifier emitted by the provider or synthesized locally.
        /// 由 Provider 发出或在本地合成的稳定工具结果标识。
        tool_result_id: String,
        /// Optional provider-visible tool-call identifier linked to the result.
        /// 与该结果关联的可选 Provider 可见工具调用标识。
        tool_call_id: Option<String>,
        /// Flattened human-readable tool-result text.
        /// 已扁平化的人类可读工具结果文本。
        text: String,
    },
}

/// Structured assistant tool-call item captured inside the canonical session protocol.
/// 在规范会话协议内部捕获的结构化助手工具调用项。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConversationToolCall {
    /// Stable tool-call item identifier emitted by the provider.
    /// 由 Provider 发出的稳定工具调用项标识。
    pub id: String,
    /// Optional provider-visible call identifier used by tool-result messages.
    /// 被工具结果消息使用的可选 Provider 可见调用标识。
    pub tool_call_id: Option<String>,
    /// Tool function name selected by the assistant.
    /// 助手选择的工具函数名称。
    pub tool_name: String,
    /// Structured JSON arguments selected by the assistant.
    /// 助手选择的结构化 JSON 参数。
    pub arguments: Value,
}

/// Appends one user-authored text input to the canonical conversation state.
/// 向规范会话状态追加一条用户文本输入。
pub fn append_user_text_message(
    state: &mut SessionConversationState,
    message_id: impl Into<String>,
    text: impl Into<String>,
    attachments: Vec<ConversationAttachment>,
) {
    state.messages.push(SessionConversationMessage {
        id: message_id.into(),
        payload: SessionConversationPayload::UserText {
            text: text.into(),
            attachments,
        },
    });
}

/// Appends one assistant-authored text node used by hidden explore or summary records.
/// 追加一条供隐藏 Explore 或 Summary 记录使用的助手文本节点。
pub fn append_assistant_text_message(
    state: &mut SessionConversationState,
    message_id: impl Into<String>,
    text: impl Into<String>,
) {
    state.messages.push(SessionConversationMessage {
        id: message_id.into(),
        payload: SessionConversationPayload::Assistant {
            text: text.into(),
            reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: None,
            tool_calls: Vec::new(),
        },
    });
}

/// Appends one canonical session record that later drives the visible transcript projection.
/// 追加一条后续将驱动可见转录投影的规范会话记录。
pub fn append_record(state: &mut SessionConversationState, record: SessionConversationRecord) {
    state.records.push(record);
}

/// Collects one latest-first unique attachment list from the canonical conversation window for host-side capability tools.
/// 从规范会话窗口中收集一份按最新优先去重后的附件列表，供宿主侧能力工具使用。
pub fn collect_host_visible_attachments(
    state: &SessionConversationState,
) -> Vec<ConversationAttachment> {
    let mut seen_keys = BTreeSet::new();
    let mut attachments = Vec::new();

    for message in state.messages.iter().rev() {
        let SessionConversationPayload::UserText {
            attachments: message_attachments,
            ..
        } = &message.payload
        else {
            continue;
        };

        for attachment in message_attachments.iter().rev() {
            let identity_key = attachment
                .asset_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
                .unwrap_or_else(|| {
                    format!(
                        "{}|{}|{}",
                        attachment.kind.modality_name(),
                        attachment.file_name,
                        attachment.local_path
                    )
                });
            if !seen_keys.insert(identity_key) {
                continue;
            }
            attachments.push(attachment.clone());
        }
    }

    attachments
}

/// Appends one streamed body delta to the matching canonical session record and returns whether it was found.
/// 向匹配的规范会话记录追加一段流式正文增量，并返回是否命中。
pub fn append_record_body_delta(
    state: &mut SessionConversationState,
    record_id: &str,
    delta: &str,
) -> bool {
    let record = state
        .records
        .iter_mut()
        .find(|record| record.id == record_id);
    let Some(record) = record else {
        return false;
    };
    record.body.push_str(delta);
    true
}

/// Rewrites the active canonical window using one compaction summary plus the retained recent tail.
/// 使用一条压缩摘要与保留的最近尾部消息改写当前活动规范窗口。
pub fn compact_conversation_with_summary(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
    policy: ContextCompactionPolicy,
) {
    let retained_tail = canonical_recent_tail_messages(&state.messages, policy);
    let mut compacted_messages = Vec::with_capacity(retained_tail.len() + 1);
    compacted_messages.push(SessionConversationMessage {
        id: summary_message_id.into(),
        payload: SessionConversationPayload::Assistant {
            text: summary_body.into(),
            reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: None,
            tool_calls: Vec::new(),
        },
    });
    compacted_messages.extend(retained_tail);
    state.messages = compacted_messages;
    state.continuation.last_provider_message_id = None;
    state.continuation.updated_at = Some(updated_at.into());
}

/// Selects the canonical message tail retained after summary insertion without splitting tool-call turns.
/// 在不切断工具调用轮次的前提下，选择摘要插入后保留的规范消息尾部。
fn canonical_recent_tail_messages(
    messages: &[SessionConversationMessage],
    policy: ContextCompactionPolicy,
) -> Vec<SessionConversationMessage> {
    if messages.is_empty() {
        return Vec::new();
    }

    let target_turns = policy.tail_turns.max(1usize);
    let budget = policy.preserve_recent_tokens;
    let segments = canonical_tail_segments(messages);
    let mut retained_tail = Vec::new();
    let mut retained_tokens = 0usize;
    let mut retained_turns = 0usize;

    for segment in segments.iter().rev() {
        if retained_turns >= target_turns {
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
            retained_turns += 1;
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

/// Builds safe tail segments at user-turn boundaries so tool calls and tool results stay together.
/// 按用户轮次边界构建安全尾部段，确保工具调用与工具结果保持在同一组内。
fn canonical_tail_segments(messages: &[SessionConversationMessage]) -> Vec<CanonicalTailSegment> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut user_starts = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| {
            matches!(
                &message.payload,
                SessionConversationPayload::UserText { .. }
            )
            .then_some(index)
        })
        .collect::<Vec<_>>();
    if user_starts.is_empty() {
        return vec![CanonicalTailSegment {
            start: 0,
            end: messages.len(),
        }];
    }
    if user_starts[0] != 0 {
        user_starts.insert(0, 0);
    }

    user_starts
        .iter()
        .enumerate()
        .map(|(position, start)| CanonicalTailSegment {
            start: *start,
            end: user_starts
                .get(position + 1)
                .copied()
                .unwrap_or(messages.len()),
        })
        .collect()
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
    ((trimmed.chars().count() + 3usize) / 4usize).max(1usize)
}

/// Rebuilds a canonical state from a bounded transcript window when only lazy-loaded rows are available.
/// 当只有懒加载转录行可用时，根据有界转录窗口重建规范会话状态。
pub fn conversation_state_from_session_messages(
    messages: &[SessionMessageEntry],
) -> SessionConversationState {
    let mut state = SessionConversationState::default();
    for message in messages {
        state.records.push(SessionConversationRecord {
            id: message.id.clone(),
            kind: message.kind,
            record_kind: message.record_kind,
            visible: message.visible,
            title: message.title.clone(),
            body: message.body.clone(),
            metadata: message.metadata.clone(),
            created_at: message.created_at.clone(),
        });

        if let Some(protocol_message) = session_message_to_protocol_message(message) {
            state.messages.push(protocol_message);
        }
    }
    state
}

/// Compacts canonical records after summary creation so old tool details stop inflating persistence.
/// 在摘要创建后压缩规范记录，避免旧工具细节继续放大持久化体积。
pub fn compact_conversation_records_after_summary(
    state: &mut SessionConversationState,
    summary_record_id: &str,
    policy: ContextCompactionPolicy,
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
            compact_historical_record(record, policy.prune)
        };
        if retained_ids.insert(retained_record.id.clone()) {
            retained_records.push(retained_record);
        }
    }

    state.records = retained_records;
}

/// Projects canonical session records into the persisted transcript model used by the existing GUI surfaces.
/// 将规范会话记录投影为现有图形界面表面使用的持久化转录模型。
pub fn conversation_state_to_session_messages(
    state: &SessionConversationState,
) -> Vec<SessionMessageEntry> {
    state
        .records
        .iter()
        .cloned()
        .map(conversation_record_to_session_message)
        .collect()
}

/// Projects the context-participating canonical records from the current anchor forward into transcript entries.
/// 将从当前锚点开始参与上下文的规范记录投影为转录条目。
pub fn conversation_state_to_context_records(
    state: &SessionConversationState,
    context_start_message_id: Option<&str>,
) -> Vec<SessionMessageEntry> {
    let start_index = context_start_message_id
        .and_then(|message_id| {
            state
                .records
                .iter()
                .position(|record| record.id == message_id)
        })
        .unwrap_or(0);

    state.records[start_index..]
        .iter()
        .cloned()
        .map(conversation_record_to_session_message)
        .filter(|message| message.contributes_to_context())
        .collect()
}

/// Converts one canonical session record back into the persisted transcript entry shape expected by the current GUI layer.
/// 将单条规范会话记录重新转换为当前图形界面层期望的持久化转录条目形状。
fn conversation_record_to_session_message(
    record: SessionConversationRecord,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: record.id,
        kind: record.kind,
        record_kind: record.record_kind,
        visible: record.visible,
        title: record.title,
        body: record.body,
        metadata: record.metadata,
        created_at: record.created_at,
    }
}

/// Converts one transcript entry into a provider-facing protocol message when it is safe to replay.
/// 在安全可重放时将一条转录项转换为面向 Provider 的协议消息。
fn session_message_to_protocol_message(
    message: &SessionMessageEntry,
) -> Option<SessionConversationMessage> {
    match message.kind {
        SessionMessageKind::User if message.contributes_to_context() => {
            Some(SessionConversationMessage {
                id: message.id.clone(),
                payload: SessionConversationPayload::UserText {
                    text: visible_user_message_canonical_text(message),
                    attachments: visible_user_message_transcript_attachments(message),
                },
            })
        }
        SessionMessageKind::Assistant
            if assistant_record_can_seed_protocol(&message.record_kind) =>
        {
            Some(SessionConversationMessage {
                id: message.id.clone(),
                payload: SessionConversationPayload::Assistant {
                    text: message.body.clone(),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            })
        }
        _ if message.contributes_to_context()
            && matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolCallOutput
            ) =>
        {
            tool_call_session_message_to_protocol_message(message)
        }
        _ if message.contributes_to_context()
            && matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolResultOutput
            ) =>
        {
            tool_result_session_message_to_protocol_message(message)
        }
        _ => None,
    }
}

/// Rebuilds one canonical assistant tool-call message from a persisted transcript row when the full conversation-state JSON is not available.
/// 当完整会话状态 JSON 不可用时，从一条持久化转录行重建一条规范助手工具调用消息。
fn tool_call_session_message_to_protocol_message(
    message: &SessionMessageEntry,
) -> Option<SessionConversationMessage> {
    let metadata = message.metadata.as_ref()?;
    let tool_name = metadata.get("tool_name").and_then(Value::as_str)?.trim();
    if tool_name.is_empty() {
        return None;
    }
    let arguments = metadata
        .get("arguments")
        .cloned()
        .or_else(|| serde_json::from_str::<Value>(message.body.trim()).ok())
        .unwrap_or(Value::Object(serde_json::Map::new()));
    let tool_call_id = metadata
        .get("tool_call_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    Some(SessionConversationMessage {
        id: message.id.clone(),
        payload: SessionConversationPayload::Assistant {
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: None,
            provider_message_state: None,
            usage: None,
            tool_calls: vec![SessionConversationToolCall {
                id: tool_call_id
                    .clone()
                    .unwrap_or_else(|| message.id.clone()),
                tool_call_id,
                tool_name: tool_name.to_string(),
                arguments,
            }],
        },
    })
}

/// Rebuilds one canonical tool-result message from a persisted transcript row when the full conversation-state JSON is not available.
/// 当完整会话状态 JSON 不可用时，从一条持久化转录行重建一条规范工具结果消息。
fn tool_result_session_message_to_protocol_message(
    message: &SessionMessageEntry,
) -> Option<SessionConversationMessage> {
    let metadata = message.metadata.as_ref();
    let tool_call_id = metadata
        .and_then(|payload| payload.get("tool_call_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let tool_result_id = metadata
        .and_then(|payload| payload.get("tool_result_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            tool_call_id
                .as_deref()
                .map(|tool_call_id| format!("tool-result-{tool_call_id}"))
        })
        .unwrap_or_else(|| message.id.clone());

    Some(SessionConversationMessage {
        id: message.id.clone(),
        payload: SessionConversationPayload::ToolResult {
            tool_result_id,
            tool_call_id,
            text: message.body.clone(),
        },
    })
}

/// Resolves the canonical user-text payload stored for one visible user transcript message, falling back to the rendered body for older rows.
/// 解析一条可见用户转录消息保存的规范用户文本载荷；若旧记录未保存该字段，则回退到渲染正文。
fn visible_user_message_canonical_text(message: &SessionMessageEntry) -> String {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("canonical_user_text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| message.body.clone())
}

/// Resolves transcript attachment metadata stored on one visible user message so canonical replay can preserve image-understanding inputs across reloads.
/// 解析保存在可见用户消息上的转录附件元数据，使规范重放在重开后仍能保留图片理解输入。
fn visible_user_message_transcript_attachments(
    message: &SessionMessageEntry,
) -> Vec<ConversationAttachment> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("transcript_attachments"))
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ConversationAttachment>>(value).ok())
        .unwrap_or_default()
}

/// Returns whether one assistant record kind is safe to replay as plain assistant context.
/// 返回某个助手记录类型是否可以作为纯文本助手上下文安全重放。
fn assistant_record_can_seed_protocol(record_kind: &SessionMessageRecordKind) -> bool {
    matches!(
        record_kind,
        SessionMessageRecordKind::AssistantOutput
            | SessionMessageRecordKind::ExploreOutput
            | SessionMessageRecordKind::CompactionSummary
            | SessionMessageRecordKind::Transcript
    )
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
}

/// Reduces an old surviving dialogue record to compact text and minimal metadata.
/// 将一条保留下来的旧对话记录降级为紧凑正文与最小元数据。
fn compact_historical_record(
    mut record: SessionConversationRecord,
    prune_tools: bool,
) -> SessionConversationRecord {
    if generated_attachment_tool_result_should_survive_compaction(&record) {
        record.body.clear();
        record.metadata = compact_generated_attachment_tool_result_metadata(record.metadata.take());
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
    use super::*;
    use crate::agent_runtime::provider_request_bridge_messages::{
        core_messages_to_provider_visible_history, session_message_to_core_message,
    };
    use serde_json::json;
    use vulcan_core::types::ConversationPayload as CoreConversationPayload;

    /// Verifies that stored assistant reasoning remains local-only and is not replayed into provider-visible history.
    /// 验证已存储的助手思考内容保持为本地数据，不会被回放到 Provider 可见历史中。
    #[test]
    fn assistant_reasoning_is_not_replayed_into_provider_visible_history() {
        let messages = vec![SessionConversationMessage {
            id: String::from("assistant-1"),
            payload: SessionConversationPayload::Assistant {
                text: String::from("继续处理。"),
                reasoning: String::from("先检查工具结果，再决定下一步。"),
                provider_message_id: Some(String::from("resp-1")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                usage: None,
            },
        }];

        let core_messages = messages
            .iter()
            .map(session_message_to_core_message)
            .collect::<Vec<_>>();
        let history = core_messages_to_provider_visible_history(&core_messages);
        let Some(message) = history.first() else {
            panic!("expected one assistant history item");
        };
        let CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_id,
            provider_message_state: _,
            tool_calls,
            usage: _,
        } = &message.payload
        else {
            panic!("expected one assistant history item");
        };

        assert_eq!(text, "继续处理。");
        assert!(reasoning.is_empty());
        assert_eq!(provider_message_id.as_deref(), Some("resp-1"));
        assert!(tool_calls.is_empty());
    }

    /// Verifies that transcript projection preserves record metadata for tool-result rich UI surfaces.
    /// 验证转录投影会保留记录元数据，供工具结果富界面表面继续使用。
    #[test]
    fn conversation_record_projection_preserves_metadata() {
        let metadata = json!({
            "tool_name": "write_file",
            "host_result": {
                "kind": "change_set",
                "payload": {
                    "path": "src/main.rs",
                },
            },
        });
        let state = SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: String::from("record-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回"),
                body: String::from("已写入文件。"),
                metadata: Some(metadata.clone()),
                created_at: String::from("1"),
            }],
            continuation: SessionProviderContinuationState::default(),
        };

        let projected = conversation_state_to_session_messages(&state);
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].metadata, Some(metadata));
    }

    /// Verifies fallback protocol reconstruction keeps one tool-call plus tool-result chain replayable when the persisted full conversation-state payload is unavailable.
    /// 验证在缺失持久化完整会话状态载荷时，回退协议重建仍会保留一条可回放的工具调用加工具结果链。
    #[test]
    fn conversation_state_from_session_messages_replays_tool_chain() {
        let messages = vec![
            SessionMessageEntry {
                id: String::from("user-1"),
                kind: SessionMessageKind::User,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("用户输入"),
                body: String::from("请读取 Cargo.toml"),
                metadata: Some(json!({
                    "canonical_user_text": "请读取 Cargo.toml"
                })),
                created_at: String::from("1"),
            },
            SessionMessageEntry {
                id: String::from("tool-call-row"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolCallOutput,
                visible: true,
                title: String::from("调用 read_file 工具"),
                body: String::from("{\"path\":\"Cargo.toml\"}"),
                metadata: Some(json!({
                    "tool_call_id": "call-1",
                    "tool_name": "read_file",
                    "arguments": { "path": "Cargo.toml" }
                })),
                created_at: String::from("2"),
            },
            SessionMessageEntry {
                id: String::from("tool-result-row"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::ToolResultOutput,
                visible: true,
                title: String::from("工具返回"),
                body: String::from("name = \"vulcan-code\""),
                metadata: Some(json!({
                    "tool_call_id": "call-1",
                    "tool_result_id": "tool-result-call-1"
                })),
                created_at: String::from("3"),
            },
        ];

        let rebuilt = conversation_state_from_session_messages(&messages);

        assert_eq!(rebuilt.messages.len(), 3);
        assert!(matches!(
            &rebuilt.messages[1].payload,
            SessionConversationPayload::Assistant { tool_calls, .. }
                if tool_calls.len() == 1
                    && tool_calls[0].tool_name == "read_file"
                    && tool_calls[0].tool_call_id.as_deref() == Some("call-1")
                    && tool_calls[0].arguments == json!({ "path": "Cargo.toml" })
        ));
        assert!(matches!(
            &rebuilt.messages[2].payload,
            SessionConversationPayload::ToolResult { tool_result_id, tool_call_id, text }
                if tool_result_id == "tool-result-call-1"
                    && tool_call_id.as_deref() == Some("call-1")
                    && text == "name = \"vulcan-code\""
        ));
    }

    /// Verifies streamed body deltas update records in place without returning a cloned growing body.
    /// 验证流式正文增量会原地更新记录，而不会返回逐渐变大的正文克隆。
    #[test]
    fn append_record_body_delta_updates_in_place() {
        let mut state = SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: String::from("assistant-1"),
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::Transcript,
                visible: true,
                title: String::from("助手"),
                body: String::from("a"),
                metadata: None,
                created_at: String::from("1"),
            }],
            continuation: SessionProviderContinuationState::default(),
        };

        assert!(append_record_body_delta(&mut state, "assistant-1", "bc"));
        assert!(!append_record_body_delta(&mut state, "missing", "x"));
        assert_eq!(state.records[0].body, "abc");
    }

    /// Verifies visible user transcript metadata can recover canonical text and attachments during protocol replay after a session reload.
    /// 验证可见用户转录元数据可以在会话重开后的协议重放阶段恢复规范文本与附件。
    #[test]
    fn user_transcript_metadata_round_trips_canonical_text_and_attachments() {
        let message = SessionMessageEntry {
            id: String::from("user-1"),
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::Transcript,
            visible: true,
            title: String::from("用户指令"),
            body: String::from("请分析图片"),
            metadata: Some(json!({
                "canonical_user_text": "请分析图片并告诉我结构",
                "transcript_attachments": [{
                    "kind": "image",
                    "file_name": "diagram.png",
                    "local_path": "D:/diagram.png",
                    "mime_type": "image/png",
                }],
            })),
            created_at: String::from("1"),
        };

        let Some(protocol_message) = session_message_to_protocol_message(&message) else {
            panic!("expected one replayable user protocol message");
        };
        let SessionConversationPayload::UserText { text, attachments } = protocol_message.payload
        else {
            panic!("expected one replayable user text payload");
        };

        assert_eq!(text, "请分析图片并告诉我结构");
        assert_eq!(attachments.len(), 1);
        assert_eq!(attachments[0].file_name, "diagram.png");
        assert_eq!(attachments[0].local_path, "D:/diagram.png");
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
                        tool_result_id: String::from("tool-result"),
                        tool_call_id: Some(String::from("tool-call-provider")),
                        text: "很大的工具输出".repeat(200),
                    },
                },
            ],
            records: Vec::new(),
            continuation: SessionProviderContinuationState::default(),
        };
        let policy = ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 16,
            ..ContextCompactionPolicy::default()
        };

        compact_conversation_with_summary(&mut state, "summary", "摘要", "2", policy);

        assert_eq!(state.messages.len(), 4);
        assert!(matches!(
            &state.messages[1].payload,
            SessionConversationPayload::UserText { text, .. } if text == "读取文件"
        ));
        assert!(matches!(
            &state.messages[2].payload,
            SessionConversationPayload::Assistant { tool_calls, .. } if tool_calls.len() == 1
        ));
        assert!(matches!(
            &state.messages[3].payload,
            SessionConversationPayload::ToolResult { text, .. }
                if text == COMPACTED_TOOL_RESULT_PLACEHOLDER
        ));
    }

    /// Verifies that hidden execution summaries no longer replay into provider-visible context.
    /// 验证隐藏执行摘要不再回放进 Provider 可见上下文。
    #[test]
    fn summary_output_is_excluded_from_context_and_protocol_replay() {
        let message = SessionMessageEntry {
            id: String::from("summary-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::SummaryOutput,
            visible: false,
            title: String::from("Summary 输出"),
            body: String::from("本轮执行成功完成。输出摘要：测试。"),
            metadata: None,
            created_at: String::from("1"),
        };
        let state = SessionConversationState {
            messages: Vec::new(),
            records: vec![SessionConversationRecord {
                id: message.id.clone(),
                kind: message.kind,
                record_kind: message.record_kind,
                visible: message.visible,
                title: message.title.clone(),
                body: message.body.clone(),
                metadata: None,
                created_at: message.created_at.clone(),
            }],
            continuation: SessionProviderContinuationState::default(),
        };

        assert!(!message.contributes_to_context());
        assert!(session_message_to_protocol_message(&message).is_none());
        assert!(conversation_state_to_context_records(&state, None).is_empty());
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
                "tool_result_id": "tool-result-call-media-1",
                "host_result": {
                    "kind": "generated_attachments",
                    "payload": {
                        "outputs": [{
                            "kind": "image",
                            "file_name": "demo.jpg",
                            "local_path": "D:/demo.jpg",
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
            continuation: SessionProviderContinuationState::default(),
        };
        compact_conversation_records_after_summary(
            &mut state,
            "missing-summary",
            ContextCompactionPolicy {
                tail_turns: 1,
                ..ContextCompactionPolicy::default()
            },
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
            Some("D:/demo.jpg")
        );
        assert!(outputs[0].get("ignored").is_none());
        assert!(metadata.get("noise").is_none());
    }
}
