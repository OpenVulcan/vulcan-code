//! Azure OpenAI host-controlled provider transport adapter spec.
//! Azure OpenAI 宿主控制 Provider 传输适配器 spec。

use crate::agent_runtime::provider_transport_azure_http::execute_azure_openai_http_streaming_turn;
use crate::agent_runtime::provider_transport_common_adapter::{
    CommonProviderStreamingAdapter, CommonProviderStreamingAdapterConfig,
    ProviderStreamingTransportSpec,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::provider_config::ModelInterleavedCapability;
use async_trait::async_trait;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Shared Azure OpenAI streaming adapter type bound to the Azure OpenAI transport spec.
/// 绑定 Azure OpenAI 传输 spec 的共享 Azure OpenAI 流式适配器类型。
pub(crate) type AzureOpenAiCoreStreamingAdapter =
    CommonProviderStreamingAdapter<AzureOpenAiStreamingTransport>;

/// Zero-sized Azure OpenAI transport spec that delegates to the Azure OpenAI HTTP streaming executor.
/// 委托给 Azure OpenAI HTTP 流式执行器的零大小 Azure OpenAI 传输 spec。
#[derive(Clone)]
pub(crate) struct AzureOpenAiStreamingTransport;

/// Creates an Azure OpenAI streaming adapter from prepared host inputs, API credentials, optional endpoint override, interleaved reasoning capability, and the live delta sender; returns a shared adapter ready for the host-controlled stream runtime.
/// 根据已准备的宿主输入、API 凭据、可选端点覆盖值、交错推理能力和实时增量发送器创建 Azure OpenAI 流式适配器；返回可交给宿主控制流运行时的共享适配器。
pub(crate) fn new_azure_openai_core_streaming_adapter(
    adapter_inputs: PreparedProviderTransportAdapterInputs,
    api_key: String,
    base_url: Option<String>,
    interleaved_reasoning: Option<ModelInterleavedCapability>,
    live_sender: ProviderLiveDeltaSender,
) -> AzureOpenAiCoreStreamingAdapter {
    AzureOpenAiCoreStreamingAdapter::new(
        adapter_inputs,
        CommonProviderStreamingAdapterConfig {
            request_label: "Azure OpenAI".to_string(),
            transport_label: "Azure OpenAI".to_string(),
            api_key,
            base_url,
            interleaved_reasoning,
            live_sender,
        },
    )
}

#[async_trait]
impl ProviderStreamingTransportSpec for AzureOpenAiStreamingTransport {
    /// Executes one Azure OpenAI HTTP streaming turn using the shared adapter state, built request, and normalized delta sink; returns the final normalized provider response.
    /// 使用共享适配器状态、已构建请求和规范化增量 sink 执行一次 Azure OpenAI HTTP 流式回合；返回最终规范化 Provider 响应。
    async fn execute_streaming_turn<S>(
        adapter: &AzureOpenAiCoreStreamingAdapter,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        execute_azure_openai_http_streaming_turn(
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
