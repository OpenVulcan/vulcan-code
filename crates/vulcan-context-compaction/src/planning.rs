//! Context-compaction planning and summary-shaping helpers.
//! 上下文压缩规划与摘要整形辅助模块。

use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_context::{
    ContextCompactionPolicy, SessionCompactionEntry, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind,
};

/// Stateless summary record returned by one compaction result before the GUI turns it into its own hidden-node shape.
/// 单次压缩结果返回的无状态摘要记录，供图形界面在落地前转换为自身隐藏节点形态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionSummaryRecord {
    /// Transcript-facing message kind used by the hidden node.
    /// 隐藏节点使用的转录消息类型。
    pub kind: SessionMessageKind,
    /// Durable hidden record kind persisted into the transcript history.
    /// 持久化到转录历史中的隐藏记录类型。
    pub record_kind: SessionMessageRecordKind,
    /// Internal title stored on the hidden node.
    /// 存储在隐藏节点上的内部标题。
    pub title: String,
    /// Internal body stored on the hidden node.
    /// 存储在隐藏节点上的内部正文。
    pub body: String,
}

/// Input consumed by the context compaction planner.
/// 供上下文压缩规划器消费的输入参数。
pub struct CompactionPlanInput<'a> {
    /// Stable session label used only to build one deterministic compaction identifier.
    /// 仅用于构建稳定压缩标识的稳定会话标签。
    pub session_label: &'a str,
    /// Context-participating transcript history for the current compaction window.
    /// 当前压缩窗口参与上下文的转录历史。
    pub messages: &'a [SessionMessageEntry],
    /// Full transcript history for the session in persisted order so compaction metadata can keep stable absolute turn indexes.
    /// 以持久化顺序提供的完整会话转录历史，用于让压缩元数据保留稳定的绝对轮次索引。
    pub all_messages: &'a [SessionMessageEntry],
    /// Optional current context-start message identifier used as the compaction input anchor.
    /// 作为压缩输入锚点使用的可选当前上下文起始消息标识。
    pub context_start_message_id: Option<&'a str>,
    /// Policy controlling the retained tail and pruning behavior for this compaction pass.
    /// 控制本次压缩保留尾部与剪枝行为的策略。
    pub policy: ContextCompactionPolicy,
}

/// Planned context-compaction window extracted before any real summarization request is dispatched.
/// 在派发真实总结请求之前提取出的上下文压缩规划窗口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionPlan {
    /// Context-participating source messages that should be summarized into the next synthetic prompt.
    /// 需要被总结为下一条合成提示词的参与上下文源消息集合。
    pub source_messages: Vec<SessionMessageEntry>,
    /// Optional source tail message identifier covered by the planned compaction window.
    /// 被计划压缩窗口覆盖的可选源历史尾消息标识。
    pub source_until_message_id: Option<String>,
    /// Optional last user-turn index covered by the compaction source window.
    /// 被压缩源窗口覆盖的可选最后一个用户轮次索引。
    pub source_until_turn_index: Option<i64>,
    /// Optional first retained user-turn index kept verbatim after the compaction rewrite.
    /// 压缩改写后按原样保留的可选第一个用户轮次索引。
    pub retained_tail_start_turn_index: Option<i64>,
    /// Stable compaction identifier prepared for the next checkpoint.
    /// 为下一枚检查点准备的稳定压缩标识。
    pub compaction_id: String,
    /// Created-at timestamp for the next checkpoint.
    /// 下一枚检查点的创建时间戳。
    pub created_at: String,
    /// Policy used when the compaction window was planned.
    /// 规划该压缩窗口时使用的策略。
    pub policy: ContextCompactionPolicy,
}

/// Output produced by the context compaction planner before the UI assigns the final summary message id.
/// 由上下文压缩规划器产出、且在界面分配最终摘要消息标识前的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionResult {
    /// Hidden summary record that should be inserted into the transcript first.
    /// 应先插入转录历史的隐藏摘要记录。
    pub summary_record: CompactionSummaryRecord,
    /// Optional source tail message identifier covered by the new summary.
    /// 新摘要覆盖的可选源历史尾消息标识。
    pub source_until_message_id: Option<String>,
    /// Optional last user-turn index covered by the compaction source window.
    /// 被压缩源窗口覆盖的可选最后一个用户轮次索引。
    pub source_until_turn_index: Option<i64>,
    /// Optional first retained user-turn index kept verbatim after the compaction rewrite.
    /// 压缩改写后按原样保留的可选第一个用户轮次索引。
    pub retained_tail_start_turn_index: Option<i64>,
    /// Stable compaction identifier prepared by the planner.
    /// 规划器预先生成的稳定压缩标识。
    pub compaction_id: String,
    /// Created-at timestamp for the new checkpoint.
    /// 新检查点的创建时间戳。
    pub created_at: String,
    /// Policy that must be used when applying this compaction result to the canonical conversation.
    /// 将该压缩结果应用到规范会话时必须使用的策略。
    pub policy: ContextCompactionPolicy,
}

/// Plans the compaction step and returns the source slice plus checkpoint metadata when compaction is needed.
/// 规划压缩步骤，并在需要压缩时返回源消息切片与检查点元数据。
pub fn plan_context_compaction(input: CompactionPlanInput<'_>) -> Option<CompactionPlan> {
    let context_start_index = input
        .context_start_message_id
        .and_then(|context_start_message_id| {
            input
                .messages
                .iter()
                .position(|message| message.id == context_start_message_id)
        })
        .unwrap_or(0);
    let context_candidates = input.messages[context_start_index..]
        .iter()
        .filter(|message| message.contributes_to_context())
        .cloned()
        .collect::<Vec<_>>();

    let retained_tail_start = recent_tail_start_index(&context_candidates, input.policy);
    if retained_tail_start == 0 {
        return None;
    }

    let compaction_source = &context_candidates[..retained_tail_start];
    let source_until_message_id = compaction_source.last().map(|message| message.id.clone());
    let source_until_turn_index = source_until_message_id
        .as_deref()
        .and_then(|message_id| absolute_turn_index_for_message(input.all_messages, message_id));
    let retained_tail_start_turn_index = context_candidates
        .get(retained_tail_start)
        .and_then(|message| absolute_turn_index_for_message(input.all_messages, &message.id));
    let created_at = current_unix_millis_text();

    Some(CompactionPlan {
        source_messages: compaction_source.to_vec(),
        source_until_message_id,
        source_until_turn_index,
        retained_tail_start_turn_index,
        compaction_id: format!("session-{}-compaction-{created_at}", input.session_label),
        created_at,
        policy: input.policy,
    })
}

impl CompactionResult {
    /// Materializes the final compaction checkpoint once the hidden summary message has been persisted.
    /// 当隐藏摘要消息已经持久化后，物化最终压缩检查点。
    pub fn materialize_checkpoint(&self, summary_message_id: String) -> SessionCompactionEntry {
        SessionCompactionEntry {
            id: self.compaction_id.clone(),
            source_until_message_id: self.source_until_message_id.clone(),
            source_until_turn_index: self.source_until_turn_index,
            context_start_message_id: Some(summary_message_id.clone()),
            context_budget_fingerprint: None,
            context_assembly_fingerprint: None,
            retained_tail_start_turn_index: self.retained_tail_start_turn_index,
            summary_message_id: Some(summary_message_id),
            created_at: self.created_at.clone(),
        }
    }

    /// Builds one prompt-based compaction result from a model-produced summary text.
    /// 根据模型产出的总结文本构建一份提示词式压缩结果。
    pub fn from_model_summary(plan: &CompactionPlan, summary_text: impl Into<String>) -> Self {
        build_compaction_result_from_summary_text(plan, summary_text.into())
    }

    /// Builds one fallback compaction result from the legacy host-side summary lines when prompt-based compaction cannot complete.
    /// 当提示词式压缩无法完成时，根据旧版宿主侧摘要行构建一份回退压缩结果。
    pub fn from_legacy_plan(plan: &CompactionPlan) -> Self {
        let summary_text =
            build_legacy_compaction_summary_text(&plan.source_messages, plan.policy.prune);
        build_compaction_result_from_summary_text(plan, summary_text)
    }
}

/// Builds the internal user prompt used to ask the model for one compact continuation summary.
/// 构建用于请求模型生成紧凑续写总结的内部用户提示词。
pub fn build_compaction_request_prompt() -> String {
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

/// Builds the synthetic continuation prompt injected as the first user message after compaction completes.
/// 构建在压缩完成后注入为首条用户消息的合成续写提示词。
pub fn build_compaction_continuation_prompt(summary_text: &str) -> String {
    let normalized_summary = normalize_compaction_summary_text(summary_text);
    format!(
        "This session is being continued from earlier context that was compacted to fit the model context window.\n\
\n\
Earlier context summary:\n\
{normalized_summary}\n\
\n\
Continue from the newer messages below without restarting the conversation, repeating this summary, or asking for the same information again."
    )
}

/// Normalizes one compaction summary into a non-empty plain-text block before it becomes the synthetic continuation prompt body.
/// 在把压缩总结转换为合成续写提示词正文之前，将其规范化为非空纯文本块。
fn normalize_compaction_summary_text(summary_text: &str) -> String {
    let trimmed = summary_text.trim();
    if trimmed.is_empty() {
        String::from("No earlier context details were preserved.")
    } else {
        trimmed.to_string()
    }
}

/// Builds one prompt-shaped compaction result from an already prepared summary text and one planned compaction window.
/// 根据一段已准备好的总结文本与一份压缩规划窗口构建提示词形态的压缩结果。
fn build_compaction_result_from_summary_text(
    plan: &CompactionPlan,
    summary_text: String,
) -> CompactionResult {
    let continuation_prompt = build_compaction_continuation_prompt(&summary_text);
    CompactionResult {
        summary_record: CompactionSummaryRecord {
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::CompactionSummary,
            title: String::from("Context Continuation Prompt"),
            body: continuation_prompt,
        },
        source_until_message_id: plan.source_until_message_id.clone(),
        source_until_turn_index: plan.source_until_turn_index,
        retained_tail_start_turn_index: plan.retained_tail_start_turn_index,
        compaction_id: plan.compaction_id.clone(),
        created_at: plan.created_at.clone(),
        policy: plan.policy,
    }
}

/// Builds the legacy host-side summary text retained as a fallback when prompt-based model compaction cannot complete.
/// 构建在提示词式模型压缩无法完成时保留的旧版宿主侧回退总结文本。
fn build_legacy_compaction_summary_text(messages: &[SessionMessageEntry], prune_tools: bool) -> String {
    messages
        .iter()
        .map(|message| compaction_summary_line_for_message(message, prune_tools))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Resolves the absolute persisted user-turn index for one transcript message identifier by replaying user-turn boundaries over the full session transcript history.
/// 通过在完整会话转录历史上重放用户轮次边界，解析某条转录消息标识对应的绝对持久化用户轮次索引。
fn absolute_turn_index_for_message(messages: &[SessionMessageEntry], message_id: &str) -> Option<i64> {
    let mut current_turn_index = -1_i64;

    for message in messages {
        if matches!(message.kind, SessionMessageKind::User) {
            current_turn_index = current_turn_index.saturating_add(1);
        }
        if message.id == message_id {
            return Some(current_turn_index.max(0));
        }
    }

    None
}

/// Selects the first context message that should remain verbatim after compaction.
/// 选择压缩后应按原样保留的第一条上下文消息。
fn recent_tail_start_index(messages: &[SessionMessageEntry], policy: ContextCompactionPolicy) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let target_turns = policy.tail_turns.max(1usize);
    let segments = session_tail_segments(messages);
    let mut retained_start = messages.len().saturating_sub(1);
    let mut retained_tokens = 0usize;
    let mut retained_turns = 0usize;

    for segment in segments.iter().rev() {
        if retained_turns >= target_turns {
            break;
        }
        let segment_tokens = estimated_session_tail_tokens(&messages[segment.start..segment.end]);
        if retained_turns == 0 || retained_tokens + segment_tokens <= policy.preserve_recent_tokens {
            retained_start = segment.start;
            retained_tokens = retained_tokens.saturating_add(segment_tokens);
            retained_turns += 1;
        } else {
            break;
        }
    }

    retained_start
}

/// Transcript-index segment representing one user turn plus following assistant-visible entries.
/// 表示一个用户轮次及其后续助手可见条目的转录索引段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionTailSegment {
    /// Inclusive segment start index.
    /// 段起始索引，包含该位置。
    start: usize,
    /// Exclusive segment end index.
    /// 段结束索引，不包含该位置。
    end: usize,
}

/// Builds transcript tail segments at user-turn boundaries.
/// 按用户轮次边界构建转录尾部段。
fn session_tail_segments(messages: &[SessionMessageEntry]) -> Vec<SessionTailSegment> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut user_starts = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| message_is_user_turn_boundary(message).then_some(index))
        .collect::<Vec<_>>();
    if user_starts.is_empty() {
        return vec![SessionTailSegment {
            start: 0,
            end: messages.len(),
        }];
    }
    if user_starts[0] != 0 {
        user_starts.insert(0, 0);
    }

    user_starts
        .iter()
        .enumerate()
        .map(|(position, start)| SessionTailSegment {
            start: *start,
            end: user_starts
                .get(position + 1)
                .copied()
                .unwrap_or(messages.len()),
        })
        .collect()
}

/// Returns whether one transcript entry starts a user-authored turn.
/// 返回一条转录项是否标记一个用户发起轮次的开始。
fn message_is_user_turn_boundary(message: &SessionMessageEntry) -> bool {
    message.kind == SessionMessageKind::User
        && message.record_kind == SessionMessageRecordKind::Transcript
}

/// Estimates the approximate token footprint of retained transcript entries.
/// 估算保留转录项的近似 Token 占用。
fn estimated_session_tail_tokens(messages: &[SessionMessageEntry]) -> usize {
    messages.iter().map(estimate_session_message_tokens).sum()
}

/// Estimates one transcript entry with a lightweight text heuristic.
/// 使用轻量文本启发式估算一条转录项。
fn estimate_session_message_tokens(message: &SessionMessageEntry) -> usize {
    estimate_text_tokens(&message.title)
        + estimate_text_tokens(&message.body)
        + message
            .metadata
            .as_ref()
            .and_then(|metadata| serde_json::to_string(metadata).ok())
            .map(|metadata| estimate_text_tokens(&metadata))
            .unwrap_or_default()
        + 16usize
}

/// Estimates plain text using the same character heuristic as the context budget path.
/// 使用与上下文预算路径一致的字符启发式估算纯文本。
fn estimate_text_tokens(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    ((trimmed.chars().count() + 3usize) / 4usize).max(1usize)
}

/// Builds one compaction summary line and optionally prunes old tool-output bodies.
/// 构建单条压缩摘要行，并可选剪枝旧工具输出正文。
fn compaction_summary_line_for_message(message: &SessionMessageEntry, prune_tools: bool) -> String {
    let label = canonical_compaction_message_label(message);
    let title = canonical_compaction_message_title(message);
    let preview = if prune_tools
        && matches!(
            message.record_kind,
            SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
        ) {
        String::from("Tool output was compacted into this placeholder.")
    } else {
        truncate_preview_lines(&message.body, 1, 88)
    };

    if title == label {
        format!("{label}: {preview}")
    } else {
        format!("{label} · {title}: {preview}")
    }
}

/// Resolves the stable English label used for model-facing compaction summaries.
/// 解析供面向模型的压缩摘要使用的稳定英文标签。
fn canonical_compaction_message_label(message: &SessionMessageEntry) -> &'static str {
    match message.record_kind {
        SessionMessageRecordKind::Transcript => message.kind.label(),
        SessionMessageRecordKind::ThinkingOutput => "THINKING",
        SessionMessageRecordKind::AssistantOutput => "OUTPUT",
        SessionMessageRecordKind::ToolCallOutput => "TOOL_CALL",
        SessionMessageRecordKind::ToolResultOutput => "TOOL_RESULT",
        SessionMessageRecordKind::SummaryOutput => "SUMMARY",
        SessionMessageRecordKind::ExploreOutput => "EXPLORE",
        SessionMessageRecordKind::CompactionSummary => "COMPACTION",
        SessionMessageRecordKind::TitleOutput => "TITLE",
        SessionMessageRecordKind::TurnTerminated => "TERMINATED",
        SessionMessageRecordKind::TurnStats => "TURN_STATS",
    }
}

/// Resolves the stable English headline used for model-facing compaction summaries without depending on GUI locale.
/// 解析供面向模型的压缩摘要使用的稳定英文标题，并且不依赖图形界面语言。
fn canonical_compaction_message_title(message: &SessionMessageEntry) -> String {
    match message.record_kind {
        SessionMessageRecordKind::Transcript => match message.kind {
            SessionMessageKind::User => String::from("User Prompt"),
            SessionMessageKind::Assistant => String::from("Assistant Message"),
            SessionMessageKind::Error => String::from("System Error"),
        },
        SessionMessageRecordKind::ThinkingOutput => String::from("Thinking"),
        SessionMessageRecordKind::AssistantOutput => String::from("Output"),
        SessionMessageRecordKind::ToolCallOutput => canonical_tool_summary_title(message, "Tool Call"),
        SessionMessageRecordKind::ToolResultOutput => {
            canonical_tool_summary_title(message, "Tool Result")
        }
        SessionMessageRecordKind::SummaryOutput => String::from("Execution Summary"),
        SessionMessageRecordKind::ExploreOutput => String::from("Explore Context"),
        SessionMessageRecordKind::CompactionSummary => String::from("Context Compaction Summary"),
        SessionMessageRecordKind::TitleOutput => String::from("Title Update"),
        SessionMessageRecordKind::TurnTerminated => String::from("Turn Terminated"),
        SessionMessageRecordKind::TurnStats => String::from("Turn Statistics"),
    }
}

/// Builds a stable English tool summary title from normalized metadata first and legacy persisted titles second.
/// 优先根据规范化元数据、其次根据旧版持久化标题构建稳定英文工具摘要标题。
fn canonical_tool_summary_title(message: &SessionMessageEntry, prefix: &str) -> String {
    let tool_name = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| legacy_tool_name_from_title(&message.title));

    tool_name
        .map(|tool_name| format!("{prefix} {tool_name}"))
        .unwrap_or_else(|| prefix.to_string())
}

/// Extracts a best-effort tool name from legacy persisted transcript headlines.
/// 从旧版持久化转录标题中提取最佳努力工具名称。
fn legacy_tool_name_from_title(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if let Some(tool_name) = trimmed
        .strip_prefix("调用 ")
        .and_then(|value| value.strip_suffix(" 工具"))
    {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }
    if let Some(tool_name) = trimmed.strip_prefix("工具返回 ") {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }
    if let Some(tool_name) = trimmed.strip_prefix("Tool Call ") {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }
    if let Some(tool_name) = trimmed.strip_prefix("Tool Result ") {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }

    None
}

/// Truncates one multi-line preview into a bounded, ellipsis-terminated single string.
/// 将多行预览截断为带省略号且受限的单条字符串。
fn truncate_preview_lines(value: &str, max_lines: usize, max_chars: usize) -> String {
    let flattened = value
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    let mut truncated = String::new();
    for character in flattened.chars() {
        if truncated.chars().count() >= max_chars {
            truncated.push('…');
            break;
        }
        truncated.push(character);
    }
    if truncated.is_empty() {
        flattened
    } else {
        truncated
    }
}

/// Returns the current Unix-milliseconds timestamp as a stable decimal string.
/// 以稳定十进制字符串形式返回当前 Unix 毫秒时间戳。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
