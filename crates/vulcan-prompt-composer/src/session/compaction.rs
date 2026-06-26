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

/// Prompt variant used for one context-compaction request.
/// 单次上下文压缩请求使用的提示词变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextCompactionRequestPromptMode {
    /// Full-fidelity mode keeps the cloned runtime conversation visible to preserve provider cache continuity.
    /// 完整保真模式保留克隆运行时会话可见，以维持 Provider 缓存连续性。
    FullFidelityMerge,
    /// Pruned-fallback mode is used when the target model window cannot hold the full cloned source.
    /// 剪枝降级模式用于目标模型窗口无法容纳完整克隆源的情况。
    PrunedFallback,
}

/// Builds the user-facing prompt used to ask the model for one compact continuation summary.
/// 构建用于请求模型生成紧凑续写总结的用户侧提示词。
///
/// # Returns
/// # 返回值
/// The user prompt that defines the compaction summary structure and no-tool protocol.
/// 返回定义压缩摘要结构与禁止工具协议的用户提示词。
pub fn build_context_compaction_request_prompt() -> String {
    build_context_compaction_request_prompt_for_mode(
        ContextCompactionRequestPromptMode::FullFidelityMerge,
    )
}

/// Builds the user-facing prompt for a specific context-compaction source mode.
/// 为指定的上下文压缩源模式构建用户侧提示词。
///
/// # Parameters
/// # 参数
/// `mode` selects the source-scope instructions used by this compaction attempt.
/// `mode` 选择本次压缩尝试使用的源范围指令。
///
/// # Returns
/// # 返回值
/// The mode-aware user prompt that defines the summary structure and no-tool protocol.
/// 返回定义摘要结构与禁止工具协议的模式感知用户提示词。
pub fn build_context_compaction_request_prompt_for_mode(
    mode: ContextCompactionRequestPromptMode,
) -> String {
    let source_scope = match mode {
        ContextCompactionRequestPromptMode::FullFidelityMerge => {
            "Source scope:\n\
- You can see the full cloned runtime conversation for this compaction attempt.\n\
- Treat any <previous-summary> block as durable carried-forward memory, then merge newer facts into it.\n\
- Do not duplicate the retained recent tail; it may remain below the final continuation summary."
        }
        ContextCompactionRequestPromptMode::PrunedFallback => {
            "Source scope:\n\
- You may see only the clone-pruned source that fits the target model context window.\n\
- Summarize only the visible source plus any <previous-summary> durable memory; do not invent omitted details.\n\
- If pruning or truncation may hide important details, record that limitation in Progress / Blocked or Next Steps."
        }
    };
    format!(
        "CRITICAL: Respond with text only. Do not call tools. Tool calls will be rejected and will waste the only compaction turn.\n\
\n\
Create a detailed continuation summary for an AI coding workflow. The summary must let the next turn continue without losing user intent, technical state, or current work.\n\
\n\
{source_scope}\n\
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
- Structure <summary> with these Markdown sections and subsections: Goal; Constraints & Preferences; Progress / Done; Progress / In Progress; Progress / Blocked; Key Decisions; Technical Context; Relevant Files and Symbols; Errors and Fixes; User Messages and Corrections; Next Steps.\n\
- Keep every listed summary section; if a section has no durable content, write \"(none)\".\n\
- Mention exact file paths, function/type names, commands, and verification outcomes when they are needed to continue safely.\n\
- Preserve user corrections and preferences, especially any instruction that changed how work should proceed.\n\
- For Next Steps, include only the next action directly aligned with the most recent active task; do not resurrect older completed work.\n\
- When Next Steps is present, include one or two short direct quotes from the most recent user/assistant messages that prove exactly where continuation should resume.\n\
- Assume newer messages will remain below this summary, so do not repeat the retained recent tail.\n\
- Do not greet, do not ask follow-up questions, and do not claim you called tools during this compaction run.\n\
\n\
Example output structure:\n\
\n\
<example>\n\
<analysis>\n\
Check the conversation chronologically, identify durable user intent, merge any previous summary with newer facts, remove obsolete details, and verify the next step is anchored in the latest active task.\n\
</analysis>\n\
\n\
<summary>\n\
## Goal\n\
- [Concise but complete description of the user's active request and why it matters.]\n\
\n\
## Constraints & Preferences\n\
- [Still-active user instruction, preference, boundary, or acceptance rule.]\n\
- [Write \"(none)\" if there is no durable constraint.]\n\
\n\
## Progress\n\
### Done\n\
- [Concrete completed implementation, verification, decision, or investigation result.]\n\
\n\
### In Progress\n\
- [What was happening immediately before compaction, including files, commands, and state.]\n\
\n\
### Blocked\n\
- [Blocker, missing evidence, failed validation, or \"(none)\".]\n\
\n\
## Key Decisions\n\
- [Decision, accepted tradeoff, or rejected approach that still matters.]\n\
\n\
## Technical Context\n\
- [Important framework, runtime, protocol, architecture, policy, or data-flow concept.]\n\
\n\
## Relevant Files and Symbols\n\
- [path/to/file.ext]\n\
  - [Why this file matters.]\n\
  - [Relevant function, type, command, or code section if needed to continue safely.]\n\
\n\
## Errors and Fixes\n\
- [Exact error or symptom.]\n\
  - [Root cause found.]\n\
  - [Fix or mitigation applied, including user corrections when relevant.]\n\
\n\
## User Messages and Corrections\n\
- [Durable non-tool user instruction, correction, preference, or change of direction.]\n\
\n\
## Next Steps\n\
- [Only the next action aligned with the latest active request, with 1-2 verbatim quotes from the last exchange that prove the exact resume point.]\n\
</summary>\n\
</example>\n\
\n\
REMINDER: Do not call any tools. Respond with plain text only: one <analysis> block followed by one <summary> block. Tool calls will be rejected and will fail this compaction task.",
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
