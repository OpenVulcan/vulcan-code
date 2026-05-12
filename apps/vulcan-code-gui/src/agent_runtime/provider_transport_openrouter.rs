//! OpenRouter host-controlled provider transport entrypoint.
//! OpenRouter 宿主控制 Provider 传输入口模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_entry_inputs::prepare_provider_transport_runtime_inputs;
use crate::agent_runtime::provider_transport_stream_runtime::execute_host_controlled_stream_with_prepared_inputs;
use crate::model::SessionId;
use reqwest::header::HeaderMap;
use rig::message::Message as RigMessage;
use serde_json::Value;
use vulcan_core::types::ConversationSnapshot as CoreConversationSnapshot;
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Executes one host-controlled OpenRouter prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenRouter 提示词调用，并流式返回规范化增量。
pub(crate) fn execute_openrouter_stream(
    provider_id: &str,
    api_key: &str,
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

    execute_host_controlled_stream_with_prepared_inputs(
        prepared_runtime_inputs,
        core_snapshot,
        move |live_sender, adapter_inputs| {
            crate::agent_runtime::provider_transport_openrouter_adapter::OpenRouterCoreStreamingAdapter::new(
                adapter_inputs,
                api_key,
                resolved_tools,
                live_sender,
            )
        },
        "OpenRouter",
    )
}
