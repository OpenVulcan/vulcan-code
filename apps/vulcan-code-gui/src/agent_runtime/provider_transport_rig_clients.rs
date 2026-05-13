//! Rig-specific provider-client construction helpers that keep third-party provider families at the outermost transport edge only.
//! 仅在最外层传输边界保留第三方 Provider 家族的 rig 专用客户端构造辅助模块。

use crate::agent_runtime::provider_protocol_anthropic_client::normalize_anthropic_base_url;
use crate::agent_runtime::provider_transport_adapter_support::map_provider_client_build_result;
use reqwest::header::HeaderMap;
use rig::client::CompletionClient as RigOpenAiCompletionClient;
use rig::providers::{anthropic, openai, openrouter};
use vulcan_core::types::CoreError as VulcanCoreError;

/// Builds one rig-backed OpenAI-compatible completion model while keeping OpenAI client construction at the transport edge only.
/// 在仅于传输边界保留 OpenAI 客户端构造的前提下，构建一个 rig 驱动的 OpenAI 兼容 completion model。
pub(crate) fn build_rig_openai_compatible_completion_model(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    request_headers: HeaderMap,
) -> Result<impl rig::completion::CompletionModel, VulcanCoreError> {
    let mut builder = openai::CompletionsClient::builder()
        .api_key(api_key)
        .http_headers(request_headers);

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client = map_provider_client_build_result(provider_id, "OpenAI 兼容", builder.build())?;
    Ok(client.completion_model(model_id))
}

/// Builds one rig-backed OpenRouter completion model while keeping OpenRouter client construction at the transport edge only.
/// 在仅于传输边界保留 OpenRouter 客户端构造的前提下，构建一个 rig 驱动的 OpenRouter completion model。
pub(crate) fn build_rig_openrouter_completion_model(
    provider_id: &str,
    api_key: &str,
    model_id: &str,
    request_headers: HeaderMap,
) -> Result<impl rig::completion::CompletionModel, VulcanCoreError> {
    let client = map_provider_client_build_result(
        provider_id,
        "OpenRouter",
        openrouter::Client::builder()
            .api_key(api_key)
            .http_headers(request_headers)
            .build(),
    )?;
    Ok(client.completion_model(model_id))
}

/// Builds one rig-backed Anthropic completion model while keeping Anthropic client construction at the transport edge only.
/// 在仅于传输边界保留 Anthropic 客户端构造的前提下，构建一个 rig 驱动的 Anthropic completion model。
pub(crate) fn build_rig_anthropic_completion_model(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    request_headers: HeaderMap,
) -> Result<impl rig::completion::CompletionModel, VulcanCoreError> {
    let mut builder = anthropic::Client::builder()
        .api_key(api_key)
        .http_headers(request_headers);

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(normalize_anthropic_base_url(base_url));
    }

    let client = map_provider_client_build_result(provider_id, "Anthropic 兼容", builder.build())?;
    Ok(client.completion_model(model_id))
}

/// Builds one rig-backed OpenAI Responses completion model while keeping Responses client construction at the transport edge only.
/// 在仅于传输边界保留 OpenAI Responses 客户端构造的前提下，构建一个 rig 驱动的 OpenAI Responses completion model。
pub(crate) fn build_rig_openai_responses_completion_model(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    request_headers: HeaderMap,
) -> Result<impl rig::completion::CompletionModel, VulcanCoreError> {
    let mut builder = openai::Client::builder()
        .api_key(api_key)
        .http_headers(request_headers);

    if let Some(base_url) = base_url.filter(|value| !value.trim().is_empty()) {
        builder = builder.base_url(base_url);
    }

    let client =
        map_provider_client_build_result(provider_id, "OpenAI Responses", builder.build())?;
    Ok(client.completion_model(model_id))
}
