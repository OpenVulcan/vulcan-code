//! Canonical conversation-message bridge helpers reused by snapshot assembly and provider-visible history projection.
//! 供快照组装与 Provider 可见历史投影复用的规范会话消息桥接辅助逻辑。

use crate::agent_runtime::provider_system_update_lowering::{
    ProviderSystemUpdateLowering, ProviderSystemUpdateLoweringPolicy, lower_provider_system_update,
};
use crate::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload, SessionConversationToolCall,
};
use crate::provider_input_normalizer::normalize_provider_user_input_for_model_with_tool_bridges;
use std::collections::BTreeSet;
use vulcan_ai_contract::ModelCapabilities;
use vulcan_context::RuntimeSystemUpdateEvent;
use vulcan_core::types::{
    ConversationMessage as CoreConversationMessage, ConversationPayload as CoreConversationPayload,
    ConversationToolCall as CoreConversationToolCall,
};

mod conversions;
mod projection;
#[cfg(test)]
mod tests;

pub(crate) use conversions::{core_message_to_session_message, session_message_to_core_message};
pub(crate) use projection::{
    core_messages_to_provider_visible_history,
    core_messages_to_provider_visible_history_for_model_with_system_updates_and_tool_bridges,
};
#[cfg(test)]
pub(crate) use projection::{
    core_messages_to_provider_visible_history_for_model,
    core_messages_to_provider_visible_history_for_model_with_system_updates,
    validate_provider_visible_tool_topology,
};
