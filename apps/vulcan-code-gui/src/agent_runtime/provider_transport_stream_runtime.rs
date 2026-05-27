//! Host-controlled provider streaming runtime wrapper for shared controller execution.
//! 面向共享控制器执行的宿主控制 Provider 流式运行时包装模块。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
use crate::agent_runtime::provider_stream::{ProviderDeltaStream, ProviderStreamDelta};
use crate::agent_runtime::provider_transport_entry_inputs::{
    PreparedProviderTransportEntryInputs, PreparedProviderTransportRuntimeInputs,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_request_budget::{
    BudgetCheckedStreamingAdapter, ProviderRequestBudgetGuard,
};
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::provider_transport_tool_context::PreparedHostToolRuntimeContext;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::task_runtime::{TaskRuntime, build_task_execution_handler};
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use async_stream::stream;
use reqwest::header::HeaderMap;
use std::sync::Arc;
use tokio::sync::mpsc::unbounded_channel;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::VulcanStreamingProviderAdapter;
use vulcan_core::tool::AllowAllToolPolicy;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
};
use vulcan_tools_manager::{ResolvedToolDefinition, VulcanToolsManager};

/// Prepared adapter launch inputs handed to one concrete Provider adapter constructor after the shared runtime wrapper consumes controller-only fields.
/// 在共享运行时包装器消费控制器专用字段后，交给具体 Provider 适配器构造过程的已准备输入对象。
#[derive(Clone)]
pub(crate) struct PreparedProviderTransportAdapterInputs {
    /// Stable provider identifier reused by the concrete Provider adapter.
    /// 被具体 Provider 适配器复用的稳定 Provider 标识。
    pub(crate) provider_id: String,
    /// Stable model identifier reused by the concrete Provider adapter.
    /// 被具体 Provider 适配器复用的稳定模型标识。
    pub(crate) model_id: String,
    /// Fully prepared request headers reused by the concrete Provider adapter.
    /// 被具体 Provider 适配器复用的完整已准备请求头。
    pub(crate) request_headers: HeaderMap,
    /// Mirrored provider-visible tool definitions reused by the concrete Provider adapter.
    /// 被具体 Provider 适配器复用的镜像 Provider 可见工具定义集合。
    pub(crate) provider_tool_definitions: Vec<ProviderToolDefinition>,
}

/// Executes one host-controlled provider stream through `vulcan-core`, forwards live deltas, and returns the final canonical state to the GUI layer.
/// 通过 `vulcan-core` 执行一次宿主控制的 Provider 流，转发实时增量，并把最终规范状态返回给图形界面层。
pub(crate) fn execute_host_controlled_stream<A, F>(
    session_id: SessionId,
    task_runtime: Arc<TaskRuntime>,
    tools_manager: Arc<VulcanToolsManager>,
    resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    controller_config: CoreControllerConfig,
    core_snapshot: CoreConversationSnapshot,
    prompt: String,
    provider_factory: F,
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    provider_label: String,
    request_budget_guard: Option<ProviderRequestBudgetGuard>,
) -> ProviderDeltaStream
where
    A: VulcanStreamingProviderAdapter<
            Request = OpenAiCompatibleCoreTurnRequest,
            Response = CoreProviderTurnResponse,
        > + Clone
        + Send
        + Sync
        + 'static,
    F: Fn(ProviderLiveDeltaSender) -> A + Send + Sync + 'static,
{
    Box::pin(stream! {
        let provider_factory = Arc::new(provider_factory);
        let provider_label_for_task = provider_label.clone();
        let provider_label_for_join = provider_label.clone();
        let (live_sender, mut live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let task_sender = live_sender.clone();
        let mut controller = VulcanCoreController::new(controller_config.clone(), core_snapshot);
        let provider_adapter = BudgetCheckedStreamingAdapter::new(
            provider_factory(live_sender.clone()),
            request_budget_guard.clone(),
        );
        let tool_executor =
            StreamingRuntimeToolExecutor::new(runtime_bridge, live_sender.clone());
        let task_backend_guard = task_runtime.install_backend(
            session_id,
            build_task_execution_handler(
                Arc::clone(&task_runtime),
                Arc::clone(&provider_factory),
                controller_config,
                tools_manager,
                resolved_tool_definitions,
                live_sender.clone(),
                provider_label.clone(),
            ),
        );
        let controller_prompt = prompt.clone();
        let controller_task = tokio::spawn(async move {
            let _task_backend_guard = task_backend_guard;
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
                    let last_successful_provider_usage =
                        controller.last_successful_provider_usage().cloned();
                    let _ = task_sender.send(Ok(ProviderStreamDelta::FinalConversationState(
                        core_snapshot_to_session_conversation_state(controller.snapshot()),
                        last_successful_provider_usage,
                    )));
                }
                Err(error) => {
                    let last_successful_provider_usage =
                        controller.last_successful_provider_usage().cloned();
                    // Preserve only the controller snapshot before the terminal error is reported.
                    // 只保留终态错误上报前的控制器快照，避免把损坏调用的流式输出写入上下文。
                    let _ = task_sender.send(Ok(ProviderStreamDelta::FinalConversationState(
                        core_snapshot_to_session_conversation_state(controller.snapshot()),
                        last_successful_provider_usage,
                    )));
                    let _ = task_sender.send(Err(format!(
                        "{}流式执行失败：{}。",
                        provider_label_for_task,
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
                provider_label_for_join,
                error
            ));
        }
    })
}

/// Executes one host-controlled provider stream from fully prepared runtime inputs, letting concrete entrypoints focus only on adapter-specific launch fields.
/// 基于完整已准备运行时输入执行一次宿主控制 Provider 流，使具体入口只关注适配器专用启动字段。
pub(crate) fn execute_host_controlled_stream_with_prepared_inputs<A, F>(
    prepared_runtime_inputs: PreparedProviderTransportRuntimeInputs,
    core_snapshot: CoreConversationSnapshot,
    provider_factory: F,
    provider_label: impl Into<String>,
) -> ProviderDeltaStream
where
    A: VulcanStreamingProviderAdapter<
            Request = OpenAiCompatibleCoreTurnRequest,
            Response = CoreProviderTurnResponse,
        > + Clone
        + Send
        + Sync
        + 'static,
    F: Fn(ProviderLiveDeltaSender, PreparedProviderTransportAdapterInputs) -> A
        + Send
        + Sync
        + 'static,
{
    let provider_label = provider_label.into();
    let PreparedProviderTransportRuntimeInputs {
        entry_inputs:
            PreparedProviderTransportEntryInputs {
                provider_id,
                model_id,
                prompt,
                request_headers,
                controller_config,
            },
        tool_context:
            PreparedHostToolRuntimeContext {
                session_id,
                tools_manager,
                runtime_bridge,
                task_runtime,
                resolved_tool_definitions,
                provider_tool_definitions,
            },
        request_budget_guard,
    } = prepared_runtime_inputs;

    let adapter_inputs = PreparedProviderTransportAdapterInputs {
        provider_id,
        model_id,
        request_headers,
        provider_tool_definitions,
    };

    execute_host_controlled_stream(
        session_id,
        task_runtime,
        tools_manager,
        resolved_tool_definitions,
        controller_config,
        core_snapshot,
        prompt,
        move |live_sender| provider_factory(live_sender, adapter_inputs.clone()),
        runtime_bridge,
        provider_label,
        request_budget_guard,
    )
}
