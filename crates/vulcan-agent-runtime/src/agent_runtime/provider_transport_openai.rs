//! OpenAI-compatible host-controlled provider transport entrypoint.
//! OpenAI 兼容宿主控制 Provider 传输入口模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_entry_inputs::{
    PreparedProviderTransportStreamLaunch, prepare_provider_transport_stream_launch,
};
use crate::agent_runtime::provider_transport_request::ProviderTransportExecutionRequest;
use crate::agent_runtime::provider_transport_stream_runtime::execute_host_controlled_stream_with_prepared_inputs;

/// Executes one host-controlled OpenAI-compatible prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 OpenAI 兼容提示词调用，并流式返回规范化增量。
pub(crate) fn execute_openai_compatible_stream(
    request: ProviderTransportExecutionRequest,
) -> ProviderDeltaStream {
    let PreparedProviderTransportStreamLaunch {
        runtime_inputs,
        core_snapshot,
        api_key,
        base_url,
        runtime_label,
        interleaved_reasoning,
        prompt_intervention,
    } = prepare_provider_transport_stream_launch(request);
    let provider_label = runtime_label.clone();

    execute_host_controlled_stream_with_prepared_inputs(
        runtime_inputs,
        core_snapshot,
        move |live_sender, adapter_inputs| {
            crate::agent_runtime::provider_transport_openai_adapter::new_openai_compatible_core_streaming_adapter(
                adapter_inputs,
                provider_label.clone(),
                api_key.clone(),
                base_url.clone(),
                interleaved_reasoning.clone(),
                live_sender,
            )
        },
        runtime_label,
        prompt_intervention,
    )
}
