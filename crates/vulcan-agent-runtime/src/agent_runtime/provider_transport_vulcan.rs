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
use vulcan_ai_contract::{
    AiExecutionRequest, AiMessage, AiOperation, AiStreamEvent, ContentItem, MessageRole,
    ModelParameterValue, NativeToolKind, PromptCachePolicy, ReasoningEffort, RequestUsage,
    ResourceKind, ResourceReference, ToolCall as PublicToolCall, ToolDefinition, ToolResult,
    UsageValue,
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
        output_token_limit: u64,
        prompt_cache: PromptCachePolicy,
        reasoning_effort: Option<ReasoningEffort>,
        model_parameters: std::collections::BTreeMap<String, ModelParameterValue>,
        input_resources: Vec<ResourceKind>,
        output_resources: Vec<ResourceKind>,
        workspace_path: String,
        live_sender: ProviderLiveDeltaSender,
    ) -> Self {
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
                    input_schema: tool.parameters,
                })
                .collect(),
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
            let mut public_message = core_message_to_public(message)?;
            resolve_public_message_resources(
                &mut public_message,
                &self.route,
                self.model_id.as_str(),
                self.input_resources.as_slice(),
            )
            .await?;
            messages.push(public_message);
        }
        let public = AiExecutionRequest {
            operation: AiOperation::ConversationRespond,
            model_id: config.model_id.clone(),
            cache_scope_id: Some(self.cache_scope_id.clone()),
            messages,
            tools: self.tools.clone(),
            native_tools: if self.route.protocol_route.protocol_profile_id
                == vulcan_ai_protocols::OPENAI_CODEX_RESPONSES_PROFILE_ID
                && self.output_resources.contains(&ResourceKind::Image)
            {
                vec![NativeToolKind::ImageGeneration]
            } else {
                Vec::new()
            },
            prompt_cache: self.prompt_cache.clone(),
            reasoning_effort: self.reasoning_effort,
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
        let mut response = PublicTurnAccumulator::default();
        while let Some(item) = stream.next().await {
            let event = item.map_err(|failure| {
                provider_failure(
                    self.provider_id(),
                    public_execution_failure_message(&failure.error).as_str(),
                )
            })?;
            let event = materialize_provider_resource_event(
                self.provider_id(),
                self.workspace_path.as_str(),
                event,
            )
            .await?;
            response.apply_event(self.provider_id(), &self.live_sender, sink, event)?;
        }
        response.finish(self.provider_id(), self.model_id())
    }
}

/// Formats one public execution error for the temporary controller error boundary.
/// 为临时控制器错误边界格式化一个公共执行错误。
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
    if let Some(seconds) = error.retry_after_seconds {
        fields.push(format!("retry_after_ms={}", seconds.saturating_mul(1_000)));
    }
    format!("{} | {}", fields.join(" | "), error.message)
}

/// Accumulates public stream events into the controller's completed-turn shape.
/// 将公共流事件累积为控制器的完整回合结构。
#[derive(Debug, Default)]
struct PublicTurnAccumulator {
    /// Flattened visible assistant output.
    /// 扁平化的可见助手输出。
    text: String,
    /// Flattened reasoning output.
    /// 扁平化的推理输出。
    reasoning: String,
    /// Last continuation signature emitted by the protocol.
    /// 协议发出的最后一个延续签名。
    reasoning_signature: Option<String>,
    /// Complete tool calls.
    /// 完整工具调用。
    tool_calls: Vec<ConversationToolCall>,
    /// Last public usage update.
    /// 最后一个公共用量更新。
    usage: Option<RequestUsage>,
    /// Whether a successful terminal response event was observed.
    /// 是否已经观察到成功响应终态事件。
    completed: bool,
}

impl PublicTurnAccumulator {
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
            AiStreamEvent::ResponseTextDelta { delta, .. } => {
                self.text.push_str(delta.as_str());
                // liveDelta preserves the same decoded text for the workbench stream while Core keeps its canonical copy.
                // liveDelta 在 Core 保留规范副本的同时，将同一段已解码文本投影到工作台流。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Output { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Output(live_delta)));
            }
            AiStreamEvent::ResponseItemStarted {
                item:
                    ContentItem::Resource {
                        resource:
                            ResourceReference {
                                kind: ResourceKind::Image,
                                local_path,
                                url,
                                ..
                            },
                    },
                ..
            } => {
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
            AiStreamEvent::ReasoningDelta { delta, .. } => {
                self.reasoning.push_str(delta.as_str());
                // liveDelta projects provider reasoning without removing it from Core's canonical response.
                // liveDelta 投影 Provider 推理内容，同时不从 Core 规范响应中移除该内容。
                let live_delta = delta.clone();
                sink.push_delta(ProviderTurnDelta::Thinking { delta });
                let _ = live_sender.send(Ok(ProviderStreamDelta::Thinking(live_delta)));
            }
            AiStreamEvent::ReasoningCompleted { signature, .. } => {
                if signature.is_some() {
                    self.reasoning_signature = signature;
                }
            }
            AiStreamEvent::ToolCallStarted {
                item_id,
                call_id,
                tool_name,
                ..
            } => {
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
                merge_usage_update(&mut self.usage, usage);
            }
            AiStreamEvent::ResponseCompleted => {
                self.completed = true;
            }
            AiStreamEvent::ResponseFailed { error } => {
                return Err(provider_failure(provider_id, error.message.as_str()));
            }
            AiStreamEvent::ResponseCancelled => {
                return Err(provider_failure(
                    provider_id,
                    "Provider execution was cancelled.",
                ));
            }
            AiStreamEvent::ResponseStarted { .. }
            | AiStreamEvent::ResponseItemStarted { .. }
            | AiStreamEvent::ReasoningStarted { .. }
            | AiStreamEvent::ResourceProgress { .. }
            | AiStreamEvent::ResponseItemCompleted { .. } => {}
        }
        Ok(())
    }

    /// Finishes the controller response after a public terminal event.
    /// 在公共终态事件后完成控制器响应。
    fn finish(
        self,
        provider_id: &str,
        model_id: &str,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError> {
        if !self.completed {
            return Err(provider_failure(
                provider_id,
                "Provider stream ended without a successful terminal response event.",
            ));
        }
        let provider_message_state = self.reasoning_signature.map(|signature| {
            json!({
                "vulcan_ai": {
                    "reasoning_signature": signature,
                }
            })
        });
        Ok(CoreProviderTurnResponse {
            assistant_text: self.text,
            assistant_reasoning: self.reasoning,
            provider_message_id: None,
            provider_message_state,
            usage: self.usage.as_ref().map(public_usage_to_core),
            tool_calls: self.tool_calls,
            continuation: Some(ProviderContinuation {
                provider_id: Some(provider_id.to_string()),
                model_id: Some(model_id.to_string()),
                ..ProviderContinuation::default()
            }),
        })
    }
}

/// Merges one partial provider usage update without erasing fields reported by earlier events.
/// 合并一条供应商局部用量更新，同时避免抹除先前事件已经报告的字段。
///
/// # Parameters
/// 参数
/// `current` stores the accumulated request usage and `update` is the next protocol event.
/// `current` 保存累计请求用量，`update` 是下一条协议事件。
fn merge_usage_update(current: &mut Option<RequestUsage>, update: RequestUsage) {
    let Some(current) = current.as_mut() else {
        *current = Some(update);
        return;
    };
    merge_usage_value(&mut current.input_tokens, update.input_tokens);
    merge_usage_value(&mut current.output_tokens, update.output_tokens);
    merge_usage_value(&mut current.reasoning_tokens, update.reasoning_tokens);
    merge_usage_value(&mut current.cache_read_tokens, update.cache_read_tokens);
    merge_usage_value(&mut current.cache_write_tokens, update.cache_write_tokens);
    merge_usage_value(
        &mut current.cache_write_5m_tokens,
        update.cache_write_5m_tokens,
    );
    merge_usage_value(
        &mut current.cache_write_1h_tokens,
        update.cache_write_1h_tokens,
    );
    merge_usage_value(&mut current.audio_input_tokens, update.audio_input_tokens);
    merge_usage_value(&mut current.audio_output_tokens, update.audio_output_tokens);
    merge_usage_value(&mut current.total_tokens, update.total_tokens);
    if update.cost.is_some() {
        current.cost = update.cost;
    }
}

/// Replaces one accumulated usage value only when the new event reports an exact value.
/// 仅当新事件报告精确值时替换一项累计用量。
///
/// # Parameters
/// 参数
/// `current` is the accumulated field and `update` is the next partial field.
/// `current` 是累计字段，`update` 是下一条局部字段。
fn merge_usage_value(current: &mut UsageValue, update: UsageValue) {
    if !matches!(update, UsageValue::Unknown) {
        *current = update;
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

/// Resolves every local message resource into an immutable request-scoped data URL.
/// 将每个本地消息资源解析为不可变的请求作用域 Data URL。
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
        let media_type = resource.media_type.as_deref().ok_or_else(|| {
            provider_failure(
                route.provider.id.as_str(),
                format!("Resource `{}` is missing its MIME type.", resource.id).as_str(),
            )
        })?;
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
            let mut content = Vec::new();
            if !reasoning.is_empty() {
                content.push(ContentItem::Reasoning {
                    text: reasoning.clone(),
                    signature: provider_message_state
                        .as_ref()
                        .and_then(|state| state.pointer("/vulcan_ai/reasoning_signature"))
                        .and_then(Value::as_str)
                        .map(str::to_string),
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
        let mut usage = Some(RequestUsage {
            input_tokens: reported(1_000),
            cache_read_tokens: reported(800),
            cache_write_tokens: reported(120),
            ..RequestUsage::default()
        });

        merge_usage_update(
            &mut usage,
            RequestUsage {
                output_tokens: reported(75),
                total_tokens: reported(1_075),
                ..RequestUsage::default()
            },
        );

        let usage = usage.expect("merged usage should exist");
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
        let mut accumulator = PublicTurnAccumulator::default();

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
}
