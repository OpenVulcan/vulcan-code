//! OpenAI Responses host-controlled provider transport adapter spec.
//! OpenAI Responses 宿主控制 Provider 传输适配器 spec。

use crate::agent_runtime::provider_transport_adapter_support::build_openai_responses_streaming_request;
use crate::agent_runtime::provider_transport_common_adapter::{
    CommonProviderStreamingAdapter, CommonProviderStreamingAdapterConfig,
    ProviderStreamingTransportSpec,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_openai_responses_http::execute_openai_responses_http_streaming_turn;
use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use async_trait::async_trait;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Shared OpenAI Responses streaming adapter type bound to the OpenAI Responses transport spec.
/// 绑定 OpenAI Responses 传输 spec 的共享 OpenAI Responses 流式适配器类型。
pub(crate) type OpenAiResponsesCoreStreamingAdapter =
    CommonProviderStreamingAdapter<OpenAiResponsesStreamingTransport>;

/// Zero-sized OpenAI Responses transport spec that delegates to the OpenAI Responses HTTP streaming executor.
/// 委托给 OpenAI Responses HTTP 流式执行器的零大小 OpenAI Responses 传输 spec。
#[derive(Clone)]
pub(crate) struct OpenAiResponsesStreamingTransport;

/// Creates an OpenAI Responses streaming adapter from prepared host inputs, API credentials, optional endpoint override, and the live delta sender; returns a shared adapter ready for the host-controlled stream runtime.
/// 根据已准备的宿主输入、API 凭据、可选端点覆盖值和实时增量发送器创建 OpenAI Responses 流式适配器；返回可交给宿主控制流运行时的共享适配器。
pub(crate) fn new_openai_responses_core_streaming_adapter(
    adapter_inputs: PreparedProviderTransportAdapterInputs,
    api_key: String,
    base_url: Option<String>,
    live_sender: ProviderLiveDeltaSender,
) -> OpenAiResponsesCoreStreamingAdapter {
    OpenAiResponsesCoreStreamingAdapter::new(
        adapter_inputs,
        CommonProviderStreamingAdapterConfig {
            request_label: "OpenAI Responses".to_string(),
            transport_label: "OpenAI Responses".to_string(),
            api_key,
            base_url,
            interleaved_reasoning: None,
            live_sender,
        },
    )
}

#[async_trait]
impl ProviderStreamingTransportSpec for OpenAiResponsesStreamingTransport {
    /// Builds an OpenAI Responses request while preserving provider-native continuation semantics.
    /// 构建 OpenAI Responses 请求，同时保留 Provider 原生 continuation 语义。
    fn build_streaming_request(
        adapter: &OpenAiResponsesCoreStreamingAdapter,
        snapshot: &CoreConversationSnapshot,
        config: &CoreControllerConfig,
    ) -> Result<OpenAiCompatibleCoreTurnRequest, VulcanCoreError> {
        build_openai_responses_streaming_request(
            adapter.provider_id.as_str(),
            adapter.model_id.as_str(),
            snapshot,
            config,
            &adapter.provider_tool_definitions,
            &adapter.request_headers,
            &adapter.live_sender,
        )
    }

    /// Executes one OpenAI Responses HTTP streaming turn using the shared adapter state, built request, and normalized delta sink; returns the final normalized provider response.
    /// 使用共享适配器状态、已构建请求和规范化增量 sink 执行一次 OpenAI Responses HTTP 流式回合；返回最终规范化 Provider 响应。
    async fn execute_streaming_turn<S>(
        adapter: &OpenAiResponsesCoreStreamingAdapter,
        request: OpenAiCompatibleCoreTurnRequest,
        sink: &mut S,
    ) -> Result<CoreProviderTurnResponse, VulcanCoreError>
    where
        S: VulcanProviderDeltaSink,
    {
        execute_openai_responses_http_streaming_turn(
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

#[cfg(test)]
mod tests {
    //! OpenAI Responses adapter request-construction tests.
    //! OpenAI Responses 适配器请求构建测试。

    use super::*;
    use crate::agent_runtime::provider_stream::ProviderStreamDelta;
    use crate::agent_runtime::provider_transport_openai_responses_http::SUPPRESS_OPENAI_RESPONSES_CONTINUATION_PARAM;
    use crate::agent_runtime::provider_transport_stream_runtime::PreparedProviderTransportAdapterInputs;
    use reqwest::header::HeaderMap;
    use serde_json::json;
    use tokio::sync::mpsc::unbounded_channel;
    use vulcan_core::provider::VulcanProviderAdapter;
    use vulcan_core::types::{
        ConversationMessage as CoreConversationMessage,
        ConversationPayload as CoreConversationPayload,
        ProviderContextInjection as CoreProviderContextInjection,
        ProviderContinuation as CoreProviderContinuation,
        ProviderPromptLayers as CoreProviderPromptLayers,
    };

    /// Builds one OpenAI Responses adapter with inert transport credentials for request-only tests.
    /// 为仅构建请求的测试创建一个携带惰性传输凭据的 OpenAI Responses 适配器。
    fn build_test_openai_responses_adapter() -> OpenAiResponsesCoreStreamingAdapter {
        // live_sender receives request snapshots emitted during build_request.
        // live_sender 接收 build_request 期间发出的请求快照。
        let (live_sender, _live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        new_openai_responses_core_streaming_adapter(
            PreparedProviderTransportAdapterInputs {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-5"),
                request_headers: HeaderMap::new(),
                provider_tool_definitions: Vec::new(),
            },
            String::from("test-key"),
            None,
            live_sender,
        )
    }

    /// Builds one minimal controller config for OpenAI Responses request-construction tests.
    /// 为 OpenAI Responses 请求构建测试创建一份最小控制器配置。
    fn build_test_controller_config(
        additional_params: Option<serde_json::Value>,
        context_injection: Option<CoreProviderContextInjection>,
    ) -> CoreControllerConfig {
        CoreControllerConfig {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-5"),
            system_prompt: String::from("system"),
            prompt_layers: CoreProviderPromptLayers::from_system_prompt(String::from("system")),
            temperature: None,
            max_turns: 8,
            additional_params,
            context_injection,
        }
    }

    /// Builds one conversation snapshot with an OpenAI Responses continuation cursor and one outgoing user prompt.
    /// 创建一份携带 OpenAI Responses continuation 游标与单条出站用户提示词的会话快照。
    fn build_test_snapshot() -> CoreConversationSnapshot {
        CoreConversationSnapshot {
            messages: vec![CoreConversationMessage {
                id: String::from("user-1"),
                payload: CoreConversationPayload::UserText {
                    text: String::from("hello"),
                    attachments: Vec::new(),
                },
            }],
            continuation: CoreProviderContinuation {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-5")),
                last_provider_message_id: Some(String::from("resp_prev")),
                updated_at: Some(String::from("123")),
            },
            ..CoreConversationSnapshot::default()
        }
    }

    /// Verifies the active OpenAI Responses adapter build path injects the previous response id into additional params.
    /// 验证当前 OpenAI Responses 适配器构建路径会把上一响应标识注入附加参数。
    #[tokio::test]
    async fn openai_responses_adapter_build_request_injects_previous_response_id() {
        // adapter is the production shared adapter bound to the OpenAI Responses transport spec.
        // adapter 是绑定到 OpenAI Responses 传输 spec 的生产共享适配器。
        let adapter = build_test_openai_responses_adapter();
        // snapshot provides the provider-scoped continuation cursor to be injected.
        // snapshot 提供待注入的 Provider 作用域 continuation 游标。
        let snapshot = build_test_snapshot();
        // config preserves user-supplied OpenAI Responses params while injecting continuation.
        // config 在注入 continuation 的同时保留用户提供的 OpenAI Responses 参数。
        let config = build_test_controller_config(
            Some(json!({
                "reasoning": { "effort": "high" }
            })),
            None,
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("OpenAI Responses request should build");
        let params = request
            .snapshot
            .additional_params
            .expect("additional params should exist");

        assert_eq!(params["previous_response_id"], "resp_prev");
        assert_eq!(params["reasoning"]["effort"], "high");
    }

    /// Verifies transient provider-facing injection suppresses OpenAI Responses continuation reuse.
    /// 验证临时 Provider 可见注入会抑制 OpenAI Responses continuation 复用。
    #[tokio::test]
    async fn openai_responses_adapter_build_request_suppresses_continuation_for_transient_injection()
     {
        // adapter is the production shared adapter bound to the OpenAI Responses transport spec.
        // adapter 是绑定到 OpenAI Responses 传输 spec 的生产共享适配器。
        let adapter = build_test_openai_responses_adapter();
        // snapshot provides a continuation cursor that must not be reused with transient context.
        // snapshot 提供在存在临时上下文时不得复用的 continuation 游标。
        let snapshot = build_test_snapshot();
        // config carries transient context injection, which marks the request as replay-unsafe.
        // config 携带临时上下文注入，用于标记该请求不适合回放复用。
        let config = build_test_controller_config(
            Some(json!({
                "reasoning": { "effort": "high" }
            })),
            Some(CoreProviderContextInjection {
                transient_prefix: String::from("<temporary>hidden</temporary>"),
                ..CoreProviderContextInjection::default()
            }),
        );

        let request = adapter
            .build_request(&snapshot, &config)
            .await
            .expect("OpenAI Responses request should build");
        let params = request
            .snapshot
            .additional_params
            .expect("additional params should exist");

        assert_eq!(
            params[SUPPRESS_OPENAI_RESPONSES_CONTINUATION_PARAM],
            serde_json::Value::Bool(true)
        );
        assert!(params.get("previous_response_id").is_none());
        assert_eq!(params["reasoning"]["effort"], "high");
    }
}
