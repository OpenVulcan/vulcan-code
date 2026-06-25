//! Context-compaction planning and summary-shaping helpers.
//! 上下文压缩规划与摘要整形辅助模块。

use std::time::{SystemTime, UNIX_EPOCH};
use vulcan_context::{
    ContextCompactionPolicy, SessionCompactionEntry, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, estimate_mixed_language_text_tokens,
};
use vulcan_prompt_composer::build_context_compaction_request_prompt;

const PREVIOUS_SUMMARY_START_TAG: &str = "<previous-summary>";
const PREVIOUS_SUMMARY_END_TAG: &str = "</previous-summary>";
const MODEL_ANALYSIS_START_TAG: &str = "<analysis>";
const MODEL_ANALYSIS_END_TAG: &str = "</analysis>";
const MODEL_SUMMARY_START_TAG: &str = "<summary>";
const MODEL_SUMMARY_END_TAG: &str = "</summary>";
const LEGACY_SUMMARY_PREFIX: &str = "Earlier context summary:\n";
const LEGACY_SUMMARY_SUFFIX: &str = "\n\nContinue from the newer messages below";

/// Stateless summary record returned by one compaction result before the WebUI turns it into its own hidden-node shape.
/// 单次压缩结果返回的无状态摘要记录，供WebUI在落地前转换为自身隐藏节点形态。
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
    /// Compact preview extracted from the summary body for diagnostics and UI reuse.
    /// 从摘要正文中提取的紧凑预览，供诊断与界面复用。
    pub preview: String,
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
    /// Optional inclusive source message identifier used for manual partial `up_to` compaction.
    /// 用于手动 partial `up_to` 压缩的可选闭区间源消息标识。
    pub source_until_message_id: Option<&'a str>,
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

/// Input budget used to decide whether a compaction request must prune its cloned source messages.
/// 用于判断压缩请求是否必须剪枝其克隆源消息的输入预算。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionInputBudget {
    /// Maximum estimated input tokens allowed for the compaction source.
    /// 压缩源允许使用的最大估算输入 Token 数。
    pub target_input_budget_tokens: usize,
    /// Fixed safety reserve subtracted from the target model context window.
    /// 从目标模型上下文窗口扣除的固定安全阈值。
    pub safety_reserve_tokens: usize,
}

/// Reason why one compaction input source was pruned before being sent to the summary model.
/// 压缩输入源在发送给总结模型前被剪枝的原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionInputPruneMode {
    /// No pruning was requested.
    /// 未请求剪枝。
    None,
    /// Pruning was required because the target model window could not hold the full source.
    /// 因目标模型窗口无法容纳完整源消息而需要剪枝。
    RequiredForWindowDowngrade,
    /// Pruning was used after the full-fidelity path failed repeatedly or overflowed.
    /// 在完整保真路径多次失败或溢出后使用剪枝。
    RetryAfterFailure,
}

impl Default for CompactionInputPruneMode {
    /// Returns the default full-fidelity mode.
    /// 返回默认的完整保真模式。
    fn default() -> Self {
        Self::None
    }
}

/// Report describing clone-only pruning performed for one compaction input request.
/// 描述单次压缩输入请求执行的仅克隆剪枝行为。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionInputPruneReport {
    /// Pruning mode used for this input.
    /// 本次输入使用的剪枝模式。
    pub mode: CompactionInputPruneMode,
    /// Target input budget used for the decision.
    /// 用于决策的目标输入预算。
    pub target_input_budget_tokens: usize,
    /// Safety reserve subtracted from the model context window.
    /// 从模型上下文窗口扣除的安全阈值。
    pub safety_reserve_tokens: usize,
    /// Estimated source tokens before pruning.
    /// 剪枝前的源消息估算 Token 数。
    pub original_token_estimate: usize,
    /// Estimated source tokens after pruning.
    /// 剪枝后的源消息估算 Token 数。
    pub pruned_token_estimate: usize,
    /// Number of tool-output records whose bodies were compacted.
    /// 正文被压缩的工具输出记录数量。
    pub pruned_tool_output_count: usize,
    /// Number of tool-output records intentionally kept intact.
    /// 被有意完整保留的工具输出记录数量。
    pub preserved_tool_output_count: usize,
    /// Number of oldest source messages dropped after tool-output pruning still exceeded the budget.
    /// 工具输出剪枝后仍超过预算时被丢弃的最旧源消息数量。
    pub dropped_source_message_count: usize,
    /// Number of remaining source messages whose body was truncated as a last-resort fit attempt.
    /// 作为最后兜底适配尝试被截断正文的剩余源消息数量。
    pub truncated_source_message_count: usize,
    /// Whether the pruned source still exceeds the target budget.
    /// 剪枝后的源消息是否仍超过目标预算。
    pub still_exceeds_budget: bool,
}

impl CompactionInputPruneReport {
    /// Builds a no-op report for full-fidelity compaction input.
    /// 为完整保真压缩输入构建一份无操作报告。
    ///
    /// # Parameters
    /// # 参数
    /// `budget` is the target input budget used by the current compaction attempt.
    /// `budget` 是当前压缩尝试使用的目标输入预算。
    /// `source_token_estimate` is the estimated size of the unmodified source.
    /// `source_token_estimate` 是未修改源消息的估算大小。
    ///
    /// # Returns
    /// # 返回值
    /// Returns a report indicating that no clone-only pruning was performed.
    /// 返回一份表示未执行仅克隆剪枝的报告。
    pub fn no_pruning(budget: CompactionInputBudget, source_token_estimate: usize) -> Self {
        Self {
            mode: CompactionInputPruneMode::None,
            target_input_budget_tokens: budget.target_input_budget_tokens,
            safety_reserve_tokens: budget.safety_reserve_tokens,
            original_token_estimate: source_token_estimate,
            pruned_token_estimate: source_token_estimate,
            pruned_tool_output_count: 0,
            preserved_tool_output_count: 0,
            dropped_source_message_count: 0,
            truncated_source_message_count: 0,
            still_exceeds_budget: source_token_estimate > budget.target_input_budget_tokens,
        }
    }
}

/// Result returned after preparing a model-facing compaction source.
/// 准备面向模型的压缩源后返回的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCompactionSource {
    /// Source messages that should be sent to the compaction model.
    /// 应发送给压缩模型的源消息。
    pub source_messages: Vec<SessionMessageEntry>,
    /// Report describing whether and how the cloned source was pruned.
    /// 描述克隆源是否以及如何被剪枝的报告。
    pub prune_report: CompactionInputPruneReport,
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

    let retained_tail_start = match input.source_until_message_id {
        Some(source_until_message_id) => context_candidates
            .iter()
            .position(|message| message.id == source_until_message_id)
            .map(|index| index.saturating_add(1))?,
        None => recent_tail_start_index(&context_candidates, input.policy),
    };
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
    build_context_compaction_request_prompt()
}

/// Builds the synthetic continuation prompt injected as the first user message after compaction completes.
/// 构建在压缩完成后注入为首条用户消息的合成续写提示词。
pub fn build_compaction_continuation_prompt(summary_text: &str) -> String {
    let normalized_summary = normalize_compaction_summary_text(summary_text);
    format!(
        "This session is being continued from earlier context that was compacted to fit the model context window.\n\
\n\
<previous-summary>\n\
{normalized_summary}\n\
</previous-summary>\n\
\n\
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
    let continuation_prompt = build_compaction_continuation_prompt(&normalized_summary);
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
    }
}

/// Builds the legacy host-side summary text retained as a fallback when prompt-based model compaction cannot complete.
/// 构建在提示词式模型压缩无法完成时保留的旧版宿主侧回退总结文本。
fn build_legacy_compaction_summary_text(
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
fn absolute_turn_index_for_message(
    messages: &[SessionMessageEntry],
    message_id: &str,
) -> Option<i64> {
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
fn recent_tail_start_index(
    messages: &[SessionMessageEntry],
    policy: ContextCompactionPolicy,
) -> usize {
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
        if retained_turns == 0 || retained_tokens + segment_tokens <= policy.preserve_recent_tokens
        {
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
    estimate_mixed_language_text_tokens(trimmed).max(1usize)
}

/// Returns whether one tool-output transcript row must stay complete in pruned compaction input.
/// 判断一条工具输出转录行在剪枝压缩输入中是否必须完整保留。
///
/// # Parameters
/// # 参数
/// `message` is the tool-output transcript row being considered for clone-only pruning.
/// `message` 是正在判断是否可仅克隆剪枝的工具输出转录行。
///
/// # Returns
/// # 返回值
/// Returns `true` when the row carries error, structural, generated, patch, or ask data.
/// 当该行包含错误、结构性数据、生成产物、补丁或 ask 数据时返回 `true`。
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
        for segment in session_tail_segments(&messages) {
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
    let tool_title = canonical_tool_summary_title(message, "Tool Output");
    let preview = tool_output_compaction_preview(&message.body);
    compacted.body = format!(
        "[Tool result compacted for context-compaction input only]\n\
Tool: {tool_title}\n\
Result ID: {}\n\
Reason: older successful tool output exceeded target compaction input budget\n\
Original size: {} chars\n\
Preserved preview:\n{}",
        message.id,
        message.body.chars().count(),
        preview
    );
    compacted.metadata = None;
    compacted
}

/// Builds a bounded preview from the head and tail of one old tool output.
/// 从旧工具输出的头尾构建有界预览。
///
/// # Parameters
/// # 参数
/// `text` is the original tool-output text.
/// `text` 是原始工具输出文本。
///
/// # Returns
/// # 返回值
/// Returns a bounded preview that preserves the beginning and ending lines.
/// 返回保留开头与结尾行的有界预览。
fn tool_output_compaction_preview(text: &str) -> String {
    let lines = text.lines().collect::<Vec<_>>();
    if lines.is_empty() {
        return String::from("<empty tool output>");
    }
    let head = lines.iter().take(8).copied().collect::<Vec<_>>();
    let tail = lines
        .iter()
        .rev()
        .take(8)
        .copied()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();
    if lines.len() <= 18 {
        return lines.join("\n");
    }
    format!(
        "{}\n...\n[{} middle lines omitted]\n...\n{}",
        head.join("\n"),
        lines.len().saturating_sub(head.len() + tail.len()),
        tail.join("\n")
    )
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_context_message(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
    ) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_string(),
            kind,
            record_kind,
            visible: true,
            title: String::new(),
            body: body.to_string(),
            metadata: None,
            created_at: String::new(),
        }
    }

    fn test_tool_result_message(id: &str, body: &str) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_string(),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("Tool Result shell_command"),
            body: body.to_string(),
            metadata: Some(serde_json::json!({
                "tool_name": "shell_command",
                "tool_result_id": id,
            })),
            created_at: String::new(),
        }
    }

    /// Verifies prompt-shaped compaction results keep a compact preview separate from the injected continuation body.
    /// 验证提示词形态压缩结果会把紧凑预览与注入的续写正文分开保留。
    #[test]
    fn compaction_result_carries_compact_preview() {
        let plan = CompactionPlan {
            source_messages: Vec::new(),
            source_until_message_id: Some(String::from("m-1")),
            source_until_turn_index: Some(1),
            retained_tail_start_turn_index: Some(2),
            compaction_id: String::from("cp-1"),
            created_at: String::from("20"),
            policy: ContextCompactionPolicy::default(),
        };

        let result = CompactionResult::from_model_summary(
            &plan,
            "保留目标、约束和未完成工作，同时不要重复工具说明。",
        );

        assert_eq!(
            result.summary_record.preview,
            "保留目标、约束和未完成工作，同时不要重复工具说明。"
        );
        assert!(
            result
                .summary_record
                .body
                .contains("<previous-summary>\n保留目标、约束和未完成工作，同时不要重复工具说明。\n</previous-summary>")
        );
    }

    /// Verifies previous summaries are persisted in the tagged cache-stable form used by later compactions.
    /// 验证历史摘要会以供后续压缩复用的带标签缓存稳定格式持久化。
    #[test]
    fn compaction_result_persists_previous_summary_block() {
        let plan = CompactionPlan {
            source_messages: Vec::new(),
            source_until_message_id: Some(String::from("m-1")),
            source_until_turn_index: Some(1),
            retained_tail_start_turn_index: Some(2),
            compaction_id: String::from("cp-1"),
            created_at: String::from("20"),
            policy: ContextCompactionPolicy::default(),
        };

        let result = CompactionResult::from_model_summary(
            &plan,
            "<previous-summary>\n保留架构决策与预算策略。\n</previous-summary>",
        );

        assert_eq!(result.summary_record.preview, "保留架构决策与预算策略。");
        assert_eq!(
            result
                .summary_record
                .body
                .matches("<previous-summary>")
                .count(),
            1
        );
        assert!(
            result
                .summary_record
                .body
                .contains("保留架构决策与预算策略。")
        );
    }

    /// Verifies model compaction output persists only the durable summary section.
    /// 验证模型压缩输出只会持久化可延续的摘要段落。
    #[test]
    fn extract_compaction_model_summary_strips_analysis_block() {
        let output = "<analysis>\n逐条检查上下文。\n</analysis>\n<summary>\nPrimary Request and Intent:\n- 继续实现上下文压缩升级。\n</summary>";

        assert_eq!(
            extract_compaction_model_summary(output),
            Some(String::from(
                "Primary Request and Intent:\n- 继续实现上下文压缩升级。"
            ))
        );
    }

    /// Verifies analysis-only model output is rejected instead of becoming durable memory.
    /// 验证只有分析草稿的模型输出会被拒绝而不是成为持久记忆。
    #[test]
    fn extract_compaction_model_summary_rejects_analysis_only_output() {
        let output = "<analysis>\n只有草稿，没有摘要。\n</analysis>";

        assert_eq!(extract_compaction_model_summary(output), None);
    }

    /// Verifies tagged model output ignores non-durable text outside summary.
    /// 验证带标签的模型输出会忽略摘要之外的非持久文本。
    #[test]
    fn extract_compaction_model_summary_ignores_text_outside_summary() {
        let output = "draft preface\n<analysis>\n草稿。\n</analysis>\nleaked note\n<summary>\nNext Step:\n- 继续验证。\n</summary>\ntrailing note";

        assert_eq!(
            extract_compaction_model_summary(output),
            Some(String::from("Next Step:\n- 继续验证。"))
        );
    }

    /// Verifies tagged model output without a summary block is rejected.
    /// 验证带标签但缺少摘要段的模型输出会被拒绝。
    #[test]
    fn extract_compaction_model_summary_rejects_tagged_output_without_summary() {
        let output = "draft preface\n<analysis>\n草稿。\n</analysis>\nleaked note";

        assert_eq!(extract_compaction_model_summary(output), None);
    }

    /// Verifies legacy continuation prompts still expose only their summary payload.
    /// 验证旧版续写提示词仍然只会暴露其中的摘要正文。
    #[test]
    fn extract_compaction_previous_summary_supports_legacy_body() {
        let body = "This session is being continued from earlier context.\n\nEarlier context summary:\n保留目标、约束和未完成项。\n\nContinue from the newer messages below without restarting.";

        assert_eq!(
            extract_compaction_previous_summary(body),
            "保留目标、约束和未完成项。"
        );
    }

    /// Verifies chained compaction keeps the previous continuation summary inside the next source window.
    /// 验证链式压缩会把上一轮续写摘要保留在下一次源窗口内。
    #[test]
    fn plan_context_compaction_keeps_previous_summary_in_next_source_window() {
        let previous_summary = test_context_message(
            "summary-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::CompactionSummary,
            "This session is being continued from earlier context.\n\n<previous-summary>\n保留架构决策、预算策略和未完成项。\n</previous-summary>\n\nContinue from the newer messages below without restarting.",
        );
        let mut messages = vec![previous_summary.clone()];
        for index in 0..8 {
            messages.push(test_context_message(
                &format!("user-{index}"),
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                &format!("新增用户上下文 {index}"),
            ));
            messages.push(test_context_message(
                &format!("assistant-{index}"),
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                &format!("新增助手输出 {index}"),
            ));
        }

        let plan = plan_context_compaction(CompactionPlanInput {
            session_label: "test-session",
            messages: &messages,
            all_messages: &messages,
            context_start_message_id: Some("summary-1"),
            source_until_message_id: None,
            policy: ContextCompactionPolicy::default(),
        })
        .expect("chained context should still produce a compaction plan");

        assert_eq!(plan.source_messages.first(), Some(&previous_summary));
        assert!(
            plan.source_messages
                .iter()
                .any(|message| message.id == "summary-1"
                    && message.record_kind == SessionMessageRecordKind::CompactionSummary)
        );
    }

    /// Verifies explicit partial source boundaries override the automatic retained-tail cut.
    /// 验证明示 partial 源边界会覆盖自动保留尾部切分点。
    #[test]
    fn plan_context_compaction_respects_explicit_source_until_message_id() {
        let messages = vec![
            test_context_message(
                "user-0",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "旧用户请求",
            ),
            test_context_message(
                "assistant-0",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "旧助手回答",
            ),
            test_context_message(
                "user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "保留的新用户请求",
            ),
            test_context_message(
                "assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "保留的新助手回答",
            ),
        ];

        let plan = plan_context_compaction(CompactionPlanInput {
            session_label: "test-session",
            messages: &messages,
            all_messages: &messages,
            context_start_message_id: None,
            source_until_message_id: Some("assistant-0"),
            policy: ContextCompactionPolicy::default(),
        })
        .expect("explicit source boundary should produce a compaction plan");

        assert_eq!(plan.source_until_message_id.as_deref(), Some("assistant-0"));
        assert_eq!(
            plan.source_messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["user-0", "assistant-0"]
        );
        assert_eq!(plan.retained_tail_start_turn_index, Some(1));
    }

    /// Verifies compaction input keeps full source messages when the target budget is sufficient.
    /// 验证目标预算充足时压缩输入会保留完整源消息。
    #[test]
    fn prepare_compaction_source_keeps_full_input_when_budget_fits() {
        let messages = vec![
            test_context_message(
                "user-0",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "请检查项目",
            ),
            test_tool_result_message("tool-0", "short successful output"),
        ];

        let prepared = prepare_compaction_source_for_target_budget(
            &messages,
            CompactionInputBudget {
                target_input_budget_tokens: 10_000,
                safety_reserve_tokens: 30_000,
            },
            ContextCompactionPolicy::default(),
            CompactionInputPruneMode::RequiredForWindowDowngrade,
        );

        assert_eq!(prepared.source_messages, messages);
        assert_eq!(prepared.prune_report.mode, CompactionInputPruneMode::None);
        assert_eq!(prepared.prune_report.pruned_tool_output_count, 0);
    }

    /// Verifies window-downgrade pruning only rewrites cloned old tool output.
    /// 验证窗口降级剪枝只会改写克隆出来的旧工具输出。
    #[test]
    fn prepare_compaction_source_prunes_old_tool_output_clone_only() {
        let long_output = (0..300)
            .map(|index| format!("line {index}: successful search hit"))
            .collect::<Vec<_>>()
            .join("\n");
        let messages = vec![
            test_context_message(
                "user-0",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "旧请求",
            ),
            test_tool_result_message("tool-old", &long_output),
            test_context_message(
                "user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "当前请求",
            ),
            test_tool_result_message("tool-recent", "recent output must stay"),
        ];
        let original = messages.clone();

        let prepared = prepare_compaction_source_for_target_budget(
            &messages,
            CompactionInputBudget {
                target_input_budget_tokens: 1_000,
                safety_reserve_tokens: 30_000,
            },
            ContextCompactionPolicy {
                tail_turns: 1,
                preserve_recent_tokens: 10_000,
                ..ContextCompactionPolicy::default()
            },
            CompactionInputPruneMode::RequiredForWindowDowngrade,
        );

        assert_eq!(messages, original, "source input must remain unchanged");
        assert!(
            prepared.source_messages[1]
                .body
                .contains("[Tool result compacted for context-compaction input only]")
        );
        assert_eq!(prepared.source_messages[3].body, "recent output must stay");
        assert_eq!(prepared.prune_report.pruned_tool_output_count, 1);
        assert_eq!(prepared.prune_report.preserved_tool_output_count, 1);
        assert_eq!(prepared.prune_report.dropped_source_message_count, 0);
    }

    /// Verifies clone-only pruning can drop oldest segments when tool-output pruning still exceeds budget.
    /// 验证仅克隆剪枝在工具输出剪枝后仍超预算时可以丢弃最旧分段。
    #[test]
    fn prepare_compaction_source_drops_old_segments_when_tool_pruning_is_not_enough() {
        let huge_context = (0..200)
            .map(|index| format!("context line {index}: detailed non-tool discussion"))
            .collect::<Vec<_>>()
            .join("\n");
        let messages = vec![
            test_context_message(
                "user-0",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                &huge_context,
            ),
            test_context_message(
                "user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                &huge_context,
            ),
            test_context_message(
                "user-2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "recent request must stay",
            ),
        ];
        let original = messages.clone();

        let prepared = prepare_compaction_source_for_target_budget(
            &messages,
            CompactionInputBudget {
                target_input_budget_tokens: 1,
                safety_reserve_tokens: 30_000,
            },
            ContextCompactionPolicy {
                tail_turns: 1,
                preserve_recent_tokens: 10_000,
                ..ContextCompactionPolicy::default()
            },
            CompactionInputPruneMode::RequiredForWindowDowngrade,
        );

        assert_eq!(messages, original, "source input must remain unchanged");
        assert_eq!(prepared.source_messages.len(), 1);
        assert_eq!(prepared.source_messages[0].id, "user-2");
        assert_eq!(prepared.prune_report.dropped_source_message_count, 2);
        assert_eq!(prepared.prune_report.pruned_tool_output_count, 0);
    }

    /// Verifies clone-only pruning can split an oversized protected tail while keeping the newest message.
    /// 验证仅克隆剪枝可以切分超大的受保护尾部，同时保留最新消息。
    #[test]
    fn prepare_compaction_source_splits_oversized_tail_when_needed() {
        let huge_context = (0..200)
            .map(|index| format!("tail line {index}: detailed recent discussion"))
            .collect::<Vec<_>>()
            .join("\n");
        let messages = vec![
            test_context_message(
                "user-0",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                &huge_context,
            ),
            test_context_message(
                "assistant-0",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                &huge_context,
            ),
            test_context_message(
                "assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "newest detail must stay",
            ),
        ];
        let original = messages.clone();

        let prepared = prepare_compaction_source_for_target_budget(
            &messages,
            CompactionInputBudget {
                target_input_budget_tokens: 1,
                safety_reserve_tokens: 30_000,
            },
            ContextCompactionPolicy {
                tail_turns: 1,
                preserve_recent_tokens: 10_000,
                ..ContextCompactionPolicy::default()
            },
            CompactionInputPruneMode::RequiredForWindowDowngrade,
        );

        assert_eq!(messages, original, "source input must remain unchanged");
        assert_eq!(prepared.source_messages.len(), 1);
        assert_eq!(prepared.source_messages[0].id, "assistant-1");
        assert!(
            prepared.source_messages[0]
                .body
                .contains("[Message truncated for context-compaction input only]")
        );
        assert_eq!(prepared.prune_report.dropped_source_message_count, 2);
        assert_eq!(prepared.prune_report.truncated_source_message_count, 1);
    }

    /// Verifies error-like tool output is preserved even when clone-only pruning is enabled.
    /// 验证启用仅克隆剪枝时仍会完整保留错误类工具输出。
    #[test]
    fn prepare_compaction_source_preserves_error_tool_output() {
        let long_output = (0..300)
            .map(|index| format!("line {index}: successful output"))
            .collect::<Vec<_>>()
            .join("\n");
        let messages = vec![
            test_context_message(
                "user-0",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "旧请求",
            ),
            test_tool_result_message("tool-old", &long_output),
            test_tool_result_message(
                "tool-error",
                "stderr: compilation failed\nerror[E0425]: cannot find value",
            ),
            test_context_message(
                "user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "当前请求",
            ),
        ];

        let prepared = prepare_compaction_source_for_target_budget(
            &messages,
            CompactionInputBudget {
                target_input_budget_tokens: 1_000,
                safety_reserve_tokens: 30_000,
            },
            ContextCompactionPolicy {
                tail_turns: 1,
                preserve_recent_tokens: 1,
                ..ContextCompactionPolicy::default()
            },
            CompactionInputPruneMode::RequiredForWindowDowngrade,
        );

        assert_eq!(
            prepared.source_messages[2].body,
            "stderr: compilation failed\nerror[E0425]: cannot find value"
        );
        assert_eq!(prepared.prune_report.preserved_tool_output_count, 1);
        assert_eq!(prepared.prune_report.pruned_tool_output_count, 1);
        assert_eq!(prepared.prune_report.dropped_source_message_count, 0);
        assert_eq!(prepared.prune_report.truncated_source_message_count, 0);
    }
}
