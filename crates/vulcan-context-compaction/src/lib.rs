//! Shared context-compaction helpers reused by the host runtime and workspace UI.
//! 供宿主运行时与工作区界面复用的共享上下文压缩辅助能力。

mod planning;
mod preview;
mod rewrite;

pub use planning::{
    CompactionContinuationPromptOptions, CompactionInputBudget, CompactionInputPruneMode,
    CompactionInputPruneReport, CompactionPlan, CompactionPlanInput, CompactionResult,
    CompactionSummaryRecord, CompactionTailSelectionReport, PreparedCompactionSource,
    build_compaction_continuation_prompt, build_compaction_continuation_prompt_with_options,
    build_compaction_request_prompt, build_compaction_request_prompt_for_mode,
    estimate_compaction_source_tokens, extract_compaction_model_summary,
    extract_compaction_previous_summary, plan_context_compaction,
    plan_conversation_image_compaction, prepare_compaction_source_for_target_budget,
    preview_compaction_model_output,
};
pub use preview::{
    TOOL_OUTPUT_PREVIEW_HEAD_TOKENS, TOOL_OUTPUT_PREVIEW_MARKER_PREFIX,
    TOOL_OUTPUT_PREVIEW_TAIL_TOKENS, TOOL_OUTPUT_PREVIEW_THRESHOLD_TOKENS, ToolOutputPreview,
    preview_tool_output,
};
pub use rewrite::{
    ConversationRecordCompactionReport, compact_conversation_records_after_summary,
    compact_conversation_records_after_summary_with_retained_ids,
    compact_conversation_with_compaction_result, compact_conversation_with_summary,
    compact_conversation_with_summary_until,
};
pub use vulcan_prompt_composer::ContextCompactionRequestPromptMode;
