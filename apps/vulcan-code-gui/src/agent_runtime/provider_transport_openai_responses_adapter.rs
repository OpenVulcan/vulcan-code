//! OpenAI Responses host-controlled provider transport adapter.
//! OpenAI Responses 宿主控制 Provider 传输适配器。

use crate::agent_runtime::provider_protocol_rig_content::core_provider_turn_deltas_from_rig_assistant_content;
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_adapter_support::{
    build_openai_responses_streaming_request, map_provider_client_build_result,
    unsupported_streaming_execute,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_rig_completion::execute_rig_openai_like_streaming_turn;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use rig::client::CompletionClient;
use rig::providers::openai;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Streaming OpenAI Responses provider adapter that lets `vulcan-core` own request construction while the GUI only consumes normalized deltas.
/// 让 `vulcan-core` 持有请求构造权、而图形界面只消费规范化增量的流式 OpenAI Responses Provider 适配器。
#[derive(Clone)]
pub(crate) struct OpenAiResponsesCoreStreamingAdapter {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    pub(crate) provider_id: String,
    /// Resolved API key used by the low-level OpenAI Responses transport.
    /// 被低层 OpenAI Responses 传输层使用的已解析 API Key。
    pub(crate) api_key: String,
    /// Optional base URL override used by the low-level OpenAI Responses transport.
    /// 被低层 OpenAI Responses 传输层使用的可选基础地址覆盖值。
    pub(crate) base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 被低层传输层使用的稳定模型标识。
    pub(crate) model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    pub(crate) request_headers: HeaderMap,
    /// Low-level rig tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的低层 rig 工具定义集合。
    pub(crate) provider_tool_definitions: Vec<ProviderToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the GUI runtime.
    /// 用于把快照与规范化流式增量转发回图形界面运行时的实时发送器。
    pub(crate) live_sender: ProviderLiveDeltaSender,
}

impl OpenAiResponsesCoreStreamingAdapter {
    /// Creates one OpenAI Responses streaming adapter from the shared prepared adapter inputs plus provider-specific launch fields.
    /// 使用共享的已准备适配器输入与 Provider 专用启动字段创建一个 OpenAI Responses 流式适配器。
    pub(crate) fn new(
        adapter_inputs: PreparedProviderTransportAdapterInputs,
        api_key: String,
        base_url: Option<String>,
        live_sender: ProviderLiveDeltaSender,
    ) -> Self {
        let PreparedProviderTransportAdapterInputs {
            provider_id,
            model_id,
            request_headers,
            provider_tool_definitions,
        } = adapter_inputs;

        Self {
            provider_id,
            api_key,
            base_url,
            model_id,
            request_headers,
            provider_tool_definitions,
            live_sender,
        }
    }
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
        build_openai_responses_streaming_request(
            self.provider_id.as_str(),
            self.model_id.as_str(),
            snapshot,
            config,
            &self.provider_tool_definitions,
            &self.request_headers,
            &self.live_sender,
        )
    }

    /// One-shot request execution is intentionally unsupported because the OpenAI Responses host path is owned by the streaming adapter.
    /// 单次非流式请求执行被刻意禁用，因为 OpenAI Responses 宿主链路完全由流式适配器持有。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        unsupported_streaming_execute("OpenAI Responses")
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

        let client = map_provider_client_build_result(
            self.provider_id.as_str(),
            "OpenAI Responses",
            builder.build(),
        )?;
        let model = client.completion_model(self.model_id.as_str());
        execute_rig_openai_like_streaming_turn(
            self.provider_id.as_str(),
            "OpenAI Responses",
            self.model_id.as_str(),
            &model,
            request,
            &self.provider_tool_definitions,
            &self.live_sender,
            sink,
            core_provider_turn_deltas_from_rig_assistant_content,
        )
        .await
    }
}
