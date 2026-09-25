//! Runtime stream driver that consumes one prepared launch payload and emits provider events until terminal completion.
//! 消费一份已准备启动载荷并持续发出 Provider 事件直到终态完成的运行时流驱动模块。

use crate::agent_runtime::events::ExecutionStreamEvent;
use crate::agent_runtime::runtime_stream_driver_loop::drive_runtime_stream_provider_loop;
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch;
use crate::agent_runtime::runtime_stream_setup::build_runtime_start_events;
use crate::provider_config::ProviderConfigurationFile;
use async_stream::stream;
use futures_util::{Stream, StreamExt};

/// Drives one prepared runtime-stream launch payload and yields all start, delta, and terminal events in order.
/// 驱动一份已准备完成的运行时流启动载荷，并按顺序产出全部启动、增量与终态事件。
pub fn drive_runtime_stream_launch(
    config: ProviderConfigurationFile,
    launch: RuntimeStreamLaunch,
    prompt_intervention: Option<RuntimePromptIntervention>,
) -> impl Stream<Item = ExecutionStreamEvent> + Send + 'static {
    // Box at construction so callers do not reserve the driver's concrete state on their poll stack.
    // 在构造处装箱，避免调用方在轮询栈上为驱动的具体状态预留空间。
    Box::pin(stream! {
        let session_id = launch.session_id;
        let preparation = launch.preparation.clone();
        let discovery_events = launch.discovery_events.clone();
        let lowering_repair_events = launch.lowering_repair_events.clone();

        for event in build_runtime_start_events(session_id, &preparation, discovery_events, lowering_repair_events) {
            yield event;
        }

        // Keep the large provider loop out of the launch driver's inline state.
        // 避免将大型供应商循环内嵌在启动驱动状态中。
        let mut provider_events = drive_runtime_stream_provider_loop(config, launch, prompt_intervention);
        while let Some(event) = provider_events.next().await {
            yield event;
        }
    })
}
