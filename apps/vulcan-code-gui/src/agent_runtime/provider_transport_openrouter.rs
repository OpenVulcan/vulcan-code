//! OpenRouter host-controlled provider transport entrypoint.
//! OpenRouter 宿主控制 Provider 传输入口模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_common::{
    execute_host_controlled_stream, prepare_host_tool_runtime_context,
};
use crate::model::SessionId;
use reqwest::header::HeaderMap;
use rig::message::Message as RigMessage;
use serde_json::Value;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
};
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
    let provider_id = provider_id.to_string();
    let api_key = api_key.to_string();
    let model_id = model_id.to_string();
    let prompt = prompt.to_string();
    let system_preamble = system_preamble.to_string();
    let request_headers = headers.clone();
    let prepared_tool_context =
        prepare_host_tool_runtime_context(tools_manager, &resolved_tools, session_id);

    execute_host_controlled_stream(
        CoreControllerConfig {
            provider_id: provider_id.clone(),
            model_id: model_id.clone(),
            system_prompt: system_preamble,
            temperature,
            max_turns,
            additional_params: additional_params.clone(),
        },
        core_snapshot,
        prompt,
        move |live_sender| {
            crate::agent_runtime::provider_transport_openrouter_adapter::OpenRouterCoreStreamingAdapter {
            provider_id: provider_id.clone(),
            api_key,
            model_id,
            request_headers,
            resolved_tools,
            rig_tool_definitions: prepared_tool_context.rig_tool_definitions,
            live_sender,
        }
        },
        prepared_tool_context.runtime_bridge,
        "OpenRouter",
    )
}
