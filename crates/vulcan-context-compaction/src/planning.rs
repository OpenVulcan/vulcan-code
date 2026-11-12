//! Context-compaction planning and summary-shaping helpers.
//! 上下文压缩规划与摘要整形辅助模块。

mod budget;
mod models;
mod plan;
mod summary;
mod tail;
#[cfg(test)]
mod tests;
mod time;

const PREVIOUS_SUMMARY_START_TAG: &str = "<previous-summary>";
const PREVIOUS_SUMMARY_END_TAG: &str = "</previous-summary>";
const MODEL_ANALYSIS_START_TAG: &str = "<analysis>";
const MODEL_ANALYSIS_END_TAG: &str = "</analysis>";
const MODEL_SUMMARY_START_TAG: &str = "<summary>";
const MODEL_SUMMARY_END_TAG: &str = "</summary>";
const LEGACY_SUMMARY_PREFIX: &str = "Earlier context summary:\n";
const LEGACY_SUMMARY_SUFFIX: &str = "\n\nContinue from the newer messages below";

pub use budget::{estimate_compaction_source_tokens, prepare_compaction_source_for_target_budget};
pub use models::{
    CompactionContinuationPromptOptions, CompactionInputBudget, CompactionInputPruneMode,
    CompactionInputPruneReport, CompactionPlan, CompactionPlanInput, CompactionResult,
    CompactionSummaryRecord, CompactionTailSelectionReport, PreparedCompactionSource,
};
pub use plan::{plan_context_compaction, plan_conversation_image_compaction};
pub use summary::{
    build_compaction_continuation_prompt, build_compaction_continuation_prompt_with_options,
    build_compaction_request_prompt, build_compaction_request_prompt_for_mode,
    extract_compaction_model_summary, extract_compaction_previous_summary,
    preview_compaction_model_output,
};
