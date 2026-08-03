//! Vulcan public-contract adapter for the existing host tool controller.
//! 面向现有宿主工具控制器的 Vulcan 公共契约适配器。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_entry_inputs::{
    PreparedProviderTransportStreamLaunch, prepare_provider_transport_stream_launch,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_request::ProviderTransportExecutionRequest;
use crate::agent_runtime::provider_transport_stream_runtime::execute_host_controlled_stream_with_prepared_inputs;
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
use crate::attachment_assets::{AttachmentDescriptor, stage_attachment_bytes};
use async_stream::try_stream;
use async_trait::async_trait;
use base64::Engine;
use futures_util::StreamExt;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiContinuation, AiExecutionRequest, AiMessage, AiOperation, AiStreamEvent, ContentItem,
    MessageRole, ModelParameterValue, NativeToolKind, ParallelToolCallPolicy, PromptCachePolicy,
    ProviderReplayEnvelope, ReasoningEffort, ReasoningOutputPolicy, RequestUsage, ResourceKind,
    ResourceReference, StructuredOutputPolicy, ToolCall as PublicToolCall, ToolChoicePolicy,
    ToolDefinition, ToolResult, UsageValue,
};
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationAttachmentKind,
    ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot, ConversationToolCall,
    CoreError as VulcanCoreError, ProviderContinuation,
    ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderRequestToolSchema as CoreProviderRequestToolSchema, ProviderTurnDelta,
    ProviderTurnResponse as CoreProviderTurnResponse, ProviderUsageSnapshot,
};
use vulcan_provider_adapters::{
    CredentialExecutionSecret, ProviderExecutionClient, ProviderExecutionRoute,
    ResolvedPublicAiExecution,
};
use vulcan_tools_manager::{
    VulcanToolSchemaProjectionContext, project_tool_parameters_schema,
    resolve_tool_schema_projection_profile,
};

/// Vulcan-owned default output budget used when the provider reports no independent output limit.
/// 当供应商未报告独立输出上限时使用的 Vulcan 本地默认输出预算。
const DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT: u64 = 32_000;

/// Resolves the requested output budget while treating catalog output limits as upper bounds.
/// 在将目录输出限制视为上限的前提下解析请求输出预算。
///
/// # Parameters
/// 参数
/// `configured_limit` is the optional user narrowing and `catalog_limit` is the provider-reported
/// model maximum.
/// `configured_limit` 是可选的用户收窄值，`catalog_limit` 是供应商报告的模型最大值。
///
/// # Returns
/// 返回值
/// The configured or default request budget capped by the catalog maximum when present.
/// 返回配置值或默认请求预算；存在目录最大值时以其封顶。
fn resolved_public_output_token_limit(
    configured_limit: Option<u64>,
    catalog_limit: Option<u64>,
) -> u64 {
    let requested_limit = configured_limit.unwrap_or(DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT);
    catalog_limit
        .map(|maximum| requested_limit.min(maximum))
        .unwrap_or(requested_limit)
}
/// MiniMax M3 maximum local image payload size documented for one request resource.
/// MiniMax M3 针对单个请求资源文档化的本地图像载荷上限。
const MINIMAX_M3_IMAGE_BYTE_LIMIT: u64 = 10_000_000;
/// MiniMax M3 maximum local video payload size documented for one request resource.
/// MiniMax M3 针对单个请求资源文档化的本地视频载荷上限。
const MINIMAX_M3_VIDEO_BYTE_LIMIT: u64 = 50_000_000;

/// Executes one public-contract provider run through the existing host controller.
/// 通过现有宿主控制器执行一次公共契约 Provider 运行。
///
/// # Parameters
/// 参数
/// `request` owns host controller inputs and `resolved` freezes the account, protocol, model,
/// secret, cache, and model-parameter route.
/// `request` 持有宿主控制器输入，`resolved` 冻结账号、协议、模型、秘密、缓存与模型参数路由。
///
/// # Returns
/// 返回值
/// A stream of existing workbench deltas projected from public events.
/// 返回从公共事件投影出的现有工作台增量流。
pub(crate) fn execute_vulcan_public_stream(
    request: ProviderTransportExecutionRequest,
    resolved: ResolvedPublicAiExecution,
    reasoning_effort: Option<ReasoningEffort>,
) -> ProviderDeltaStream {
    let cache_scope_id = request.session_id.to_string();
    let workspace_path = request.workspace_path.clone();
    let output_token_limit = resolved_public_output_token_limit(
        resolved.overrides.output_limit,
        resolved.model.capabilities.output_limit,
    );
    let prompt_cache = resolved.overrides.prompt_cache.clone().unwrap_or_default();
    let PreparedProviderTransportStreamLaunch {
        runtime_inputs,
        core_snapshot,
        prompt_intervention,
    } = prepare_provider_transport_stream_launch(request);
    execute_host_controlled_stream_with_prepared_inputs(
        runtime_inputs,
        core_snapshot,
        move |live_sender, adapter_inputs| {
            VulcanPublicContractAdapter::new(
                resolved.route.clone(),
                resolved.model.id.clone(),
                cache_scope_id.clone(),
                resolved.secret.clone(),
                resolved.client.clone(),
                adapter_inputs.provider_tool_definitions,
                resolved.model.capabilities.provider_native_tools.clone(),
                output_token_limit,
                prompt_cache.clone(),
                reasoning_effort,
                resolved.overrides.model_parameters.clone(),
                resolved.model.capabilities.input_resources.clone(),
                resolved.model.capabilities.output_resources.clone(),
                workspace_path.clone(),
                live_sender,
            )
        },
        "Vulcan 公共契约",
        prompt_intervention,
    )
}

/// Captures one exact public wire request without dispatching network traffic.
/// 捕获一个精确的公共线级请求，但不分派网络流量。
///
/// # Parameters
/// 参数
/// `request` owns the same host inputs as normal execution and `resolved` freezes the route.
/// `request` 持有与正常执行相同的宿主输入，`resolved` 冻结执行路由。
///
/// # Returns
/// 返回值
/// A one-item stream containing the projected wire JSON or a controlled error.
/// 返回只包含投影线级 JSON 的单项流，或受控错误。
pub(crate) fn capture_vulcan_public_stream(
    request: ProviderTransportExecutionRequest,
    resolved: ResolvedPublicAiExecution,
    reasoning_effort: Option<ReasoningEffort>,
) -> ProviderDeltaStream {
    let cache_scope_id = request.session_id.to_string();
    let workspace_path = request.workspace_path.clone();
    let output_token_limit = resolved_public_output_token_limit(
        resolved.overrides.output_limit,
        resolved.model.capabilities.output_limit,
    );
    Box::pin(try_stream! {
        let prompt_cache = resolved.overrides.prompt_cache.clone().unwrap_or_default();
        let prepared = prepare_provider_transport_stream_launch(request);
        let controller_config = prepared.runtime_inputs.entry_inputs.controller_config;
        let provider_tool_definitions = prepared
            .runtime_inputs
            .tool_context
            .provider_tool_definitions;
        let (live_sender, _live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let adapter = VulcanPublicContractAdapter::new(
            resolved.route.clone(),
            resolved.model.id.clone(),
            cache_scope_id,
            resolved.secret,
            resolved.client.clone(),
            provider_tool_definitions,
            resolved.model.capabilities.provider_native_tools,
            output_token_limit,
            prompt_cache,
            reasoning_effort,
            resolved.overrides.model_parameters,
            resolved.model.capabilities.input_resources,
            resolved.model.capabilities.output_resources,
            workspace_path,
            live_sender,
        );
        let public_request = adapter
            .build_request(&prepared.core_snapshot, &controller_config)
            .await
            .map_err(|error| error.to_string())?;
        let encoded = resolved
            .client
            .encode_request_with_credential(
                &resolved.route,
                &public_request.public,
                &adapter.secret,
            )
            .map_err(|failure| failure.to_string())?;
        yield ProviderStreamDelta::WireRequest(encoded.body);
    })
}

/// Bridges Vulcan public requests and events into the host-owned controller.
/// 将 Vulcan 公共请求与事件桥接到宿主控制器。
#[derive(Clone)]
pub(crate) struct VulcanPublicContractAdapter {
    /// Immutable provider execution route.
    /// 不可变供应商执行路由。
    route: ProviderExecutionRoute,
    /// Exact model identifier fixed for this controller execution.
    /// 为本次控制器执行固定的精确模型标识。
    model_id: String,
    /// Stable session cache scope shared by every physical request in this controller run.
    /// 本次控制器运行中每个物理请求共享的稳定会话缓存作用域。
    cache_scope_id: String,
    /// Exact credential secret loaded by the host.
    /// 宿主加载的精确凭据秘钥。
    secret: CredentialExecutionSecret,
    /// Shared public-contract execution client.
    /// 共享公共契约执行客户端。
    client: ProviderExecutionClient,
    /// Model-visible host tool definitions.
    /// 模型可见的宿主工具定义。
    tools: Vec<ToolDefinition>,
    /// Provider-native tools declared by the exact discovered model route.
    /// 精确发现模型路由声明的供应商原生工具。
    provider_native_tools: Vec<NativeToolKind>,
    /// Resolved output limit for every controller turn.
    /// 每个控制器回合使用的已解析输出限制。
    output_token_limit: u64,
    /// Resolved prompt-cache policy snapshot.
    /// 已解析提示词缓存策略快照。
    prompt_cache: PromptCachePolicy,
    /// Optional user-selected reasoning effort validated against the active catalog.
    /// 根据当前目录校验后的可选用户推理强度。
    reasoning_effort: Option<ReasoningEffort>,
    /// Validated model-specific values.
    /// 已校验模型独立参数值。
    model_parameters: std::collections::BTreeMap<String, ModelParameterValue>,
    /// Resource kinds declared by the exact discovered model.
    /// 精确发现模型声明的资源类别。
    input_resources: Vec<ResourceKind>,
    /// Output resource kinds declared by the exact discovered model.
    /// 精确发现模型声明的输出资源类别。
    output_resources: Vec<ResourceKind>,
    /// Canonical workspace path that owns materialized provider-generated resources.
    /// 拥有供应商生成资源落盘文件的规范工作区路径。
    workspace_path: String,
    /// Live host sender for tool request construction.
    /// 工具请求构造使用的宿主实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

/// Carries the public request together with the controller budget snapshot.
/// 同时承载公共请求与控制器预算快照。
#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct VulcanPublicContractRequest {
    /// Provider-neutral execution request.
    /// 供应商无关执行请求。
    pub(crate) public: AiExecutionRequest,
    /// Exact canonical snapshot used by the request budget guard.
    /// 请求预算门使用的精确规范快照。
    pub(crate) budget: CoreProviderRequestSnapshot,
}

impl VulcanPublicContractAdapter {
    /// Creates one immutable public-contract controller adapter.
    /// 创建一个不可变公共契约控制器适配器。
    ///
    /// # Parameters
    /// 参数
    /// `route`, `secret`, and `client` identify the exact execution path; `tools` are host
    /// definitions; `output_token_limit`, `prompt_cache`, and `model_parameters` are resolved
    /// before dispatch; `live_sender` projects tool construction to the WebUI.
    /// `route`、`secret` 与 `client` 标识精确执行路径；`tools` 是宿主工具定义；
    /// `output_token_limit`、`prompt_cache` 与 `model_parameters` 在分派前解析；
    /// `live_sender` 将工具构造投影到 WebUI。
    ///
    /// # Returns
    /// 返回值
    /// A reusable adapter whose route remains fixed for the controller run.
    /// 返回在控制器运行期间路由保持固定的可复用适配器。
    pub(crate) fn new(
        route: ProviderExecutionRoute,
        model_id: String,
        cache_scope_id: String,
        secret: CredentialExecutionSecret,
        client: ProviderExecutionClient,
        tools: Vec<ProviderToolDefinition>,
        provider_native_tools: Vec<NativeToolKind>,
        output_token_limit: u64,
        prompt_cache: PromptCachePolicy,
        reasoning_effort: Option<ReasoningEffort>,
        model_parameters: std::collections::BTreeMap<String, ModelParameterValue>,
        input_resources: Vec<ResourceKind>,
        output_resources: Vec<ResourceKind>,
        workspace_path: String,
        live_sender: ProviderLiveDeltaSender,
    ) -> Self {
        // schema_projection_context freezes the exact provider, protocol family, and model before any tool definition enters the public request.
        // schema_projection_context 在任何工具定义进入公共请求前冻结精确供应商、协议族与模型。
        let schema_projection_context = VulcanToolSchemaProjectionContext {
            provider_id: route.provider.id.as_str(),
            api_family_key: tool_schema_api_family_key(
                route.protocol_route.protocol_profile_id.as_str(),
            ),
            model_id: model_id.as_str(),
            additional_params: None,
        };
        // schema_projection_profile reuses the host-owned compatibility policy instead of duplicating provider schema rewrites in each protocol encoder.
        // schema_projection_profile 复用宿主自有兼容策略，避免在每个协议编码器中重复供应商 Schema 改写。
        let schema_projection_profile =
            resolve_tool_schema_projection_profile(&schema_projection_context);
        Self {
            route,
            model_id,
            cache_scope_id,
            secret,
            client,
            tools: tools
                .into_iter()
                .map(|tool| ToolDefinition {
                    name: tool.name,
                    description: tool.description,
                    input_schema: project_tool_parameters_schema(
                        &tool.parameters,
                        schema_projection_profile,
                    ),
                })
                .collect(),
            provider_native_tools,
            output_token_limit,
            prompt_cache,
            reasoning_effort,
            model_parameters,
            input_resources,
            output_resources,
            workspace_path,
            live_sender,
        }
    }
}

/// Maps one public protocol profile to the stable API-family key consumed by the shared tool-schema projector.
/// 将一个公共协议配置映射为共享工具 Schema 投影器使用的稳定 API 族键。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` is the immutable protocol profile selected for the execution route.
/// `protocol_profile_id` 是执行路由选定的不可变协议配置标识。
///
/// # Returns
/// 返回值
/// The exact compatibility family key used by `vulcan-tools-manager`.
/// 返回 `vulcan-tools-manager` 使用的精确兼容族键。
fn tool_schema_api_family_key(protocol_profile_id: &str) -> &'static str {
    match protocol_profile_id {
        vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        | vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID => "google-gemini-generate-content",
        vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID => "openai-responses-compatible",
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => "openai-chat-compatible",
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        | vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID
        | vulcan_ai_protocols::CLAUDE_CODE_MESSAGES_PROFILE_ID => "anthropic-messages",
        vulcan_ai_protocols::GEMINI_INTERACTIONS_PROFILE_ID => "gemini-interactions",
        vulcan_ai_protocols::COHERE_CHAT_PROFILE_ID => "cohere-chat",
        _ => "unknown",
    }
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
        let system_prompt = config
            .provider_prompt_layers_with_persistent_injection()
            .render_or_legacy(config.system_prompt.as_str());
        let mut messages = Vec::new();
        if !system_prompt.is_empty() {
            messages.push(AiMessage {
                role: MessageRole::System,
                content: vec![ContentItem::Text {
                    text: system_prompt.clone(),
                }],
            });
        }
        for message in &snapshot.messages {
            let mut public_message = core_message_to_public(
                message,
                self.provider_id(),
                self.route.protocol_route.protocol_profile_id.as_str(),
                self.model_id(),
            )?;
            resolve_public_message_resources(
                &mut public_message,
                &self.route,
                self.model_id.as_str(),
                self.input_resources.as_slice(),
                &self.client,
            )
            .await?;
            messages.push(public_message);
        }
        // continuation is emitted only after the Core configuration proves the exact append-only boundary.
        // continuation 仅在 Core 配置证明精确追加边界后才会发出。
        let continuation = build_public_continuation(
            snapshot,
            config,
            self.route.provider.id.as_str(),
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.model_id.as_str(),
            usize::from(!system_prompt.is_empty()),
        );
        let mut native_tools = self.provider_native_tools.clone();
        if self.route.protocol_route.protocol_profile_id
            == vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID
            && self.output_resources.contains(&ResourceKind::Image)
            && !native_tools.contains(&NativeToolKind::ImageGeneration)
        {
            // Preserve the existing Codex image-generation capability while merging route metadata.
            // 合并路由元数据时保留现有 Codex 图片生成能力。
            native_tools.push(NativeToolKind::ImageGeneration);
        }
        let public = AiExecutionRequest {
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
            prompt_layers: config.provider_prompt_layers_with_persistent_injection(),
            prompt,
            history: snapshot.messages.clone(),
            temperature: config.temperature,
            max_turns: config.max_turns,
            tool_schemas: self
                .tools
                .iter()
                .map(|tool| CoreProviderRequestToolSchema {
                    id: tool.name.clone(),
                    name: tool.name.clone(),
                    description: tool.description.clone(),
                    source_label: String::from("host"),
                    parameters_schema: tool.input_schema.clone(),
                })
                .collect(),
            additional_params: None,
            request_headers: None,
            continuation: snapshot.continuation.clone(),
        };
        Ok(VulcanPublicContractRequest { public, budget })
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
/// 仅在精确本地边界与上下文变更证明同时成功时构建公共延续信息。
///
/// # Parameters
/// 参数
/// `snapshot` and `config` are the Core-owned canonical state and safety policy; route identifiers
/// freeze continuation ownership; `system_message_count` accounts for the synthetic public system message.
/// `snapshot` 与 `config` 是 Core 持有的规范状态和安全策略；路由标识冻结延续归属；
/// `system_message_count` 计入合成的公共系统消息。
///
/// # Returns
/// 返回值
/// A proven public continuation, or `None` when any ownership, boundary, or mutation proof fails.
/// 返回已证明安全的公共延续；任一归属、边界或变更证明失败时返回 `None`。
fn build_public_continuation(
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

#[async_trait]
impl VulcanStreamingProviderAdapter for VulcanPublicContractAdapter {
    async fn execute_request_streaming<S>(
        &self,
        request: Self::Request,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let VulcanPublicContractRequest { public, budget } = request;
        self.live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(budget)))
            .map_err(|_| {
                provider_failure(
                    self.provider_id(),
                    "The provider request stream closed before dispatch.",
                )
            })?;
        let execution_id = uuid::Uuid::new_v4().to_string();
        let mut stream = self.client.execute(
            self.route.clone(),
            public,
            self.secret.clone(),
            execution_id,
        );
        let mut response = PublicTurnAccumulator::new(
            self.provider_id(),
            self.route.protocol_route.protocol_profile_id.as_str(),
            self.model_id(),
        );
        while let Some(item) = stream.next().await {
            let event = item.map_err(|mut failure| {
                attach_execution_route_context(
                    &mut failure.error,
                    self.provider_id(),
                    self.route.entry.id.as_str(),
                    self.model_id(),
                );
                public_execution_failure(self.provider_id(), *failure.error)
            })?;
            let mut event = materialize_provider_resource_event(
                self.provider_id(),
                self.workspace_path.as_str(),
                event,
            )
            .await?;
            if let AiStreamEvent::ResponseFailed { error } = &mut event {
                attach_execution_route_context(
                    error,
                    self.provider_id(),
                    self.route.entry.id.as_str(),
                    self.model_id(),
                );
            }
            response.apply_event(self.provider_id(), &self.live_sender, sink, event)?;
        }
        response.finish()
    }
}

/// Attaches the exact resolved route to one structured execution failure.
/// 为一个结构化执行失败附加精确解析后的执行路由。
///
/// # Parameters
/// 参数
/// `error` is the normalized failure; the remaining values identify the immutable physical route.
/// `error` 是归一化失败；其余值标识不可变的物理执行路由。
fn attach_execution_route_context(
    error: &mut vulcan_ai_contract::AiExecutionError,
    provider_id: &str,
    entry_id: &str,
    model_id: &str,
) {
    error.provider_id = Some(provider_id.to_string());
    error.entry_id = Some(entry_id.to_string());
    error.model_id = Some(model_id.to_string());
}

/// Builds a Core provider failure that retains the normalized public error.
/// 构建一项保留归一化公共错误的 Core Provider 失败。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the controller route and `error` is the structured failure.
/// `provider_id` 标识控制器路由，`error` 是结构化失败。
///
/// # Returns
/// 返回值
/// A Core error carrying both display text and typed details.
/// 返回同时承载展示文本与强类型细节的 Core 错误。
fn public_execution_failure(
    provider_id: &str,
    error: vulcan_ai_contract::AiExecutionError,
) -> VulcanCoreError {
    VulcanCoreError::ProviderFailure {
        provider_id: provider_id.to_string(),
        message: public_execution_failure_message(&error),
        details: Some(Box::new(error)),
    }
}

/// Formats one public execution error for user-facing diagnostics only.
/// 仅为面向用户的诊断格式化一个公共执行错误。
fn public_execution_failure_message(error: &vulcan_ai_contract::AiExecutionError) -> String {
    let mut fields = vec![
        format!("category={:?}", error.category),
        format!("stage={}", error.stage),
        format!("dispatch_outcome={:?}", error.dispatch_outcome),
        format!(
            "semantic_output_observed={}",
            error.semantic_output_observed
        ),
        format!("retryable={}", error.retryable),
    ];
    if let Some(status) = error.http_status {
        fields.push(format!("HTTP {status}"));
    }
    if let Some(subcategory) = &error.subcategory {
        fields.push(format!("subcategory={subcategory}"));
    }
    if let Some(code) = &error.upstream_code {
        fields.push(format!("upstream_code={code}"));
    }
    if let Some(retry_after) = &error.retry_after {
        fields.push(format!("retry_after_ms={}", retry_after.delay_milliseconds));
    }
    format!("{} | {}", fields.join(" | "), error.message)
}

/// Accumulates public stream events into the controller's completed-turn shape.
/// 将公共流事件累积为控制器的完整回合结构。
#[derive(Debug)]
struct PublicTurnAccumulator {
    /// Exact provider definition scope for durable replay.
    /// 可持久化重放使用的精确供应商定义作用域。
    provider_id: String,
    /// Exact protocol profile scope for provider-specific replay signatures.
    /// 供应商专属重放签名使用的精确协议 Profile 作用域。
    protocol_profile_id: String,
    /// Exact model scope for durable replay.
    /// 可持久化重放使用的精确模型作用域。
    model_id: String,
    /// Flattened visible assistant output.
    /// 扁平化的可见助手输出。
    text: String,
    /// Flattened reasoning output.
    /// 扁平化的推理输出。
    reasoning: String,
    /// Latest provider response or assistant-message identifier.
    /// 最新供应商响应或助手消息标识。
    provider_message_id: Option<String>,
    /// Complete tool calls.
    /// 完整工具调用。
    tool_calls: Vec<ConversationToolCall>,
    /// Last public usage update.
    /// 最后一个公共用量更新。
    usage: Option<RequestUsage>,
    /// Stream item lookup keyed by stable public item identifiers.
    /// 按稳定公共项目标识索引的流项目查找表。
    replay_item_indices: BTreeMap<String, usize>,
    /// Assistant items retained in original stream order.
    /// 按原始流顺序保留的助手项目。
    replay_items: Vec<ReplayAccumulatorItem>,
    /// Whether a successful terminal response event was observed.
    /// 是否已经观察到成功响应终态事件。
    completed: bool,
}

/// Stores one in-progress assistant item for deterministic replay assembly.
/// 为确定性重放组装保存一个进行中的助手项目。
#[derive(Debug)]
struct ReplayAccumulatorItem {
    /// Stable public stream item identifier.
    /// 稳定公共流项目标识。
    item_id: String,
    /// Current semantic item state.
    /// 当前语义项目状态。
    state: ReplayAccumulatorItemState,
}

/// Represents the closed set of assistant item states assembled from stream events.
/// 表示从流事件组装出的封闭助手项目状态集合。
#[derive(Debug)]
enum ReplayAccumulatorItemState {
    /// Incrementally assembled visible text.
    /// 增量组装的可见文本。
    Text(String),
    /// Incrementally assembled reasoning and its optional replay signature.
    /// 增量组装的推理及其可选重放签名。
    Reasoning {
        /// Visible reasoning summary or thinking text.
        /// 可见推理摘要或思考文本。
        text: String,
        /// Provider-specific opaque continuation signature.
        /// 供应商专属不透明延续签名。
        signature: Option<String>,
    },
    /// Complete non-incremental public content.
    /// 完整的非增量公共内容。
    Complete(ContentItem),
    /// Tool identity is known but its arguments are still incomplete.
    /// 工具身份已知但参数仍不完整。
    PendingTool,
}

impl PublicTurnAccumulator {
    /// Creates one accumulator bound to an immutable execution route.
    /// 创建一个绑定到不可变执行路由的累积器。
    ///
    /// # Parameters
    /// 参数
    /// The identifiers are the provider definition, protocol profile, and model selected for
    /// this physical request.
    /// 三个标识分别是本次物理请求选定的供应商定义、协议 Profile 与模型。
    ///
    /// # Returns
    /// 返回值
    /// A new empty turn accumulator.
    /// 返回一个新的空回合累积器。
    fn new(provider_id: &str, protocol_profile_id: &str, model_id: &str) -> Self {
        Self {
            provider_id: provider_id.to_string(),
            protocol_profile_id: protocol_profile_id.to_string(),
            model_id: model_id.to_string(),
            text: String::new(),
            reasoning: String::new(),
            provider_message_id: None,
            tool_calls: Vec::new(),
            usage: None,
            replay_item_indices: BTreeMap::new(),
            replay_items: Vec::new(),
            completed: false,
        }
    }

    /// Registers one stream item exactly once while preserving arrival order.
    /// 精确注册一次流项目并保留到达顺序。
    ///
    /// # Parameters
    /// 参数
    /// `item_id` is the stable public identifier and `state` is its initial semantic state.
    /// `item_id` 是稳定公共标识，`state` 是其初始语义状态。
    ///
    /// # Returns
    /// 返回值
    /// Success when the identifier is new; duplicate identifiers are controlled failures.
    /// 标识为新值时成功；重复标识会产生受控失败。
    fn register_replay_item(
        &mut self,
        item_id: String,
        state: ReplayAccumulatorItemState,
    ) -> Result<(), VulcanCoreError> {
        if self.replay_item_indices.contains_key(item_id.as_str()) {
            return Err(provider_failure(
                self.provider_id.as_str(),
                format!("Public stream item `{item_id}` started more than once.").as_str(),
            ));
        }
        let index = self.replay_items.len();
        self.replay_item_indices.insert(item_id.clone(), index);
        self.replay_items
            .push(ReplayAccumulatorItem { item_id, state });
        Ok(())
    }

    /// Resolves the mutable semantic state for one previously started item.
    /// 解析一个已启动项目的可变语义状态。
    ///
    /// # Parameters
    /// 参数
    /// `item_id` is the stable identifier from a delta or completion event.
    /// `item_id` 是增量或完成事件携带的稳定标识。
    ///
    /// # Returns
    /// 返回值
    /// The exact registered state, or a controlled ordering failure.
    /// 返回精确注册状态；事件顺序无效时返回受控失败。
    fn replay_item_state_mut(
        &mut self,
        item_id: &str,
    ) -> Result<&mut ReplayAccumulatorItemState, VulcanCoreError> {
        let index = self
            .replay_item_indices
            .get(item_id)
            .copied()
            .ok_or_else(|| {
                provider_failure(
                    self.provider_id.as_str(),
                    format!("Public stream item `{item_id}` was updated before it started.")
                        .as_str(),
                )
            })?;
        Ok(&mut self.replay_items[index].state)
    }

    /// Materializes all completed assistant items in original stream order.
    /// 按原始流顺序实体化所有已完成助手项目。
    ///
    /// # Returns
    /// 返回值
    /// Ordered public content, or a controlled failure when a tool remains incomplete.
    /// 返回有序公共内容；工具仍不完整时返回受控失败。
    fn replay_content(&self) -> Result<Vec<ContentItem>, VulcanCoreError> {
        self.replay_items
            .iter()
            .map(|item| match &item.state {
                ReplayAccumulatorItemState::Text(text) => {
                    Ok(ContentItem::Text { text: text.clone() })
                }
                ReplayAccumulatorItemState::Reasoning { text, signature } => {
                    Ok(ContentItem::Reasoning {
                        text: text.clone(),
                        signature: signature.clone(),
                    })
                }
                ReplayAccumulatorItemState::Complete(content) => Ok(content.clone()),
                ReplayAccumulatorItemState::PendingTool => Err(provider_failure(
                    self.provider_id.as_str(),
                    format!(
                        "Public tool stream item `{}` remained incomplete at response completion.",
                        item.item_id
                    )
                    .as_str(),
                )),
            })
            .collect()
    }

    /// Applies one public stream event.
    /// 应用一个公共流事件。
    fn apply_event<S>(
        &mut self,
        provider_id: &str,
        live_sender: &ProviderLiveDeltaSender,
        sink: &mut S,
        event: AiStreamEvent,
    ) -> Result<(), VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        match event {
            AiStreamEvent::ResponseMetadataUpdated { continuation } => {
                if let Some(existing) = self.provider_message_id.as_deref()
                    && existing != continuation.provider_message_id
                {
                    return Err(provider_failure(
                        provider_id,
                        "Provider response identity changed during one physical request.",
                    ));
                }
                self.provider_message_id = Some(continuation.provider_message_id);
            }
            AiStreamEvent::ResponseTextDelta { item_id, delta } => {
                let ReplayAccumulatorItemState::Text(text) =
                    self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public text delta targeted a non-text item.",
                    ));
                };
                text.push_str(delta.as_str());
                self.text.push_str(delta.as_str());
                // liveDelta preserves the same decoded text for the workbench stream while Core keeps its canonical copy.
                // liveDelta 在 Core 保留规范副本的同时，将同一段已解码文本投影到工作台流。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Output { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Output(live_delta)));
            }
            AiStreamEvent::ResponseItemStarted { item_id, item } => {
                let replay_state = match &item {
                    ContentItem::Text { text } => ReplayAccumulatorItemState::Text(text.clone()),
                    _ => ReplayAccumulatorItemState::Complete(item.clone()),
                };
                self.register_replay_item(item_id, replay_state)?;
                if let ContentItem::Resource {
                    resource:
                        ResourceReference {
                            kind: ResourceKind::Image,
                            local_path,
                            url,
                            ..
                        },
                } = item
                {
                    let location = local_path.or(url).ok_or_else(|| {
                        provider_failure(
                            provider_id,
                            "Provider-generated image has no local path or user-safe URL.",
                        )
                    })?;
                    let markdown = format!(
                        "{}![Generated image](<{}>)",
                        if self.text.ends_with('\n') || self.text.is_empty() {
                            ""
                        } else {
                            "\n\n"
                        },
                        location
                    );
                    self.text.push_str(markdown.as_str());
                    // liveMarkdown keeps generated-resource output visible before the terminal checkpoint.
                    // liveMarkdown 让生成资源输出在终态检查点到达前即可见。
                    let live_markdown = markdown.clone();
                    sink.push_delta(ProviderTurnDelta::Output { delta: markdown });
                    let _ = live_sender.send(Ok(ProviderStreamDelta::Output(live_markdown)));
                }
            }
            AiStreamEvent::ReasoningStarted { item_id } => {
                self.register_replay_item(
                    item_id,
                    ReplayAccumulatorItemState::Reasoning {
                        text: String::new(),
                        signature: None,
                    },
                )?;
            }
            AiStreamEvent::ReasoningDelta { item_id, delta } => {
                let ReplayAccumulatorItemState::Reasoning { text, .. } =
                    self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public reasoning delta targeted a non-reasoning item.",
                    ));
                };
                text.push_str(delta.as_str());
                self.reasoning.push_str(delta.as_str());
                // liveDelta projects provider reasoning without removing it from Core's canonical response.
                // liveDelta 投影 Provider 推理内容，同时不从 Core 规范响应中移除该内容。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Thinking { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Thinking(live_delta)));
            }
            AiStreamEvent::ReasoningCompleted { item_id, signature } => {
                let ReplayAccumulatorItemState::Reasoning {
                    signature: current_signature,
                    ..
                } = self.replay_item_state_mut(item_id.as_str())?
                else {
                    return Err(provider_failure(
                        provider_id,
                        "Public reasoning completion targeted a non-reasoning item.",
                    ));
                };
                *current_signature = signature;
            }
            AiStreamEvent::ToolCallStarted {
                item_id,
                call_id,
                tool_name,
                ..
            } => {
                self.register_replay_item(
                    item_id.clone(),
                    ReplayAccumulatorItemState::PendingTool,
                )?;
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Detected {
                        request_stream_id: item_id,
                        tool_call_id: call_id,
                        tool_name,
                        initial_arguments: String::new(),
                        argument_assembler: None,
                    },
                )));
            }
            AiStreamEvent::ToolCallArgumentsDelta { item_id, delta, .. } => {
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::ArgumentsDelta {
                        request_stream_id: item_id,
                        delta,
                    },
                )));
            }
            AiStreamEvent::ToolCallCompleted { item_id, call } => {
                let replay_state = self.replay_item_state_mut(item_id.as_str())?;
                if !matches!(replay_state, ReplayAccumulatorItemState::PendingTool) {
                    return Err(provider_failure(
                        provider_id,
                        "Public tool completion targeted a non-tool item.",
                    ));
                }
                *replay_state = ReplayAccumulatorItemState::Complete(ContentItem::ToolCall {
                    call: call.clone(),
                });
                let final_arguments = serde_json::to_string(&call.arguments).map_err(|error| {
                    provider_failure(
                        provider_id,
                        format!("Public tool arguments could not be serialized: {error}").as_str(),
                    )
                })?;
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Completed {
                        request_stream_id: item_id,
                        canonical_tool_call_id: Some(call.id.clone()),
                        final_arguments: Some(final_arguments),
                    },
                )));
                self.tool_calls.push(ConversationToolCall {
                    id: call.id.clone(),
                    tool_call_id: Some(call.id),
                    tool_name: call.name,
                    arguments: call.arguments,
                });
            }
            AiStreamEvent::ToolCallAborted { item_id, message } => {
                let _ = live_sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
                    ToolRequestFragment::Aborted {
                        request_stream_id: item_id,
                        reason: message.clone(),
                    },
                )));
                return Err(provider_failure(provider_id, message.as_str()));
            }
            AiStreamEvent::UsageUpdated { usage } => {
                if let Some(current) = self.usage.as_mut() {
                    current.merge_update(usage);
                } else {
                    self.usage = Some(usage);
                }
            }
            AiStreamEvent::ResponseCompleted => {
                self.completed = true;
            }
            AiStreamEvent::ResponseFailed { error } => {
                return Err(public_execution_failure(provider_id, error));
            }
            AiStreamEvent::ResponseCancelled => {
                return Err(provider_failure(
                    provider_id,
                    "Provider execution was cancelled.",
                ));
            }
            AiStreamEvent::ResponseStarted { .. }
            | AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::ResponseItemCompleted { .. } => {}
        }
        Ok(())
    }

    /// Finishes the controller response after a public terminal event.
    /// 在公共终态事件后完成控制器响应。
    fn finish(self) -> Result<CoreProviderTurnResponse, VulcanCoreError> {
        if !self.completed {
            return Err(provider_failure(
                self.provider_id.as_str(),
                "Provider stream ended without a successful terminal response event.",
            ));
        }
        let replay_content = self.replay_content()?;
        let provider_message_state = Some(json!({
            "vulcan_ai": {
                "replay": ProviderReplayEnvelope::new(
                    self.provider_id.clone(),
                    self.protocol_profile_id.clone(),
                    self.model_id.clone(),
                    replay_content,
                ),
            }
        }));
        Ok(CoreProviderTurnResponse {
            assistant_text: self.text,
            assistant_reasoning: self.reasoning,
            provider_message_id: self.provider_message_id.clone(),
            provider_message_state,
            usage: self.usage.as_ref().map(public_usage_to_core),
            tool_calls: self.tool_calls,
            continuation: Some(ProviderContinuation {
                provider_id: Some(self.provider_id),
                protocol_profile_id: Some(self.protocol_profile_id),
                model_id: Some(self.model_id),
                last_provider_message_id: self.provider_message_id,
                ..ProviderContinuation::default()
            }),
        })
    }
}

/// Materializes one provider-generated Data URL into a host-owned workspace file.
/// 将供应商生成的 Data URL 落盘为宿主自有的工作区文件。
///
/// # Parameters
/// 参数
/// `provider_id` identifies the failing route in controlled diagnostics.
/// `provider_id` 在受控诊断中标识失败路由。
/// `workspace_path` is the canonical workspace that owns generated outputs.
/// `workspace_path` 是拥有生成输出的规范工作区。
/// `event` is one normalized provider event.
/// `event` 是一项规范化供应商事件。
///
/// # Returns
/// 返回值
/// The unchanged event when no generated resource is present, or the same resource event with
/// a local path after managed attachment staging and index publication.
/// 当不存在生成资源时原样返回事件；否则在托管附件暂存与索引发布后返回带本地路径的同一资源事件。
async fn materialize_provider_resource_event(
    provider_id: &str,
    workspace_path: &str,
    event: AiStreamEvent,
) -> Result<AiStreamEvent, VulcanCoreError> {
    let AiStreamEvent::ResponseItemStarted {
        item_id,
        item: ContentItem::Resource { mut resource },
    } = event
    else {
        return Ok(event);
    };
    let Some(data_url) = resource.url.as_deref() else {
        return Ok(AiStreamEvent::ResponseItemStarted {
            item_id,
            item: ContentItem::Resource { resource },
        });
    };
    if !data_url.starts_with("data:") {
        return Ok(AiStreamEvent::ResponseItemStarted {
            item_id,
            item: ContentItem::Resource { resource },
        });
    }
    let (media_type, encoded) = parse_generated_data_url(provider_id, data_url)?;
    if resource.kind != ResourceKind::Image {
        return Err(provider_failure(
            provider_id,
            "Only provider-generated image Data URLs are currently registered.",
        ));
    }
    let descriptor = generated_image_descriptor(provider_id, media_type)?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|error| {
            provider_failure(
                provider_id,
                format!("Provider-generated image Base64 is invalid: {error}").as_str(),
            )
        })?;
    const MAX_GENERATED_IMAGE_BYTES: usize = 50 * 1024 * 1024;
    if bytes.len() > MAX_GENERATED_IMAGE_BYTES {
        return Err(provider_failure(
            provider_id,
            "Provider-generated image exceeds the 50 MiB host limit.",
        ));
    }
    if workspace_path.trim().is_empty() {
        return Err(provider_failure(
            provider_id,
            "Generated output requires a workspace path.",
        ));
    }
    let workspace_path = workspace_path.to_string();
    let suggested_file_name = format!(
        "{}.{}",
        safe_generated_resource_stem(resource.id.as_str()),
        descriptor.extension
    );
    let attachment = tokio::task::spawn_blocking(move || {
        stage_attachment_bytes(
            workspace_path.as_str(),
            &descriptor,
            suggested_file_name.as_str(),
            None,
            bytes.as_slice(),
        )
    })
    .await
    .map_err(|error| {
        provider_failure(
            provider_id,
            format!("Generated output staging task failed: {error}").as_str(),
        )
    })?
    .map_err(|error| {
        provider_failure(
            provider_id,
            format!("Generated output could not enter managed attachment storage: {error}")
                .as_str(),
        )
    })?;
    resource.media_type = Some(media_type.to_string());
    resource.local_path = Some(attachment.local_path);
    resource.url = None;
    Ok(AiStreamEvent::ResponseItemStarted {
        item_id,
        item: ContentItem::Resource { resource },
    })
}

/// Parses one exact Base64 Data URL returned by a provider resource event.
/// 解析供应商资源事件返回的一项精确 Base64 Data URL。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled errors and `data_url` is the exact returned URL.
/// `provider_id` 标识受控错误，`data_url` 是精确返回的 URL。
///
/// # Returns
/// 返回值
/// The declared MIME type and encoded payload.
/// 返回声明的 MIME 类型与编码载荷。
fn parse_generated_data_url<'a>(
    provider_id: &str,
    data_url: &'a str,
) -> Result<(&'a str, &'a str), VulcanCoreError> {
    let body = data_url.strip_prefix("data:").ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated resource URL is not a Data URL.",
        )
    })?;
    let (declaration, encoded) = body.split_once(',').ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated Data URL is missing its payload separator.",
        )
    })?;
    let media_type = declaration.strip_suffix(";base64").ok_or_else(|| {
        provider_failure(
            provider_id,
            "Provider-generated Data URL is not Base64 encoded.",
        )
    })?;
    if encoded.is_empty() {
        return Err(provider_failure(
            provider_id,
            "Provider-generated Data URL payload is empty.",
        ));
    }
    Ok((media_type, encoded))
}

/// Resolves a registered generated-image MIME type to its managed attachment descriptor.
/// 将已注册的生成图片 MIME 类型解析为托管附件描述。
///
/// # Parameters
/// 参数
/// `provider_id` identifies controlled errors and `media_type` is the exact declared MIME type.
/// `provider_id` 标识受控错误，`media_type` 是精确声明的 MIME 类型。
///
/// # Returns
/// 返回值
/// A stable managed attachment descriptor or a controlled error for an unregistered type.
/// 返回稳定托管附件描述；未注册类型则返回受控错误。
fn generated_image_descriptor(
    provider_id: &str,
    media_type: &str,
) -> Result<AttachmentDescriptor, VulcanCoreError> {
    match media_type {
        "image/png" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/png",
            extension: "png",
        }),
        "image/jpeg" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/jpeg",
            extension: "jpg",
        }),
        "image/webp" => Ok(AttachmentDescriptor {
            kind: ConversationAttachmentKind::Image,
            mime_type: "image/webp",
            extension: "webp",
        }),
        other => Err(provider_failure(
            provider_id,
            format!("Generated image MIME type `{other}` is not registered.").as_str(),
        )),
    }
}

/// Produces a filesystem-safe stem from one provider-neutral resource identifier.
/// 从一个供应商无关资源标识生成文件系统安全的主文件名。
///
/// # Parameters
/// 参数
/// `resource_id` is the stable normalized resource identifier.
/// `resource_id` 是稳定的规范化资源标识。
///
/// # Returns
/// 返回值
/// A non-empty ASCII filename stem.
/// 返回非空 ASCII 文件主名。
fn safe_generated_resource_stem(resource_id: &str) -> String {
    let stem = resource_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .take(80)
        .collect::<String>();
    if stem.is_empty() {
        String::from("generated-image")
    } else {
        stem
    }
}

/// Resolves every local message resource into an immutable request-scoped URL or data URL.
/// 将每个本地消息资源解析为不可变的请求作用域原生 URL 或 Data URL。
///
/// # Parameters
/// 参数
/// `message` is the public message being prepared, `route` fixes the selected protocol,
/// and `model_resource_kinds` is the exact discovered model capability snapshot.
/// `message` 是正在准备的公共消息，`route` 固定所选协议，
/// `model_resource_kinds` 是精确发现的模型能力快照。
///
/// # Returns
/// 返回值
/// Returns success after every resource is validated and resolved, or a controlled
/// pre-dispatch error before any provider request is sent.
/// 在每个资源完成校验与解析后返回成功；否则在发送任何供应商请求前返回受控错误。
async fn resolve_public_message_resources(
    message: &mut AiMessage,
    route: &ProviderExecutionRoute,
    model_id: &str,
    model_resource_kinds: &[ResourceKind],
    client: &ProviderExecutionClient,
) -> Result<(), VulcanCoreError> {
    for item in &mut message.content {
        let ContentItem::Resource { resource } = item else {
            continue;
        };
        validate_resource_projection(route, model_id, model_resource_kinds, resource)?;
        if resource.url.is_some() {
            continue;
        }
        let local_path = resource.local_path.as_deref().ok_or_else(|| {
            provider_failure(
                route.provider.id.as_str(),
                format!(
                    "Resource `{}` has neither a local path nor a URL.",
                    resource.id
                )
                .as_str(),
            )
        })?;
        let media_type = resource.media_type.as_deref().ok_or_else(|| {
            provider_failure(
                route.provider.id.as_str(),
                format!("Resource `{}` is missing its MIME type.", resource.id).as_str(),
            )
        })?;
        if let Some(prepared_url) = client.prepared_media_url(
            route,
            model_id,
            std::path::Path::new(local_path),
            resource.kind,
            media_type,
        ) {
            resource.url = Some(prepared_url);
            continue;
        }
        if let Some(byte_limit) = minimax_local_resource_byte_limit(route, model_id, resource) {
            let metadata = tokio::fs::metadata(local_path).await.map_err(|error| {
                provider_failure(
                    route.provider.id.as_str(),
                    format!(
                        "Resource `{}` metadata could not be read: {error}",
                        resource.id
                    )
                    .as_str(),
                )
            })?;
            if metadata.len() > byte_limit {
                return Err(provider_failure(
                    route.provider.id.as_str(),
                    format!(
                        "Resource `{}` is {} bytes and exceeds the MiniMax M3 limit of {byte_limit} bytes.",
                        resource.id,
                        metadata.len()
                    )
                    .as_str(),
                ));
            }
        }
        let bytes = tokio::fs::read(local_path).await.map_err(|error| {
            provider_failure(
                route.provider.id.as_str(),
                format!("Resource `{}` could not be read: {error}", resource.id).as_str(),
            )
        })?;
        if let Some(byte_limit) = minimax_local_resource_byte_limit(route, model_id, resource)
            && u64::try_from(bytes.len()).unwrap_or(u64::MAX) > byte_limit
        {
            return Err(provider_failure(
                route.provider.id.as_str(),
                format!(
                    "Resource `{}` changed while reading and now exceeds the MiniMax M3 limit of {byte_limit} bytes.",
                    resource.id
                )
                .as_str(),
            ));
        }
        resource.url = Some(format!(
            "data:{media_type};base64,{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        ));
    }
    Ok(())
}

/// Validates one resource against both model discovery and the selected protocol projector.
/// 同时依据模型发现结果与所选协议投影器校验一个资源。
///
/// # Parameters
/// 参数
/// `route` identifies the provider and selected protocol, `model_resource_kinds` contains
/// exact model declarations, and `resource` is the requested resource.
/// `route` 标识供应商与所选协议，`model_resource_kinds` 包含精确模型声明，
/// `resource` 是请求使用的资源。
///
/// # Returns
/// 返回值
/// Returns success only when both capability layers support the resource.
/// 仅当两层能力均支持该资源时返回成功。
fn validate_resource_projection(
    route: &ProviderExecutionRoute,
    model_id: &str,
    model_resource_kinds: &[ResourceKind],
    resource: &ResourceReference,
) -> Result<(), VulcanCoreError> {
    if !model_resource_kinds.contains(&resource.kind) {
        return Err(provider_failure(
            route.provider.id.as_str(),
            format!(
                "Model `{}` does not declare {:?} input.",
                model_id, resource.kind
            )
            .as_str(),
        ));
    }
    let protocol = route.protocol_route.protocol_profile_id.as_str();
    let protocol_supports = match protocol {
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID => true,
        vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        | vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID => {
            resource.kind == ResourceKind::Image
        }
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID
        | vulcan_ai_protocols::CLAUDE_CODE_MESSAGES_PROFILE_ID => {
            resource.kind == ResourceKind::Image
        }
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID => {
            matches!(resource.kind, ResourceKind::Image | ResourceKind::Video)
        }
        vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID
        | vulcan_ai_protocols::GEMINI_INTERACTIONS_PROFILE_ID
        | vulcan_ai_protocols::GOOGLE_CODE_ASSIST_PROFILE_ID => true,
        _ => false,
    };
    if !protocol_supports {
        return Err(provider_failure(
            route.provider.id.as_str(),
            format!(
                "Protocol profile `{protocol}` has no registered {:?} input projector.",
                resource.kind
            )
            .as_str(),
        ));
    }
    if resource.kind == ResourceKind::Audio && resource.url.is_some() {
        return Err(provider_failure(
            route.provider.id.as_str(),
            "OpenRouter audio input requires a local Base64 payload and does not accept URLs.",
        ));
    }
    if resource.kind == ResourceKind::File
        && resource.media_type.as_deref() != Some("application/pdf")
    {
        return Err(provider_failure(
            route.provider.id.as_str(),
            "The registered OpenRouter file projector currently accepts PDF resources only.",
        ));
    }
    if route.provider.id == "minimax"
        && model_id == "MiniMax-M3"
        && resource.kind == ResourceKind::Video
        && !matches!(
            resource.media_type.as_deref(),
            Some(
                "video/mp4"
                    | "video/x-msvideo"
                    | "video/quicktime"
                    | "video/mov"
                    | "video/x-matroska"
            )
        )
    {
        return Err(provider_failure(
            route.provider.id.as_str(),
            "MiniMax M3 video input accepts MP4, AVI, MOV, and MKV MIME types only.",
        ));
    }
    Ok(())
}

/// Returns the exact MiniMax local-resource byte limit for a supported M3 resource.
/// 返回受支持 M3 资源的精确 MiniMax 本地资源字节上限。
///
/// # Parameters
/// 参数
/// `route` and `model_id` identify the exact provider/model contract; `resource` supplies
/// the resource kind.
/// `route` 与 `model_id` 标识精确供应商/模型契约；`resource` 提供资源类别。
///
/// # Returns
/// 返回值
/// A documented byte limit for MiniMax M3 image/video input, otherwise no host limit.
/// 对 MiniMax M3 图像/视频输入返回文档化字节限制，否则不返回宿主限制。
fn minimax_local_resource_byte_limit(
    route: &ProviderExecutionRoute,
    model_id: &str,
    resource: &ResourceReference,
) -> Option<u64> {
    if route.provider.id != "minimax" || model_id != "MiniMax-M3" {
        return None;
    }
    match resource.kind {
        ResourceKind::Image => Some(MINIMAX_M3_IMAGE_BYTE_LIMIT),
        ResourceKind::Video => Some(MINIMAX_M3_VIDEO_BYTE_LIMIT),
        _ => None,
    }
}

/// Converts one canonical controller message into the public contract.
/// 将一条规范控制器消息转换为公共契约。
fn core_message_to_public(
    message: &vulcan_core::types::ConversationMessage,
    provider_id: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<AiMessage, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, attachments } => {
            let mut content = vec![ContentItem::Text { text: text.clone() }];
            for (index, attachment) in attachments.iter().enumerate() {
                let resource_id = attachment
                    .asset_id
                    .clone()
                    .unwrap_or_else(|| format!("{}:attachment:{index}", message.id));
                let kind = match attachment.kind {
                    ConversationAttachmentKind::Audio => ResourceKind::Audio,
                    ConversationAttachmentKind::Image => ResourceKind::Image,
                    ConversationAttachmentKind::Video => ResourceKind::Video,
                    ConversationAttachmentKind::Pdf => ResourceKind::File,
                    ConversationAttachmentKind::File => ResourceKind::File,
                };
                content.push(ContentItem::Resource {
                    resource: ResourceReference {
                        id: resource_id,
                        kind,
                        media_type: Some(attachment.mime_type.clone()),
                        local_path: Some(attachment.local_path.clone()),
                        url: None,
                    },
                });
            }
            Ok(AiMessage {
                role: MessageRole::User,
                content,
            })
        }
        CoreConversationPayload::Assistant {
            text,
            reasoning,
            provider_message_state,
            tool_calls,
            ..
        } => {
            if let Some(content) = replay_content_for_route(
                provider_message_state.as_ref(),
                provider_id,
                protocol_profile_id,
                model_id,
            )? {
                return Ok(AiMessage {
                    role: MessageRole::Assistant,
                    content,
                });
            }
            let mut content = Vec::new();
            if !reasoning.is_empty() {
                content.push(ContentItem::Reasoning {
                    text: reasoning.clone(),
                    signature: None,
                });
            }
            if !text.is_empty() {
                content.push(ContentItem::Text { text: text.clone() });
            }
            content.extend(tool_calls.iter().map(|call| ContentItem::ToolCall {
                call: PublicToolCall {
                    id: call.tool_call_id.clone().unwrap_or_else(|| call.id.clone()),
                    name: call.tool_name.clone(),
                    arguments: call.arguments.clone(),
                    signature: None,
                },
            }));
            Ok(AiMessage {
                role: MessageRole::Assistant,
                content,
            })
        }
        CoreConversationPayload::ToolResult {
            tool_call_id,
            is_error,
            text,
        } => {
            let call_id = tool_call_id.clone().ok_or_else(|| {
                provider_failure(
                    "vulcan",
                    "Canonical tool result is missing its provider-visible call identifier.",
                )
            })?;
            Ok(AiMessage {
                role: MessageRole::Tool,
                content: vec![ContentItem::ToolResult {
                    result: ToolResult {
                        call_id,
                        is_error: *is_error,
                        content: Value::String(text.clone()),
                    },
                }],
            })
        }
        CoreConversationPayload::AgentMessage { .. } => Ok(AiMessage {
            role: MessageRole::User,
            content: vec![ContentItem::Text {
                text: message
                    .payload
                    .agent_message_model_text()
                    .ok_or_else(|| provider_failure("vulcan", "Agent message rendering failed."))?,
            }],
        }),
    }
}

/// Reads durable ordered replay content only for the exact active execution route.
/// 仅为当前精确执行路由读取可持久化的有序重放内容。
///
/// # Parameters
/// 参数
/// `provider_message_state` is the Core-persisted assistant extension; the remaining identifiers
/// define the currently selected execution route.
/// `provider_message_state` 是 Core 持久化的助手扩展；其余标识定义当前选定执行路由。
///
/// # Returns
/// 返回值
/// Exact ordered content for a matching route, no replay for a different route, or a controlled
/// error when a declared replay envelope is malformed.
/// 路由匹配时返回精确有序内容；路由不同时不重放；已声明封套损坏时返回受控错误。
fn replay_content_for_route(
    provider_message_state: Option<&Value>,
    provider_id: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> Result<Option<Vec<ContentItem>>, VulcanCoreError> {
    let Some(replay_value) =
        provider_message_state.and_then(|state| state.pointer("/vulcan_ai/replay"))
    else {
        return Ok(None);
    };
    let envelope: ProviderReplayEnvelope =
        serde_json::from_value(replay_value.clone()).map_err(|error| {
            provider_failure(
                provider_id,
                format!("Persisted public replay envelope is invalid: {error}").as_str(),
            )
        })?;
    if !envelope.matches_route(provider_id, protocol_profile_id, model_id) {
        return Ok(None);
    }
    Ok(Some(envelope.content))
}

/// Returns the model-visible text of one canonical controller message.
/// 返回一条规范控制器消息的模型可见文本。
fn core_message_text(
    message: &vulcan_core::types::ConversationMessage,
) -> Result<String, VulcanCoreError> {
    match &message.payload {
        CoreConversationPayload::UserText { text, .. } => Ok(text.clone()),
        CoreConversationPayload::Assistant { text, .. } => Ok(text.clone()),
        CoreConversationPayload::ToolResult { text, .. } => Ok(text.clone()),
        CoreConversationPayload::AgentMessage { .. } => message
            .payload
            .agent_message_model_text()
            .ok_or_else(|| provider_failure("vulcan", "Agent message rendering failed.")),
    }
}

/// Converts public usage into the controller's temporary accounting shape.
/// 将公共用量转换为控制器的临时统计结构。
fn public_usage_to_core(usage: &RequestUsage) -> ProviderUsageSnapshot {
    ProviderUsageSnapshot {
        input_tokens: known_usage(&usage.input_tokens),
        uncached_input_tokens: known_usage(&usage.uncached_input_tokens),
        output_tokens: known_usage(&usage.output_tokens),
        reasoning_tokens: known_usage(&usage.reasoning_tokens),
        total_tokens: known_usage(&usage.total_tokens),
        cached_input_tokens: known_usage(&usage.cache_read_tokens),
        cache_creation_input_tokens: known_usage(&usage.cache_write_tokens),
    }
}

/// Converts one known public token count into the controller's platform size.
/// 将一个已知公共 Token 数转换为控制器平台大小。
fn known_usage(value: &UsageValue) -> Option<usize> {
    match value {
        UsageValue::Unknown => None,
        UsageValue::Known { value, .. } => usize::try_from(*value).ok(),
    }
}

/// Creates one controller provider failure.
/// 创建一个控制器供应商失败。
fn provider_failure(provider_id: &str, message: &str) -> VulcanCoreError {
    VulcanCoreError::ProviderFailure {
        provider_id: provider_id.to_string(),
        message: message.to_string(),
        details: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// RecordingProviderDeltaSink retains canonical Core deltas for adapter projection tests.
    /// RecordingProviderDeltaSink 为适配器投影测试保留规范 Core 增量。
    #[derive(Default)]
    struct RecordingProviderDeltaSink {
        /// deltas contains every canonical delta received from the public adapter.
        /// deltas 包含公共适配器发出的每一项规范增量。
        deltas: Vec<ProviderTurnDelta>,
    }

    impl VulcanProviderDeltaSink for RecordingProviderDeltaSink {
        /// Stores one canonical provider delta.
        /// 保存一项规范 Provider 增量。
        ///
        /// # Parameters
        /// 参数
        /// `delta` is the exact decoded delta produced by the public protocol.
        /// `delta` 是公共协议产出的精确解码增量。
        fn push_delta(&mut self, delta: ProviderTurnDelta) {
            self.deltas.push(delta);
        }
    }

    /// Verifies split Anthropic usage events retain start-event cache fields and terminal output fields.
    /// 验证拆分的 Anthropic 用量事件会同时保留开始事件的缓存字段与终态输出字段。
    #[test]
    fn partial_usage_updates_merge_without_erasing_known_fields() {
        let reported = |value| UsageValue::Known {
            value,
            source: vulcan_ai_contract::UsageValueSource::Reported,
        };
        let mut usage = RequestUsage {
            input_tokens: reported(1_000),
            cache_read_tokens: reported(800),
            cache_write_tokens: reported(120),
            ..RequestUsage::default()
        };

        usage.merge_update(RequestUsage {
            output_tokens: reported(75),
            total_tokens: reported(1_075),
            ..RequestUsage::default()
        });

        assert_eq!(usage.input_tokens, reported(1_000));
        assert_eq!(usage.cache_read_tokens, reported(800));
        assert_eq!(usage.cache_write_tokens, reported(120));
        assert_eq!(usage.output_tokens, reported(75));
        assert_eq!(usage.total_tokens, reported(1_075));
    }

    /// Verifies public text and reasoning deltas reach both Core and the live workbench stream.
    /// 验证公共文本与推理增量同时到达 Core 和实时工作台流。
    #[test]
    fn public_turn_deltas_are_forwarded_to_live_workbench_stream() {
        let (live_sender, mut live_receiver) = tokio::sync::mpsc::unbounded_channel();
        let mut sink = RecordingProviderDeltaSink::default();
        let mut accumulator =
            PublicTurnAccumulator::new("minimax", "anthropic-messages", "MiniMax-M3");

        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ResponseItemStarted {
                    item_id: String::from("text-item"),
                    item: ContentItem::Text {
                        text: String::new(),
                    },
                },
            )
            .expect("text item should start");
        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ResponseTextDelta {
                    item_id: String::from("text-item"),
                    delta: String::from("最终答案"),
                },
            )
            .expect("text delta should be accepted");
        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ReasoningStarted {
                    item_id: String::from("reasoning-item"),
                },
            )
            .expect("reasoning item should start");
        accumulator
            .apply_event(
                "minimax",
                &live_sender,
                &mut sink,
                AiStreamEvent::ReasoningDelta {
                    item_id: String::from("reasoning-item"),
                    delta: String::from("先分析"),
                },
            )
            .expect("reasoning delta should be accepted");

        assert!(matches!(
            sink.deltas.as_slice(),
            [
                ProviderTurnDelta::Output { delta: output },
                ProviderTurnDelta::Thinking {
                    delta: reasoning
                },
            ] if output == "最终答案" && reasoning == "先分析"
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::Output(output))) if output == "最终答案"
        ));
        assert!(matches!(
            live_receiver.try_recv(),
            Ok(Ok(ProviderStreamDelta::Thinking(reasoning))) if reasoning == "先分析"
        ));
    }

    /// Verifies generated image bytes enter managed attachment storage and the Data URL is discarded.
    /// 验证生成图片字节会进入托管附件存储，且 Data URL 会被丢弃。
    #[tokio::test]
    async fn generated_image_event_materializes_inside_workspace() {
        let workspace = std::env::temp_dir().join(format!(
            "vulcan-generated-resource-test-{}",
            uuid::Uuid::new_v4()
        ));
        tokio::fs::create_dir_all(&workspace)
            .await
            .expect("test workspace should be created");
        let event = AiStreamEvent::ResponseItemStarted {
            item_id: String::from("item"),
            item: ContentItem::Resource {
                resource: ResourceReference {
                    id: String::from("image:item"),
                    kind: ResourceKind::Image,
                    media_type: Some(String::from("image/png")),
                    local_path: None,
                    url: Some(String::from("data:image/png;base64,aW1hZ2U=")),
                },
            },
        };
        let materialized = materialize_provider_resource_event(
            "openai",
            workspace.to_string_lossy().as_ref(),
            event,
        )
        .await
        .expect("generated image should materialize");
        let AiStreamEvent::ResponseItemStarted {
            item: ContentItem::Resource { resource },
            ..
        } = materialized
        else {
            panic!("materialized event should remain a resource item");
        };
        assert!(resource.url.is_none());
        let local_path = PathBuf::from(
            resource
                .local_path
                .expect("materialized resource should have a local path"),
        );
        assert!(local_path.starts_with(&workspace));
        assert_eq!(
            tokio::fs::read(&local_path)
                .await
                .expect("materialized image should be readable"),
            b"image"
        );
        tokio::fs::remove_dir_all(&workspace)
            .await
            .expect("test workspace should be removed");
    }

    /// Verifies a user-safe remote resource URL is preserved instead of being decoded as Base64.
    /// 验证用户安全的远程资源 URL 会被保留，而不会被当作 Base64 解码。
    #[tokio::test]
    async fn remote_generated_image_url_remains_remote() {
        let event = AiStreamEvent::ResponseItemStarted {
            item_id: String::from("item"),
            item: ContentItem::Resource {
                resource: ResourceReference {
                    id: String::from("remote-image"),
                    kind: ResourceKind::Image,
                    media_type: Some(String::from("image/png")),
                    local_path: None,
                    url: Some(String::from("https://example.invalid/image.png")),
                },
            },
        };
        let preserved = materialize_provider_resource_event("openai", "C:\\workspace", event)
            .await
            .expect("remote resource should remain valid");
        assert!(matches!(
            preserved,
            AiStreamEvent::ResponseItemStarted {
                item: ContentItem::Resource { resource },
                ..
            } if resource.url.as_deref() == Some("https://example.invalid/image.png")
                && resource.local_path.is_none()
        ));
    }

    /// Verifies generated resource file stems cannot escape the host-owned output directory.
    /// 验证生成资源文件主名无法逃逸宿主自有输出目录。
    #[test]
    fn generated_resource_stem_removes_path_syntax() {
        assert_eq!(
            safe_generated_resource_stem("../../image:item"),
            "------image-item"
        );
    }

    /// Verifies a catalog maximum does not become the default request output budget.
    /// 验证目录最大值不会变成默认请求输出预算。
    #[test]
    fn public_output_limit_uses_default_below_catalog_maximum() {
        assert_eq!(
            resolved_public_output_token_limit(None, Some(393_216)),
            DEFAULT_PUBLIC_OUTPUT_TOKEN_LIMIT
        );
    }

    /// Verifies user narrowing remains authoritative and cannot exceed the catalog maximum.
    /// 验证用户收窄值保持权威且不能超过目录最大值。
    #[test]
    fn public_output_limit_honors_user_narrowing_and_catalog_cap() {
        assert_eq!(
            resolved_public_output_token_limit(Some(16_000), Some(393_216)),
            16_000
        );
        assert_eq!(
            resolved_public_output_token_limit(Some(500_000), Some(393_216)),
            393_216
        );
    }

    /// Verifies a cursor produced by another protocol profile is never relabeled and reused.
    /// 验证由其他协议配置产生的游标绝不会被重新标记并复用。
    #[test]
    fn continuation_rejects_protocol_profile_switch() {
        let config = CoreControllerConfig {
            provider_id: String::from("minimax"),
            model_id: String::from("MiniMax-M3"),
            system_prompt: String::from("system"),
            ..CoreControllerConfig::default()
        };
        let snapshot = CoreConversationSnapshot {
            messages: vec![vulcan_core::types::ConversationMessage {
                id: String::from("assistant-1"),
                payload: CoreConversationPayload::Assistant {
                    text: String::from("answer"),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("response-1")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            continuation: ProviderContinuation {
                provider_id: Some(String::from("minimax")),
                protocol_profile_id: Some(String::from("anthropic-messages")),
                model_id: Some(String::from("MiniMax-M3")),
                last_provider_message_id: Some(String::from("response-1")),
                last_local_message_id: Some(String::from("assistant-1")),
                context_rewrite_fingerprint: Some(config.provider_rewrite_fingerprint()),
                ..ProviderContinuation::default()
            },
            ..CoreConversationSnapshot::default()
        };

        assert!(
            build_public_continuation(
                &snapshot,
                &config,
                "minimax",
                "openai-chat",
                "MiniMax-M3",
                1,
            )
            .is_none()
        );
    }
}
