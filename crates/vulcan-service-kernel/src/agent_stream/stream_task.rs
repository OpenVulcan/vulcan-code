//! Agent runtime stream task entrypoints.
//! Agent 运行时流任务入口。

use super::*;

mod content_delta;
mod context_policy_refresh;
mod finished;
mod launch_failure;
mod prepared;
mod provider_route_switch;
mod request_phase;
mod request_trace;
mod runtime_event;
mod stream_exit;

use context_policy_refresh::refreshed_execution_request_after_context_compaction_policy_change;
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
use vulcan_agent_runtime::agent_runtime::provider_v1_bridge::ProviderV1LifecycleFacts;

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
}

/// Persists one owned stream failure in the authoritative V1 event ledger.
/// 在权威 V1 事件账本中持久化一次自有流失败。
///
/// # Parameters
/// 参数
/// `task_state` resolves the exact registered session owner and `session_id` selects the failed
/// session. `reason_code` is a stable redacted diagnostic identifier.
/// `task_state` 解析精确的已注册会话所有者，`session_id` 选择失败会话；`reason_code`
/// 是稳定且已脱敏的诊断标识。
///
/// # Returns
/// 返回值
/// Returns only after either the active API is durably process-interrupted or one permanent
/// display error is durably appended when no interruptible API exists.
/// 仅在活动 API 已可靠投影为进程中断，或不存在可中断 API 时已可靠追加永久显示错误后返回。
pub(crate) async fn persist_v1_execution_failure_boundary(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    reason_code: &'static str,
) -> Result<(), String> {
    let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
        task_state, session_id,
    )?;
    let now_ms = current_v1_unix_millis()?;
    handle
        .mutate_durable_if_changed_async(
            crate::session_runtime_registry_v1::SessionMutationAdmission::ExistingTurn,
            crate::session_runtime_registry_v1::SessionFlushReason::DisplayBoundary,
            move |document| {
                if document.interrupt_active_api_for_process(now_ms)?.is_some() {
                    return Ok(
                        crate::session_runtime_registry_v1::SessionMutationChange::Changed(()),
                    );
                }
                let turn_id = document.active_turn().map(|turn| turn.turn_id);
                let already_persisted = document.events().iter().rev().any(|event| {
                    if event.turn_id != turn_id {
                        return false;
                    }
                    match &event.event {
                        vulcan_session_core::v1::EventBody::ApiStats(stats) => {
                            stats.end_reason
                                == Some(vulcan_session_core::v1::ApiEndReason::ProcessInterrupted)
                        }
                        vulcan_session_core::v1::EventBody::DisplayNote(note) => {
                            note.notice_kind
                                == vulcan_session_core::v1::DisplayNoticeKind::SystemError
                                && note.reason_code == reason_code
                        }
                        _ => false,
                    }
                });
                if already_persisted {
                    return Ok(
                        crate::session_runtime_registry_v1::SessionMutationChange::Unchanged(()),
                    );
                }
                document.append_display_note(
                    turn_id,
                    vulcan_session_core::v1::DisplayNoticeKind::SystemError,
                    vulcan_session_core::v1::NoticeSeverity::Error,
                    reason_code,
                    Some(reason_code.to_string()),
                    None,
                    None,
                    None,
                    now_ms,
                )?;
                Ok(crate::session_runtime_registry_v1::SessionMutationChange::Changed(()))
            },
        )
        .await
        .map(|_| ())
        .map_err(|error| format!("failed to persist V1 stream failure `{reason_code}`: {error}"))
}

/// Settles one stream terminal authority and releases its exact admission owner.
/// 结算一个流终态权威，并释放其精确准入 owner。
///
/// # Parameters
/// 参数
/// `task_state` owns the registered V1 session and terminal-persistence owner table.
/// `task_state` 持有已注册 V1 会话与终态持久化 owner 表。
/// `session_id` selects the exact finished session.
/// `session_id` 选择精确的已结束会话。
/// `v1_api_terminal_persisted` reports whether Provider handling already produced a durable API terminal.
/// `v1_api_terminal_persisted` 表示 Provider 处理是否已经生成可靠 API 终态。
/// `fallback_reason_code` is the stable redacted fact used only when no API terminal exists.
/// `fallback_reason_code` 是仅在不存在 API 终态时使用的稳定脱敏事实。
/// `terminal_persistence_owner` is the exact owner adopted after removing the active stream task.
/// `terminal_persistence_owner` 是移除活动流任务后接管的精确 owner。
///
/// # Returns
/// 返回值
/// `true` only after a durable terminal fact exists and the exact admission owner is released.
/// 仅在可靠终态事实存在且精确准入 owner 已释放后返回 `true`。
pub(crate) async fn settle_stream_terminal_authority(
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    v1_api_terminal_persisted: bool,
    fallback_reason_code: &'static str,
    terminal_persistence_owner: SessionTerminalPersistenceOwnerGuard,
) -> bool {
    // A pre-ApiStarted failure has no ApiStats receipt, so the same V1 owner must append a permanent error before admission reopens.
    // ApiStarted 前失败没有 ApiStats 回执，因此重新开放准入前必须由同一 V1 owner 追加永久错误。
    let terminal_state_persisted = if v1_api_terminal_persisted {
        true
    } else {
        match persist_v1_execution_failure_boundary(task_state, session_id, fallback_reason_code)
            .await
        {
            Ok(()) => true,
            Err(error) => {
                eprintln!(
                    "[webui-service] stream fallback terminal persistence failed: session={session_id} reason={fallback_reason_code} error={error}"
                );
                false
            }
        }
    };
    if !terminal_state_persisted {
        // An uncertain durable write remains fail-closed by deliberately retaining the exact owner.
        // 可靠写入结果不确定时会有意保留精确 owner，从而保持失败关闭。
        drop(terminal_persistence_owner);
        return false;
    }
    match terminal_persistence_owner.release() {
        Ok(()) => true,
        Err(error) => {
            eprintln!(
                "[webui-service] stream terminal-persistence owner release failed: session={session_id} error={error}"
            );
            false
        }
    }
}

/// Completes V1 durability after an owned terminal stream failure.
/// 在自有终态流失败后完成 V1 持久化。
///
/// # Parameters
/// 参数
/// `task_state` owns the exact failed session state.
/// `task_state` 持有精确失败会话状态。
/// `session_id` selects the failed launch and `reason_code` identifies its stable display fact.
/// `session_id` 选择失败的启动，`reason_code` 标识其稳定显示事实。
/// `terminal_persistence_owner` binds cleanup to the exact marker installed before active ownership was released.
/// `terminal_persistence_owner` 将清理绑定到释放活动所有权前安装的精确标记。
/// # Returns
/// 返回值
/// This asynchronous cleanup reports failures through service diagnostics and always leaves the in-process token fail-closed when its exact owner remains current.
/// 此异步清理通过服务诊断报告失败，并在精确所有者仍为当前所有者时始终使进程内令牌保持失败关闭。
async fn finalize_failed_execution_stream_retry_lifecycle(
    task_state: SharedServiceBridgeState,
    session_id: SessionId,
    terminal_persistence_owner: SessionTerminalPersistenceOwner,
    reason_code: &'static str,
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
    settle_stream_terminal_authority(
        &task_state,
        session_id,
        false,
        reason_code,
        terminal_persistence_guard,
    )
    .await;
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
                                state.clone(),
                                session_id,
                                SessionTerminalPersistenceOwner::KernelGeneration(
                                    kernel_generation,
                                ),
                                "runtime_event_hook_correlation_rejected",
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
        // providerTelemetryGuard closes an active physical request even when this task future is aborted.
        // providerTelemetryGuard 即使当前任务 Future 被中止，也会关闭活动真实请求。
        let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
            task_state.clone(),
            Some(controller_runtime.clone()),
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
        // active_preparation preserves the exact provider/model budget facts for a mid-turn compaction restart.
        // active_preparation 保留中途回合压缩重启所需的精确 Provider/模型预算事实。
        let mut active_preparation: Option<ExecutionPreparation> = None;
        // preparedRetryCheckpointError blocks the next network-dispatch durability boundary fail closed.
        // preparedRetryCheckpointError 以失败关闭方式阻止下一次网络分派持久化屏障。
        let mut prepared_retry_checkpoint_error: Option<String> = None;
        // providerV1Sink retains the same registered SessionHandle for this complete stream task.
        // providerV1Sink 在当前完整流任务期间保留同一个已注册 SessionHandle。
        let mut provider_v1_sink =
            match provider_v1_stream_sink_for_session(&task_state, session_id) {
                Ok(sink) => sink,
                Err(error) => {
                    // Acquisition failure blocks the existing Started gate instead of falling back to legacy persistence.
                    // 获取失败会阻止既有 Started 闸门，而不是降级到旧持久化。
                    prepared_retry_checkpoint_error = Some(error);
                    None
                }
            };
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
            None,
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
                ExecutionStreamEvent::ProviderRequestCheckpoint { .. } => {
                    // The V1 Provider permit and bounded trace store already own exact request facts.
                    // V1 Provider 许可与有界追踪存储已经拥有精确请求事实。
                }
                ExecutionStreamEvent::ProviderRequestPreparationBoundary {
                    event,
                    candidate,
                    completion,
                } => {
                    let event_kind = event.kind;
                    let lifecycle = event.v1_lifecycle.clone();
                    let permit_result = match (provider_v1_sink.as_mut(), lifecycle) {
                        (Some(sink), Some(ProviderV1LifecycleFacts::RequestPrepared(facts))) => {
                            sink.prepare_request(facts, candidate)
                                .await
                                .map_err(|error| error.to_string())
                        }
                        (Some(_), Some(ProviderV1LifecycleFacts::ExcludedNonSession))
                        | (None, Some(ProviderV1LifecycleFacts::ExcludedNonSession)) => {
                            Ok(candidate.into_excluded_non_session())
                        }
                        (Some(_), _) => {
                            Err(ProviderV1SinkError::MissingTypedRequestFacts.to_string())
                        }
                        (None, Some(ProviderV1LifecycleFacts::RequestPrepared(_))) => {
                            Err(String::from(
                                "A V1 Provider preparation boundary has no registered session owner.",
                            ))
                        }
                        (None, _) => Err(String::from(
                            "Provider preparation is missing its typed lifecycle facts.",
                        )),
                    };
                    if let Err(error) = permit_result.as_ref() {
                        prepared_retry_checkpoint_error = Some(error.clone());
                        eprintln!(
                            "[webui-service] Provider V1 preparation boundary failed: {error}"
                        );
                    }
                    let _ = completion.complete(permit_result);
                    if event_kind != ProviderRequestTelemetryEventKind::Started {
                        eprintln!(
                            "[webui-service] Provider preparation boundary carried a non-Started telemetry event"
                        );
                    }
                }
                ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
                    if prepared_retry_checkpoint_error.is_some() {
                        // A rejected Started boundary owns no ledger row, so later non-durable
                        // telemetry from that physical attempt must remain outside the ledger.
                        // 被拒绝的 Started 边界不拥有账本行，因此该物理尝试后续的非可靠遥测
                        // 必须保持在账本之外。
                        continue;
                    }
                    if provider_v1_sink.is_some() {
                        if let Err(error) = handle_v1_provider_request_telemetry(
                            &task_state,
                            &controller_runtime,
                            &mut provider_telemetry_guard,
                            event,
                            &mut active_provider_goal_id,
                            &mut execution_provider_goal_id,
                            active_stream_task_id,
                            None,
                        )
                        .await
                        {
                            eprintln!(
                                "[webui-service] Provider V1 realtime telemetry update failed: {error}"
                            );
                        }
                        continue;
                    }
                    prepared_retry_checkpoint_error = Some(String::from(
                        "Provider V1 session ownership is unavailable for realtime telemetry.",
                    ));
                    eprintln!(
                        "[webui-service] ignored unowned legacy Provider telemetry: session={session_id}"
                    );
                }
                ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
                    // eventKind is retained across the accounting call so Started can update the exact dispatch gate state.
                    // eventKind 跨记账调用保留，使 Started 能更新精确的分派闸门状态。
                    let event_kind = event.kind;
                    // v1Lifecycle is cloned before the legacy telemetry handler consumes the event.
                    // v1Lifecycle 在旧遥测处理器消费事件前完成克隆。
                    let v1_lifecycle = event.v1_lifecycle.clone();
                    if let Some(error) = rejected_provider_boundary_error(
                        &mut prepared_retry_checkpoint_error,
                        event_kind,
                    ) {
                        let _ = completion.complete(Err(error));
                        continue;
                    }
                    if let Some(sink) = provider_v1_sink.as_mut() {
                        // v1BoundaryResult runs on the same existing Provider barrier; it does not create another gate.
                        // v1BoundaryResult 在同一个既有 Provider 闸门上运行，不创建第二套闸门。
                        let v1_boundary_result = match (event_kind, v1_lifecycle) {
                            (
                                ProviderRequestTelemetryEventKind::Started,
                                Some(ProviderV1LifecycleFacts::RequestPrepared(_)),
                            ) => Err(ProviderV1SinkError::UnexpectedRequestStartDurabilityBoundary),
                            (
                                ProviderRequestTelemetryEventKind::Completed,
                                Some(ProviderV1LifecycleFacts::ApiTerminal(facts)),
                            ) => sink.accept_terminal(facts).await.map(|_| ()),
                            (
                                ProviderRequestTelemetryEventKind::Dispatched,
                                Some(ProviderV1LifecycleFacts::TransportObserved(facts)),
                            ) => sink.mark_transport_observed(facts).await,
                            (
                                ProviderRequestTelemetryEventKind::Started
                                | ProviderRequestTelemetryEventKind::Completed,
                                Some(ProviderV1LifecycleFacts::ExcludedNonSession),
                            ) => Ok(()),
                            (ProviderRequestTelemetryEventKind::Started, _) => {
                                Err(ProviderV1SinkError::MissingTypedRequestFacts)
                            }
                            (ProviderRequestTelemetryEventKind::Completed, _) => {
                                Err(ProviderV1SinkError::MissingTypedTerminalFacts)
                            }
                            _ => Ok(()),
                        };
                        if let Err(error) = v1_boundary_result {
                            let error = error.to_string();
                            if event_kind == ProviderRequestTelemetryEventKind::Started {
                                prepared_retry_checkpoint_error = Some(error.clone());
                            }
                            let _ = completion.complete(Err(error.clone()));
                            eprintln!(
                                "[webui-service] Provider V1 durability boundary failed: {error}"
                            );
                            continue;
                        }
                        let boundary_result = handle_v1_provider_request_telemetry(
                            &task_state,
                            &controller_runtime,
                            &mut provider_telemetry_guard,
                            event,
                            &mut active_provider_goal_id,
                            &mut execution_provider_goal_id,
                            active_stream_task_id,
                            Some(completion),
                        )
                        .await;
                        match boundary_result {
                            Ok(()) => {}
                            Err(error) => {
                                if event_kind == ProviderRequestTelemetryEventKind::Started {
                                    prepared_retry_checkpoint_error = Some(error.clone());
                                }
                                eprintln!(
                                    "[webui-service] Provider V1 durability completion failed: {error}"
                                );
                            }
                        }
                        continue;
                    }
                    // Missing V1 ownership rejects the existing Provider barrier and never falls back to the legacy ledger.
                    // V1 所有权缺失时拒绝既有 Provider 闸门，绝不降级到旧账本。
                    let error = String::from(
                        "Provider V1 session ownership is unavailable for a durability boundary.",
                    );
                    prepared_retry_checkpoint_error = Some(error.clone());
                    let _ = completion.complete(Err(error.clone()));
                    eprintln!("[webui-service] {error}");
                }
                ExecutionStreamEvent::ProviderLoweringRepair { .. } => {
                    // Typed V1 request facts supersede legacy segmented lowering-repair rows.
                    // 强类型 V1 请求事实取代旧分段 lowering-repair 行。
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
                    if prepared_retry_checkpoint_error.is_some() {
                        continue;
                    }
                    if let Some(sink) = provider_v1_sink.as_mut() {
                        let v1_update = current_v1_unix_millis().and_then(|now_ms| {
                            sink.append_reasoning_delta(&delta, now_ms)
                                .map_err(|error| error.to_string())
                        });
                        if let Err(error) = v1_update {
                            prepared_retry_checkpoint_error = Some(error.clone());
                            eprintln!(
                                "[webui-service] Provider V1 reasoning update failed: {error}"
                            );
                            continue;
                        }
                        trace_content_delta_stream_event(
                            &mut request_trace_state,
                            session_id,
                            StreamContentKind::Thinking,
                            &delta,
                        );
                        continue;
                    }
                    prepared_retry_checkpoint_error = Some(String::from(
                        "Provider V1 session ownership is unavailable for a reasoning delta.",
                    ));
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
                ExecutionStreamEvent::ThinkingCompleted { session_id: _ } => {
                    if prepared_retry_checkpoint_error.is_some() {
                        continue;
                    }
                    if let Some(sink) = provider_v1_sink.as_mut() {
                        let v1_update = current_v1_unix_millis().and_then(|now_ms| {
                            sink.complete_reasoning(now_ms)
                                .map_err(|error| error.to_string())
                        });
                        if let Err(error) = v1_update {
                            prepared_retry_checkpoint_error = Some(error.clone());
                            eprintln!(
                                "[webui-service] Provider V1 reasoning completion failed: {error}"
                            );
                            continue;
                        }
                        // V1 already owns the authoritative reasoning event; the legacy sealing path must not run.
                        // V1 已经拥有权威思考事件；不得再运行旧封口路径。
                        continue;
                    }
                    prepared_retry_checkpoint_error = Some(String::from(
                        "Provider V1 session ownership is unavailable for reasoning completion.",
                    ));
                }
                ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                    if prepared_retry_checkpoint_error.is_some() {
                        continue;
                    }
                    if let Some(sink) = provider_v1_sink.as_mut() {
                        let v1_update = current_v1_unix_millis().and_then(|now_ms| {
                            sink.append_assistant_delta(&delta, now_ms)
                                .map_err(|error| error.to_string())
                        });
                        if let Err(error) = v1_update {
                            prepared_retry_checkpoint_error = Some(error.clone());
                            eprintln!(
                                "[webui-service] Provider V1 assistant update failed: {error}"
                            );
                            continue;
                        }
                        trace_content_delta_stream_event(
                            &mut request_trace_state,
                            session_id,
                            StreamContentKind::Output,
                            &delta,
                        );
                        continue;
                    }
                    prepared_retry_checkpoint_error = Some(String::from(
                        "Provider V1 session ownership is unavailable for an assistant delta.",
                    ));
                }
                ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                    if provider_v1_sink.is_some() && prepared_retry_checkpoint_error.is_some() {
                        if event.tool_execution_boundary.is_some()
                            && let Err(rejection_error) = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event(&event)
                        {
                            eprintln!(
                                "[webui-service] reject failed Provider V1 tool boundary failed: {rejection_error}"
                            );
                        }
                        continue;
                    }
                    if event.kind
                        == vulcan_agent_runtime::agent_runtime::events::RuntimeEventKind::ToolRequestStarted
                    {
                        // The first tool-request event is the service boundary where tool-only Provider output becomes replay-relevant even without a visible token.
                        // 首条工具请求事件是纯工具 Provider 输出即使没有可见 Token 也开始影响重放的服务边界。
                        provider_telemetry_guard.observe_semantic_output();
                    }
                    if let Some(boundary) = event.tool_execution_boundary.as_ref() {
                        if let Some(sink) = provider_v1_sink.as_mut() {
                            // The existing runtime barrier is released only after the same V1 owner durably applies this exact boundary.
                            // 仅在同一个 V1 所有者可靠应用当前精确边界后，才释放既有运行时闸门。
                            let v1_tool_result = match current_v1_unix_millis() {
                                Ok(now_ms) => sink.apply_tool_boundary(boundary, now_ms).await,
                                Err(error) => Err(ProviderV1SinkError::Timestamp(error)),
                            };
                            let boundary_outcome = match v1_tool_result {
                                Ok(outcome) => outcome,
                                Err(error) => {
                                    // The exact existing tool barrier is rejected; no legacy transcript fallback runs.
                                    // 拒绝精确的既有工具闸门；不运行旧转录降级路径。
                                    if let Err(rejection_error) = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event(&event) {
                                        eprintln!(
                                            "[webui-service] reject Provider V1 tool boundary failed: {rejection_error}"
                                        );
                                    }
                                    prepared_retry_checkpoint_error = Some(error.to_string());
                                    eprintln!(
                                        "[webui-service] Provider V1 tool boundary failed: {error}"
                                    );
                                    continue;
                                }
                            };
                            let acknowledgement = match boundary_outcome {
                                ProviderV1ToolBoundaryOutcome::Applied => vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_runtime_event_projection_barrier_from_event(&event),
                                ProviderV1ToolBoundaryOutcome::BatchPrepared(identities) => vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::acknowledge_tool_batch_projection_barrier_from_event(&event, identities),
                            };
                            if let Err(error) = acknowledgement {
                                prepared_retry_checkpoint_error = Some(error.clone());
                                eprintln!(
                                    "[webui-service] acknowledge Provider V1 tool boundary failed: {error}"
                                );
                            }
                            // V1 is the sole persistence owner for admitted sessions; legacy recovery/transcript projection is skipped.
                            // 对已准入会话，V1 是唯一持久化所有者；跳过旧恢复与转录投影。
                            continue;
                        }
                        // Missing V1 ownership is a hard durability failure; no retry sidecar may become a second authority.
                        // 缺失 V1 所有权属于可靠性硬失败；不得让重试 sidecar 成为第二权威。
                        let error = String::from(
                            "Provider V1 session ownership is unavailable for a tool boundary.",
                        );
                        prepared_retry_checkpoint_error = Some(error.clone());
                        if let Err(rejection_error) = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event(&event) {
                            eprintln!(
                                "[webui-service] reject unowned Provider V1 tool boundary failed: {rejection_error}"
                            );
                        }
                        eprintln!("[webui-service] {error} session={session_id}");
                        continue;
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
                        &runtime_event_hook_policy,
                        session_id,
                        active_stream_task_id,
                        event,
                    )
                    .await;
                }
                ExecutionStreamEvent::ConversationCheckpoint { session_id, .. } => {
                    if provider_v1_sink.is_some() {
                        // ProviderSource now comes from the typed final response; legacy conversation snapshots are not a V1 source.
                        // ProviderSource 现在来自强类型最终响应；旧会话快照不是 V1 来源。
                        continue;
                    }
                    // A legacy checkpoint cannot become a persistence fallback when V1 ownership is absent.
                    // V1 所有权缺失时，旧 checkpoint 不得成为持久化降级路径。
                    prepared_retry_checkpoint_error = Some(String::from(
                        "Provider V1 session ownership is unavailable for a conversation checkpoint.",
                    ));
                    eprintln!(
                        "[webui-service] ignored unowned legacy conversation checkpoint: session={session_id}"
                    );
                }
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
                                    task_state.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    "context_compaction_policy_refresh_failed",
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
                                    task_state.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    "pending_provider_route_activation_failed",
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
                                task_state.clone(),
                                session_id,
                                SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id),
                                "context_compaction_retry_limit_reached",
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
                                task_state.clone(),
                                session_id,
                                SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id),
                                "context_compaction_preparation_missing",
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
                                    task_state.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    "context_compaction_request_missing",
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
                                    task_state.clone(),
                                    session_id,
                                    SessionTerminalPersistenceOwner::StreamTask(
                                        active_stream_task_id,
                                    ),
                                    "context_compaction_failed",
                                )
                                .await;
                            }
                            return;
                        }
                    }
                }
                ExecutionStreamEvent::Finished(report) => {
                    // The sink is the sole producer of durable physical-API and logical-Turn receipts.
                    // sink 是可靠物理 API 与逻辑回合回执的唯一生产者。
                    let v1_finished_durability = provider_v1_sink
                        .as_mut()
                        .map(ProviderV1StreamSink::take_finished_durability)
                        .unwrap_or_default();
                    match handle_finished_stream_event(
                        &app,
                        &task_state,
                        &controller_runtime,
                        &agent_runtime,
                        active_stream_task_id,
                        stream_task_started_at,
                        execution_provider_goal_id.as_deref(),
                        v1_finished_durability,
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
        if let Some(event) = provider_telemetry_guard.take_cancelled_event() {
            provider_telemetry_guard.forward(event);
        }
        if transient_reasoning_status_active
            && let Err(error) = emit_transient_reasoning_status(&app, &task_state, session_id, None)
        {
            eprintln!("[webui-service] transient reasoning status clear failed: {error}");
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
                        return;
                    }
                };
            // The V1 API terminal or permanent display fact is the only durable retry authority.
            // V1 API 终态或永久显示事实是唯一可靠重试权威。
            let reason_code = match stream_exit_reason {
                StreamTaskExitReason::StreamExhausted => "stream_exhausted",
                StreamTaskExitReason::StaleKernelGeneration => "stale_kernel_generation",
                StreamTaskExitReason::FinishedEventHandled => "finished_event_already_handled",
            };
            settle_stream_terminal_authority(
                &task_state,
                session_id,
                false,
                reason_code,
                terminal_persistence_guard,
            )
            .await;
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
                            registration_failure_state,
                            session_id,
                            SessionTerminalPersistenceOwner::KernelGeneration(kernel_generation),
                            "stream_task_registration_failed",
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

/// Applies non-persistent Provider telemetry after the V1 session owner accepted its boundary.
/// 在 V1 会话所有者接纳边界后应用非持久化 Provider 遥测。
///
/// # Parameters
/// 参数
/// The arguments retain exact stream ownership, Goal accounting, Hook order, and the existing
/// single-use Provider completion without consulting telemetry JSON for V1 identity.
/// 参数保留精确流所有权、目标记账、Hook 顺序与既有一次性 Provider 完成句柄，并且不从遥测 JSON 读取 V1 身份。
///
/// # Returns
/// 返回值
/// Success after owner binding and non-persistent consumers are aligned; the optional existing
/// completion receives the same result exactly once.
/// 所有权绑定与非持久化消费者对齐后返回成功；可选既有完成句柄恰好一次收到相同结果。
#[allow(clippy::too_many_arguments)]
async fn handle_v1_provider_request_telemetry(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    event: ProviderRequestTelemetryEvent,
    active_provider_goal_id: &mut Option<String>,
    execution_provider_goal_id: &mut Option<String>,
    active_stream_task_id: Uuid,
    completion: Option<ProviderRequestDurabilityCompletion>,
) -> Result<(), String> {
    let event_kind = event.kind;
    let event_session_id = event.session_id;
    let completed_usage = event.completed_usage.clone();
    // providerIdentityResult validates the typed V1 request boundary without copying identity into legacy stream state.
    // providerIdentityResult 校验强类型 V1 请求边界，不再把身份复制到旧流状态。
    let provider_identity_result = match event_kind {
        ProviderRequestTelemetryEventKind::Started => match event.v1_lifecycle.as_ref() {
            Some(ProviderV1LifecycleFacts::RequestPrepared(_))
            | Some(ProviderV1LifecycleFacts::ExcludedNonSession) => Ok(()),
            _ => Err(String::from(
                "Provider V1 Started telemetry is missing typed request identity.",
            )),
        },
        _ => Ok(()),
    };
    if provider_identity_result.is_ok() {
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
                                        "[webui-service] Provider V1 goal task binding failed: {error}"
                                    );
                                }
                            }
                        }
                        goal_id
                    }
                    Err(error) => {
                        eprintln!("[webui-service] Provider V1 goal binding failed: {error}");
                        None
                    }
                };
            }
            ProviderRequestTelemetryEventKind::Completed => {
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
                    eprintln!("[webui-service] Provider V1 goal usage accounting failed: {error}");
                }
            }
            ProviderRequestTelemetryEventKind::Dispatched
            | ProviderRequestTelemetryEventKind::FirstToken
            | ProviderRequestTelemetryEventKind::Retrying => {}
        }
    }
    if provider_identity_result.is_ok() {
        provider_telemetry_guard.observe_durable_boundary(&event);
    }
    provider_telemetry_guard.forward(event);
    if let Some(completion) = completion {
        let _ = completion.complete(provider_identity_result.clone());
    }
    provider_identity_result
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
use content_delta::trace_content_delta_stream_event;
