//! Shared context-compaction helpers reused by the host runtime and workspace UI.
//! 供宿主运行时与工作区界面复用的共享上下文压缩辅助能力。

mod planning;
mod rewrite;

pub use planning::{
    CompactionContinuationPromptOptions, CompactionInputBudget, CompactionInputPruneMode,
    CompactionInputPruneReport, CompactionPlan, CompactionPlanInput, CompactionResult,
    CompactionSummaryRecord, PreparedCompactionSource, build_compaction_continuation_prompt,
    build_compaction_continuation_prompt_with_options, build_compaction_request_prompt,
    build_compaction_request_prompt_for_mode, estimate_compaction_source_tokens,
    extract_compaction_model_summary, extract_compaction_previous_summary, plan_context_compaction,
    prepare_compaction_source_for_target_budget,
};
pub use rewrite::{
    compact_conversation_records_after_summary, compact_conversation_with_summary,
    compact_conversation_with_summary_until,
};
pub use vulcan_prompt_composer::ContextCompactionRequestPromptMode;
