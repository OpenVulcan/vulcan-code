//! Host-controlled provider streaming runtime wrapper for shared controller execution.
//! 面向共享控制器执行的宿主控制 Provider 流式运行时包装模块。

use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
use crate::agent_runtime::provider_stream::{ProviderDeltaStream, ProviderStreamDelta};
use crate::agent_runtime::provider_transport_entry_inputs::{
    PreparedProviderTransportEntryInputs, PreparedProviderTransportRuntimeInputs,
};
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_request::ProviderTransportTurnStartMode;
use crate::agent_runtime::provider_transport_request_budget::{
    BudgetCheckedStreamingAdapter, ProviderRequestBudgetGuard,
};
use crate::agent_runtime::provider_transport_retry::{
    ProviderRetryPolicy, provider_delta_makes_retry_unsafe,
};
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::provider_transport_tool_context::PreparedHostToolRuntimeContext;
use crate::agent_runtime::provider_transport_turn_request::OpenAiCompatibleCoreTurnRequest;
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::task_runtime::{TaskRuntime, build_task_execution_handler};
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use async_stream::stream;
use reqwest::header::HeaderMap;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc::unbounded_channel;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::VulcanStreamingProviderAdapter;
use vulcan_core::tool::AllowAllToolPolicy;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    ProviderTurnResponse as CoreProviderTurnResponse,
};

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

/// Executes one host-controlled provider stream through `vulcan-core`, forwards live deltas, and returns the final canonical state to the workbench layer.
/// 通过 `vulcan-core` 执行一次宿主控制的 Provider 流，转发实时增量，并把最终规范状态返回给工作台层。
pub(crate) fn execute_host_controlled_stream<A, F>(
    session_id: SessionId,
    task_runtime: Arc<TaskRuntime>,
    controller_config: CoreControllerConfig,
    core_snapshot: CoreConversationSnapshot,
    prompt: String,
    turn_start_mode: ProviderTransportTurnStartMode,
    provider_factory: F,
    runtime_bridge: Arc<VulcanManagedToolRuntimeBridge>,
    provider_label: String,
    request_budget_guard: Option<ProviderRequestBudgetGuard>,
    retry_policy: ProviderRetryPolicy,
    prompt_intervention: Option<RuntimePromptIntervention>,
    tool_request_rejection_reason: Option<String>,
    tool_request_rejection_all_tools: bool,
    tool_request_rejection_tool_ids: Vec<String>,
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
        let provider_label_for_join = provider_label.clone();
        let prompt_intervention = prompt_intervention.clone();
        let (live_sender, mut live_receiver) =
            unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let task_sender = live_sender.clone();
        let controller_prompt = prompt.clone();
        let controller_task = tokio::spawn(async move {
            let mut completed_retries = 0u32;
            loop {
                let (attempt_sender, mut attempt_receiver) =
                    unbounded_channel::<Result<ProviderStreamDelta, String>>();
                let mut controller =
                    VulcanCoreController::new(controller_config.clone(), core_snapshot.clone());
                let provider_adapter = BudgetCheckedStreamingAdapter::new(
                    provider_factory(attempt_sender.clone()),
                    request_budget_guard.clone(),
                );
                let tool_executor = StreamingRuntimeToolExecutor::new(
                    Arc::clone(&runtime_bridge),
                    attempt_sender.clone(),
                    tool_request_rejection_reason.clone(),
                    tool_request_rejection_all_tools,
                    tool_request_rejection_tool_ids.clone(),
                );
                let task_backend_guard = match task_runtime.install_backend(
                    session_id,
                    build_task_execution_handler(
                        Arc::clone(&task_runtime),
                        controller_config.clone(),
                        attempt_sender.clone(),
                    ),
                ) {
                    Ok(guard) => guard,
                    Err(error) => {
                        let _ = task_sender.send(Err(error));
                        break;
                    }
                };
                let tool_policy = AllowAllToolPolicy;
                let mut unsafe_delta_seen = false;
                let attempt_result = {
                    let checkpoint_sender = attempt_sender.clone();
                    let prompt_intervention = prompt_intervention.clone();
                    let checkpoint = move |
                        snapshot: &CoreConversationSnapshot,
                        usage: Option<&vulcan_core::types::ProviderUsageSnapshot>,
                    | {
                        let _ = checkpoint_sender.send(Ok(
                            ProviderStreamDelta::FinalConversationState(
                                core_snapshot_to_session_conversation_state(snapshot),
                                usage.cloned(),
                            ),
                        ));
                    };
                    let intervention = move |controller: &mut VulcanCoreController| {
                        prompt_intervention
                            .as_ref()
                            .map(|intervention| intervention(session_id, controller))
                            .unwrap_or(false)
                    };
                    let run_future = async {
                        match turn_start_mode {
                            ProviderTransportTurnStartMode::UserPrompt => {
                                controller.run_user_turn_streaming_with_checkpoints_and_interventions(
                                    &provider_adapter,
                                    &tool_executor,
                                    &tool_policy,
                                    controller_prompt.clone(),
                                    checkpoint,
                                    intervention,
                                )
                                .await
                            }
                            ProviderTransportTurnStartMode::SeededProviderInput => {
                                controller.run_seeded_turn_streaming_with_checkpoints_and_interventions(
                                    &provider_adapter,
                                    &tool_executor,
                                    &tool_policy,
                                    checkpoint,
                                    intervention,
                                )
                                .await
                            }
                        }
                    };
                    tokio::pin!(run_future);
                    loop {
                        tokio::select! {
                            maybe_item = attempt_receiver.recv() => {
                                if let Some(item) = maybe_item {
                                    if let Ok(delta) = &item {
                                        unsafe_delta_seen |= provider_delta_makes_retry_unsafe(delta);
                                    }
                                    let _ = task_sender.send(item);
                                }
                            }
                            result = &mut run_future => {
                                break result;
                            }
                        }
                    }
                };
                drop(task_backend_guard);

                while let Ok(item) = attempt_receiver.try_recv() {
                    if let Ok(delta) = &item {
                        unsafe_delta_seen |= provider_delta_makes_retry_unsafe(delta);
                    }
                    let _ = task_sender.send(item);
                }

                match attempt_result {
                    Ok(_) => {
                        let last_successful_provider_usage =
                            controller.last_successful_provider_usage().cloned();
                        let _ = task_sender.send(Ok(ProviderStreamDelta::FinalConversationState(
                            core_snapshot_to_session_conversation_state(controller.snapshot()),
                            last_successful_provider_usage,
                        )));
                        break;
                    }
                    Err(error) => {
                        if let Some(decision) =
                            retry_policy.next_retry(&error, completed_retries, unsafe_delta_seen)
                        {
                            completed_retries += 1;
                            let delay_ms = decision.delay.as_millis() as u64;
                            let retry_event = crate::agent_runtime::events::RuntimeEvent::new(
                                crate::agent_runtime::events::RuntimeEventKind::Retrying,
                                "Provider 自动重试",
                                format!(
                                    "{} 第 {}/{} 次重试将在 {} ms 后开始：{}",
                                    provider_label,
                                    decision.retry_number,
                                    decision.max_retries,
                                    delay_ms,
                                    decision.reason
                                ),
                            )
                            .with_metadata(Some(json!({
                                "attempt": decision.retry_number,
                                "max_retries": decision.max_retries,
                                "delay_ms": delay_ms,
                                "reason": decision.reason,
                            })));
                            // Stop retrying when the host runtime consumer has already dropped this stream.
                            // 当宿主运行时消费端已经丢弃当前流时，停止继续后台重试。
                            if task_sender
                                .send(Ok(ProviderStreamDelta::RuntimeEvent(retry_event)))
                                .is_err()
                            {
                                break;
                            }
                            tokio::time::sleep(decision.delay).await;
                            continue;
                        }

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
                            provider_label,
                            error
                        )));
                        break;
                    }
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
    prompt_intervention: Option<RuntimePromptIntervention>,
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
                turn_start_mode,
                request_headers,
                controller_config,
            },
        tool_context:
            PreparedHostToolRuntimeContext {
                session_id,
                runtime_bridge,
                task_runtime,
                provider_tool_definitions,
                ..
            },
        request_budget_guard,
        retry_policy,
        tool_request_rejection_reason,
        tool_request_rejection_all_tools,
        tool_request_rejection_tool_ids,
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
        controller_config,
        core_snapshot,
        prompt,
        turn_start_mode,
        move |live_sender| provider_factory(live_sender, adapter_inputs.clone()),
        runtime_bridge,
        provider_label,
        request_budget_guard,
        retry_policy,
        prompt_intervention,
        tool_request_rejection_reason,
        tool_request_rejection_all_tools,
        tool_request_rejection_tool_ids,
    )
}
