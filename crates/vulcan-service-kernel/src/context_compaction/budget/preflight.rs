//! Durable usage pressure checked before the first business Provider request.
//! 在首个业务 Provider 请求之前检查可靠用量压力。

use super::*;
use vulcan_agent_runtime::agent_runtime::provider::ProviderRuntimeBlueprint;
use vulcan_session_core::v1::{
    SessionDocument,
    event::{ApiPurposeKind, EventBody},
};

/// Reads prior uncompacted business usage with the pending request's frozen route and policy.
/// 按待发请求冻结的路由与策略读取此前尚未压缩的业务用量。
/// `state` owns `session_id`, `route` is frozen for dispatch, and `budget` supplies current limits.
/// `state` 持有 `session_id`，`route` 已为分派冻结，`budget` 提供当前限制。
/// Returns an observed soft-compaction budget, no eligible observation, or an explicit storage failure.
/// 返回实测软压缩预算、无可用观测，或显式存储错误。
pub(in crate::context_compaction) fn observed_compaction_budget_before_provider_request(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    route: &ProviderRuntimeBlueprint,
    budget: &ContextBudgetSnapshot,
) -> Result<Option<ContextBudgetSnapshot>, String> {
    if !budget.compaction_auto_enabled || !budget.baseline_matches_active_runtime {
        return Ok(None);
    }
    // Release the service lock before acquiring the session writer and publisher locks.
    // 获取会话写入器及发布锁前释放服务锁。
    let handle = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .session_runtime_registry
        .get(session_id)
        .map_err(|error| error.to_string())?;
    // The resident document answers whenever it holds the latest business sample, without the writer lock.
    // 常驻文档持有最近业务样本时直接作答，不占用写入锁。
    let resident = handle
        .read(|document| observed_compaction_budget_from_document(document, route, budget, false))
        .map_err(|error| error.to_string())??;
    if let PreflightObservation::Decided(observed) = resident {
        return Ok(observed.map(|observed| *observed));
    }
    // Read sealed segments too: a newly opened turn can be the only resident tail after restart.
    // 同时读取封存分段：重启后新回合可能是内存中唯一保留的尾部。
    match handle
        .read_full_durable(|document| {
            observed_compaction_budget_from_document(document, route, budget, true)
        })
        .map_err(|error| error.to_string())??
    {
        PreflightObservation::Decided(observed) => Ok(observed.map(|observed| *observed)),
        PreflightObservation::Exhausted => Ok(None),
    }
}

/// Outcome of scanning one document for the latest business usage sample.
/// 在一份文档中扫描最近业务用量样本的结果。
enum PreflightObservation {
    /// The latest business sample was reached and settles the answer.
    /// 已到达最近业务样本并据此定论。
    Decided(Option<Box<ContextBudgetSnapshot>>),
    /// The resident window ended before any decisive sample.
    /// 常驻窗口在任何定论样本之前结束。
    Exhausted,
}

/// Projects the latest usable business observation within the current uncompacted context.
/// 投影当前尚未压缩上下文中最近一次可用的业务观测。
/// `document` is resident or complete history (`complete_history`), `route` is frozen for dispatch, and `budget`
/// owns the current policy.
/// `document` 为常驻或完整历史(`complete_history`)，`route` 已为分派冻结，`budget` 持有当前策略。
/// Returns a soft-compaction signal only; it never calibrates the final request or changes its hard-limit gate.
/// 仅返回软压缩信号；绝不校准最终请求，也不修改其硬上限闸门。
fn observed_compaction_budget_from_document(
    document: &SessionDocument,
    route: &ProviderRuntimeBlueprint,
    budget: &ContextBudgetSnapshot,
    complete_history: bool,
) -> Result<PreflightObservation, String> {
    // A committed summary invalidates all earlier usage, including the summary request's own usage.
    // 已提交摘要会使之前的全部用量失效，包含摘要请求自身的用量。
    let current_compaction = document
        .runtime_lock
        .current_compaction_id
        .map(|id| {
            document
                .compactions
                .iter()
                .find(|node| node.compaction_id == id)
                .ok_or_else(|| {
                    String::from(
                        "Compaction preflight found no node for the current compaction pointer.",
                    )
                })
        })
        .transpose()?;
    // This is the actual retained frontier, independent of the session's selected numeric window.
    // 这是实际保留前沿，与会话所选数值窗口相互独立。
    let retained_from_seq = current_compaction
        .map(|node| node.retained_from_seq.value())
        .unwrap_or(1);
    for event in document.events().iter().rev() {
        let EventBody::ApiStats(stats) = &event.event else {
            continue;
        };
        if !matches!(
            stats.purpose,
            ApiPurposeKind::MainGeneration
                | ApiPurposeKind::RetryGeneration
                | ApiPurposeKind::Continue
                | ApiPurposeKind::Subagent
        ) {
            continue;
        }
        // API identity, rather than neighboring positions or success status, owns the usage sample.
        // 用量样本由 API 身份归属，而非相邻位置或成功状态。
        let Some(started) = document
            .events()
            .iter()
            .find_map(|event| match &event.event {
                EventBody::ApiStarted(started) if started.api_call_id == stats.api_call_id => {
                    Some(started)
                }
                _ => None,
            })
        else {
            if !complete_history {
                return Ok(PreflightObservation::Exhausted);
            }
            return Err(format!(
                "Compaction preflight found no start for API {}.",
                stats.api_call_id.value()
            ));
        };
        if started.projection.segment_generation != document.segment_generation()
            || started.projection.retained_from_seq.value() != retained_from_seq
            || current_compaction
                .is_some_and(|node| started.projection.through_seq < node.api_stats_seq)
            || started.request.provider != route.provider_id
            || started.request.model != route.model_id
            || started.request.profile.as_deref() != Some(route.protocol_profile_id.as_str())
            || started.request.connection_id.as_deref() != Some(route.connection_id.as_str())
        {
            // Do not search behind an edit, compaction or route transition for an older convenient sample.
            // 不越过编辑、压缩或路由切换寻找更早的便利样本。
            return Ok(PreflightObservation::Decided(None));
        }
        if stats.usage.input_tokens.is_none() && stats.usage.total_tokens.is_none() {
            // A failure without input evidence cannot reset the previous known context to zero.
            // 没有输入证据的失败不能把此前已知上下文重置为零。
            continue;
        }
        // Reuse the same normalized dimensions and total/output accounting as the context meter.
        // 复用上下文用量表使用的同一归一化维度及总量、输出记账。
        let usage =
            crate::workbench_context_usage::v1_usage_to_core(&stats.usage).ok_or_else(|| {
                String::from("Compaction preflight usage exceeds the host token range.")
            })?;
        // Historical pressure uses today's frozen thresholds but remains separate from exact request accounting.
        // 历史压力使用本次冻结阈值，但与精确请求记账保持分离。
        let mut observed = budget.clone();
        apply_observed_usage_to_context_budget(&mut observed, &usage);
        observed.accounting_source =
            vulcan_context::ContextBudgetAccountingSource::ProviderReported;
        observed.accounting_warning_code = None;
        observed.provider_anchor_call_id = Some(stats.api_call_id.value().to_string());
        return Ok(PreflightObservation::Decided(Some(Box::new(observed))));
    }
    Ok(if complete_history {
        PreflightObservation::Decided(None)
    } else {
        PreflightObservation::Exhausted
    })
}

#[cfg(test)]
mod tests;
