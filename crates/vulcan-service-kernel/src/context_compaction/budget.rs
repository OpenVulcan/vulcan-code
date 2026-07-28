//! Context compaction budget helpers.
//! 上下文压缩 budget 辅助逻辑。

use super::*;

/// Returns the current compact budget note for one compaction status row.
/// 返回单条压缩状态行使用的当前预算摘要。
pub(crate) fn context_compaction_budget_note_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> String {
    context_compaction_budget_snapshot_from_state(state, session_id)
        .map(|budget| budget.summary_note())
        .unwrap_or_else(|| String::from("当前未提供预算快照"))
}

/// Returns the best available budget snapshot for compaction decisions.
/// 返回用于压缩决策的最佳可用预算快照。
///
/// # Parameters
/// # 参数
/// `state` is the shared service bridge state containing live and persisted runtime snapshots.
/// `state` 是包含实时与持久运行态快照的共享服务桥接状态。
/// `session_id` identifies the session whose context budget should be read.
/// `session_id` 标识需要读取上下文预算的会话。
/// # Returns
/// # 返回值
/// Returns the live snapshot enriched with the latest historical provider usage when available.
/// 返回实时快照，并在可用时合并最近一次历史 Provider 用量。
pub(crate) fn context_compaction_budget_snapshot_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Option<ContextBudgetSnapshot> {
    let (live_budget, runtime_state, public_model_capabilities) = state
        .lock()
        .ok()
        .map(|guard| {
            (
                guard.live_context_budgets.get(&session_id).cloned(),
                guard
                    .workspace_snapshot
                    .as_ref()
                    .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                    .cloned(),
                active_public_ai_model_capabilities(
                    guard
                        .agent_runtime
                        .as_ref()
                        .map(|runtime| runtime.public_ai_runtime.as_ref()),
                ),
            )
        })
        .unwrap_or_default();
    let historical_budget = runtime_state.as_ref().and_then(|runtime_state| {
        historical_context_budget_snapshot_from_runtime_state(
            runtime_state,
            public_model_capabilities.as_ref(),
        )
    });
    live_budget
        .map(|mut live_budget| {
            if live_budget.observed_usage.is_none()
                && let Some(usage) = historical_budget
                    .as_ref()
                    .and_then(|budget| budget.observed_usage.as_ref())
            {
                apply_observed_usage_to_context_budget(&mut live_budget, usage);
            }
            live_budget
        })
        .or(historical_budget)
}

/// Builds the target compaction input budget from one context budget snapshot.
/// 根据上下文预算快照构建目标压缩输入预算。
pub(crate) fn context_compaction_input_budget_from_snapshot(
    budget: Option<&ContextBudgetSnapshot>,
) -> Option<CompactionInputBudget> {
    let safety_reserve_tokens = CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS
        .max(MIN_CONTEXT_COMPACTION_INPUT_SAFETY_RESERVE_TOKENS);
    budget
        .and_then(|budget| budget.context_limit.or(budget.effective_input_limit))
        .map(|context_limit| CompactionInputBudget {
            target_input_budget_tokens: context_limit
                .saturating_sub(safety_reserve_tokens)
                .max(1usize),
            safety_reserve_tokens,
        })
}

/// Returns the latest provider-observed input tokens for compaction downgrade decisions.
/// 返回用于压缩降级判断的最近一次 Provider 实测输入 Token。
pub(crate) fn context_compaction_observed_input_tokens_from_snapshot(
    budget: Option<&ContextBudgetSnapshot>,
) -> Option<usize> {
    budget.and_then(|budget| {
        budget.observed_usage.as_ref().and_then(|usage| {
            vulcan_context::observed_input_tokens_from_usage(Some(usage)).or(usage.total_tokens)
        })
    })
}

/// Returns the latest budget-estimated input tokens when provider feedback is unavailable.
/// 当 Provider 反馈不可用时返回最近一次预算估算输入 Token。
pub(crate) fn context_compaction_estimated_input_tokens_from_snapshot(
    budget: Option<&ContextBudgetSnapshot>,
) -> Option<usize> {
    budget.map(|budget| budget.estimated_input_tokens)
}

/// Immutable baseline captured before a compaction draft is built.
/// 构建压缩候选状态前捕获的不可变基线。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WebuiContextCompactionBaseline {
    /// Number of canonical provider messages in the active state.
    /// 活跃状态中的规范 Provider 消息数量。
    message_count: usize,
    /// Number of canonical transcript records in the active state.
    /// 活跃状态中的规范转录记录数量。
    record_count: usize,
    /// Latest canonical provider message id in the active state.
    /// 活跃状态中最新的规范 Provider 消息标识。
    last_message_id: Option<String>,
    /// Latest canonical transcript record id in the active state.
    /// 活跃状态中最新的规范转录记录标识。
    last_record_id: Option<String>,
    /// Context-start anchor active when the draft was built.
    /// 构建候选状态时生效的上下文起始锚点。
    context_start_message_id: Option<String>,
}

impl WebuiContextCompactionBaseline {
    /// Captures the current active runtime state shape used to validate a later draft commit.
    /// 捕获当前活跃运行时状态形态，用于校验后续候选状态提交。
    ///
    /// # Parameters
    /// # 参数
    /// `runtime_state` is the active session runtime state before draft construction begins.
    /// `runtime_state` 是候选状态构建开始前的活跃会话运行时状态。
    ///
    /// # Returns
    /// # 返回值
    /// A compact baseline containing state shape and anchor identifiers.
    /// 返回包含状态形态与锚点标识的紧凑基线。
    pub(crate) fn capture(runtime_state: &SessionRuntimeStateEntry) -> Self {
        Self {
            message_count: runtime_state.conversation_state.messages.len(),
            record_count: runtime_state.conversation_state.records.len(),
            last_message_id: runtime_state
                .conversation_state
                .messages
                .last()
                .map(|message| message.id.clone()),
            last_record_id: runtime_state
                .conversation_state
                .records
                .last()
                .map(|record| record.id.clone()),
            context_start_message_id: runtime_state.context_start_message_id.clone(),
        }
    }

    /// Returns whether the active runtime state still matches this baseline.
    /// 返回活跃运行时状态是否仍匹配该基线。
    ///
    /// # Parameters
    /// # 参数
    /// `runtime_state` is the active runtime state checked immediately before draft commit.
    /// `runtime_state` 是候选状态提交前立即检查的活跃运行时状态。
    ///
    /// # Returns
    /// # 返回值
    /// `true` when the active state has not changed in ways that would make the draft stale.
    /// 当活跃状态未发生会使候选状态过期的变化时返回 `true`。
    pub(crate) fn matches(&self, runtime_state: &SessionRuntimeStateEntry) -> bool {
        self == &Self::capture(runtime_state)
    }
}
