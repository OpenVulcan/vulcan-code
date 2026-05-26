/// Prompt compaction effect observed by the composer.
/// 组合器观察到的提示词压缩效果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptCompactionSignal {
    /// No compaction-related rebuild is needed.
    /// 不需要压缩相关重建。
    None,
    /// Rebuild the prompt baseline on the next turn.
    /// 在下一轮重建提示词基线。
    RebuildFromBaseline,
}
