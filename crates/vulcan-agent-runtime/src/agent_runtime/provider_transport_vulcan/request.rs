//! Public request assembly and frozen tool/continuation projection.
//! 公共请求组装与冻结的工具及延续信息投影�?
use super::message_projection::{core_message_text, core_message_to_public};
use super::{VulcanPublicContractAdapter, provider_failure};
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use async_trait::async_trait;
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    APPLY_PATCH_LARK_GRAMMAR, APPLY_PATCH_TOOL_NAME, AiContinuation, AiExecutionRequest, AiMessage,
    AiOperation, ContentItem, FreeformToolArgumentEncoding, FreeformToolFormat, FreeformToolSyntax,
    MessageRole, NativeToolKind, ParallelToolCallPolicy, ReasoningOutputPolicy, ResourceKind,
    StructuredOutputPolicy, ToolChoicePolicy, ToolDefinition, ToolInputFormat,
};
use vulcan_core::provider::VulcanProviderAdapter;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema,
    ProviderTurnResponse as CoreProviderTurnResponse,
};
use vulcan_provider_adapters::{
    CodexApplyPatchToolType, ProviderExecutionRoute, ToolSchemaProjectionMode,
};
use vulcan_tools_manager::{VulcanToolSchemaProjectionProfile, project_tool_parameters_schema};

/// Carries the public request together with the controller budget snapshot.
/// 同时承载公共请求与控制器预算快照�?
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VulcanPublicContractRequest {
    /// Provider-neutral execution request.
    /// 供应商无关执行请求�?
    pub(crate) public: AiExecutionRequest,
    /// Exact controller configuration used to build this public request.
    /// 用于构建当前公共请求的精确控制器配置�?
    pub(crate) controller_config: CoreControllerConfig,
    /// Exact canonical controller snapshot used to build this public request.
    /// 用于构建当前公共请求的精确规范控制器快照�?
    pub(crate) controller_snapshot: CoreConversationSnapshot,
    /// Exact source IDs mapped to positions at assembly time, excluding synthetic system messages.
    /// 组装时映射至位置的精确来源ID，不包含合成系统消息�?
    #[serde(skip)]
    pub(crate) source_message_positions: BTreeMap<String, usize>,
    /// Exact canonical snapshot used by the request budget guard.
    /// 请求预算门使用的精确规范快照�?
    pub(crate) budget: CoreProviderRequestSnapshot,
    /// Host context-budget decision captured from this exact canonical request.
    /// 从当前精确规范请求捕获的宿主上下文预算决策�?
    #[serde(skip)]
    pub(crate) context_budget: Option<crate::context_budget::ContextBudgetSnapshot>,
    /// Frozen request-budget guard that the V1 owner must reapply to the rebuilt final request.
    /// V1 owner 必须重新应用到最终重建请求上的冻结请求预算守卫�?
    #[serde(skip)]
    pub(crate) request_budget_guard:
        Option<crate::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard>,
}

/// Maps one configured provider contract onto the shared tool-schema projection implementation.
/// 将一项已配置 Provider 契约映射到共享工�?Schema 投影实现�?///
/// # Parameters
/// 参数
/// `mode` is the exact value frozen into the execution route.
/// `mode` 是冻结进执行路由的精确值�?///
/// # Returns
/// 返回�?/// The corresponding shared projection profile without protocol or model-name inference.
/// 返回对应共享投影配置，不进行协议或模型名推断�?
fn configured_tool_schema_projection_profile(
    mode: ToolSchemaProjectionMode,
) -> VulcanToolSchemaProjectionProfile {
    match mode {
        ToolSchemaProjectionMode::Lightweight => {
            VulcanToolSchemaProjectionProfile::StandardObjectRoot
        }
        ToolSchemaProjectionMode::Passthrough => VulcanToolSchemaProjectionProfile::PassThrough,
        ToolSchemaProjectionMode::Strict => VulcanToolSchemaProjectionProfile::GeminiSlim,
    }
}

/// Projects host-owned provider tools through the exact route-bound compatibility profile.
/// 通过精确路由绑定的兼容配置投影宿主持有的 Provider 工具�?///
/// # Parameters
/// 参数
/// `route` freezes the configured schema contract; `tools` contains the already resolved
/// provider-visible definitions.
/// `route` 冻结已配置的 Schema 契约；`tools` 包含已经解析�?Provider 可见定义�?///
/// # Returns
/// 返回�?/// Route-normalized public tool definitions suitable for ordinary responses.
/// 返回适用于普�?Responses 的路由正规化公共工具定义�?
pub(crate) fn project_public_tool_definitions(
    route: &ProviderExecutionRoute,
    tools: Vec<ProviderToolDefinition>,
) -> Vec<ToolDefinition> {
    // schemaProjectionProfile maps only the route-frozen configuration; model names and protocol
    // families never silently change the schema contract.
    // schemaProjectionProfile 只映射路由冻结配置；模型名与协议族绝不会静默改变 Schema 契约�?
    let schema_projection_profile =
        configured_tool_schema_projection_profile(route.tool_schema_projection);
    tools
        .into_iter()
        .map(|tool| {
            // inputFormat is activated only by the exact frozen native route behavior and tool name.
            // inputFormat 仅由精确冻结的原生路由行为与工具名称共同激活�?
            let input_format = if tool.name == APPLY_PATCH_TOOL_NAME
                && route.codex_behavior.as_ref().is_some_and(|behavior| {
                    behavior.apply_patch_tool_type == CodexApplyPatchToolType::Freeform
                }) {
                ToolInputFormat::Freeform {
                    format: FreeformToolFormat::Grammar {
                        syntax: FreeformToolSyntax::Lark,
                        definition: APPLY_PATCH_LARK_GRAMMAR.to_string(),
                    },
                    argument_encoding: FreeformToolArgumentEncoding::ApplyPatchObject,
                }
            } else {
                ToolInputFormat::JsonSchema
            };
            ToolDefinition {
                name: tool.name,
                description: tool.description,
                input_schema: project_tool_parameters_schema(
                    &tool.parameters,
                    schema_projection_profile,
                ),
                input_format,
                strict: false,
            }
        })
        .collect()
}

/// Resolves provider-native tools exactly as the ordinary public Responses request path does.
/// 按普通公�?Responses 请求路径的相同规则解�?Provider 原生工具�?///
/// # Parameters
/// 参数
/// `protocol_profile_id`, `output_resources`, and `native_tools` are frozen from one resolved
/// provider execution.
/// `protocol_profile_id`、`output_resources` �?`native_tools` 来自同一份已解析 Provider 执行�?///
/// # Returns
/// 返回�?/// The deduplicated native-tool list, including Codex image generation when the route supports image output.
/// 返回去重后的原生工具列表；当路由支持图片输出时包�?Codex 图片生成�?
pub(crate) fn resolve_public_native_tools(
    protocol_profile_id: &str,
    output_resources: &[ResourceKind],
    mut native_tools: Vec<NativeToolKind>,
) -> Vec<NativeToolKind> {
    if protocol_profile_id == vulcan_llm_core::OPENAI_CODEX_RESPONSES_PROFILE_ID
        && output_resources.contains(&ResourceKind::Image)
        && !native_tools.contains(&NativeToolKind::ImageGeneration)
    {
        native_tools.push(NativeToolKind::ImageGeneration);
    }
    native_tools
}

#[async_trait]
impl VulcanProviderAdapter for VulcanPublicContractAdapter {
    type Request = VulcanPublicContractRequest;
    type Response = CoreProviderTurnResponse;

    fn provider_id(&self) -> &str {
        self.route.provider.id.as_str()
    }

    fn model_id(&self) -> &str {
        self.model_id.as_str()
    }

    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        // The stable prefix contains only the prompt-composer baseline; dynamic host text is appended as User events below.
        // 稳定前缀只包含提示词组合器基线；动态宿主文本在下方作为 User 事件追加�?
        let system_prompt = config
            .prompt_layers
            .render_or_legacy(config.system_prompt.as_str());
        let mut messages = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(AiMessage {
                role: MessageRole::System,
                content: vec![ContentItem::Text {
                    text: system_prompt.clone(),
                }],
                provider_context_items: Vec::new(),
            });
        }
        // Build the canonical history before inserting host events so source positions remain exact after projection.
        // 先构建规范历史再插入宿主事件，确保投影后来源位置仍然精确�?
        let mut projected_history = Vec::with_capacity(snapshot.messages.len());
        // Record identities beside conversion so capture never guesses a synthetic-message offset.
        // 在转换位置旁记录身份，使捕获绝不猜测合成消息偏移�?
        let mut source_message_positions = BTreeMap::new();
        for message in &snapshot.messages {
            if message.id.trim().is_empty()
                || projected_history
                    .iter()
                    .any(|(source_id, _)| source_id == &message.id)
            {
                return Err(provider_failure(
                    self.provider_id(),
                    "Provider source message identities are empty or duplicated.",
                ));
            }
            let public_message = core_message_to_public(
                message,
                self.provider_id(),
                self.route.connection.id.as_str(),
                self.route.request_url("/responses").as_str(),
                self.route.protocol_route.protocol_profile_id.as_str(),
                self.model_id(),
            )?;
            projected_history.push((message.id.clone(), public_message));
        }
        for (source_id, public_message) in projected_history {
            source_message_positions.insert(source_id, messages.len());
            messages.push(public_message);
        }
        // Native user media is prepared only by its exact prompt owner after this pure candidate stage.
        // 原生用户媒体只在本纯候选阶段之后由精确提示词所有者准备�?        // continuation is emitted only after the Core configuration proves the exact append-only boundary.
        // continuation 仅在 Core 配置证明精确追加边界后才会发出�?
        let continuation = build_public_continuation(
            snapshot,
            config,
            self.route.provider.id.as_str(),
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.model_id.as_str(),
            usize::from(!system_prompt.is_empty()),
        );
        // nativeTools follows the shared ordinary/remote Codex capability projection.
        // nativeTools 遵循普通与远端 Codex 共享的能力投影�?
        let native_tools = resolve_public_native_tools(
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.output_resources.as_slice(),
            self.provider_native_tools.clone(),
        );
        let rebuilt_public = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: config.model_id.clone(),
            cache_scope_id: Some(self.cache_scope_id.clone()),
            continuation,
            messages,
            tools: self.tools.clone(),
            native_tools,
            tool_choice: ToolChoicePolicy::Auto,
            parallel_tool_calls: ParallelToolCallPolicy::ProviderDefault,
            structured_output: StructuredOutputPolicy::None,
            stop_sequences: Vec::new(),
            prompt_cache: self.prompt_cache.clone(),
            reasoning_effort: self.reasoning_effort,
            reasoning_output: ReasoningOutputPolicy::ProviderDefault,
            model_parameters: self.model_parameters.clone(),
            resources: Vec::new(),
            output_token_limit: Some(self.output_token_limit),
        };
        let prompt = snapshot
            .messages
            .last()
            .map(core_message_text)
            .transpose()?
            .unwrap_or_default();
        let budget = CoreProviderRequestSnapshot {
            provider_id: self.route.provider.id.clone(),
            model_id: config.model_id.clone(),
            system_prompt,
            prompt_layers: config.prompt_layers.clone(),
            prompt,
            prompt_in_history: true,
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: self
                .tools
                .iter()
                .zip(self.tool_sources.iter())
                .map(
                    |(tool, (source_kind, source_label))| CoreProviderRequestToolSchema {
                        id: tool.name.clone(),
                        name: tool.name.clone(),
                        description: tool.description.clone(),
                        source_kind: source_kind.clone(),
                        source_label: source_label.clone(),
                        parameters_schema: tool.input_schema.clone(),
                    },
                )
                .collect(),
            additional_params: None,
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        };
        Ok(VulcanPublicContractRequest {
            public: rebuilt_public,
            controller_config: config.clone(),
            controller_snapshot: snapshot.clone(),
            source_message_positions,
            budget,
            context_budget: None,
            request_budget_guard: None,
        })
    }

    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(provider_failure(
            self.provider_id(),
            "The Vulcan public-contract adapter requires streaming execution.",
        ))
    }
}

/// Builds one public continuation only when the exact local boundary and context mutation proof both succeed.
/// 仅在精确本地边界与上下文变更证明同时成功时构建公共延续信息�?///
/// # Parameters
/// 参数
/// `snapshot` and `config` are the Core-owned canonical state and safety policy; route identifiers
/// freeze continuation ownership; `system_message_count` accounts for the synthetic public system message.
/// `snapshot` �?`config` �?Core 持有的规范状态和安全策略；路由标识冻结延续归属；
/// `system_message_count` 计入合成的公共系统消息�?///
/// # Returns
/// 返回�?/// A proven public continuation, or `None` when any ownership, boundary, or mutation proof fails.
/// 返回已证明安全的公共延续；任一归属、边界或变更证明失败时返�?`None`�?
pub(super) fn build_public_continuation(
    snapshot: &CoreConversationSnapshot,
    config: &CoreControllerConfig,
    provider_id: &str,
    protocol_profile_id: &str,
    model_id: &str,
    system_message_count: usize,
) -> Option<AiContinuation> {
    let continuation = &snapshot.continuation;
    if continuation.provider_id.as_deref() != Some(provider_id)
        || continuation.protocol_profile_id.as_deref() != Some(protocol_profile_id)
        || continuation.model_id.as_deref() != Some(model_id)
    {
        return None;
    }
    let provider_message_id = continuation
        .last_provider_message_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())?;
    let covered_local_message_id = continuation
        .last_local_message_id
        .as_deref()
        .filter(|value| !value.trim().is_empty())?;
    let mut matching_indices = snapshot
        .messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| (message.id == covered_local_message_id).then_some(index));
    let covered_index = matching_indices.next()?;
    if matching_indices.next().is_some() {
        return None;
    }
    let uncovered_message_ids = snapshot.messages[covered_index + 1..]
        .iter()
        .map(|message| message.id.as_str())
        .collect::<Vec<_>>();
    if snapshot.messages[covered_index + 1..]
        .iter()
        .any(|message| is_runtime_host_event_message_id(message.id.as_str()))
    {
        return None;
    }
    if !config.continuation_reuse_is_proven(continuation, &uncovered_message_ids) {
        return None;
    }
    Some(AiContinuation {
        provider_id: provider_id.to_string(),
        protocol_profile_id: protocol_profile_id.to_string(),
        model_id: model_id.to_string(),
        provider_message_id: provider_message_id.to_string(),
        covered_local_message_id: covered_local_message_id.to_string(),
        covered_message_count: system_message_count
            .saturating_add(covered_index)
            .saturating_add(1),
    })
}

/// Identifies canonical host User messages that must be covered before continuation reuse.
/// 识别必须先被延续游标覆盖的规范宿主 User 消息。
pub(super) fn is_runtime_host_event_message_id(message_id: &str) -> bool {
    message_id.starts_with(vulcan_context::runtime_host::RUNTIME_HOST_MESSAGE_PREFIX)
}

#[cfg(test)]
mod tests;
