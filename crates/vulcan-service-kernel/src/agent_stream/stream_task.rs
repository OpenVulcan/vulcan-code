//! Agent runtime stream task entrypoints.
//! Agent 运行时流任务入口。

use super::*;

mod content_delta;
mod context_policy_refresh;
pub(crate) mod conversation_checkpoint;
mod finished;
mod launch_failure;
mod prepared;
mod provider_route_switch;
mod request_phase;
mod request_trace;
mod runtime_event;
mod stream_exit;

use context_policy_refresh::refreshed_execution_request_after_context_compaction_policy_change;
#[cfg(test)]
pub(crate) use conversation_checkpoint::persist_execution_retry_tool_recovery_boundary;
use conversation_checkpoint::{
    handle_conversation_checkpoint_stream_event, retain_prepared_execution_retry_checkpoint,
};
pub(crate) use conversation_checkpoint::{
    invalidate_execution_retry_reference, invalidate_execution_retry_reference_if_matches,
    invalidate_execution_retry_runtime_state,
    mark_execution_retry_snapshot_provider_dispatch_unknown,
    reconcile_interrupted_execution_retry_canonical_state,
    refreeze_claimed_execution_retry_snapshot_before_provider_dispatch,
    refreeze_claimed_execution_retry_snapshot_with_canonical_state,
};
use finished::{FinishedStreamEventOutcome, handle_finished_stream_event};
pub(crate) use launch_failure::StreamTaskLaunchFailureCommit;
pub(crate) use launch_failure::{
    StreamTaskLaunchFailureOutcome, commit_stream_task_registration_failure,
    handle_pre_registration_execution_stream_task_launch_failure,
    handle_pre_registration_stream_task_launch_failure, handle_stream_task_registration_failure,
};
pub(crate) use prepared::PreparedStreamFailureCommit;
pub(crate) use prepared::commit_prepared_stream_failure;
use prepared::{PreparedStreamEventAction, handle_prepared_stream_event};
use provider_route_switch::refreshed_execution_request_after_pending_provider_route_switch;
use request_phase::{RequestPhaseStreamEvent, handle_request_phase_stream_event};
use request_trace::StreamRequestTraceState;
use runtime_event::{handle_runtime_stream_event, runtime_event_hook_policy_for_execution_scope};
pub(crate) use stream_exit::StreamTaskExitOutcome;
pub(crate) use stream_exit::{StreamTaskExitReason, handle_stream_task_exit};

/// Returns the original checkpoint failure for one rejected Provider durability boundary.
/// 为一个已拒绝的 Provider 可靠边界返回原始 checkpoint 失败。
///
/// # Parameters
/// 参数
/// `retained_error` owns the request-local failure; `event_kind` identifies whether the rejected
/// attempt has reached its terminal boundary.
/// `retained_error` 持有请求局部失败；`event_kind` 标识被拒绝尝试是否已经到达终态边界。
///
/// # Returns
/// 返回值
/// The cloned original error while present; a terminal boundary also releases retained ownership.
/// 存在时返回克隆的原始错误；终态边界还会释放已保留所有权。
fn rejected_provider_boundary_error(
    retained_error: &mut Option<String>,
    event_kind: ProviderRequestTelemetryEventKind,
) -> Option<String> {
    // error is cloned before terminal release so Started and Completed receive the same diagnosis.
    // error 在终态释放前克隆，确保 Started 与 Completed 收到同一个诊断。
    let error = retained_error.as_ref().cloned()?;
    if event_kind == ProviderRequestTelemetryEventKind::Completed {
        *retained_error = None;
    }
    Some(error)
}

#[cfg(test)]
mod rejected_provider_boundary_tests {
    use super::*;

    /// Verifies a rejected Started boundary retains its original error through terminal cleanup.
    /// 验证被拒绝的 Started 边界会把原始错误保留到终态清理。
    #[test]
    fn checkpoint_error_survives_started_and_closes_on_completed() {
        // retainedError models one retry-checkpoint persistence failure before Provider dispatch.
        // retainedError 模拟 Provider 分派前的一次重试 checkpoint 持久化失败。
        let mut retained_error = Some(String::from("exact checkpoint failure"));

        assert_eq!(
            rejected_provider_boundary_error(
                &mut retained_error,
                ProviderRequestTelemetryEventKind::Started,
            )
            .as_deref(),
            Some("exact checkpoint failure")
        );
        assert_eq!(retained_error.as_deref(), Some("exact checkpoint failure"));
        assert_eq!(
            rejected_provider_boundary_error(
                &mut retained_error,
                ProviderRequestTelemetryEventKind::Completed,
            )
            .as_deref(),
            Some("exact checkpoint failure")
        );
        assert!(retained_error.is_none());
    }

    /// Verifies an unreleased Started gate remains an exact pre-dispatch retry boundary.
    /// 验证尚未释放的 Started 闸门保持为精确的分派前重试边界。
    #[test]
    fn unreleased_started_gate_proves_provider_was_not_dispatched() {
        // taskId is irrelevant when the stronger pre-dispatch proof is present.
        // 存在更强的分派前证明时，taskId 不参与证据选择。
        let task_id = Uuid::new_v4();

        assert!(matches!(
            failed_execution_stream_retry_evidence(true, task_id, None),
            FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch
        ));
    }

    /// Verifies a released Started gate falls back to the exact durable physical-call binding.
    /// 验证已释放的 Started 闸门会回退到精确的可靠物理调用绑定。
    #[test]
    fn released_started_gate_uses_exact_provider_call_binding() {
        // taskId and providerCallId form the only accepted runtime-bound identity pair.
        // taskId 与 providerCallId 构成唯一可接受的运行时绑定身份对。
        let task_id = Uuid::new_v4();
        let provider_call_id = String::from("provider-call-1");

        assert!(matches!(
            failed_execution_stream_retry_evidence(false, task_id, Some(&provider_call_id)),
            FailedExecutionStreamRetryEvidence::RuntimeBoundary(Some((bound_task_id, bound_provider_call_id)))
                if bound_task_id == task_id && bound_provider_call_id == provider_call_id
        ));
    }
}

/// Materializes the retained terminal boundary for an owned stream exit without `Finished`.
/// 为未产生 `Finished` 的自有流退出物化保留终态边界。
///
/// # Parameters
/// 参数
/// `task_state` owns the retained session, and `session_id` selects the exact early-exit owner.
/// `task_state` 持有保留会话，`session_id` 选择精确的提前退出所有者。
///
/// # Returns
/// 返回值
/// Success after the failed terminal continuation ledger is committed in authoritative memory.
/// 失败终态 Continue 账本写入权威内存后返回成功。
pub(crate) fn materialize_early_stream_terminal_snapshot(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    // terminalAt is shared by the continuation ledger and retained runtime update.
    // terminalAt 由 Continue 账本与保留运行态更新时间共同使用。
    let terminal_at = current_unix_millis_text();
    let mut guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // workspace is the authoritative state whose exact revision will be persisted after this lock is released.
    // workspace 是当前权威状态；释放锁后会持久化其精确修订。
    let workspace = guard
        .workspace_snapshot
        .as_mut()
        .ok_or_else(|| String::from("Early stream terminal workspace state is missing."))?;
    {
        // runtimeState receives a real terminal ledger boundary instead of remaining as an ownerless claim.
        // runtimeState 接收真实终态账本边界，避免继续停留在无所有者的领取状态。
        let runtime_state = workspace
            .session_runtime_state
            .get_mut(&session_id)
            .ok_or_else(|| String::from("Early stream terminal runtime state is missing."))?;
        refresh_terminal_continuation_state(runtime_state, false, terminal_at.as_str())?;
        runtime_state.updated_at = terminal_at;
    }
    Ok(())
}

/// Describes the exact dispatch evidence owned by a terminal stream failure cleanup.
/// 描述终态流失败清理所拥有的精确分派证据。
#[derive(Clone, Debug)]
enum FailedExecutionStreamRetryEvidence {
    /// The failure boundary precedes Provider dispatch, so the failed attempt provably dispatched nothing.
    /// 失败边界位于 Provider 分派之前，因此能够证明失败尝试没有分派任何内容。
    ProvenNotDispatchedBeforeProviderDispatch,
    /// The stream ran and supplies its latest exact stream-to-provider-call binding when available.
    /// 流已经运行，并在可用时提供最近一次精确的流到 Provider 调用绑定。
    RuntimeBoundary(Option<(Uuid, String)>),
}

/// Selects terminal retry evidence from the exact pre-dispatch gate state.
/// 根据精确的分派前闸门状态选择终态重试证据。
///
/// # Parameters
/// 参数
/// `provider_dispatch_proven_absent` is true only while the current request remains before a successful durable Started release.
/// `provider_dispatch_proven_absent` 仅在当前请求尚未成功释放可靠 Started 闸门时为 true。
/// `active_stream_task_id` and `active_provider_call_id` identify the latest durably started physical request otherwise.
/// `active_stream_task_id` 与 `active_provider_call_id` 在其他情况下标识最近一次可靠启动的物理请求。
///
/// # Returns
/// 返回值
/// Exact pre-dispatch proof or the strongest available runtime Provider-call binding.
/// 返回精确的分派前证明，或当前可用的最强运行时 Provider 调用绑定。
fn failed_execution_stream_retry_evidence(
    provider_dispatch_proven_absent: bool,
    active_stream_task_id: Uuid,
    active_provider_call_id: Option<&String>,
) -> FailedExecutionStreamRetryEvidence {
    if provider_dispatch_proven_absent {
        FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch
    } else {
        FailedExecutionStreamRetryEvidence::RuntimeBoundary(
            active_provider_call_id
                .map(|provider_call_id| (active_stream_task_id, provider_call_id.clone())),
        )
    }
}

/// Reads the exact retained execution-retry reference for one session.
/// 读取一个会话精确保留的执行重试引用。
///
/// # Parameters
/// 参数
/// `task_state` owns the authoritative workspace and `session_id` selects the target runtime state.
/// `task_state` 持有权威工作区，`session_id` 选择目标运行态。
///
/// # Returns
/// 返回值
/// The current immutable reference, `None` when no retry exists, or an explicit state-lock failure.
/// 返回当前不可变引用；不存在重试时返回 `None`；状态锁失败时返回明确错误。
fn retained_execution_retry_reference(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<vulcan_workbench_model::ExecutionRetrySnapshotRef>, String> {
    // guard protects the authoritative workspace and retry reference read.
    // guard 保护权威工作区与重试引用读取。
    let guard = task_state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok(guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
        .and_then(|runtime_state| runtime_state.execution_retry_snapshot.clone()))
}

/// Completes retry refreeze, Continue projection, and durable persistence after an owned terminal stream failure.
/// 在自有终态流失败后完成重试重冻、Continue 投影与可靠持久化。
///
/// # Parameters
/// 参数
/// `app` emits the authoritative continuation projection and `task_state` owns the exact failed session state.
/// `app` 发送权威 Continue 投影，`task_state` 持有精确失败会话状态。
/// `controller_runtime` owns immutable retry storage and session persistence, while `session_id` selects the failed launch.
/// `controller_runtime` 持有不可变重试存储与会话持久化能力，`session_id` 选择启动失败的会话。
/// `terminal_persistence_owner` binds cleanup to the exact marker installed before active ownership was released.
/// `terminal_persistence_owner` 将清理绑定到释放活动所有权前安装的精确标记。
/// `evidence` fixes the exact dispatch boundary and `message` is retained when a new interruption error is required.
/// `evidence` 固定精确分派边界，`message` 在需要创建新中断错误时保留。
///
/// # Returns
/// 返回值
/// This asynchronous cleanup reports failures through service diagnostics and always leaves the in-process token fail-closed when its exact owner remains current.
/// 此异步清理通过服务诊断报告失败，并在精确所有者仍为当前所有者时始终使进程内令牌保持失败关闭。
async fn finalize_failed_execution_stream_retry_lifecycle(
    app: ServiceEventEmitter,
    task_state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    session_id: SessionId,
    terminal_persistence_owner: SessionTerminalPersistenceOwner,
    evidence: FailedExecutionStreamRetryEvidence,
    message: String,
) {
    // terminal_persistence_guard keeps Continue, prompts, Stop, and kernel turns closed through every early return.
    // terminal_persistence_guard 在所有提前返回路径中持续阻塞 Continue、提示词、Stop 与 kernel 回合。
    let terminal_persistence_guard = match SessionTerminalPersistenceOwnerGuard::adopt_registered(
        task_state.clone(),
        session_id,
        terminal_persistence_owner,
    ) {
        Ok(guard) => guard,
        Err(error) => {
            eprintln!(
                "[webui-service] terminal stream failure persistence ownership unavailable: session={session_id} owner={terminal_persistence_owner:?} error={error}"
            );
            return;
        }
    };
    // Active provider drafts belong to the failed physical request and must not survive its same-request retry.
    // 活跃 Provider 草稿属于失败的物理请求，不得在同请求重试后继续保留。
    if let Err(error) = discard_active_stream_content_records(&app, &task_state, session_id) {
        eprintln!(
            "[webui-service] terminal stream failure draft cleanup failed: session={session_id} error={error}"
        );
    }
    // expectedReference binds every compensation step to the retry identity owned by the failed launch.
    // expectedReference 将每个补偿步骤绑定到启动失败所拥有的重试身份。
    let expected_reference = match retained_execution_retry_reference(&task_state, session_id) {
        Ok(reference) => reference,
        Err(error) => {
            eprintln!(
                "[webui-service] terminal stream failure initial retry reference read failed: session={session_id} error={error}"
            );
            None
        }
    };
    // refreezeResult applies only the dispatch evidence proven by the exact failure boundary.
    // refreezeResult 仅应用精确失败边界已经证明的分派证据。
    let refreeze_result = match evidence {
        FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch => {
            refreeze_claimed_execution_retry_snapshot_before_provider_dispatch(
                &task_state,
                &controller_runtime,
                session_id,
                &message,
            )
        }
        FailedExecutionStreamRetryEvidence::RuntimeBoundary(provider_call_binding) => {
            conversation_checkpoint::refreeze_claimed_execution_retry_snapshot(
                &task_state,
                &controller_runtime,
                session_id,
                provider_call_binding,
                &message,
            )
        }
    };
    if let Err(error) = refreeze_result {
        eprintln!(
            "[webui-service] terminal stream failure retry refreeze failed: session={session_id} error={error}"
        );
        if let Some(expected_reference) = expected_reference.as_ref()
            && let Err(invalidation_error) = invalidate_execution_retry_reference_if_matches(
                &task_state,
                session_id,
                expected_reference,
                error.blocked_reason(),
                error.error_summary(),
            )
        {
            eprintln!(
                "[webui-service] terminal stream failure exact retry invalidation failed: session={session_id} error={invalidation_error}"
            );
        }
    }
    // settledReference is retained only when it is the unclaimed terminal successor of the failed launch identity.
    // settledReference 仅在其为启动失败身份的未领取终态后继时保留。
    let settled_reference = match retained_execution_retry_reference(&task_state, session_id) {
        Ok(Some(current)) => expected_reference.as_ref().and_then(|expected| {
            (current.snapshot_id == expected.snapshot_id
                && current.generation == expected.generation
                && current.logical_execution_id == expected.logical_execution_id
                && current.claimed_attempt_id.is_none()
                && matches!(
                    current.state,
                    vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen
                        | vulcan_workbench_model::ExecutionRetrySnapshotState::Invalidated
                ))
            .then_some(current)
        }),
        Ok(None) => None,
        Err(error) => {
            eprintln!(
                "[webui-service] terminal stream failure settled retry reference read failed: session={session_id} error={error}"
            );
            None
        }
    };
    if let Err(error) =
        emit_conversation_continuation_summary_changed(&app, &task_state, session_id)
    {
        eprintln!(
            "[webui-service] terminal stream failure continuation projection failed: session={session_id} error={error}"
        );
    }
    // terminal_state_persisted is the only condition that may reopen admission after this failed launch.
    // terminal_state_persisted 是当前失败启动后允许重新开放准入的唯一条件。
    let mut terminal_state_persisted = false;
    match persist_latest_retained_workspace_session_update(
        &task_state,
        controller_runtime.clone(),
        session_id,
    )
    .await
    {
        Ok(()) => terminal_state_persisted = true,
        Err(error) => {
            eprintln!(
                "[webui-service] terminal stream failure persistence failed: session={session_id} error={error}"
            );
            // Exact invalidation cannot damage a replacement Continue claim or a newer user turn.
            // 精确失效不会损坏替代 Continue 领取或更新的用户回合。
            if let Some(settled_reference) = settled_reference.as_ref()
                    && invalidate_execution_retry_reference_if_matches(
                        &task_state,
                        session_id,
                        settled_reference,
                        "retry_snapshot_transition_failed",
                        "The terminal stream failure boundary could not be persisted and was invalidated.",
                    )
                    .unwrap_or(false)
                {
                    let _ = emit_conversation_continuation_summary_changed(
                        &app,
                        &task_state,
                        session_id,
                    );
                    match persist_latest_retained_workspace_session_update(
                            &task_state,
                            controller_runtime.clone(),
                            session_id,
                        )
                        .await
                    {
                            Ok(()) => terminal_state_persisted = true,
                            Err(fallback_error) => eprintln!(
                                "[webui-service] terminal stream failure blocked fallback persistence failed: session={session_id} error={fallback_error}"
                            ),
                    }
                }
        }
    }
    // The guard opens admission only after the latest terminal or fail-closed replacement state is durable.
    // 该守卫仅在最新终态或失败关闭替代状态可靠落盘后开放准入。
    if terminal_state_persisted {
        if let Err(error) = terminal_persistence_guard.release() {
            eprintln!(
                "[webui-service] terminal stream failure persistence owner release failed: session={session_id} error={error}"
            );
        }
    } else {
        // Unreleased drop deliberately retains the in-process admission block after an uncertain write.
        // 未释放即丢弃会在写入结果不确定后有意保留进程内准入阻塞。
        drop(terminal_persistence_guard);
    }
    if let Err(error) =
        emit_conversation_continuation_summary_changed(&app, &task_state, session_id)
    {
        eprintln!(
            "[webui-service] terminal stream failure post-persistence continuation projection failed: session={session_id} error={error}"
        );
    }
}

/// Spawns one background task that forwards agent runtime stream events into the WebUI state.
/// 启动一个后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
pub(crate) fn spawn_agent_stream_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
    expected_kernel_generation: u64,
    expected_kernel_turn_id: String,
) {
    spawn_agent_stream_task_with_auto_compaction_state(
        app,
        state,
        controller_runtime,
        agent_runtime,
        request,
        expected_kernel_generation,
        expected_kernel_turn_id,
        false,
    );
}

/// Spawns one background task that forwards agent runtime stream events with per-request auto-compaction state.
/// 启动一个携带单请求自动压缩状态的后台任务，将 Agent 运行时流事件转发到 WebUI 状态。
///
/// # Parameters
/// # 参数
/// `app` emits workbench snapshots and runtime stream projections.
/// `app` 发出工作台快照与运行时流投影。
/// `state` owns retained service state and active stream handles.
/// `state` 持有保留的服务状态与活跃流句柄。
/// `controller_runtime` persists provider checkpoints and terminal session state.
/// `controller_runtime` 持久化 Provider 检查点与终态会话状态。
/// `agent_runtime` provides provider, tool, hook, and compaction runtimes.
/// `agent_runtime` 提供 Provider、工具、hook 与压缩运行时。
/// `request` is the prepared execution request for the current user turn.
/// `request` 是当前用户回合的待执行请求。
/// `expected_kernel_generation` and `expected_kernel_turn_id` preserve exact ownership across compaction restarts.
/// `expected_kernel_generation` 与 `expected_kernel_turn_id` 跨压缩重启保留精确所有权。
/// `auto_compaction_already_attempted` prevents repeated automatic compaction loops for one user request.
/// `auto_compaction_already_attempted` 防止单个用户请求反复进入自动压缩循环。
pub(crate) fn spawn_agent_stream_task_with_auto_compaction_state(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    request: ExecutionPreparationRequest,
    expected_kernel_generation: u64,
    expected_kernel_turn_id: String,
    auto_compaction_already_attempted: bool,
) {
    let session_id = request.session_id;
    // expectedKernelGenerationOption retains the existing terminalization interface while registration uses the required generation directly.
    // expectedKernelGenerationOption 保留现有终态化接口，同时登记直接使用必需代际。
    let expected_kernel_generation_option = Some(expected_kernel_generation);
    // runtime_event_hook_policy captures the request-authoritative Hook scope before the request moves into the task.
    // runtime_event_hook_policy 在请求移入任务前捕获由请求权威确定的 Hook 范围。
    let runtime_event_hook_policy = runtime_event_hook_policy_for_execution_scope(
        &request.execution_scope,
        request.runtime_policy.hooks_enabled,
        request.conversation_hook_turn_id.as_deref(),
    );
    let runtime_event_hook_policy = match runtime_event_hook_policy {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("[webui-service] runtime event Hook correlation rejected: {error}");
            // cleanup_outcome records how the pre-registration contract failure terminalized the session.
            // cleanup_outcome 记录预注册契约失败如何终态化该会话。
            let cleanup_outcome = handle_pre_registration_execution_stream_task_launch_failure(
                &app,
                &state,
                session_id,
                expected_kernel_generation_option,
            );
            match cleanup_outcome {
                Ok(outcome) => {
                    webui_trace(
                        "stream_hook_correlation_rejected",
                        format!("session={session_id} cleanup={outcome:?} error={error}"),
                    );
                    if outcome == StreamTaskLaunchFailureOutcome::FailedOwnerFinalized {
                        if let Some(kernel_generation) = expected_kernel_generation_option {
                            spawn_service_task(finalize_failed_execution_stream_retry_lifecycle(
                                app.clone(),
                                state.clone(),
                                controller_runtime.clone(),
                                session_id,
                                SessionTerminalPersistenceOwner::KernelGeneration(
                                    kernel_generation,
                                ),
                                FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch,
                                format!(
                                    "The execution stream failed before launch because its Hook correlation was rejected: {error}"
                                ),
                            ));
                        } else {
                            eprintln!(
                                "[webui-service] finalized Hook correlation failure lacks its kernel generation: session={session_id}"
                            );
                        }
                    }
                }
                Err(cleanup_error) => eprintln!(
                    "[webui-service] runtime event Hook correlation cleanup failed: {cleanup_error}; original error: {error}"
                ),
            }
            return;
        }
    };
    let session_goal_id = request
        .session_goal_context
        .as_ref()
        .map(|context| context.goal_id.clone());
    let active_stream_task_id = Uuid::new_v4();
    webui_after_precheck_trace(
        "after_precheck_19_stream_task_scheduled",
        format!("session={session_id}"),
    );
    webui_trace("stream_task_spawn", format!("session={session_id}"));
    let task_state = state.clone();
    let registration_failure_app = app.clone();
    let registration_failure_state = state.clone();
    // registrationFailureControllerRuntime remains available if the gated task never starts and therefore cannot own terminal persistence.
    // registrationFailureControllerRuntime 在闸门任务从未启动、因而无法负责终态持久化时继续可用。
    let registration_failure_controller_runtime = controller_runtime.clone();
    // registrationExpectedKernelTurnId remains outside the moved task so registration validates the same immutable owner.
    // registrationExpectedKernelTurnId 保留在被移动任务之外，使登记校验同一不可变所有者。
    let registration_expected_kernel_turn_id = expected_kernel_turn_id.clone();
    let task_future = async move {
        // request is enriched exactly once with the authorized session selection before tool resolution.
        // request 在工具解析前仅使用已授权会话选择增强一次。
        let mut request = request;
        if request.knowledge_access.is_none() {
            request.knowledge_access =
                match resolve_turn_knowledge_access(&task_state, session_id).await {
                    Ok(access) => access,
                    Err(error) => {
                        eprintln!("[webui-service] knowledge access resolution failed: {error}");
                        None
                    }
                };
        }
        if let Some(access) = request.knowledge_access.as_mut()
            && access.knowledge_search_handler.is_none()
        {
            access.knowledge_search_handler =
                Some(build_knowledge_search_handler(task_state.clone()));
        }
        let stream_task_started_at = current_unix_millis_value();
        webui_after_precheck_trace(
            "after_precheck_20_stream_task_entered",
            format!("session={session_id}"),
        );
        webui_trace("stream_task_entered", format!("session={session_id}"));
        let mut record_ids = SessionStreamingRecordIds::default();
        // providerTelemetryGuard closes an active physical request even when this task future is aborted.
        // providerTelemetryGuard 即使当前任务 Future 被中止，也会关闭活动真实请求。
        let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
            task_state.clone(),
            Some(controller_runtime.clone()),
            Some(app.clone()),
        );
        // executionProviderGoalId preserves goal ownership across tool-result requests after a goal reaches a stopped state.
        // executionProviderGoalId 在目标进入停止状态后，跨工具结果请求保留目标归属。
        let mut execution_provider_goal_id = request
            .session_goal_context
            .as_ref()
            .map(|goal_context| goal_context.goal_id.clone());
        // activeProviderGoalId binds exactly one physical request to its resolved goal owner.
        // activeProviderGoalId 将恰好一次物理请求绑定到其解析后的目标所有者。
        let mut active_provider_goal_id: Option<String> = None;
        // activeProviderCallId retains the latest exact physical call for terminal retry classification.
        // activeProviderCallId 保留最近一个精确物理调用，用于终态重试分类。
        let mut active_provider_call_id: Option<String> = None;
        // active_preparation preserves the exact provider/model budget facts for a mid-turn compaction restart.
        // active_preparation 保留中途回合压缩重启所需的精确 Provider/模型预算事实。
        let mut active_preparation: Option<ExecutionPreparation> = None;
        // preparedRetryCheckpointError blocks the next network-dispatch durability boundary fail closed.
        // preparedRetryCheckpointError 以失败关闭方式阻止下一次网络分派持久化屏障。
        let mut prepared_retry_checkpoint_error: Option<String> = None;
        // providerDispatchProvenAbsent remains true from an exact request checkpoint until durable Started releases network dispatch.
        // providerDispatchProvenAbsent 从精确请求检查点起保持 true，直到可靠 Started 放行网络分派。
        let mut provider_dispatch_proven_absent = false;
        let intervention_started_at = current_unix_millis_value();
        let prompt_intervention = build_webui_prompt_intervention(
            app.clone(),
            task_state.clone(),
            request.context_compaction_window_node_id,
        );
        webui_after_precheck_trace(
            "after_precheck_21_intervention_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(intervention_started_at)
            ),
        );
        let provider_stream_create_started_at = current_unix_millis_value();
        webui_trace(
            "stream_provider_create_start",
            format!("session={session_id}"),
        );
        let mut stream = Box::pin(stream_runtime_run_with_intervention_plugins_and_public_ai(
            agent_runtime.provider_configuration.clone(),
            agent_runtime.agent_profile_catalog.clone(),
            agent_runtime.luaskills_runtime.clone(),
            Some(agent_runtime.vulcan_plugin_runtime.clone()),
            Some(Arc::clone(&agent_runtime.extension_generation)),
            Arc::clone(&agent_runtime.task_runtime),
            Arc::clone(&agent_runtime.question_runtime),
            Arc::clone(&agent_runtime.command_session_runtime),
            Some(Arc::clone(&agent_runtime.public_ai_runtime)),
            request.clone(),
            Some(prompt_intervention),
        ));

        webui_trace(
            "stream_provider_create_ready",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(provider_stream_create_started_at)
            ),
        );
        webui_after_precheck_trace(
            "after_precheck_22_stream_created",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(provider_stream_create_started_at)
            ),
        );
        webui_trace(
            "stream_task_poll_start",
            format!(
                "session={session_id} elapsed_ms={}",
                current_unix_millis_value().saturating_sub(stream_task_started_at)
            ),
        );
        webui_after_precheck_trace(
            "after_precheck_23_poll_start",
            format!(
                "session={session_id} elapsed_ms_since_task={}",
                current_unix_millis_value().saturating_sub(stream_task_started_at)
            ),
        );
        // This request-local state machine keeps latency markers consistent across every stream event kind.
        // 该请求局部状态机用于确保所有流事件类型的延迟标记保持一致。
        let mut request_trace_state = StreamRequestTraceState::new(stream_task_started_at);
        // transientReasoningStatusActive avoids redundant clear events while preserving every exit path.
        // transientReasoningStatusActive 避免冗余清除事件，同时覆盖所有退出路径。
        let mut transient_reasoning_status_active = false;
        // stream_exit_reason makes every polling-loop terminal path explicit without a shadow completion boolean.
        // stream_exit_reason 显式表达每条轮询循环终止路径，不再使用影子完成布尔值。
        let stream_exit_reason = loop {
            // event is the next ordered runtime item owned by this exact stream generation.
            // event 是当前精确流代际拥有的下一条有序运行时事件。
            let Some(event) = stream.next().await else {
                break StreamTaskExitReason::StreamExhausted;
            };
            if !webui_stream_event_matches_kernel_generation(
                &task_state,
                &event,
                expected_kernel_generation_option,
            ) {
                webui_trace(
                    "stream_event_skipped_stale_generation",
                    format!(
                        "session={} expected_generation={}",
                        execution_stream_event_session_id(&event),
                        expected_kernel_generation_option
                            .map(|generation| generation.to_string())
                            .unwrap_or_else(|| String::from("<none>"))
                    ),
                );
                break StreamTaskExitReason::StaleKernelGeneration;
            }
            match event {
                ExecutionStreamEvent::Prepared(preparation) => {
                    active_preparation = Some(preparation.clone());
                    match handle_prepared_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        &request,
                        preparation,
                        session_id,
                        stream_task_started_at,
                        auto_compaction_already_attempted,
                        active_stream_task_id,
                        expected_kernel_generation,
                        &expected_kernel_turn_id,
                    )
                    .await
                    {
                        PreparedStreamEventAction::Continue => {}
                        PreparedStreamEventAction::Restarted
                        | PreparedStreamEventAction::Failed => {
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::ProviderRequestCheckpoint {
                    session_id,
                    logical_request_id,
                    logical_turn_id,
                    provider_attempt_index,
                    provider_family,
                    snapshot,
                    exact_request,
                    controller_config,
                    controller_snapshot,
                } => {
                    // A request checkpoint is emitted before the Started durability gate and therefore proves that this attempt has not dispatched yet.
                    // 请求检查点在 Started 可靠闸门之前发出，因此证明当前 attempt 尚未分派。
                    provider_dispatch_proven_absent = true;
                    let checkpoint_result = require_segmented_session_directory_from_state(
                        &task_state,
                        &controller_runtime.data_root,
                        session_id,
                    )
                    .and_then(|session_directory| {
                        record_webui_provider_request_checkpoint(
                            &session_directory,
                            session_id,
                            &provider_family,
                            &snapshot,
                        )
                    });
                    if let Err(error) = checkpoint_result {
                        eprintln!("[webui-service] provider request checkpoint failed: {error}");
                    }
                    // retryCheckpointExpected distinguishes legacy display-only snapshots from exact pre-dispatch checkpoints.
                    // retryCheckpointExpected 区分旧式仅展示快照与精确分派前检查点。
                    let retry_checkpoint_expected = exact_request.is_some()
                        || controller_config.is_some()
                        || controller_snapshot.is_some();
                    if let (
                        Some(logical_request_id),
                        Some(provider_attempt_index),
                        Some(exact_request),
                        Some(controller_config),
                        Some(controller_snapshot),
                    ) = (
                        logical_request_id,
                        provider_attempt_index,
                        exact_request,
                        controller_config,
                        controller_snapshot,
                    ) {
                        let checkpoint_result = active_preparation
                            .as_ref()
                            .ok_or_else(|| {
                                String::from(
                                    "Execution retry checkpoint arrived before prepared execution state.",
                                )
                            })
                            .and_then(|preparation| {
                                retain_prepared_execution_retry_checkpoint(
                                    &task_state,
                                    session_id,
                                    active_stream_task_id,
                                    logical_request_id,
                                    logical_turn_id,
                                    provider_attempt_index,
                                    preparation,
                                    exact_request,
                                    controller_config,
                                    controller_snapshot,
                                )
                            });
                        if let Err(error) = checkpoint_result {
                            eprintln!(
                                "[webui-service] execution retry checkpoint persistence failed: {error}"
                            );
                            prepared_retry_checkpoint_error = Some(error);
                        }
                    } else if retry_checkpoint_expected {
                        let error = String::from(
                            "Exact Provider request checkpoint is missing its typed logical identity or canonical body.",
                        );
                        eprintln!(
                            "[webui-service] execution retry checkpoint persistence failed: {error}"
                        );
                        prepared_retry_checkpoint_error = Some(error);
                    }
                }
                ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
                    // Legacy non-gated Started telemetry cannot retain pre-dispatch proof because the network is not waiting on a completion handle.
                    // 旧式无闸门 Started 遥测没有完成句柄阻塞网络，因此不能保留分派前证明。
                    if event.kind == ProviderRequestTelemetryEventKind::Started {
                        provider_dispatch_proven_absent = false;
                    }
                    if prepared_retry_checkpoint_error.is_some() {
                        // A rejected Started boundary owns no ledger row, so later non-durable
                        // telemetry from that physical attempt must remain outside the ledger.
                        // 被拒绝的 Started 边界不拥有账本行，因此该物理尝试后续的非可靠遥测
                        // 必须保持在账本之外。
                        continue;
                    }
                    if let Err(error) = handle_stream_provider_request_telemetry(
                        &task_state,
                        &controller_runtime,
                        &app,
                        &mut provider_telemetry_guard,
                        event,
                        &mut active_provider_goal_id,
                        &mut execution_provider_goal_id,
                        &mut active_provider_call_id,
                        active_stream_task_id,
                        None,
                    )
                    .await
                    {
                        eprintln!(
                            "[webui-service] provider usage telemetry state update failed: {error}"
                        );
                    }
                }
                ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
                    // eventKind is retained across the accounting call so Started can update the exact dispatch gate state.
                    // eventKind 跨记账调用保留，使 Started 能更新精确的分派闸门状态。
                    let event_kind = event.kind;
                    if let Some(error) = rejected_provider_boundary_error(
                        &mut prepared_retry_checkpoint_error,
                        event_kind,
                    ) {
                        let _ = completion.complete(Err(error));
                        continue;
                    }
                    let boundary_result = handle_stream_provider_request_telemetry(
                        &task_state,
                        &controller_runtime,
                        &app,
                        &mut provider_telemetry_guard,
                        event,
                        &mut active_provider_goal_id,
                        &mut execution_provider_goal_id,
                        &mut active_provider_call_id,
                        active_stream_task_id,
                        Some(completion),
                    )
                    .await;
                    match boundary_result {
                        Ok(()) => {
                            if event_kind == ProviderRequestTelemetryEventKind::Started {
                                // Returning success releases the Provider runtime, after which dispatch may begin immediately.
                                // 返回成功会释放 Provider 运行时，此后网络分派可能立即开始。
                                provider_dispatch_proven_absent = false;
                            }
                        }
                        Err(error) => {
                            if event_kind == ProviderRequestTelemetryEventKind::Started {
                                // Failed durable Started completion proves the Provider runtime never received permission to dispatch.
                                // 可靠 Started 完成失败证明 Provider 运行时从未获得分派许可。
                                prepared_retry_checkpoint_error = Some(error.clone());
                            }
                            eprintln!(
                                "[webui-service] durable provider usage boundary failed: {error}"
                            );
                        }
                    }
                }
                ExecutionStreamEvent::ProviderLoweringRepair { session_id, event } => {
                    let repair_result = require_segmented_session_directory_from_state(
                        &task_state,
                        &controller_runtime.data_root,
                        session_id,
                    )
                    .and_then(|session_directory| {
                        record_webui_provider_lowering_repair(&session_directory, session_id, event)
                    });
                    if let Err(error) = repair_result {
                        eprintln!("[webui-service] provider lowering repair failed: {error}");
                    }
                }
                ExecutionStreamEvent::RequestReady { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::RequestReady,
                    );
                }
                ExecutionStreamEvent::WireRequestReady { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::WireRequestReady,
                    );
                }
                ExecutionStreamEvent::RequestStarted { session_id } => {
                    handle_request_phase_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        session_id,
                        active_stream_task_id,
                        stream_task_started_at,
                        RequestPhaseStreamEvent::RequestStarted,
                    );
                }
                ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                    handle_content_delta_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        &mut record_ids,
                        session_id,
                        StreamContentKind::Thinking,
                        delta,
                    );
                }
                ExecutionStreamEvent::TransientReasoningStatusChanged { session_id, status } => {
                    transient_reasoning_status_active = status.is_some();
                    if let Err(error) =
                        emit_transient_reasoning_status(&app, &task_state, session_id, status)
                    {
                        eprintln!(
                            "[webui-service] transient reasoning status emit failed: {error}"
                        );
                    }
                }
                ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                    if let Err(error) = complete_and_persist_streaming_thinking_phase(
                        &app,
                        &task_state,
                        &controller_runtime,
                        session_id,
                        &mut record_ids,
                    ) {
                        eprintln!("[webui-service] complete thinking phase failed: {error}");
                    }
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    handle_content_delta_stream_event(
                        &app,
                        &task_state,
                        &mut request_trace_state,
                        &mut record_ids,
                        session_id,
                        StreamContentKind::Output,
                        delta,
                    );
                }
                ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                    if event.kind
                        == vulcan_agent_runtime::agent_runtime::events::RuntimeEventKind::ToolRequestStarted
                    {
                        // The first tool-request event is the service boundary where tool-only Provider output becomes replay-relevant even without a visible token.
                        // 首条工具请求事件是纯工具 Provider 输出即使没有可见 Token 也开始影响重放的服务边界。
                        provider_telemetry_guard.observe_semantic_output();
                    }
                    if let Some(boundary) = event.tool_execution_boundary.as_ref() {
                        // expectedToolRetryReference binds both persistence and failure invalidation to the same immutable owner.
                        // expectedToolRetryReference 将持久化与失败失效都绑定到同一个不可变所有者。
                        let expected_tool_retry_reference =
                            conversation_checkpoint::execution_retry_reference_for_tool_boundary(
                                &task_state,
                                session_id,
                            );
                        // toolBoundaryPersistence cannot apply a late event to a concurrently replaced stop/restart snapshot.
                        // toolBoundaryPersistence 不允许较晚事件作用到并发替换后的停止或重启快照。
                        let tool_boundary_persistence =
                            match expected_tool_retry_reference.as_ref() {
                                Ok(expected_reference) => {
                                    conversation_checkpoint::persist_execution_retry_tool_recovery_boundary(
                                        &task_state,
                                        &controller_runtime,
                                        session_id,
                                        expected_reference,
                                        boundary,
                                    )
                                    .await
                                }
                                Err(error) => Err(error.clone()),
                            };
                        if let Err(error) = tool_boundary_persistence {
                            eprintln!(
                                "[webui-service] persist exact tool recovery boundary failed: {error}"
                            );
                            // blockedReason preserves the exact structural reason for the localized disabled Continue tooltip.
                            // blockedReason 为本地化的禁用 Continue 提示保留精确结构原因。
                            let blocked_reason =
                                conversation_checkpoint::tool_recovery_boundary_blocked_reason(
                                    &error,
                                );
                            // errorSummary retains a controlled audit message without exposing internal diagnostics to the user.
                            // errorSummary 保留受控审计说明，同时不向用户暴露内部诊断细节。
                            let error_summary =
                                conversation_checkpoint::tool_recovery_boundary_error_summary(
                                    blocked_reason,
                                );
                            // barrierRejected wakes the exact waiting executor with failure, so Core cannot advance after a durability error.
                            // barrierRejected 以失败唤醒精确等待中的执行器，确保可靠性错误后 Core 无法继续推进。
                            if let Err(rejection_error) = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event(&event) {
                            eprintln!(
                                "[webui-service] reject tool recovery durability barrier failed: {rejection_error}"
                            );
                        }
                            if let Ok(expected_reference) = expected_tool_retry_reference.as_ref() {
                                match invalidate_execution_retry_reference_if_matches(
                                    &task_state,
                                    session_id,
                                    expected_reference,
                                    blocked_reason,
                                    error_summary,
                                ) {
                                    Ok(_) => {}
                                    Err(invalidation_error) => {
                                        eprintln!(
                                            "[webui-service] invalidate failed tool recovery boundary failed: {invalidation_error}"
                                        );
                                    }
                                }
                            }
                            if let Err(summary_error) =
                                emit_conversation_continuation_summary_changed(
                                    &app,
                                    &task_state,
                                    session_id,
                                )
                            {
                                eprintln!(
                                    "[webui-service] emit failed tool recovery boundary summary failed: {summary_error}"
                                );
                            }
                            continue;
                        }
                    }
                    if event.kind
                        == vulcan_agent_runtime::agent_runtime::events::RuntimeEventKind::ToolBatchPrepared
                    {
                        if let Err(error) = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_runtime_event_projection_barrier_from_event(&event) {
                            eprintln!(
                                "[webui-service] acknowledge durable tool batch boundary failed: {error}"
                            );
                        }
                        continue;
                    }
                    handle_runtime_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        &mut request_trace_state,
                        &mut record_ids,
                        &runtime_event_hook_policy,
                        session_id,
                        active_stream_task_id,
                        event,
                    )
                    .await;
                }
                ExecutionStreamEvent::ConversationCheckpoint {
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                    ..
                } => handle_conversation_checkpoint_stream_event(
                    &app,
                    &task_state,
                    &controller_runtime,
                    session_id,
                    conversation_state,
                    last_successful_provider_usage,
                ),
                ExecutionStreamEvent::ContextCompactionPolicyRefreshRequired {
                    session_id,
                    conversation_state,
                    reason,
                    provider_request_continuation,
                } => {
                    match refreshed_execution_request_after_context_compaction_policy_change(
                        &task_state,
                        active_stream_task_id,
                        &request,
                        conversation_state,
                        provider_request_continuation,
                    ) {
                        Ok(updated_request) => {
                            webui_trace(
                                "stream_context_compaction_policy_restarting",
                                format!("session={session_id} reason={reason}"),
                            );
                            let owns_active_stream_entry = task_state
                                .lock()
                                .ok()
                                .and_then(|mut guard| {
                                    remove_active_stream_task_if_id(
                                        &mut guard,
                                        session_id,
                                        active_stream_task_id,
                                    )
                                })
                                .is_some();
                            if !owns_active_stream_entry {
                                return;
                            }
                            spawn_agent_stream_task_with_auto_compaction_state(
                                app.clone(),
                                task_state.clone(),
                                controller_runtime.clone(),
                                agent_runtime.clone(),
                                updated_request,
                                expected_kernel_generation,
                                expected_kernel_turn_id.clone(),
                                false,
                            );
                            return;
                        }
                        Err(error) => {
                            eprintln!(
                                "[webui-service] context compaction policy refresh failed: session={session_id} reason={reason} error={error}"
                            );
                            let failure_commit = commit_prepared_stream_failure(
                                &app,
                                &task_state,
                                session_id,
                                active_stream_task_id,
                                "context compaction policy refresh failed",
                            );
                            if failure_commit == PreparedStreamFailureCommit::Committed {
                                finalize_failed_execution_stream_retry_lifecycle(
                                    app.clone(),
                                    task_state.clone(),
                                    controller_runtime.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch,
                                    format!(
                                        "The updated context compaction policy could not be activated before the next request: {error}"
                                    ),
                                )
                                .await;
                            }
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::ProviderRouteSwitchRequired {
                    session_id,
                    conversation_state,
                    reason,
                    provider_request_continuation,
                } => {
                    match refreshed_execution_request_after_pending_provider_route_switch(
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        active_stream_task_id,
                        &request,
                        conversation_state,
                        provider_request_continuation,
                    )
                    .await
                    {
                        Ok(updated_request) => {
                            webui_trace(
                                "stream_pending_provider_route_restarting",
                                format!("session={session_id} reason={reason}"),
                            );
                            let owns_active_stream_entry = task_state
                                .lock()
                                .ok()
                                .and_then(|mut guard| {
                                    remove_active_stream_task_if_id(
                                        &mut guard,
                                        session_id,
                                        active_stream_task_id,
                                    )
                                })
                                .is_some();
                            if !owns_active_stream_entry {
                                return;
                            }
                            spawn_agent_stream_task_with_auto_compaction_state(
                                app.clone(),
                                task_state.clone(),
                                controller_runtime.clone(),
                                agent_runtime.clone(),
                                updated_request,
                                expected_kernel_generation,
                                expected_kernel_turn_id.clone(),
                                false,
                            );
                            return;
                        }
                        Err(error) => {
                            eprintln!(
                                "[webui-service] pending Provider route activation failed: session={session_id} reason={reason} error={error}"
                            );
                            let failure_commit = commit_prepared_stream_failure(
                                &app,
                                &task_state,
                                session_id,
                                active_stream_task_id,
                                "pending Provider route activation failed",
                            );
                            if failure_commit == PreparedStreamFailureCommit::Committed {
                                finalize_failed_execution_stream_retry_lifecycle(
                                    app.clone(),
                                    task_state.clone(),
                                    controller_runtime.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch,
                                    format!(
                                        "The exact pending Provider route could not be activated before the next request: {error}"
                                    ),
                                )
                                .await;
                            }
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::ContextCompactionRequired {
                    session_id,
                    conversation_state,
                    reason,
                } => {
                    if auto_compaction_already_attempted {
                        eprintln!(
                            "[webui-service] repeated mid-turn context compaction rejected: session={session_id} reason={reason}"
                        );
                        let failure_commit = commit_prepared_stream_failure(
                            &app,
                            &task_state,
                            session_id,
                            active_stream_task_id,
                            "mid-turn context compaction retry limit reached",
                        );
                        if failure_commit == PreparedStreamFailureCommit::Committed {
                            finalize_failed_execution_stream_retry_lifecycle(
                                app.clone(),
                                task_state.clone(),
                                controller_runtime.clone(),
                                session_id,
                                SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id),
                                FailedExecutionStreamRetryEvidence::RuntimeBoundary(
                                    active_provider_call_id.as_ref().map(|provider_call_id| {
                                        (active_stream_task_id, provider_call_id.clone())
                                    }),
                                ),
                                format!(
                                    "Mid-turn context compaction was requested repeatedly: {reason}"
                                ),
                            )
                            .await;
                        }
                        return;
                    }
                    let Some(preparation) = active_preparation.as_ref() else {
                        let failure_commit = commit_prepared_stream_failure(
                            &app,
                            &task_state,
                            session_id,
                            active_stream_task_id,
                            "mid-turn context compaction preparation missing",
                        );
                        if failure_commit == PreparedStreamFailureCommit::Committed {
                            finalize_failed_execution_stream_retry_lifecycle(
                                app.clone(),
                                task_state.clone(),
                                controller_runtime.clone(),
                                session_id,
                                SessionTerminalPersistenceOwner::StreamTask(
                                    active_stream_task_id,
                                ),
                                FailedExecutionStreamRetryEvidence::RuntimeBoundary(
                                    active_provider_call_id.as_ref().map(|provider_call_id| {
                                        (active_stream_task_id, provider_call_id.clone())
                                    }),
                                ),
                                format!(
                                    "Mid-turn context compaction lacked its prepared execution state: {reason}"
                                ),
                            )
                            .await;
                        }
                        return;
                    };
                    let mut compaction_request = request.clone();
                    compaction_request.conversation_state = conversation_state;
                    let compaction_result =
                        force_auto_compact_webui_session_after_provider_boundary(
                            &app,
                            &task_state,
                            &controller_runtime,
                            &agent_runtime,
                            active_stream_task_id,
                            &compaction_request,
                            preparation,
                        )
                        .await;
                    match compaction_result {
                        Ok(Some(updated_request)) => {
                            webui_trace(
                                "stream_mid_turn_compaction_restarting",
                                format!("session={session_id} reason={reason}"),
                            );
                            let owns_active_stream_entry = task_state
                                .lock()
                                .ok()
                                .and_then(|mut guard| {
                                    remove_active_stream_task_if_id(
                                        &mut guard,
                                        session_id,
                                        active_stream_task_id,
                                    )
                                })
                                .is_some();
                            if !owns_active_stream_entry {
                                return;
                            }
                            spawn_agent_stream_task_with_auto_compaction_state(
                                app.clone(),
                                task_state.clone(),
                                controller_runtime.clone(),
                                agent_runtime.clone(),
                                updated_request,
                                expected_kernel_generation,
                                expected_kernel_turn_id.clone(),
                                true,
                            );
                            return;
                        }
                        Ok(None) => {
                            let error = String::from(
                                "mid-turn context compaction was required but produced no refreshed request",
                            );
                            eprintln!(
                                "[webui-service] {error}: session={session_id} reason={reason}"
                            );
                            let failure_commit = commit_prepared_stream_failure(
                                &app,
                                &task_state,
                                session_id,
                                active_stream_task_id,
                                "mid-turn context compaction produced no request",
                            );
                            if failure_commit == PreparedStreamFailureCommit::Committed {
                                finalize_failed_execution_stream_retry_lifecycle(
                                    app.clone(),
                                    task_state.clone(),
                                    controller_runtime.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    FailedExecutionStreamRetryEvidence::RuntimeBoundary(
                                        active_provider_call_id.as_ref().map(|provider_call_id| {
                                            (active_stream_task_id, provider_call_id.clone())
                                        }),
                                    ),
                                    error,
                                )
                                .await;
                            }
                            return;
                        }
                        Err(error) => {
                            eprintln!(
                                "[webui-service] mid-turn context compaction failed: session={session_id} reason={reason} error={error}"
                            );
                            let failure_commit = commit_prepared_stream_failure(
                                &app,
                                &task_state,
                                session_id,
                                active_stream_task_id,
                                "mid-turn context compaction failure",
                            );
                            if failure_commit == PreparedStreamFailureCommit::Committed {
                                finalize_failed_execution_stream_retry_lifecycle(
                                    app.clone(),
                                    task_state.clone(),
                                    controller_runtime.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    FailedExecutionStreamRetryEvidence::RuntimeBoundary(
                                        active_provider_call_id.as_ref().map(|provider_call_id| {
                                            (active_stream_task_id, provider_call_id.clone())
                                        }),
                                    ),
                                    format!("Mid-turn context compaction failed: {error}"),
                                )
                                .await;
                            }
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::Finished(report) => {
                    // finished_session_id keeps the exact owner available before the report moves into terminal handling.
                    // finished_session_id 在报告移入终态处理前保留精确所属会话。
                    let finished_session_id = report.session_id;
                    if report.error.is_some() {
                        // A failed physical request cannot retain partial output that the exact retry will generate again.
                        // 失败的物理请求不能保留精确重试会再次生成的局部输出。
                        match discard_active_stream_content_records(
                            &app,
                            &task_state,
                            finished_session_id,
                        ) {
                            Ok(_) => record_ids = SessionStreamingRecordIds::default(),
                            Err(error) => {
                                eprintln!(
                                    "[webui-service] discard failed content records failed: {error}"
                                );
                                if let Err(seal_error) = seal_stream_content_records(
                                    &app,
                                    &task_state,
                                    finished_session_id,
                                    &mut record_ids,
                                ) {
                                    eprintln!(
                                        "[webui-service] seal failed content records fallback failed: {seal_error}"
                                    );
                                }
                            }
                        }
                    } else if let Err(error) = seal_stream_content_records(
                        &app,
                        &task_state,
                        finished_session_id,
                        &mut record_ids,
                    ) {
                        eprintln!("[webui-service] seal finished content records failed: {error}");
                    }
                    match handle_finished_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        active_stream_task_id,
                        stream_task_started_at,
                        execution_provider_goal_id.as_deref(),
                        failed_execution_stream_retry_evidence(
                            provider_dispatch_proven_absent,
                            active_stream_task_id,
                            active_provider_call_id.as_ref(),
                        ),
                        report,
                    )
                    .await
                    {
                        FinishedStreamEventOutcome::Handled
                        | FinishedStreamEventOutcome::StaleOwner
                        | FinishedStreamEventOutcome::OwnershipUnavailable => {
                            break StreamTaskExitReason::FinishedEventHandled;
                        }
                    }
                }
            }
        };
        if let Some(event) = provider_telemetry_guard.take_cancelled_event()
            && let Err(error) = process_provider_request_telemetry_event(
                &task_state,
                &controller_runtime,
                Some(&app),
                &mut provider_telemetry_guard,
                event,
                "provider_usage_cancelled",
            )
            .await
        {
            eprintln!("[webui-service] provider cancellation usage state update failed: {error}");
        }
        if transient_reasoning_status_active
            && let Err(error) = emit_transient_reasoning_status(&app, &task_state, session_id, None)
        {
            eprintln!("[webui-service] transient reasoning status clear failed: {error}");
        }
        // Remaining identities belong to an incomplete physical request and must not become duplicate visible output after retry.
        // 剩余身份属于未完成的物理请求，重试后不得成为重复的可见输出。
        match discard_active_stream_content_records(&app, &task_state, session_id) {
            Ok(_) => {}
            Err(error) => {
                eprintln!("[webui-service] discard exiting content records failed: {error}");
                if let Err(seal_error) =
                    seal_stream_content_records(&app, &task_state, session_id, &mut record_ids)
                {
                    eprintln!(
                        "[webui-service] seal exiting content records fallback failed: {seal_error}"
                    );
                }
            }
        }
        // stream_exit_outcome records the exact ownership decision made after polling stops.
        // stream_exit_outcome 记录轮询停止后作出的精确所有权决策。
        let stream_exit_outcome = handle_stream_task_exit(
            &app,
            &task_state,
            session_id,
            active_stream_task_id,
            stream_exit_reason,
        );
        if stream_exit_outcome == StreamTaskExitOutcome::EarlyTerminationCleaned {
            // terminal_persistence_guard binds this asynchronous settlement to the exact removed stream task.
            // terminal_persistence_guard 将当前异步结算绑定到已移除的精确流任务。
            let terminal_persistence_guard =
                match SessionTerminalPersistenceOwnerGuard::adopt_registered(
                    task_state.clone(),
                    session_id,
                    SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id),
                ) {
                    Ok(guard) => guard,
                    Err(error) => {
                        eprintln!(
                            "[webui-service] early stream terminal-persistence ownership unavailable: session={session_id} task={active_stream_task_id} error={error}"
                        );
                        let _ = emit_conversation_continuation_summary_changed(
                            &app,
                            &task_state,
                            session_id,
                        );
                        return;
                    }
                };
            // refreezeResult closes the claimed sidecar before a new terminal continuation generation is published.
            // refreezeResult 在发布新的终态 Continue 代际前关闭已领取 sidecar。
            let refreeze_result = match failed_execution_stream_retry_evidence(
                provider_dispatch_proven_absent,
                active_stream_task_id,
                active_provider_call_id.as_ref(),
            ) {
                FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch => {
                    refreeze_claimed_execution_retry_snapshot_before_provider_dispatch(
                        &task_state,
                        &controller_runtime,
                        session_id,
                        "The execution stream ended before an authoritative Finished event.",
                    )
                }
                FailedExecutionStreamRetryEvidence::RuntimeBoundary(provider_call_binding) => {
                    conversation_checkpoint::refreeze_claimed_execution_retry_snapshot(
                        &task_state,
                        &controller_runtime,
                        session_id,
                        provider_call_binding,
                        "The execution stream ended before an authoritative Finished event.",
                    )
                }
            };
            if let Err(error) = refreeze_result {
                eprintln!("[webui-service] refreeze interrupted execution retry failed: {error}");
                // A failed refreeze must invalidate the exact in-memory claim before any later duplicate can classify it as admission-in-flight.
                // 重冻失败时必须先使精确内存领取失效，避免后续重复请求继续被判定为准入进行中。
                if let Err(invalidation_error) = invalidate_execution_retry_reference(
                    &task_state,
                    session_id,
                    error.blocked_reason(),
                    error.error_summary(),
                ) {
                    eprintln!(
                        "[webui-service] interrupted execution retry invalidation failed: {invalidation_error}"
                    );
                }
            }
            // terminalSnapshot records a real failed continuation boundary for both ordinary and resumed executions.
            // terminalSnapshot 为普通执行与恢复执行同时记录真实的失败 Continue 边界。
            let terminal_boundary_materialized = match materialize_early_stream_terminal_snapshot(
                &task_state,
                session_id,
            ) {
                Ok(()) => true,
                Err(error) => {
                    eprintln!(
                        "[webui-service] materialize early stream terminal boundary failed: {error}"
                    );
                    if let Err(invalidation_error) = invalidate_execution_retry_reference(
                        &task_state,
                        session_id,
                        "retry_snapshot_transition_failed",
                        "The early stream terminal boundary could not be materialized and was invalidated.",
                    ) {
                        eprintln!(
                            "[webui-service] early stream terminal invalidation failed: {invalidation_error}"
                        );
                    }
                    true
                }
            };
            // terminal_state_persisted is the only condition that may reopen admission after an early exit.
            // terminal_state_persisted 是提前退出后允许重新开放准入的唯一条件。
            let mut terminal_state_persisted = false;
            if terminal_boundary_materialized {
                match persist_latest_retained_workspace_session_update(
                    &task_state,
                    controller_runtime.clone(),
                    session_id,
                )
                .await
                {
                    Ok(()) => terminal_state_persisted = true,
                    Err(error) => {
                        eprintln!(
                            "[webui-service] persist early stream terminal boundary failed: {error}"
                        );
                        // A continuation whose main-state terminal boundary was not durable must remain blocked in this process.
                        // 主状态终态边界未可靠落盘的 Continue 在当前进程中必须保持阻断。
                        match invalidate_execution_retry_reference(
                            &task_state,
                            session_id,
                            "retry_snapshot_transition_failed",
                            "The early stream terminal boundary could not be persisted and was invalidated.",
                        ) {
                            Ok(()) => {
                                match persist_latest_retained_workspace_session_update(
                                    &task_state,
                                    controller_runtime.clone(),
                                    session_id,
                                )
                                .await
                                {
                                    Ok(()) => terminal_state_persisted = true,
                                    Err(fallback_error) => eprintln!(
                                        "[webui-service] persist blocked early stream fallback failed: {fallback_error}"
                                    ),
                                }
                            }
                            Err(invalidation_error) => eprintln!(
                                "[webui-service] early stream persistence invalidation failed: {invalidation_error}"
                            ),
                        }
                    }
                }
            }
            // Admission opens only after the latest terminal or fail-closed replacement revision is durable.
            // 仅在最新终态或失败关闭替代修订可靠落盘后开放准入。
            if terminal_state_persisted {
                if let Err(error) = terminal_persistence_guard.release() {
                    eprintln!(
                        "[webui-service] early stream terminal-persistence owner release failed: session={session_id} error={error}"
                    );
                }
            } else {
                // Unreleased drop deliberately retains the in-process admission block after an uncertain write.
                // 未释放即丢弃会在写入结果不确定后有意保留进程内准入阻塞。
                drop(terminal_persistence_guard);
            }
            if let Err(error) =
                emit_conversation_continuation_summary_changed(&app, &task_state, session_id)
            {
                eprintln!("[webui-service] emit early stream continuation summary failed: {error}");
            }
        }
        webui_trace(
            "stream_task_exit_handled",
            format!(
                "session={session_id} reason={stream_exit_reason:?} outcome={stream_exit_outcome:?}"
            ),
        );
    };
    if let Err(error) = spawn_registered_kernel_stream_task_with_mode(
        &state,
        session_id,
        active_stream_task_id,
        session_goal_id,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        expected_kernel_generation,
        &registration_expected_kernel_turn_id,
        task_future,
    ) {
        eprintln!("[webui-service] register stream task failed: {error}");
        let cleanup_outcome = handle_stream_task_registration_failure(
            &registration_failure_app,
            &registration_failure_state,
            session_id,
            expected_kernel_generation_option,
            &error,
        );
        match cleanup_outcome {
            Ok(outcome) => {
                webui_trace(
                    "stream_task_registration_failure_handled",
                    format!("session={session_id} outcome={outcome:?}"),
                );
                if outcome == StreamTaskLaunchFailureOutcome::FailedOwnerFinalized {
                    if let Some(kernel_generation) = expected_kernel_generation_option {
                        spawn_service_task(finalize_failed_execution_stream_retry_lifecycle(
                            registration_failure_app,
                            registration_failure_state,
                            registration_failure_controller_runtime,
                            session_id,
                            SessionTerminalPersistenceOwner::KernelGeneration(kernel_generation),
                            FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch,
                            format!(
                                "The execution stream failed before its registered start gate opened: {error}"
                            ),
                        ));
                    } else {
                        eprintln!(
                            "[webui-service] finalized registration failure lacks its kernel generation: session={session_id}"
                        );
                    }
                }
            }
            Err(cleanup_error) => eprintln!(
                "[webui-service] register stream task failure projection failed: {cleanup_error}"
            ),
        }
    }
}

/// Applies one physical Provider telemetry event and releases its optional durable barrier.
/// 应用一条物理 Provider 遥测事件并释放其可选可靠屏障。
///
/// # Parameters
/// 参数
/// The arguments retain exact session Goal ownership, telemetry order, and durable completion state.
/// 参数保留精确的会话目标归属、遥测顺序与可靠完成状态。
///
/// # Returns
/// 返回值
/// Success only when authoritative session accounting and persistence succeed.
/// 仅在权威会话记账与持久化成功时返回成功。
#[allow(clippy::too_many_arguments)]
async fn handle_stream_provider_request_telemetry(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    app: &ServiceEventEmitter,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: ProviderRequestTelemetryEvent,
    active_provider_goal_id: &mut Option<String>,
    execution_provider_goal_id: &mut Option<String>,
    active_provider_call_id: &mut Option<String>,
    active_stream_task_id: Uuid,
    completion: Option<ProviderRequestDurabilityCompletion>,
) -> Result<(), String> {
    // eventKind and payload facts survive ownership transfer into durable accounting.
    // eventKind 与载荷事实会在事件所有权移入可靠记账后继续使用。
    let event_kind = event.kind;
    let event_session_id = event.session_id;
    let started_provider_call_id = if event_kind == ProviderRequestTelemetryEventKind::Started {
        event.provider_call_id().map(str::to_string).map(Some)
    } else {
        Ok(None)
    };
    let completed_usage = event.completed_usage.clone();
    // accountingResult owns authoritative ledger mutation and its durable boundary.
    // accountingResult 持有权威账本变更及其可靠边界。
    let accounting_result = process_provider_request_telemetry_event(
        state,
        controller_runtime,
        Some(app),
        provider_telemetry_guard,
        event,
        "provider_usage_telemetry",
    )
    .await;
    // providerCallBindingResult binds Started identity only after its authoritative ledger boundary is durable.
    // providerCallBindingResult 仅在 Started 权威账本边界可靠持久化后绑定调用身份。
    let provider_call_binding_result = if accounting_result.is_ok() {
        started_provider_call_id.and_then(|provider_call_id| {
            let Some(provider_call_id) = provider_call_id else {
                return Ok(());
            };
            // The stream-local identity is retained after durability even if the registry ownership check fails closed.
            // 即使注册表所有权校验失败关闭，流局部身份也会在可靠持久化后保留。
            *active_provider_call_id = Some(provider_call_id.clone());
            let mut guard = state
                .lock()
                .map_err(|error| format!("failed to lock service state: {error}"))?;
            if !bind_active_stream_task_provider_call_if_id(
                &mut guard,
                event_session_id,
                active_stream_task_id,
                provider_call_id,
            ) {
                return Err(String::from(
                    "Provider Started boundary lost exact active-stream ownership.",
                ));
            }
            Ok(())
        })
    } else {
        Ok(())
    };
    // Durable owner-side lifecycle state must never get ahead of the authoritative Provider ledger.
    // 所有者侧生命周期状态绝不能领先于权威 Provider 账本。
    if accounting_result.is_ok() && provider_call_binding_result.is_ok() {
        match event_kind {
            ProviderRequestTelemetryEventKind::Started => {
                *active_provider_goal_id = match session_goal_id_for_provider_request(
                    state,
                    event_session_id,
                    execution_provider_goal_id.as_deref(),
                ) {
                    Ok(goal_id) => {
                        if let Some(goal_id) = goal_id.as_ref() {
                            *execution_provider_goal_id = Some(goal_id.clone());
                            // The retained task binding lets an immediate user stop pause a Goal created during this execution.
                            // 保留任务绑定使用户立即停止时也能暂停当前执行期间创建的目标。
                            match state.lock() {
                                Ok(mut guard) => {
                                    let _ = bind_active_stream_task_goal_if_id(
                                        &mut guard,
                                        event_session_id,
                                        active_stream_task_id,
                                        goal_id.clone(),
                                    );
                                }
                                Err(error) => {
                                    eprintln!(
                                        "[webui-service] provider request goal task binding failed: {error}"
                                    );
                                }
                            }
                        }
                        goal_id
                    }
                    Err(error) => {
                        eprintln!("[webui-service] provider request goal binding failed: {error}");
                        None
                    }
                };
            }
            ProviderRequestTelemetryEventKind::Completed => {
                // completedGoalId is consumed exactly once with the durably retained physical request terminal event.
                // completedGoalId 随可靠保留的物理请求终态事件恰好消费一次。
                let completed_goal_id = active_provider_goal_id.take();
                if let (Some(goal_id), Some(usage)) =
                    (completed_goal_id.as_deref(), completed_usage.as_ref())
                    && let Err(error) = account_session_goal_provider_usage(
                        state,
                        controller_runtime,
                        event_session_id,
                        goal_id,
                        usage,
                    )
                    .await
                {
                    eprintln!("[webui-service] provider goal usage accounting failed: {error}");
                }
            }
            ProviderRequestTelemetryEventKind::Dispatched
            | ProviderRequestTelemetryEventKind::FirstToken
            | ProviderRequestTelemetryEventKind::Retrying => {}
        }
    }
    // result keeps ownership binding failures fail-closed without hiding a simultaneous accounting failure.
    // result 以失败关闭方式保留所有权绑定失败，同时不隐藏并发发生的记账失败。
    let result = match (provider_call_binding_result, accounting_result) {
        (Ok(()), result) => result,
        (Err(binding_error), Ok(())) => Err(binding_error),
        (Err(binding_error), Err(accounting_error)) => Err(format!(
            "{binding_error}; Provider telemetry accounting also failed: {accounting_error}"
        )),
    };
    if let Some(completion) = completion {
        let _ = completion.complete(result.clone());
    }
    result
}

/// Resolves the non-widening knowledge access snapshot for one provider turn.
/// 为一个 Provider 回合解析不可扩大的知识库访问快照。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace, runtime state, and retained knowledge runtime.
/// `state` 持有当前工作区、运行态与保留的知识库运行时。
/// `session_id` identifies the conversation whose selection must be applied.
/// `session_id` 标识必须应用其选择的会话。
///
/// # Returns
/// 返回值
/// A project-bound access snapshot, possibly with an empty library list, or `None` without a project.
/// 绑定项目且知识库列表可能为空的访问快照；无所属项目时返回 `None`。
async fn resolve_turn_knowledge_access(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<Option<ExecutionKnowledgeAccessContext>, String> {
    let (runtime, project_id, selection) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let Some(runtime) = guard.knowledge_runtime.clone() else {
            return Ok(None);
        };
        let Some(workspace) = guard.workspace_snapshot.as_ref() else {
            return Ok(None);
        };
        let Some(project_id) = owning_project_id_for_session(workspace, session_id) else {
            return Ok(None);
        };
        let selection = workspace
            .session_runtime_state
            .get(&session_id)
            .map(|runtime_state| runtime_state.knowledge_library_selection.clone())
            .unwrap_or_default();
        (runtime, project_id, selection)
    };
    let mut library_ids = match selection.mode {
        vulcan_workbench_model::SessionKnowledgeLibrarySelectionMode::Inherit => {
            runtime.authorized_library_ids(&project_id, true).await?
        }
        vulcan_workbench_model::SessionKnowledgeLibrarySelectionMode::Explicit => {
            let authorized: BTreeSet<String> = runtime
                .authorized_library_ids(&project_id, false)
                .await?
                .into_iter()
                .collect();
            selection
                .library_ids
                .into_iter()
                .filter(|library_id| authorized.contains(library_id))
                .collect()
        }
    };
    library_ids.sort();
    library_ids.dedup();
    Ok(Some(ExecutionKnowledgeAccessContext {
        project_id,
        library_ids,
        retrieval_mode: selection.retrieval_mode,
        knowledge_search_handler: Some(build_knowledge_search_handler(state.clone())),
    }))
}
use content_delta::handle_content_delta_stream_event;
