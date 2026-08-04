//! Provider/model runtime blueprint resolution used by the compose execution layer.
//! compose 执行层使用的 Provider/Model 运行时蓝图解析逻辑。

use crate::agent_runtime::provider_reasoning::{
    ProviderRuntimeReasoningResolution, resolve_effective_reasoning_effort,
};
use crate::provider_config::ReasoningEffortLevel;

/// Resolved provider/model runtime blueprint used by execution preparation.
/// 执行准备阶段使用的已解析 Provider/Model 运行时蓝图。
#[derive(Debug, Clone)]
pub struct ProviderRuntimeBlueprint {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Exact provider service-entry identifier selected for execution.
    /// 执行选择的精确供应商服务入口标识。
    pub service_entry_id: String,
    /// Exact credential connection identifier selected for execution.
    /// 执行选择的精确凭据连接标识。
    pub connection_id: String,
    /// Human-readable provider name.
    /// 人类可读的供应商名称。
    pub provider_name: String,
    /// Runtime family used by the currently selected provider.
    /// 当前选中供应商使用的运行时族。
    /// Human-readable runtime label.
    /// 人类可读的运行时标签。
    pub runtime_label: String,
    /// Stable model identifier.
    /// 稳定模型标识。
    pub model_id: String,
    /// Human-readable model name.
    /// 人类可读的模型名称。
    pub model_name: String,
    /// Exact provider-neutral wire protocol profile selected for this immutable execution.
    /// 为本次不可变执行选定的精确供应商无关 Wire 协议 Profile。
    pub protocol_profile_id: String,
    /// Exact provider-internal secondary route selected for this immutable execution.
    /// 为本次不可变执行选定的精确供应商内部二级路由。
    pub secondary_route: vulcan_provider_adapters::SecondaryRouteSelection,
    /// Exact public model capability snapshot captured for this immutable execution.
    /// 为本次不可变执行捕获的精确公共模型能力快照。
    pub public_capabilities: vulcan_ai_contract::ModelCapabilities,
    /// Resolved base URL when one is available.
    /// 当存在时的已解析接口基址。
    pub base_url: Option<String>,
    /// Whether the provider can actively refresh remote models.
    /// 该供应商是否可以主动刷新远端模型列表。
    pub supports_remote_model_refresh: bool,
    /// Human-readable credential source summary.
    /// 人类可读的凭据来源摘要。
    pub credential_source: String,
    /// Shared reasoning-resolution result derived from model overrides, provider overrides, and runtime defaults.
    /// 由模型覆盖、供应商覆盖与运行时默认值共同解析出的共享推理控制结果。
    pub reasoning: ProviderRuntimeReasoningResolution,
}

impl ProviderRuntimeBlueprint {
    /// Returns the canonical provider API family derived from the resolved runtime kind.
    /// 返回从已解析运行时类型派生出的规范 Provider 接口族。
    /// Returns the stable API-family key used by diagnostics and request previews.
    /// 返回供诊断与请求预览复用的稳定接口族键值。
    pub fn protocol_profile_key(&self) -> &str {
        self.protocol_profile_id.as_str()
    }

    /// Returns the human-readable API-family label used by diagnostics surfaces.
    /// 返回供诊断界面使用的人类可读接口族标签。
    pub fn protocol_profile_label(&self) -> &'static str {
        match self.protocol_profile_id.as_str() {
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => "Anthropic Messages",
            vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
                "Anthropic Messages + Video"
            }
            vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => "OpenAI Chat Completions",
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID => "OpenAI Responses",
            _ => "Unregistered protocol profile",
        }
    }

    /// Returns a compact stable runtime-kind key used by diagnostics and future factories.
    /// 返回供诊断与后续工厂使用的紧凑稳定运行时键值。
    pub const fn runtime_kind_key(&self) -> &'static str {
        "public-ai-contract"
    }

    /// Returns a compact endpoint summary rendered in execution diagnostics.
    /// 返回展示在执行诊断中的紧凑接口摘要。
    pub fn endpoint_summary(&self) -> String {
        self.base_url
            .clone()
            .unwrap_or_else(|| String::from("由运行时自动解析"))
    }

    /// Returns whether the active model accepts direct tool exposure for the current turn.
    /// 返回当前激活模型是否接受本轮直接暴露工具。
    pub fn supports_tool_call(&self) -> bool {
        self.public_capabilities.tool_calling.is_supported()
    }

    /// Returns whether the active model accepts explicit temperature overrides.
    /// 返回当前激活模型是否接受显式温度覆盖。
    pub fn supports_temperature(&self) -> bool {
        self.public_capabilities
            .model_parameters
            .iter()
            .any(|definition| {
                definition.id == vulcan_provider_adapters::SAMPLING_TEMPERATURE_PARAMETER_ID
            })
    }
}

/// Resolves the new public AI selection into the temporary controller blueprint shape.
/// 将新的公共 AI 选择解析为临时控制器蓝图形态。
///
/// # Parameters
/// 参数
/// `runtime` owns the in-memory catalog, `config` owns the active selection, and
/// `requested_reasoning_effort` is retained only as a host diagnostic preference.
/// `runtime` 持有内存目录，`config` 持有当前选择，`requested_reasoning_effort`
/// 仅作为宿主诊断偏好保留。
///
/// # Returns
/// 返回值
/// A controller compatibility snapshot whose network dispatch is still owned by the public adapter.
/// 返回网络分派仍由公共适配器持有的控制器兼容快照。
pub fn resolve_public_ai_provider_blueprint(
    runtime: &vulcan_provider_adapters::PublicAiRuntime,
    requested_reasoning_effort: ReasoningEffortLevel,
    provider_id_override: Option<&str>,
    model_id_override: Option<&str>,
    connection_id_override: Option<&str>,
) -> Result<ProviderRuntimeBlueprint, String> {
    let configuration = runtime.configuration_snapshot()?;
    let selection = match (
        provider_id_override,
        model_id_override,
        connection_id_override,
    ) {
        (Some(provider_id), Some(model_id), Some(connection_id)) => runtime
            .resolve_connection_model_selection(
                &configuration,
                connection_id,
                provider_id,
                model_id,
            )?,
        (Some(provider_id), Some(model_id), None) => {
            runtime.resolve_model_selection(&configuration, provider_id, model_id)?
        }
        (None, None, None) => runtime.resolve_active_selection(&configuration)?,
        _ => {
            return Err(String::from(
                "Public AI request overrides require provider_id and model_id; connection_id is optional only with both.",
            ));
        }
    };
    let protocol_id = selection.route.protocol_route.protocol_profile_id.as_str();
    // Apply validated local narrowing to the immutable request blueprint so budgeting and
    // transport dispatch observe the same limits.
    // 将已校验的本地收窄应用到不可变请求蓝图，确保预算与传输分派观察到相同限制。
    let mut capabilities = selection.model.capabilities.clone();
    // Apply the shared host default before exposing the blueprint to budgeting and transport.
    // 在将运行时蓝图交给预算与传输层前应用共享宿主默认策略。
    vulcan_provider_adapters::apply_host_default_reasoning_policy(&mut capabilities);
    if let Some(context_limit) = selection.overrides.context_limit {
        capabilities.context_limit = Some(context_limit);
    }
    if let Some(output_limit) = selection.overrides.output_limit {
        capabilities.output_limit = Some(output_limit);
    }
    // effectiveReasoningEffort resolves the same concrete default shown by the workbench after the exact model is known.
    // effectiveReasoningEffort 在精确模型确定后解析工作台展示的同一具体默认档位。
    let effective_reasoning_effort =
        resolve_effective_reasoning_effort(&capabilities, Some(requested_reasoning_effort));
    // reasoningNote explains only an incompatible explicit non-sentinel preference; an absent Off sentinel is a normal default path.
    // reasoningNote 仅解释不兼容的显式非哨兵偏好；缺省 Off 哨兵属于正常默认路径。
    let reasoning_note = (requested_reasoning_effort != ReasoningEffortLevel::Off
        && requested_reasoning_effort != effective_reasoning_effort)
        .then(|| {
            format!(
                "Requested reasoning effort `{}` is unsupported by model `{}`; using `{}`.",
                requested_reasoning_effort.compact_label(),
                selection.model.id,
                effective_reasoning_effort.compact_label()
            )
        });
    Ok(ProviderRuntimeBlueprint {
        provider_id: selection.route.provider.id.clone(),
        service_entry_id: selection.route.entry.id.clone(),
        connection_id: selection.route.connection.id.clone(),
        provider_name: selection.route.provider.display_name.clone(),
        runtime_label: String::from("Vulcan Public AI Contract"),
        model_id: selection.model.id.clone(),
        model_name: selection.model.display_name.clone(),
        protocol_profile_id: protocol_id.to_string(),
        secondary_route: selection.route.secondary_route.clone(),
        public_capabilities: capabilities,
        base_url: Some(selection.route.entry.base_url.clone()),
        supports_remote_model_refresh: true,
        credential_source: selection.route.connection.display_name.clone(),
        reasoning: ProviderRuntimeReasoningResolution {
            requested_effort: effective_reasoning_effort,
            source_label: String::from("公共契约"),
            note: reasoning_note,
        },
    })
}
