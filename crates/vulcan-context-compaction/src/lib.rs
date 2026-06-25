//! Shared context-compaction helpers reused by the host runtime and workspace UI.
//! 供宿主运行时与工作区界面复用的共享上下文压缩辅助能力。

mod planning;
mod rewrite;

pub use planning::{
    CompactionPlan, CompactionPlanInput, CompactionResult, CompactionSummaryRecord,
    build_compaction_continuation_prompt, build_compaction_request_prompt,
    extract_compaction_model_summary, extract_compaction_previous_summary, plan_context_compaction,
};
pub use rewrite::{
    compact_conversation_records_after_summary, compact_conversation_with_summary,
    compact_conversation_with_summary_until,
};
