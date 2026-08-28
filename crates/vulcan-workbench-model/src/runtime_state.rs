//! Shared process-local session adapters and queued-prompt models reused by WebUI and runtime layers.
//! 在 WebUI 与运行时层之间复用的进程内会话适配状态与排队提示词模型。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use vulcan_context::{
    ContextCompactionWindowNodeId, ContextWindowPolicy, SessionConversationState,
};
use vulcan_context_injection::ContextInjectionState;
use vulcan_core::types::ConversationAttachment;
use vulcan_provider_config::config::{PersistedPromptSkillState, ReasoningEffortLevel};
use vulcan_session_core::SessionRuntimeKernel;
use vulcan_vmm::{PostActionRequest, ProfileNodeEntry, VmmConfigLayer};

use crate::SessionKnowledgeLibrarySelection;

/// Maximum number of pending prompts retained in the visible V1 queue projection.
/// 可见 V1 队列投影中允许保留的最大待发送提示词数量。
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

mod media_references;
mod planning;
mod prompt_models;
mod prompt_source;
mod session_entry;
mod vmm_state;

pub use media_references::*;
pub use planning::*;
pub use prompt_models::*;
pub use prompt_source::*;
pub use session_entry::*;
pub use vmm_state::*;

#[cfg(test)]
mod tests;
