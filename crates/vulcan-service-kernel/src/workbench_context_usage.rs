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
    let historical_context_budget;
    let context_budget = if let Some(context_budget) = live_context_budgets.get(&session_id) {
        Some(context_budget)
    } else {
        historical_context_budget = historical_context_budget_snapshot_from_runtime_state(
            runtime_state,
            v1_usage_facts,
            model_capabilities,
            compression_window_limit,
        );
        historical_context_budget.as_ref()
    };
    let mut tooltip_rows = Vec::new();
    if let Some(context_budget) = context_budget {
        tooltip_rows.push(format!(
            "输入占用：{:.0}% · {}",
            context_budget.usage_percent(),
            context_budget.severity.display_label(),
        ));
        tooltip_rows.push(format!("输入窗口：{}", context_budget.usage_label()));
        tooltip_rows.push(format!(
            "预算来源：{}",
            context_budget.accounting_source.display_label()
        ));
        if let Some(warning_code) = context_budget.accounting_warning_code.as_deref() {
            tooltip_rows.push(format!(
                "用量提示：当前值不是精确 Provider 用量（{warning_code}）。"
            ));
        }
        tooltip_rows.extend(context_budget_breakdown_rows(context_budget));
        tooltip_rows.push(format!(
            "预留：输出 {} · 推理 {} · 压缩 {}",
            context_budget.reserved_output_tokens,
            context_budget.reserved_reasoning_tokens,
            context_budget.reserved_compaction_tokens
        ));
        if !context_budget.baseline_matches_active_runtime {
            let reason = context_budget
                .baseline_mismatch_reason
                .as_deref()
                .unwrap_or("当前运行时上下文与压缩锚点不一致");
            tooltip_rows.push(format!("恢复：基线失配（{reason}），已回退完整上下文。"));
        }
        if let Some(usage) = context_budget.observed_usage.as_ref() {
            tooltip_rows.push(format!("最近 API：{}", provider_usage_label(usage)));
        }
    } else {
        tooltip_rows.push(String::from(
            "请求拆分：暂无历史用量，下一次请求将先显示字符粗估",
        ));
    }

    tooltip_rows.push(if v1_usage_facts.has_compaction {
        String::from("压缩状态：已使用摘要前文")
    } else {
        String::from("压缩状态：尚未触发")
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
        tooltip_title: String::from("上下文容量"),
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
        ("messages", "消息", message_units),
        ("system-tools", "系统工具", system_tool_units),
        ("mcp-tools", "MCP 工具", mcp_tool_units),
        ("other", "其他", other_units),
        ("skills", "技能", skill_units),
        ("system-prompt", "系统提示词", system_prompt_units),
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
    // Historical UI percentages follow the session node; provider admission retains the model limit.
    // 历史界面百分比遵循会话节点；Provider 准入仍保留模型上限。
    let provider_request_hard_limit = context_limit.map(|limit| {
        limit
            .saturating_mul(usize::from(window_policy.effective_context_window_percent))
            .saturating_div(100)
    });
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
    let active_context_tokens = input_tokens;
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
        estimated_total_tokens: observed_usage.total_tokens.unwrap_or_else(|| {
            input_tokens
                .saturating_add(observed_usage.output_tokens.unwrap_or_default())
                .saturating_add(observed_usage.reasoning_tokens.unwrap_or_default())
        }),
        usage_ratio,
        observed_usage: Some(observed_usage.clone()),
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
        ("系统", context_budget.estimated_system_tokens),
        ("历史", context_budget.estimated_history_tokens),
        ("当前", context_budget.estimated_prompt_tokens),
        ("工具", context_budget.estimated_tool_tokens),
    ];
    let raw_total = raw_parts
        .iter()
        .fold(0_usize, |total, (_, value)| total.saturating_add(*value));
    let prediction_row = format!(
        "预测拆分（字符估算）：{}",
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
        String::from("说明：API实测为准；拆分已按实测输入比例校准。"),
        prediction_row,
        format!("校准拆分（实测）：{calibrated}"),
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
        parts.push(format!("输入 {input}"));
    }
    if let Some(output) = usage.output_tokens {
        parts.push(format!("输出 {output}"));
    }
    if let Some(reasoning) = usage.reasoning_tokens {
        parts.push(format!("推理 {reasoning}"));
    }
    if let Some(total) = usage.total_tokens {
        parts.push(format!("总计 {total}"));
    }
    if let Some(cached) = usage.cached_input_tokens {
        parts.push(format!("缓存命中 {cached}"));
    }
    if let Some(created) = usage.cache_creation_input_tokens {
        parts.push(format!("缓存写入 {created}"));
    }
    if parts.is_empty() {
        String::from("无可用 Token 数据")
    } else {
        parts.join(" / ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
                String::from("说明：API实测为准；拆分已按实测输入比例校准。"),
                String::from("预测拆分（字符估算）：系统 10 / 历史 30 / 当前 20 / 工具 40"),
                String::from("校准拆分（实测）：系统 20 / 历史 60 / 当前 40 / 工具 80"),
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
                "预测拆分（字符估算）：系统 10 / 历史 30 / 当前 20 / 工具 40"
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
