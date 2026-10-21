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
use vulcan_ai_contract::ModelCapabilities;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    ConversationPayload, CoreError as VulcanCoreError,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse, ProviderUsageSnapshot,
};
pub(crate) use vulcan_session_core::v1::ProviderRequestBudgetEvidence;
use vulcan_session_core::v1::{
    JS_SAFE_INTEGER_MAX, ProviderProjectionProof, ProviderRequestContextBreakdown,
    ProviderRequestTextMeasurement, ProviderUsageBudgetAnchor,
};

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
    // Native media contributes attachment metadata instead of Base64 prompt characters.
    // 原生媒体贡献附件元数据，而非Base64提示词字符。
    let media_snapshot = crate::native_media_input::budget_snapshot(snapshot)?;
    // All evidence measurements below share the same non-persistent accounting representation.
    // 下方所有证据测量共享相同非持久记账表示。
    let snapshot = media_snapshot.as_ref();
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
        request_text_measurement: ProviderRequestTextMeasurement {
            character_count: budget_u64(
                "request.character_count",
                serialized_text.chars().count(),
            )?,
            utf8_byte_count: budget_u64("request.utf8_byte_count", serialized.len())?,
            structural_units: budget_u64(
                "request.structural_units",
                snapshot
                    .history
                    .len()
                    .saturating_mul(4)
                    .saturating_add(snapshot.tool_schemas.len().saturating_mul(8))
                    .saturating_add(16),
            )?,
            history_message_count: budget_u64(
                "request.history_message_count",
                snapshot.history.len(),
            )?,
            prompt_in_history: snapshot.prompt_in_history,
            history_fingerprint,
            prefix_invariant_fingerprint,
            context_breakdown: build_request_context_breakdown(snapshot, distinct_prompt)?,
        },
    })
}

/// Exact budget decision and durable request evidence for one V1-permitted physical request.
/// 一次经 V1 许可的物理请求所对应的精确预算判定与持久化请求证据。
#[derive(Debug, Clone)]
pub struct ProviderRequestBudgetDecision {
    /// Context-budget decision validated against the exact final request.
    /// 针对精确最终请求校验得到的上下文预算判定。
    pub context_budget: ContextBudgetSnapshot,
    /// Non-sensitive evidence that must be persisted in the matching api_started event.
    /// 必须持久化到匹配 api_started 事件中的非敏感证据。
    pub evidence: ProviderRequestBudgetEvidence,
}

/// Builds non-sensitive request-category weights from exact structured request ownership.
/// 根据精确结构化请求归属构建非敏感请求分类权重。
fn build_request_context_breakdown(
    snapshot: &CoreProviderRequestSnapshot,
    distinct_prompt: Option<&str>,
) -> Result<ProviderRequestContextBreakdown, String> {
    // breakdown keeps display categories separate without treating character weights as Provider facts.
    // breakdown 保持展示分类分离，同时不把字符权重冒充 Provider 事实。
    let mut breakdown = ProviderRequestContextBreakdown {
        message_units: serialized_character_units(&(distinct_prompt, &snapshot.history))?,
        ..ProviderRequestContextBreakdown::default()
    };
    if snapshot.prompt_layers.is_empty() {
        breakdown.system_prompt_units = text_character_units(&[snapshot.system_prompt.as_str()])?;
    } else {
        breakdown.system_prompt_units = text_character_units(&[
            snapshot.prompt_layers.system.as_str(),
            snapshot.prompt_layers.developer.as_str(),
        ])?;
        breakdown.system_tool_units =
            text_character_units(&[snapshot.prompt_layers.tool_guidance.as_str()])?;
        breakdown.other_units =
            text_character_units(&[snapshot.prompt_layers.runtime_context.as_str()])?;
    }
    for tool in &snapshot.tool_schemas {
        // toolUnits measures the exact projected schema, including its stable source metadata.
        // toolUnits 测量精确投影后的 Schema，并包含其稳定来源元数据。
        let tool_units = serialized_character_units(tool)?;
        match tool.source_kind.as_str() {
            "native"
            | "vulcan_managed"
            | "vulcan_plugin"
            | "ai_host_binding"
            | crate::agent_runtime::code_mode::CODE_MODE_TOOL_SOURCE_KIND => {
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
fn text_character_units(parts: &[&str]) -> Result<u64, String> {
    let non_empty_parts = parts
        .iter()
        .map(|part| part.trim())
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let body_units = non_empty_parts.iter().fold(0_usize, |sum, part| {
        sum.saturating_add(part.chars().count())
    });
    budget_u64(
        "request.context_breakdown.text_units",
        body_units.saturating_add(non_empty_parts.len().saturating_sub(1).saturating_mul(2)),
    )
}

/// Counts Unicode scalar units in one validated structured request component.
/// 统计单个已校验结构化请求组件中的 Unicode 标量权重。
fn serialized_character_units(value: &impl Serialize) -> Result<u64, String> {
    let serialized = serde_json::to_string(value)
        .map_err(|error| format!("Provider request breakdown serialization failed: {error}"))?;
    budget_u64(
        "request.context_breakdown.serialized_units",
        serialized.chars().count(),
    )
}

/// Converts one in-memory collection size into the durable JS-safe request-budget domain.
/// 把一个内存集合尺寸转换为持久化且 JS 安全的请求预算数值域。
fn budget_u64(field: &str, value: usize) -> Result<u64, String> {
    let value = u64::try_from(value)
        .map_err(|_| format!("Provider request budget `{field}` exceeds the u64 domain."))?;
    if value > JS_SAFE_INTEGER_MAX {
        return Err(format!(
            "Provider request budget `{field}` exceeds the JS-safe integer domain."
        ));
    }
    Ok(value)
}

/// Returns a SHA-256 identity for one serializable non-secret value.
/// 返回一个可序列化非秘密值的 SHA-256 身份。
fn digest_serializable(value: &impl Serialize) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("Provider budget evidence serialization failed: {error}"))?;
    Ok(format!("sha256:{}", hex::encode(Sha256::digest(bytes))))
}

/// Request-budget guard shared by the host-controlled streaming adapter wrapper.
/// 由宿主控制流式适配器包装层共享的请求预算守卫。
#[derive(Debug, Clone)]
pub struct ProviderRequestBudgetGuard {
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
}

impl ProviderRequestBudgetGuard {
    /// Builds a guard whose calibration authority is exclusively the V1 event ledger.
    /// 构建一份仅以 V1 事件账本作为校准权威的预算守卫。
    ///
    /// # Parameters
    /// 参数
    /// Route, capabilities, reasoning, compaction, and explicit limits are frozen for the physical
    /// request family; no legacy Provider-call ledger baseline participates.
    /// 路由、能力、推理、压缩与显式限制会针对物理请求族冻结；不再参与任何旧 Provider 调用账本基线。
    ///
    /// # Returns
    /// 返回值
    /// Returns a reusable guard that accepts only an explicit V1 usage anchor at final preparation.
    /// 返回一份可复用守卫，仅在最终准备阶段接受显式 V1 用量锚点。
    #[allow(clippy::too_many_arguments)]
    pub fn new_v1(
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        protocol_profile_id: impl Into<String>,
        route_revision: impl Into<String>,
        capabilities: ModelCapabilities,
        reasoning: ProviderRuntimeReasoningResolution,
        context_compaction_policy: Option<ContextCompactionPolicy>,
        context_budget_override: Option<usize>,
        prefill_input_tokens: Option<usize>,
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
        }
    }

    /// Validates one final request snapshot and rejects dispatch when automatic compaction should run first.
    /// 校验一份最终请求快照，并在应先执行自动压缩时拒绝发送。
    fn build_budget_snapshot(
        &self,
        snapshot: &CoreProviderRequestSnapshot,
    ) -> Result<ContextBudgetSnapshot, String> {
        // Preliminary admission uses the same media accounting as the authoritative V1 request.
        // 初步准入使用与权威V1请求相同的媒体记账方式。
        let media_snapshot = crate::native_media_input::budget_snapshot(snapshot)?;
        // Keep evidence and the fallback estimator aligned without rewriting controller history.
        // 对齐证据与回退估算器，不改写控制器历史。
        let snapshot = media_snapshot.as_ref();
        build_provider_request_budget_evidence(
            snapshot,
            self.protocol_profile_id.as_str(),
            self.route_revision.as_str(),
        )?;
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
                observed_usage: None,
                accounted_input_tokens: None,
                accounting_source: ContextBudgetAccountingSource::CharacterEstimate,
                accounting_warning_code: Some("provider_usage_baseline_unavailable"),
                provider_anchor_call_id: None,
                prefill_input_tokens: self.prefill_input_tokens,
            },
        ))
    }

    /// Builds and validates the budget for the exact request rebuilt by the V1 permit owner.
    /// 为 V1 许可 owner 重建的精确请求构建并校验预算。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the immutable request that will reach transport, `projection` is the exact V1
    /// range used to build it, and `anchor` is the latest matching api_started/api_stats pair.
    /// `snapshot` 是即将进入传输层的不可变请求，`projection` 是构建它所使用的精确 V1 区间，
    /// `anchor` 是最近一组匹配的 api_started/api_stats 事实。
    ///
    /// # Returns
    /// 返回值
    /// Returns one validated budget plus the evidence that must enter the same api_started commit.
    /// 返回一份已校验预算，以及必须进入同一 api_started 提交的请求证据。
    pub fn build_v1_budget_decision(
        &self,
        snapshot: &CoreProviderRequestSnapshot,
        projection: &ProviderProjectionProof,
        anchor: Option<&ProviderUsageBudgetAnchor>,
    ) -> Result<ProviderRequestBudgetDecision, String> {
        // Preserve the durable tool payload while reusing the ordinary attachment estimator.
        // 保留持久工具载荷，同时复用普通附件估算器。
        let media_snapshot = crate::native_media_input::budget_snapshot(snapshot)?;
        // Anchor comparison and fallback estimates must use identical media-aware history.
        // 锚点比较及回退估算必须使用相同的媒体感知历史。
        let snapshot = media_snapshot.as_ref();
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
        let Some(anchor) = anchor else {
            self.validate_budget(&fallback)
                .map_err(|error| error.to_string())?;
            return Ok(ProviderRequestBudgetDecision {
                context_budget: fallback,
                evidence,
            });
        };
        let provider_usage = v1_usage_to_provider_usage(&anchor.usage)?;
        let Some(provider_input_tokens) =
            vulcan_context::observed_input_tokens_from_usage(Some(&provider_usage))
        else {
            self.validate_budget(&fallback)
                .map_err(|error| error.to_string())?;
            return Ok(ProviderRequestBudgetDecision {
                context_budget: fallback,
                evidence,
            });
        };
        let current_measurement = &evidence.request_text_measurement;
        let previous_measurement = &anchor.evidence.request_text_measurement;
        let previous_history_count = usize::try_from(previous_measurement.history_message_count)
            .map_err(|_| {
                String::from("The V1 Provider budget anchor history count exceeds usize.")
            })?;
        let prefix_match = if previous_history_count < snapshot.history.len() {
            let prefix_fingerprint =
                digest_serializable(&&snapshot.history[..previous_history_count])?;
            previous_measurement.prompt_in_history
                && current_measurement.prompt_in_history
                && previous_measurement.history_fingerprint == prefix_fingerprint
                && previous_measurement.prefix_invariant_fingerprint
                    == current_measurement.prefix_invariant_fingerprint
                && current_measurement.character_count >= previous_measurement.character_count
                && current_measurement.utf8_byte_count >= previous_measurement.utf8_byte_count
                && current_measurement.structural_units >= previous_measurement.structural_units
        } else {
            false
        };
        let stable_shape = anchor.projection.segment_generation == projection.segment_generation
            && anchor.projection.retained_from_seq == projection.retained_from_seq
            && anchor.evidence.model_route_fingerprint == evidence.model_route_fingerprint
            && anchor.evidence.toolset_fingerprint == evidence.toolset_fingerprint
            && anchor.evidence.system_prompt_fingerprint == evidence.system_prompt_fingerprint;
        let exact_match = stable_shape
            && anchor.evidence.request_assembly_fingerprint
                == evidence.request_assembly_fingerprint;
        let (accounted_input_tokens, accounting_source, warning_code) = if exact_match {
            (
                provider_input_tokens,
                ContextBudgetAccountingSource::ProviderReported,
                None,
            )
        } else if stable_shape && prefix_match {
            (
                provider_input_tokens.saturating_add(estimate_v1_measurement_delta_tokens(
                    previous_measurement,
                    current_measurement,
                )?),
                ContextBudgetAccountingSource::ProviderAnchorPlusCharacterDelta,
                Some("provider_anchor_character_delta"),
            )
        } else {
            let budget = ContextBudgetSnapshot {
                accounting_warning_code: Some(String::from("provider_usage_baseline_mismatch")),
                ..fallback
            };
            self.validate_budget(&budget)
                .map_err(|error| error.to_string())?;
            return Ok(ProviderRequestBudgetDecision {
                context_budget: budget,
                evidence,
            });
        };
        let anchor_id = anchor.api_call_id.value().to_string();
        let budget = build_request_context_compaction_budget_snapshot(
            RequestContextCompactionBudgetSnapshotInput {
                provider_id: self.provider_id.as_str(),
                model_id: self.model_id.as_str(),
                protocol_profile_id: self.protocol_profile_id.as_str(),
                capabilities: &self.capabilities,
                reasoning: &self.reasoning,
                request_snapshot: snapshot,
                compaction_policy: self.context_compaction_policy,
                context_budget_override: self.context_budget_override,
                observed_usage: Some(&provider_usage),
                accounted_input_tokens: Some(accounted_input_tokens),
                accounting_source,
                accounting_warning_code: warning_code,
                provider_anchor_call_id: Some(anchor_id.as_str()),
                prefill_input_tokens: self.prefill_input_tokens,
            },
        );
        self.validate_budget(&budget)
            .map_err(|error| error.to_string())?;
        Ok(ProviderRequestBudgetDecision {
            context_budget: budget,
            evidence,
        })
    }

    /// Validates one already-built budget snapshot before dispatch.
    /// 在分派前校验一份已经构建的预算快照。
    fn validate_budget(&self, budget: &ContextBudgetSnapshot) -> Result<(), VulcanCoreError> {
        // Automatic compaction is attempted before this final gate. A session planning threshold
        // must not reject a request that still fits the real model after that attempt.
        // 自动压缩先于最终准入；压缩尝试后会话规划阈值不能拒绝仍适配真实模型的请求。
        let hard_window_exceeded = budget
            .provider_request_hard_limit
            .is_some_and(|limit| budget.active_context_tokens >= limit);
        if hard_window_exceeded {
            return Err(VulcanCoreError::ContextCompactionRequired {
                message: format!(
                    "The request exceeds the context window of the selected model: {}. Choose a model with enough capacity or reduce the context.",
                    budget.summary_note()
                ),
            });
        }

        Ok(())
    }
}

/// Converts persisted V1 usage into the provider-neutral runtime accounting shape.
/// 把持久化 V1 用量转换为 Provider 中立的运行时记账结构。
fn v1_usage_to_provider_usage(
    usage: &vulcan_session_core::v1::UsageSnapshot,
) -> Result<ProviderUsageSnapshot, String> {
    Ok(ProviderUsageSnapshot {
        update_kind: Some(vulcan_ai_contract::UsageUpdateKind::Final),
        input_tokens: optional_usage_usize("usage.input_tokens", usage.input_tokens)?,
        uncached_input_tokens: optional_usage_usize(
            "usage.uncached_input_tokens",
            usage.uncached_input_tokens,
        )?,
        output_tokens: optional_usage_usize("usage.output_tokens", usage.output_tokens)?,
        reasoning_tokens: optional_usage_usize("usage.reasoning_tokens", usage.reasoning_tokens)?,
        total_tokens: optional_usage_usize("usage.total_tokens", usage.total_tokens)?,
        cached_input_tokens: optional_usage_usize(
            "usage.cache_read_tokens",
            usage.cache_read_tokens,
        )?,
        cache_creation_input_tokens: optional_usage_usize(
            "usage.cache_write_tokens",
            usage.cache_write_tokens,
        )?,
        audio_input_tokens: optional_usage_usize(
            "usage.audio_input_tokens",
            usage.audio_input_tokens,
        )?,
        audio_output_tokens: optional_usage_usize(
            "usage.audio_output_tokens",
            usage.audio_output_tokens,
        )?,
        ..ProviderUsageSnapshot::default()
    })
}

/// Converts one optional durable usage dimension into the host runtime integer domain.
/// 把一个可选持久化用量维度转换为宿主运行时整数域。
fn optional_usage_usize(field: &str, value: Option<u64>) -> Result<Option<usize>, String> {
    value
        .map(|value| {
            usize::try_from(value)
                .map_err(|_| format!("Provider budget anchor `{field}` exceeds usize."))
        })
        .transpose()
}

/// Estimates the proven append-only delta between two durable V1 request measurements.
/// 估算两份持久化 V1 请求测量之间已经证明的仅追加增量。
fn estimate_v1_measurement_delta_tokens(
    previous: &ProviderRequestTextMeasurement,
    current: &ProviderRequestTextMeasurement,
) -> Result<usize, String> {
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
    usize::try_from(
        non_ascii_upper_bound
            .saturating_add(ascii_characters.div_ceil(4))
            .saturating_add(structural_delta),
    )
    .map_err(|_| String::from("V1 Provider request delta exceeds the host usize domain."))
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
    /// Stores the same frozen guard for the unique V1 final-request preparation boundary.
    /// 为唯一 V1 最终请求准备边界保存同一份冻结守卫。
    fn set_request_budget_guard(&mut self, guard: ProviderRequestBudgetGuard);
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

    fn set_request_budget_guard(&mut self, guard: ProviderRequestBudgetGuard) {
        self.request_budget_guard = Some(guard);
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
            request.set_request_budget_guard(guard.clone());
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
                        "Request budget gate blocked sending: the gate has no budget snapshot aligned with the final request, so network dispatch was stopped.",
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

    /// Builds one exact V1 projection proof for budget-anchor tests.
    /// 为预算锚点测试构建一份精确的 V1 投影证明。
    fn v1_projection(segment_generation: u64) -> ProviderProjectionProof {
        ProviderProjectionProof {
            segment_generation,
            document_revision: 2,
            retained_from_seq: vulcan_session_core::v1::Seq::try_new(1)
                .expect("valid retained seq"),
            through_seq: vulcan_session_core::v1::Seq::try_new(3).expect("valid through seq"),
            fingerprint_sha256: "a".repeat(64),
        }
    }

    /// Builds one V1 usage anchor from the exact request evidence.
    /// 从精确请求证据构建一个 V1 用量锚点。
    fn v1_budget_anchor(
        request: &CoreProviderRequestSnapshot,
        projection: ProviderProjectionProof,
        input_tokens: u64,
    ) -> ProviderUsageBudgetAnchor {
        ProviderUsageBudgetAnchor {
            api_call_id: vulcan_session_core::v1::ApiCallId::try_new(7)
                .expect("valid API identity"),
            projection,
            evidence: build_provider_request_budget_evidence(
                request,
                vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
                "sha256:test-route",
            )
            .expect("valid request evidence"),
            usage: vulcan_session_core::v1::UsageSnapshot {
                input_tokens: Some(input_tokens),
                uncached_input_tokens: Some(input_tokens),
                output_tokens: Some(3),
                total_tokens: Some(input_tokens + 3),
                ..vulcan_session_core::v1::UsageSnapshot::default()
            },
        }
    }

    /// An exact V1 request and projection reuse only the matching durable usage pair.
    /// 精确的 V1 请求与投影只复用匹配的可靠用量事实组合。
    #[test]
    fn exact_v1_anchor_calibrates_the_final_request_budget() {
        let request = request_snapshot(Vec::new());
        let projection = v1_projection(4);
        let anchor = v1_budget_anchor(&request, projection.clone(), 77);
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
        );

        let decision = guard
            .build_v1_budget_decision(&request, &projection, Some(&anchor))
            .expect("matching V1 budget anchor must calibrate the final request");

        assert_eq!(
            decision.context_budget.accounting_source,
            ContextBudgetAccountingSource::ProviderReported
        );
        assert_eq!(decision.context_budget.active_context_tokens, 77);
        assert_eq!(
            decision.context_budget.provider_anchor_call_id.as_deref(),
            Some("7")
        );
        assert_eq!(decision.evidence, anchor.evidence);
    }

    /// A different V1 segment generation explicitly rejects durable usage reuse.
    /// 不同的 V1 分片代际会显式拒绝复用可靠用量。
    #[test]
    fn v1_generation_mismatch_falls_back_without_reusing_the_anchor() {
        let request = request_snapshot(Vec::new());
        let current_projection = v1_projection(5);
        let anchor = v1_budget_anchor(&request, v1_projection(4), 77);
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
        );

        let decision = guard
            .build_v1_budget_decision(&request, &current_projection, Some(&anchor))
            .expect("generation mismatch must use an explicit character estimate");

        assert_eq!(
            decision.context_budget.accounting_source,
            ContextBudgetAccountingSource::CharacterEstimate
        );
        assert_eq!(decision.context_budget.provider_anchor_call_id, None);
        assert_eq!(
            decision.context_budget.accounting_warning_code.as_deref(),
            Some("provider_usage_baseline_mismatch")
        );
    }

    /// Native media cannot trigger preliminary compaction by being mistaken for Base64 prompt text.
    /// 原生媒体不能因Base64被误作提示词正文而触发初步压缩。
    #[test]
    fn native_media_budget_admits_direct_and_v1_wrapped_results() {
        // A small context window would reject this fixture if media were counted as ordinary text.
        // 若把媒体计作普通正文，小型上下文窗口将拒绝本夹具。
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            Some(8_000),
            None,
        );
        // Exact durable binary content is retained while its accounting uses existing attachment metadata.
        // 保留精确持久二进制内容，同时记账使用既有附件元数据。
        let envelope = serde_json::json!({"$vulcan_native_media_input":1,"text":"Read image","byte_len":60_000,"upload":null,
            "resource":{"id":"budget-media","kind":"image","media_type":"image/png","source_kind":"inline_data","file_name":"image.png","url":format!("data:image/png;base64,{}", "YWJj".repeat(20_000))}}).to_string();
        for text in [
            envelope.clone(),
            serde_json::json!({"model_output":envelope,"host_result":null}).to_string(),
        ] {
            // Exercise both real controller representations at the same preliminary guard.
            // 在同一初步守卫处覆盖两种真实控制器表示。
            let request = request_snapshot(vec![ConversationMessage {
                id: "native-budget".into(),
                payload: ConversationPayload::ToolResult {
                    tool_call_id: Some("read".into()),
                    is_error: false,
                    text,
                },
            }]);
            // Production validates this budget before the authoritative V1 rebuild.
            // 生产流程在权威V1重建之前校验此预算。
            let budget = guard.build_budget_snapshot(&request).unwrap();
            guard.validate_budget(&budget).unwrap();
            assert!(
                serde_json::to_string(&request)
                    .unwrap()
                    .contains("$vulcan_native_media_input")
            );
        }
    }

    /// Verifies a large tool result is rechecked before the next physical provider request.
    /// 验证大型工具结果会在下一次真实 Provider 请求前重新接受预算检查。
    #[test]
    fn tool_result_growth_requires_compaction_before_next_provider_request() {
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(1_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            Some(100),
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
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
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

    /// Verifies an absolute planning line does not become a fictitious provider limit.
    /// 验证绝对规划阈值不会变成虚构的 Provider 上限。
    #[test]
    fn absolute_compaction_limit_does_not_block_unknown_model_window() {
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
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
        assert_eq!(budget.provider_request_hard_limit, None);
        guard
            .validate_budget(&budget)
            .expect("a planning line cannot reject a request without a declared model limit");
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
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
        )
        .expect("embedded request evidence should build");
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
        );
        let embedded_budget = guard
            .build_budget_snapshot(&request)
            .expect("embedded prompt budget should build");

        let mut distinct_request = request;
        distinct_request.prompt_in_history = false;
        let distinct_evidence = build_provider_request_budget_evidence(
            &distinct_request,
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
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
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
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
        let guard = ProviderRequestBudgetGuard::new_v1(
            "test-provider",
            "test-model",
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            "sha256:test-route",
            ModelCapabilities {
                context_limit: Some(10_000),
                ..ModelCapabilities::default()
            },
            ProviderRuntimeReasoningResolution::default(),
            Some(ContextCompactionPolicy::default()),
            None,
            None,
        );

        let error = guard
            .build_budget_snapshot(&request)
            .expect_err("deep tool schemas must fail before dispatch");

        assert!(error.contains("JSON depth limit"));
    }
}
