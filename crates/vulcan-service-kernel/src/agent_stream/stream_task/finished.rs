//! Finished stream event orchestration.
//! 流终态事件编排。

use super::conversation_checkpoint::{
    clear_execution_retry_snapshot_after_success, refreeze_claimed_execution_retry_snapshot,
};
use super::*;

/// Outcome of handling one finished stream report.
/// 单个流终态报告的处理结果。
pub(super) enum FinishedStreamEventOutcome {
    /// The active stream owner completed the terminal handling sequence.
    /// 活动流所有者已经完成终态处理序列。
    Handled,
    /// The task no longer owns the session stream slot and skipped terminal side effects.
    /// 当前任务已不再拥有会话流槽位，并跳过终态副作用。
    StaleOwner,
    /// Shared-state ownership could not be inspected, so terminal side effects were skipped.
    /// 无法检查共享状态所有权，因此跳过终态副作用。
    OwnershipUnavailable,
}

/// Handles one terminal execution report after stream generation validation succeeds.
/// 在流代际校验成功后处理单个终态执行报告。
///
/// # Parameters
/// # 参数
/// `app` emits terminal workbench deltas and completion-hook updates.
/// `app` 发送终态工作台增量与完成 Hook 更新。
/// `task_state` owns the active-stream registry and retained workspace state.
/// `task_state` 持有活动流注册表与保留工作区状态。
/// `controller_runtime` persists checkpoints, lifecycle boundaries, goals, and sessions.
/// `controller_runtime` 持久化检查点、生命周期边界、目标与会话。
/// `agent_runtime` provides completion hooks and turn-chaining runtimes.
/// `agent_runtime` 提供完成 Hook 与回合链式运行时。
/// `active_stream_task_id` identifies the exact task allowed to commit terminal effects.
/// `active_stream_task_id` 标识获准提交终态副作用的精确任务。
/// `stream_task_started_at` is the task-entry timestamp used for goal elapsed accounting.
/// `stream_task_started_at` 是用于目标耗时计量的任务入口时间戳。
/// `execution_goal_id` is the latest Goal identity bound at a physical Provider boundary.
/// `execution_goal_id` 是在物理 Provider 边界绑定的最新目标身份。
/// `retry_evidence` proves either a pre-dispatch failure or the latest exact physical Provider call bound to this stream owner.
/// `retry_evidence` 证明分派前失败，或标识绑定到当前流所有者的最近一个精确物理 Provider 调用。
/// `report` is the authoritative terminal report emitted by the Agent runtime.
/// `report` 是 Agent 运行时发送的权威终态报告。
///
/// # Returns
/// # 返回值
/// `Handled` for the active owner, `StaleOwner` for an identity mismatch, or `OwnershipUnavailable` when state locking fails.
/// 活动所有者返回 `Handled`，身份不匹配返回 `StaleOwner`，状态加锁失败返回 `OwnershipUnavailable`。
pub(super) async fn handle_finished_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    active_stream_task_id: Uuid,
    stream_task_started_at: i128,
    execution_goal_id: Option<&str>,
    retry_evidence: FailedExecutionStreamRetryEvidence,
    report: ExecutionRunReport,
) -> FinishedStreamEventOutcome {
    // The report session is the sole owner key for all terminal mutations.
    // 报告会话是全部终态变更的唯一所有权键。
    let finished_session_id = report.session_id;
    let terminal_error = report.error.clone();
    webui_trace(
        "stream_finished",
        format!(
            "session={finished_session_id} provider_requests={} error={} output_chars={}",
            report.provider_request_count,
            report.error.is_some(),
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );

    // Removing the exact active task and installing its terminal-persistence marker form one mandatory ownership gate.
    // 移除精确活动任务并安装其终态持久化标记，共同构成强制所有权闸门。
    // terminal_persistence_owner_id identifies this exact stream boundary in the shared admission table.
    // terminal_persistence_owner_id 在共享准入表中标识当前精确流边界。
    let terminal_persistence_owner_id =
        SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id);
    let removed_active_stream_task = match task_state.lock() {
        Ok(mut guard) => {
            // Identity equality prevents a replaced task from terminalizing the replacement.
            // 身份相等校验防止已被替换的任务终结替代任务。
            if !active_stream_task_matches_id(&guard, finished_session_id, active_stream_task_id) {
                return FinishedStreamEventOutcome::StaleOwner;
            }
            if let Err(error) = register_session_terminal_persistence_owner(
                &mut guard,
                finished_session_id,
                terminal_persistence_owner_id,
            ) {
                eprintln!(
                    "[webui-service] finish stream terminal-persistence registration failed: session={finished_session_id} task_id={active_stream_task_id} error={error}"
                );
                return FinishedStreamEventOutcome::OwnershipUnavailable;
            }
            let removed_entry = remove_active_stream_task_if_id(
                &mut guard,
                finished_session_id,
                active_stream_task_id,
            );
            if removed_entry.is_some() {
                finish_webui_session_kernel_turn(
                    &mut guard,
                    finished_session_id,
                    finished_turn_terminal_status(report.error.is_some()),
                );
            } else {
                // rollback keeps an impossible registry disappearance from leaking a permanent admission block.
                // rollback 避免不可能的注册表消失泄漏永久准入阻塞。
                let _ = rollback_session_terminal_persistence_owner(
                    &mut guard,
                    finished_session_id,
                    terminal_persistence_owner_id,
                );
            }
            removed_entry
        }
        Err(error) => {
            eprintln!("[webui-service] finish stream ownership check failed: {error}");
            webui_trace(
                "stream_finished_skipped_stale_owner",
                format!("session={finished_session_id} task_id={active_stream_task_id}"),
            );
            return FinishedStreamEventOutcome::OwnershipUnavailable;
        }
    };
    let Some(removed_active_stream_task) = removed_active_stream_task else {
        webui_trace(
            "stream_finished_skipped_stale_owner",
            format!("session={finished_session_id} task_id={active_stream_task_id}"),
        );
        return FinishedStreamEventOutcome::StaleOwner;
    };
    // terminalPersistenceOwner blocks every replacement Continue, prompt, Stop, and kernel turn until persistence settles.
    // terminalPersistenceOwner 在持久化结算前阻止全部替代 Continue、提示词、Stop 与 kernel 回合。
    let terminal_persistence_owner = match SessionTerminalPersistenceOwnerGuard::adopt_registered(
        task_state.clone(),
        finished_session_id,
        terminal_persistence_owner_id,
    ) {
        Ok(owner) => owner,
        Err(error) => {
            eprintln!(
                "[webui-service] finish stream terminal-persistence adoption failed: session={finished_session_id} task_id={active_stream_task_id} error={error}"
            );
            return FinishedStreamEventOutcome::OwnershipUnavailable;
        }
    };
    // retainedGoalId closes the interval between an AI-created Goal tool result and the next Provider boundary.
    // retainedGoalId 闭合 AI 创建目标工具结果与下一次 Provider 边界之间的时间窗口。
    let retained_goal_id = removed_active_stream_task
        .session_goal_id()
        .map(str::to_owned);
    // terminalGoalId prefers the stream-local physical-request binding and falls back to the atomically retained task binding.
    // terminalGoalId 优先使用流内物理请求绑定，并回退到原子保留的任务绑定。
    let terminal_goal_id = execution_goal_id.or(retained_goal_id.as_deref());

    if let Err(error) = record_webui_provider_response_checkpoint(controller_runtime, &report) {
        eprintln!("[webui-service] provider response checkpoint write failed: {error}");
    }
    // Goal elapsed time uses whole seconds and saturates invalid platform conversions to zero.
    // 目标耗时使用整秒，并将无效的平台转换饱和为零。
    let goal_elapsed_seconds = current_unix_millis_value()
        .saturating_sub(stream_task_started_at)
        .checked_div(1_000)
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_default();
    webui_trace(
        "stream_goal_accounting_start",
        format!(
            "session={finished_session_id} has_goal={} elapsed_seconds={goal_elapsed_seconds}",
            terminal_goal_id.is_some()
        ),
    );
    match account_session_goal_from_report(
        task_state,
        controller_runtime,
        &report,
        terminal_goal_id,
        goal_elapsed_seconds,
    )
    .await
    {
        Ok(true) => {
            if let Err(error) =
                emit_session_addons_changed_delta(app, task_state, finished_session_id)
            {
                eprintln!("[webui-service] terminal session goal delta emit failed: {error}");
            }
        }
        Ok(false) => {}
        Err(error) => {
            eprintln!("[webui-service] session goal accounting failed: {error}");
        }
    }
    webui_trace(
        "stream_goal_accounting_finished",
        format!("session={finished_session_id}"),
    );

    // Boundary events are captured before report ownership moves into the in-memory projection.
    // 边界事件在报告所有权移入内存投影前完成捕获。
    let terminal_lifecycle = report
        .turn_completed_boundary_event(
            u64::try_from(current_unix_millis_value()).unwrap_or_default(),
        )
        .map(|terminal_boundary_event| {
            // Final assistant boundaries precede the hidden terminal boundary in lifecycle storage.
            // 最终助手边界在生命周期存储中位于隐藏终态边界之前。
            let final_boundary_events = report
                .final_assistant_boundary_event(
                    u64::try_from(current_unix_millis_value()).unwrap_or_default(),
                )
                .into_iter()
                .collect::<Vec<_>>();
            // The preparation title is authoritative; missing failure preparation uses the established placeholder.
            // 准备阶段标题是权威来源；失败报告缺失准备信息时使用既定占位标题。
            let session_title = report
                .preparation
                .as_ref()
                .map(|preparation| preparation.session_title.clone())
                .unwrap_or_else(|| String::from("Untitled"));
            (
                final_boundary_events,
                terminal_boundary_event,
                session_title,
            )
        });
    // Measures terminal report projection, including turn stats and final session state.
    // 衡量终态报告投影耗时，包括统计行与最终会话状态。
    let final_report_apply_started_at = current_unix_millis_value();
    webui_trace(
        "stream_final_report_apply_start",
        format!("session={finished_session_id}"),
    );
    // The optional apply result gates lifecycle, hook, and persistence work after projection failure.
    // 可选应用结果在投影失败后阻止生命周期、Hook 与持久化工作。
    // terminalProjectionFallbackRequired records whether the fail-closed phase must be persisted after retry invalidation finishes.
    // terminalProjectionFallbackRequired 记录重试失效完成后是否必须持久化失败关闭阶段。
    let (final_report_apply, terminal_projection_fallback_required) =
        match apply_execution_report_to_memory(app, task_state, agent_runtime, report) {
            Ok(apply) => (Some(apply), false),
            Err(error) => {
                eprintln!("[webui-service] apply final report failed: {error}");
                // A terminal report projection error must never leave the visible conversation running after ownership is released.
                // 终态报告投影错误绝不能在所有权释放后仍让可见会话保持运行中。
                let fallback_required = match apply_session_phase(
                    app,
                    task_state,
                    finished_session_id,
                    SessionRunPhase::Failed,
                ) {
                    Ok(()) => true,
                    Err(projection_error) => {
                        eprintln!(
                            "[webui-service] apply final report fallback phase failed: {projection_error}"
                        );
                        false
                    }
                };
                (None, fallback_required)
            }
        };
    webui_trace_if_slow(
        "stream_final_report_apply_slow",
        final_report_apply_started_at,
        format!(
            "session={finished_session_id} applied={}",
            final_report_apply.is_some()
        ),
    );
    // clearedRetrySnapshotId is eligible for physical cleanup only after the cleared main reference is durably committed.
    // clearedRetrySnapshotId 仅在主引用清除状态可靠提交后才允许执行物理清理。
    let mut cleared_retry_snapshot_id = None;
    let retry_lifecycle_result = match (terminal_error.as_deref(), final_report_apply.is_some()) {
        (Some(error), _) => match retry_evidence {
            FailedExecutionStreamRetryEvidence::ProvenNotDispatchedBeforeProviderDispatch => {
                refreeze_claimed_execution_retry_snapshot_before_provider_dispatch(
                    task_state,
                    controller_runtime,
                    finished_session_id,
                    error,
                )
            }
            FailedExecutionStreamRetryEvidence::RuntimeBoundary(provider_call_binding) => {
                refreeze_claimed_execution_retry_snapshot(
                    task_state,
                    controller_runtime,
                    finished_session_id,
                    provider_call_binding,
                    error,
                )
            }
        }
        .map_err(|error| error.to_string()),
        (None, true) => {
            clear_execution_retry_snapshot_after_success(task_state, finished_session_id).map(
                |snapshot_id| {
                    cleared_retry_snapshot_id = snapshot_id;
                },
            )
        }
        (None, false) => invalidate_execution_retry_reference(
            task_state,
            finished_session_id,
            "semantic_output_observed",
            "The provider completed, but the terminal projection failed; replay is unsafe.",
        ),
    };
    if let Err(error) = retry_lifecycle_result {
        eprintln!("[webui-service] execution retry terminal transition failed: {error}");
        if let Err(invalidation_error) = invalidate_execution_retry_reference(
            task_state,
            finished_session_id,
            "retry_snapshot_transition_failed",
            "The execution retry sidecar failed its terminal transition and was invalidated.",
        ) {
            eprintln!(
                "[webui-service] execution retry reference invalidation failed: {invalidation_error}"
            );
        }
    }
    // terminalStatePersistenceRequired covers both an accepted report and its explicit failed-phase fallback.
    // terminalStatePersistenceRequired 同时覆盖已接纳报告及其显式失败阶段回退。
    let terminal_state_persistence_required =
        final_report_apply.is_some() || terminal_projection_fallback_required;
    // expectedRetryReference binds persistence compensation to the exact terminal retry identity.
    // expectedRetryReference 将持久化补偿绑定到精确终态重试身份。
    let expected_retry_reference =
        retained_execution_retry_reference(task_state, finished_session_id);
    if let Err(error) = expected_retry_reference.as_ref() {
        eprintln!(
            "[webui-service] read terminal retry reference before persistence failed: {error}"
        );
    }
    // terminalStatePersisted is the sole authority for follow-up turns, hooks, and lifecycle publication.
    // terminalStatePersisted 是后续回合、Hook 与生命周期发布的唯一权威依据。
    let mut terminal_state_persisted = false;
    if terminal_state_persistence_required {
        // finalPersistStartedAt measures both the primary write and any fail-closed replacement write.
        // finalPersistStartedAt 衡量主写入以及可能发生的失败关闭替代写入。
        let final_persist_started_at = current_unix_millis_value();
        // primaryPersistenceResult includes exact snapshot acquisition so missing retained state also fails closed.
        // primaryPersistenceResult 包含精确快照获取，因此保留状态缺失时同样失败关闭。
        let primary_persistence_result =
            match current_workspace_snapshot_from_shared_state(task_state) {
                Ok(Some(latest_workspace_snapshot)) => {
                    persist_workspace_session_update(
                        Some(task_state),
                        controller_runtime.clone(),
                        &latest_workspace_snapshot,
                        finished_session_id,
                    )
                    .await
                }
                Ok(None) => Err(String::from(
                    "The retained terminal workspace is unavailable for persistence.",
                )),
                Err(error) => Err(format!(
                    "Failed to read the retained terminal workspace for persistence: {error}"
                )),
            };
        match primary_persistence_result {
            Ok(()) => terminal_state_persisted = true,
            Err(error) => {
                eprintln!("[webui-service] persist final report failed: {error}");
                // fallbackAllowed remains true for a successful turn with no retry token, or after exact token invalidation.
                // fallbackAllowed 在成功回合没有重试令牌，或精确令牌已失效后保持为 true。
                let fallback_allowed = match expected_retry_reference.as_ref() {
                    Ok(Some(expected_reference)) => {
                        match invalidate_execution_retry_reference_if_matches(
                            task_state,
                            finished_session_id,
                            expected_reference,
                            "retry_snapshot_transition_failed",
                            "The terminal conversation state could not be persisted and its retry token was invalidated.",
                        ) {
                            Ok(invalidated) => invalidated,
                            Err(invalidation_error) => {
                                eprintln!(
                                    "[webui-service] invalidate unpersisted terminal retry reference failed: {invalidation_error}"
                                );
                                false
                            }
                        }
                    }
                    Ok(None) => true,
                    Err(_) => false,
                };
                if fallback_allowed {
                    // blockedPersistenceResult retries only the latest in-memory revision after fail-closed invalidation.
                    // blockedPersistenceResult 仅在失败关闭失效后重试最新内存修订。
                    let blocked_persistence_result =
                        match current_workspace_snapshot_from_shared_state(task_state) {
                            Ok(Some(blocked_workspace_snapshot)) => {
                                persist_workspace_session_update(
                                    Some(task_state),
                                    controller_runtime.clone(),
                                    &blocked_workspace_snapshot,
                                    finished_session_id,
                                )
                                .await
                            }
                            Ok(None) => Err(String::from(
                                "The blocked terminal workspace is unavailable for fallback persistence.",
                            )),
                            Err(snapshot_error) => Err(format!(
                                "Failed to read the blocked terminal workspace for fallback persistence: {snapshot_error}"
                            )),
                        };
                    match blocked_persistence_result {
                        Ok(()) => terminal_state_persisted = true,
                        Err(fallback_error) => eprintln!(
                            "[webui-service] persist blocked final report fallback failed: {fallback_error}"
                        ),
                    }
                }
            }
        }
        webui_trace_if_slow(
            "stream_final_persist_slow",
            final_persist_started_at,
            format!("session={finished_session_id} persisted={terminal_state_persisted}"),
        );
    }
    if terminal_state_persisted {
        // Physical cleanup occurs only after the durable main document no longer references this immutable body.
        // 只有在可靠主文档不再引用该不可变正文后，才执行物理清理。
        if let Some(snapshot_id) = cleared_retry_snapshot_id.as_deref()
            && let Err(error) =
                vulcan_workspace_store::workspace_store::delete_execution_retry_snapshot(
                    &controller_runtime.data_root,
                    finished_session_id,
                    snapshot_id,
                )
        {
            eprintln!(
                "[webui-service] post-persistence execution retry garbage collection failed: {error}"
            );
        }
    }
    // The exact marker opens admission only after the latest terminal or fail-closed replacement revision is durable.
    // 仅在最新终态或失败关闭替代修订可靠落盘后，精确标记才开放准入。
    if terminal_state_persisted {
        if let Err(error) = terminal_persistence_owner.release() {
            eprintln!(
                "[webui-service] finished stream terminal-persistence owner release failed: session={finished_session_id} error={error}"
            );
        }
    } else {
        // Dropping without release deliberately retains the authoritative in-process admission block.
        // 未释放即丢弃会有意保留权威进程内准入阻塞。
        drop(terminal_persistence_owner);
    }
    if let Err(error) =
        emit_conversation_continuation_summary_changed(app, task_state, finished_session_id)
    {
        eprintln!(
            "[webui-service] continuation summary refresh after terminal persistence failed: {error}"
        );
    }

    if terminal_state_persisted
        && let Some(apply) = final_report_apply.as_ref()
        && !apply.vmm_lifecycle_events.is_empty()
    {
        // vmm_lifecycle_events contain only stable identities, hashes, counters, and byte totals.
        // vmm_lifecycle_events 仅包含稳定身份、哈希、计数与字节总量。
        let vmm_lifecycle_events = apply.vmm_lifecycle_events.clone();
        let data_root = controller_runtime.data_root.clone();
        spawn_service_task(async move {
            let timestamp = current_unix_millis_text();
            if let Err(error) = crate::vmm_runtime_audit::record_vmm_lifecycle_events(
                &data_root,
                finished_session_id,
                &timestamp,
                vmm_lifecycle_events,
            ) {
                webui_trace(
                    "runtime_vmm_terminal_audit_failed",
                    format!("session={finished_session_id} error={error}"),
                );
            }
        });
    }

    if terminal_state_persisted
        && let (Some(apply), Some((final_boundary_events, terminal_boundary_event, session_title))) =
            (final_report_apply.as_ref(), terminal_lifecycle)
        && let Some(project_id) =
            owning_project_id_for_session(&apply.workspace_snapshot, finished_session_id)
    {
        // The cloned runtime moves into the non-blocking external lifecycle task.
        // 克隆后的运行时移入非阻塞外部生命周期任务。
        let controller_runtime_for_lifecycle = controller_runtime.clone();
        spawn_service_task(async move {
            // Lifecycle latency is measured inside the spawned task to exclude scheduling delay ambiguity.
            // 生命周期延迟在生成任务内部测量，以排除调度延迟歧义。
            let lifecycle_started_at = current_unix_millis_value();
            if let Err(error) = finish_session_external_turn_lifecycle(
                &controller_runtime_for_lifecycle,
                &project_id,
                finished_session_id,
                &session_title,
                final_boundary_events,
                terminal_boundary_event,
            ) {
                eprintln!("[webui-service] append session terminal boundary failed: {error}");
            }
            webui_trace_if_slow(
                "stream_terminal_lifecycle_finish_slow",
                lifecycle_started_at,
                format!("session={finished_session_id}"),
            );
        });
    }

    // Chained turns start only after the accepted terminal state and detached retry reference are durable.
    // 链式回合仅在已接纳终态与已分离重试引用可靠落盘后启动。
    let started_next_prompt = if terminal_state_persisted {
        maybe_start_webui_turn_chain_after_finished(
            app,
            task_state,
            controller_runtime,
            agent_runtime,
            finished_session_id,
        )
        .await
    } else {
        false
    };
    if terminal_state_persisted
        && let Some(apply) = final_report_apply.as_ref()
        && let Some(event) = conversation_run_settled_event_from_terminal_facts(
            finished_session_id,
            owning_project_id_for_session(&apply.workspace_snapshot, finished_session_id),
            active_stream_task_id,
            apply.conversation_run_outcome,
            started_next_prompt,
            current_unix_millis_text(),
        )
    {
        app.emit(ServiceEvent::ConversationRunSettled(Box::new(event)));
    }
    // A completion request exists only when terminal memory projection opened a completed-turn hook.
    // 仅当终态内存投影打开完成回合 Hook 时才存在完成请求。
    if terminal_state_persisted
        && let Some(completion_request) = final_report_apply
            .as_ref()
            .and_then(|apply| apply.completion_request.clone())
    {
        spawn_conversation_completion_hook_task(
            app.clone(),
            task_state.clone(),
            controller_runtime.clone(),
            agent_runtime.clone(),
            completion_request,
        );
    }
    FinishedStreamEventOutcome::Handled
}

/// Reads the latest retained workspace only when a failed terminal projection requires fallback persistence.
/// 仅在失败的终态投影需要回退持久化时读取最新保留工作区。
///
/// # Parameters
/// 参数
/// `task_state` owns the post-invalidation workspace revision.
/// `task_state` 持有完成失效后的工作区修订。
/// `fallback_required` records whether the fail-closed phase was applied successfully.
/// `fallback_required` 记录失败关闭阶段是否已经成功应用。
///
/// # Returns
/// 返回值
/// The latest retained snapshot when persistence is required, `None` otherwise, or a lock error.
/// 需要持久化时返回最新保留快照，否则返回 `None`；加锁失败时返回错误。
#[cfg(test)]
fn terminal_projection_fallback_snapshot_for_persistence(
    task_state: &SharedServiceBridgeState,
    fallback_required: bool,
) -> Result<Option<WorkspaceSnapshot>, String> {
    if !fallback_required {
        return Ok(None);
    }
    current_workspace_snapshot_from_shared_state(task_state)
}

/// Builds a settled lifecycle event only from terminal facts that close the visible conversation run.
/// 仅根据闭合可见会话运行的终态事实构建最终结束生命周期事件。
///
/// # Parameters
/// # 参数
/// `conversation_id` identifies the regular conversation whose report was accepted.
/// `conversation_id` 标识终态报告已被接纳的普通会话。
/// `project_id` is the owning project resolved from the accepted workspace projection.
/// `project_id` 是从已接纳工作区投影解析出的所属项目。
/// `active_stream_task_id` is the exact stream identity that owned terminal side effects.
/// `active_stream_task_id` 是持有终态副作用的精确流身份。
/// `outcome` exists only when the completion gate declared the logical turn final.
/// `outcome` 仅在完成闸门声明逻辑回合最终结束时存在。
/// `started_next_prompt` indicates whether mailbox, queue, or goal continuation started another turn.
/// `started_next_prompt` 表示 mailbox、队列或目标续跑是否启动了另一回合。
/// `occurred_at` is the service timestamp captured for the emitted event.
/// `occurred_at` 是为发出事件捕获的服务时间戳。
///
/// # Returns
/// # 返回值
/// A deterministic lifecycle event for final idle settlement, otherwise `None`.
/// 最终空闲结束时返回确定性生命周期事件，否则返回 `None`。
fn conversation_run_settled_event_from_terminal_facts(
    conversation_id: SessionId,
    project_id: Option<String>,
    active_stream_task_id: Uuid,
    outcome: Option<vulcan_service_api::ConversationRunOutcome>,
    started_next_prompt: bool,
    occurred_at: String,
) -> Option<ConversationRunSettledEvent> {
    if started_next_prompt {
        return None;
    }
    // outcome is the finality proof produced by the authoritative completion gate.
    // outcome 是权威完成闸门产生的最终性证明。
    let outcome = outcome?;
    // run_id is the exact stream UUID registered before execution began.
    // run_id 是执行开始前注册的精确流 UUID。
    let run_id = active_stream_task_id.to_string();
    Some(ConversationRunSettledEvent {
        event_id: format!("{conversation_id}:{run_id}:settled"),
        conversation_id: conversation_id.to_string(),
        project_id,
        run_id,
        outcome,
        occurred_at,
    })
}

/// Maps report error presence to the session-kernel terminal status.
/// 将报告错误存在性映射到会话内核终态状态。
///
/// # Parameters
/// # 参数
/// `has_error` indicates whether the terminal execution report carries an error.
/// `has_error` 表示终态执行报告是否携带错误。
///
/// # Returns
/// # 返回值
/// `Failed` for error reports and `Completed` for successful reports.
/// 错误报告返回 `Failed`，成功报告返回 `Completed`。
fn finished_turn_terminal_status(has_error: bool) -> TurnTerminalStatus {
    if has_error {
        TurnTerminalStatus::Failed
    } else {
        TurnTerminalStatus::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one frozen retry reference used to distinguish stale and latest fallback snapshots.
    /// 构建一条冻结重试引用，用于区分过期与最新回退快照。
    ///
    /// # Returns
    /// 返回值
    /// A lightweight reference that initially proves replay safety.
    /// 返回初始能够证明重放安全的轻量引用。
    fn retry_reference_for_terminal_fallback_test()
    -> vulcan_workbench_model::ExecutionRetrySnapshotRef {
        vulcan_workbench_model::ExecutionRetrySnapshotRef {
            schema_version: vulcan_workbench_model::EXECUTION_RETRY_SNAPSHOT_SCHEMA_VERSION,
            snapshot_id: String::from("00000000-0000-0000-0000-000000000001"),
            generation: 1,
            state: vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen,
            content_sha256: "0".repeat(64),
            content_bytes: 1,
            logical_execution_id: String::from("execution-1"),
            pending_action_label: String::from("resume_preparation"),
            provider_route: None,
            preparation_route_selection: Some(
                vulcan_workbench_model::ExecutionRetryPreparationRouteSelection {
                    active_provider_id: None,
                    active_connection_id: None,
                    active_model_id: None,
                },
            ),
            context_generation_id: None,
            canonical_dispatch_sha256: "0".repeat(64),
            retryable: true,
            requires_manual_confirmation: false,
            blocked_reason: None,
            claimed_attempt_id: None,
            error_summary: None,
            updated_at: String::from("1"),
        }
    }

    /// Verifies fallback persistence reads the post-invalidation state instead of a stale phase snapshot.
    /// 验证回退持久化会读取失效后的状态，而不是过期的阶段快照。
    #[test]
    fn terminal_projection_fallback_persists_latest_retry_invalidation() {
        // sessionId identifies the exact retained regular conversation under test.
        // sessionId 标识被测的精确保留常规会话。
        let session_id = SessionId::new_v4();
        // serviceState supplies the minimum workspace shell and session summary required by the shared-state reader.
        // serviceState 提供共享状态读取器所需的最小工作区壳与会话摘要。
        let mut service_state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        service_state
            .workspace_snapshot
            .as_mut()
            .expect("test workspace should exist")
            .session_runtime_state
            .insert(
                session_id,
                SessionRuntimeStateEntry {
                    execution_retry_snapshot: Some(retry_reference_for_terminal_fallback_test()),
                    ..SessionRuntimeStateEntry::default()
                },
            );
        // taskState exposes the exact retained revision used by production fallback persistence.
        // taskState 暴露生产回退持久化使用的精确保留修订。
        let task_state: SharedServiceBridgeState =
            std::sync::Arc::new(std::sync::Mutex::new(service_state));
        // staleSnapshot models the phase-only clone that existed before retry invalidation.
        // staleSnapshot 模拟重试失效前存在的仅阶段克隆。
        let stale_snapshot = current_workspace_snapshot_from_shared_state(&task_state)
            .expect("stale snapshot read should succeed")
            .expect("stale workspace should exist");

        invalidate_execution_retry_reference(
            &task_state,
            session_id,
            "semantic_output_observed",
            "The provider completed before terminal projection failed.",
        )
        .expect("retry invalidation should succeed");
        // latestSnapshot is the exact state selected for fallback persistence after invalidation.
        // latestSnapshot 是失效后被回退持久化选择的精确状态。
        let latest_snapshot =
            terminal_projection_fallback_snapshot_for_persistence(&task_state, true)
                .expect("latest fallback read should succeed")
                .expect("latest fallback workspace should exist");
        let stale_reference = stale_snapshot.session_runtime_state[&session_id]
            .execution_retry_snapshot
            .as_ref()
            .expect("stale retry reference should exist");
        let latest_reference = latest_snapshot.session_runtime_state[&session_id]
            .execution_retry_snapshot
            .as_ref()
            .expect("latest retry reference should exist");
        assert_eq!(
            stale_reference.state,
            vulcan_workbench_model::ExecutionRetrySnapshotState::Frozen
        );
        assert_eq!(
            latest_reference.state,
            vulcan_workbench_model::ExecutionRetrySnapshotState::Invalidated
        );
        assert_eq!(
            latest_reference.blocked_reason.as_deref(),
            Some("semantic_output_observed")
        );
        assert!(
            terminal_projection_fallback_snapshot_for_persistence(&task_state, false)
                .expect("disabled fallback read should succeed")
                .is_none()
        );
    }

    /// Verifies that report success and failure map to exact kernel terminal states.
    /// 验证报告成功与失败会映射到精确的内核终态。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn report_error_presence_defines_kernel_terminal_status() {
        assert!(matches!(
            finished_turn_terminal_status(false),
            TurnTerminalStatus::Completed
        ));
        assert!(matches!(
            finished_turn_terminal_status(true),
            TurnTerminalStatus::Failed
        ));
    }

    /// Verifies completed and failed outcomes produce distinct deterministic lifecycle events.
    /// 验证完成与失败结果会产生不同且确定的生命周期事件。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn final_idle_terminal_facts_build_deterministic_settlement_events() {
        // conversation_id identifies the exact regular session under test.
        // conversation_id 标识本测试中的精确普通会话。
        let conversation_id = SessionId::new_v4();
        // run_id identifies one exact stream generation for deterministic event ids.
        // run_id 标识一个精确流代次，用于生成确定性事件标识。
        let run_id = Uuid::new_v4();
        // completed is the event derived from a final successful report.
        // completed 是从最终成功报告派生的事件。
        let completed = conversation_run_settled_event_from_terminal_facts(
            conversation_id,
            Some(String::from("project-a")),
            run_id,
            Some(vulcan_service_api::ConversationRunOutcome::Completed),
            false,
            String::from("1000"),
        )
        .expect("final successful run should settle");
        // failed is the event derived from the same exact run identity with a failure result.
        // failed 是使用相同精确运行身份与失败结果派生的事件。
        let failed = conversation_run_settled_event_from_terminal_facts(
            conversation_id,
            Some(String::from("project-a")),
            run_id,
            Some(vulcan_service_api::ConversationRunOutcome::Failed),
            false,
            String::from("1000"),
        )
        .expect("final failed run should settle");
        assert_eq!(completed.event_id, failed.event_id);
        assert_eq!(completed.run_id, run_id.to_string());
        assert_eq!(completed.conversation_id, conversation_id.to_string());
        assert_eq!(completed.project_id.as_deref(), Some("project-a"));
        assert_eq!(
            completed.outcome,
            vulcan_service_api::ConversationRunOutcome::Completed
        );
        assert_eq!(
            failed.outcome,
            vulcan_service_api::ConversationRunOutcome::Failed
        );
    }

    /// Verifies pending child work and immediate chained turns suppress settlement events.
    /// 验证待处理子工作与立即链式回合会抑制最终结束事件。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn unfinished_or_chained_terminal_facts_do_not_build_settlement_events() {
        // conversation_id identifies the exact regular session under test.
        // conversation_id 标识本测试中的精确普通会话。
        let conversation_id = SessionId::new_v4();
        // run_id identifies the exact owned stream generation.
        // run_id 标识精确持有的流代次。
        let run_id = Uuid::new_v4();
        assert!(
            conversation_run_settled_event_from_terminal_facts(
                conversation_id,
                None,
                run_id,
                None,
                false,
                String::from("1000"),
            )
            .is_none()
        );
        assert!(
            conversation_run_settled_event_from_terminal_facts(
                conversation_id,
                None,
                run_id,
                Some(vulcan_service_api::ConversationRunOutcome::Completed),
                true,
                String::from("1000"),
            )
            .is_none()
        );
    }
}
