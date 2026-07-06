//! OpenRouter host-controlled provider transport adapter spec.
//! OpenRouter 宿主控制 Provider 传输适配器 spec。

use crate::agent_runtime::provider_transport_common_adapter::{
    CommonProviderStreamingAdapter, CommonProviderStreamingAdapterConfig,
    ProviderStreamingTransportSpec,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_openai_compatible_http::execute_openai_compatible_http_streaming_turn;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::provider_config::ModelInterleavedCapability;
use async_trait::async_trait;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Fixed OpenRouter OpenAI-compatible API base URL used by the transport executor.
/// 传输执行器使用的固定 OpenRouter OpenAI 兼容 API 基础地址。
const OPENROUTER_CHAT_COMPLETIONS_BASE_URL: &str = "https://openrouter.ai/api/v1";

/// Shared OpenRouter streaming adapter type bound to the OpenRouter transport spec.
/// 绑定 OpenRouter 传输 spec 的共享 OpenRouter 流式适配器类型。
pub(crate) type OpenRouterCoreStreamingAdapter =
    CommonProviderStreamingAdapter<OpenRouterStreamingTransport>;

/// Zero-sized OpenRouter transport spec that delegates to the OpenAI-compatible HTTP streaming executor.
/// 委托给 OpenAI 兼容 HTTP 流式执行器的零大小 OpenRouter 传输 spec。
#[derive(Clone)]
pub(crate) struct OpenRouterStreamingTransport;

/// Creates an OpenRouter streaming adapter from prepared host inputs, API credentials, interleaved reasoning capability, and the live delta sender; returns a shared adapter ready for the host-controlled stream runtime.
/// 根据已准备的宿主输入、API 凭据、交错推理能力和实时增量发送器创建 OpenRouter 流式适配器；返回可交给宿主控制流运行时的共享适配器。
pub(crate) fn new_openrouter_core_streaming_adapter(
    adapter_inputs: PreparedProviderTransportAdapterInputs,
    api_key: String,
    interleaved_reasoning: Option<ModelInterleavedCapability>,
    live_sender: ProviderLiveDeltaSender,
) -> OpenRouterCoreStreamingAdapter {
    OpenRouterCoreStreamingAdapter::new(
        adapter_inputs,
        CommonProviderStreamingAdapterConfig {
            request_label: "OpenRouter".to_string(),
            transport_label: "OpenRouter".to_string(),
            api_key,
            base_url: Some(OPENROUTER_CHAT_COMPLETIONS_BASE_URL.to_string()),
            interleaved_reasoning,
            live_sender,
        },
    )
}

#[async_trait]
impl ProviderStreamingTransportSpec for OpenRouterStreamingTransport {
    /// Executes one OpenRouter HTTP streaming turn using the shared adapter state, built request, and normalized delta sink; returns the final normalized provider response.
    /// 使用共享适配器状态、已构建请求和规范化增量 sink 执行一次 OpenRouter HTTP 流式回合；返回最终规范化 Provider 响应。
    async fn execute_streaming_turn<S>(
        adapter: &OpenRouterCoreStreamingAdapter,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        execute_openai_compatible_http_streaming_turn(
            adapter.provider_id.as_str(),
            adapter.transport_label.as_str(),
            adapter.api_key.as_str(),
            adapter.base_url.as_deref(),
            adapter.model_id.as_str(),
            adapter.request_headers.clone(),
            request,
            &adapter.provider_tool_definitions,
            &adapter.live_sender,
            sink,
        )
        .await
    }
}
