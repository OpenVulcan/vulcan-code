//! Cohere native host-controlled provider transport adapter spec.
//! Cohere 原生宿主控制 Provider 传输适配器 spec。

use crate::agent_runtime::provider_transport_cohere_http::execute_cohere_http_streaming_turn;
use crate::agent_runtime::provider_transport_common_adapter::{
    CommonProviderStreamingAdapter, CommonProviderStreamingAdapterConfig,
    ProviderStreamingTransportSpec,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use async_trait::async_trait;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Shared Cohere streaming adapter type bound to the Cohere transport spec.
/// 绑定 Cohere 传输 spec 的共享 Cohere 流式适配器类型。
pub(crate) type CohereCoreStreamingAdapter =
    CommonProviderStreamingAdapter<CohereStreamingTransport>;

/// Zero-sized Cohere transport spec that delegates to the Cohere HTTP streaming executor.
/// 委托给 Cohere HTTP 流式执行器的零大小 Cohere 传输 spec。
#[derive(Clone)]
pub(crate) struct CohereStreamingTransport;

/// Creates a Cohere streaming adapter from prepared host inputs, API credentials, optional endpoint override, and the live delta sender; returns a shared adapter ready for the host-controlled stream runtime.
/// 根据已准备的宿主输入、API 凭据、可选端点覆盖值和实时增量发送器创建 Cohere 流式适配器；返回可交给宿主控制流运行时的共享适配器。
pub(crate) fn new_cohere_core_streaming_adapter(
    adapter_inputs: PreparedProviderTransportAdapterInputs,
    api_key: String,
    base_url: Option<String>,
    live_sender: ProviderLiveDeltaSender,
) -> CohereCoreStreamingAdapter {
    CohereCoreStreamingAdapter::new(
        adapter_inputs,
        CommonProviderStreamingAdapterConfig {
            request_label: "Cohere".to_string(),
            transport_label: "Cohere".to_string(),
            api_key,
            base_url,
            interleaved_reasoning: None,
            live_sender,
        },
    )
}

#[async_trait]
impl ProviderStreamingTransportSpec for CohereStreamingTransport {
    /// Executes one Cohere HTTP streaming turn using the shared adapter state, built request, and normalized delta sink; returns the final normalized provider response.
    /// 使用共享适配器状态、已构建请求和规范化增量 sink 执行一次 Cohere HTTP 流式回合；返回最终规范化 Provider 响应。
    async fn execute_streaming_turn<S>(
        adapter: &CohereCoreStreamingAdapter,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        execute_cohere_http_streaming_turn(
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
