//! Conversation hook assembly and dispatch helpers for runtime-visible extension points.
//! 面向运行时可见扩展点的会话 hook 组装与调度辅助。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::question_runtime::QuestionPrompt;
use crate::conversation_protocol::{SessionConversationPayload, SessionConversationState};
use crate::model::{PendingSessionPromptSource, SessionId, VmmProfileSyncState};
use futures_channel::oneshot;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::time::Duration;
use tokio::runtime::Handle;
#[cfg(test)]
use vulcan_hooks::HookFailurePolicy;
use vulcan_hooks::{
    HookActor, HookContext, HookDispatchReport, HookDispatchRequest, HookDispatcher, HookError,
    HookFailure, HookMode, HookPayload, HookPointDescriptor, HookRegistrationSnapshot, HookScope,
    SystemPromptHookPhase,
};
use vulcan_vmm::PostActionRequest;

/// Metadata key used by hooks to report VMM writeback side effects.
/// hook 用于报告 VMM 写回副作用的 metadata 键。
pub const HOOK_METADATA_VMM_WRITEBACK_EFFECTS: &str = "vmm_writeback_effects";
/// Metadata key used by hooks to report VMM profile synchronization effects.
/// hook 用于报告 VMM 画像同步副作用的 metadata 键。
pub const HOOK_METADATA_VMM_PROFILE_SYNC_EFFECT: &str = "vmm_profile_sync_effect";
/// Metadata key used by the built-in VMM precheck hook to return typed recall batches.
/// 内置 VMM precheck hook 用于返回类型化召回批次的 metadata 键。
pub const HOOK_METADATA_VMM_RECALL_BATCH_EFFECT: &str = "vmm_recall_batch_effect";
/// Metadata key used by the built-in VMM precheck hook to return authoritative session status.
/// 内置 VMM precheck hook 用于返回权威会话状态的 metadata 键。
pub const HOOK_METADATA_VMM_SESSION_STATUS_EFFECT: &str = "vmm_session_status_effect";

/// Hook point fired to prepare persistent session profile context independently from user recall.
/// 独立于用户召回、用于准备持久会话画像上下文的 Hook 点位。
pub const HOOK_CONVERSATION_PROFILE_PREPARE: &str = "conversation.profile.prepare";
/// Hook point fired before a user turn is sent to provider preparation.
/// 在用户回合发送到 Provider 准备阶段前触发的 hook 点位。
pub const HOOK_CONVERSATION_TURN_PRECHECK: &str = "conversation.turn.precheck";
/// Hook point fired whenever a user-authored event joins a logical turn.
/// 每当用户撰写事件加入一个逻辑回合时触发的 hook 点位。
pub const HOOK_CONVERSATION_USER_EVENT: &str = "conversation.user.event";
/// Hook point fired when the runtime ask tool opens a question.
/// 运行时 ask 工具打开问题时触发的 hook 点位。
pub const HOOK_CONVERSATION_QUESTION_ASKED: &str = "conversation.question.asked";
/// Hook point fired when the user answers a runtime ask question.
/// 用户回答运行时 ask 问题时触发的 hook 点位。
pub const HOOK_CONVERSATION_QUESTION_ANSWERED: &str = "conversation.question.answered";
/// Hook point fired when the user rejects a runtime ask question.
/// 用户拒绝运行时 ask 问题时触发的 hook 点位。
pub const HOOK_CONVERSATION_QUESTION_REJECTED: &str = "conversation.question.rejected";
/// Hook point fired when a tool call starts.
/// 工具调用开始时触发的 hook 点位。
pub const HOOK_CONVERSATION_TOOL_STARTED: &str = "conversation.tool.started";
/// Hook point fired when a tool call finishes.
/// 工具调用完成时触发的 hook 点位。
pub const HOOK_CONVERSATION_TOOL_FINISHED: &str = "conversation.tool.finished";
/// Hook point fired when a logical user turn completes with a final assistant answer.
/// 逻辑用户回合以最终助手回答完成时触发的 hook 点位。
pub const HOOK_CONVERSATION_TURN_COMPLETED: &str = "conversation.turn.completed";
/// Hook point fired when a logical user turn terminates without a final assistant answer.
/// 逻辑用户回合未产出最终助手回答而终止时触发的 hook 点位。
pub const HOOK_CONVERSATION_TURN_TERMINATED: &str = "conversation.turn.terminated";
/// Hook point fired when a session context compaction finishes.
/// 会话上下文压缩完成时触发的 hook 点位。
pub const HOOK_CONVERSATION_COMPACTED: &str = "conversation.compacted";
/// Transform hook fired before one shell_command invocation launches its process.
/// 在单次 shell_command 调用启动进程前触发的 Transform Hook。
pub const HOOK_TOOL_SHELL_COMMAND_BEFORE_EXECUTE: &str = "tool.shell_command.before_execute";
/// Transform hook fired while assembling Git commit-summary context.
/// 在组装 Git 提交摘要上下文时触发的 Transform Hook。
pub const HOOK_GIT_COMMIT_SUMMARY_CONTEXT: &str = "git.commit_summary.context";
/// Host-side safety ceiling shared by precheck, profile preparation, and completed-turn hook dispatch.
/// precheck、画像准备与完成回合 hook 调度共享的宿主侧安全兜底上限。
const CONVERSATION_HOOK_DISPATCH_TIMEOUT: Duration = Duration::from_millis(120_000);

mod canonical_inputs;
mod completion;
mod helpers;
mod precheck;
mod profile;
mod runtime;
mod user_event;
pub use user_event::ConversationUserEventRequest;
#[cfg(test)]
mod tests;
mod types;

pub use completion::{ConversationCompletionOutcome, ConversationCompletionRequest};
pub(crate) use helpers::host_hook_point_descriptors;
pub use precheck::{ConversationPrecheckOutcome, ConversationPrecheckRequest};
pub use profile::{ConversationProfileOutcome, ConversationProfileRequest};
pub use runtime::{ConversationHookRuntime, ConversationPromptHookRequests};
pub use types::{
    CanonicalInTurnHookInput, ConversationTimelineItem, ConversationTurnCompletionStatus,
    VmmProfileSyncHookEffect, VmmRecallBatchHookEffect, VmmSessionStatusHookEffect,
    VmmWritebackHookEffect, vmm_profile_sync_effect_from_report,
    vmm_recall_batch_effect_from_report, vmm_session_status_effect_from_report,
};

use helpers::*;
use types::ActiveConversationTurn;
