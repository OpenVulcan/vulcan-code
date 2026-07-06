//! workbench context usage helpers.
//! workbench context usage 辅助逻辑。

use super::*;

/// Builds the selected-session context usage orb snapshot.
/// 构建当前选中会话的上下文占用圆盘快照。
pub(crate) fn context_usage_snapshot_from_runtime_state(
    session_id: SessionId,
    runtime_state: Option<&SessionRuntimeStateEntry>,
    live_context_budgets: &BTreeMap<SessionId, ContextBudgetSnapshot>,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ContextUsageSnapshot> {
    let runtime_state = runtime_state?;
    let historical_context_budget;
    let context_budget = if let Some(context_budget) = live_context_budgets.get(&session_id) {
        Some(context_budget)
    } else {
        historical_context_budget = historical_context_budget_snapshot_from_runtime_state(
            runtime_state,
            provider_configuration,
            ai_tool_configuration,
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
        tooltip_rows.push(format!(
            "输入窗口（API实测/本地估算）：{}",
            context_budget.usage_label()
        ));
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
        tooltip_rows.push(String::from("请求拆分：暂无历史用量，等待预算引擎计算"));
    }

    tooltip_rows.push(if runtime_state.last_compaction_id.is_some() {
        String::from("压缩状态：已使用摘要前文")
    } else {
        String::from("压缩状态：尚未触发")
    });

    Some(ContextUsageSnapshot {
        percent: context_budget
            .map(|budget| budget.usage_percent().round() as u8)
            .unwrap_or(0),
        severity: context_budget
            .map(context_budget_severity_key)
            .unwrap_or_else(|| String::from("unknown")),
        baseline_matches_active_runtime: context_budget
            .map(|budget| budget.baseline_matches_active_runtime)
            .unwrap_or(true),
        tooltip_title: String::from("上下文占用"),
        tooltip_rows,
    })
}

/// Builds one historical context-budget snapshot from the last persisted provider usage.
/// 根据最近一次持久化 Provider 用量构建历史上下文预算快照。
///
/// # Parameters
/// 参数
/// `runtime_state` is the selected session runtime-state entry.
/// `runtime_state` 是当前选中会话的运行态条目。
/// `provider_configuration` is used to resolve the session model catalog entry.
/// `provider_configuration` 用于解析会话模型目录项。
/// `ai_tool_configuration` is used to resolve effective model capability overrides.
/// `ai_tool_configuration` 用于解析生效模型能力覆盖。
///
/// # Returns
/// 返回值
/// A best-effort context budget snapshot when historical usage exists.
/// 当存在历史用量时返回尽力而为的上下文预算快照。
pub(crate) fn historical_context_budget_snapshot_from_runtime_state(
    runtime_state: &SessionRuntimeStateEntry,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ContextBudgetSnapshot> {
    let observed_usage = runtime_state
        .last_successful_provider_usage
        .as_ref()
        .or(runtime_state.last_provider_usage.as_ref())
        .filter(|usage| usage.has_any_tokens())?;
    let provider_id = runtime_state.active_provider_id.as_deref()?;
    let model_id = runtime_state.active_model_id.as_deref()?;
    let capabilities = runtime_state_model_capabilities(
        runtime_state,
        provider_configuration,
        ai_tool_configuration,
    );
    let effective_input_limit = capabilities
        .as_ref()
        .and_then(|capabilities| capabilities.input_limit.or(capabilities.context_limit));
    let input_tokens = vulcan_context::observed_input_tokens_from_usage(Some(observed_usage))
        .or(observed_usage.total_tokens)
        .unwrap_or_default();
    let mut usage_ratio = effective_input_limit
        .map(|limit| input_tokens as f32 / limit.max(1) as f32)
        .unwrap_or(0.0);
    if effective_input_limit.is_none() && input_tokens > 0 {
        usage_ratio = 0.01;
    }

    Some(ContextBudgetSnapshot {
        fingerprint: runtime_state
            .live_context_budget_fingerprint
            .clone()
            .unwrap_or_else(|| format!("historical:{provider_id}:{model_id}")),
        context_limit: capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.context_limit),
        effective_input_limit,
        reserved_output_tokens: capabilities
            .as_ref()
            .and_then(|capabilities| capabilities.output_limit)
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
        estimated_total_tokens: observed_usage.total_tokens.unwrap_or_else(|| {
            input_tokens
                .saturating_add(observed_usage.output_tokens.unwrap_or_default())
                .saturating_add(observed_usage.reasoning_tokens.unwrap_or_default())
        }),
        usage_ratio,
        observed_usage: Some(observed_usage.clone()),
        severity: historical_context_budget_severity(usage_ratio),
    })
}

/// Resolves model capabilities for the model recorded by one session runtime state.
/// 为单个会话运行态记录的模型解析模型能力。
///
/// # Parameters
/// 参数
/// `runtime_state` supplies the provider and model identity.
/// `runtime_state` 提供供应商与模型身份。
/// `provider_configuration` supplies enabled model catalog data.
/// `provider_configuration` 提供已启用模型目录数据。
/// `ai_tool_configuration` supplies capability overrides.
/// `ai_tool_configuration` 提供能力覆盖配置。
///
/// # Returns
/// 返回值
/// Effective model capabilities when the provider can be resolved.
/// 当供应商可解析时返回生效模型能力。
pub(crate) fn runtime_state_model_capabilities(
    runtime_state: &SessionRuntimeStateEntry,
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Option<ResolvedModelCapabilities> {
    let provider_id = runtime_state.active_provider_id.as_ref()?;
    let model_id = runtime_state.active_model_id.as_ref()?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let model = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .find(|model| model.id == *model_id);
    Some(resolve_effective_model_capabilities(
        definition,
        model.as_ref(),
        ai_tool_configuration,
    ))
}

/// Resolves base model capabilities for the model recorded by one session runtime state.
/// 为单个会话运行态记录的模型解析基础模型能力。
///
/// # Parameters
/// 参数
/// `runtime_state` supplies the provider and model identity.
/// `runtime_state` 提供供应商与模型身份。
/// `provider_configuration` supplies enabled model catalog data.
/// `provider_configuration` 提供已启用模型目录数据。
///
/// # Returns
/// 返回值
/// Base model capabilities before user AI-extension routing overlays are applied.
/// 返回应用用户 AI 扩展路由覆盖之前的基础模型能力。
pub(crate) fn runtime_state_base_model_capabilities(
    runtime_state: &SessionRuntimeStateEntry,
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ResolvedModelCapabilities> {
    let provider_id = runtime_state.active_provider_id.as_ref()?;
    let model_id = runtime_state.active_model_id.as_ref()?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let model = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .find(|model| model.id == *model_id);
    Some(resolve_model_capabilities(definition, model.as_ref()))
}

/// Resolves base model capabilities for the globally selected provider model.
/// 为全局选中的供应商模型解析基础模型能力。
pub(crate) fn active_provider_configuration_model_capabilities(
    provider_configuration: &ProviderConfigurationFile,
) -> Option<ResolvedModelCapabilities> {
    let (provider_id, model_id) = provider_configuration
        .model
        .as_deref()
        .and_then(provider_config::parse_active_model_reference)?;
    let definition = provider_config::supported_provider_by_id(provider_id)?;
    let model = provider_config::effective_model_choices(provider_id, provider_configuration)
        .into_iter()
        .find(|model| model.id == model_id);
    Some(resolve_model_capabilities(definition, model.as_ref()))
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
    let raw_total: usize = raw_parts.iter().map(|(_, value)| *value).sum();
    let prediction_row = format!(
        "预测拆分（预算器）：{}",
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
                String::from("预测拆分（预算器）：系统 10 / 历史 30 / 当前 20 / 工具 40"),
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
                "预测拆分（预算器）：系统 10 / 历史 30 / 当前 20 / 工具 40"
            )]
        );
    }
}
