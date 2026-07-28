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
    BudgetCheckedStreamingAdapter, ProviderBudgetRequest, ProviderRequestBudgetGuard,
};
use crate::agent_runtime::provider_transport_retry::{
    ProviderRetryPolicy, provider_delta_makes_retry_unsafe,
};
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::provider_transport_tool_context::PreparedHostToolRuntimeContext;
use crate::agent_runtime::runtime_stream_intervention::RuntimePromptIntervention;
use crate::agent_runtime::task_runtime::{TaskRuntime, build_task_execution_handler};
use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::model::SessionId;
use async_stream::stream;
use serde_json::json;
use std::collections::BTreeSet;
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
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse>
        + Clone
        + Send
        + Sync
        + 'static,
    A::Request: ProviderBudgetRequest,
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
            // retrySnapshot advances to the controller's last committed state so later physical Provider requests can retry without replaying completed tools.
            // retrySnapshot 推进到控制器最后提交的状态，使后续真实 Provider 请求可在不重放已完成工具的前提下重试。
            let mut retry_snapshot = core_snapshot;
            // retryResumesSeededInput prevents the original user prompt from being appended twice after resuming a committed snapshot.
            // retryResumesSeededInput 防止从已提交快照恢复后再次追加原始用户提示词。
            let mut retry_resumes_seeded_input = false;
            loop {
                let (attempt_sender, mut attempt_receiver) =
                    unbounded_channel::<Result<ProviderStreamDelta, String>>();
                // controller rejects an invalid retry snapshot through the stream instead of panicking inside the task.
                // controller 通过流拒绝无效的重试快照，而不是在任务内部触发 panic。
                let mut controller = match VulcanCoreController::new(
                    controller_config.clone(),
                    retry_snapshot.clone(),
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
                // physicalRequestSeen distinguishes the retry attempt's first request from a later request reached after success.
                // physicalRequestSeen 区分本次重试尝试的首个请求与成功后继续到达的后续请求。
                let mut physical_request_seen = false;
                // activeToolRequestStreams tracks visible request drafts for attempt-scoped terminal cleanup.
                // activeToolRequestStreams 跟踪可见请求草稿，用于按尝试执行终态清理。
                let mut active_tool_request_streams = BTreeSet::<String>::new();
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
                        let effective_turn_start_mode = if retry_resumes_seeded_input {
                            ProviderTransportTurnStartMode::SeededProviderInput
                        } else {
                            turn_start_mode
                        };
                        match effective_turn_start_mode {
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
                                        observe_provider_request_retry_safety(
                                            &mut unsafe_delta_seen,
                                            delta,
                                        );
                                        observe_provider_request_retry_reset(
                                            &mut physical_request_seen,
                                            &mut completed_retries,
                                            delta,
                                        );
                                        observe_active_tool_request_streams(
                                            &mut active_tool_request_streams,
                                            delta,
                                        );
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
                        observe_provider_request_retry_safety(&mut unsafe_delta_seen, delta);
                        observe_provider_request_retry_reset(
                            &mut physical_request_seen,
                            &mut completed_retries,
                            delta,
                        );
                        observe_active_tool_request_streams(
                            &mut active_tool_request_streams,
                            delta,
                        );
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
                        abort_active_tool_request_streams(
                            &task_sender,
                            &mut active_tool_request_streams,
                            "provider_stream_completed_without_request_terminal",
                        );
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
                        // retryDecision is computed before request cleanup so superseded cards receive the exact retry terminal state.
                        // retryDecision 会在请求清理前计算，使被替换卡片获得精确的重试终态。
                        let retry_decision =
                            retry_policy.next_retry(&error, completed_retries, unsafe_delta_seen);
                        abort_active_tool_request_streams(
                            &task_sender,
                            &mut active_tool_request_streams,
                            if retry_decision.is_some() {
                                "provider_retry_superseded"
                            } else {
                                "provider_attempt_failed"
                            },
                        );
                        if let Some(decision) = retry_decision {
                            // retrySnapshot contains the user prompt, completed assistant/tool messages, and no failed Provider response.
                            // retrySnapshot 包含用户提示词及已完成的助手/工具消息，但不包含失败的 Provider 响应。
                            retry_snapshot = controller.snapshot().clone();
                            retry_resumes_seeded_input = true;
                            completed_retries = completed_retries.saturating_add(1);
                            let delay_ms = decision.delay.as_millis() as u64;
                            // delaySeconds rounds sub-second waits upward for an honest user-facing countdown value.
                            // delaySeconds 向上取整不足一秒的等待，以提供真实的用户可见倒计时数值。
                            let delay_seconds = delay_ms.div_ceil(1_000);
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
                            // retryDetail distinguishes count-limited transient retries from cancellation-bound rate-limit retries.
                            // retryDetail 区分受次数限制的瞬态重试与仅由取消终止的限流重试。
                            let retry_detail = if decision.unbounded {
                                format!(
                                    "{} 遇到限流，已安排第 {} 次重试；{} 秒后重试（将持续重试，直到用户终止）。",
                                    provider_label,
                                    decision.retry_number,
                                    delay_seconds,
                                )
                            } else {
                                format!(
                                    "{} 第 {}/{} 次重试将在 {} 秒后开始：{}",
                                    provider_label,
                                    decision.retry_number,
                                    decision.max_retries,
                                    delay_seconds,
                                    decision.reason,
                                )
                            };
                            let retry_event = crate::agent_runtime::events::RuntimeEvent::new(
                                crate::agent_runtime::events::RuntimeEventKind::Retrying,
                                "Provider 自动重试",
                                retry_detail,
                            )
                            .with_metadata(Some(json!({
                                "attempt": decision.retry_number,
                                "max_retries": decision.max_retries,
                                "unbounded": decision.unbounded,
                                "delay_ms": delay_ms,
                                "delay_seconds": delay_seconds,
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

/// Updates retry safety at the boundary of each physical Provider request.
/// 在每次真实 Provider 请求边界更新重试安全状态。
///
/// `unsafe_delta_seen` records whether the current physical request exposed non-replayable output.
/// `unsafe_delta_seen` 记录当前真实请求是否已经暴露不可重放输出。
/// `delta` is the next normalized Provider stream delta.
/// `delta` 是下一条规范化 Provider 流增量。
/// This function mutates the supplied flag and returns no value.
/// 本函数修改传入标记且不返回值。
fn observe_provider_request_retry_safety(
    unsafe_delta_seen: &mut bool,
    delta: &ProviderStreamDelta,
) {
    if matches!(delta, ProviderStreamDelta::RequestSnapshot(_)) {
        *unsafe_delta_seen = false;
        return;
    }
    if provider_delta_makes_retry_unsafe(delta) {
        *unsafe_delta_seen = true;
    }
}

/// Resets consecutive retry accounting when the controller reaches another physical request after one successful request.
/// 当控制器在一次请求成功后进入下一次真实请求时，重置连续重试计数。
///
/// # Parameters
/// 参数
/// `physical_request_seen` records whether the current retry attempt already dispatched one physical request.
/// `physical_request_seen` 记录当前重试尝试是否已经发出过一次真实请求。
/// `completed_retries` stores consecutive failed retries since the latest successful physical request.
/// `completed_retries` 保存最近一次真实请求成功后的连续失败重试次数。
/// `delta` is the current Provider stream delta.
/// `delta` 是当前 Provider 流增量。
///
/// # Returns
/// 返回值
/// This function returns no value and mutates the retry state only at request boundaries.
/// 本函数不返回值，仅在请求边界处修改重试状态。
fn observe_provider_request_retry_reset(
    physical_request_seen: &mut bool,
    completed_retries: &mut u32,
    delta: &ProviderStreamDelta,
) {
    if !matches!(delta, ProviderStreamDelta::RequestSnapshot(_)) {
        return;
    }
    if *physical_request_seen {
        *completed_retries = 0;
        return;
    }
    *physical_request_seen = true;
}

/// Updates the attempt-scoped set of visible tool requests from one normalized provider delta.
/// 根据一条规范化 Provider 增量更新尝试级可见工具请求集合。
///
/// # Parameters
/// 参数
/// `active_request_streams` owns request identifiers that have started but not reached a terminal fragment.
/// `active_request_streams` 持有已开始但尚未到达终态碎片的请求标识。
/// `delta` is the next normalized Provider stream change.
/// `delta` 是下一条规范化 Provider 流变化。
///
/// # Returns
/// 返回值
/// This function mutates the supplied set and returns no value.
/// 此函数修改传入集合，不返回值。
fn observe_active_tool_request_streams(
    active_request_streams: &mut BTreeSet<String>,
    delta: &ProviderStreamDelta,
) {
    let ProviderStreamDelta::ToolRequestFragment(fragment) = delta else {
        return;
    };
    match fragment {
        ToolRequestFragment::Detected {
            request_stream_id, ..
        } => {
            active_request_streams.insert(request_stream_id.clone());
        }
        ToolRequestFragment::Completed {
            request_stream_id, ..
        }
        | ToolRequestFragment::Aborted {
            request_stream_id, ..
        } => {
            active_request_streams.remove(request_stream_id);
        }
        ToolRequestFragment::ArgumentsDelta { .. } => {}
    }
}

/// Emits deterministic abort fragments for every request left active by a completed or failed provider attempt.
/// 为 Provider 尝试完成或失败后仍处于活动状态的每条请求发出确定性中止碎片。
///
/// # Parameters
/// 参数
/// `sender` is the outer stream channel observed by the runtime projection.
/// `sender` 是运行时投影所观察的外层流通道。
/// `active_request_streams` owns the sorted request identifiers requiring cleanup.
/// `active_request_streams` 持有需要清理的已排序请求标识。
/// `reason` is a stable non-content diagnostic reason.
/// `reason` 是不含正文的稳定诊断原因。
///
/// # Returns
/// 返回值
/// This function drains the supplied set and returns no value.
/// 此函数清空传入集合，不返回值。
fn abort_active_tool_request_streams(
    sender: &ProviderLiveDeltaSender,
    active_request_streams: &mut BTreeSet<String>,
    reason: &str,
) {
    for request_stream_id in std::mem::take(active_request_streams) {
        let _ = sender.send(Ok(ProviderStreamDelta::ToolRequestFragment(
            ToolRequestFragment::Aborted {
                request_stream_id,
                reason: reason.to_string(),
            },
        )));
    }
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
    A: VulcanStreamingProviderAdapter<Response = CoreProviderTurnResponse>
        + Clone
        + Send
        + Sync
        + 'static,
    A::Request: ProviderBudgetRequest,
    F: Fn(ProviderLiveDeltaSender, PreparedProviderTransportAdapterInputs) -> A
        + Send
        + Sync
        + 'static,
{
    let provider_label = provider_label.into();
    let PreparedProviderTransportRuntimeInputs {
        entry_inputs:
            PreparedProviderTransportEntryInputs {
                prompt,
                turn_start_mode,
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

    /// Builds one minimal request snapshot used to verify physical-request retry boundaries.
    /// 构建一份用于验证真实请求重试边界的最小请求快照。
    ///
    /// This function accepts no parameters and returns a valid empty request snapshot.
    /// 本函数不接收参数，并返回一份有效的空请求快照。
    fn test_request_snapshot() -> vulcan_core::types::ProviderRequestSnapshot {
        vulcan_core::types::ProviderRequestSnapshot {
            provider_id: String::from("test-provider"),
            model_id: String::from("test-model"),
            system_prompt: String::new(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::default(),
            prompt: String::from("test prompt"),
            history: Vec::new(),
            temperature: None,
            max_turns: 1,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: vulcan_core::types::ProviderContinuation::default(),
        }
    }

    /// Verifies a new physical Provider request clears output emitted by earlier successful requests from the retry-safety decision.
    /// 验证新的真实 Provider 请求会从重试安全判定中清除更早成功请求已经发出的输出。
    #[test]
    fn physical_request_snapshot_resets_retry_safety() {
        // unsafeDeltaSeen models visible output emitted by one earlier successful physical request.
        // unsafeDeltaSeen 模拟更早一次成功真实请求已经发出的可见输出。
        let mut unsafe_delta_seen = false;
        observe_provider_request_retry_safety(
            &mut unsafe_delta_seen,
            &ProviderStreamDelta::Output(String::from("earlier response")),
        );
        assert!(unsafe_delta_seen);

        observe_provider_request_retry_safety(
            &mut unsafe_delta_seen,
            &ProviderStreamDelta::RequestSnapshot(test_request_snapshot()),
        );
        assert!(!unsafe_delta_seen);
    }

    /// Verifies a successful physical request resets retry numbering before the next request starts.
    /// 验证一次真实请求成功后，下一次请求开始前会重置重试序号。
    #[test]
    fn next_physical_request_resets_consecutive_retry_count() {
        // physicalRequestSeen starts false for each outer retry attempt.
        // physicalRequestSeen 在每次外层重试尝试开始时为 false。
        let mut physical_request_seen = false;
        // completedRetries models three consecutive failures before the current attempt succeeds.
        // completedRetries 模拟当前尝试成功前已经发生三次连续失败。
        let mut completed_retries = 3;

        observe_provider_request_retry_reset(
            &mut physical_request_seen,
            &mut completed_retries,
            &ProviderStreamDelta::RequestSnapshot(test_request_snapshot()),
        );
        assert_eq!(completed_retries, 3);

        observe_provider_request_retry_reset(
            &mut physical_request_seen,
            &mut completed_retries,
            &ProviderStreamDelta::RequestSnapshot(test_request_snapshot()),
        );
        assert_eq!(completed_retries, 0);
    }

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
