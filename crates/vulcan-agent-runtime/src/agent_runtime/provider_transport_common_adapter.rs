//! Shared host-controlled streaming provider adapter.
//! 共享的宿主控制流式 Provider 适配器。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_adapter_support::{
    build_standard_streaming_request, unsupported_streaming_execute,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::provider_config::ModelInterleavedCapability;
use async_trait::async_trait;
use reqwest::header::HeaderMap;
use std::marker::PhantomData;
use vulcan_core::provider::{
    VulcanProviderAdapter, VulcanProviderDeltaSink, VulcanStreamingProviderAdapter,
};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Provider-specific launch configuration consumed by the shared streaming adapter constructor.
/// 由共享流式适配器构造函数消费的 Provider 专用启动配置。
#[derive(Clone)]
pub(crate) struct CommonProviderStreamingAdapterConfig {
    /// Human-readable label used while building requests and reporting unsupported non-streaming execution.
    /// 构建请求和报告不支持非流式执行时使用的人类可读标签。
    pub(crate) request_label: String,
    /// Human-readable label passed to the low-level transport diagnostics.
    /// 传递给底层传输诊断逻辑的人类可读标签。
    pub(crate) transport_label: String,
    /// Resolved API key used by the low-level provider transport.
    /// 底层 Provider 传输层使用的已解析 API Key。
    pub(crate) api_key: String,
    /// Optional base URL override used by the low-level provider transport.
    /// 底层 Provider 传输层使用的可选基础地址覆盖值。
    pub(crate) base_url: Option<String>,
    /// Optional model-scoped interleaved reasoning capability reused during request replay.
    /// 请求回放阶段复用的可选模型级交错推理能力描述。
    pub(crate) interleaved_reasoning: Option<ModelInterleavedCapability>,
    /// Live sender used to forward snapshots and normalized stream deltas to the host runtime.
    /// 用于把快照与规范化流式增量转发回宿主运行时的实时发送器。
    pub(crate) live_sender: ProviderLiveDeltaSender,
}

/// Shared streaming adapter that owns the duplicated `vulcan-core` adapter flow while delegating provider-specific HTTP execution to a transport spec.
/// 共享流式适配器持有重复的 `vulcan-core` 适配流程，并把 Provider 专用 HTTP 执行委托给传输 spec。
#[derive(Clone)]
pub(crate) struct CommonProviderStreamingAdapter<T> {
    /// Stable provider identifier attached to every request and continuation snapshot.
    /// 附着到每次请求与 continuation 快照上的稳定 Provider 标识。
    pub(crate) provider_id: String,
    /// Human-readable label used while building requests and reporting unsupported non-streaming execution.
    /// 构建请求和报告不支持非流式执行时使用的人类可读标签。
    pub(crate) request_label: String,
    /// Human-readable label passed to the low-level transport diagnostics.
    /// 传递给底层传输诊断逻辑的人类可读标签。
    pub(crate) transport_label: String,
    /// Resolved API key used by the low-level provider transport.
    /// 底层 Provider 传输层使用的已解析 API Key。
    pub(crate) api_key: String,
    /// Optional base URL override used by the low-level provider transport.
    /// 底层 Provider 传输层使用的可选基础地址覆盖值。
    pub(crate) base_url: Option<String>,
    /// Stable model identifier used by the low-level transport.
    /// 底层传输层使用的稳定模型标识。
    pub(crate) model_id: String,
    /// Fully resolved request headers attached to the transport request.
    /// 附着到传输请求上的完整已解析请求头。
    pub(crate) request_headers: HeaderMap,
    /// Provider-visible tool definitions mirrored from the resolved host tool definitions.
    /// 从已解析宿主工具定义镜像而来的 Provider 可见工具定义集合。
    pub(crate) provider_tool_definitions: Vec<ProviderToolDefinition>,
    /// Live sender used to forward snapshots and normalized stream deltas to the host runtime.
    /// 用于把快照与规范化流式增量转发回宿主运行时的实时发送器。
    pub(crate) live_sender: ProviderLiveDeltaSender,
    /// Optional model-scoped interleaved reasoning capability reused during request replay.
    /// 请求回放阶段复用的可选模型级交错推理能力描述。
    pub(crate) interleaved_reasoning: Option<ModelInterleavedCapability>,
    /// Zero-sized marker connecting this shared adapter instance to one provider transport spec.
    /// 把这个共享适配器实例连接到某个 Provider 传输 spec 的零大小标记。
    transport_spec: PhantomData<T>,
}

impl<T> CommonProviderStreamingAdapter<T> {
    /// Creates one shared streaming adapter from prepared host inputs and provider-specific configuration; returns an adapter ready for the host-controlled stream runtime.
    /// 根据已准备的宿主输入与 Provider 专用配置创建共享流式适配器；返回可交给宿主控制流运行时的适配器。
    pub(crate) fn new(
        adapter_inputs: PreparedProviderTransportAdapterInputs,
        config: CommonProviderStreamingAdapterConfig,
    ) -> Self {
        // Split shared launch inputs once so every provider uses the same ownership and request-state mapping.
        // 只拆分一次共享启动输入，确保所有 Provider 使用同一套所有权与请求状态映射。
        let PreparedProviderTransportAdapterInputs {
            provider_id,
            model_id,
            request_headers,
            provider_tool_definitions,
        } = adapter_inputs;

        Self {
            provider_id,
            request_label: config.request_label,
            transport_label: config.transport_label,
            api_key: config.api_key,
            base_url: config.base_url,
            model_id,
            request_headers,
            provider_tool_definitions,
            live_sender: config.live_sender,
            interleaved_reasoning: config.interleaved_reasoning,
            transport_spec: PhantomData,
        }
    }
}

/// Provider-specific transport spec that supplies only the low-level HTTP streaming execution for the shared adapter.
/// Provider 专用传输 spec 只为共享适配器提供底层 HTTP 流式执行逻辑。
#[async_trait]
pub(crate) trait ProviderStreamingTransportSpec:
    Clone + Send + Sync + 'static + Sized
{
    /// Builds the provider-specific streaming request from the shared adapter state and canonical snapshot.
    /// 根据共享适配器状态与规范快照构建 Provider 专用流式请求。
    fn build_streaming_request(
        adapter: &CommonProviderStreamingAdapter<Self>,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
        build_standard_streaming_request(
            adapter.provider_id.as_str(),
            adapter.model_id.as_str(),
            adapter.request_label.as_str(),
            snapshot,
            config,
            &adapter.provider_tool_definitions,
            &adapter.request_headers,
            &adapter.live_sender,
            adapter.interleaved_reasoning.clone(),
        )
    }

    /// Executes one provider-specific HTTP streaming turn; `adapter` carries the shared state, `request` is the built turn request, and `sink` receives normalized deltas before the final response is returned.
    /// 执行一次 Provider 专用 HTTP 流式回合；`adapter` 携带共享状态，`request` 是已构建的回合请求，`sink` 在返回最终响应前接收规范化增量。
    async fn execute_streaming_turn<S>(
        adapter: &CommonProviderStreamingAdapter<Self>,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink;
}

#[async_trait]
impl<T> VulcanProviderAdapter for CommonProviderStreamingAdapter<T>
where
    T: ProviderStreamingTransportSpec,
{
    type Request = OpenAiCompatibleCoreTurnRequest;
    type Response = CoreProviderTurnResponse;

    /// Returns the stable provider identifier used by the shared streaming adapter.
    /// 返回共享流式适配器使用的稳定 Provider 标识。
    fn provider_id(&self) -> &str {
        &self.provider_id
    }

    /// Returns the stable model identifier used by the shared streaming adapter.
    /// 返回共享流式适配器使用的稳定模型标识。
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
        T::build_streaming_request(self, snapshot, config)
    }

    /// Rejects one-shot request execution because host-controlled providers are executed through the streaming adapter.
    /// 拒绝单次非流式请求执行，因为宿主控制 Provider 通过流式适配器执行。
    async fn execute_request(
        &self,
        _request: Self::Request,
    ) -> Result<Self::Response, VulcanCoreError> {
        unsupported_streaming_execute(self.request_label.as_str())
    }
}

#[async_trait]
impl<T> VulcanStreamingProviderAdapter for CommonProviderStreamingAdapter<T>
where
    T: ProviderStreamingTransportSpec,
{
    /// Executes one exact provider transport turn, forwards normalized stream deltas live, and returns the normalized final turn response.
    /// 执行一次精确的 Provider 传输回合，实时转发规范化流式增量，并返回规范化后的最终回合响应。
    async fn execute_request_streaming<S>(
        &self,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<Self::Response, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        T::execute_streaming_turn(self, request, sink).await
    }
}
