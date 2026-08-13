use vulcan_context::{ContextCompactionPolicy, SessionMessageEntry, SessionMessageRecordKind};

use super::summary::canonical_tool_summary_title;
use super::tail::{
    estimated_session_tail_tokens, recent_tail_start_index, session_api_round_segments,
};
use super::{
    CompactionInputBudget, CompactionInputPruneMode, CompactionInputPruneReport,
    PreparedCompactionSource,
};
use crate::preview::preview_tool_output;
/// Estimates the model-facing size of a compaction source with the same lightweight transcript heuristic used by the planner.
/// 使用规划器相同的轻量转录启发式估算压缩源的模型可见大小。
///
/// # Parameters
/// # 参数
/// `messages` are the source messages that would be sent to the compaction model.
/// `messages` 是将发送给压缩模型的源消息。
///
/// # Returns
/// # 返回值
/// Returns a lightweight estimated token count for the supplied source messages.
/// 返回所给源消息的轻量估算 Token 数。
pub fn estimate_compaction_source_tokens(messages: &[SessionMessageEntry]) -> usize {
    estimated_session_tail_tokens(messages)
}

/// Prepares a clone-only compaction source for the target input budget.
/// 为目标输入预算准备一份仅克隆的压缩源。
///
/// # Parameters
/// # 参数
/// `messages` is the immutable planned source window.
/// `messages` 是不可变的计划源窗口。
/// `budget` is the target input budget derived from the active model window.
/// `budget` 是从当前模型窗口派生出的目标输入预算。
/// `policy` controls recent-tail preservation.
/// `policy` 控制最近尾部保留策略。
/// `mode` explains why pruning is allowed.
/// `mode` 说明为何允许剪枝。
///
/// # Returns
/// # 返回值
/// An owned source message list and a pruning report; the original input is never mutated.
/// 返回拥有所有权的源消息列表与剪枝报告；原始输入永远不会被修改。
pub fn prepare_compaction_source_for_target_budget(
    messages: &[SessionMessageEntry],
    budget: CompactionInputBudget,
    policy: ContextCompactionPolicy,
    mode: CompactionInputPruneMode,
) -> PreparedCompactionSource {
    let original_token_estimate = estimate_compaction_source_tokens(messages);
    if mode == CompactionInputPruneMode::None
        || original_token_estimate <= budget.target_input_budget_tokens
    {
        return PreparedCompactionSource {
            source_messages: messages.to_vec(),
            prune_report: CompactionInputPruneReport::no_pruning(budget, original_token_estimate),
        };
    }

    let recent_tail_start = recent_tail_start_index(messages, policy);
    let mut pruned_tool_output_count = 0usize;
    let mut preserved_tool_output_count = 0usize;
    let mut truncated_source_message_count = 0usize;
    let mut source_messages = messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            let is_tool_output = matches!(
                message.record_kind,
                SessionMessageRecordKind::ToolCallOutput
                    | SessionMessageRecordKind::ToolResultOutput
            );
            if !is_tool_output {
                return message.clone();
            }
            if index >= recent_tail_start || tool_output_must_remain_full(message) {
                preserved_tool_output_count = preserved_tool_output_count.saturating_add(1);
                return message.clone();
            }

            pruned_tool_output_count = pruned_tool_output_count.saturating_add(1);
            compact_tool_output_for_compaction_input(message)
        })
        .collect::<Vec<_>>();
    let mut dropped_source_message_count = 0usize;
    let mut pruned_token_estimate = estimate_compaction_source_tokens(&source_messages);
    if pruned_token_estimate > budget.target_input_budget_tokens {
        let original_len = source_messages.len();
        source_messages = drop_oldest_compaction_source_messages_for_budget(
            source_messages,
            budget.target_input_budget_tokens,
            recent_tail_start,
        );
        dropped_source_message_count = original_len.saturating_sub(source_messages.len());
        pruned_token_estimate = estimate_compaction_source_tokens(&source_messages);
        if pruned_token_estimate > budget.target_input_budget_tokens
            && let Some(message) = source_messages.first_mut()
        {
            let truncated = truncate_compaction_source_message_for_budget(
                message,
                budget.target_input_budget_tokens,
            );
            if truncated.body != message.body {
                *message = truncated;
                truncated_source_message_count = 1;
                pruned_token_estimate = estimate_compaction_source_tokens(&source_messages);
            }
        }
    }
    PreparedCompactionSource {
        source_messages,
        prune_report: CompactionInputPruneReport {
            mode,
            target_input_budget_tokens: budget.target_input_budget_tokens,
            safety_reserve_tokens: budget.safety_reserve_tokens,
            original_token_estimate,
            pruned_token_estimate,
            pruned_tool_output_count,
            preserved_tool_output_count,
            dropped_source_message_count,
            truncated_source_message_count,
            still_exceeds_budget: pruned_token_estimate > budget.target_input_budget_tokens,
        },
    }
}

fn tool_output_must_remain_full(message: &SessionMessageEntry) -> bool {
    tool_output_text_looks_error_critical(&message.body)
        || tool_output_metadata_looks_structurally_critical(message.metadata.as_ref())
        || canonical_tool_summary_title(message, "")
            .to_ascii_lowercase()
            .contains("ask")
}

/// Returns whether tool-output text carries errors or failures that should not be compacted away.
/// 判断工具输出正文是否包含不应被压缩移除的错误或失败信息。
///
/// # Parameters
/// # 参数
/// `text` is the tool-output body text.
/// `text` 是工具输出正文文本。
///
/// # Returns
/// # 返回值
/// Returns `true` when the text contains known error or failure markers.
/// 当文本包含已知错误或失败标记时返回 `true`。
fn tool_output_text_looks_error_critical(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    [
        "error",
        "failed",
        "failure",
        "panic",
        "exception",
        "traceback",
        "compilation failed",
        "test failed",
        "exit code",
        "stderr",
        "context length",
        "prompt too long",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
}

/// Returns whether metadata points to a host result that should stay available to the summary model.
/// 判断元数据是否指向应继续供摘要模型使用的宿主结果。
///
/// # Parameters
/// # 参数
/// `metadata` is the optional tool-result metadata attached to the transcript row.
/// `metadata` 是转录行附带的可选工具结果元数据。
///
/// # Returns
/// # 返回值
/// Returns `true` when metadata references generated attachments, patches, change sets, or ask data.
/// 当元数据引用生成附件、补丁、变更集或 ask 数据时返回 `true`。
fn tool_output_metadata_looks_structurally_critical(metadata: Option<&serde_json::Value>) -> bool {
    let Some(metadata) = metadata else {
        return false;
    };
    metadata_contains_string(metadata, "generated_attachments")
        || metadata_contains_string(metadata, "change_set")
        || metadata_contains_string(metadata, "patch")
        || metadata_contains_string(metadata, "ask")
}

/// Drops oldest source-message segments, then splits the protected tail if it still cannot fit the target budget.
/// 先丢弃最旧源消息分段；若受保护尾部仍无法进入目标预算，则继续切分该尾部。
///
/// # Parameters
/// # 参数
/// `messages` are the cloned model-facing source messages after tool-output pruning.
/// `messages` 是工具输出剪枝后的模型侧克隆源消息。
/// `target_input_budget_tokens` is the maximum estimated input budget.
/// `target_input_budget_tokens` 是最大估算输入预算。
/// `recent_tail_start` is the first index of the recent tail that should be preserved before last-resort splitting.
/// `recent_tail_start` 是最后兜底切分前应保留的最近尾部起始索引。
///
/// # Returns
/// # 返回值
/// Returns a shortened cloned source that preserves at least one newest message.
/// 返回一份至少保留一条最新消息的缩短克隆源。
fn drop_oldest_compaction_source_messages_for_budget(
    messages: Vec<SessionMessageEntry>,
    target_input_budget_tokens: usize,
    recent_tail_start: usize,
) -> Vec<SessionMessageEntry> {
    if messages.len() <= 1 {
        return messages;
    }
    if estimate_compaction_source_tokens(&messages) <= target_input_budget_tokens {
        return messages;
    }
    let protected_start = recent_tail_start.min(messages.len().saturating_sub(1));
    let mut drop_until = 0usize;
    if protected_start > 0 {
        for segment in session_api_round_segments(&messages) {
            if segment.end > protected_start {
                break;
            }
            drop_until = segment.end;
            let candidate = &messages[drop_until..];
            if !candidate.is_empty()
                && estimate_compaction_source_tokens(candidate) <= target_input_budget_tokens
            {
                break;
            }
        }
    }

    let mut shortened = messages[drop_until..].to_vec();
    while shortened.len() > 1
        && estimate_compaction_source_tokens(&shortened) > target_input_budget_tokens
    {
        shortened.remove(0);
    }
    shortened
}

/// Truncates one remaining oversized source message body for compaction input only.
/// 仅针对压缩输入截断一条剩余的超大源消息正文。
///
/// # Parameters
/// # 参数
/// `message` is the remaining cloned source message that still exceeds the target budget.
/// `message` 是仍超过目标预算的剩余克隆源消息。
/// `target_input_budget_tokens` is the approximate target token budget used to derive a character cap.
/// `target_input_budget_tokens` 是用于派生字符上限的近似目标 Token 预算。
///
/// # Returns
/// # 返回值
/// Returns a cloned message with a bounded body prefix, or the original clone when no truncation is possible.
/// 返回带有有界正文前缀的克隆消息；无法截断时返回原克隆。
fn truncate_compaction_source_message_for_budget(
    message: &SessionMessageEntry,
    target_input_budget_tokens: usize,
) -> SessionMessageEntry {
    let original_chars = message.body.chars().count();
    if original_chars == 0 {
        return message.clone();
    }
    let keep_chars = target_input_budget_tokens
        .saturating_mul(4)
        .max(1usize)
        .min(original_chars);
    if keep_chars >= original_chars {
        return message.clone();
    }
    let preserved_prefix = message.body.chars().take(keep_chars).collect::<String>();
    let mut truncated = message.clone();
    truncated.body = format!(
        "[Message truncated for context-compaction input only]\n\
Reason: remaining source message exceeded target compaction input budget\n\
Original size: {original_chars} chars\n\
Preserved prefix:\n{preserved_prefix}"
    );
    truncated
}

/// Recursively checks whether JSON metadata contains one string fragment.
/// 递归检查 JSON 元数据是否包含指定字符串片段。
///
/// # Parameters
/// # 参数
/// `value` is the JSON value to inspect.
/// `value` 是需要检查的 JSON 值。
/// `needle` is the lowercase string fragment to find.
/// `needle` 是需要查找的小写字符串片段。
///
/// # Returns
/// # 返回值
/// Returns `true` when the fragment is found in an object key or string value.
/// 当片段出现在对象键或字符串值中时返回 `true`。
fn metadata_contains_string(value: &serde_json::Value, needle: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.to_ascii_lowercase().contains(needle),
        serde_json::Value::Array(items) => items
            .iter()
            .any(|item| metadata_contains_string(item, needle)),
        serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
            key.to_ascii_lowercase().contains(needle) || metadata_contains_string(value, needle)
        }),
        _ => false,
    }
}

/// Builds one compact clone of a tool-output transcript row for compaction input only.
/// 构建一条仅用于压缩输入的工具输出转录行紧凑克隆。
///
/// # Parameters
/// # 参数
/// `message` is the original tool-output transcript row.
/// `message` 是原始工具输出转录行。
///
/// # Returns
/// # 返回值
/// Returns a cloned row with a bounded preview body and no metadata.
/// 返回一条带有有界预览正文且不带元数据的克隆行。
fn compact_tool_output_for_compaction_input(message: &SessionMessageEntry) -> SessionMessageEntry {
    let mut compacted = message.clone();
    compacted.body = preview_tool_output(&message.body).text;
    compacted.metadata = None;
    compacted
}
