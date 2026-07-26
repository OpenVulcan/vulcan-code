//! AI-specialized tool routing shared by provider input preparation and diagnostics.
//! 供 Provider 输入准备与诊断复用的 AI 专用工具路由逻辑。

use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
use crate::provider_config::{AiToolKind, AiToolPolicy};

/// Effective route chosen for one semantic AI-specialized tool.
/// 单个语义 AI 专用工具选择出的生效路由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AiToolRouteKind {
    /// The tool is executed by a provider-native server-side tool.
    /// 该工具由 Provider 原生服务端工具执行。
    ProviderNative,
    /// The tool should be exposed as one explicitly bound host function.
    /// 该工具应作为一个显式绑定的宿主函数暴露。
    HostBinding,
    /// The tool is intentionally not exposed for the current model.
    /// 当前模型刻意不暴露该工具。
    Disabled,
}

/// One router decision for a semantic AI-specialized tool.
/// 单个语义 AI 专用工具的路由器决策。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AiToolRouteDecision {
    /// Semantic AI tool kind.
    /// 语义 AI 工具类型。
    pub(crate) kind: AiToolKind,
    /// Effective route kind.
    /// 生效路由类型。
    pub(crate) route: AiToolRouteKind,
    /// Optional provider-native tool type.
    /// 可选 Provider 原生工具类型。
    pub(crate) native_tool_type: Option<String>,
    /// Optional provider-native request patch.
    /// 可选 Provider 原生请求补丁。
    pub(crate) native_request_patch: Option<serde_json::Value>,
    /// Optional explicitly bound host tool identifier.
    /// 可选的显式绑定宿主工具标识。
    pub(crate) host_tool_id: Option<String>,
    /// Whether explicit user approval is required.
    /// 是否需要用户显式批准。
    pub(crate) requires_approval: bool,
    /// Whether structured citations are expected.
    /// 是否预期具备结构化引用。
    pub(crate) citation_support: bool,
    /// Human-readable capability source label.
    /// 人类可读能力来源标签。
    pub(crate) source_label: String,
    /// Human-readable explanation for diagnostics.
    /// 供诊断使用的人类可读解释。
    pub(crate) reason: String,
}

/// Full AI-tool plan resolved for one provider/model turn.
/// 为单个 Provider/Model 回合解析出的完整 AI 工具计划。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct AiToolPlan {
    /// Per-tool routing decisions.
    /// 逐工具路由决策集合。
    pub(crate) decisions: Vec<AiToolRouteDecision>,
}

impl AiToolPlan {
    /// Returns provider-native decisions that must be encoded into the provider request.
    /// 返回必须编码进 Provider 请求的 Provider 原生决策集合。
    pub(crate) fn provider_native_decisions(&self) -> Vec<&AiToolRouteDecision> {
        self.decisions
            .iter()
            .filter(|decision| decision.route == AiToolRouteKind::ProviderNative)
            .collect()
    }

    /// Returns whether the plan has no resolved AI-specialized tools.
    /// 返回当前计划是否没有任何已解析 AI 专用工具。
    pub(crate) fn is_empty(&self) -> bool {
        self.decisions.is_empty()
    }
}

/// Resolves AI-specialized tool routing for the active provider/model blueprint.
/// 为当前激活的 Provider/Model 蓝图解析 AI 专用工具路由。
pub(crate) fn resolve_ai_tool_plan(blueprint: &ProviderRuntimeBlueprint) -> AiToolPlan {
    let decisions = blueprint
        .capabilities
        .ai_tools
        .iter()
        .map(|capability| {
            let native_available = capability.native_provider_tool
                && (capability
                    .native_tool_type
                    .as_ref()
                    .is_some_and(|value| !value.trim().is_empty())
                    || capability.native_request_patch.is_some());
            let host_binding_available = capability
                .host_tool_id
                .as_ref()
                .is_some_and(|value| !value.trim().is_empty());

            let (route, reason) = match capability.policy {
                AiToolPolicy::Disabled => (
                    AiToolRouteKind::Disabled,
                    String::from("配置策略为 disabled，当前不会暴露该 AI 专用工具。"),
                ),
                AiToolPolicy::ForceNative if native_available => (
                    AiToolRouteKind::ProviderNative,
                    String::from("配置策略要求 Provider 原生工具，且当前模型已声明支持。"),
                ),
                AiToolPolicy::ForceNative => (
                    AiToolRouteKind::Disabled,
                    String::from("配置策略要求 Provider 原生工具，但当前模型未声明可用原生工具。"),
                ),
                AiToolPolicy::ForceHost if host_binding_available => (
                    AiToolRouteKind::HostBinding,
                    String::from("配置策略要求宿主工具，且已配置唯一的显式绑定工具。"),
                ),
                AiToolPolicy::ForceHost => (
                    AiToolRouteKind::Disabled,
                    String::from("配置策略要求宿主工具，但尚未配置唯一的显式绑定工具。"),
                ),
            };

            AiToolRouteDecision {
                kind: capability.kind,
                route,
                native_tool_type: capability.native_tool_type.clone(),
                native_request_patch: capability.native_request_patch.clone(),
                host_tool_id: capability.host_tool_id.clone(),
                requires_approval: capability.requires_approval,
                citation_support: capability.citation_support,
                source_label: capability.source_label.clone(),
                reason,
            }
        })
        .collect();

    AiToolPlan { decisions }
}

#[cfg(test)]
mod tests {
    //! AI-tool router regression tests.
    //! AI 工具路由器回归测试。

    use super::{AiToolRouteKind, resolve_ai_tool_plan};
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::agent_runtime::provider_reasoning::ProviderRuntimeReasoningResolution;
    use crate::provider_capabilities::{ResolvedAiToolCapability, ResolvedModelCapabilities};
    use crate::provider_config::{
        AiToolKind, AiToolPolicy, ProviderRuntimeKind, ReasoningEffortLevel,
    };

    /// Builds one minimal blueprint with a single AI-tool capability.
    /// 构建一份只包含单个 AI 工具能力的最小蓝图。
    fn blueprint_with_ai_tool(capability: ResolvedAiToolCapability) -> ProviderRuntimeBlueprint {
        ProviderRuntimeBlueprint {
            provider_id: String::from("openai"),
            provider_name: String::from("OpenAI"),
            runtime_kind: ProviderRuntimeKind::OpenAi,
            runtime_label: String::from("OpenAI"),
            model_id: String::from("gpt-test"),
            model_name: String::from("GPT Test"),
            openrouter_endpoint_route: None,
            base_url: None,
            supports_remote_model_refresh: false,
            credential_source: String::from("测试"),
            user_agent: None,
            extra_headers: Default::default(),
            extra_body: serde_json::json!({}),
            reasoning: ProviderRuntimeReasoningResolution {
                requested_effort: ReasoningEffortLevel::Off,
                source_label: String::from("测试"),
                additional_params: None,
                note: None,
            },
            capabilities: ResolvedModelCapabilities {
                ai_tools: vec![capability],
                ..ResolvedModelCapabilities::default()
            },
            interleaved_reasoning: None,
        }
    }

    /// Routes explicitly selected native web search to the declared provider tool.
    /// 将显式选择的原生网页搜索绑定到已声明的 Provider 工具。
    #[test]
    fn router_uses_explicit_provider_native_web_search() {
        let blueprint = blueprint_with_ai_tool(ResolvedAiToolCapability {
            kind: AiToolKind::WebSearch,
            policy: AiToolPolicy::ForceNative,
            native_provider_tool: true,
            native_tool_type: Some(String::from("web_search_preview")),
            native_request_patch: None,
            host_tool_id: None,
            requires_approval: false,
            result_modalities: vec![String::from("text")],
            citation_support: true,
            source_label: String::from("测试"),
        });

        let plan = resolve_ai_tool_plan(&blueprint);

        assert_eq!(plan.decisions[0].route, AiToolRouteKind::ProviderNative);
    }
}
