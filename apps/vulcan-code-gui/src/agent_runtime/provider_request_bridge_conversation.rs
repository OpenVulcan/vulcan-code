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
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;

/// Converts one session-owned canonical conversation state into the `vulcan-core` snapshot shape.
/// 将一份会话持有的规范会话状态转换为 `vulcan-core` 快照形状。
pub(crate) fn build_core_conversation_snapshot(
    conversation_state: &SessionConversationState,
) -> CoreConversationSnapshot {
    CoreConversationSnapshot {
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
    }
}

/// Converts one `vulcan-core` snapshot back into the GUI-owned canonical session state without dropping durable records or continuation data.
/// 将一份 `vulcan-core` 快照转换回 GUI 持有的规范会话状态，同时不丢失持久化记录与 continuation 数据。
pub(crate) fn core_snapshot_to_session_conversation_state(
    snapshot: &CoreConversationSnapshot,
) -> SessionConversationState {
    SessionConversationState {
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
    }
}
