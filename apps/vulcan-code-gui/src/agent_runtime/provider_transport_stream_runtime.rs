//! Host-controlled provider streaming runtime wrapper for shared controller execution.
//! 面向共享控制器执行的宿主控制 Provider 流式运行时包装模块。

use crate::agent_runtime::provider_request_bridge::core_snapshot_to_session_conversation_state;
use crate::agent_runtime::provider_stream::{ProviderDeltaStream, ProviderStreamDelta};
use crate::agent_runtime::provider_transport_common::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use async_stream::stream;
use std::sync::Arc;
use tokio::sync::mpsc::unbounded_channel;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::VulcanStreamingProviderAdapter;
use vulcan_core::tool::AllowAllToolPolicy;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
};

/// Executes one host-controlled provider stream through `vulcan-core`, forwards live deltas, and returns the final canonical state to the GUI layer.
/// 通过 `vulcan-core` 执行一次宿主控制的 Provider 流，转发实时增量，并把最终规范状态返回给图形界面层。
pub(crate) fn execute_host_controlled_stream<A, F>(
    controller_config: CoreControllerConfig,
    core_snapshot: CoreConversationSnapshot,
    prompt: String,
    provider_factory: F,
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    provider_label: &'static str,
) -> ProviderDeltaStream
where
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse> + Send + Sync + 'static,
    F: FnOnce(ProviderLiveDeltaSender) -> A + Send + 'static,
{
    Box::pin(stream! {
        let (live_sender, mut live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let task_sender = live_sender.clone();
        let mut controller = VulcanCoreController::new(controller_config, core_snapshot);
        let provider_adapter = provider_factory(live_sender.clone());
        let tool_executor =
            StreamingRuntimeToolExecutor::new(runtime_bridge, live_sender.clone());
        let controller_prompt = prompt.clone();
        let controller_task = tokio::spawn(async move {
            let tool_policy = AllowAllToolPolicy;
            match controller
                .run_user_turn_streaming(
                    &provider_adapter,
                    &tool_executor,
                    &tool_policy,
                    controller_prompt,
                )
                .await
            {
                Ok(_) => {
                    let _ = task_sender.send(Ok(ProviderStreamDelta::FinalConversationState(
                        core_snapshot_to_session_conversation_state(controller.snapshot()),
                    )));
                }
                Err(error) => {
                    let _ = task_sender.send(Err(format!(
                        "{}流式执行失败：{}。",
                        provider_label,
                        error
                    )));
                }
            }
        });
        drop(live_sender);

        while let Some(item) = live_receiver.recv().await {
            yield item;
        }

        if let Err(error) = controller_task.await {
            yield Err(format!(
                "{}控制器后台任务失败：{}。",
                provider_label,
                error
            ));
        }
    })
}
