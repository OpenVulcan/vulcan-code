//! Canonical conversation protocol models shared by WebUI state, persistence, and runtime bridges.
//! 供WebUI状态、持久化与运行时桥接共享的规范会话协议模型。

#[path = "protocol/identity.rs"]
pub(crate) mod identity;
#[path = "protocol/projection.rs"]
mod projection;
#[path = "protocol/types.rs"]
mod types;

pub use identity::{
    SessionConversationMessageIdentityError, SessionConversationRecordIdentityError,
    validate_session_conversation_message_identities,
    validate_session_conversation_record_identities,
};
pub use projection::{
    append_assistant_text_message, append_record, append_record_body_delta,
    append_session_conversation_message, append_user_text_message,
    collect_host_visible_attachments, conversation_state_from_session_messages,
    conversation_state_to_compaction_source_messages, conversation_state_to_context_records,
    conversation_state_to_session_messages, merge_session_messages_with_conversation_state_records,
};
pub(crate) use projection::{
    historical_tool_result_text_message, normalize_replayed_protocol_messages,
};
pub use types::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationRecord,
    SessionConversationState, SessionConversationToolCall, SessionProviderContinuationState,
};

#[cfg(test)]
#[path = "protocol/tests.rs"]
mod tests;
