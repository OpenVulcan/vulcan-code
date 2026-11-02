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

/// Builds the live display preview of one still-streaming model compaction response.
/// 为仍在流式生成的模型压缩响应构建实时显示预览。
///
/// # Parameters
/// # 参数
/// `output_text` is the cumulative raw assistant output received so far.
/// `output_text` 是目前已收到的累计原始助手输出。
///
/// # Returns
/// # 返回值
/// The summary body once its tag has started, otherwise the drafting text without protocol tags.
/// The preview is display-only; `extract_compaction_model_summary` stays the sole durable authority.
/// 摘要标签开始后返回摘要正文，否则返回去掉协议标签的草稿文本。
/// 预览只用于显示；`extract_compaction_model_summary` 仍是唯一的持久权威。
pub fn preview_compaction_model_output(output_text: &str) -> String {
    if let Some((_, after_start)) = output_text.split_once(MODEL_SUMMARY_START_TAG) {
        let summary = after_start
            .split_once(MODEL_SUMMARY_END_TAG)
            .map_or(after_start, |(summary, _)| summary);
        return summary.trim().to_string();
    }
    output_text
        .replace(MODEL_ANALYSIS_START_TAG, "")
        .replace(MODEL_ANALYSIS_END_TAG, "")
        .trim()
        .to_string()
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
