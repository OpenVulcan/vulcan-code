//! Host-controlled provider transport adapters extracted from the GUI executor.
//! 从图形界面执行器中拆分出的宿主控制 Provider 传输适配层。

use crate::agent_runtime::events::{RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::executor::{
    ProviderDeltaStream, ProviderStreamDelta, attach_tool_name_to_runtime_metadata,
    build_host_tool_result_id, build_openai_compatible_turn_request_snapshot,
    build_openai_compatible_turn_response, build_tool_result_runtime_metadata,
    core_message_to_rig_message, core_messages_to_rig_history,
    core_provider_turn_delta_to_runtime_delta, core_provider_turn_deltas_from_assistant_content,
    core_snapshot_to_session_conversation_state, format_host_tool_result_runtime_detail,
    format_tool_call_runtime_detail, inject_previous_response_id_into_additional_params,
    resolved_tool_definition_to_rig_tool_definition,
};
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use async_stream::stream;
use async_trait::async_trait;
use futures_util::StreamExt;
use reqwest::header::HeaderMap;
use rig::completion::{CompletionModel, ToolDefinition as RigToolDefinition};
use rig::message::Message as RigMessage;
use rig::prelude::CompletionClient;
use rig::providers::{anthropic, openai, openrouter};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::tool::{AllowAllToolPolicy, VulcanToolExecutor};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
    ToolInvocationRequest as CoreToolInvocationRequest,
    ToolInvocationResult as CoreToolInvocationResult,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Shared live-delta sender used to forward request snapshots, streaming chunks, and runtime events from the host-owned controller task.
/// 用于从宿主自有控制器任务转发请求快照、流式分片与运行时事件的共享实时增量发送器。
type ProviderLiveDeltaSender = UnboundedSender<Result<ProviderStreamDelta, String>>;

/// Serializable OpenAI-compatible single-turn request owned by `vulcan-core` while preserving transport-only rig messages outside persistence.
/// 由 `vulcan-core` 持有的可序列化 OpenAI 兼容单回合请求，同时在持久化之外保留仅供传输使用的 rig 消息。
#[derive(Debug, Clone, serde::Serialize)]
struct OpenAiCompatibleCoreTurnRequest {
    /// Structured request snapshot persisted for inspection and debugging.
    /// 被持久化用于检查与调试的结构化请求快照。
    #[serde(flatten)]
    snapshot: CoreProviderRequestSnapshot,
    /// Provider-visible history converted into rig messages for the real transport layer.
    /// 为真实传输层转换出的 Provider 可见 rig 历史消息。
    #[serde(skip_serializing)]
    current_history: Vec<RigMessage>,
    /// Last outgoing prompt message converted into the rig shape expected by the transport layer.
    /// 被转换为传输层所需 rig 形状的最后一条出站提示消息。
    #[serde(skip_serializing)]
    current_prompt: RigMessage,
}

/// Streaming OpenAI-compatible provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenAI 兼容 Provider 适配器。
#[derive(Clone)]
struct OpenAiCompatibleCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level OpenAI-compatible transport.
    /// 被低层 OpenAI 兼容传输层使用的已解析 API Key。
    api_key: String,
    /// Optional base URL override used by the low-level OpenAI-compatible transport.
    /// 被低层 OpenAI 兼容传输层使用的可选基础地址覆盖值。
    base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for OpenAiCompatibleCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the OpenAI-compatible streaming adapter.
    /// 返回 OpenAI 兼容流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the OpenAI-compatible streaming adapter.
    /// 返回 OpenAI 兼容流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "OpenAI 兼容单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            config.additional_params.clone(),
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the OpenAI-compatible host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenAI 兼容宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "OpenAI 兼容流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for OpenAiCompatibleCoreStreamingAdapter {
    /// Executes one exact OpenAI-compatible transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 OpenAI 兼容传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let mut builder = openai::CompletionsClient::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone());

        if let Some(base_url) = self
            .base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.base_url(base_url);
        }

        let client = builder
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 OpenAI 兼容客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("OpenAI 兼容流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("OpenAI 兼容流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming OpenRouter provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenRouter Provider 适配器。
#[derive(Clone)]
struct OpenRouterCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level OpenRouter transport.
    /// 被低层 OpenRouter 传输层使用的已解析 API Key。
    api_key: String,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for OpenRouterCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the OpenRouter streaming adapter.
    /// 返回 OpenRouter 流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the OpenRouter streaming adapter.
    /// 返回 OpenRouter 流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "OpenRouter 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            config.additional_params.clone(),
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the OpenRouter host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenRouter 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "OpenRouter 流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for OpenRouterCoreStreamingAdapter {
    /// Executes one exact OpenRouter transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 OpenRouter 传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let client = openrouter::Client::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone())
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 OpenRouter 客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("OpenRouter 流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("OpenRouter 流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming Anthropic provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 Anthropic Provider 适配器。
#[derive(Clone)]
struct AnthropicCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level Anthropic transport.
    /// 被低层 Anthropic 传输层使用的已解析 API Key。
    api_key: String,
    /// Optional base URL override used by the low-level Anthropic transport.
    /// 被低层 Anthropic 传输层使用的可选基础地址覆盖值。
    base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for AnthropicCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the Anthropic streaming adapter.
    /// 返回 Anthropic 流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the Anthropic streaming adapter.
    /// 返回 Anthropic 流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "Anthropic 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            config.additional_params.clone(),
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the Anthropic host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 Anthropic 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "Anthropic 流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for AnthropicCoreStreamingAdapter {
    /// Executes one exact Anthropic transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 Anthropic 传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let mut builder = anthropic::Client::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone());

        if let Some(base_url) = self
            .base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.base_url(base_url);
        }

        let client = builder
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 Anthropic 兼容客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("Anthropic 兼容流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("Anthropic 兼容流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming OpenAI Responses provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenAI Responses Provider 适配器。
#[derive(Clone)]
struct OpenAiResponsesCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    provider_id: String,
    /// Resolved API key used by the low-level OpenAI Responses transport.
    /// 被低层 OpenAI Responses 传输层使用的已解析 API Key。
    api_key: String,
    /// Optional base URL override used by the low-level OpenAI Responses transport.
    /// 被低层 OpenAI Responses 传输层使用的可选基础地址覆盖值。
    base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanProviderAdapter for OpenAiResponsesCoreStreamingAdapter {
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the OpenAI Responses streaming adapter.
    /// 返回 OpenAI Responses 流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the OpenAI Responses streaming adapter.
    /// 返回 OpenAI Responses 流式适配器使用的稳定模型标识。
    fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Builds one exact single-turn request from the canonical controller snapshot, injects `previous_response_id` when possible, and forwards the structured snapshot immediately.
    /// 根据规范控制器快照构建一份精确单回合请求，在可行时注入 `previous_response_id`，并立即转发结构化快照。
    async fn build_request(
        &self,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<Self::Request, VulcanCoreError> {
        let Some((current_prompt_message, current_history_messages)) =
            snapshot.messages.split_last()
        else {
            return Err(VulcanCoreError::InvalidState {
                message: String::from(
                    "OpenAI Responses 单回合请求构建失败：规范消息窗口为空，无法解析最后一条出站提示消息。",
                ),
            });
        };
        let current_history = core_messages_to_rig_history(current_history_messages);
        let current_prompt = core_message_to_rig_message(current_prompt_message);
        let effective_additional_params = inject_previous_response_id_into_additional_params(
            config.additional_params.clone(),
            &snapshot.continuation,
        );
        let request_snapshot = build_openai_compatible_turn_request_snapshot(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            config.system_prompt.as_str(),
            &current_history,
            &current_prompt,
            config.temperature,
            config.max_turns,
            &self.resolved_tools,
            &self.request_headers,
            effective_additional_params,
        );
        let _ = self
            .live_sender
            .send(Ok(ProviderStreamDelta::RequestSnapshot(
                request_snapshot.clone(),
            )));

        Ok(OpenAiCompatibleCoreTurnRequest {
            snapshot: request_snapshot,
            current_history,
            current_prompt,
        })
    }

    /// One-shot request execution is intentionally unsupported because the OpenAI Responses host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenAI Responses 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        Err(VulcanCoreError::InvalidState {
            message: String::from(
                "OpenAI Responses 流式适配器不支持非流式 execute_request 调用，请改用 execute_request_streaming。",
            ),
        })
    }
}

#[async_trait]
impl VulcanStreamingProviderAdapter for OpenAiResponsesCoreStreamingAdapter {
    /// Executes one exact OpenAI Responses transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 OpenAI Responses 传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        let mut builder = openai::Client::builder()
            .api_key(self.api_key.as_str())
            .http_headers(self.request_headers.clone());

        if let Some(base_url) = self
            .base_url
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder = builder.base_url(base_url);
        }

        let client = builder
            .build()
            .map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("创建 OpenAI Responses 客户端失败：{error}"),
            })?;
        let model = client.completion_model(self.model_id.as_str());
        let mut request_builder = model
            .completion_request(request.current_prompt)
            .preamble(request.snapshot.system_prompt.clone())
            .messages(request.current_history)
            .temperature_opt(request.snapshot.temperature)
            .additional_params_opt(request.snapshot.additional_params.clone());

        if !self.rig_tool_definitions.is_empty() {
            request_builder = request_builder.tools(self.rig_tool_definitions.clone());
        }

        let mut stream =
            request_builder
                .stream()
                .await
                .map_err(|error| VulcanCoreError::ProviderFailure {
                    provider_id: self.provider_id.clone(),
                    message: format!("OpenAI Responses 流式请求启动失败：{error}"),
                })?;

        while let Some(item) = stream.next().await {
            let content = item.map_err(|error| VulcanCoreError::ProviderFailure {
                provider_id: self.provider_id.clone(),
                message: format!("OpenAI Responses 流式执行失败：{error}"),
            })?;
            for delta in core_provider_turn_deltas_from_assistant_content(content) {
                sink.push_delta(delta.clone());
                let _ = self
                    .live_sender
                    .send(Ok(core_provider_turn_delta_to_runtime_delta(delta)));
            }
        }

        Ok(build_openai_compatible_turn_response(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            stream.message_id.clone(),
            &stream.choice,
        ))
    }
}

/// Streaming tool executor that forwards host-side tool boundary events live while delegating the real tool invocation to the shared bridge.
/// 在把真实工具调用委派给共享桥接层的同时，实时转发宿主侧工具边界事件的流式工具执行器。
#[derive(Clone)]
struct StreamingRuntimeToolExecutor {
    /// Shared host-managed bridge that owns the real tool dispatch and metadata normalization.
    /// 持有真实工具调度与元数据规范化能力的共享宿主管理桥接层。
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    /// Live sender used to forward tool boundary runtime events into the GUI stream.
    /// 用于把工具边界运行时事件转发进图形界面流中的实时发送器。
    live_sender: ProviderLiveDeltaSender,
}

#[async_trait]
impl VulcanToolExecutor for StreamingRuntimeToolExecutor {
    /// Executes one host-managed tool request, forwards start and finish runtime events, and returns the normalized tool result to the controller.
    /// 执行一次宿主管理工具请求，转发开始与完成运行时事件，并将规范化工具结果返回给控制器。
    async fn execute_tool(
        &self,
        request: &CoreToolInvocationRequest,
    ) -> Result<CoreToolInvocationResult, VulcanCoreError> {
        let _ = self.live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
            RuntimeEvent::new(
                RuntimeEventKind::ToolStarted,
                format!(
                    "工具调用 · {} · {}",
                    request.tool_call_id, request.tool_name
                ),
                format_tool_call_runtime_detail(&request.tool_name, &request.arguments),
            )
            .with_metadata(Some(json!({
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "arguments": request.arguments,
            }))),
        )));
        let result = self.runtime_bridge.execute_tool(request).await?;
        let tool_result_id = build_host_tool_result_id(&request.tool_call_id);
        let event_metadata = attach_tool_name_to_runtime_metadata(
            build_tool_result_runtime_metadata(
                &request.tool_call_id,
                &tool_result_id,
                result.host_result.as_ref(),
            ),
            &request.tool_name,
        );
        let _ = self.live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
            RuntimeEvent::new(
                RuntimeEventKind::ToolFinished,
                format!("工具返回 · {}", request.tool_call_id),
                format_host_tool_result_runtime_detail(&tool_result_id, &result.model_output),
            )
            .with_metadata(event_metadata),
        )));
        Ok(result)
    }
}

/// Executes one host-controlled provider stream through `vulcan-core`, forwards live deltas, and returns the final canonical state to the GUI layer.
/// 通过 `vulcan-core` 执行一次宿主控制的 Provider 流，转发实时增量，并把最终规范状态返回给图形界面层。
fn execute_host_controlled_stream<A, F>(
    controller_config: CoreControllerConfig,
    core_snapshot: CoreConversationSnapshot,
    prompt: String,
    provider_factory: F,
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    provider_label: &'static str,
) -> ProviderDeltaStream
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Send + Sync + 'static,
    F: FnOnce(ProviderLiveDeltaSender) -> A + Send + 'static,
{
    Box::pin(stream! {
        let (live_sender, mut live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let task_sender = live_sender.clone();
        let mut controller = VulcanCoreController::new(controller_config, core_snapshot);
        let provider_adapter = provider_factory(live_sender.clone());
        let tool_executor = StreamingRuntimeToolExecutor {
            runtime_bridge,
            live_sender: live_sender.clone(),
        };
        let controller_prompt = prompt.clone();
        let controller_task = tokio::spawn(async move {
            let tool_policy = AllowAllToolPolicy;
            match controller
                .run_user_turn_streaming(
                    &provider_adapter,
                    &tool_executor,
                    &tool_policy,
                    controller_prompt,
                )
                .await
            {
                Ok(_) => {
                    let _ = task_sender.send(Ok(ProviderStreamDelta::FinalConversationState(
                        core_snapshot_to_session_conversation_state(controller.snapshot()),
                    )));
                }
                Err(error) => {
                    let _ = task_sender.send(Err(format!(
                        "{}流式执行失败：{}。",
                        provider_label,
                        error
                    )));
                }
            }
        });
        drop(live_sender);

        while let Some(item) = live_receiver.recv().await {
            yield item;
        }

        if let Err(error) = controller_task.await {
            yield Err(format!(
                "{}控制器后台任务失败：{}。",
                provider_label,
                error
            ));
        }
    })
}

/// Executes one host-controlled OpenAI-compatible prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenAI 兼容提示词调用，并流式返回规范化增量。
pub(crate) fn execute_openai_compatible_stream(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();
    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| OpenAiCompatibleCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "OpenAI 兼容",
    )
}

/// Executes one host-controlled OpenAI Responses prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenAI Responses 提示词调用，并流式返回规范化增量。
pub(crate) fn execute_openai_responses_stream(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();

    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| OpenAiResponsesCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "OpenAI Responses",
    )
}

/// Executes one host-controlled Anthropic prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 Anthropic 提示词调用，并流式返回规范化增量。
pub(crate) fn execute_anthropic_stream(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string);
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();

    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| AnthropicCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "Anthropic 兼容",
    )
}

/// Executes one host-controlled OpenRouter prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenRouter 提示词调用，并流式返回规范化增量。
pub(crate) fn execute_openrouter_stream(
    provider_id: &str,
    api_key: &str,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
        Arc::new(tools_manager),
        &resolved_tools,
        session_id,
    ));
    let rig_tool_definitions = resolved_tools
        .iter()
        .map(resolved_tool_definition_to_rig_tool_definition)
        .collect::<Vec<_>>();

    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| OpenRouterCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions,
            live_sender,
        },
        runtime_bridge,
        "OpenRouter",
    )
}
