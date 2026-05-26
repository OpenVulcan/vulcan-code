//! Canonical conversation protocol models shared by GUI state, persistence, and runtime bridges.
//! 供图形界面状态、持久化与运行时桥接共享的规范会话协议模型。

use crate::transcript::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use vulcan_core::types::{ConversationAttachment, ProviderUsageSnapshot};

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

    for record in state.records.iter().rev() {
        let Some(metadata) = record.metadata.as_ref() else {
            continue;
        };
        for attachment in generated_attachments_from_tool_metadata(metadata) {
            let identity_key = attachment_identity_key(&attachment);
            if !seen_keys.insert(identity_key) {
                continue;
            }
            attachments.push(attachment);
        }
    }

    for message in state.messages.iter().rev() {
        let SessionConversationPayload::UserText {
            attachments: message_attachments,
            ..
        } = &message.payload
        else {
            continue;
        };

        for attachment in message_attachments.iter().rev() {
            let identity_key = attachment_identity_key(attachment);
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
                id: tool_call_id.clone().unwrap_or_else(|| message.id.clone()),
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
        .and_then(|metadata| metadata.get("canonical_text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| message.body.clone())
}

/// Resolves the structured attachment list stored on one visible user transcript message, falling back to an empty list for older rows.
/// 解析一条可见用户转录消息保存的结构化附件列表；若旧记录未保存附件，则回退为空集合。
fn visible_user_message_transcript_attachments(
    message: &SessionMessageEntry,
) -> Vec<ConversationAttachment> {
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("attachments"))
        .cloned()
        .and_then(|value| serde_json::from_value::<Vec<ConversationAttachment>>(value).ok())
        .unwrap_or_default()
}

/// Returns whether one assistant-side transcript record can seed a canonical assistant message when reconstructing the protocol state from transcript rows alone.
/// 返回当仅依赖转录行重建协议状态时，某条助手侧转录记录是否可以作为规范助手消息的种子。
fn assistant_record_can_seed_protocol(record_kind: &SessionMessageRecordKind) -> bool {
    matches!(
        record_kind,
        SessionMessageRecordKind::AssistantOutput
            | SessionMessageRecordKind::ExploreOutput
            | SessionMessageRecordKind::CompactionSummary
            | SessionMessageRecordKind::Transcript
    )
}

/// Builds one stable attachment identity key that prefers asset identifiers and falls back to modality, file name, and local path.
/// 构建单条稳定附件身份键，优先使用资产标识，缺失时退回到模态、文件名与本地路径的组合。
fn attachment_identity_key(attachment: &ConversationAttachment) -> String {
    attachment
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
        })
}

/// Extracts generated attachments from one tool-result metadata object when that record carries a generated-attachments host result.
/// 当某条工具结果元数据携带 generated-attachments 宿主结果时，从中提取生成附件集合。
fn generated_attachments_from_tool_metadata(metadata: &Value) -> Vec<ConversationAttachment> {
    metadata
        .as_object()
        .and_then(|_| resolve_generated_attachment_host_result_from_metadata(metadata))
        .and_then(|host_result| host_result.get("payload"))
        .and_then(|payload| payload.get("outputs"))
        .cloned()
        .and_then(|outputs| serde_json::from_value::<Vec<ConversationAttachment>>(outputs).ok())
        .unwrap_or_default()
}

/// Resolves the generated-attachments host-result payload from one record metadata object.
/// 从单条记录元数据对象中解析 generated-attachments 宿主结果载荷。
fn resolve_generated_attachment_host_result_from_metadata(metadata: &Value) -> Option<&Value> {
    metadata
        .get("host_result")
        .filter(|value| value.get("kind").and_then(Value::as_str) == Some("generated_attachments"))
        .or_else(|| {
            (metadata.get("kind").and_then(Value::as_str) == Some("generated_attachments"))
                .then_some(metadata)
        })
}
