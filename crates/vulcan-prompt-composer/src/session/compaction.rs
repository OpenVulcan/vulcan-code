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

/// Builds the user-facing prompt used to ask the model for one compact continuation summary.
/// 构建用于请求模型生成紧凑续写总结的用户侧提示词。
pub fn build_context_compaction_request_prompt() -> String {
    String::from(
        "Summarize the earlier session context into a compact continuation brief for an AI coding workflow.\n\
\n\
Requirements:\n\
- Preserve the user's main goal, constraints, decisions, accepted tradeoffs, and unfinished work.\n\
- Mention concrete file paths, symbols, commands, errors, and verification results when they still matter.\n\
- Highlight any pending next steps or follow-up checks.\n\
- Prefer dense factual prose or short headings over storytelling.\n\
- Do not greet, do not ask follow-up questions, and do not mention tools.\n\
- Return plain text only without code fences.\n\
- Assume newer messages will remain below this summary, so do not repeat them.",
    )
}

/// Builds the developer-side instruction used to keep compaction requests focused and tool-free.
/// 构建用于保持压缩请求聚焦且不调用工具的开发者侧指令。
pub fn build_context_compaction_developer_segment() -> String {
    String::from(
        "Internal host task: summarize earlier conversation context for continuation.\n\
         - Do not call tools.\n\
         - Do not delegate.\n\
         - Do not ask questions.\n\
         - Return plain text only.",
    )
}
