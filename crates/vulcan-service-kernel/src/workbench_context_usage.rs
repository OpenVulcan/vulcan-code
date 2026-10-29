//! workbench context usage helpers.
//! workbench context usage 辅助逻辑。

use super::*;
use crate::session_runtime_registry_v1::{SessionRuntimeError, SessionRuntimeRegistry};
use vulcan_session_core::v1::event::ApiStartedEvent;
use vulcan_session_core::v1::{
    AggregatedUsage, ApiStatsEvent, ConversationEvent, EventBody, ProviderRequestContextBreakdown,
    ProviderRequestTextMeasurement, ProviderUsageBudgetAnchor, SessionDocument, UsageSnapshot,
};

/// Canonical V1 Provider facts required by the context-usage projection.
/// 上下文用量投影所需的规范 V1 Provider 事实。
#[derive(Debug, Clone)]
pub(crate) struct SessionContextUsageFacts {
    /// Latest completed API that matches the current active request route and reports input usage.
    /// 与当前生效请求路由一致且上报输入用量的最近一次已完成 API。
    latest_anchor: ProviderUsageBudgetAnchor,
    /// Poison-preserving aggregate over every terminal physical API in the V1 stream.
    /// 对 V1 流中每次终态物理 API 进行保留未知传播语义的聚合。
    cumulative_usage: AggregatedUsage,
    /// Latest request measurement eligible for the current route, including an in-flight request.
    /// 当前路由可用的最近请求测量，包含在途请求。
    latest_request_measurement: ProviderRequestTextMeasurement,
    /// Whether the current V1 runtime points at a committed compaction node.
    /// 当前 V1 runtime 是否指向一个已提交压缩节点。
    has_compaction: bool,
}

/// Projects context-usage facts from one authoritative V1 session document.
/// 从一个权威 V1 会话文档投影上下文用量事实。
pub(crate) fn session_context_usage_facts_from_v1_document(
    document: &SessionDocument,
) -> Option<SessionContextUsageFacts> {
    let events = document.events();
    let active_request = &document.metadata.config.active_request;
    let terminal_usages = events.iter().filter_map(|event| match &event.event {
        EventBody::ApiStats(stats) => Some(&stats.usage),
        _ => None,
    });
    let cumulative_usage = AggregatedUsage::aggregate(terminal_usages);
    let latest_anchor = events.iter().rev().find_map(|event| {
        let EventBody::ApiStats(stats) = &event.event else {
            return None;
        };
        if stats.usage.input_tokens.is_none() && stats.usage.total_tokens.is_none() {
            return None;
        }
        let started = matching_v1_api_started(events, stats)?;
        if !v1_request_matches_active_route(started, active_request) {
            return None;
        }
        Some(ProviderUsageBudgetAnchor {
            api_call_id: started.api_call_id,
            projection: started.projection.clone(),
            evidence: started.request_budget_evidence.clone(),
            usage: stats.usage.clone(),
        })
    })?;
    let latest_request_measurement = events
        .iter()
        .rev()
        .filter_map(|event| match &event.event {
            EventBody::ApiStarted(started)
                if v1_request_matches_active_route(started, active_request) =>
            {
                Some(started)
            }
            _ => None,
        })
        .find(|started| {
            events.iter().find_map(|event| match &event.event {
                EventBody::ApiStats(stats) if stats.api_call_id == started.api_call_id => {
                    Some(!stats.usage.has_no_usage())
                }
                _ => None,
            }) != Some(false)
        })
        .map(|started| {
            started
                .request_budget_evidence
                .request_text_measurement
                .clone()
        })
        .unwrap_or_else(|| latest_anchor.evidence.request_text_measurement.clone());
    Some(SessionContextUsageFacts {
        latest_anchor,
        cumulative_usage,
        latest_request_measurement,
        has_compaction: document.runtime_lock.current_compaction_id.is_some(),
    })
}

/// Reads context-usage facts only from an already registered V1 session owner.
/// 仅从已经注册的 V1 会话 owner 读取上下文用量事实。
pub(crate) fn session_context_usage_facts_from_v1_registry(
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<Option<SessionContextUsageFacts>, String> {
    let handle = match registry.get(session_id) {
        Ok(handle) => handle,
        Err(SessionRuntimeError::SessionNotRegistered(_)) => return Ok(None),
        Err(error) => {
            return Err(format!(
                "failed to resolve registered V1 context usage for session {session_id}: {error}"
            ));
        }
    };
    handle
        .read(session_context_usage_facts_from_v1_document)
        .map_err(|error| {
            format!("failed to read V1 context usage for session {session_id}: {error}")
        })
}

/// Publishes committed API usage for the selected composer without refreshing the workspace.
/// 为选中会话的输入区发布已提交 API 用量，不刷新整个工作区。
/// `app` delivers deltas, `state` owns the selection, and `session_id` identifies the completed API owner.
/// `app` 发送增量，`state` 持有选中状态，`session_id` 标识已完成 API 的所属会话。
/// Returns success for an unchanged or unselected surface, or an explicit projection error.
/// 未变化或未选中的界面返回成功，投影失败时返回明确错误。
pub(crate) fn emit_session_context_usage_changed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    // Read the session owner outside the service lock to preserve the existing lock order.
    // 在服务锁外读取会话所有者，保持现有锁顺序。
    let conversation_id = session_id.to_string();
    // Skip unrelated completions before scanning their API history.
    // 扫描 API 历史前跳过无关会话的完成事件。
    let registry = {
        // The full Workbench snapshot binds composer usage to selected_conversation_id too.
        // 完整 Workbench 快照同样将输入区用量绑定到 selected_conversation_id。
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if guard.workbench.selected_conversation_id.as_deref() != Some(&conversation_id) {
            return Ok(());
        }
        guard.session_runtime_registry.clone()
    };
    // Usage is derived from the same durable API facts used when switching conversations.
    // 用量来自切换会话时使用的同一份可靠 API 事实。
    let facts = session_context_usage_facts_from_v1_registry(registry.as_ref(), session_id)?;
    // Recheck selection after reading facts so background completion cannot replace another session's orb.
    // 读取事实后重新检查选中状态，避免后台完成覆盖其他会话的用量圈。
    let envelope = {
        // This guard commits the retained value and its revision atomically.
        // 此守卫原子提交保留值及其修订号。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if guard.workbench.selected_conversation_id.as_deref() != Some(&conversation_id) {
            return Ok(());
        }
        // Runtime identity and capabilities are the existing authoritative projection inputs.
        // 运行时身份与能力是现有权威投影输入。
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        // Select the runtime row that owns these API facts.
        // 选择拥有这些 API 事实的运行时行。
        let runtime_state = workspace.session_runtime_state.get(&session_id);
        // Resolve capabilities without starting a new Provider request.
        // 解析能力时不启动新的 Provider 请求。
        let capabilities = session_public_ai_model_capabilities(
            guard
                .agent_runtime
                .as_ref()
                .map(|runtime| runtime.public_ai_runtime.as_ref()),
            session_id,
            runtime_state,
        );
        // The session's chosen compaction window is the denominator once no live budget remains.
        // 没有实时预算时，以会话选定的压缩窗口作为分母。
        let compression_window_limit = context_compaction_window_limit_from_configuration(
            runtime_state,
            guard
                .agent_runtime
                .as_ref()
                .and_then(|runtime| runtime.public_ai_runtime.configuration_snapshot().ok())
                .as_ref(),
        );
        // Reuse the canonical calibrated projection instead of maintaining a second token counter.
        // 复用规范校准投影，避免维护第二份 Token 计数器。
        let context_usage = context_usage_snapshot_from_runtime_state(
            session_id,
            runtime_state,
            facts.as_ref(),
            &guard.live_context_budgets,
            capabilities.as_ref(),
            compression_window_limit,
        );
        if guard.workbench.context_usage == context_usage {
            return Ok(());
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        guard.workbench.revision = guard.workbench_generation;
        guard.workbench.context_usage = context_usage.clone();
        WorkbenchDeltaEnvelope {
            generation: guard.workbench_generation,
            delta: WorkbenchDelta::ContextUsageChanged(ContextUsageChangedDelta { context_usage }),
        }
    };
    emit_service_event_with_trace(
        app,
        "workbench_api_usage_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(envelope)),
    );
    Ok(())
}

/// Finds the immutable V1 API-start fact paired with one terminal stats event.
/// 查找与一条终态统计事件配对的不可变 V1 API 起始事实。
fn matching_v1_api_started<'a>(
    events: &'a [ConversationEvent],
    stats: &ApiStatsEvent,
) -> Option<&'a ApiStartedEvent> {
    events.iter().find_map(|event| match &event.event {
        EventBody::ApiStarted(started) if started.api_call_id == stats.api_call_id => Some(started),
        _ => None,
    })
}

/// Checks whether one persisted request belongs to the current active request route.
/// 检查一条持久化请求是否属于当前生效请求路由。
fn v1_request_matches_active_route(
    started: &ApiStartedEvent,
    active: &vulcan_session_core::v1::config::ActiveRequestConfig,
) -> bool {
    active.provider.as_deref() == Some(started.request.provider.as_str())
        && started.request.connection_id == active.connection_id
        && active.model.as_deref() == Some(started.request.model.as_str())
        && started.request.thinking_level == active.thinking_level
        && started.request.temperature == active.temperature
}

/// Builds the selected-session context usage orb snapshot.
/// 构建当前选中会话的上下文占用圆盘快照。
pub(crate) fn context_usage_snapshot_from_runtime_state(
    session_id: SessionId,
    runtime_state: Option<&SessionRuntimeStateEntry>,
    v1_usage_facts: Option<&SessionContextUsageFacts>,
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
    model_capabilities: Option<&vulcan_ai_contract::ModelCapabilities>,
    compression_window_limit: Option<usize>,
) -> Option<ContextUsageSnapshot> {
    let runtime_state = runtime_state?;
    // providerUsage gates the surface on a completed usable Provider observation, not on a draft or optimistic user message.
    // providerUsage 依据已完成且可用的 Provider 观测控制界面，而不是草稿或乐观用户消息。
    let v1_usage_facts = v1_usage_facts?;
    // The UI reports the latest completed API, so preparation estimates must be calibrated on every projection.
    // 界面展示最近完成的 API，因此每次投影都必须使用实测用量校准准备阶段估算。
    let mut display_context_budget;
    let context_budget = if let Some(context_budget) = live_context_budgets.get(&session_id) {
        display_context_budget = Some(context_budget.clone());
        if let Some(budget) = display_context_budget.as_mut()
            && let Some(usage) = v1_usage_to_core(&v1_usage_facts.latest_anchor.usage)
        {
            apply_observed_usage_to_context_budget(budget, &usage);
            budget.accounting_source =
                vulcan_context::ContextBudgetAccountingSource::ProviderReported;
            budget.accounting_warning_code = None;
            budget.provider_anchor_call_id =
                Some(v1_usage_facts.latest_anchor.api_call_id.value().to_string());
        }
        display_context_budget.as_ref()
    } else {
        display_context_budget = historical_context_budget_snapshot_from_runtime_state(
            runtime_state,
            v1_usage_facts,
            model_capabilities,
            compression_window_limit,
        );
        display_context_budget.as_ref()
    };
    let mut tooltip_rows = Vec::new();
    if let Some(context_budget) = context_budget {
        tooltip_rows.push(format!(
            "Input usage: {:.0}% · {}",
            context_budget.usage_percent(),
            context_budget.severity.display_label(),
        ));
        tooltip_rows.push(format!("Input window: {}", context_budget.usage_label()));
        tooltip_rows.push(format!(
            "Budget source: {}",
            context_budget.accounting_source.display_label()
        ));
        if let Some(warning_code) = context_budget.accounting_warning_code.as_deref() {
            tooltip_rows.push(format!(
                "Usage note: the current value is not exact provider usage ({warning_code})."
            ));
        }
        tooltip_rows.extend(context_budget_breakdown_rows(context_budget));
        tooltip_rows.push(format!(
            "Reserved: output {} · reasoning {} · compaction {}",
            context_budget.reserved_output_tokens,
            context_budget.reserved_reasoning_tokens,
            context_budget.reserved_compaction_tokens
        ));
        if !context_budget.baseline_matches_active_runtime {
            let reason = context_budget
                .baseline_mismatch_reason
                .as_deref()
                .unwrap_or("The current runtime context disagrees with the compaction anchor");
            tooltip_rows.push(format!(
                "Recovery: baseline mismatch ({reason}); fell back to the full context."
            ));
        }
        if let Some(usage) = context_budget.observed_usage.as_ref() {
            tooltip_rows.push(format!("Latest API: {}", provider_usage_label(usage)));
        }
    } else {
        tooltip_rows.push(String::from(
            "Request breakdown: no usage history yet; the next request shows a character estimate first",
        ));
    }

    tooltip_rows.push(if v1_usage_facts.has_compaction {
        String::from("Compaction state: using the summarized history")
    } else {
        String::from("Compaction state: not triggered yet")
    });

    let used_tokens = context_budget
        .map(|budget| budget.active_context_tokens)
        .unwrap_or_default();
    let usage_basis_points = context_budget
        .filter(|budget| budget.effective_context_window.is_some())
        .map(|budget| context_usage_basis_points(budget.usage_ratio))
        .unwrap_or_default();
    Some(ContextUsageSnapshot {
        percent: context_budget
            .map(|budget| budget.usage_percent().round() as u8)
            .unwrap_or(0),
        usage_basis_points,
        used_tokens,
        max_tokens: context_budget.and_then(|budget| budget.effective_context_window),
        cache_hit_basis_points: context_usage_cache_hit_basis_points(v1_usage_facts),
        breakdown: context_usage_breakdown(v1_usage_facts, used_tokens),
        severity: context_budget
            .map(context_budget_severity_key)
            .unwrap_or_else(|| String::from("unknown")),
        baseline_matches_active_runtime: context_budget
            .map(|budget| budget.baseline_matches_active_runtime)
            .unwrap_or(true),
        tooltip_rows,
    })
}

/// Converts a clamped context ratio into integer basis points.
/// 将已钳制的上下文比例转换为整数万分比。
fn context_usage_basis_points(usage_ratio: f32) -> u16 {
    (usage_ratio * 10_000.0).round().clamp(0.0, 10_000.0) as u16
}

/// Calculates cumulative active-branch cache-hit rate from Provider facts only.
/// 仅根据 Provider 事实计算活动分支累计缓存命中率。
fn context_usage_cache_hit_basis_points(facts: &SessionContextUsageFacts) -> Option<u16> {
    let usage = aggregated_v1_usage_to_core(&facts.cumulative_usage)?;
    let cached_input_tokens = usage.cached_input_tokens?;
    let input_tokens = vulcan_context::observed_input_tokens_from_usage(Some(&usage))?;
    if input_tokens == 0 {
        return Some(0);
    }
    let basis_points = (cached_input_tokens.min(input_tokens) as u128)
        .saturating_mul(10_000)
        .saturating_div(input_tokens as u128);
    u16::try_from(basis_points).ok()
}

/// Builds calibrated context rows from the latest active request measurement.
/// 根据最近一次活动请求测量构建校准后的上下文分类行。
fn context_usage_breakdown(
    facts: &SessionContextUsageFacts,
    used_tokens: usize,
) -> Vec<ContextUsageBreakdownItem> {
    let breakdown = &facts.latest_request_measurement.context_breakdown;
    context_usage_breakdown_items(breakdown, used_tokens)
}

/// Converts one persisted request measurement into ordered calibrated display rows.
/// 将一份持久化请求测量转换为有序校准展示行。
fn context_usage_breakdown_items(
    breakdown: &ProviderRequestContextBreakdown,
    used_tokens: usize,
) -> Vec<ContextUsageBreakdownItem> {
    let Ok(message_units) = usize::try_from(breakdown.message_units) else {
        return Vec::new();
    };
    let Ok(system_tool_units) = usize::try_from(breakdown.system_tool_units) else {
        return Vec::new();
    };
    let Ok(mcp_tool_units) = usize::try_from(breakdown.mcp_tool_units) else {
        return Vec::new();
    };
    let Ok(skill_units) = usize::try_from(breakdown.skill_units) else {
        return Vec::new();
    };
    let Ok(system_prompt_units) = usize::try_from(breakdown.system_prompt_units) else {
        return Vec::new();
    };
    let Ok(other_units) = usize::try_from(breakdown.other_units) else {
        return Vec::new();
    };
    let categories = [
        ("messages", "Messages", message_units),
        ("system-tools", "System tools", system_tool_units),
        ("mcp-tools", "MCP tools", mcp_tool_units),
        ("other", "Other", other_units),
        ("skills", "Skills", skill_units),
        ("system-prompt", "System prompt", system_prompt_units),
    ];
    let non_zero_categories = categories
        .into_iter()
        .filter(|(_, _, units)| *units > 0)
        .collect::<Vec<_>>();
    let units = non_zero_categories
        .iter()
        .map(|(_, _, units)| *units)
        .collect::<Vec<_>>();
    let token_shares = proportional_integer_shares(used_tokens, &units);
    let basis_point_shares = proportional_integer_shares(10_000, &units);
    non_zero_categories
        .into_iter()
        .enumerate()
        .map(|(index, (kind, label, _))| ContextUsageBreakdownItem {
            kind: kind.to_string(),
            label: label.to_string(),
            tokens: token_shares[index],
            basis_points: u16::try_from(basis_point_shares[index]).unwrap_or_default(),
        })
        .collect()
}

/// Distributes one integer total by weights while preserving the exact total.
/// 按权重分配整数总量，同时保持分配结果精确等于总量。
fn proportional_integer_shares(total: usize, weights: &[usize]) -> Vec<usize> {
    let total_weight = weights
        .iter()
        .fold(0_u128, |sum, weight| sum.saturating_add(*weight as u128));
    if total_weight == 0 {
        return vec![0; weights.len()];
    }
    let mut shares = weights
        .iter()
        .map(|weight| {
            (total as u128)
                .saturating_mul(*weight as u128)
                .saturating_div(total_weight) as usize
        })
        .collect::<Vec<_>>();
    let mut remainders = weights
        .iter()
        .enumerate()
        .map(|(index, weight)| {
            (
                index,
                (total as u128).saturating_mul(*weight as u128) % total_weight,
            )
        })
        .collect::<Vec<_>>();
    remainders.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    let assigned = shares
        .iter()
        .fold(0_usize, |sum, share| sum.saturating_add(*share));
    for (index, _) in remainders.into_iter().take(total.saturating_sub(assigned)) {
        shares[index] = shares[index].saturating_add(1);
    }
    shares
}

/// Returns the current context total reported by one Provider response.
/// 返回一份 Provider 响应报告的当前上下文总量。
///
/// # Parameters
/// 参数
/// `usage` supplies the observed total or output dimensions; `input_tokens` is the
/// already validated input dimension used when the Provider omitted its total.
/// `usage` 提供实测总量或输出维度；Provider 未提供总量时，`input_tokens` 是已校验的输入维度。
///
/// # Returns
/// 返回值
/// The total context following the response without recounting reasoning inside output.
/// 返回响应完成后的上下文总量，不重复计入已包含在输出中的思考量。
pub(crate) fn provider_context_tokens_from_usage(
    usage: &vulcan_core::types::ProviderUsageSnapshot,
    input_tokens: usize,
) -> usize {
    // Public usage profiles include reasoning in output_tokens. When output is absent,
    // reasoning_tokens still supplies a known lower bound for the generated side.
    // 公共用量档案的 output_tokens 已包含思考量；输出缺失时，reasoning_tokens
    // 仍可作为生成侧已知的下界。
    usage.total_tokens.unwrap_or_else(|| {
        input_tokens.saturating_add(
            usage
                .output_tokens
                .unwrap_or_default()
                .max(usage.reasoning_tokens.unwrap_or_default()),
        )
    })
}

/// Builds one historical context-budget snapshot from the last persisted provider usage.
/// 根据最近一次持久化 Provider 用量构建历史上下文预算快照。
///
/// # Parameters
/// 参数
/// `runtime_state` is the selected session runtime-state entry.
/// `runtime_state` 是当前选中会话的运行态条目。
/// `model_capabilities` contains the already resolved public model limits.
/// `model_capabilities` 包含已经解析完成的公共模型限制。
///
/// # Returns
/// 返回值
/// A best-effort context budget snapshot when historical usage exists.
/// 当存在历史用量时返回尽力而为的上下文预算快照。
pub(crate) fn historical_context_budget_snapshot_from_runtime_state(
    runtime_state: &SessionRuntimeStateEntry,
    v1_usage_facts: &SessionContextUsageFacts,
    model_capabilities: Option<&vulcan_ai_contract::ModelCapabilities>,
    compression_window_limit: Option<usize>,
) -> Option<ContextBudgetSnapshot> {
    let observed_usage = v1_usage_to_core(&v1_usage_facts.latest_anchor.usage)?;
    let provider_id = runtime_state.active_provider_id.as_deref()?;
    let model_id = runtime_state.active_model_id.as_deref()?;
    let context_limit = model_capabilities
        .and_then(|capabilities| capabilities.context_limit)
        .and_then(|limit| usize::try_from(limit).ok());
    let window_policy = runtime_state
        .active_context_window_policy
        .unwrap_or_default();
    let planning_context_limit = compression_window_limit.or(context_limit);
    let effective_context_window = planning_context_limit.map(|limit| {
        limit
            .saturating_mul(usize::from(window_policy.effective_context_window_percent))
            .saturating_div(100)
    });
    // Historical reconstruction preserves the real model limit for Provider admission; the
    // configured percentage remains scoped to the planning window above.
    // 历史重建为 Provider 准入保留模型真实上限；配置百分比仍只作用于上方规划窗口。
    let provider_request_hard_limit = context_limit;
    let auto_compact_limit = context_limit.map(|raw_window| {
        // compressionWindow is session-scoped while rawWindow remains the effective hard model window.
        // compressionWindow 只作用于会话压缩，而 rawWindow 仍是模型有效硬窗口。
        let compression_window = compression_window_limit.unwrap_or(raw_window);
        let percentage_limit = compression_window
            .saturating_mul(usize::from(window_policy.auto_compact_percent))
            .saturating_div(100)
            .max(1);
        let compact_line = window_policy
            .auto_compact_token_limit
            .and_then(|limit| usize::try_from(limit).ok())
            .map(|limit| limit.min(percentage_limit))
            .unwrap_or(percentage_limit);
        compact_line
            .saturating_add(window_policy.fallback_buffer_tokens)
            .min(effective_context_window.unwrap_or(raw_window))
    });
    let input_tokens = vulcan_context::observed_input_tokens_from_usage(Some(&observed_usage))
        .or(observed_usage.total_tokens)
        .unwrap_or_default();
    // The latest response belongs to the next request context; use the same Provider total
    // as a live budget so a restored session cannot undercount a model-switch boundary.
    // 最近一次响应属于下一次请求上下文；与实时预算使用同一 Provider 总量，
    // 避免恢复后的会话低估模型切换边界。
    let active_context_tokens = provider_context_tokens_from_usage(&observed_usage, input_tokens);
    let auto_compact_context_tokens = match window_policy.auto_compact_scope {
        vulcan_context::ContextWindowUsageScope::Total => active_context_tokens,
        vulcan_context::ContextWindowUsageScope::BodyAfterPrefix => active_context_tokens
            .saturating_sub(
                runtime_state
                    .auto_compact_prefill_input_tokens
                    .unwrap_or(active_context_tokens),
            ),
    };
    let mut usage_ratio = effective_context_window
        .map(|limit| active_context_tokens as f32 / limit.max(1) as f32)
        .unwrap_or(0.0);
    if effective_context_window.is_none() && active_context_tokens > 0 {
        usage_ratio = 0.01;
    }

    Some(ContextBudgetSnapshot {
        fingerprint: runtime_state
            .live_context_budget_fingerprint
            .clone()
            .unwrap_or_else(|| format!("historical:{provider_id}:{model_id}")),
        context_limit,
        effective_input_limit: effective_context_window,
        effective_context_window,
        provider_request_hard_limit,
        auto_compact_limit,
        auto_compact_scope: window_policy.auto_compact_scope,
        auto_compact_prefill_input_tokens: runtime_state.auto_compact_prefill_input_tokens,
        fallback_buffer_tokens: window_policy.fallback_buffer_tokens,
        reserved_output_tokens: model_capabilities
            .and_then(|capabilities| capabilities.output_limit)
            .and_then(|limit| usize::try_from(limit).ok())
            .unwrap_or_default(),
        reserved_reasoning_tokens: 0,
        reserved_compaction_tokens: 0,
        compaction_auto_enabled: false,
        baseline_matches_active_runtime: true,
        baseline_budget_fingerprint_matches: true,
        baseline_assembly_fingerprint_matches: true,
        baseline_mismatch_reason: None,
        estimated_system_tokens: 0,
        estimated_history_tokens: input_tokens,
        estimated_prompt_tokens: 0,
        estimated_tool_tokens: 0,
        estimated_input_tokens: input_tokens,
        active_context_tokens,
        auto_compact_context_tokens,
        estimated_total_tokens: active_context_tokens,
        usage_ratio,
        observed_usage: Some(observed_usage),
        accounting_source: vulcan_context::ContextBudgetAccountingSource::ProviderReported,
        accounting_warning_code: None,
        provider_anchor_call_id: Some(v1_usage_facts.latest_anchor.api_call_id.value().to_string()),
        severity: effective_context_window
            .zip(auto_compact_limit)
            .map(|(hard_limit, auto_limit)| {
                vulcan_context::budget::context_budget_severity_for_limits_with_compaction_context(
                    active_context_tokens,
                    Some(hard_limit),
                    Some(auto_limit),
                    auto_compact_context_tokens,
                )
            })
            .unwrap_or_else(|| historical_context_budget_severity(usage_ratio)),
    })
}

/// Resolves one session node id to its current configured token value without persisting the number.
/// 将一个会话节点标识解析为当前配置的 Token 值，且不持久化该数值。
pub(crate) fn context_compaction_window_limit_from_configuration(
    runtime_state: Option<&SessionRuntimeStateEntry>,
    configuration: Option<&vulcan_provider_adapters::PersistedAiConfiguration>,
) -> Option<usize> {
    let node_id = runtime_state?.context_compaction_window_node_id?;
    let tokens = configuration?
        .context_compaction_window_nodes
        .tokens_for(node_id)
        .ok()?;
    usize::try_from(tokens).ok()
}

/// Resolves one historical usage ratio into the shared context-budget severity buckets.
/// 将历史占用比例解析为共享上下文预算严重级别。
///
/// # Parameters
/// 参数
/// `usage_ratio` is the input-token usage ratio against the effective window.
/// `usage_ratio` 是输入 token 相对有效窗口的占用比例。
///
/// # Returns
/// 返回值
/// The broad context-budget severity bucket.
/// 返回粗粒度上下文预算严重级别。
pub(crate) fn historical_context_budget_severity(usage_ratio: f32) -> ContextBudgetSeverity {
    vulcan_context::budget::context_budget_severity_for_ratio(usage_ratio)
}

/// Builds the context-budget token breakdown rows used by the tooltip.
/// 构建提示浮层使用的上下文预算 Token 拆分行。
pub(crate) fn context_budget_breakdown_rows(context_budget: &ContextBudgetSnapshot) -> Vec<String> {
    let raw_parts = [
        ("System", context_budget.estimated_system_tokens),
        ("History", context_budget.estimated_history_tokens),
        ("Current", context_budget.estimated_prompt_tokens),
        ("Tools", context_budget.estimated_tool_tokens),
    ];
    let raw_total = raw_parts
        .iter()
        .fold(0_usize, |total, (_, value)| total.saturating_add(*value));
    let prediction_row = format!(
        "Estimated breakdown (character estimate): {}",
        context_budget_token_breakdown_text(&raw_parts, None)
    );

    let Some(observed_input) =
        vulcan_context::observed_input_tokens_from_usage(context_budget.observed_usage.as_ref())
    else {
        return vec![prediction_row];
    };
    if raw_total == 0 {
        return vec![prediction_row];
    }
    let calibrated =
        context_budget_token_breakdown_text(&raw_parts, Some((observed_input, raw_total)));
    vec![
        String::from(
            "Note: API measurements are authoritative; the breakdown is calibrated to the measured input ratio.",
        ),
        prediction_row,
        format!("Calibrated breakdown (measured): {calibrated}"),
    ]
}

/// Builds one slash-separated context-budget token breakdown row.
/// 构建单条斜杠分隔的上下文预算 Token 拆分行。
fn context_budget_token_breakdown_text(
    raw_parts: &[(&str, usize)],
    calibration: Option<(usize, usize)>,
) -> String {
    let mut row = String::new();
    for (label, value) in raw_parts {
        if !row.is_empty() {
            row.push_str(" / ");
        }
        let display_value = if let Some((observed_input, raw_total)) = calibration {
            let scaled = ((*value as f64 / raw_total as f64) * observed_input as f64).round();
            scaled as usize
        } else {
            *value
        };
        row.push_str(label);
        row.push(' ');
        row.push_str(&display_value.to_string());
    }
    row
}

/// Returns the stable context-budget severity key consumed by WebUI CSS.
/// 返回 WebUI CSS 消费的稳定上下文预算严重级别键。
pub(crate) fn context_budget_severity_key(context_budget: &ContextBudgetSnapshot) -> String {
    if !context_budget.baseline_matches_active_runtime {
        return String::from("baseline-mismatch");
    }
    context_budget.severity.stable_key().to_string()
}

/// Converts one V1 usage snapshot into the existing context-budget value type without estimates.
/// 将一份 V1 用量快照无估算地转换为现有上下文预算值类型。
fn v1_usage_to_core(usage: &UsageSnapshot) -> Option<vulcan_core::types::ProviderUsageSnapshot> {
    Some(vulcan_core::types::ProviderUsageSnapshot {
        update_kind: None,
        input_tokens: checked_v1_token_dimension(usage.input_tokens)?,
        uncached_input_tokens: checked_v1_token_dimension(usage.uncached_input_tokens)?,
        output_tokens: checked_v1_token_dimension(usage.output_tokens)?,
        reasoning_tokens: checked_v1_token_dimension(usage.reasoning_tokens)?,
        total_tokens: checked_v1_token_dimension(usage.total_tokens)?,
        cached_input_tokens: checked_v1_token_dimension(usage.cache_read_tokens)?,
        cache_creation_input_tokens: checked_v1_token_dimension(usage.cache_write_tokens)?,
        audio_input_tokens: checked_v1_token_dimension(usage.audio_input_tokens)?,
        audio_output_tokens: checked_v1_token_dimension(usage.audio_output_tokens)?,
        extraction_profile_id: None,
        extraction_profile_revision: None,
        source_paths: BTreeMap::new(),
        validation_flags: Vec::new(),
        response_model: None,
        response_id: None,
        billing_details: Vec::new(),
    })
}

/// Converts one poison-preserving V1 aggregate into the existing context-budget value type.
/// 将一份保留未知传播语义的 V1 聚合转换为现有上下文预算值类型。
fn aggregated_v1_usage_to_core(
    usage: &AggregatedUsage,
) -> Option<vulcan_core::types::ProviderUsageSnapshot> {
    v1_usage_to_core(&UsageSnapshot {
        input_tokens: usage.input_tokens,
        uncached_input_tokens: usage.uncached_input_tokens,
        output_tokens: usage.output_tokens,
        reasoning_tokens: usage.reasoning_tokens,
        total_tokens: usage.total_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        cache_write_tokens: usage.cache_write_tokens,
        audio_input_tokens: usage.audio_input_tokens,
        audio_output_tokens: usage.audio_output_tokens,
        billing_tier: None,
    })
}

/// Converts one optional JS-safe V1 token count into the host release width.
/// 将一个可选 JS 安全 V1 Token 数转换为宿主 Release 位宽。
fn checked_v1_token_dimension(value: Option<u64>) -> Option<Option<usize>> {
    match value {
        Some(value) => usize::try_from(value).ok().map(Some),
        None => Some(None),
    }
}

/// Formats one provider usage row for the context usage tooltip.
/// 为上下文占用提示浮层格式化单条 Provider 用量行。
pub(crate) fn provider_usage_label(usage: &vulcan_core::types::ProviderUsageSnapshot) -> String {
    let mut parts = Vec::new();
    if let Some(input) = usage.input_tokens {
        parts.push(format!("Input {input}"));
    }
    if let Some(output) = usage.output_tokens {
        parts.push(format!("Output {output}"));
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        parts.push(format!("Reasoning {reasoning}"));
    }
    if let Some(total) = usage.total_tokens {
        parts.push(format!("Total {total}"));
    }
    if let Some(cached) = usage.cached_input_tokens {
        parts.push(format!("Cache hits {cached}"));
    }
    if let Some(created) = usage.cache_creation_input_tokens {
        parts.push(format!("Cache writes {created}"));
    }
    if parts.is_empty() {
        String::from("No token data available")
    } else {
        parts.join(" / ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies restored and live budgets include the last response before model switching.
    /// 验证恢复与实时预算在切换模型前都计入最近一次响应。
    #[test]
    fn provider_context_total_includes_the_latest_response() {
        let usage = vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(249_000),
            output_tokens: Some(3_000),
            ..Default::default()
        };
        assert_eq!(provider_context_tokens_from_usage(&usage, 249_000), 252_000);
        let usage_with_total = vulcan_core::types::ProviderUsageSnapshot {
            total_tokens: Some(251_000),
            ..usage
        };
        assert_eq!(
            provider_context_tokens_from_usage(&usage_with_total, 249_000),
            251_000
        );
    }

    /// Verifies reasoning already included in output cannot falsely trip a model-switch limit.
    /// 验证已包含在输出中的思考量不会错误触发模型切换限制。
    #[test]
    fn provider_context_total_does_not_double_count_reasoning() {
        let usage = vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(240_000),
            output_tokens: Some(8_000),
            reasoning_tokens: Some(6_000),
            ..Default::default()
        };
        assert_eq!(provider_context_tokens_from_usage(&usage, 240_000), 248_000);

        let usage_without_output = vulcan_core::types::ProviderUsageSnapshot {
            output_tokens: None,
            ..usage
        };
        assert_eq!(
            provider_context_tokens_from_usage(&usage_without_output, 240_000),
            246_000
        );
    }

    /// Builds one context budget snapshot for tooltip row tests.
    /// 为提示浮层行测试构建单个上下文预算快照。
    fn test_context_budget_snapshot() -> ContextBudgetSnapshot {
        ContextBudgetSnapshot {
            estimated_system_tokens: 10,
            estimated_history_tokens: 30,
            estimated_prompt_tokens: 20,
            estimated_tool_tokens: 40,
            estimated_input_tokens: 100,
            observed_usage: Some(vulcan_core::types::ProviderUsageSnapshot {
                input_tokens: Some(200),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// Verifies context budget breakdown rows include raw and observed-calibrated text.
    /// 验证上下文预算拆分行包含原始文本与实测校准文本。
    #[test]
    fn context_budget_breakdown_rows_include_calibrated_text() {
        let context_budget = test_context_budget_snapshot();

        assert_eq!(
            context_budget_breakdown_rows(&context_budget),
            vec![
                String::from(
                    "Note: API measurements are authoritative; the breakdown is calibrated to the measured input ratio."
                ),
                String::from(
                    "Estimated breakdown (character estimate): System 10 / History 30 / Current 20 / Tools 40"
                ),
                String::from(
                    "Calibrated breakdown (measured): System 20 / History 60 / Current 40 / Tools 80"
                ),
            ]
        );
    }

    /// Verifies context budget breakdown rows keep only prediction text without observed input.
    /// 验证缺少实测输入时上下文预算拆分行只保留预测文本。
    #[test]
    fn context_budget_breakdown_rows_skip_calibration_without_observed_input() {
        let mut context_budget = test_context_budget_snapshot();
        context_budget.observed_usage = None;

        assert_eq!(
            context_budget_breakdown_rows(&context_budget),
            vec![String::from(
                "Estimated breakdown (character estimate): System 10 / History 30 / Current 20 / Tools 40"
            )]
        );
    }

    /// Verifies a draft or first optimistic message cannot reveal the usage control before Provider usage exists.
    /// 验证草稿或首条乐观消息在 Provider 用量产生前无法显示占用控件。
    #[test]
    fn context_usage_snapshot_stays_hidden_before_first_provider_usage() {
        let session_id = SessionId::new_v4();
        let runtime_state = SessionRuntimeStateEntry::default();
        let live_context_budgets = BTreeMap::from([(session_id, test_context_budget_snapshot())]);

        assert!(
            context_usage_snapshot_from_runtime_state(
                session_id,
                Some(&runtime_state),
                None,
                &live_context_budgets,
                None,
                None,
            )
            .is_none()
        );
    }

    /// Verifies terminal Provider facts replace stale live estimates after growth and compaction, including reload parity.
    /// 验证终态 Provider 事实在增长和压缩后替换过期实时估算，并保持刷新前后一致。
    #[test]
    fn context_usage_recalibrates_cached_budget_after_compaction() {
        // session_id and runtime_state bind both live and restored projections to the same configured route.
        // session_id 与 runtime_state 将实时和恢复投影绑定到同一配置路由。
        let session_id = SessionId::new_v4();
        let runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some("provider".into()),
            active_model_id: Some("model".into()),
            ..Default::default()
        };
        // cached_budget reproduces a pre-compaction estimate that formerly shadowed every later API result.
        // cached_budget 复现此前会覆盖所有后续 API 结果的压缩前估算。
        let cached_budget = ContextBudgetSnapshot {
            active_context_tokens: 682_800,
            effective_context_window: Some(243_200),
            auto_compact_limit: Some(230_400),
            ..test_context_budget_snapshot()
        };
        let live_budgets = BTreeMap::from([(session_id, cached_budget)]);
        for input_tokens in [700_000, 32_709] {
            // evidence and facts contain only canonical API measurement and terminal usage, never cumulative cost.
            // evidence 与 facts 仅包含规范 API 测量和终态用量，绝不使用累计消耗。
            let evidence =
                crate::session_runtime_registry_v1::test_provider_request_budget_evidence();
            let facts = SessionContextUsageFacts {
                latest_request_measurement: evidence.request_text_measurement.clone(),
                latest_anchor: ProviderUsageBudgetAnchor {
                    api_call_id: vulcan_session_core::v1::ApiCallId::try_new(1)
                        .expect("valid API id"),
                    projection: vulcan_session_core::v1::event::ProviderProjectionProof {
                        segment_generation: 1,
                        document_revision: 1,
                        retained_from_seq: vulcan_session_core::v1::Seq::try_new(1)
                            .expect("valid seq"),
                        through_seq: vulcan_session_core::v1::Seq::try_new(1).expect("valid seq"),
                        fingerprint_sha256: "test-projection".into(),
                    },
                    evidence,
                    usage: UsageSnapshot {
                        input_tokens: Some(input_tokens),
                        output_tokens: Some(294),
                        ..Default::default()
                    },
                },
                cumulative_usage: AggregatedUsage::aggregate(std::iter::empty::<&UsageSnapshot>()),
                has_compaction: true,
            };
            // project uses identical canonical facts with and without the process-local cache.
            // project 分别使用存在和不存在进程内缓存的相同规范事实。
            let project = |budgets| {
                context_usage_snapshot_from_runtime_state(
                    session_id,
                    Some(&runtime_state),
                    Some(&facts),
                    budgets,
                    None,
                    Some(256_000),
                )
                .expect("terminal usage is visible")
            };
            let live = project(&live_budgets);
            let restored = project(&BTreeMap::new());
            assert_eq!(live.used_tokens, input_tokens as usize + 294);
            assert_eq!(live.used_tokens, restored.used_tokens);
            assert_eq!(live.max_tokens, restored.max_tokens);
            assert_eq!(live.usage_basis_points, restored.usage_basis_points);
            assert!(
                live.tooltip_rows
                    .iter()
                    .all(|row| !row.contains("not exact provider usage"))
            );
        }
        assert_eq!(live_budgets[&session_id].active_context_tokens, 682_800);
    }

    /// Verifies detailed rows preserve the Provider-calibrated token and percentage totals.
    /// 验证明细行保持 Provider 校准后的 Token 与百分比总量。
    #[test]
    fn context_usage_breakdown_items_preserve_calibrated_totals() {
        let breakdown = ProviderRequestContextBreakdown {
            message_units: 70,
            system_tool_units: 10,
            mcp_tool_units: 8,
            skill_units: 4,
            system_prompt_units: 3,
            other_units: 5,
        };

        let items = context_usage_breakdown_items(&breakdown, 12_345);

        assert_eq!(items.iter().map(|item| item.tokens).sum::<usize>(), 12_345);
        assert_eq!(
            items
                .iter()
                .map(|item| usize::from(item.basis_points))
                .sum::<usize>(),
            10_000
        );
        assert_eq!(
            items
                .iter()
                .map(|item| item.kind.as_str())
                .collect::<Vec<_>>(),
            vec![
                "messages",
                "system-tools",
                "mcp-tools",
                "other",
                "skills",
                "system-prompt",
            ]
        );
    }
}
