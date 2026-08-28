use serde_json::Value;
use vulcan_ai_contract::{
    canonicalize_codex_responses_replay_items, is_replayable_codex_responses_item,
};
use vulcan_context::{SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind};
use vulcan_prompt_composer::{
    ContextCompactionRequestPromptMode, build_context_compaction_request_prompt,
    build_context_compaction_request_prompt_for_mode,
};

use super::{
    CompactionContinuationPromptOptions, CompactionPlan, CompactionResult, CompactionSummaryRecord,
    LEGACY_SUMMARY_PREFIX, LEGACY_SUMMARY_SUFFIX, MODEL_ANALYSIS_END_TAG, MODEL_ANALYSIS_START_TAG,
    MODEL_SUMMARY_END_TAG, MODEL_SUMMARY_START_TAG, PREVIOUS_SUMMARY_END_TAG,
    PREVIOUS_SUMMARY_START_TAG,
};
impl CompactionResult {
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

    /// Builds a route-bound compaction result from the standalone Responses compact endpoint.
    /// 根据独立 Responses compact 端点构建绑定路由的压缩结果。
    ///
    /// # Parameters
    /// 参数
    /// `plan` supplies the local checkpoint identity and retained-tail policy; `route` freezes
    /// the provider identity; `output_items`
    /// contains the provider-returned opaque items that must be replayed verbatim.
    /// `plan` 提供本地检查点身份与尾部保留策略；`output_items` 包含必须原样重放的供应商不透明项。
    ///
    /// # Returns
    /// 返回值
    /// A compaction result with a safe diagnostic summary and validated opaque item list.
    /// 返回带安全诊断摘要且已验证不透明项列表的压缩结果。
    pub fn from_remote_responses(
        plan: &CompactionPlan,
        route: super::ProviderContextRoute,
        output_items: Vec<Value>,
    ) -> Result<Self, String> {
        Self::from_remote_responses_with_metadata(plan, route, output_items, None, None)
    }

    /// Builds a route-bound remote compaction result while retaining optional upstream metadata.
    /// 构建绑定路由的远端压缩结果，并保留可选上游元数据。
    ///
    /// # Parameters
    /// 参数
    /// `usage` and `response_id` are diagnostic metadata and are never used to replay opaque items.
    /// `usage` 与 `response_id` 仅用于诊断，绝不用于重放不透明项。
    pub fn from_remote_responses_with_metadata(
        plan: &CompactionPlan,
        route: super::ProviderContextRoute,
        mut output_items: Vec<Value>,
        usage: Option<Value>,
        response_id: Option<String>,
    ) -> Result<Self, String> {
        canonicalize_codex_responses_replay_items(&mut output_items);
        let contains_compaction = output_items
            .iter()
            .any(|item| item.get("type").and_then(Value::as_str) == Some("compaction"));
        if output_items.is_empty()
            || !output_items.iter().all(is_replayable_codex_responses_item)
            || !contains_compaction
        {
            return Err(String::from(
                "Remote Responses compaction returned an invalid ordered replayable item list.",
            ));
        }
        let summary_text = format!(
            "Remote Responses compaction checkpoint installed ({} provider item{}).",
            output_items.len(),
            if output_items.len() == 1 { "" } else { "s" },
        );
        let mut result = build_compaction_result_from_summary_text(plan, summary_text.clone());
        // remoteSummary is host diagnostics only; opaque provider items own continuation semantics.
        // remoteSummary 仅用于宿主诊断；续写语义由不透明供应商项持有。
        result.summary_record.kind = SessionMessageKind::Assistant;
        result.summary_record.title = String::from("Remote Responses Checkpoint");
        result.summary_record.body = summary_text.clone();
        result.summary_record.preview = summary_text;
        result.provider_context_items = output_items;
        result.provider_context_route = Some(route);
        result.provider_context_usage = usage;
        result.provider_context_response_id = response_id;
        Ok(result)
    }
}

/// Builds the internal user prompt used to ask the model for one compact continuation summary.
/// 构建用于请求模型生成紧凑续写总结的内部用户提示词。
pub fn build_compaction_request_prompt() -> String {
    build_context_compaction_request_prompt()
}

/// Builds the internal user prompt for one mode-specific compact continuation summary.
/// 构建指定模式的紧凑续写总结内部用户提示词。
///
/// # Parameters
/// # 参数
/// `mode` selects the compaction source assumptions explained to the model.
/// `mode` 选择告知模型的压缩源假设。
///
/// # Returns
/// # 返回值
/// The mode-aware prompt used by prompt-based compaction attempts.
/// 返回提示词式压缩尝试使用的模式感知提示词。
pub fn build_compaction_request_prompt_for_mode(
    mode: ContextCompactionRequestPromptMode,
) -> String {
    build_context_compaction_request_prompt_for_mode(mode)
}

/// Builds the synthetic continuation prompt injected as the first user message after compaction completes.
/// 构建在压缩完成后注入为首条用户消息的合成续写提示词。
pub fn build_compaction_continuation_prompt(summary_text: &str) -> String {
    build_compaction_continuation_prompt_with_options(
        summary_text,
        CompactionContinuationPromptOptions::default(),
    )
}

/// Builds the synthetic continuation prompt with explicit post-compaction context facts.
/// 根据明确的压缩后上下文事实构建合成续写提示词。
///
/// # Parameters
/// # 参数
/// `summary_text` is the durable summary payload to wrap.
/// `summary_text` 是需要包装的持久摘要正文。
/// `options` contains facts about the post-compaction retained context.
/// `options` 包含压缩后保留上下文的事实。
///
/// # Returns
/// # 返回值
/// The synthetic continuation prompt inserted as the new context anchor.
/// 返回作为新上下文锚点插入的合成续写提示词。
pub fn build_compaction_continuation_prompt_with_options(
    summary_text: &str,
    options: CompactionContinuationPromptOptions,
) -> String {
    let normalized_summary = normalize_compaction_summary_text(summary_text);
    let retained_tail_note = if options.recent_messages_preserved {
        "Recent messages after this summary are preserved verbatim below. Treat them as the live continuation and do not repeat them.\n\n"
    } else {
        ""
    };
    format!(
        "This session is being continued from earlier context that was compacted to fit the model context window.\n\
\n\
<previous-summary>\n\
{normalized_summary}\n\
</previous-summary>\n\
\n\
{retained_tail_note}\
Continue from the newer messages below without restarting the conversation, repeating this summary, or asking for the same information again."
    )
}

/// Extracts the durable summary payload from either the current tagged form or the legacy continuation prompt form.
/// 从当前带标签格式或旧版续写提示词格式中提取持久摘要正文。
pub fn extract_compaction_previous_summary(summary_body: &str) -> String {
    if let Some((_, after_start)) = summary_body.split_once(PREVIOUS_SUMMARY_START_TAG) {
        let summary = after_start
            .split_once(PREVIOUS_SUMMARY_END_TAG)
            .map(|(summary, _)| summary)
            .unwrap_or(after_start);
        return summary.trim().to_string();
    }

    let summary_body = summary_body
        .split_once(LEGACY_SUMMARY_PREFIX)
        .map(|(_, summary)| summary)
        .unwrap_or(summary_body);
    let summary_body = summary_body
        .split_once(LEGACY_SUMMARY_SUFFIX)
        .map(|(summary, _)| summary)
        .unwrap_or(summary_body);
    summary_body.trim().to_string()
}

/// Extracts the durable summary payload from one model compaction response.
/// 从单次模型压缩响应中提取可持久化的摘要正文。
///
/// # Parameters
/// # 参数
/// `summary_text` is the raw assistant output returned by the internal compaction request.
/// `summary_text` 是内部压缩请求返回的原始助手输出。
///
/// # Returns
/// # 返回值
/// A non-empty durable summary body, or `None` when the tagged protocol is missing required durable content.
/// 返回非空的持久摘要正文；当标签协议缺少必需的持久内容时返回 `None`。
pub fn extract_compaction_model_summary(summary_text: &str) -> Option<String> {
    let contains_analysis = summary_text.contains(MODEL_ANALYSIS_START_TAG);
    let contains_summary = summary_text.contains(MODEL_SUMMARY_START_TAG);
    let stripped_analysis = strip_tagged_sections(
        summary_text,
        MODEL_ANALYSIS_START_TAG,
        MODEL_ANALYSIS_END_TAG,
    );
    let summary_payload = if contains_summary {
        extract_tagged_section(
            &stripped_analysis,
            MODEL_SUMMARY_START_TAG,
            MODEL_SUMMARY_END_TAG,
        )?
    } else if contains_analysis {
        return None;
    } else {
        stripped_analysis
    };
    let extracted = extract_compaction_previous_summary(&summary_payload);
    let normalized = extracted.trim().to_string();
    (!normalized.is_empty()).then_some(normalized)
}

/// Normalizes one compaction summary into a non-empty plain-text block before it becomes the synthetic continuation prompt body.
/// 在把压缩总结转换为合成续写提示词正文之前，将其规范化为非空纯文本块。
fn normalize_compaction_summary_text(summary_text: &str) -> String {
    let extracted = extract_compaction_previous_summary(summary_text);
    let trimmed = extracted.trim();
    if trimmed.is_empty() {
        String::from("No earlier context details were preserved.")
    } else {
        trimmed.to_string()
    }
}

/// Extracts the first payload enclosed by one exact tag pair.
/// 提取第一段由精确标签对包裹的正文。
///
/// # Parameters
/// # 参数
/// `text` is the source text to scan.
/// `text` 是要扫描的源文本。
/// `start_tag` is the exact opening tag to find.
/// `start_tag` 是要查找的精确起始标签。
/// `end_tag` is the exact closing tag to find.
/// `end_tag` 是要查找的精确结束标签。
///
/// # Returns
/// # 返回值
/// The trimmed payload from the first complete tag pair, or `None` when no complete pair exists.
/// 返回第一组完整标签内裁剪后的正文；当不存在完整标签对时返回 `None`。
fn extract_tagged_section(text: &str, start_tag: &str, end_tag: &str) -> Option<String> {
    let (_, after_start) = text.split_once(start_tag)?;
    let (section, _) = after_start.split_once(end_tag)?;
    Some(section.trim().to_string())
}

/// Removes every complete section enclosed by one exact tag pair.
/// 移除每一段由精确标签对包裹的完整正文。
///
/// # Parameters
/// # 参数
/// `text` is the source text to rewrite.
/// `text` 是要改写的源文本。
/// `start_tag` is the exact opening tag that starts removable sections.
/// `start_tag` 是可移除段落的精确起始标签。
/// `end_tag` is the exact closing tag that ends removable sections.
/// `end_tag` 是可移除段落的精确结束标签。
///
/// # Returns
/// # 返回值
/// The source text with every complete tagged section removed and surrounding whitespace trimmed.
/// 返回移除所有完整标签段并裁剪外侧空白后的源文本。
fn strip_tagged_sections(text: &str, start_tag: &str, end_tag: &str) -> String {
    let mut remaining = text;
    let mut stripped = String::new();
    while let Some((before, after_start)) = remaining.split_once(start_tag) {
        stripped.push_str(before);
        let Some((_, after_end)) = after_start.split_once(end_tag) else {
            stripped.push_str(start_tag);
            stripped.push_str(after_start);
            return stripped.trim().to_string();
        };
        remaining = after_end;
    }
    stripped.push_str(remaining);
    stripped.trim().to_string()
}

/// Builds one prompt-shaped compaction result from an already prepared summary text and one planned compaction window.
/// 根据一段已准备好的总结文本与一份压缩规划窗口构建提示词形态的压缩结果。
fn build_compaction_result_from_summary_text(
    plan: &CompactionPlan,
    summary_text: String,
) -> CompactionResult {
    let normalized_summary = normalize_compaction_summary_text(&summary_text);
    let continuation_prompt = build_compaction_continuation_prompt_with_options(
        &normalized_summary,
        CompactionContinuationPromptOptions {
            recent_messages_preserved: plan.retained_tail_start_turn_index.is_some(),
        },
    );
    CompactionResult {
        summary_record: CompactionSummaryRecord {
            kind: SessionMessageKind::User,
            record_kind: SessionMessageRecordKind::CompactionSummary,
            title: String::from("Context Continuation Prompt"),
            body: continuation_prompt,
            preview: truncate_preview_lines(&normalized_summary, 2, 88),
        },
        source_until_message_id: plan.source_until_message_id.clone(),
        source_until_turn_index: plan.source_until_turn_index,
        retained_tail_start_turn_index: plan.retained_tail_start_turn_index,
        compaction_id: plan.compaction_id.clone(),
        created_at: plan.created_at.clone(),
        policy: plan.policy,
        tail_selection: plan.tail_selection.clone(),
        provider_context_items: Vec::new(),
        provider_context_route: None,
        provider_context_usage: None,
        provider_context_response_id: None,
    }
}

/// Builds the legacy host-side summary text retained as a fallback when prompt-based model compaction cannot complete.
/// 构建在提示词式模型压缩无法完成时保留的旧版宿主侧回退总结文本。
pub(super) fn build_legacy_compaction_summary_text(
    messages: &[SessionMessageEntry],
    prune_tools: bool,
) -> String {
    messages
        .iter()
        .map(|message| compaction_summary_line_for_message(message, prune_tools))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Resolves the absolute persisted user-turn index for one transcript message identifier by replaying user-turn boundaries over the full session transcript history.
/// 通过在完整会话转录历史上重放用户轮次边界，解析某条转录消息标识对应的绝对持久化用户轮次索引。
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
        let preview_body = if message.record_kind == SessionMessageRecordKind::CompactionSummary {
            extract_compaction_previous_summary(&message.body)
        } else {
            message.body.clone()
        };
        truncate_preview_lines(&preview_body, 1, 88)
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
        SessionMessageRecordKind::ToolDetailOutput => "TOOL_DETAIL",
        SessionMessageRecordKind::CompactionSummary => "COMPACTION",
        SessionMessageRecordKind::CompactionStatus => "COMPACTION_STATUS",
        SessionMessageRecordKind::DisplayNote => "DISPLAY_NOTE",
        SessionMessageRecordKind::TitleOutput => "TITLE",
        SessionMessageRecordKind::TurnTerminated => "TERMINATED",
        SessionMessageRecordKind::TurnStats => "TURN_STATS",
    }
}

/// Resolves the stable English headline used for model-facing compaction summaries without depending on WebUI locale.
/// 解析供面向模型的压缩摘要使用的稳定英文标题，并且不依赖WebUI语言。
fn canonical_compaction_message_title(message: &SessionMessageEntry) -> String {
    match message.record_kind {
        SessionMessageRecordKind::Transcript => match message.kind {
            SessionMessageKind::User => String::from("User Prompt"),
            SessionMessageKind::Assistant => String::from("Assistant Message"),
            SessionMessageKind::Error => String::from("System Error"),
        },
        SessionMessageRecordKind::ThinkingOutput => String::from("Thinking"),
        SessionMessageRecordKind::AssistantOutput => String::from("Output"),
        SessionMessageRecordKind::ToolCallOutput => {
            canonical_tool_summary_title(message, "Tool Call")
        }
        SessionMessageRecordKind::ToolResultOutput => {
            canonical_tool_summary_title(message, "Tool Result")
        }
        SessionMessageRecordKind::SummaryOutput => String::from("Execution Summary"),
        SessionMessageRecordKind::ExploreOutput => String::from("Explore Context"),
        SessionMessageRecordKind::ToolDetailOutput => String::from("Tool Detail"),
        SessionMessageRecordKind::CompactionSummary => String::from("Context Compaction Summary"),
        SessionMessageRecordKind::CompactionStatus => String::from("Context Compaction Status"),
        SessionMessageRecordKind::DisplayNote => String::from("Display Note"),
        SessionMessageRecordKind::TitleOutput => String::from("Title Update"),
        SessionMessageRecordKind::TurnTerminated => String::from("Turn Terminated"),
        SessionMessageRecordKind::TurnStats => String::from("Turn Statistics"),
    }
}

/// Builds a stable English tool summary title from normalized metadata first and legacy persisted titles second.
/// 优先根据规范化元数据、其次根据旧版持久化标题构建稳定英文工具摘要标题。
pub(super) fn canonical_tool_summary_title(message: &SessionMessageEntry, prefix: &str) -> String {
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
pub(super) fn truncate_preview_lines(value: &str, max_lines: usize, max_chars: usize) -> String {
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
