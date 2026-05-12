//! Shared provider-request bridge helpers reused by the executor, transport adapters, and regression tests.
//! 供执行器、传输适配器与回归测试复用的共享 Provider 请求桥接辅助逻辑。

pub(crate) use crate::agent_runtime::provider_request_bridge_messages::core_messages_to_provider_visible_history;
use crate::agent_runtime::provider_request_bridge_messages::{
    core_message_to_session_message, session_message_to_core_message,
};
use crate::agent_runtime::provider_request_bridge_records::{
    core_continuation_to_session_continuation, core_record_to_session_record,
    session_continuation_to_core_continuation, session_record_to_core_record,
};
pub(crate) use crate::agent_runtime::provider_request_bridge_rig::{
    core_message_to_rig_message, core_messages_to_rig_history,
};
pub(crate) use crate::agent_runtime::provider_request_bridge_snapshot::{
    build_openai_compatible_turn_request_snapshot, serialize_provider_request_headers,
};
pub(crate) use crate::agent_runtime::provider_request_bridge_tools::{
    resolved_tool_definition_to_core_tool_schema, resolved_tool_definition_to_rig_tool_definition,
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
