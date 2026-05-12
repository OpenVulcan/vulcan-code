//! OpenAI-compatible host-controlled provider transport entrypoint.
//! OpenAI 兼容宿主控制 Provider 传输入口模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_entry_inputs::{
    normalize_optional_base_url, prepare_provider_transport_runtime_inputs,
};
use crate::agent_runtime::provider_transport_stream_runtime::execute_host_controlled_stream_with_prepared_inputs;
use crate::model::SessionId;
use reqwest::header::HeaderMap;
use rig::message::Message as RigMessage;
use serde_json::Value;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Executes one host-controlled OpenAI-compatible prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenAI 兼容提示词调用，并流式返回规范化增量。
pub(crate) fn execute_openai_compatible_stream(
    provider_id: &str,
    api_key: &str,
    base_url: Option<&str>,
    model_id: &str,
    system_preamble: &str,
    prompt: &str,
    _request_history: Vec<RigMessage>,
    core_snapshot: CoreConversationSnapshot,
    temperature: Option<f64>,
    max_turns: usize,
    headers: &HeaderMap,
    tools_manager: VulcanToolsManager,
    session_id: SessionId,
    resolved_tools: Vec<ResolvedToolDefinition>,
    additional_params: Option<Value>,
) -> ProviderDeltaStream {
    let api_key = api_key.to_string();
    let prepared_runtime_inputs = prepare_provider_transport_runtime_inputs(
        provider_id,
        model_id,
        system_preamble,
        prompt,
        temperature,
        max_turns,
        headers,
        additional_params,
        tools_manager,
        &resolved_tools,
        session_id,
    );
    let base_url = normalize_optional_base_url(base_url);

    execute_host_controlled_stream_with_prepared_inputs(
        prepared_runtime_inputs,
        core_snapshot,
        move |live_sender, adapter_inputs| {
            crate::agent_runtime::provider_transport_openai_adapter::OpenAiCompatibleCoreStreamingAdapter {
            provider_id: adapter_inputs.provider_id,
            api_key,
            base_url,
            model_id: adapter_inputs.model_id,
            request_headers: adapter_inputs.request_headers,
            resolved_tools,
            rig_tool_definitions: adapter_inputs.rig_tool_definitions,
            live_sender,
        }
        },
        "OpenAI 兼容",
    )
}
