use crate::types::PromptTarget;

/// Return the stable target ordering rank.
/// 返回稳定目标排序权重。
pub fn prompt_target_rank(target: PromptTarget) -> u8 {
    match target {
        PromptTarget::System => 0,
        PromptTarget::Developer => 1,
        PromptTarget::User => 2,
        PromptTarget::ToolMetadata => 3,
        PromptTarget::DiagnosticsOnly => 4,
    }
}
