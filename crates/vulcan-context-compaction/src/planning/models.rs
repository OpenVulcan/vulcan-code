use vulcan_context::{
    ContextCompactionPolicy, ContextCompactionTailPolicySource, SessionMessageEntry,
    SessionMessageKind, SessionMessageRecordKind,
};

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

/// Options used when building the synthetic post-compaction continuation prompt.
/// 构建压缩后合成续写提示词时使用的选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CompactionContinuationPromptOptions {
    /// Whether recent messages are known to be preserved verbatim after the summary.
    /// 是否已知摘要后会按原文保留最近消息。
    pub recent_messages_preserved: bool,
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
    /// Optional last user-turn index covered by the compaction source window when available.
    /// 可用时记录被压缩源窗口覆盖的最后一个用户轮次索引。
    pub source_until_turn_index: Option<i64>,
    /// Optional first retained user-turn index kept verbatim after the compaction rewrite when the tail starts on a user turn.
    /// 当尾部从用户轮次开始时，记录压缩改写后按原样保留的第一个用户轮次索引。
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
    /// Frozen retained-tail selection consumed by rewrite without recomputing boundaries.
    /// 由 rewrite 消费且不重新计算边界的冻结尾部选择。
    pub tail_selection: CompactionTailSelectionReport,
}

/// Frozen retained-tail selection and diagnostics produced by the planner.
/// 由规划器产出的冻结尾部选择与诊断。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactionTailSelectionReport {
    /// Raw model-declared total context window used by the formula.
    /// 公式使用的模型原始声明总上下文窗口。
    pub context_limit: Option<usize>,
    /// Provenance of the adaptive tail policy.
    /// 自适应尾部策略的来源。
    pub policy_source: ContextCompactionTailPolicySource,
    /// Nominal retained-tail token budget.
    /// 名义尾部保留 Token 预算。
    pub nominal_token_budget: usize,
    /// Maximum retained assistant/provider execution segments.
    /// 最大保留 Assistant/Provider 执行段数量。
    pub segment_limit: usize,
    /// Actual retained execution-segment count.
    /// 实际保留执行段数量。
    pub retained_segment_count: usize,
    /// Ordered canonical message identifiers in the retained contiguous suffix.
    /// 保留连续后缀中的有序规范消息标识。
    pub retained_message_ids: Vec<String>,
    /// Estimated tokens after historical tool-output previews are projected.
    /// 投影历史工具输出预览后的估算 Token 数。
    pub retained_token_estimate: usize,
    /// Number of historical tool outputs previewed in the retained suffix.
    /// 保留后缀中被预览的历史工具输出数量。
    pub previewed_tool_output_count: usize,
    /// Estimated original tool-output tokens omitted from the projection.
    /// 从投影中省略的原始工具输出估算 Token 数。
    pub omitted_tool_output_token_estimate: usize,
    /// Whether the newest complete segment alone exceeds the nominal budget.
    /// 最新完整执行段是否单独超过名义预算。
    pub newest_segment_exceeds_nominal_budget: bool,
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
    /// Frozen retained-tail selection that rewrite must apply exactly.
    /// rewrite 必须精确应用的冻结尾部选择。
    pub tail_selection: CompactionTailSelectionReport,
}
