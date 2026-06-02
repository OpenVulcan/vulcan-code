//! Direct Azure OpenAI HTTP and SSE transport owned by the host runtime.
//! 宿主运行时自有的 Azure OpenAI HTTP 与 SSE 直连传输模块。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_openai_compatible_http::{
    OpenAiCompatibleAuthorizationStrategy, OpenAiCompatibleHttpStreamingOptions,
    execute_openai_compatible_http_streaming_turn_with_options,
};
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use reqwest::header::HeaderMap;
use vulcan_core::provider::VulcanProviderDeltaSink;
use vulcan_core::types::{
    CoreError as VulcanCoreError, ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Latest Azure OpenAI chat-completions API version used by the host-owned direct transport.
/// 宿主自有直连传输使用的最新 Azure OpenAI Chat Completions API 版本。
const AZURE_OPENAI_CHAT_COMPLETIONS_API_VERSION: &str = "2024-10-21";

/// Executes one direct Azure OpenAI streaming turn without going through any third-party completion-model abstraction.
/// 在不经过任何第三方 completion model 抽象的前提下执行一次直连 Azure OpenAI 流式回合。
pub(crate) async fn execute_azure_openai_http_streaming_turn<S>(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    deployment_id: &str,
    request_headers: HeaderMap,
    request: OpenAiCompatibleCoreTurnRequest,
    provider_tool_definitions: &[ProviderToolDefinition],
    live_sender: &ProviderLiveDeltaSender,
    sink: &mut S,
) -> Result<CoreProviderTurnResponse, VulcanCoreError>
where
    S: VulcanProviderDeltaSink,
{
    execute_openai_compatible_http_streaming_turn_with_options(
        provider_id,
        api_key,
        deployment_id,
        request_headers,
        request,
        provider_tool_definitions,
        live_sender,
        sink,
        OpenAiCompatibleHttpStreamingOptions {
            provider_label: "Azure OpenAI".to_string(),
            endpoint: build_azure_openai_chat_completions_endpoint(base_url, deployment_id),
            include_model_field: false,
            authorization_strategy: OpenAiCompatibleAuthorizationStrategy::StaticApiKeyHeader {
                header_name: "api-key",
                header_label: "API Key ",
            },
        },
    )
    .await
}

/// Builds the final Azure OpenAI `/chat/completions` endpoint from the resolved resource base URL and deployment identifier.
/// 根据已解析资源基础地址与部署标识构建最终 Azure OpenAI `/chat/completions` 端点。
fn build_azure_openai_chat_completions_endpoint(
    base_url: Option<&str>,
    deployment_id: &str,
) -> String {
    let normalized_base_url = base_url
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().trim_end_matches('/').to_string())
        .unwrap_or_else(|| String::from("https://example-resource.openai.azure.com/openai"));

    if normalized_base_url.ends_with("/openai/v1") {
        return format!(
            "{}/chat/completions?api-version={}",
            normalized_base_url, AZURE_OPENAI_CHAT_COMPLETIONS_API_VERSION
        );
    }

    format!(
        "{}/deployments/{}/chat/completions?api-version={}",
        normalized_base_url, deployment_id, AZURE_OPENAI_CHAT_COMPLETIONS_API_VERSION
    )
}
