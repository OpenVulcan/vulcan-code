//! Anthropic host-controlled provider transport entrypoint.
//! Anthropic 宿主控制 Provider 传输入口模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_entry_inputs::{
    normalize_optional_base_url, prepare_provider_transport_runtime_inputs,
};
use crate::agent_runtime::provider_transport_stream_runtime::execute_host_controlled_stream;
use crate::model::SessionId;
use reqwest::header::HeaderMap;
use rig::message::Message as RigMessage;
use serde_json::Value;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Executes one host-controlled Anthropic prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 Anthropic 提示词调用，并流式返回规范化增量。
pub(crate) fn execute_anthropic_stream(
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
    let provider_id = prepared_runtime_inputs.entry_inputs.provider_id.clone();
    let base_url = normalize_optional_base_url(base_url);

    execute_host_controlled_stream(
        prepared_runtime_inputs.entry_inputs.controller_config,
        core_snapshot,
        prepared_runtime_inputs.entry_inputs.prompt,
        move |live_sender| {
            crate::agent_runtime::provider_transport_anthropic_adapter::AnthropicCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            base_url,
            model_id: prepared_runtime_inputs.entry_inputs.model_id,
            request_headers: prepared_runtime_inputs.entry_inputs.request_headers,
            resolved_tools,
            rig_tool_definitions: prepared_runtime_inputs.tool_context.rig_tool_definitions,
            live_sender,
        }
        },
        prepared_runtime_inputs.tool_context.runtime_bridge,
        "Anthropic 兼容",
    )
}
