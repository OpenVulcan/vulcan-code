//! Azure OpenAI host-controlled provider transport entrypoint.
//! Azure OpenAI 宿主控制 Provider 传输入口模块。

use crate::agent_runtime::provider_stream::ProviderDeltaStream;
use crate::agent_runtime::provider_transport_entry_inputs::{
    PreparedProviderTransportStreamLaunch, prepare_provider_transport_stream_launch,
};
use crate::agent_runtime::provider_transport_request::ProviderTransportExecutionRequest;
use crate::agent_runtime::provider_transport_stream_runtime::execute_host_controlled_stream_with_prepared_inputs;

/// Executes one host-controlled Azure OpenAI prompt through `vulcan-core` and streams normalized deltas.
/// 通过 `vulcan-core` 执行一次宿主控制的 Azure OpenAI 提示词调用，并流式返回规范化增量。
pub(crate) fn execute_azure_openai_stream(
    request: ProviderTransportExecutionRequest,
) -> ProviderDeltaStream {
    let PreparedProviderTransportStreamLaunch {
        runtime_inputs,
        core_snapshot,
        api_key,
        base_url,
        runtime_label: _,
        interleaved_reasoning,
        prompt_intervention,
    } = prepare_provider_transport_stream_launch(request);

    execute_host_controlled_stream_with_prepared_inputs(
        runtime_inputs,
        core_snapshot,
        move |live_sender, adapter_inputs| {
            crate::agent_runtime::provider_transport_azure_adapter::new_azure_openai_core_streaming_adapter(
                adapter_inputs,
                api_key.clone(),
                base_url.clone(),
                interleaved_reasoning.clone(),
                live_sender,
            )
        },
        "Azure OpenAI",
        prompt_intervention,
    )
}
