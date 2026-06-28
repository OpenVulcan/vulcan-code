//! Shared session runtime-state and queued-prompt models reused by WebUI, persistence, and runtime layers.
//! 在 WebUI、持久化与运行时层之间复用的共享会话运行时状态与排队提示词模型。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_context::SessionConversationState;
use vulcan_context_injection::ContextInjectionState;
use vulcan_core::types::{ConversationAttachment, ProviderUsageSnapshot};
use vulcan_provider_config::config::{
    AiToolKind, OpenRouterEndpointRouteSelection, PersistedAiToolConfiguration,
    PersistedPromptSkillState, ReasoningEffortLevel,
};
use vulcan_vmm::{PostActionRequest, ProfileNodeEntry, VmmConfigLayer};

/// Returns whether the canonical conversation state has no durable payload.
/// 返回规范会话状态是否没有任何需要持久化的有效载荷。
fn session_conversation_state_is_empty(state: &SessionConversationState) -> bool {
    state.messages.is_empty()
        && state.records.is_empty()
        && state.continuation.provider_id.is_none()
        && state.continuation.model_id.is_none()
        && state.continuation.last_provider_message_id.is_none()
        && state.continuation.updated_at.is_none()
}

/// Maximum number of pending prompts retained in either visible or internal queues.
/// 可见队列或内部队列中允许保留的最大待发送提示词数量。
pub const PENDING_PROMPT_QUEUE_LIMIT: usize = 10;

///
/// # Parameters
/// 参数
/// `value` is the boolean reference to inspect.
/// `value` 是需要检查的布尔引用。
///
/// # Returns
/// 返回值
/// `true` when the referenced boolean is false.
/// 当引用的布尔值为 false 时返回 `true`。
fn is_false(value: &bool) -> bool {
    !*value
}

/// Returns whether one usize value is zero for serde skip helpers.
/// 为 serde 跳过辅助函数判断单个 usize 值是否为零。
///
/// # Parameters
/// 参数
/// `value` is the usize reference to inspect.
/// `value` 是需要检查的 usize 引用。
///
/// # Returns
/// 返回值
/// `true` when the referenced value is zero.
/// 当引用的值为零时返回 `true`。
fn is_zero_usize(value: &usize) -> bool {
    *value == 0
}

mod internal_queue;
mod panel_queue;
mod planning;
mod prompt_models;
mod prompt_source;
mod session_entry;
mod vmm_state;

pub use internal_queue::*;
pub use panel_queue::*;
pub use planning::*;
pub use prompt_models::*;
pub use prompt_source::*;
pub use session_entry::*;
pub use vmm_state::*;

#[cfg(test)]
mod tests;
