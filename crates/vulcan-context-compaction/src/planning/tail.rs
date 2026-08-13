use crate::preview::preview_tool_output;
use serde_json::Value;
use vulcan_context::{
    ContextCompactionPolicy, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
    estimate_mixed_language_text_tokens,
};

use super::CompactionTailSelectionReport;

/// Internal retained-tail boundary plus its frozen public report.
/// 内部尾部保留边界及其冻结公共报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct CompactionTailSelection {
    /// Inclusive start index of the retained contiguous suffix.
    /// 保留连续后缀的包含式起始索引。
    pub(super) start_index: usize,
    /// Frozen selection report consumed by the plan and rewrite.
    /// 由计划与 rewrite 消费的冻结选择报告。
    pub(super) report: CompactionTailSelectionReport,
}

/// Selects the largest contiguous retained suffix under the segment and nominal token limits.
/// 在执行段与名义 Token 双上限下选择最大的连续保留后缀。
///
/// # Parameters
/// # 参数
/// `messages` is the ordered canonical compaction source and `policy` is the frozen adaptive policy.
/// `messages` 是有序规范压缩源，`policy` 是冻结的自适应策略。
///
/// # Returns
/// # 返回值
/// The exact suffix start plus the immutable selection report consumed by planning and rewrite.
/// 返回精确后缀起点以及供规划与 rewrite 消费的不可变选择报告。
pub(super) fn select_compaction_tail(
    messages: &[SessionMessageEntry],
    policy: ContextCompactionPolicy,
) -> CompactionTailSelection {
    if messages.is_empty() {
        return CompactionTailSelection {
            start_index: 0,
            report: empty_tail_selection_report(policy),
        };
    }

    // segments preserve assistant/provider execution boundaries independently of user turns.
    // segments 独立于 User 回合保留 Assistant/Provider 执行边界。
    let segments = session_api_round_segments(messages);
    let mut retained_start = messages.len().saturating_sub(1);
    let mut retained_tokens = 0usize;
    let mut retained_segments = 0usize;
    let mut previewed_tool_output_count = 0usize;
    let mut omitted_tool_output_token_estimate = 0usize;
    let mut newest_segment_exceeds_nominal_budget = false;

    for segment in segments.iter().rev() {
        if retained_segments >= policy.tail_turns.max(1usize) {
            break;
        }
        let is_newest_segment = retained_segments == 0;
        let projection =
            projected_segment_for_tail(&messages[segment.start..segment.end], is_newest_segment);
        let segment_tokens = estimated_session_tail_tokens(&projection.messages);
        if is_newest_segment {
            newest_segment_exceeds_nominal_budget = segment_tokens > policy.preserve_recent_tokens;
        } else if retained_tokens.saturating_add(segment_tokens) > policy.preserve_recent_tokens {
            break;
        }

        retained_start = segment.start;
        retained_tokens = retained_tokens.saturating_add(segment_tokens);
        retained_segments = retained_segments.saturating_add(1);
        previewed_tool_output_count =
            previewed_tool_output_count.saturating_add(projection.previewed_tool_output_count);
        omitted_tool_output_token_estimate = omitted_tool_output_token_estimate
            .saturating_add(projection.omitted_tool_output_token_estimate);
        if newest_segment_exceeds_nominal_budget {
            break;
        }
    }

    CompactionTailSelection {
        start_index: retained_start,
        report: CompactionTailSelectionReport {
            context_limit: policy.tail_context_limit,
            policy_source: policy.tail_policy_source,
            nominal_token_budget: policy.preserve_recent_tokens,
            segment_limit: policy.tail_turns,
            retained_segment_count: retained_segments,
            retained_message_ids: messages[retained_start..]
                .iter()
                .map(|message| message.id.clone())
                .collect(),
            retained_token_estimate: retained_tokens,
            previewed_tool_output_count,
            omitted_tool_output_token_estimate,
            newest_segment_exceeds_nominal_budget,
        },
    }
}

/// Builds a frozen report for an explicit suffix boundary after validating execution-segment safety.
/// 在验证执行段安全边界后，为显式后缀边界构建冻结报告。
///
/// # Parameters
/// # 参数
/// `messages` is the ordered canonical source, `start_index` is the requested inclusive suffix start,
/// and `policy` is the frozen adaptive policy.
/// `messages` 是有序规范源，`start_index` 是请求的包含式后缀起点，`policy` 是冻结的自适应策略。
///
/// # Returns
/// # 返回值
/// A validated exact selection, or `None` when the requested start splits an execution segment.
/// 返回校验后的精确选择；请求起点拆分执行段时返回 `None`。
pub(super) fn tail_selection_for_explicit_start(
    messages: &[SessionMessageEntry],
    start_index: usize,
    policy: ContextCompactionPolicy,
) -> Option<CompactionTailSelection> {
    if start_index >= messages.len() {
        return Some(CompactionTailSelection {
            start_index,
            report: empty_tail_selection_report(policy),
        });
    }
    let segments = session_api_round_segments(messages);
    let first_segment_position = segments
        .iter()
        .position(|segment| segment.start == start_index)?;
    let mut retained_tokens = 0usize;
    let mut previewed_tool_output_count = 0usize;
    let mut omitted_tool_output_token_estimate = 0usize;
    for (position, segment) in segments[first_segment_position..].iter().enumerate() {
        let is_newest_segment = first_segment_position + position + 1 == segments.len();
        let projection =
            projected_segment_for_tail(&messages[segment.start..segment.end], is_newest_segment);
        retained_tokens =
            retained_tokens.saturating_add(estimated_session_tail_tokens(&projection.messages));
        previewed_tool_output_count =
            previewed_tool_output_count.saturating_add(projection.previewed_tool_output_count);
        omitted_tool_output_token_estimate = omitted_tool_output_token_estimate
            .saturating_add(projection.omitted_tool_output_token_estimate);
    }
    let retained_segment_count = segments.len().saturating_sub(first_segment_position);
    Some(CompactionTailSelection {
        start_index,
        report: CompactionTailSelectionReport {
            context_limit: policy.tail_context_limit,
            policy_source: policy.tail_policy_source,
            nominal_token_budget: policy.preserve_recent_tokens,
            segment_limit: policy.tail_turns,
            retained_segment_count,
            retained_message_ids: messages[start_index..]
                .iter()
                .map(|message| message.id.clone())
                .collect(),
            retained_token_estimate: retained_tokens,
            previewed_tool_output_count,
            omitted_tool_output_token_estimate,
            newest_segment_exceeds_nominal_budget: segments
                .last()
                .map(|segment| {
                    estimated_session_tail_tokens(&messages[segment.start..segment.end])
                        > policy.preserve_recent_tokens
                })
                .unwrap_or(false),
        },
    })
}

/// Resolves the inclusive start index of the adaptive retained suffix.
/// 解析自适应保留后缀的包含式起始索引。
///
/// # Parameters
/// # 参数
/// `messages` is the ordered canonical source and `policy` is the frozen adaptive policy.
/// `messages` 是有序规范源，`policy` 是冻结的自适应策略。
///
/// # Returns
/// # 返回值
/// The start index selected by the single shared tail selector.
/// 返回共享唯一尾部选择器选出的起始索引。
pub(super) fn recent_tail_start_index(
    messages: &[SessionMessageEntry],
    policy: ContextCompactionPolicy,
) -> usize {
    select_compaction_tail(messages, policy).start_index
}

/// Model-facing projection statistics for one retained execution segment.
/// 单个保留执行段的模型侧投影统计。
struct ProjectedTailSegment {
    /// Projected transcript messages used only for token estimation.
    /// 仅用于 Token 估算的投影转录消息。
    messages: Vec<SessionMessageEntry>,
    /// Number of historical tool results previewed in this segment.
    /// 本执行段中被预览的历史工具结果数量。
    previewed_tool_output_count: usize,
    /// Estimated original tokens omitted by previews in this segment.
    /// 本执行段预览省略的原始估算 Token 数。
    omitted_tool_output_token_estimate: usize,
}

/// Projects historical tool results for one segment while preserving the newest segment verbatim.
/// 投影单个历史执行段的工具结果，同时保持最新执行段原文不变。
///
/// # Parameters
/// # 参数
/// `messages` contains one complete execution segment and `is_newest_segment` controls full-text protection.
/// `messages` 包含一个完整执行段，`is_newest_segment` 控制最新段全文保护。
///
/// # Returns
/// # 返回值
/// The model-facing segment clone plus exact preview and omission statistics.
/// 返回面向模型的执行段克隆以及精确预览和省略统计。
fn projected_segment_for_tail(
    messages: &[SessionMessageEntry],
    is_newest_segment: bool,
) -> ProjectedTailSegment {
    let mut previewed_tool_output_count = 0usize;
    let mut omitted_tool_output_token_estimate = 0usize;
    let messages = messages
        .iter()
        .cloned()
        .map(|mut message| {
            if !is_newest_segment
                && message.record_kind == SessionMessageRecordKind::ToolResultOutput
            {
                let preview = preview_tool_output(&message.body);
                if preview.previewed {
                    previewed_tool_output_count = previewed_tool_output_count.saturating_add(1);
                    omitted_tool_output_token_estimate = omitted_tool_output_token_estimate
                        .saturating_add(preview.omitted_token_estimate);
                    message.body = preview.text;
                }
            }
            message
        })
        .collect();
    ProjectedTailSegment {
        messages,
        previewed_tool_output_count,
        omitted_tool_output_token_estimate,
    }
}

/// Builds an empty retained-tail report for an empty suffix.
/// 为一个空后缀构建空尾部保留报告。
///
/// # Parameters
/// # 参数
/// `policy` is the frozen adaptive policy whose diagnostic fields must remain visible.
/// `policy` 是诊断字段必须保持可见的冻结自适应策略。
///
/// # Returns
/// # 返回值
/// A zero-count report that preserves the policy window, source, and limits.
/// 返回保留策略窗口、来源与上限的零计数报告。
fn empty_tail_selection_report(policy: ContextCompactionPolicy) -> CompactionTailSelectionReport {
    CompactionTailSelectionReport {
        context_limit: policy.tail_context_limit,
        policy_source: policy.tail_policy_source,
        nominal_token_budget: policy.preserve_recent_tokens,
        segment_limit: policy.tail_turns,
        retained_segment_count: 0,
        retained_message_ids: Vec::new(),
        retained_token_estimate: 0,
        previewed_tool_output_count: 0,
        omitted_tool_output_token_estimate: 0,
        newest_segment_exceeds_nominal_budget: false,
    }
}

/// Transcript-index segment representing one API round plus its following tool-visible entries.
/// 表示一个 API 执行轮次及其后续工具可见条目的转录索引段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SessionTailSegment {
    /// Inclusive segment start index.
    /// 段起始索引，包含该位置。
    pub(super) start: usize,
    /// Exclusive segment end index.
    /// 段结束索引，不包含该位置。
    pub(super) end: usize,
}

/// Builds transcript tail segments at assistant API-round boundaries.
/// 按 Assistant API 执行轮次边界构建转录尾部段。
///
/// # Parameters
/// # 参数
/// `messages` is the ordered model-facing transcript source to segment.
/// `messages` 是待分段的有序模型侧转录源。
///
/// # Returns
/// # 返回值
/// Contiguous execution segments covering every input message exactly once.
/// 返回连续执行段，确保每条输入消息恰好被覆盖一次。
pub(super) fn session_api_round_segments(
    messages: &[SessionMessageEntry],
) -> Vec<SessionTailSegment> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut round_starts = Vec::new();
    let mut last_round_key: Option<String> = None;
    for (index, message) in messages.iter().enumerate() {
        let Some(round_key) = message_api_round_key(message) else {
            continue;
        };
        if last_round_key.as_deref() != Some(round_key.as_str()) {
            round_starts.push(index);
            last_round_key = Some(round_key);
        }
    }
    if round_starts.is_empty() {
        return vec![SessionTailSegment {
            start: 0,
            end: messages.len(),
        }];
    }
    if round_starts[0] != 0 {
        // Leading history belongs to the first Assistant/provider segment; it is not an extra segment.
        // 首条 Assistant 之前的历史属于第一个 Assistant/Provider 执行段，不能额外计为一段。
        round_starts[0] = 0;
    }

    round_starts
        .iter()
        .enumerate()
        .map(|(position, start)| SessionTailSegment {
            start: *start,
            end: round_starts
                .get(position + 1)
                .copied()
                .unwrap_or(messages.len()),
        })
        .collect()
}

/// Returns the stable API-round key for one assistant transcript entry.
/// 返回一条 Assistant 转录项的稳定 API 执行轮次键。
fn message_api_round_key(message: &SessionMessageEntry) -> Option<String> {
    if message.kind != SessionMessageKind::Assistant
        || !matches!(
            message.record_kind,
            SessionMessageRecordKind::AssistantOutput | SessionMessageRecordKind::ToolCallOutput
        )
    {
        return None;
    }
    message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("provider_message_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(message.id.clone()))
}

/// Estimates the approximate token footprint of retained transcript entries.
/// 估算保留转录项的近似 Token 占用。
///
/// # Parameters
/// # 参数
/// `messages` contains the exact model-facing entries whose combined cost is required.
/// `messages` 包含需要计算合计成本的精确模型侧条目。
///
/// # Returns
/// # 返回值
/// The saturating sum of the shared per-message lightweight estimates.
/// 返回共享逐消息轻量估算值的饱和合计。
pub(super) fn estimated_session_tail_tokens(messages: &[SessionMessageEntry]) -> usize {
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
    estimate_mixed_language_text_tokens(trimmed).max(1usize)
}
