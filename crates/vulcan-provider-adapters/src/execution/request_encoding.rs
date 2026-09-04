//! Deterministic provider request encoding and final wire-contract validation.
//! 确定性的供应商请求编码与最终线路契约校验。

use super::projection_profiles::{apply_mapping_step, apply_request_projection_profiles};
use super::route::{route_uses_native_codex_responses, upstream_protocol_for_route};
use super::{
    ProviderExecutionClient, ProviderExecutionFailure, ProviderExecutionRoute, pre_dispatch_failure,
};
use crate::{
    CredentialExecutionSecret, CustomModelRequestProjection, RequestProjectorKind,
    SAMPLING_TEMPERATURE_PARAMETER_ID, SecondaryRouteSelection,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiExecutionRequest, ContentItem, ModelParameterValue, ReasoningEffort, StructuredOutputPolicy,
};
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_PROFILE_ID, ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
    CLAUDE_CODE_MESSAGES_PROFILE_ID, COHERE_CHAT_PROFILE_ID, GEMINI_GENERATE_CONTENT_PROFILE_ID,
    GEMINI_INTERACTIONS_PROFILE_ID, GOOGLE_CODE_ASSIST_PROFILE_ID, OPENAI_CHAT_PROFILE_ID,
    OPENAI_CODEX_RESPONSES_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
    gemini_replay_signature_payload, wrap_openai_reasoning_signature,
};
use vulcan_llm_core::{
    AnthropicReasoningMode, EncodedWireRequest, ProviderRouteContext, UpstreamProtocol,
    encode_internal_responses_request, project_internal_request_for_provider,
    validate_target_media_projection,
};

impl ProviderExecutionClient {
    /// Encodes one immutable execution request without dispatching it.
    /// 编码一个不可变执行请求，但不进行网络分派。
    ///
    /// # Parameters
    /// 参数
    /// `route` is the frozen provider route and `request` is the provider-neutral request.
    /// `route` 是冻结的供应商路由，`request` 是供应商无关请求。
    ///
    /// # Returns
    /// 返回值
    /// The exact projected wire request or a controlled validation failure.
    /// 返回精确投影的线级请求或受控校验失败。
    pub fn encode_request(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        validate_request_policy(route, request)?;
        validate_request_projection_guards_before_encoding(route, request)?;
        // Internal media stays typed until the exact target profile projects every resource.
        // 内部媒体保持强类型，直到精确目标 Profile 投影全部资源。
        let encoded = project_internal_baseline_for_provider(route, request)?;
        project_provider_request(route, request, encoded)
    }

    /// Proves that one exact encoded request preserves the caller-required no-reasoning contract.
    /// 证明一份精确编码请求保留了调用方要求的无思考契约。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` are the frozen execution inputs; `model_is_fixed_off` is the exact
    /// selected-model capability fact rather than a model-name inference.
    /// `route` 与 `request` 是冻结的执行输入；`model_is_fixed_off` 是所选模型的精确能力事实，
    /// 而不是根据模型名称推断的结果。
    ///
    /// # Returns
    /// 返回值
    /// The encoded request when it cannot enable reasoning and either carries an explicit
    /// protocol-level disable projection or targets a model declared permanently reasoning-off.
    /// 当编码请求不会开启思考，且携带协议级显式关闭投影或目标模型被声明为永久关闭思考时，
    /// 返回该编码请求。
    pub fn encode_required_disabled_reasoning_request(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        model_is_fixed_off: bool,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        if request.reasoning_effort != Some(ReasoningEffort::None) {
            return Err(pre_dispatch_failure(
                "Required disabled-reasoning validation received a request without reasoning effort none.",
            ));
        }
        let encoded = self.encode_request(route, request)?;
        validate_required_disabled_reasoning_wire(route, &encoded, model_is_fixed_off)?;
        Ok(encoded)
    }

    /// Encodes one immutable request with credential-bound non-secret identity projection.
    /// 编码一个不可变请求，并投影凭据绑定的非秘密身份信息。
    ///
    /// # Parameters
    /// 参数
    /// `route` and `request` form the public route projection; `secret` supplies only the
    /// account metadata required in the wire body.
    /// `route` 与 `request` 构成公共路由投影；`secret` 仅提供线级请求体所需的账号元数据。
    ///
    /// # Returns
    /// 返回值
    /// The exact request body used by dispatch and wire preview, without authentication headers.
    /// 返回分派与线级预览共同使用的精确请求体，不包含认证请求头。
    pub fn encode_request_with_credential(
        &self,
        route: &ProviderExecutionRoute,
        request: &AiExecutionRequest,
        secret: &CredentialExecutionSecret,
    ) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
        let encoded = self.encode_request(route, request)?;
        project_credential_request(route, secret, encoded)
    }
}

/// Reports whether one route's manifest request profiles reference a model parameter.
/// 报告一个路由的 Manifest 请求 Profile 是否引用某模型参数。
///
/// A closed step references a parameter either through a `CopyRequestField` field value or
/// through a request-field source path with the `@model_parameters.<id>` form (plan §R8.5).
/// 封闭步骤通过 `CopyRequestField` 的字段值或 `@model_parameters.<id>` 形式的请求字段
/// 源路径引用参数（方案 §R8.5）。
fn request_projection_profiles_cover_parameter(
    route: &ProviderExecutionRoute,
    parameter_id: &str,
) -> bool {
    let expected_field = format!("model_parameters.{parameter_id}");
    let expected_source = format!("@model_parameters.{parameter_id}");
    route.request_projection_profiles.iter().any(|profile| {
        profile.steps.iter().any(|step| match step {
            crate::provider_manifest::MappingStep::CopyRequestField { field, .. } => {
                *field == expected_field
            }
            crate::provider_manifest::MappingStep::EnumMap { source_path, .. }
            | crate::provider_manifest::MappingStep::ConditionalSet { source_path, .. }
            | crate::provider_manifest::MappingStep::DeleteIfEquals { source_path, .. }
            | crate::provider_manifest::MappingStep::DeleteUnlessEquals { source_path, .. }
            | crate::provider_manifest::MappingStep::RejectIfPresent { source_path, .. } => {
                *source_path == expected_source
            }
            crate::provider_manifest::MappingStep::RejectIfAllPresent { conditions, .. } => {
                conditions
                    .iter()
                    .any(|condition| condition.source_path == expected_source)
            }
            _ => false,
        })
    })
}

/// Applies route-scoped request policy before encoding.
/// 在编码前应用路由作用域请求策略。
pub(super) fn validate_request_policy(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Result<(), ProviderExecutionFailure> {
    for parameter_id in request.model_parameters.keys() {
        // Temperature is a protocol-neutral host primitive; provider-specific admissibility and
        // conditional omission are declared by the effective model Profile.
        // Temperature 是协议无关的宿主原语；供应商特定的可用性与条件省略由生效模型
        // Profile 声明。
        let registered_temperature_route = parameter_id == SAMPLING_TEMPERATURE_PARAMETER_ID;
        let registered_custom_parameter = route
            .custom_model_projection
            .as_ref()
            .is_some_and(|projection| projection.model_parameter_paths.contains_key(parameter_id));
        // Manifest request profiles register the model parameters their closed steps reference;
        // the legacy per-provider projector gate is superseded by the profile chain (plan §R8.5).
        // Manifest 请求 Profile 注册其封闭步骤引用的模型参数；旧版逐供应商投影器门槛被
        // Profile 链取代（方案 §R8.5）。
        let registered_by_profile =
            request_projection_profiles_cover_parameter(route, parameter_id);
        if !registered_temperature_route && !registered_custom_parameter && !registered_by_profile {
            return Err(pre_dispatch_failure(format!(
                "Model-specific parameter `{parameter_id}` has no registered route projector."
            )));
        }
    }
    Ok(())
}

/// Evaluates request-field-only manifest guards before the canonical protocol encoder runs.
/// 在规范协议编码器运行前执行仅依赖请求字段的 Manifest 守卫。
///
/// Provider profiles may declare a more specific endpoint restriction than the provider-neutral
/// baseline encoder. Running only pure request-field guards here preserves that configuration
/// authority; guards that depend on the encoded body remain deferred to normal projection.
/// 供应商 Profile 可以声明比供应商无关基线编码器更具体的端点限制。此处仅提前执行纯请求字段
/// 守卫，以保留配置权威；依赖编码正文的守卫仍延后到常规投影阶段。
///
/// # Parameters
/// 参数
/// `route` is the frozen route and `request` is the immutable provider-neutral request.
/// `route` 是冻结路由，`request` 是不可变的供应商无关请求。
///
/// # Returns
/// 返回值
/// Success when every eligible guard accepts the request, otherwise its manifest-owned failure.
/// 所有可提前执行的守卫均接受请求时返回成功，否则返回其 Manifest 自有失败。
pub(super) fn validate_request_projection_guards_before_encoding(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Result<(), ProviderExecutionFailure> {
    let mut preflight = EncodedWireRequest {
        path: String::from("/manifest-request-guard-preflight"),
        headers: BTreeMap::new(),
        body: Value::Object(Default::default()),
    };
    for profile in &route.request_projection_profiles {
        for step in &profile.steps {
            let request_field_only = match step {
                crate::provider_manifest::MappingStep::RejectIfPresent { source, .. } => {
                    *source == crate::provider_manifest::MappingStepSource::RequestField
                }
                crate::provider_manifest::MappingStep::RejectIfAllPresent {
                    conditions, ..
                } => conditions.iter().all(|condition| {
                    condition.source == crate::provider_manifest::MappingStepSource::RequestField
                }),
                _ => false,
            };
            if request_field_only {
                apply_mapping_step(request, &mut preflight, step)?;
            }
        }
    }
    Ok(())
}

/// Encodes typed `request` once and projects its complete media contract for the frozen `route`.
/// 对强类型 `request` 只编码一次，为冻结 `route` 投影其完整媒体契约。
/// Returns the target wire request or a pre-dispatch failure, without protocol fallback.
/// 返回目标线路请求或分派前失败，不执行协议回退。
fn project_internal_baseline_for_provider(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    // Exact protocol selection occurs before any encoding; there is no standard-Responses preflight.
    // 精确协议选择先于任意编码，不再经过标准 Responses 前置编码。
    let Some(upstream_protocol) = upstream_protocol_for_route(route) else {
        return Err(pre_dispatch_failure(format!(
            "No Responses request converter is registered for protocol `{}`; dispatch was not attempted.",
            route.protocol_route.protocol_profile_id
        )));
    };
    // This immutable clone retains canonical history while adapting only target-owned replay evidence.
    // 此不可变输入的克隆保留规范历史，仅适配目标持有的重放证据。
    let mut canonical_request = request.clone();
    if !route_uses_native_codex_responses(route) {
        // Other targets replay full history; they cannot reuse the Codex wire cursor.
        // 其他目标重放完整历史，不能复用 Codex 线路游标。
        canonical_request.continuation = None;
    }
    prepare_target_reasoning_replay_for_responses(&mut canonical_request, upstream_protocol);
    // A non-serializable internal owner preserves audio/video and all original resource fields.
    // 不可序列化的内部所有者保留音视频及全部原始资源字段。
    let mut internal = encode_internal_responses_request(&canonical_request).map_err(|error| {
        pre_dispatch_failure(format!("LLM core request encoding failed: {error}"))
    })?;
    attach_target_tool_replay_to_responses(internal.body_mut(), request, upstream_protocol)?;
    // The declared projector, not provider name inference, owns Anthropic reasoning translation.
    // 已声明投影器持有 Anthropic 思考转换，不根据供应商名称推断。
    let context = ProviderRouteContext {
        upstream_protocol,
        anthropic_reasoning_mode: if route.entry.request_projector == RequestProjectorKind::Direct {
            AnthropicReasoningMode::NativeBudget
        } else {
            AnthropicReasoningMode::ProviderSpecific
        },
    };
    project_internal_request_for_provider(
        internal,
        &context,
        route.protocol_route.protocol_profile_id.as_str(),
    )
    .map_err(|error| pre_dispatch_failure(format!("LLM provider conversion failed: {error}")))
}

/// Rewraps same-provider Gemini reasoning signatures for the internal Responses conversion body.
/// 为内部 Responses 转换正文重新封装同供应商 Gemini 推理签名。
///
/// # Parameters
/// 参数
/// `request` is the conversion-only clone and `upstream_protocol` selects whether Gemini replay
/// metadata is meaningful for the frozen target route.
/// `request` 是仅供转换使用的克隆，`upstream_protocol` 决定 Gemini 重放元数据是否适用于冻结目标路由。
fn prepare_target_reasoning_replay_for_responses(
    request: &mut AiExecutionRequest,
    upstream_protocol: UpstreamProtocol,
) {
    if !matches!(
        upstream_protocol,
        UpstreamProtocol::GeminiGenerateContent
            | UpstreamProtocol::GeminiInteractions
            | UpstreamProtocol::GoogleCodeAssist
    ) {
        return;
    }
    for message in &mut request.messages {
        for item in &mut message.content {
            let ContentItem::Reasoning { signature, .. } = item else {
                continue;
            };
            let Some(raw_signature) = signature
                .as_deref()
                .and_then(gemini_replay_signature_payload)
            else {
                continue;
            };
            *signature = wrap_openai_reasoning_signature(raw_signature);
        }
    }
}

/// Attaches same-provider Gemini function-call signatures to internal Responses items.
/// 将同供应商 Gemini 函数调用签名附加到内部 Responses Item。
///
/// # Parameters
/// 参数
/// `canonical_body` is never dispatched directly, `request` owns signed calls, and
/// `upstream_protocol` is the exact target route protocol.
/// `canonical_body` 不会被直接分派，`request` 持有已签名调用，`upstream_protocol` 是精确目标路由协议。
///
/// # Returns
/// 返回值
/// Success after every matching call receives a bounded internal extension, or a controlled
/// conversion failure when the canonical body cannot preserve its call identity.
/// 每个匹配调用收到有界内部扩展后返回成功；若规范正文无法保留调用身份，则返回受控转换失败。
fn attach_target_tool_replay_to_responses(
    canonical_body: &mut Value,
    request: &AiExecutionRequest,
    upstream_protocol: UpstreamProtocol,
) -> Result<(), ProviderExecutionFailure> {
    if !matches!(
        upstream_protocol,
        UpstreamProtocol::GeminiGenerateContent | UpstreamProtocol::GoogleCodeAssist
    ) {
        return Ok(());
    }
    let mut signatures = BTreeMap::new();
    for message in &request.messages {
        for item in &message.content {
            let ContentItem::ToolCall { call } = item else {
                continue;
            };
            if let Some(signature) = call
                .signature
                .as_deref()
                .and_then(gemini_replay_signature_payload)
            {
                signatures.insert(call.id.as_str(), signature);
            }
        }
    }
    if signatures.is_empty() {
        return Ok(());
    }
    let input = canonical_body
        .get_mut("input")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| {
            pre_dispatch_failure(
                "Responses conversion body is missing the input array required for Gemini signed calls.",
            )
        })?;
    for item in input {
        let Some(call_id) = item.get("call_id").and_then(Value::as_str) else {
            continue;
        };
        let Some(signature) = signatures.remove(call_id) else {
            continue;
        };
        item["x_vulcan_provider_signature"] = Value::String(signature.to_string());
    }
    if let Some((call_id, _)) = signatures.into_iter().next() {
        return Err(pre_dispatch_failure(format!(
            "Responses conversion body did not preserve signed Gemini function call `{call_id}`."
        )));
    }
    Ok(())
}

/// Applies provider-only fields after protocol encoding.
/// 在协议编码后应用仅供应商字段。
pub(super) fn project_provider_request(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if let Some(endpoint_path) = route.protocol_route.endpoint_path.as_ref() {
        encoded.path = endpoint_path.clone();
    }
    project_sampling_temperature(request, &mut encoded)?;
    // The ordered Manifest Profile chain is the only provider/model-specific request mapper.
    // Empty chains mean that baseline protocol encoding needs no vendor mutation and never
    // reactivate a hidden code-owned provider projector (plan §R8.5/§R10).
    // 有序 Manifest Profile 链是唯一的供应商/模型特定请求映射器。空链表示协议基线无需
    // 供应商变换，绝不会重新启用隐藏的代码持有供应商 projector（方案 §R8.5/§R10）。
    apply_request_projection_profiles(route, request, &mut encoded)?;
    project_secondary_route(route, &mut encoded)?;
    if let Some(projection) = route.custom_model_projection.as_ref() {
        project_custom_model_request(projection, request, &mut encoded)?;
    }
    validate_structured_output_projection(route, request, &encoded)?;
    // Provider mappings cannot reintroduce internal media or broaden the selected protocol family.
    // 供应商映射不能重新引入内部媒体或扩大选中协议族的能力。
    validate_target_media_projection(
        route.protocol_route.protocol_profile_id.as_str(),
        &encoded.body,
    )
    .map_err(|error| pre_dispatch_failure(format!("Final media projection is invalid: {error}")))?;
    Ok(encoded)
}

/// Validates the final provider body against the closed no-reasoning wire vocabulary.
/// 根据封闭的无思考线路词汇校验最终供应商请求体。
///
/// # Parameters
/// 参数
/// `route` identifies the exact protocol for diagnostics; `encoded` is the final body after every
/// manifest and custom projection; `model_is_fixed_off` comes from the frozen model capability.
/// `route` 为诊断提供精确协议；`encoded` 是全部 Manifest 与自定义投影完成后的最终请求体；
/// `model_is_fixed_off` 来自冻结模型能力。
///
/// # Returns
/// 返回值
/// Success only when no enabled-reasoning control survives and a switchable model retains one
/// recognized explicit disable projection.
/// 仅当请求中没有残留开启思考的控制项，且可切换模型保留一个已识别的显式关闭投影时返回成功。
fn validate_required_disabled_reasoning_wire(
    route: &ProviderExecutionRoute,
    encoded: &EncodedWireRequest,
    model_is_fixed_off: bool,
) -> Result<(), ProviderExecutionFailure> {
    if wire_body_explicitly_enables_reasoning(&encoded.body) {
        return Err(pre_dispatch_failure(format!(
            "Protocol `{}` produced a final request that enables reasoning for a disabled-reasoning execution.",
            route.protocol_route.protocol_profile_id
        )));
    }
    if model_is_fixed_off || wire_body_explicitly_disables_reasoning(route, &encoded.body) {
        return Ok(());
    }
    Err(pre_dispatch_failure(format!(
        "Protocol `{}` does not preserve an explicit disabled-reasoning projection in the final request.",
        route.protocol_route.protocol_profile_id
    )))
}

/// Reports whether a final JSON body contains the explicit no-reasoning control registered for its exact protocol.
/// 判断最终 JSON 请求体是否包含其精确协议已注册的显式无思考控制项。
///
/// # Parameters
/// 参数
/// `route` owns the immutable protocol identity and `body` is the exact post-projection provider request body.
/// `route` 持有不可变协议身份，`body` 是完成投影后的精确供应商请求体。
///
/// # Returns
/// 返回值
/// `true` only when the field belongs to the selected protocol family; an irrelevant dialect can never satisfy the proof.
/// 仅当字段属于所选协议族时返回 `true`；无关协议方言绝不能满足该证明。
fn wire_body_explicitly_disables_reasoning(route: &ProviderExecutionRoute, body: &Value) -> bool {
    match upstream_protocol_for_route(route) {
        Some(UpstreamProtocol::OpenAiResponses) => {
            json_pointer_is_disabled_label(body, "/reasoning/effort")
        }
        Some(UpstreamProtocol::OpenAiChatCompletions) => {
            chat_body_explicitly_disables_reasoning(route, body)
        }
        Some(UpstreamProtocol::AnthropicMessages) => {
            json_pointer_is_disabled_label(body, "/thinking/type")
        }
        Some(UpstreamProtocol::GeminiGenerateContent) => {
            json_pointer_is_numeric_zero(body, "/generationConfig/thinkingConfig/thinkingBudget")
        }
        Some(UpstreamProtocol::GoogleCodeAssist) => json_pointer_is_numeric_zero(
            body,
            "/request/generationConfig/thinkingConfig/thinkingBudget",
        ),
        // The registered Interactions encoder exposes only positive thinking levels. Until the
        // protocol owns an explicit off value, switchable models must fail closed.
        // 已注册的 Interactions 编码器只暴露正向思考档位；在协议持有显式关闭值前，
        // 可切换模型必须采用失败关闭策略。
        Some(UpstreamProtocol::GeminiInteractions) | None => false,
    }
}

/// Reports whether one Chat-compatible final body preserves its frozen route's declared disabled-reasoning projection.
/// 判断一个 Chat 兼容最终请求体是否保留其冻结路由声明的关闭思考投影。
///
/// # Parameters
/// 参数
/// `route` owns the exact ordered Manifest projection chain and `body` is its final output.
/// `route` 持有精确的有序 Manifest 投影链，`body` 是该链的最终输出。
///
/// # Returns
/// 返回值
/// `true` only when a value derived from `reasoning_effort=none` survives at the exact path,
/// or the active profile explicitly declares that `none` deletes that path; unrelated vendor
/// dialects cannot satisfy the proof.
/// 仅当由 `reasoning_effort=none` 派生的值保留在精确路径，或当前 Profile 显式声明
/// `none` 会删除该路径时返回 `true`；无关供应商方言无法满足该证明。
fn chat_body_explicitly_disables_reasoning(route: &ProviderExecutionRoute, body: &Value) -> bool {
    use crate::provider_manifest::{MappingStep, MappingStepSource};

    // A retained value declaration remains authoritative over cleanup-only deletion steps.
    // 保留值声明优先于仅用于清理的删除步骤，避免把清理动作误判为关闭契约。
    let mut declared_value_projection = false;
    // A deletion can prove omission semantics only when no retained off value is declared.
    // 仅在未声明保留式关闭值时，删除动作才能证明省略字段语义。
    let mut declared_disabled_deletion = false;
    // The deletion proof is satisfied only when its exact target is absent from the final body.
    // 仅当精确删除目标未出现在最终请求体中时，删除证明才成立。
    let mut matched_disabled_deletion = false;
    for step in route
        .request_projection_profiles
        .iter()
        .flat_map(|profile| profile.steps.iter())
    {
        let expected = match step {
            MappingStep::CopyRequestField { path, field } if field == "reasoning_effort" => {
                Some((path.as_str(), Value::String(String::from("none"))))
            }
            MappingStep::EnumMap {
                path,
                source: MappingStepSource::RequestField,
                source_path,
                mapping,
                ..
            } if source_path == "@reasoning_effort" => mapping
                .get("none")
                .filter(|value| !value.is_null())
                .cloned()
                .map(|value| (path.as_str(), value)),
            MappingStep::ConditionalSet {
                path,
                source: MappingStepSource::RequestField,
                source_path,
                equals,
                then,
                otherwise,
            } if source_path == "@reasoning_effort" => {
                let value = if equals == "none" { then } else { otherwise };
                (!value.is_null()).then(|| (path.as_str(), value.clone()))
            }
            MappingStep::DeleteIfEquals {
                path,
                source: MappingStepSource::RequestField,
                source_path,
                equals,
            } if source_path == "@reasoning_effort" && equals == "none" => {
                declared_disabled_deletion = true;
                matched_disabled_deletion |= body.pointer(path).is_none();
                None
            }
            _ => None,
        };
        let Some((path, expected)) = expected else {
            continue;
        };
        declared_value_projection = true;
        if body.pointer(path) == Some(&expected) {
            return true;
        }
    }

    if declared_value_projection {
        return false;
    }
    matched_disabled_deletion
        || (!declared_disabled_deletion
            && json_pointer_is_disabled_label(body, "/reasoning_effort"))
}

/// Reports whether a final JSON body contains a contradictory enabled-reasoning control.
/// 判断最终 JSON 请求体是否包含相互矛盾的开启思考控制项。
///
/// # Parameters
/// 参数
/// `body` is the exact post-projection provider request body.
/// `body` 是完成投影后的精确供应商请求体。
///
/// # Returns
/// 返回值
/// `true` when a registered reasoning field explicitly enables work or carries any non-zero budget.
/// 已注册思考字段显式开启工作或携带任意非零预算时返回 `true`。
fn wire_body_explicitly_enables_reasoning(body: &Value) -> bool {
    ["/reasoning/effort", "/reasoning_effort"]
        .into_iter()
        .filter_map(|path| body.pointer(path).and_then(Value::as_str))
        .any(|value| !disabled_reasoning_label(value))
        || body.pointer("/reasoning/enabled").and_then(Value::as_bool) == Some(true)
        || body
            .pointer("/thinking/type")
            .and_then(Value::as_str)
            .is_some_and(|value| !disabled_reasoning_label(value))
        || body.pointer("/enable_thinking").and_then(Value::as_bool) == Some(true)
        || [
            "/generationConfig/thinkingConfig/thinkingBudget",
            "/generation_config/thinking_budget",
            "/thinkingConfig/thinkingBudget",
        ]
        .into_iter()
        .filter_map(|path| body.pointer(path).and_then(Value::as_i64))
        .any(|value| value != 0)
        || [
            "/reasoning/max_tokens",
            "/reasoning/budget_tokens",
            "/thinking/budget_tokens",
            "/reasoning_budget",
            "/thinking_budget",
        ]
        .into_iter()
        .any(|path| json_pointer_is_numeric_nonzero(body, path))
        || [
            "/generationConfig/thinkingConfig/thinkingLevel",
            "/generation_config/thinking_level",
        ]
        .into_iter()
        .filter_map(|path| body.pointer(path).and_then(Value::as_str))
        .any(|value| !disabled_reasoning_label(value))
}

/// Reports whether one JSON pointer resolves to a registered disabled-reasoning label.
/// 判断一个 JSON 指针是否解析为已注册的关闭思考标签。
///
/// # Parameters
/// 参数
/// `body` is the final provider body and `path` is one code-owned JSON pointer.
/// `body` 是最终供应商请求体，`path` 是代码持有的 JSON 指针。
///
/// # Returns
/// 返回值
/// `true` when the pointed string is `none`, `off`, or `disabled`, case-insensitively.
/// 指向的字符串不区分大小写等于 `none`、`off` 或 `disabled` 时返回 `true`。
fn json_pointer_is_disabled_label(body: &Value, path: &str) -> bool {
    body.pointer(path)
        .and_then(Value::as_str)
        .is_some_and(disabled_reasoning_label)
}

/// Reports whether one reasoning label belongs to the closed disabled vocabulary.
/// 判断一个思考标签是否属于封闭的关闭词汇。
///
/// # Parameters
/// 参数
/// `value` is one provider wire string.
/// `value` 是一个供应商线路字符串。
///
/// # Returns
/// 返回值
/// `true` only for `none`, `off`, or `disabled` after ASCII case folding.
/// 仅在 ASCII 大小写折叠后为 `none`、`off` 或 `disabled` 时返回 `true`。
fn disabled_reasoning_label(value: &str) -> bool {
    matches!(
        value.to_ascii_lowercase().as_str(),
        "none" | "off" | "disabled"
    )
}

/// Reports whether one JSON pointer resolves to an exact numeric zero.
/// 判断一个 JSON 指针是否解析为精确数值零。
///
/// # Parameters
/// 参数
/// `body` is the final provider body and `path` is one code-owned JSON pointer.
/// `body` 是最终供应商请求体，`path` 是代码持有的 JSON 指针。
///
/// # Returns
/// 返回值
/// `true` for integer or floating-point zero and `false` for missing or non-numeric values.
/// 整数或浮点数零返回 `true`；缺失或非数值返回 `false`。
fn json_pointer_is_numeric_zero(body: &Value, path: &str) -> bool {
    body.pointer(path).is_some_and(|value| {
        value.as_i64() == Some(0)
            || value.as_u64() == Some(0)
            || value.as_f64().is_some_and(|number| number == 0.0)
    })
}

/// Reports whether one JSON pointer resolves to any non-zero numeric reasoning budget.
/// 判断一个 JSON 指针是否解析为任意非零数值思考预算。
///
/// # Parameters
/// 参数
/// `body` is the final provider body and `path` is one code-owned JSON pointer.
/// `body` 是最终供应商请求体，`path` 是一个代码持有的 JSON 指针。
///
/// # Returns
/// 返回值
/// `true` for positive or negative non-zero integers and floats; missing or zero values are false.
/// 正数或负数的非零整数与浮点数返回 `true`；缺失或零值返回 `false`。
fn json_pointer_is_numeric_nonzero(body: &Value, path: &str) -> bool {
    body.pointer(path).is_some_and(|value| {
        value.as_i64().is_some_and(|number| number != 0)
            || value.as_u64().is_some_and(|number| number != 0)
            || value.as_f64().is_some_and(|number| number != 0.0)
    })
}

/// Projects an explicitly frozen secondary endpoint through the registered order primitive.
/// 通过已注册的顺序原语投影一条显式冻结的二级端点。
///
/// # Parameters
/// 参数
/// `route` carries the immutable secondary selection and `encoded` is the protocol body.
/// `route` 承载不可变二级选择，`encoded` 是协议请求体。
///
/// # Returns
/// 返回值
/// Success for automatic routing or one non-conflicting pinned endpoint.
/// 自动路由或一条不冲突的固定端点返回成功。
fn project_secondary_route(
    route: &ProviderExecutionRoute,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let SecondaryRouteSelection::PinnedEndpoint { route_id, .. } = &route.secondary_route else {
        return Ok(());
    };
    let body = request_body_mut(encoded, "Secondary route")?;
    if body.contains_key("provider") {
        return Err(pre_dispatch_failure(
            "Secondary-route projection conflicts with an existing provider field.",
        ));
    }
    body.insert(
        String::from("provider"),
        json!({
            "order": [route_id],
            "allow_fallbacks": false,
        }),
    );
    Ok(())
}

/// Verifies protocol-native JSON Schema fields survive provider-specific request projection.
/// 验证协议原生 JSON Schema 字段在供应商专属请求投影后仍然存在。
///
/// # Parameters
/// 参数
/// `route` identifies the exact protocol, `request` carries the requested schema, and `encoded`
/// is the final request after all provider and custom projections.
/// `route` 标识精确协议，`request` 携带请求 Schema，`encoded` 是完成全部供应商与自定义投影后的最终请求。
///
/// # Returns
/// 返回值
/// Success when the protocol carries the exact requested schema, or a pre-dispatch failure when
/// a supported Chat/Responses/Messages projection dropped or changed it.
/// 当协议携带精确请求 Schema 时返回成功；支持的 Chat、Responses、Messages 投影丢失或改写 Schema 时返回分派前失败。
fn validate_structured_output_projection(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
    encoded: &EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let protocol_id = route.protocol_route.protocol_profile_id.as_str();
    match &request.structured_output {
        StructuredOutputPolicy::None => Ok(()),
        StructuredOutputPolicy::JsonObject => {
            let projected_type = match protocol_id {
                OPENAI_CHAT_PROFILE_ID | COHERE_CHAT_PROFILE_ID => encoded
                    .body
                    .pointer("/response_format/type")
                    .and_then(Value::as_str),
                OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => encoded
                    .body
                    .pointer("/text/format/type")
                    .and_then(Value::as_str),
                GEMINI_GENERATE_CONTENT_PROFILE_ID => encoded
                    .body
                    .pointer("/generationConfig/responseMimeType")
                    .and_then(Value::as_str),
                GOOGLE_CODE_ASSIST_PROFILE_ID => encoded
                    .body
                    .pointer("/request/generationConfig/responseMimeType")
                    .and_then(Value::as_str),
                GEMINI_INTERACTIONS_PROFILE_ID => encoded
                    .body
                    .pointer("/response_format/mime_type")
                    .and_then(Value::as_str),
                _ => return Ok(()),
            };
            let expected_type = match protocol_id {
                OPENAI_CHAT_PROFILE_ID
                | OPENAI_RESPONSES_PROFILE_ID
                | OPENAI_CODEX_RESPONSES_PROFILE_ID
                | COHERE_CHAT_PROFILE_ID => "json_object",
                GEMINI_GENERATE_CONTENT_PROFILE_ID
                | GOOGLE_CODE_ASSIST_PROFILE_ID
                | GEMINI_INTERACTIONS_PROFILE_ID => "application/json",
                _ => return Ok(()),
            };
            if projected_type == Some(expected_type) {
                return Ok(());
            }
            Err(pre_dispatch_failure(format!(
                "Protocol `{protocol_id}` dropped or changed the requested structured-output JSON Object mode.",
            )))
        }
        StructuredOutputPolicy::JsonSchema { schema, .. } => {
            let (projected_type, projected_schema, expected_type) = match protocol_id {
                ANTHROPIC_MESSAGES_PROFILE_ID
                | ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
                | CLAUDE_CODE_MESSAGES_PROFILE_ID => (
                    encoded
                        .body
                        .pointer("/output_config/format/type")
                        .and_then(Value::as_str),
                    encoded.body.pointer("/output_config/format/schema"),
                    "json_schema",
                ),
                OPENAI_CHAT_PROFILE_ID => (
                    encoded
                        .body
                        .pointer("/response_format/type")
                        .and_then(Value::as_str),
                    encoded.body.pointer("/response_format/json_schema/schema"),
                    "json_schema",
                ),
                OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID => (
                    encoded
                        .body
                        .pointer("/text/format/type")
                        .and_then(Value::as_str),
                    encoded.body.pointer("/text/format/schema"),
                    "json_schema",
                ),
                GEMINI_GENERATE_CONTENT_PROFILE_ID => (
                    encoded
                        .body
                        .pointer("/generationConfig/responseMimeType")
                        .and_then(Value::as_str),
                    encoded.body.pointer("/generationConfig/responseJsonSchema"),
                    "application/json",
                ),
                GOOGLE_CODE_ASSIST_PROFILE_ID => (
                    encoded
                        .body
                        .pointer("/request/generationConfig/responseMimeType")
                        .and_then(Value::as_str),
                    encoded
                        .body
                        .pointer("/request/generationConfig/responseJsonSchema"),
                    "application/json",
                ),
                COHERE_CHAT_PROFILE_ID => (
                    encoded
                        .body
                        .pointer("/response_format/type")
                        .and_then(Value::as_str),
                    encoded.body.pointer("/response_format/schema"),
                    "json_object",
                ),
                GEMINI_INTERACTIONS_PROFILE_ID => (
                    encoded
                        .body
                        .pointer("/response_format/mime_type")
                        .and_then(Value::as_str),
                    encoded.body.pointer("/response_format/schema"),
                    "application/json",
                ),
                _ => return Ok(()),
            };
            if projected_type == Some(expected_type) && projected_schema == Some(schema) {
                return Ok(());
            }
            Err(pre_dispatch_failure(format!(
                "Protocol `{protocol_id}` dropped or changed the requested structured-output JSON Schema.",
            )))
        }
    }
}

/// Applies one validated custom model projection after standard protocol encoding.
/// 在标准协议编码后应用一个已校验的自定义模型投影。
fn project_custom_model_request(
    projection: &CustomModelRequestProjection,
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    for (path, value) in &projection.defaults {
        set_json_path(&mut encoded.body, path, value.clone(), false)?;
    }
    for (parameter_id, path) in &projection.model_parameter_paths {
        if let Some(value) = request.model_parameters.get(parameter_id) {
            let value = model_parameter_json_value(value)?;
            set_json_path(&mut encoded.body, path, value, true)?;
        }
    }
    if let Some(reasoning_effort) = request.reasoning_effort
        && let Some(mutation) = projection.reasoning.get(&reasoning_effort)
    {
        for path in &mutation.delete {
            delete_json_path(&mut encoded.body, path)?;
        }
        for (path, value) in &mutation.set {
            set_json_path(&mut encoded.body, path, value.clone(), true)?;
        }
    }
    for path in &projection.delete {
        delete_json_path(&mut encoded.body, path)?;
    }
    for (path, value) in &projection.overrides {
        set_json_path(&mut encoded.body, path, value.clone(), true)?;
    }
    Ok(())
}

/// Converts one strongly typed model value into its provider JSON value.
/// 将一个强类型模型值转换为供应商 JSON 值。
fn model_parameter_json_value(
    value: &ModelParameterValue,
) -> Result<Value, ProviderExecutionFailure> {
    match value {
        ModelParameterValue::Boolean(value) => Ok(Value::Bool(*value)),
        ModelParameterValue::Integer(value) => Ok(Value::Number((*value).into())),
        ModelParameterValue::Decimal(value) => serde_json::from_str(value).map_err(|error| {
            pre_dispatch_failure(format!(
                "Custom decimal model parameter `{value}` is invalid JSON: {error}"
            ))
        }),
        ModelParameterValue::String(value) => Ok(Value::String(value.clone())),
        ModelParameterValue::Array(values) => values
            .iter()
            .map(model_parameter_json_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array),
        ModelParameterValue::Object(values) => values
            .iter()
            .map(|(key, value)| Ok((key.clone(), model_parameter_json_value(value)?)))
            .collect::<Result<serde_json::Map<_, _>, ProviderExecutionFailure>>()
            .map(Value::Object),
    }
}

/// Sets one validated dot-separated object path.
/// 设置一条已校验的点分隔对象路径。
fn set_json_path(
    root: &mut Value,
    path: &str,
    value: Value,
    replace: bool,
) -> Result<(), ProviderExecutionFailure> {
    let segments = path.split('.').collect::<Vec<_>>();
    let (leaf, parents) = segments
        .split_last()
        .ok_or_else(|| pre_dispatch_failure("Custom request path is empty."))?;
    let mut current = root;
    for segment in parents {
        let object = current.as_object_mut().ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Custom request path `{path}` crosses a non-object value."
            ))
        })?;
        current = object
            .entry((*segment).to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
    let object = current.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure(format!(
            "Custom request path `{path}` targets a non-object parent."
        ))
    })?;
    if object.contains_key(*leaf) && !replace {
        return Ok(());
    }
    object.insert((*leaf).to_string(), value);
    Ok(())
}

/// Deletes one validated dot-separated object path when present.
/// 在存在时删除一条已校验的点分隔对象路径。
fn delete_json_path(root: &mut Value, path: &str) -> Result<(), ProviderExecutionFailure> {
    let segments = path.split('.').collect::<Vec<_>>();
    let (leaf, parents) = segments
        .split_last()
        .ok_or_else(|| pre_dispatch_failure("Custom request path is empty."))?;
    let mut current = root;
    for segment in parents {
        let Some(object) = current.as_object_mut() else {
            return Ok(());
        };
        let Some(next) = object.get_mut(*segment) else {
            return Ok(());
        };
        current = next;
    }
    if let Some(object) = current.as_object_mut() {
        object.remove(*leaf);
    }
    Ok(())
}

/// Projects the shared sampling-temperature parameter into one registered provider body.
/// 将共享采样温度参数投影到一个已注册供应商请求体。
///
/// # Parameters
/// 参数
/// `request` contains the typed parameter and `encoded` is the mutable wire request.
/// `request` 包含强类型参数，`encoded` 是可变 Wire 请求。
///
/// # Returns
/// 返回值
/// Success when absent or projected without conflict.
/// 缺失或无冲突投影时返回成功。
fn project_sampling_temperature(
    request: &AiExecutionRequest,
    encoded: &mut EncodedWireRequest,
) -> Result<(), ProviderExecutionFailure> {
    let Some(value) = request
        .model_parameters
        .get(SAMPLING_TEMPERATURE_PARAMETER_ID)
    else {
        return Ok(());
    };
    let ModelParameterValue::Decimal(decimal) = value else {
        return Err(pre_dispatch_failure(
            "The registered sampling temperature value must be a decimal string.",
        ));
    };
    let temperature = serde_json::from_str::<Value>(decimal).map_err(|error| {
        pre_dispatch_failure(format!(
            "Sampling temperature could not be projected as JSON: {error}"
        ))
    })?;
    let body = request_body_mut(encoded, "Protocol")?;
    if body.contains_key("temperature") {
        return Err(pre_dispatch_failure(
            "Sampling temperature projection conflicts with an existing protocol field.",
        ));
    }
    body.insert(String::from("temperature"), temperature);
    Ok(())
}

/// Borrows one encoded request body as a JSON object with a provider-specific diagnostic.
/// 以供应商专属诊断把编码请求体借用为 JSON 对象。
///
/// # Parameters
/// 参数
/// `encoded` is the mutable wire request and `label` identifies its projector.
/// `encoded` 是可变 Wire 请求，`label` 标识其投影器。
///
/// # Returns
/// 返回值
/// The mutable JSON object, or a controlled pre-dispatch failure.
/// 返回可变 JSON 对象，或受控分派前失败。
fn request_body_mut<'a>(
    encoded: &'a mut EncodedWireRequest,
    label: &str,
) -> Result<&'a mut serde_json::Map<String, Value>, ProviderExecutionFailure> {
    encoded
        .body
        .as_object_mut()
        .ok_or_else(|| pre_dispatch_failure(format!("{label} request body must be a JSON object.")))
}

/// Applies credential-bound non-secret identity fields immediately before dispatch.
/// 在分派前立即应用凭据绑定的非秘密身份字段。
///
/// # Parameters
/// 参数
/// `route` fixes the exact protocol, `secret` carries validated account metadata, and
/// `encoded` is the provider-projected request.
/// `route` 固定精确协议，`secret` 承载已验证账号元数据，`encoded` 是供应商投影请求。
///
/// # Returns
/// 返回值
/// A request containing required credential identity without exposing access material in JSON.
/// 返回包含所需凭据身份且不在 JSON 中暴露访问材料的请求。
pub(super) fn project_credential_request(
    route: &ProviderExecutionRoute,
    secret: &CredentialExecutionSecret,
    mut encoded: EncodedWireRequest,
) -> Result<EncodedWireRequest, ProviderExecutionFailure> {
    if route.protocol_route.protocol_profile_id
        != vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID
    {
        return Ok(encoded);
    }
    let project_id = secret.project_id().ok_or_else(|| {
        pre_dispatch_failure(
            "Google Code Assist execution requires a validated credential project id.",
        )
    })?;
    let body = encoded.body.as_object_mut().ok_or_else(|| {
        pre_dispatch_failure("Google Code Assist request body must be a JSON object.")
    })?;
    if body.contains_key("project") {
        return Err(pre_dispatch_failure(
            "Google Code Assist project projection conflicts with a protocol field.",
        ));
    }
    body.insert(
        String::from("project"),
        Value::String(project_id.to_string()),
    );
    Ok(encoded)
}

#[cfg(test)]
mod tests;

/// Removes a Responses continuation whenever the wire request is explicitly non-persistent.
/// 在线路请求明确不持久化时移除 Responses 延续游标。
///
/// # Parameters
/// 参数
/// `route` identifies the exact protocol profile and `request` is the immutable canonical request.
/// `route` 标识精确协议配置，`request` 是不可变规范请求。
///
/// # Returns
/// 返回值
/// A request that replays complete context for standard and Codex store=false Responses calls.
/// 返回一项为标准及 Codex store=false Responses 调用重放完整上下文的请求。
pub(super) fn request_without_nonpersistent_responses_continuation(
    route: &ProviderExecutionRoute,
    request: &AiExecutionRequest,
) -> AiExecutionRequest {
    let mut request = request.clone();
    if matches!(
        route.protocol_route.protocol_profile_id.as_str(),
        OPENAI_RESPONSES_PROFILE_ID | OPENAI_CODEX_RESPONSES_PROFILE_ID
    ) {
        request.continuation = None;
    }
    request
}
