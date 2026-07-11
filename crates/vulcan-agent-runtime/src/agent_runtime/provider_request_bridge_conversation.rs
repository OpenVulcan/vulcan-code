//! Canonical conversation-snapshot bridge helpers reused by request snapshots, runtime bootstrap, and regression tests.
//! 供请求快照、运行时 bootstrap 与回归测试复用的规范会话快照桥接辅助逻辑。

use crate::agent_runtime::provider_request_bridge_messages::{
    core_message_to_session_message, session_message_to_core_message,
};
use crate::agent_runtime::provider_request_bridge_records::{
    core_continuation_to_session_continuation, core_record_to_session_record,
    session_continuation_to_core_continuation, session_record_to_core_record,
};
use crate::conversation_protocol::SessionConversationState;
use vulcan_context::protocol::{
    validate_session_conversation_message_identities,
    validate_session_conversation_record_identities,
};
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;

/// Converts one session-owned canonical conversation state into the `vulcan-core` snapshot shape.
/// 将一份会话持有的规范会话状态转换为 `vulcan-core` 快照形状。
///
/// # Parameters
/// 参数
/// `conversation_state` is the authoritative session-owned state whose message and record identities are validated before conversion.
/// `conversation_state` 是转换前需要校验消息与记录身份的权威会话状态。
///
/// # Returns
/// 返回值
/// Returns the lossless core snapshot, or an identity error without producing a partial snapshot.
/// 返回无损 core 快照；身份无效时返回错误且不产生部分快照。
pub(crate) fn build_core_conversation_snapshot(
    conversation_state: &SessionConversationState,
) -> Result<CoreConversationSnapshot, String> {
    validate_session_conversation_message_identities(&conversation_state.messages)
        .map_err(|error| format!("Invalid canonical conversation message identity: {error}"))?;
    validate_session_conversation_record_identities(&conversation_state.records)
        .map_err(|error| format!("Invalid canonical conversation record identity: {error}"))?;
    Ok(CoreConversationSnapshot {
        messages: conversation_state
            .messages
            .iter()
            .map(session_message_to_core_message)
            .collect(),
        records: conversation_state
            .records
            .iter()
            .map(session_record_to_core_record)
            .collect(),
        continuation: session_continuation_to_core_continuation(&conversation_state.continuation),
    })
}

/// Converts one `vulcan-core` snapshot back into the host-owned canonical session state without dropping durable records or continuation data.
/// 将一份 `vulcan-core` 快照转换回运行时持有的规范会话状态，同时不丢失持久化记录与 continuation 数据。
///
/// # Parameters
/// 参数
/// `snapshot` is the complete core snapshot whose messages, records, and continuation are converted.
/// `snapshot` 是需要转换消息、记录与 continuation 的完整 core 快照。
///
/// # Returns
/// 返回值
/// Returns the validated session state, or an identity error without exposing invalid canonical state.
/// 返回经过校验的会话状态；身份无效时返回错误且不暴露无效规范状态。
pub(crate) fn core_snapshot_to_session_conversation_state(
    snapshot: &CoreConversationSnapshot,
) -> Result<SessionConversationState, String> {
    // conversation_state preserves every core-owned canonical field before one whole-window identity validation.
    // conversation_state 在执行完整窗口身份校验前保留 core 持有的全部规范字段。
    let conversation_state = SessionConversationState {
        messages: snapshot
            .messages
            .iter()
            .map(core_message_to_session_message)
            .collect(),
        records: snapshot
            .records
            .iter()
            .map(core_record_to_session_record)
            .collect(),
        continuation: core_continuation_to_session_continuation(&snapshot.continuation),
    };
    validate_session_conversation_message_identities(&conversation_state.messages)
        .map_err(|error| format!("Invalid canonical conversation message identity: {error}"))?;
    validate_session_conversation_record_identities(&conversation_state.records)
        .map_err(|error| format!("Invalid canonical conversation record identity: {error}"))?;
    Ok(conversation_state)
}
