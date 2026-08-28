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
    let (
        live_budget,
        runtime_state,
        public_model_capabilities,
        compression_window_limit,
        session_runtime_registry,
    ) = state
        .lock()
        .ok()
        .map(|guard| {
            // runtimeState is cloned once so node resolution and historical usage share one revision.
            // runtimeState 只克隆一次，使节点解析与历史用量共享同一修订。
            let runtime_state = guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                .cloned();
            // publicAiConfiguration resolves only the numeric node value and is never persisted by the session.
            // publicAiConfiguration 只解析节点数值，且该数值绝不会由会话持久化。
            let public_ai_configuration = guard
                .agent_runtime
                .as_ref()
                .and_then(|runtime| runtime.public_ai_runtime.configuration_snapshot().ok());
            (
                guard.live_context_budgets.get(&session_id).cloned(),
                runtime_state.clone(),
                active_public_ai_model_capabilities(
                    guard
                        .agent_runtime
                        .as_ref()
                        .map(|runtime| runtime.public_ai_runtime.as_ref()),
                ),
                crate::context_compaction_window_limit_from_configuration(
                    runtime_state.as_ref(),
                    public_ai_configuration.as_ref(),
                ),
                Some(guard.session_runtime_registry.clone()),
            )
        })
        .unwrap_or_default();
    let v1_usage_facts = session_runtime_registry.as_ref().and_then(|registry| {
        crate::session_context_usage_facts_from_v1_registry(registry.as_ref(), session_id)
            .ok()
            .flatten()
    });
    let historical_budget = runtime_state
        .as_ref()
        .zip(v1_usage_facts.as_ref())
        .and_then(|(runtime_state, v1_usage_facts)| {
            historical_context_budget_snapshot_from_runtime_state(
                runtime_state,
                v1_usage_facts,
                public_model_capabilities.as_ref(),
                compression_window_limit,
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
