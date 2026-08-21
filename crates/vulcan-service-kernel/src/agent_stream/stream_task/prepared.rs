//! Prepared stream event handling helpers.
//! Prepared 流事件处理辅助逻辑。

use super::*;

/// Control-flow action returned after handling one prepared stream event.
/// 处理单个 Prepared 流事件后返回的控制流动作。
pub(crate) enum PreparedStreamEventAction {
    /// Continue polling the current provider stream.
    /// 继续轮询当前 Provider 流。
    Continue,
    /// The turn restarted with an automatically compacted request.
    /// 当前回合已经使用自动压缩后的请求重启。
    Restarted,
    /// The turn failed while preparing the provider request.
    /// 当前回合在准备 Provider 请求时失败。
    Failed,
}

/// Ownership result from committing one terminal Prepared-stream failure.
/// 提交单个 Prepared 流终止失败后的所有权结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedStreamFailureCommit {
    /// The failing stream owned and removed its exact task entry, then terminalized the kernel.
    /// 失败流拥有并移除了自身精确任务条目，随后终结 kernel。
    Committed,
    /// A replacement task already owns the session, so no shared terminal state was changed.
    /// 替代任务已经拥有会话，因此未修改共享终态。
    Superseded,
    /// Retained phase state was unavailable, or the service-state registry could not be locked.
    /// 保留阶段状态不可用，或无法锁定服务状态注册表。
    StateUnavailable,
}

/// Commits one terminal Prepared-stream failure only when the caller still owns the exact active task.
/// 仅当调用方仍拥有精确活动任务时提交单个 Prepared 流终止失败。
///
/// # Parameters
/// # 参数
/// `app` emits the terminal Failed phase delta.
/// `app` 发送终止性的 Failed 阶段增量。
/// `task_state` owns active stream tasks, kernels, and retained session phases.
/// `task_state` 持有活动流任务、kernel 与保留会话阶段。
/// `session_id` identifies the failing Prepared stream session.
/// `session_id` 标识 Prepared 流失败的会话。
/// `active_stream_task_id` proves ownership of the exact stream generation being terminated.
/// `active_stream_task_id` 证明被终结精确流代次的所有权。
/// `failure_context` labels diagnostics for the preparation or automatic-compaction failure source.
/// `failure_context` 标记准备或自动压缩失败来源的诊断上下文。
///
/// # Returns
/// # 返回值
/// A typed ownership outcome describing whether terminal state was committed, superseded, or unavailable.
/// 返回描述终态已提交、已被替代或状态不可用的强类型所有权结果。
pub(crate) fn commit_prepared_stream_failure(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    failure_context: &str,
) -> PreparedStreamFailureCommit {
    // terminal_persistence_owner identifies this exact stream after its active registry entry is removed.
    // terminal_persistence_owner 在活动注册表条目移除后标识当前精确流。
    let terminal_persistence_owner =
        SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id);
    // terminal_phase_event is committed before destructive ownership cleanup under the same state guard.
    // terminal_phase_event 在同一状态 guard 内先于破坏性所有权清理提交。
    let terminal_phase_event = match task_state.lock() {
        Ok(mut guard) => {
            if !active_stream_task_matches_id(&guard, session_id, active_stream_task_id) {
                return PreparedStreamFailureCommit::Superseded;
            }
            if let Err(error) = register_session_terminal_persistence_owner(
                &mut guard,
                session_id,
                terminal_persistence_owner,
            ) {
                eprintln!(
                    "[webui-service] {failure_context} terminal-persistence registration failed: session={session_id} task={active_stream_task_id} error={error}"
                );
                return PreparedStreamFailureCommit::StateUnavailable;
            }
            // phase_event is optional because an exact owner still requires retry invalidation, kernel cleanup, and persistence after projection failure.
            // phase_event 为可选值，因为阶段投影失败后，精确所有者仍需完成重试失效、kernel 清理与持久化。
            let phase_event = match apply_session_phase_to_state(
                &mut guard,
                session_id,
                SessionRunPhase::Failed,
            ) {
                Ok(event) => Some(event),
                Err(error) => {
                    eprintln!(
                        "[webui-service] {failure_context} terminal phase state unavailable: session={session_id} task={active_stream_task_id} error={error}"
                    );
                    None
                }
            };
            // terminalAt closes the resumable execution ledger before exact stream ownership is released.
            // terminalAt 在释放精确流所有权前闭合可恢复执行账本。
            let terminal_at = current_unix_millis_text();
            let continuation_result = guard
                .workspace_snapshot
                .as_mut()
                .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
                .ok_or_else(|| {
                    String::from("Prepared stream failure runtime state is unavailable.")
                })
                .and_then(|runtime_state| {
                    refresh_terminal_continuation_state(runtime_state, false, &terminal_at)?;
                    runtime_state.updated_at = terminal_at;
                    Ok(())
                });
            if let Err(error) = continuation_result {
                // retryInvalidated closes the exact token fail-closed when its terminal ledger cannot advance.
                // retryInvalidated 在终态账本无法推进时以失败关闭方式闭合精确令牌。
                let retry_invalidated = guard
                    .workspace_snapshot
                    .as_mut()
                    .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
                    .is_some_and(|runtime_state| {
                        super::conversation_checkpoint::invalidate_execution_retry_runtime_state(
                            runtime_state,
                            "retry_snapshot_transition_failed",
                            "The Prepared stream terminal continuation could not advance and was invalidated.",
                            current_unix_millis_text(),
                        )
                    });
                eprintln!(
                    "[webui-service] {failure_context} continuation terminalization failed: session={session_id} task={active_stream_task_id} retry_invalidated={retry_invalidated} error={error}"
                );
            }
            // removed_task consumes the exact owner only after its Failed phase is retained.
            // removed_task 仅在 Failed 阶段保留后消费精确所有者。
            let removed_task =
                remove_active_stream_task_if_id(&mut guard, session_id, active_stream_task_id);
            if removed_task.is_none() {
                let _ = rollback_session_terminal_persistence_owner(
                    &mut guard,
                    session_id,
                    terminal_persistence_owner,
                );
                eprintln!(
                    "[webui-service] {failure_context} exact stream owner disappeared inside terminal transaction: session={session_id} task={active_stream_task_id}"
                );
                return PreparedStreamFailureCommit::StateUnavailable;
            }
            finish_webui_session_kernel_turn(&mut guard, session_id, TurnTerminalStatus::Failed);
            phase_event
        }
        Err(error) => {
            eprintln!(
                "[webui-service] {failure_context} terminal commit failed to lock service state: {error}"
            );
            return PreparedStreamFailureCommit::StateUnavailable;
        }
    };
    if let Some(terminal_phase_event) = terminal_phase_event {
        emit_service_event_with_trace(
            app,
            "workbench_prepared_failure_phase_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(terminal_phase_event)),
        );
    }
    PreparedStreamFailureCommit::Committed
}

/// Applies one prepared event and performs any required automatic compaction restart.
/// 应用单个 Prepared 事件，并执行必要的自动压缩重启。
///
/// # Parameters
/// 参数
/// `app` emits workbench snapshots and session phase updates.
/// `app` 发送工作台快照与会话阶段更新。
/// `task_state` owns active stream task state for the current WebUI session.
/// `task_state` 持有当前 WebUI 会话的活跃流任务状态。
/// `controller_runtime` persists preparation checkpoints and terminal state.
/// `controller_runtime` 持久化准备检查点与终态状态。
/// `agent_runtime` provides runtimes needed to rebuild a compacted request.
/// `agent_runtime` 提供重建压缩请求所需的运行时集合。
/// `request` is the current execution request being streamed.
/// `request` 是当前正在流式执行的请求。
/// `preparation` is the runtime preparation payload from the agent stream.
/// `preparation` 是来自 Agent 流的运行时准备载荷。
/// `session_id` identifies the active WebUI session.
/// `session_id` 标识当前活跃 WebUI 会话。
/// `stream_task_started_at` is used for latency tracing.
/// `stream_task_started_at` 用于延迟追踪。
/// `auto_compaction_already_attempted` prevents repeated automatic compaction loops.
/// `auto_compaction_already_attempted` 防止重复进入自动压缩循环。
/// `active_stream_task_id` limits cleanup to the task handling this prepared event.
/// `active_stream_task_id` 将清理限制为处理当前 Prepared 事件的任务。
/// `expected_kernel_generation` and `expected_kernel_turn_id` preserve the immutable owner across a compaction restart.
/// `expected_kernel_generation` 与 `expected_kernel_turn_id` 跨压缩重启保留不可变所有者。
///
/// # Returns
/// 返回值
/// A control-flow action telling the caller whether to continue or stop the current task.
/// 返回控制流动作，告知调用方继续当前任务还是停止当前任务。
pub(crate) async fn handle_prepared_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    request: &ExecutionPreparationRequest,
    preparation: ExecutionPreparation,
    session_id: SessionId,
    stream_task_started_at: i128,
    auto_compaction_already_attempted: bool,
    active_stream_task_id: Uuid,
    expected_kernel_generation: u64,
    expected_kernel_turn_id: &str,
) -> PreparedStreamEventAction {
    webui_after_precheck_trace(
        "after_precheck_24_prepared_event",
        format!(
            "session={} provider={} model={} elapsed_ms_since_task={}",
            preparation.session_id,
            preparation.model_blueprint.provider_id,
            preparation.model_blueprint.model_id,
            current_unix_millis_value().saturating_sub(stream_task_started_at)
        ),
    );
    webui_trace(
        "stream_prepared",
        format!(
            "session={} provider={} model={} elapsed_ms={}",
            preparation.session_id,
            preparation.model_blueprint.provider_id,
            preparation.model_blueprint.model_id,
            current_unix_millis_value().saturating_sub(stream_task_started_at)
        ),
    );
    if let Err(error) = apply_execution_preparation(
        app,
        task_state,
        controller_runtime,
        active_stream_task_id,
        &preparation,
    ) {
        eprintln!("[webui-service] apply preparation failed: {error}");
        // failure_commit records whether this exact stream owned the terminal kernel transition.
        // failure_commit 记录当前精确流是否拥有终止 kernel 转换。
        let failure_commit = commit_prepared_stream_failure(
            app,
            task_state,
            session_id,
            active_stream_task_id,
            "execution preparation failure",
        );
        webui_trace(
            "stream_preparation_failure_committed",
            format!("session={session_id} outcome={failure_commit:?}"),
        );
        if failure_commit == PreparedStreamFailureCommit::Committed {
            finalize_failed_execution_stream_retry_lifecycle(
                app.clone(),
                task_state.clone(),
                controller_runtime.clone(),
                session_id,
                SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id),
                FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch,
                format!("The execution preparation could not be applied: {error}"),
            )
            .await;
        }
        return PreparedStreamEventAction::Failed;
    }
    webui_after_precheck_trace(
        "after_precheck_25_preparation_applied",
        format!(
            "session={session_id} elapsed_ms_since_task={}",
            current_unix_millis_value().saturating_sub(stream_task_started_at)
        ),
    );
    let auto_compaction_started_at = current_unix_millis_value();
    let auto_compaction_result = if auto_compaction_already_attempted {
        Ok(None)
    } else {
        maybe_auto_compact_webui_session_before_provider_request(
            app,
            task_state,
            controller_runtime,
            agent_runtime,
            active_stream_task_id,
            request,
            &preparation,
        )
        .await
    };
    webui_after_precheck_trace(
        "after_precheck_26_auto_compaction_checked",
        format!(
            "session={session_id} already_attempted={} elapsed_ms={}",
            auto_compaction_already_attempted,
            current_unix_millis_value().saturating_sub(auto_compaction_started_at)
        ),
    );
    match auto_compaction_result {
        Ok(Some(updated_request)) => {
            webui_trace(
                "stream_auto_compaction_restarting",
                format!("session={session_id}"),
            );
            let owns_active_stream_entry = task_state
                .lock()
                .ok()
                .and_then(|mut guard| {
                    remove_active_stream_task_if_id(&mut guard, session_id, active_stream_task_id)
                })
                .is_some();
            if !owns_active_stream_entry {
                return PreparedStreamEventAction::Restarted;
            }
            spawn_agent_stream_task_with_auto_compaction_state(
                app.clone(),
                task_state.clone(),
                controller_runtime.clone(),
                agent_runtime.clone(),
                updated_request,
                expected_kernel_generation,
                expected_kernel_turn_id.to_string(),
                true,
            );
            PreparedStreamEventAction::Restarted
        }
        Ok(None) => PreparedStreamEventAction::Continue,
        Err(error) => {
            eprintln!(
                "[webui-service] automatic context compaction failed before provider request: {error}"
            );
            // failure_commit records whether this exact stream owned the terminal kernel transition.
            // failure_commit 记录当前精确流是否拥有终止 kernel 转换。
            let failure_commit = commit_prepared_stream_failure(
                app,
                task_state,
                session_id,
                active_stream_task_id,
                "automatic context compaction failure",
            );
            webui_trace(
                "stream_auto_compaction_failure_committed",
                format!("session={session_id} outcome={failure_commit:?}"),
            );
            if failure_commit == PreparedStreamFailureCommit::Committed {
                finalize_failed_execution_stream_retry_lifecycle(
                    app.clone(),
                    task_state.clone(),
                    controller_runtime.clone(),
                    session_id,
                    SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id),
                    FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch,
                    format!(
                        "Automatic context compaction failed before Provider dispatch: {error}"
                    ),
                )
                .await;
            }
            PreparedStreamEventAction::Failed
        }
    }
}
