//! Host-controlled provider transport router that selects the correct provider-family adapter for one prepared execution request.
//! 为单次已准备执行请求选择正确 Provider 家族适配器的宿主控制传输路由层。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_anthropic::execute_anthropic_stream;
use crate::agent_runtime::provider_transport_azure::execute_azure_openai_stream;
use crate::agent_runtime::provider_transport_cohere::execute_cohere_stream;
use crate::agent_runtime::provider_transport_google::execute_google_gemini_stream;
use crate::agent_runtime::provider_transport_openai::execute_openai_compatible_stream;
use crate::agent_runtime::provider_transport_openai_responses::execute_openai_responses_stream;
use crate::agent_runtime::provider_transport_openrouter::execute_openrouter_stream;
use crate::agent_runtime::provider_transport_request::ProviderTransportExecutionRequest;
use crate::provider_config::ProviderRuntimeKind;

/// Dispatches one prepared provider execution request into the correct provider-family transport adapter.
/// 将一份已准备好的 Provider 执行请求分发到正确的 Provider 家族传输适配器。
pub(crate) fn execute_provider_transport_stream(
    request: ProviderTransportExecutionRequest,
) -> ProviderDeltaStream {
    match request.runtime_kind {
        ProviderRuntimeKind::OpenAi => execute_openai_responses_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.task_runtime,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::OpenAiCompatible
        | ProviderRuntimeKind::XAi
        | ProviderRuntimeKind::Mistral
        | ProviderRuntimeKind::Groq
        | ProviderRuntimeKind::TogetherAi
        | ProviderRuntimeKind::Perplexity => execute_openai_compatible_stream(
            &request.provider_id,
            request.runtime_label.as_str(),
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.task_runtime,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::GoogleGemini => execute_google_gemini_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.task_runtime,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::Cohere => execute_cohere_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.task_runtime,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::OpenRouter => execute_openrouter_stream(
            &request.provider_id,
            &request.api_key,
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.task_runtime,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::Anthropic => execute_anthropic_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.task_runtime,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
        ProviderRuntimeKind::AzureOpenAi => execute_azure_openai_stream(
            &request.provider_id,
            &request.api_key,
            request.base_url.as_deref(),
            &request.model_id,
            &request.system_preamble,
            &request.prompt,
            request.request_history,
            request.core_snapshot,
            request.temperature,
            request.max_turns,
            &request.headers,
            request.task_runtime,
            request.tools_manager,
            request.session_id,
            request.resolved_tools,
            request.additional_params,
        ),
    }
}
