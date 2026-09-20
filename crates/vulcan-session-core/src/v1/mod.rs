//! V1 unified session fact model.
//! V1 统一会话事实模型。
//!
//! The V1 model reasserts the single-fact-source architecture: a contiguous,
//! strongly typed main event stream (`event`), typed identities (`ids`),
//! per-API/turn statistics (`stats`), the runtime lock and compaction
//! history (`context`), and the lightweight metadata document (`config`).
//! These types are build-time fixtures until the vertical switch; production
//! call sites keep using the legacy modules until then.
//! V1 模型重申单一事实源架构：连续强类型主事件流(`event`)、强类型身份
//! (`ids`)、逐 API/回合统计(`stats`)、运行时锁定与压缩历史(`context`)、
//! 轻量元数据文档(`config`)。在垂直切换前这些类型只是构建期 fixture，
//! 生产调用点在此之前继续使用旧模块。

pub mod budget;
pub mod compaction_frontier;
pub mod config;
pub mod context;
pub mod document;
pub mod errors;
pub mod event;
pub mod goal;
pub mod ids;
pub mod lifetime_usage;
pub mod projection;
pub mod service_dto;
pub mod stats;
pub mod user_prompt;

#[cfg(test)]
mod tests;

pub use budget::{
    ProviderRequestBudgetEvidence, ProviderRequestContextBreakdown, ProviderRequestTextMeasurement,
    ProviderUsageBudgetAnchor, latest_provider_usage_budget_anchor,
};
pub use config::{
    DraftState, MetadataDocument, ModeConfig, QueuedInputIdentity, SessionConfig,
    TerminalSettlementRecord, TerminalSettlementState,
};
pub use context::{
    CompactionNode, CompactionsDocument, LockedBaselineProjection, LockedBaselineSegmentInput,
    LockedPromptTarget, LockedSource, LockedSourceKind, RuntimeDocument,
    RuntimeInitializationState, RuntimeLock,
};
pub use document::{
    ActiveApiState, ActiveTurnState, ApiCompletion, ApiGroupCompletionReceipt,
    ApiTransportObservedReceipt, ClientInputSubmission, CompactionCommitInput, ContinuationRequest,
    ContinueTurnReceipt, DirtyFile, DraftMutationReceipt, EditGenerationInput,
    EnqueueInputDisposition, EnqueueInputReceipt, ForkGenerationInput, GenerationBuildOutput,
    GoalModeTransition, IdempotentMutation, PlanModeTransitionInput, PrepareAndStartApiInput,
    PreparedApiStart, PreparedToolBatchReceipt, PreparedToolReceipt, QueuedInputDraftReceipt,
    QueuedInputOrderReceipt, QueuedInputPendingTurnReceipt, QueuedInputRemovalReceipt,
    RootInputReplacement, SessionDocument, SubagentReturnBuildRequest, SubagentReturnInput,
    SubagentReturnReceipt, ToolCompletion, ToolCompletionReceipt, ToolPreparation,
    TurnCompletionTransition, TurnOrigin, TurnSuccessor,
};
pub use errors::DomainError;
pub use event::{
    ApiCallStatus, ApiEndReason, ApiErrorCategory, ApiPurposeKind, ApiStatsEvent, ApiTimings,
    AssistantReasoningEvent, AssistantSemanticStatus, AssistantTextEvent, ContinuationEligibility,
    ConversationEvent, DisplayNoteEvent, DisplayNoticeKind, EventBody, EventVisibility,
    ExecutionControlEvent, ExecutionControlKind, ExternalResourceKind, ExternalResourceRef,
    NoticeSeverity, ProviderProjectionProof, ProviderSource, ProviderSourceInput,
    QueuedInputResolutionEvent, QueuedInputResolutionKind, RedactedApiError, SubagentReturnEvent,
    SubagentReturnStatsEvent, SubagentReturnStatus, SystemContextEvent, SystemContextOperation,
    SystemContextScope, SystemRoleHint, ToolCallEvent, ToolCallStatus, ToolPayload,
    TurnBoundaryEvent, TurnOutcome, TurnStatsEvent, UserInputEvent, UserInputSource,
};
pub use ids::{
    ApiCallId, CLIENT_ACTION_ID_MAX_BYTES, ClientActionId, CompactionId, IdAllocator,
    JS_SAFE_INTEGER_MAX, LOCAL_ID_MIN, PendingInputId, RequestStreamId, ResourceId,
    SUBAGENT_TASK_ID_MAX_BYTES, Seq, SubagentTaskId, ToolCallId, TurnId,
};
pub use stats::{AggregatedUsage, StatBucket, TurnStatBuckets, UsageSnapshot, goal_budget_tokens};
pub use user_prompt::{
    UserPromptMediaBinding, UserPromptMediaDelivery, UserPromptMediaOrigin, UserPromptMediaRoute,
    UserPromptParameters, UserPromptResourceBinding, UserPromptResourceDelivery,
    UserPromptResourceDeliveryKind, UserPromptResourcePreparation, UserPromptResourceSource,
};
