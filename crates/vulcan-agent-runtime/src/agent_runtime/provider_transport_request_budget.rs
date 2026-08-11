//! Provider request budget guard used immediately after request assembly and before network dispatch.
//! 在请求装配完成后、网络发送前使用的 Provider 请求预算守卫。

use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
use crate::agent_runtime::provider_transport_vulcan::VulcanPublicContractRequest;
use crate::context_budget::{
    ContextBudgetAccountingSource, ContextBudgetSnapshot, ContextCompactionPolicy,
    RequestContextCompactionBudgetSnapshotInput, build_request_context_compaction_budget_snapshot,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::{Arc, RwLock};
use vulcan_ai_contract::ModelCapabilities;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    ConversationPayload, CoreError as VulcanCoreError,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
};
use vulcan_workbench_model::{
    SessionProviderCallRecord, SessionProviderRequestContextBreakdown,
    SessionProviderRequestTextMeasurement,
};

/// Stable non-sensitive evidence derived from one exact canonical Provider request.
/// 从单个精确规范 Provider 请求派生的稳定非敏感证据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderRequestBudgetEvidence {
    /// Digest of the complete canonical request.
    /// 完整规范请求的摘要。
    pub(crate) request_assembly_fingerprint: String,
    /// Digest of the immutable provider/model/protocol route.
    /// 不可变 Provider/Model/协议路由摘要。
    pub(crate) model_route_fingerprint: String,
    /// Digest of the exact exposed tool schemas.
    /// 精确暴露工具 Schema 的摘要。
    pub(crate) toolset_fingerprint: String,
    /// Digest of system and layered prompt inputs.
    /// 系统提示与分层提示输入的摘要。
    pub(crate) system_prompt_fingerprint: String,
    /// Non-sensitive request size and history-prefix measurement.
    /// 非敏感请求尺寸与历史前缀测量。
    pub(crate) request_text_measurement: SessionProviderRequestTextMeasurement,
}

/// Maximum nested JSON container depth admitted into request evidence serialization.
/// 请求证据序列化允许接受的 JSON 容器最大嵌套深度。
const PROVIDER_REQUEST_EVIDENCE_MAX_JSON_DEPTH: usize = 64;

/// Maximum JSON nodes inspected across one request evidence value.
/// 单个请求证据值允许检查的 JSON 节点数上限。
const PROVIDER_REQUEST_EVIDENCE_MAX_JSON_NODES: usize = 1_000_000;

/// Validates every untrusted structured JSON value before recursive Serde serialization.
/// 在递归 Serde 序列化前校验所有不受信任的结构化 JSON 值。
///
/// `snapshot` is the exact assembled Provider request containing tool schemas and replayed tool arguments.
/// `snapshot` 是包含工具 Schema 与回放工具参数的精确已装配 Provider 请求。
/// Returns success only when every JSON subtree stays inside fixed depth and node limits.
/// 仅当每棵 JSON 子树均位于固定深度与节点限制内时返回成功。
fn validate_provider_request_json_depth(
    snapshot: &CoreProviderRequestSnapshot,
) -> Result<(), String> {
    for (index, tool) in snapshot.tool_schemas.iter().enumerate() {
        validate_bounded_provider_request_json(
            &tool.parameters_schema,
            format!("tool_schemas[{index}].parameters_schema").as_str(),
        )?;
    }
    if let Some(additional_params) = snapshot.additional_params.as_ref() {
        validate_bounded_provider_request_json(additional_params, "additional_params")?;
    }
    for (message_index, message) in snapshot.history.iter().enumerate() {
        match &message.payload {
            ConversationPayload::Assistant {
                provider_message_state,
                tool_calls,
                ..
            } => {
                if let Some(provider_message_state) = provider_message_state.as_ref() {
                    validate_bounded_provider_request_json(
                        provider_message_state,
                        format!("history[{message_index}].provider_message_state").as_str(),
                    )?;
                }
                for (tool_index, tool_call) in tool_calls.iter().enumerate() {
                    validate_bounded_provider_request_json(
                        &tool_call.arguments,
                        format!("history[{message_index}].tool_calls[{tool_index}].arguments")
                            .as_str(),
                    )?;
                }
            }
            ConversationPayload::AgentMessage {
                metadata: Some(metadata),
                ..
            } => validate_bounded_provider_request_json(
                metadata,
                format!("history[{message_index}].metadata").as_str(),
            )?,
            ConversationPayload::UserText { .. }
            | ConversationPayload::ToolResult { .. }
            | ConversationPayload::AgentMessage { metadata: None, .. } => {}
        }
    }
    Ok(())
}

/// Iteratively checks one JSON value so deeply nested provider data cannot overflow the Rust stack.
/// 以迭代方式检查一个 JSON 值，避免深层 Provider 数据使 Rust 栈溢出。
///
/// `value` is one untrusted request subtree and `label` identifies the rejected owner without content.
/// `value` 是一棵不受信任的请求子树，`label` 在不包含正文的情况下标识被拒绝的所有者。
/// Returns a controlled error when depth or total node count exceeds the fixed evidence bounds.
/// 深度或节点总数超过固定证据边界时返回受控错误。
fn validate_bounded_provider_request_json(
    value: &serde_json::Value,
    label: &str,
) -> Result<(), String> {
    let mut pending = vec![(value, 0usize)];
    let mut visited = 0usize;
    while let Some((current, depth)) = pending.pop() {
        visited = visited.saturating_add(1);
        if visited > PROVIDER_REQUEST_EVIDENCE_MAX_JSON_NODES {
            return Err(format!(
                "Provider request evidence `{label}` exceeds the {PROVIDER_REQUEST_EVIDENCE_MAX_JSON_NODES}-node limit."
            ));
        }
        if depth > PROVIDER_REQUEST_EVIDENCE_MAX_JSON_DEPTH {
            return Err(format!(
                "Provider request evidence `{label}` exceeds the {PROVIDER_REQUEST_EVIDENCE_MAX_JSON_DEPTH}-level JSON depth limit."
            ));
        }
        match current {
            serde_json::Value::Array(items) => {
                pending.extend(items.iter().map(|item| (item, depth.saturating_add(1))));
            }
            serde_json::Value::Object(object) => {
                pending.extend(object.values().map(|item| (item, depth.saturating_add(1))));
            }
            serde_json::Value::Null
            | serde_json::Value::Bool(_)
            | serde_json::Value::Number(_)
            | serde_json::Value::String(_) => {}
        }
    }
    Ok(())
}

/// Builds stable evidence without retaining prompt, response, header, or credential content.
/// 构建稳定证据且不保留提示词、响应、请求头或凭据正文。
pub(crate) fn build_provider_request_budget_evidence(
    snapshot: &CoreProviderRequestSnapshot,
    protocol_profile_id: &str,
    route_revision: &str,
) -> Result<ProviderRequestBudgetEvidence, String> {
    validate_provider_request_json_depth(snapshot)?;
    // distinct_prompt excludes the display-oriented prompt copy when canonical history already owns it.
    // 当规范历史已经持有提示词时，distinct_prompt 排除仅供展示使用的提示词副本。
    let distinct_prompt = (!snapshot.prompt_in_history).then_some(snapshot.prompt.as_str());
    // Request headers are intentionally excluded because they may contain credentials.
    // 请求头被刻意排除，因为其中可能包含凭据。
    let request_assembly_fingerprint = digest_serializable(&(
        snapshot.provider_id.as_str(),
        snapshot.model_id.as_str(),
        snapshot.system_prompt.as_str(),
        &snapshot.prompt_layers,
        distinct_prompt,
        &snapshot.history,
        snapshot.temperature.map(f64::to_bits),
        snapshot.max_turns,
        &snapshot.tool_schemas,
        &snapshot.additional_params,
        &snapshot.continuation,
    ))?;
    let model_route_fingerprint = digest_serializable(&(
        snapshot.provider_id.as_str(),
        snapshot.model_id.as_str(),
        protocol_profile_id,
        route_revision,
    ))?;
    let toolset_fingerprint = digest_serializable(&snapshot.tool_schemas)?;
    let system_prompt_fingerprint =
        digest_serializable(&(snapshot.system_prompt.as_str(), &snapshot.prompt_layers))?;
    let history_fingerprint = digest_serializable(&snapshot.history)?;
    let prefix_invariant_fingerprint = digest_serializable(&(
        snapshot.system_prompt.as_str(),
        &snapshot.prompt_layers,
        distinct_prompt,
        snapshot.temperature.map(f64::to_bits),
        snapshot.max_turns,
        &snapshot.tool_schemas,
        &snapshot.additional_params,
        &snapshot.continuation,
    ))?;
    let serialized = serde_json::to_vec(&(
        snapshot.system_prompt.as_str(),
        &snapshot.prompt_layers,
        distinct_prompt,
        &snapshot.history,
        &snapshot.tool_schemas,
        &snapshot.additional_params,
        &snapshot.continuation,
    ))
    .map_err(|error| {
        format!("canonical Provider request evidence serialization failed: {error}")
    })?;
    let serialized_text = String::from_utf8_lossy(&serialized);
    Ok(ProviderRequestBudgetEvidence {
        request_assembly_fingerprint,
        model_route_fingerprint,
        toolset_fingerprint,
        system_prompt_fingerprint,
        request_text_measurement: SessionProviderRequestTextMeasurement {
            character_count: serialized_text.chars().count(),
            utf8_byte_count: serialized.len(),
            structural_units: snapshot
                .history
                .len()
                .saturating_mul(4)
                .saturating_add(snapshot.tool_schemas.len().saturating_mul(8))
                .saturating_add(16),
            history_message_count: snapshot.history.len(),
            prompt_in_history: snapshot.prompt_in_history,
            history_fingerprint: Some(history_fingerprint),
            prefix_invariant_fingerprint: Some(prefix_invariant_fingerprint),
            context_breakdown: Some(build_request_context_breakdown(snapshot, distinct_prompt)?),
        },
    })
}

/// Builds non-sensitive request-category weights from exact structured request ownership.
/// 根据精确结构化请求归属构建非敏感请求分类权重。
fn build_request_context_breakdown(
    snapshot: &CoreProviderRequestSnapshot,
    distinct_prompt: Option<&str>,
) -> Result<SessionProviderRequestContextBreakdown, String> {
    // breakdown keeps display categories separate without treating character weights as Provider facts.
    // breakdown 保持展示分类分离，同时不把字符权重冒充 Provider 事实。
    let mut breakdown = SessionProviderRequestContextBreakdown {
        message_units: serialized_character_units(&(distinct_prompt, &snapshot.history))?,
        ..SessionProviderRequestContextBreakdown::default()
    };
    if snapshot.prompt_layers.is_empty() {
        breakdown.system_prompt_units = text_character_units(&[snapshot.system_prompt.as_str()]);
    } else {
        breakdown.system_prompt_units = text_character_units(&[
            snapshot.prompt_layers.system.as_str(),
            snapshot.prompt_layers.developer.as_str(),
        ]);
        breakdown.system_tool_units =
            text_character_units(&[snapshot.prompt_layers.tool_guidance.as_str()]);
        breakdown.other_units =
            text_character_units(&[snapshot.prompt_layers.runtime_context.as_str()]);
    }
    for tool in &snapshot.tool_schemas {
        // toolUnits measures the exact projected schema, including its stable source metadata.
        // toolUnits 测量精确投影后的 Schema，并包含其稳定来源元数据。
        let tool_units = serialized_character_units(tool)?;
        match tool.source_kind.as_str() {
            "native" | "vulcan_managed" | "vulcan_plugin" | "ai_host_binding" => {
                breakdown.system_tool_units =
                    breakdown.system_tool_units.saturating_add(tool_units);
            }
            "mcp" => {
                breakdown.mcp_tool_units = breakdown.mcp_tool_units.saturating_add(tool_units);
            }
            "luaskills" => {
                breakdown.skill_units = breakdown.skill_units.saturating_add(tool_units);
            }
            _ => {
                breakdown.other_units = breakdown.other_units.saturating_add(tool_units);
            }
        }
    }
    breakdown.other_units = breakdown
        .other_units
        .saturating_add(serialized_character_units(&(
            &snapshot.additional_params,
            &snapshot.continuation,
            snapshot.temperature.map(f64::to_bits),
            snapshot.max_turns,
        ))?);
    Ok(breakdown)
}

/// Counts visible text units with the same blank-line separators used by prompt rendering.
/// 按提示词渲染使用的空行分隔统计可见文本权重。
fn text_character_units(parts: &[&str]) -> usize {
    let non_empty_parts = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let body_units = non_empty_parts.iter().fold(0_usize, |sum, part| {
        sum.saturating_add(part.chars().count())
    });
    body_units.saturating_add(non_empty_parts.len().saturating_sub(1).saturating_mul(2))
}

/// Counts Unicode scalar units in one validated structured request component.
/// 统计单个已校验结构化请求组件中的 Unicode 标量权重。
fn serialized_character_units(value: &impl Serialize) -> Result<usize, String> {
    serde_json::to_string(value)
        .map(|serialized| serialized.chars().count())
        .map_err(|error| format!("Provider request breakdown serialization failed: {error}"))
}

/// Returns a SHA-256 identity for one serializable non-secret value.
/// 返回一个可序列化非秘密值的 SHA-256 身份。
fn digest_serializable(value: &impl Serialize) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("Provider budget evidence serialization failed: {error}"))?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

/// Shared mutable baseline updated only after a Provider call boundary is durably persisted.
/// 仅在 Provider 调用边界持久化成功后更新的共享可变基线。
#[derive(Debug, Clone)]
pub(crate) struct ProviderUsageBudgetBaselineHandle {
    /// Latest exact ledger record visible to the next logical Provider request.
    /// 下一次逻辑 Provider 请求可见的最近精确账本记录。
    value: Arc<RwLock<Option<SessionProviderCallRecord>>>,
}

impl ProviderUsageBudgetBaselineHandle {
    /// Builds one shared baseline from the persisted session snapshot used at launch.
    /// 根据启动时使用的持久化会话快照构建共享基线。
    pub(crate) fn new(baseline: Option<SessionProviderCallRecord>) -> Self {
        Self {
            value: Arc::new(RwLock::new(baseline)),
        }
    }

    /// Reads the latest durably admitted baseline.
    /// 读取最近已通过持久化准入的基线。
    pub(crate) fn snapshot(&self) -> Result<Option<SessionProviderCallRecord>, String> {
        self.value
            .read()
            .map(|value| value.clone())
            .map_err(|_| String::from("The Provider Usage budget baseline is locked."))
    }

    /// Replaces the baseline after its owning ledger revision is durable.
    /// 在所属账本修订持久化完成后替换基线。
    pub(crate) fn replace(
        &self,
        baseline: Option<SessionProviderCallRecord>,
    ) -> Result<(), String> {
        *self
            .value
            .write()
            .map_err(|_| String::from("The Provider Usage budget baseline is locked."))? = baseline;
        Ok(())
    }
}

/// Request-budget guard shared by the host-controlled streaming adapter wrapper.
/// 由宿主控制流式适配器包装层共享的请求预算守卫。
#[derive(Clone)]
pub(crate) struct ProviderRequestBudgetGuard {
    /// Stable provider identifier used by the request budget fingerprint.
    /// 请求预算指纹使用的稳定 Provider 标识。
    provider_id: String,
    /// Stable model identifier used by the token estimator.
    /// Token 估算器使用的稳定模型标识。
    model_id: String,
    /// Canonical provider API family used by context-budget computation.
    /// 上下文预算计算使用的规范 Provider 接口族。
    protocol_profile_id: String,
    /// Content-addressed revision of the exact frozen route.
    /// 精确冻结路由的内容寻址修订号。
    route_revision: String,
    /// Resolved provider/model capabilities used to derive budget limits.
    /// 用于推导预算上限的已解析 Provider/Model 能力。
    capabilities: ModelCapabilities,
    /// Resolved reasoning controls used to reserve thinking tokens.
    /// 用于预留思考 Token 的已解析推理控制。
    reasoning: ProviderRuntimeReasoningResolution,
    /// Optional compaction policy resolved for this request.
    /// 为本次请求解析出的可选压缩策略。
    context_compaction_policy: Option<ContextCompactionPolicy>,
    /// Optional request-level input-token budget limit.
    /// 可选的请求级输入 Token 预算上限。
    context_budget_override: Option<usize>,
    /// Absolute BodyAfterPrefix baseline from the active compaction window.
    /// 当前压缩窗口的绝对 BodyAfterPrefix 基线。
    prefill_input_tokens: Option<usize>,
    /// Current context generation used to reject stale Provider anchors.
    /// 用于拒绝过期 Provider 锚点的当前上下文代际。
    context_generation_id: Option<String>,
    /// Latest durable Provider call candidate shared with telemetry persistence.
    /// 与遥测持久化共享的最近持久 Provider 调用候选。
    provider_usage_baseline: ProviderUsageBudgetBaselineHandle,
}

impl ProviderRequestBudgetGuard {
    /// Builds one request-budget guard for the active provider/model pair.
    /// 为当前 Provider/Model 组合构建一份请求预算守卫。
    #[cfg(test)]
    pub(crate) fn new(
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        protocol_profile_id: impl Into<String>,
        route_revision: impl Into<String>,
        capabilities: ModelCapabilities,
        reasoning: ProviderRuntimeReasoningResolution,
        context_compaction_policy: Option<ContextCompactionPolicy>,
        context_budget_override: Option<usize>,
        prefill_input_tokens: Option<usize>,
        context_generation_id: Option<String>,
        provider_usage_baseline: Option<SessionProviderCallRecord>,
    ) -> Self {
        Self::with_shared_baseline(
            provider_id,
            model_id,
            protocol_profile_id,
            route_revision,
            capabilities,
            reasoning,
            context_compaction_policy,
            context_budget_override,
            prefill_input_tokens,
            context_generation_id,
            ProviderUsageBudgetBaselineHandle::new(provider_usage_baseline),
        )
    }

    /// Builds one guard around a baseline handle shared with Provider telemetry.
    /// 围绕与 Provider 遥测共享的基线句柄构建预算守卫。
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn with_shared_baseline(
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        protocol_profile_id: impl Into<String>,
        route_revision: impl Into<String>,
        capabilities: ModelCapabilities,
        reasoning: ProviderRuntimeReasoningResolution,
        context_compaction_policy: Option<ContextCompactionPolicy>,
        context_budget_override: Option<usize>,
        prefill_input_tokens: Option<usize>,
        context_generation_id: Option<String>,
        provider_usage_baseline: ProviderUsageBudgetBaselineHandle,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            protocol_profile_id: protocol_profile_id.into(),
            route_revision: route_revision.into(),
            capabilities,
            reasoning,
            context_compaction_policy,
            context_budget_override,
            prefill_input_tokens,
            context_generation_id,
            provider_usage_baseline,
        }
    }

    /// Validates one final request snapshot and rejects dispatch when automatic compaction should run first.
    /// 校验一份最终请求快照，并在应先执行自动压缩时拒绝发送。
    fn build_budget_snapshot(
        &self,
        snapshot: &CoreProviderRequestSnapshot,
    ) -> Result<ContextBudgetSnapshot, String> {
        let evidence = build_provider_request_budget_evidence(
            snapshot,
            self.protocol_profile_id.as_str(),
            self.route_revision.as_str(),
        )?;
        let fallback = build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: self.provider_id.as_str(),
                model_id: self.model_id.as_str(),
                protocol_profile_id: self.protocol_profile_id.as_str(),
                capabilities: &self.capabilities,
                reasoning: &self.reasoning,
                request_snapshot: snapshot,
                compaction_policy: self.context_compaction_policy,
                context_budget_override: self.context_budget_override,
                observed_usage: None,
                accounted_input_tokens: None,
                accounting_source: ContextBudgetAccountingSource::CharacterEstimate,
                accounting_warning_code: Some("provider_usage_baseline_unavailable"),
                provider_anchor_call_id: None,
                prefill_input_tokens: self.prefill_input_tokens,
            },
        );
        let provider_usage_baseline = self.provider_usage_baseline.snapshot()?;
        let Some(baseline) = provider_usage_baseline.as_ref() else {
            return Ok(fallback);
        };
        let Some(provider_usage) = baseline.provider_usage_evidence.provider_usage.as_ref() else {
            return Ok(fallback);
        };
        let Some(provider_input_tokens) =
            vulcan_context::observed_input_tokens_from_usage(Some(provider_usage))
        else {
            return Ok(fallback);
        };
        let current_measurement = &evidence.request_text_measurement;
        let prefix_match = if let Some(previous) = baseline.request_text_measurement.as_ref()
            && previous.history_message_count < snapshot.history.len()
        {
            let prefix_fingerprint =
                digest_serializable(&&snapshot.history[..previous.history_message_count])?;
            previous.prompt_in_history
                && current_measurement.prompt_in_history
                && previous.history_fingerprint.as_deref() == Some(prefix_fingerprint.as_str())
                && previous.prefix_invariant_fingerprint.is_some()
                && previous.prefix_invariant_fingerprint
                    == current_measurement.prefix_invariant_fingerprint
                && current_measurement.character_count >= previous.character_count
                && current_measurement.utf8_byte_count >= previous.utf8_byte_count
                && current_measurement.structural_units >= previous.structural_units
        } else {
            false
        };
        let stable_shape = baseline.active_branch_member
            && baseline.superseded_by_edit_generation.is_none()
            && baseline.context_generation_id == self.context_generation_id
            && baseline.model_route_fingerprint.as_deref()
                == Some(evidence.model_route_fingerprint.as_str())
            && baseline.toolset_fingerprint.as_deref()
                == Some(evidence.toolset_fingerprint.as_str())
            && baseline.system_prompt_fingerprint.as_deref()
                == Some(evidence.system_prompt_fingerprint.as_str());
        let exact_match = stable_shape
            && baseline.request_assembly_fingerprint.as_deref()
                == Some(evidence.request_assembly_fingerprint.as_str());
        let (accounted_input_tokens, accounting_source, warning_code) = if exact_match {
            (
                provider_input_tokens,
                ContextBudgetAccountingSource::ProviderReported,
                None,
            )
        } else if stable_shape && prefix_match {
            let Some(previous) = baseline.request_text_measurement.as_ref() else {
                return Ok(ContextBudgetSnapshot {
                    accounting_warning_code: Some(String::from("provider_usage_baseline_mismatch")),
                    ..fallback
                });
            };
            let delta_tokens = estimate_measurement_delta_tokens(previous, current_measurement);
            (
                provider_input_tokens.saturating_add(delta_tokens),
                ContextBudgetAccountingSource::ProviderAnchorPlusCharacterDelta,
                Some("provider_anchor_character_delta"),
            )
        } else {
            return Ok(ContextBudgetSnapshot {
                accounting_warning_code: Some(String::from("provider_usage_baseline_mismatch")),
                ..fallback
            });
        };
        Ok(build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: self.provider_id.as_str(),
                model_id: self.model_id.as_str(),
                protocol_profile_id: self.protocol_profile_id.as_str(),
                capabilities: &self.capabilities,
                reasoning: &self.reasoning,
                request_snapshot: snapshot,
                compaction_policy: self.context_compaction_policy,
                context_budget_override: self.context_budget_override,
                observed_usage: Some(provider_usage),
                accounted_input_tokens: Some(accounted_input_tokens),
                accounting_source,
                accounting_warning_code: warning_code,
                provider_anchor_call_id: Some(baseline.provider_call_id.as_str()),
                prefill_input_tokens: self.prefill_input_tokens,
            },
        ))
    }

    /// Validates one already-built budget snapshot before dispatch.
    /// 在分派前校验一份已经构建的预算快照。
    fn validate_budget(&self, budget: &ContextBudgetSnapshot) -> Result<(), VulcanCoreError> {
        let hard_window_exceeded = matches!(
            budget.severity,
            vulcan_context::ContextBudgetSeverity::Overflow
        );
        if hard_window_exceeded || budget.should_compact() {
            return Err(VulcanCoreError::ContextCompactionRequired {
                message: format!(
                    "请求预算门阻止发送：{}。请先执行上下文压缩后重新组装请求。",
                    budget.summary_note()
                ),
            });
        }

        Ok(())
    }
}

/// Estimates only the proven appended request delta without loading a tokenizer dictionary.
/// 在不加载 Tokenizer 词典的前提下，仅估算已证明追加的请求增量。
fn estimate_measurement_delta_tokens(
    previous: &SessionProviderRequestTextMeasurement,
    current: &SessionProviderRequestTextMeasurement,
) -> usize {
    let character_delta = current
        .character_count
        .saturating_sub(previous.character_count);
    let byte_delta = current
        .utf8_byte_count
        .saturating_sub(previous.utf8_byte_count);
    let structural_delta = current
        .structural_units
        .saturating_sub(previous.structural_units);
    let non_ascii_upper_bound = byte_delta.saturating_sub(character_delta).div_ceil(2);
    let ascii_characters = character_delta.saturating_sub(non_ascii_upper_bound);
    non_ascii_upper_bound
        .saturating_add(ascii_characters.div_ceil(4))
        .saturating_add(structural_delta)
}

/// Streaming adapter wrapper that validates the exact built request before provider dispatch.
/// 在 Provider 分发前校验精确已构建请求的流式适配器包装层。
#[derive(Clone)]
pub(crate) struct BudgetCheckedStreamingAdapter<A> {
    /// Inner provider adapter that still owns provider-specific request execution.
    /// 仍然持有 Provider 专属请求执行逻辑的内部适配器。
    inner: A,
    /// Optional request-budget guard used before network dispatch.
    /// 网络分发前使用的可选请求预算守卫。
    request_budget_guard: Option<ProviderRequestBudgetGuard>,
}

/// Exposes the canonical request snapshot consumed by the budget guard.
/// 公开预算门消费的规范请求快照。
pub(crate) trait ProviderBudgetRequest {
    /// Returns the exact canonical snapshot represented by this transport request.
    /// 返回该传输请求所表示的精确规范快照。
    fn budget_snapshot(&self) -> &CoreProviderRequestSnapshot;
    /// Stores the budget decision derived from this exact canonical request.
    /// 保存从当前精确规范请求派生的预算决策。
    fn set_context_budget(&mut self, budget: ContextBudgetSnapshot);
    /// Returns the stored request-aligned budget decision.
    /// 返回已保存且与请求对齐的预算决策。
    fn context_budget(&self) -> Option<&ContextBudgetSnapshot>;
}

impl ProviderBudgetRequest for VulcanPublicContractRequest {
    fn budget_snapshot(&self) -> &CoreProviderRequestSnapshot {
        &self.budget
    }

    fn set_context_budget(&mut self, budget: ContextBudgetSnapshot) {
        self.context_budget = Some(budget);
    }

    fn context_budget(&self) -> Option<&ContextBudgetSnapshot> {
        self.context_budget.as_ref()
    }
}

impl<A> BudgetCheckedStreamingAdapter<A> {
    /// Wraps one provider adapter with an optional request-budget guard.
    /// 使用可选请求预算守卫包装一个 Provider 适配器。
    pub(crate) fn new(inner: A, request_budget_guard: Option<ProviderRequestBudgetGuard>) -> Self {
        Self {
            inner,
            request_budget_guard,
        }
    }
}

#[async_trait::async_trait]
impl<A> VulcanProviderAdapter for BudgetCheckedStreamingAdapter<A>
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Clone + Send + Sync,
    A::Request: ProviderBudgetRequest,
{
    type Request = A::Request;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the wrapped adapter.
    /// 返回被包装适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        self.inner.provider_id()
    }

    /// Returns the stable model identifier used by the wrapped adapter.
    /// 返回被包装适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        self.inner.model_id()
    }

    /// Builds one provider request through the wrapped adapter.
    /// 通过被包装适配器构建一份 Provider 请求。
    async fn build_request(
        &self,
        snapshot: &vulcan_core::types::ConversationSnapshot,
        config: &vulcan_core::types::ControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let mut request = self.inner.build_request(snapshot, config).await?;
        if let Some(guard) = self.request_budget_guard.as_ref() {
            let budget = guard
                .build_budget_snapshot(request.budget_snapshot())
                .map_err(|message| VulcanCoreError::ContextCompactionRequired { message })?;
            request.set_context_budget(budget);
        }
        Ok(request)
    }

    /// Forwards one-shot execution to the wrapped adapter.
    /// 将单次执行转发给被包装适配器。
    async fn execute_request(
        &self,
        request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        self.inner.execute_request(request).await
    }

    /// Validates the exact request snapshot before network dispatch.
    /// 在网络分发前校验精确请求快照。
    fn validate_built_request(&self, request: &Self::Request) -> Result<(), VulcanCoreError> {
        self.inner.validate_built_request(request)?;
        if let Some(guard) = self.request_budget_guard.as_ref() {
            let budget = request.context_budget().ok_or_else(|| {
                VulcanCoreError::ContextCompactionRequired {
                    message: String::from(
                        "请求预算门缺少与最终请求对齐的预算快照，已阻止网络分派。",
                    ),
                }
            })?;
            guard.validate_budget(budget)?;
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<A> VulcanStreamingProviderAdapter for BudgetCheckedStreamingAdapter<A>
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Clone + Send + Sync,
    A::Request: ProviderBudgetRequest,
{
    /// Forwards streaming execution to the wrapped adapter after validation has already passed.
    /// 在校验通过后将流式执行转发给被包装适配器。
    async fn execute_request_streaming<S>(
        &self,
        request: Self::Request,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        self.inner.execute_request_streaming(request, sink).await
    }
}

#[cfg(test)]
mod tests {
    //! Request-budget guard tests for provider and tool boundaries.
    //! Provider 与工具边界的请求预算守卫测试。

    use super::*;
    use vulcan_core::types::{
        ConversationMessage, ConversationPayload, ProviderContinuation, ProviderPromptLayers,
        ProviderUsageSnapshot,
    };
    use vulcan_workbench_model::{
        SessionProviderCallCompleted, SessionProviderCallIdentity, SessionProviderCallLedger,
        SessionProviderCallRecord, SessionProviderCallStarted, SessionProviderCallStatus,
        SessionProviderUsageAvailability, SessionProviderUsageEvidence,
    };

    /// Builds one minimal request snapshot for request-boundary budget tests.
    /// 为请求边界预算测试构建一份最小请求快照。
    ///
    /// `history` is the exact canonical history assembled for the next physical request.
    /// `history` 是为下一次真实请求装配的精确规范历史。
    /// The returned snapshot contains no provider-specific optional fields.
    /// 返回的快照不包含供应商专属可选字段。
    fn request_snapshot(history: Vec<ConversationMessage>) -> CoreProviderRequestSnapshot {
        CoreProviderRequestSnapshot {
            provider_id: String::from("test-provider"),
            model_id: String::from("test-model"),
            system_prompt: String::new(),
            prompt_layers: ProviderPromptLayers::default(),
            prompt: String::from("continue"),
            prompt_in_history: false,
            history,
            temperature: None,
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: ProviderContinuation::default(),
        }
    }

    /// Verifies exact tool ownership is retained in non-sensitive request category weights.
    /// 验证精确工具归属会保留在非敏感请求分类权重中。
    #[test]
    fn request_context_breakdown_separates_system_mcp_and_skill_tools() {
        let mut request = request_snapshot(Vec::new());
        request.prompt_layers = ProviderPromptLayers {
            system: String::from("system"),
            developer: String::from("developer"),
            tool_guidance: String::from("tool guidance"),
            runtime_context: String::from("runtime context"),
        };
        request.tool_schemas = ["native", "mcp", "luaskills"]
            .into_iter()
            .map(
                |source_kind| vulcan_core::types::ProviderRequestToolSchema {
                    id: format!("{source_kind}-tool"),
                    name: format!("{source_kind}_tool"),
                    description: String::from("tool"),
                    source_kind: source_kind.to_string(),
                    source_label: source_kind.to_string(),
                    parameters_schema: serde_json::json!({"type": "object"}),
                },
            )
            .collect();

        let breakdown = build_request_context_breakdown(&request, Some(request.prompt.as_str()))
            .expect("request category measurement should serialize");

        assert!(breakdown.message_units > 0);
        assert!(breakdown.system_prompt_units > 0);
        assert!(breakdown.system_tool_units > 0);
        assert!(breakdown.mcp_tool_units > 0);
        assert!(breakdown.skill_units > 0);
        assert!(breakdown.other_units > 0);
    }

    /// Builds one completed durable Provider call that exactly describes the supplied request.
    /// 构建一条精确描述给定请求的已完成持久化 Provider 调用。
    ///
    /// `request` supplies the immutable request fingerprints and text measurement.
    /// `request` 提供不可变请求指纹与文本测量。
    /// `provider_input_tokens` is the final input-token count reported by the Provider.
    /// `provider_input_tokens` 是 Provider 最终报告的输入 Token 数。
    /// The returned record belongs to context generation `context-1` and can seed a budget guard.
    /// 返回记录属于上下文代际 `context-1`，可用于初始化预算守卫。
    fn completed_provider_baseline(
        request: &CoreProviderRequestSnapshot,
        provider_input_tokens: usize,
    ) -> SessionProviderCallRecord {
        let evidence = build_provider_request_budget_evidence(
            request,
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
        )
        .expect("baseline request evidence should build");
        let identity = SessionProviderCallIdentity {
            logical_request_id: String::from("logical-1"),
            execution_id: String::from("execution-1"),
            turn_id: Some(String::from("turn-1")),
            attempt_index: 0,
            retry_number: 0,
            owner_kind: String::from("main_agent"),
            owner_id: String::from("agent-1"),
            owner_name_snapshot: String::from("Agent One"),
            operation: String::from("conversation"),
            provider_id: String::from("test-provider"),
            provider_name_snapshot: String::from("Test Provider"),
            model_id: String::from("test-model"),
            model_name_snapshot: String::from("Test Model"),
            connection_id: String::from("connection-1"),
            service_entry_id: String::from("public-api"),
            protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
            manifest_revision: 1,
            manifest_generation: 1,
            binding_source: String::from("system_manifest"),
            binding_rule: Some(String::from("rule-1")),
            route_revision: String::from("sha256:test-route"),
            request_profiles: Vec::new(),
            response_profiles: Vec::new(),
        };
        let mut ledger = SessionProviderCallLedger::default();
        assert!(ledger.advance_context_generation(String::from("context-1")));
        assert!(ledger.observe_started(SessionProviderCallStarted {
            provider_call_id: String::from("call-1"),
            identity: identity.clone(),
            prepared_at_ms: 1,
            request_assembly_fingerprint: Some(evidence.request_assembly_fingerprint),
            context_generation_id: Some(String::from("context-1")),
            context_start_record_id: None,
            model_route_fingerprint: Some(evidence.model_route_fingerprint),
            toolset_fingerprint: Some(evidence.toolset_fingerprint),
            system_prompt_fingerprint: Some(evidence.system_prompt_fingerprint),
            request_text_measurement: Some(evidence.request_text_measurement),
            budget_accounting: None,
        }));
        assert!(ledger.observe_completed(SessionProviderCallCompleted {
            provider_call_id: String::from("call-1"),
            identity,
            status: SessionProviderCallStatus::Succeeded,
            provider_usage_evidence: SessionProviderUsageEvidence {
                availability: SessionProviderUsageAvailability::Final,
                provider_usage: Some(ProviderUsageSnapshot {
                    update_kind: Some(vulcan_ai_contract::UsageUpdateKind::Final),
                    input_tokens: Some(provider_input_tokens),
                    output_tokens: Some(9),
                    total_tokens: Some(provider_input_tokens.saturating_add(9)),
                    ..ProviderUsageSnapshot::default()
                }),
            },
            prepared_at_ms: 1,
            dispatched_at_ms: Some(2),
            first_event_at_ms: Some(3),
            first_token_at_ms: Some(4),
            completed_at_ms: 5,
            dispatch_outcome: Some(String::from("dispatched")),
            dispatch_outcome_known: Some(true),
            semantic_output_observed: Some(true),
            semantic_output_observed_known: Some(true),
            replay_safe: Some(false),
            error_category: None,
            error_code: None,
        }));
        ledger
            .calls
            .remove("call-1")
            .expect("durable ledger should contain the terminal call")
    }

    /// Builds a request-budget guard seeded by one durable Provider usage baseline.
    /// 构建一个由持久化 Provider 用量基线初始化的请求预算守卫。
    ///
    /// `baseline` is the exact terminal call eligible for reuse in context generation `context-1`.
    /// `baseline` 是可在上下文代际 `context-1` 中复用的精确终态调用。
    /// The returned guard uses the stable route and model fixtures shared by this test module.
    /// 返回守卫使用本测试模块共享的稳定路由与模型夹具。
    fn guard_with_baseline(baseline: SessionProviderCallRecord) -> ProviderRequestBudgetGuard {
        ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
            Some(String::from("context-1")),
            Some(baseline),
        )
    }

    /// Verifies a large tool result is rechecked before the next physical provider request.
    /// 验证大型工具结果会在下一次真实 Provider 请求前重新接受预算检查。
    #[test]
    fn tool_result_growth_requires_compaction_before_next_provider_request() {
        let guard = ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(1_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            Some(100),
            None,
            None,
            None,
        );

        let initial_budget = guard
            .build_budget_snapshot(&request_snapshot(Vec::new()))
            .expect("initial budget should build");
        guard
            .validate_budget(&initial_budget)
            .expect("the first compact request should pass the dispatch budget gate");

        let request_after_tool = request_snapshot(vec![ConversationMessage {
            id: String::from("tool-result-1"),
            payload: ConversationPayload::ToolResult {
                tool_call_id: Some(String::from("call-1")),
                is_error: false,
                text: "large tool output ".repeat(200),
            },
        }]);
        let expanded_budget = guard
            .build_budget_snapshot(&request_after_tool)
            .expect("expanded budget should build");
        let error = guard
            .validate_budget(&expanded_budget)
            .expect_err("tool-result growth must trigger compaction before dispatch");

        assert!(matches!(
            error,
            VulcanCoreError::ContextCompactionRequired { .. }
        ));
    }

    /// Verifies disabling automatic compaction never disables the hard-window dispatch gate.
    /// 验证关闭自动压缩绝不会同时关闭硬窗口分派门禁。
    #[test]
    fn auto_disabled_hard_window_still_rejects_dispatch() {
        let guard = ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(100),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy {
                auto: false,
                ..ContextCompactionPolicy::default()
            }),
            None,
            None,
            None,
            None,
        );
        let request = request_snapshot(vec![ConversationMessage {
            id: String::from("large-user-1"),
            payload: ConversationPayload::UserText {
                text: "hard window payload ".repeat(100),
                attachments: Vec::new(),
            },
        }]);
        let budget = guard
            .build_budget_snapshot(&request)
            .expect("hard-window budget should build");

        assert_eq!(
            budget.severity,
            vulcan_context::ContextBudgetSeverity::Overflow
        );
        assert!(!budget.should_compact());
        assert!(matches!(
            guard.validate_budget(&budget),
            Err(VulcanCoreError::ContextCompactionRequired { .. })
        ));
    }

    /// Verifies an explicit absolute compaction line works without a declared model window.
    /// 验证模型未声明窗口时显式绝对压缩线仍然生效。
    #[test]
    fn absolute_compaction_limit_works_without_model_window() {
        let guard = ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities::default(),
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy {
                window: vulcan_context::ContextWindowPolicy {
                    auto_compact_token_limit: Some(20),
                    ..vulcan_context::ContextWindowPolicy::default()
                },
                ..ContextCompactionPolicy::default()
            }),
            None,
            None,
            None,
            None,
        );
        let request = request_snapshot(vec![ConversationMessage {
            id: String::from("absolute-user-1"),
            payload: ConversationPayload::UserText {
                text: "absolute compaction payload ".repeat(20),
                attachments: Vec::new(),
            },
        }]);
        let budget = guard
            .build_budget_snapshot(&request)
            .expect("absolute-line budget should build");

        assert_eq!(budget.effective_context_window, None);
        assert_eq!(budget.auto_compact_limit, Some(20));
        assert_eq!(
            budget.severity,
            vulcan_context::ContextBudgetSeverity::CompactSoon
        );
        assert!(matches!(
            guard.validate_budget(&budget),
            Err(VulcanCoreError::ContextCompactionRequired { .. })
        ));
    }

    /// Verifies a prompt already present in canonical history is measured exactly once.
    /// 验证已经存在于规范历史中的提示词只会被计量一次。
    #[test]
    fn canonical_history_prompt_copy_is_not_double_counted() {
        let mut request = request_snapshot(vec![ConversationMessage {
            id: String::from("user-1"),
            payload: ConversationPayload::UserText {
                text: String::from("continue"),
                attachments: Vec::new(),
            },
        }]);
        request.prompt_in_history = true;
        let embedded_evidence = build_provider_request_budget_evidence(
            &request,
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
        )
        .expect("embedded request evidence should build");
        let guard = ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
            None,
            None,
        );
        let embedded_budget = guard
            .build_budget_snapshot(&request)
            .expect("embedded prompt budget should build");

        let mut distinct_request = request.clone();
        distinct_request.prompt_in_history = false;
        let distinct_evidence = build_provider_request_budget_evidence(
            &distinct_request,
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
        )
        .expect("distinct request evidence should build");

        assert_eq!(embedded_budget.estimated_prompt_tokens, 0);
        assert!(
            embedded_evidence.request_text_measurement.character_count
                < distinct_evidence.request_text_measurement.character_count
        );
        assert_ne!(
            embedded_evidence.request_assembly_fingerprint,
            distinct_evidence.request_assembly_fingerprint
        );
    }

    /// Verifies upstream-normalized GBK text enters the non-recursive character estimator safely.
    /// 验证由上游规范化的 GBK 文本会安全进入非递归字符估算器。
    #[test]
    fn normalized_gbk_text_uses_character_estimation_without_panicking() {
        let source = "旧会话编辑后继续处理供应商用量，并保留中文错误信息。";
        let (encoded, _, encode_had_errors) = encoding_rs::GBK.encode(source);
        assert!(!encode_had_errors);
        let (decoded, _, decode_had_errors) = encoding_rs::GBK.decode(encoded.as_ref());
        assert!(!decode_had_errors);
        let request = request_snapshot(vec![ConversationMessage {
            id: String::from("gbk-user-1"),
            payload: ConversationPayload::UserText {
                text: decoded.into_owned(),
                attachments: Vec::new(),
            },
        }]);
        let guard = ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
            None,
            None,
        );

        let budget = guard
            .build_budget_snapshot(&request)
            .expect("normalized GBK request budget should build");

        assert_eq!(
            budget.accounting_source,
            ContextBudgetAccountingSource::CharacterEstimate
        );
        assert!(budget.active_context_tokens > 0);
    }

    /// Verifies an excessively deep tool schema fails closed before recursive serialization.
    /// 验证过深工具 Schema 会在递归序列化前失败封闭。
    #[test]
    fn deeply_nested_tool_schema_returns_controlled_budget_error() {
        let mut parameters_schema = serde_json::Value::Null;
        for _ in 0..=PROVIDER_REQUEST_EVIDENCE_MAX_JSON_DEPTH {
            parameters_schema = serde_json::json!({"nested": parameters_schema});
        }
        let mut request = request_snapshot(Vec::new());
        request
            .tool_schemas
            .push(vulcan_core::types::ProviderRequestToolSchema {
                id: String::from("deep-tool"),
                name: String::from("deep_tool"),
                description: String::from("Deep schema rejection fixture"),
                source_kind: String::from("native"),
                source_label: String::from("test"),
                parameters_schema,
            });
        let guard = ProviderRequestBudgetGuard::new(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
            None,
            None,
        );

        let error = guard
            .build_budget_snapshot(&request)
            .expect_err("deep tool schemas must fail before dispatch");

        assert!(error.contains("JSON depth limit"));
    }

    /// Verifies a durable terminal Usage boundary changes the next logical request baseline immediately.
    /// 验证持久化终态用量边界会立即改变下一次逻辑请求的基线。
    #[test]
    fn durable_usage_handle_refreshes_the_next_request_budget() {
        let request = request_snapshot(Vec::new());
        let baseline_handle = ProviderUsageBudgetBaselineHandle::new(None);
        let guard = ProviderRequestBudgetGuard::with_shared_baseline(
            "test-provider",
            "test-model",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
            Some(String::from("context-1")),
            baseline_handle.clone(),
        );
        let before = guard
            .build_budget_snapshot(&request)
            .expect("budget without a Provider baseline should build");
        assert_eq!(
            before.accounting_source,
            ContextBudgetAccountingSource::CharacterEstimate
        );

        let durable_record = completed_provider_baseline(&request, 77);
        baseline_handle
            .replace(Some(durable_record))
            .expect("durable boundary should refresh the shared baseline");

        let after = guard
            .build_budget_snapshot(&request)
            .expect("budget with the durable Provider baseline should build");
        assert_eq!(
            after.accounting_source,
            ContextBudgetAccountingSource::ProviderReported
        );
        assert_eq!(after.active_context_tokens, 77);
        assert_eq!(after.provider_anchor_call_id.as_deref(), Some("call-1"));
    }

    /// Verifies a proven canonical-history prefix reuses Provider input usage and estimates only the suffix.
    /// 验证已证明的规范历史前缀会复用 Provider 输入用量，并且只估算新增后缀。
    #[test]
    fn canonical_prefix_uses_provider_anchor_plus_character_delta() {
        let mut baseline_request = request_snapshot(vec![ConversationMessage {
            id: String::from("user-1"),
            payload: ConversationPayload::UserText {
                text: String::from("first request"),
                attachments: Vec::new(),
            },
        }]);
        baseline_request.prompt = String::from("first request");
        baseline_request.prompt_in_history = true;
        let guard = guard_with_baseline(completed_provider_baseline(&baseline_request, 101));

        let mut next_request = request_snapshot(vec![
            baseline_request.history[0].clone(),
            ConversationMessage {
                id: String::from("tool-result-1"),
                payload: ConversationPayload::ToolResult {
                    tool_call_id: Some(String::from("tool-call-1")),
                    is_error: false,
                    text: String::from("a durable response followed by a tool result"),
                },
            },
        ]);
        next_request.prompt = String::from("a durable response followed by a tool result");
        next_request.prompt_in_history = true;
        let budget = guard
            .build_budget_snapshot(&next_request)
            .expect("prefix-aligned request budget should build");

        assert_eq!(
            budget.accounting_source,
            ContextBudgetAccountingSource::ProviderAnchorPlusCharacterDelta
        );
        assert!(budget.active_context_tokens > 101);
        assert_eq!(
            budget.accounting_warning_code.as_deref(),
            Some("provider_anchor_character_delta")
        );
        assert_eq!(budget.provider_anchor_call_id.as_deref(), Some("call-1"));
    }

    /// Verifies appended history cannot reuse a Provider anchor when non-history request shape changes.
    /// 验证追加历史时如果非历史请求形状发生变化，则不能复用 Provider 锚点。
    #[test]
    fn changed_prefix_invariant_rejects_provider_anchor_reuse() {
        let mut baseline_request = request_snapshot(vec![ConversationMessage {
            id: String::from("user-1"),
            payload: ConversationPayload::UserText {
                text: String::from("first request"),
                attachments: Vec::new(),
            },
        }]);
        baseline_request.prompt = String::from("first request");
        baseline_request.prompt_in_history = true;
        baseline_request.additional_params = Some(serde_json::json!({"mode": "baseline"}));
        let guard = guard_with_baseline(completed_provider_baseline(&baseline_request, 101));

        let mut next_request = baseline_request.clone();
        next_request.history.push(ConversationMessage {
            id: String::from("tool-result-1"),
            payload: ConversationPayload::ToolResult {
                tool_call_id: Some(String::from("tool-call-1")),
                is_error: false,
                text: String::from("new tool result"),
            },
        });
        next_request.additional_params = Some(serde_json::json!({"mode": "changed"}));
        let budget = guard
            .build_budget_snapshot(&next_request)
            .expect("shape-mismatched request budget should use character estimation");

        assert_eq!(
            budget.accounting_source,
            ContextBudgetAccountingSource::CharacterEstimate
        );
        assert_eq!(
            budget.accounting_warning_code.as_deref(),
            Some("provider_usage_baseline_mismatch")
        );
        assert_eq!(budget.provider_anchor_call_id, None);
    }

    /// Verifies an altered request with unchanged history is never classified as a strict append.
    /// 验证历史未增长的变更请求绝不会被归类为严格追加。
    #[test]
    fn unchanged_history_cannot_enter_prefix_delta_mode() {
        let mut baseline_request = request_snapshot(vec![ConversationMessage {
            id: String::from("user-1"),
            payload: ConversationPayload::UserText {
                text: String::from("first request"),
                attachments: Vec::new(),
            },
        }]);
        baseline_request.prompt = String::from("first request");
        baseline_request.prompt_in_history = true;
        let guard = guard_with_baseline(completed_provider_baseline(&baseline_request, 101));

        let mut changed_request = baseline_request.clone();
        changed_request.max_turns = baseline_request.max_turns.saturating_add(1);
        let budget = guard
            .build_budget_snapshot(&changed_request)
            .expect("same-history request change should use character estimation");

        assert_eq!(
            budget.accounting_source,
            ContextBudgetAccountingSource::CharacterEstimate
        );
        assert_eq!(budget.provider_anchor_call_id, None);
    }

    /// Verifies a changed canonical prefix invalidates Provider usage reuse and falls back explicitly.
    /// 验证规范前缀发生变化时会使 Provider 用量复用失效，并显式回退。
    #[test]
    fn changed_canonical_prefix_rejects_provider_anchor_reuse() {
        let mut baseline_request = request_snapshot(vec![ConversationMessage {
            id: String::from("user-1"),
            payload: ConversationPayload::UserText {
                text: String::from("original branch"),
                attachments: Vec::new(),
            },
        }]);
        baseline_request.prompt = String::from("original branch");
        baseline_request.prompt_in_history = true;
        let guard = guard_with_baseline(completed_provider_baseline(&baseline_request, 101));

        let mut edited_request = request_snapshot(vec![ConversationMessage {
            id: String::from("user-1"),
            payload: ConversationPayload::UserText {
                text: String::from("edited branch"),
                attachments: Vec::new(),
            },
        }]);
        edited_request.prompt = String::from("edited branch");
        edited_request.prompt_in_history = true;
        let budget = guard
            .build_budget_snapshot(&edited_request)
            .expect("mismatched request budget should build through character estimation");

        assert_eq!(
            budget.accounting_source,
            ContextBudgetAccountingSource::CharacterEstimate
        );
        assert_eq!(
            budget.accounting_warning_code.as_deref(),
            Some("provider_usage_baseline_mismatch")
        );
        assert_eq!(budget.provider_anchor_call_id, None);
    }
}
