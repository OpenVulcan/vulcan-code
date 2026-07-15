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
use tokio::task::AbortHandle;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::provider::VulcanStreamingProviderAdapter;
use vulcan_core::tool::AllowAllToolPolicy;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    CoreError, ProviderTurnResponse as CoreProviderTurnResponse,
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

/// Aborts one spawned Provider controller task when its owning output stream disappears.
/// 当持有输出流消失时，中止对应已派生 Provider 控制器任务。
struct AbortProviderControllerOnDrop {
    /// Exact abort authority for the controller task owned by the current stream instance.
    /// 当前流实例所持控制器任务的精确中止权限。
    handle: AbortHandle,
}

impl AbortProviderControllerOnDrop {
    /// Creates one stream-scoped abort owner for a spawned Provider controller task.
    /// 为已派生 Provider 控制器任务创建一个流作用域中止所有者。
    ///
    /// # Parameters
    /// 参数
    /// `handle` identifies the exact controller task whose lifetime must not outlive the output stream.
    /// `handle` 标识生命周期不得超过输出流的精确控制器任务。
    ///
    /// # Returns
    /// 返回值
    /// A guard that aborts the task when the owning stream scope is dropped.
    /// 返回在持有流作用域被丢弃时中止任务的守卫。
    fn new(handle: AbortHandle) -> Self {
        Self { handle }
    }
}

impl Drop for AbortProviderControllerOnDrop {
    /// Aborts the owned controller task when stream execution leaves its scope.
    /// 当流执行离开其作用域时中止所持控制器任务。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; aborting an already completed task is a harmless no-op.
    /// 不返回值；中止已经完成的任务是无害的空操作。
    fn drop(&mut self) {
        self.handle.abort();
    }
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
                // controller rejects an invalid retry snapshot through the stream instead of panicking inside the task.
                // controller 通过流拒绝无效的重试快照，而不是在任务内部触发 panic。
                let mut controller = match VulcanCoreController::new(
                    controller_config.clone(),
                    core_snapshot.clone(),
                ) {
                    Ok(controller) => controller,
                    Err(error) => {
                        let _ = task_sender.send(Err(format!(
                            "Invalid core conversation snapshot: {error}"
                        )));
                        break;
                    }
                };
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
                        // checkpoint_delta exposes a checkpoint only after its complete message namespace passes validation.
                        // checkpoint_delta 仅在完整消息命名空间通过校验后暴露检查点。
                        let checkpoint_delta = core_snapshot_to_session_conversation_state(snapshot)
                            .map(|conversation_state| {
                                ProviderStreamDelta::FinalConversationState(
                                    conversation_state,
                                    usage.cloned(),
                                )
                            });
                        let _ = checkpoint_sender.send(checkpoint_delta);
                    };
                    let intervention = move |controller: &mut VulcanCoreController| {
                        let Some(intervention) = prompt_intervention.as_ref() else {
                            return Ok(false);
                        };
                            intervention(session_id, controller).map_err(|error| {
                                CoreError::InvalidState {
                                    message: error.to_string(),
                                }
                            })
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
                // backendReleaseResult is authoritative cleanup evidence required before retry or completion handling.
                // backendReleaseResult 是进入重试或完成处理前所需的权威清理证据。
                let backend_release_result = task_backend_guard.release();

                while let Ok(item) = attempt_receiver.try_recv() {
                    if let Ok(delta) = &item {
                        unsafe_delta_seen |= provider_delta_makes_retry_unsafe(delta);
                    }
                    let _ = task_sender.send(item);
                }

                if let Err(error) = backend_release_result {
                    let _ = task_sender.send(Err(format!(
                        "The task backend could not be released after the provider attempt: {error}"
                    )));
                    break;
                }

                match attempt_result {
                    Ok(_) => {
                        let last_successful_provider_usage =
                            controller.last_successful_provider_usage().cloned();
                        // final_delta rejects an invalid successful snapshot instead of publishing corrupt terminal state.
                        // final_delta 会拒绝无效的成功快照，而不是发布损坏的终态。
                        let final_delta =
                            core_snapshot_to_session_conversation_state(controller.snapshot()).map(
                                |conversation_state| {
                                    ProviderStreamDelta::FinalConversationState(
                                        conversation_state,
                                        last_successful_provider_usage,
                                    )
                                },
                            );
                        let _ = task_sender.send(final_delta);
                        break;
                    }
                    Err(error) => {
                        if let Some(decision) =
                            retry_policy.next_retry(&error, completed_retries, unsafe_delta_seen)
                        {
                            completed_retries += 1;
                            let delay_ms = decision.delay.as_millis() as u64;
                            // failedRequestDelta closes the exact physical request before the retry wait begins.
                            // failedRequestDelta 在重试等待开始前关闭精确的真实请求。
                            let failed_request_delta = ProviderStreamDelta::RequestFailed {
                                will_retry: true,
                                retry_number: decision.retry_number,
                                max_retries: decision.max_retries,
                                delay_ms,
                                error_category: String::from("retryable_provider_transport_error"),
                            };
                            if task_sender.send(Ok(failed_request_delta)).is_err() {
                                break;
                            }
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
                        // failedRequestDelta closes the exact physical request before terminal stream teardown.
                        // failedRequestDelta 在终态流清理前关闭精确的真实请求。
                        let failed_request_delta = ProviderStreamDelta::RequestFailed {
                            will_retry: false,
                            retry_number: 0,
                            max_retries: completed_retries,
                            delay_ms: 0,
                            error_category: String::from("provider_transport_error"),
                        };
                        let _ = task_sender.send(Ok(failed_request_delta));
                        // Preserve only the controller snapshot before the terminal error is reported.
                        // 只保留终态错误上报前的控制器快照，避免把损坏调用的流式输出写入上下文。
                        // final_delta preserves the last valid controller state only when its message identities remain canonical.
                        // final_delta 仅在消息身份仍规范时保留最后一份有效控制器状态。
                        let final_delta =
                            core_snapshot_to_session_conversation_state(controller.snapshot()).map(
                                |conversation_state| {
                                    ProviderStreamDelta::FinalConversationState(
                                        conversation_state,
                                        last_successful_provider_usage,
                                    )
                                },
                            );
                        let _ = task_sender.send(final_delta);
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
        // controllerTaskAbortGuard binds the spawned controller lifetime to this exact output stream.
        // controllerTaskAbortGuard 将已派生控制器生命周期绑定到当前精确输出流。
        let _controller_task_abort_guard =
            AbortProviderControllerOnDrop::new(controller_task.abort_handle());
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

#[cfg(test)]
mod tests {
    //! Provider controller stream-ownership tests.
    //! Provider 控制器流所有权测试。

    use super::*;
    use tokio::sync::Notify;

    /// Verifies dropping the stream-scoped guard cancels its still-running controller task.
    /// 验证丢弃流作用域守卫会取消其仍在运行的控制器任务。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails if the controller task survives owner teardown.
    /// 此测试无返回值；若控制器任务在所有者销毁后仍存活则测试失败。
    #[tokio::test]
    async fn provider_controller_abort_guard_cancels_owned_task_on_drop() {
        // taskStarted confirms the spawned controller reached its pending body before cancellation.
        // taskStarted 确认已派生控制器在取消前已经进入等待主体。
        let task_started = Arc::new(Notify::new());
        // taskStartedSignal transfers the exact start notification into the spawned task.
        // taskStartedSignal 将精确启动通知转移到已派生任务中。
        let task_started_signal = Arc::clone(&task_started);
        // controllerTask models a controller that would run forever without stream-owned cancellation.
        // controllerTask 模拟在没有流所有权取消时会永久运行的控制器。
        let controller_task = tokio::spawn(async move {
            task_started_signal.notify_one();
            std::future::pending::<()>().await;
        });
        // controllerTaskAbortGuard owns cancellation for the exact spawned task.
        // controllerTaskAbortGuard 持有精确已派生任务的取消责任。
        let controller_task_abort_guard =
            AbortProviderControllerOnDrop::new(controller_task.abort_handle());
        task_started.notified().await;

        drop(controller_task_abort_guard);
        // joinError proves the owned task terminated through cancellation rather than normal completion.
        // joinError 证明所持任务通过取消终止，而不是正常完成。
        let join_error = controller_task
            .await
            .expect_err("dropping the stream owner must cancel the controller task");
        assert!(join_error.is_cancelled());
    }
}
