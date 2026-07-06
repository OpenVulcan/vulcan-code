//! Google Gemini native host-controlled provider transport adapter spec.
//! Google Gemini 原生宿主控制 Provider 传输适配器 spec。

use crate::agent_runtime::provider_transport_common_adapter::{
    CommonProviderStreamingAdapter, CommonProviderStreamingAdapterConfig,
    ProviderStreamingTransportSpec,
};
use crate::agent_runtime::provider_transport_google_http::execute_google_gemini_http_streaming_turn;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use async_trait::async_trait;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Shared Google Gemini streaming adapter type bound to the Google Gemini transport spec.
/// 绑定 Google Gemini 传输 spec 的共享 Google Gemini 流式适配器类型。
pub(crate) type GoogleGeminiCoreStreamingAdapter =
    CommonProviderStreamingAdapter<GoogleGeminiStreamingTransport>;

/// Zero-sized Google Gemini transport spec that delegates to the Google Gemini HTTP streaming executor.
/// 委托给 Google Gemini HTTP 流式执行器的零大小 Google Gemini 传输 spec。
#[derive(Clone)]
pub(crate) struct GoogleGeminiStreamingTransport;

/// Creates a Google Gemini streaming adapter from prepared host inputs, API credentials, optional endpoint override, and the live delta sender; returns a shared adapter ready for the host-controlled stream runtime.
/// 根据已准备的宿主输入、API 凭据、可选端点覆盖值和实时增量发送器创建 Google Gemini 流式适配器；返回可交给宿主控制流运行时的共享适配器。
pub(crate) fn new_google_gemini_core_streaming_adapter(
    adapter_inputs: PreparedProviderTransportAdapterInputs,
    api_key: String,
    base_url: Option<String>,
    live_sender: ProviderLiveDeltaSender,
) -> GoogleGeminiCoreStreamingAdapter {
    GoogleGeminiCoreStreamingAdapter::new(
        adapter_inputs,
        CommonProviderStreamingAdapterConfig {
            request_label: "Google Gemini".to_string(),
            transport_label: "Google Gemini".to_string(),
            api_key,
            base_url,
            interleaved_reasoning: None,
            live_sender,
        },
    )
}

#[async_trait]
impl ProviderStreamingTransportSpec for GoogleGeminiStreamingTransport {
    /// Executes one Google Gemini HTTP streaming turn using the shared adapter state, built request, and normalized delta sink; returns the final normalized provider response.
    /// 使用共享适配器状态、已构建请求和规范化增量 sink 执行一次 Google Gemini HTTP 流式回合；返回最终规范化 Provider 响应。
    async fn execute_streaming_turn<S>(
        adapter: &GoogleGeminiCoreStreamingAdapter,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        execute_google_gemini_http_streaming_turn(
            adapter.provider_id.as_str(),
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
