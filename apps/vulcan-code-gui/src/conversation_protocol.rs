//! Canonical conversation protocol models shared by the GUI, persistence, and runtime bridge.
//! 供图形界面、持久化层与运行时桥接共享的规范会话协议模型。

use crate::model::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Minimum active-history size that triggers one protocol compaction rewrite.
/// 触发一次协议历史压缩改写所需的最小活动历史条数。
pub const PROTOCOL_COMPACTION_TRIGGER_COUNT: usize = 10;

/// Number of latest canonical messages retained verbatim after one compaction rewrite.
/// 一次压缩改写后按原样保留的最新规范消息条数。
pub const PROTOCOL_COMPACTION_RETAINED_TAIL_COUNT: usize = 4;

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
    /// Structured canonical payload that can be losslessly projected back into rig history.
    /// 可以稳定投影回 rig 历史的结构化规范载荷。
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
        /// Structured tool-call list emitted by the assistant turn.
        /// 由该助手回合发出的结构化工具调用列表。
        tool_calls: Vec<SessionConversationToolCall>,
    },
    /// Tool-result payload returned to the model after one assistant tool call.
    /// 在一次助手工具调用后回传给模型的工具结果载荷。
    ToolResult {
        /// Stable tool-result identifier emitted by rig or synthesized locally.
        /// 由 rig 发出或在本地合成的稳定工具结果标识。
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
) {
    state.messages.push(SessionConversationMessage {
        id: message_id.into(),
        payload: SessionConversationPayload::UserText { text: text.into() },
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
            tool_calls: Vec::new(),
        },
    });
}

/// Appends one canonical session record that later drives the visible transcript projection.
/// 追加一条后续将驱动可见转录投影的规范会话记录。
pub fn append_record(state: &mut SessionConversationState, record: SessionConversationRecord) {
    state.records.push(record);
}

/// Appends one streamed body delta to the matching canonical session record and returns the updated body when found.
/// 向匹配的规范会话记录追加一段流式正文增量，并在命中时返回更新后的正文。
pub fn append_record_body_delta(
    state: &mut SessionConversationState,
    record_id: &str,
    delta: &str,
) -> Option<String> {
    let record = state
        .records
        .iter_mut()
        .find(|record| record.id == record_id)?;
    record.body.push_str(delta);
    Some(record.body.clone())
}

/// Rewrites the active canonical window using one compaction summary plus the retained recent tail.
/// 使用一条压缩摘要与保留的最近尾部消息改写当前活动规范窗口。
pub fn compact_conversation_with_summary(
    state: &mut SessionConversationState,
    summary_message_id: impl Into<String>,
    summary_body: impl Into<String>,
    updated_at: impl Into<String>,
) {
    let retained_tail = if state.messages.len() > PROTOCOL_COMPACTION_RETAINED_TAIL_COUNT {
        state.messages[state.messages.len() - PROTOCOL_COMPACTION_RETAINED_TAIL_COUNT..].to_vec()
    } else {
        state.messages.clone()
    };
    let mut compacted_messages = Vec::with_capacity(retained_tail.len() + 1);
    compacted_messages.push(SessionConversationMessage {
        id: summary_message_id.into(),
        payload: SessionConversationPayload::Assistant {
            text: summary_body.into(),
            reasoning: String::new(),
            provider_message_id: None,
            tool_calls: Vec::new(),
        },
    });
    compacted_messages.extend(retained_tail);
    state.messages = compacted_messages;
    state.continuation.last_provider_message_id = None;
    state.continuation.updated_at = Some(updated_at.into());
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
                tool_calls: Vec::new(),
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
            tool_calls,
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
}
