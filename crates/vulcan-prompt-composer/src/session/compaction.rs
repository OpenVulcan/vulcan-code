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
///
/// # Returns
/// # 返回值
/// The user prompt that defines the compaction summary structure and no-tool protocol.
/// 返回定义压缩摘要结构与禁止工具协议的用户提示词。
pub fn build_context_compaction_request_prompt() -> String {
    String::from(
        "CRITICAL: Respond with text only. Do not call tools. Tool calls will be rejected and will waste the only compaction turn.\n\
\n\
Create a detailed continuation summary for an AI coding workflow. The summary must let the next turn continue without losing user intent, technical state, or current work.\n\
\n\
Output format:\n\
- Respond with an <analysis> block followed by a <summary> block.\n\
- The <analysis> block is only a private drafting checklist and will be discarded.\n\
- The <summary> block is the only durable content that will remain in context.\n\
- Do not use code fences.\n\
\n\
Analysis checklist:\n\
- Chronologically identify the user's explicit requests and intent changes.\n\
- Identify key decisions, constraints, accepted tradeoffs, still-active preferences, and architectural concepts.\n\
- Identify concrete file paths, symbols, commands, errors, fixes, tests, and verification results that still matter.\n\
- Identify solved problems, ongoing troubleshooting, completed work, pending tasks, current work, and the exact next step needed to continue.\n\
- Preserve the user's own non-tool messages, especially corrections or changes in direction.\n\
- Distinguish durable memory from obsolete or duplicated context.\n\
\n\
Summary requirements:\n\
- If the input contains a <previous-summary> block, earlier compacted summary, or continuation prompt, treat it as durable memory rather than ordinary dialogue.\n\
- Preserve still-relevant details from earlier summaries by carrying them forward with minimal rewriting; only remove details that are clearly obsolete, contradicted, or duplicated by newer context.\n\
- Focus new summarization effort on context added after the previous summary, then merge it into the carried-forward memory.\n\
- Include these sections in <summary>: Primary Request and Intent; Key Technical Concepts; Files and Code Sections; Errors and Fixes; Problem Solving; All User Messages; Completed Work; Pending Tasks; Current Work; Optional Next Step.\n\
- Mention exact file paths, function/type names, commands, and verification outcomes when they are needed to continue safely.\n\
- Preserve user corrections and preferences, especially any instruction that changed how work should proceed.\n\
- For Optional Next Step, include only the next step directly aligned with the most recent active task; do not resurrect older completed work.\n\
- When Optional Next Step is present, include one or two short direct quotes from the most recent user/assistant messages that prove exactly where continuation should resume.\n\
- Assume newer messages will remain below this summary, so do not repeat the retained recent tail.\n\
- Do not greet, do not ask follow-up questions, and do not claim you called tools during this compaction run.",
    )
}

/// Builds the developer-side instruction used to keep compaction requests focused and tool-free.
/// 构建用于保持压缩请求聚焦且不调用工具的开发者侧指令。
///
/// # Returns
/// # 返回值
/// The developer instruction segment that constrains internal compaction behavior.
/// 返回约束内部压缩行为的开发者指令片段。
pub fn build_context_compaction_developer_segment() -> String {
    String::from(
        "Internal host task: summarize earlier conversation context for continuation.\n\
         - Do not call tools.\n\
         - Tool calls will be rejected and will waste this only compaction turn.\n\
         - Do not delegate.\n\
         - Do not ask questions.\n\
         - Return exactly one <analysis> block followed by one <summary> block.\n\
         - Put durable continuation content only inside <summary>.",
    )
}
