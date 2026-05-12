//! OpenRouter host-controlled provider transport adapter.
//! OpenRouter 宿主控制 Provider 传输适配器。

use crate::agent_runtime::provider_protocol_bridge::core_provider_turn_deltas_from_assistant_content;
use crate::agent_runtime::provider_transport_adapter_support::{
    build_standard_streaming_request, execute_openai_like_streaming_turn,
    map_provider_client_build_result, unsupported_streaming_execute,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use rig::completion::ToolDefinition as RigToolDefinition;
use rig::prelude::CompletionClient;
use rig::providers::openrouter;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};
use vulcan_tools_manager::ResolvedToolDefinition;

/// Streaming OpenRouter provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenRouter Provider 适配器。
#[derive(Clone)]
pub(crate) struct OpenRouterCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    pub(crate) provider_id: String,
    /// Resolved API key used by the low-level OpenRouter transport.
    /// 被低层 OpenRouter 传输层使用的已解析 API Key。
    pub(crate) api_key: String,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    pub(crate) model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    pub(crate) request_headers: HeaderMap,
    /// Resolved host tool definitions exposed to the provider for the current turn.
    /// 当前回合对 Provider 暴露的已解析宿主工具定义集合。
    pub(crate) resolved_tools: Vec<ResolvedToolDefinition>,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    pub(crate) rig_tool_definitions: Vec<RigToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    pub(crate) live_sender: ProviderLiveDeltaSender,
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
        build_standard_streaming_request(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            "OpenRouter",
            snapshot,
            config,
            &self.resolved_tools,
            &self.request_headers,
            &self.live_sender,
        )
    }

    /// One-shot request execution is intentionally unsupported because the OpenRouter host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenRouter 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        unsupported_streaming_execute("OpenRouter")
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
        let client = map_provider_client_build_result(
            self.provider_id.as_str(),
            "OpenRouter",
            openrouter::Client::builder()
                .api_key(self.api_key.as_str())
                .http_headers(self.request_headers.clone())
                .build(),
        )?;
        let model = client.completion_model(self.model_id.as_str());
        execute_openai_like_streaming_turn(
            self.provider_id.as_str(),
            "OpenRouter",
            self.model_id.as_str(),
            &model,
            request,
            &self.rig_tool_definitions,
            &self.live_sender,
            sink,
            core_provider_turn_deltas_from_assistant_content,
        )
        .await
    }
}
