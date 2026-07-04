//! Durable transcript-record and continuation bridge helpers reused by provider-request projection and runtime state round-trips.
//! 供 Provider 请求投影与运行时状态往返复用的持久化转录记录与 continuation 桥接辅助逻辑。

use crate::conversation_protocol::{SessionConversationRecord, SessionProviderContinuationState};
use crate::model::{SessionMessageKind, SessionMessageRecordKind};
use serde_json::Value;
use vulcan_core::types::{
    ConversationRecord as CoreConversationRecord, ProviderContinuation as CoreProviderContinuation,
    TranscriptRecordKind as CoreTranscriptRecordKind, TranscriptRole as CoreTranscriptRole,
};

/// Converts one session-owned provider continuation snapshot into the `vulcan-core` continuation shape.
/// 将一份会话持有的 Provider continuation 快照转换为 `vulcan-core` continuation 形状。
pub(crate) fn session_continuation_to_core_continuation(
    continuation: &SessionProviderContinuationState,
) -> CoreProviderContinuation {
    CoreProviderContinuation {
        provider_id: continuation.provider_id.clone(),
        model_id: continuation.model_id.clone(),
        last_provider_message_id: continuation.last_provider_message_id.clone(),
        updated_at: continuation.updated_at.clone(),
    }
}

/// Converts one `vulcan-core` continuation snapshot back into the host-owned continuation shape.
/// 将一份 `vulcan-core` continuation 快照转换回运行时持有的 continuation 形状。
pub(crate) fn core_continuation_to_session_continuation(
    continuation: &CoreProviderContinuation,
) -> SessionProviderContinuationState {
    SessionProviderContinuationState {
        provider_id: continuation.provider_id.clone(),
        model_id: continuation.model_id.clone(),
        last_provider_message_id: continuation.last_provider_message_id.clone(),
        updated_at: continuation.updated_at.clone(),
    }
}

/// Converts one session-owned durable record into the `vulcan-core` record shape so pre-existing transcript records survive controller round-trips.
/// 将一条会话持有的持久化记录转换为 `vulcan-core` 记录形状，从而让既有转录记录在控制器往返过程中得以保留。
pub(crate) fn session_record_to_core_record(
    record: &SessionConversationRecord,
) -> CoreConversationRecord {
    CoreConversationRecord {
        id: record.id.clone(),
        role: session_record_to_core_role(record),
        record_kind: session_record_to_core_record_kind(record.record_kind),
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Converts one `vulcan-core` durable record back into the host-owned record shape while preserving display metadata.
/// 将一条 `vulcan-core` 持久化记录转换回运行时持有的记录形状，同时保留显示元数据。
pub(crate) fn core_record_to_session_record(
    record: &CoreConversationRecord,
) -> SessionConversationRecord {
    SessionConversationRecord {
        id: record.id.clone(),
        kind: core_record_to_session_message_kind(record),
        record_kind: core_record_to_session_record_kind(record),
        visible: record.visible,
        title: record.title.clone(),
        body: record.body.clone(),
        metadata: record.metadata.clone(),
        created_at: record.created_at.clone(),
    }
}

/// Derives the `vulcan-core` record role from one host-owned durable record.
/// 根据一条运行时持有的持久化记录推导 `vulcan-core` 记录角色。
fn session_record_to_core_role(record: &SessionConversationRecord) -> CoreTranscriptRole {
    match record.record_kind {
        SessionMessageRecordKind::ThinkingOutput => CoreTranscriptRole::Thinking,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput => {
            CoreTranscriptRole::Tool
        }
        SessionMessageRecordKind::TurnStats => CoreTranscriptRole::Assistant,
        SessionMessageRecordKind::SummaryOutput | SessionMessageRecordKind::CompactionSummary => {
            CoreTranscriptRole::Summary
        }
        SessionMessageRecordKind::CompactionStatus => CoreTranscriptRole::System,
        SessionMessageRecordKind::DisplayNote => CoreTranscriptRole::Explore,
        SessionMessageRecordKind::ExploreOutput | SessionMessageRecordKind::ToolDetailOutput => {
            CoreTranscriptRole::Explore
        }
        _ => match record.kind {
            SessionMessageKind::User => CoreTranscriptRole::User,
            SessionMessageKind::Assistant => CoreTranscriptRole::Assistant,
            SessionMessageKind::Error => CoreTranscriptRole::System,
        },
    }
}

/// Converts one host-owned durable record kind into the `vulcan-core` record-kind classification.
/// 将一种运行时持有的持久化记录类型转换为 `vulcan-core` 记录类型分类。
fn session_record_to_core_record_kind(
    record_kind: SessionMessageRecordKind,
) -> CoreTranscriptRecordKind {
    match record_kind {
        SessionMessageRecordKind::Transcript
        | SessionMessageRecordKind::AssistantOutput
        | SessionMessageRecordKind::ToolCallOutput
        | SessionMessageRecordKind::ToolResultOutput
        | SessionMessageRecordKind::TurnStats => CoreTranscriptRecordKind::Visible,
        SessionMessageRecordKind::ThinkingOutput
        | SessionMessageRecordKind::TitleOutput
        | SessionMessageRecordKind::SummaryOutput
        | SessionMessageRecordKind::CompactionStatus
        | SessionMessageRecordKind::TurnTerminated => CoreTranscriptRecordKind::Hidden,
        SessionMessageRecordKind::DisplayNote
        | SessionMessageRecordKind::ExploreOutput
        | SessionMessageRecordKind::ToolDetailOutput => CoreTranscriptRecordKind::ProjectionOnly,
        SessionMessageRecordKind::CompactionSummary => CoreTranscriptRecordKind::Checkpoint,
    }
}

/// Derives the workbench transcript message kind from one `vulcan-core` durable record role.
/// 根据一条 `vulcan-core` 持久化记录角色推导工作台转录消息类型。
fn core_record_to_session_message_kind(record: &CoreConversationRecord) -> SessionMessageKind {
    match record.role {
        CoreTranscriptRole::User => SessionMessageKind::User,
        _ => SessionMessageKind::Assistant,
    }
}

/// Derives the workbench record kind from one `vulcan-core` durable record while preserving the semantic difference between tool calls, tool results, and hidden nodes.
/// 根据一条 `vulcan-core` 持久化记录推导工作台记录类型，同时保留工具调用、工具结果与隐藏节点之间的语义差异。
fn core_record_to_session_record_kind(record: &CoreConversationRecord) -> SessionMessageRecordKind {
    match (&record.role, &record.record_kind) {
        _ if record
            .metadata
            .as_ref()
            .and_then(Value::as_object)
            .is_some_and(|object| object.contains_key("context_compaction_status")) =>
        {
            SessionMessageRecordKind::CompactionStatus
        }
        _ if record
            .metadata
            .as_ref()
            .and_then(Value::as_object)
            .is_some_and(|object| object.contains_key("display_note")) =>
        {
            SessionMessageRecordKind::DisplayNote
        }
        _ if record
            .metadata
            .as_ref()
            .and_then(Value::as_object)
            .is_some_and(|object| object.contains_key("turn_stats")) =>
        {
            SessionMessageRecordKind::TurnStats
        }
        (CoreTranscriptRole::Thinking, _) => SessionMessageRecordKind::ThinkingOutput,
        (CoreTranscriptRole::Summary, _) | (_, CoreTranscriptRecordKind::Checkpoint) => {
            SessionMessageRecordKind::CompactionSummary
        }
        (CoreTranscriptRole::Explore, _) | (_, CoreTranscriptRecordKind::ProjectionOnly) => {
            SessionMessageRecordKind::ExploreOutput
        }
        (CoreTranscriptRole::Tool, _) if core_record_is_tool_result(record) => {
            SessionMessageRecordKind::ToolResultOutput
        }
        (CoreTranscriptRole::Tool, _) => SessionMessageRecordKind::ToolCallOutput,
        (CoreTranscriptRole::Assistant, _) if record.visible => {
            SessionMessageRecordKind::AssistantOutput
        }
        (CoreTranscriptRole::Assistant, _) => SessionMessageRecordKind::SummaryOutput,
        _ => SessionMessageRecordKind::Transcript,
    }
}

/// Returns whether one `vulcan-core` tool record semantically represents a tool result rather than the original tool call request.
/// 返回一条 `vulcan-core` 工具记录在语义上是否表示工具结果，而不是原始工具调用请求。
fn core_record_is_tool_result(record: &CoreConversationRecord) -> bool {
    record
        .metadata
        .as_ref()
        .and_then(Value::as_object)
        .map(|object| object.contains_key("is_error") || object.contains_key("kind"))
        .unwrap_or(false)
}
