//! Terminal execution report memory application for WebUI sessions.
//! WebUI 会话终态执行报告内存应用逻辑。

use vulcan_agent_runtime::{
    ExecutionPromptPurpose, ExecutionRunReport,
    agent_runtime::conversation_hooks::ConversationCompletionRequest,
    conversation_protocol::{SessionConversationState, conversation_state_to_session_messages},
};
use vulcan_core::types::ProviderUsageSnapshot;
use vulcan_service_api::ServiceEventEmitter;
use vulcan_workbench_model::{SessionId, SessionRunPhase, SessionRuntimeStateEntry, SessionState};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use super::{
    AgentRuntimeBridge, ContextUsageChangedDelta, ServiceEvent, SharedServiceBridgeState,
    UpdateConversationRunPhaseDelta, WorkbenchDelta, WorkbenchDeltaEnvelope,
    apply_checkpoint_conversation_state, apply_terminal_execution_report_to_state,
    apply_vmm_writeback_effects_to_runtime_state, build_conversation_messages_patched_delta,
    build_vmm_hook_config_snapshot, context_usage_snapshot_from_runtime_state,
    conversation_messages_from_session, current_unix_millis_text, current_unix_millis_value,
    emit_service_event_with_trace, merge_runtime_state_records_into_display_messages,
    persist_workspace_session_update, replace_conversation_messages_in_workbench_window,
    replace_display_messages_with_runtime_state_records, spawn_service_task,
    spawn_workspace_session_runtime_state_persist_task,
    spawn_workspace_session_transcript_persist_task, sync_hydrated_session_dialogue_ready_marker,
    webui_trace, webui_trace_if_slow, webui_turn_completion_gate_from_state,
    workbench_surface_matches_conversation, workspace_snapshot_contains_regular_session,
};

/// Applies one streamed conversation checkpoint to retained workspace memory.
/// 将单个流式对话检查点应用到保留工作区内存。
///
/// # Parameters
/// 参数
/// `app` emits the rebuilt workbench snapshot after the in-memory mutation.
/// `app` 用于在内存变更后发送重建的工作台快照。
/// `state` owns the shared service bridge state and retained workspace snapshot.
/// `state` 持有共享服务桥状态与保留工作区快照。
/// `controller_runtime` persists main-session checkpoint state after mutation.
/// `controller_runtime` 在变更后持久化主会话检查点状态。
/// `session_id` identifies the session receiving the checkpoint.
/// `session_id` 标识接收检查点的会话。
/// `conversation_state` is the runtime conversation state captured by the provider stream.
/// `conversation_state` 是 provider 流捕获的运行时对话状态。
/// `last_successful_provider_usage` records the latest provider usage when present.
/// `last_successful_provider_usage` 在存在时记录最近一次成功 provider 用量。
/// `provider_request_count` records the safe provider request count for continuation.
/// `provider_request_count` 记录续跑可用的安全 provider 请求次数。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after memory mutation and any required persistence tasks are scheduled.
/// 在完成内存变更并调度必要持久化任务后返回 `Ok(())`。
pub(super) fn apply_conversation_checkpoint(
    _app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    conversation_state: SessionConversationState,
    last_successful_provider_usage: Option<ProviderUsageSnapshot>,
    provider_request_count: usize,
) -> Result<(), String> {
    let (is_main_session, workspace_snapshot) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        {
            let now = current_unix_millis_text();
            // Only main-session checkpoints may persist through workspace-store; TaskSession checkpoints stay in memory here.
            // 只有主会话 checkpoint 可以通过 workspace-store 持久化；TaskSession checkpoint 在这里仅更新内存显示态。
            let is_main_session = workspace
                .sessions
                .iter()
                .any(|candidate| candidate.id == session_id);
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            apply_checkpoint_conversation_state(runtime_state, conversation_state, &now);
            if let Some(usage) = last_successful_provider_usage {
                runtime_state.last_provider_usage = Some(usage.clone());
                runtime_state.last_successful_provider_usage = Some(usage);
            }
            if provider_request_count > 0 {
                runtime_state.last_safe_provider_request_count = Some(provider_request_count);
            }
            let runtime_state_snapshot = runtime_state.clone();
            if is_main_session {
                merge_runtime_state_records_into_display_messages(
                    workspace,
                    session_id,
                    &runtime_state_snapshot,
                );
            } else {
                replace_display_messages_with_runtime_state_records(
                    workspace,
                    session_id,
                    &runtime_state_snapshot,
                );
            }
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            (is_main_session, workspace.clone())
        }
    };
    if is_main_session {
        spawn_workspace_session_transcript_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot.clone(),
            session_id,
            Some(String::from("conversation_checkpoint_transcript")),
        );
        spawn_workspace_session_runtime_state_persist_task(
            state.clone(),
            controller_runtime.clone(),
            workspace_snapshot,
            session_id,
            Some(String::from("conversation_checkpoint")),
        );
    }
    Ok(())
}

/// Result produced after applying one terminal report to retained memory.
/// 将一份终态报告应用到保留内存后产生的结果。
pub(super) struct TerminalReportMemoryApply {
    /// Updated workspace snapshot after applying the terminal report.
    /// 应用终态报告后的更新工作区快照。
    pub(super) workspace_snapshot: WorkspaceSnapshot,
    /// Completed-turn hook request that should run asynchronously.
    /// 需要异步执行的完成回合 hook 请求。
    pub(super) completion_request: Option<ConversationCompletionRequest>,
}

/// Applies a TaskSession terminal report without rebuilding the full WebUI workbench.
/// 在不重建完整 WebUI workbench 的情况下应用 TaskSession 终态报告。
///
/// # Parameters
/// 参数
/// `state` is the shared Tauri service bridge state that owns the in-memory workspace snapshot.
/// `state` 是持有内存工作区快照的共享 Tauri 服务桥状态。
/// `report` is the completed child execution report.
/// `report` 是已完成的子执行报告。
///
/// # Returns
/// 返回值
/// Returns the updated workspace snapshot and no hook completion request.
/// 返回更新后的工作区快照，且不产生 hook 完成请求。
pub(super) fn apply_task_session_execution_report_to_memory(
    state: &SharedServiceBridgeState,
    report: ExecutionRunReport,
) -> Result<TerminalReportMemoryApply, String> {
    let (session_id, is_success) =
        apply_task_session_execution_report_to_memory_in_place(state, &report)?;
    let workspace_snapshot = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
    webui_trace(
        "execution_report_applied_task_session_fast",
        format!(
            "session={session_id} error={} provider_requests={} output_chars={}",
            !is_success,
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok(TerminalReportMemoryApply {
        workspace_snapshot,
        completion_request: None,
    })
}

/// Applies a TaskSession terminal report in place without rebuilding the full workspace snapshot.
/// 原地应用 TaskSession 终态报告，不重建完整工作区快照。
///
/// # Parameters
/// 参数
/// `state` is the shared service bridge state containing the retained workspace snapshot.
/// `state` 是包含保留工作区快照的共享服务桥状态。
/// `report` is the terminal execution report to merge into the TaskSession state.
/// `report` 是要合并进 TaskSession 状态的终态执行报告。
///
/// # Returns
/// 返回值
/// Returns the affected session id and whether the report represents a successful run.
/// 返回受影响的会话标识以及该报告是否代表成功运行。
pub(super) fn apply_task_session_execution_report_to_memory_in_place(
    state: &SharedServiceBridgeState,
    report: &ExecutionRunReport,
) -> Result<(SessionId, bool), String> {
    let session_id = report.session_id;
    let is_success = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let now = current_unix_millis_text();
        let (is_success, conversation_state) = {
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            let apply_result =
                apply_terminal_execution_report_to_state(runtime_state, session_id, &now, report);
            (
                apply_result.is_success,
                runtime_state.conversation_state.clone(),
            )
        };
        let messages = conversation_state_to_session_messages(&conversation_state);
        if messages.is_empty() {
            workspace.session_messages.remove(&session_id);
        } else {
            workspace.session_messages.insert(session_id, messages);
        }
        workspace.session_run_phases.insert(
            session_id,
            if is_success {
                SessionRunPhase::Ready
            } else {
                SessionRunPhase::Failed
            },
        );
        sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
        is_success
    };
    webui_trace(
        "execution_report_applied_task_session_in_place",
        format!(
            "session={session_id} error={} provider_requests={} output_chars={}",
            !is_success,
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok((session_id, is_success))
}

/// Applies the terminal execution report to retained memory and emits the updated workbench view.
/// 将终态执行报告应用到保留内存中，并发出更新后的工作台视图。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit the refreshed workbench snapshot.
/// `app` 是用于发出刷新后工作台快照的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot mutated by the stream task.
/// `state` 持有由流任务修改的保留工作区快照。
/// `agent_runtime` provides conversation hooks and VMM synchronization state.
/// `agent_runtime` 提供会话 hook 与 VMM 同步状态。
/// `report` is the terminal execution report produced by the agent runtime.
/// `report` 是 Agent 运行时产出的终态执行报告。
///
/// # Returns
/// 返回值
/// The updated workspace snapshot after the terminal report has been applied in memory.
/// 返回终态报告已经应用到内存后的更新工作区快照。
pub(super) fn apply_execution_report_to_memory(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    report: ExecutionRunReport,
) -> Result<TerminalReportMemoryApply, String> {
    let session_id = report.session_id;
    let report_is_regular_session = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard
            .workspace_snapshot
            .as_ref()
            .is_some_and(|workspace_snapshot| {
                workspace_snapshot_contains_regular_session(workspace_snapshot, session_id)
            })
    };
    if !report_is_regular_session {
        return apply_task_session_execution_report_to_memory(state, report);
    }
    let prompt_purpose = report
        .preparation
        .as_ref()
        .map(|preparation| preparation.prompt_purpose)
        .unwrap_or(ExecutionPromptPurpose::UserTurn);
    let runs_completed_turn_hooks = prompt_purpose.runs_completed_turn_hooks();
    // Measures completion-gate inspection for active hooks and descendant subagents.
    // 衡量完成闸门对活跃 hook 与后代子代理的检查耗时。
    let completion_gate_started_at = current_unix_millis_value();
    let completion_gate = webui_turn_completion_gate_from_state(state, agent_runtime, session_id)?;
    webui_trace_if_slow(
        "execution_report_completion_gate_slow",
        completion_gate_started_at,
        format!("session={session_id}"),
    );
    let mut completion_request = None;
    // Measures in-memory workspace mutation for the terminal execution report.
    // 衡量终态执行报告写入内存工作区的耗时。
    let workspace_apply_started_at = current_unix_millis_value();
    let (workspace_snapshot, terminal_run_phase, terminal_status_label) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        {
            let now = current_unix_millis_text();
            // Main-session membership decides whether completion hooks and VMM side effects may run.
            // 主会话成员关系决定完成 hook 与 VMM 副作用是否允许执行。
            let is_main_session = workspace
                .sessions
                .iter()
                .any(|candidate| candidate.id == session_id);
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            let apply_result =
                apply_terminal_execution_report_to_state(runtime_state, session_id, &now, &report);
            let project_id = workspace
                .sessions
                .iter()
                .find(|candidate| candidate.id == session_id)
                .and_then(|session| session.project_id.clone());
            let conversation_state = runtime_state.conversation_state.clone();
            let should_dispatch_completion_hook = is_main_session
                && apply_result.is_success
                && (runs_completed_turn_hooks || completion_gate.has_active_hook_turn);
            let should_terminate_completion_hook = is_main_session
                && !apply_result.is_success
                && (runs_completed_turn_hooks || completion_gate.has_active_hook_turn);
            let completion_status = completion_gate.completion_status();
            if should_dispatch_completion_hook {
                let hook_turn_id =
                    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
                        let (hook_turn_id, request) = hook_runtime
                        .prepare_turn_completion_dispatch_with_status(
                        session_id,
                        project_id.clone(),
                        report.assistant_output.as_deref().unwrap_or(
                            "The model completed the run but did not return visible text output.",
                        ),
                        report.provider_request_count,
                        &conversation_state,
                        completion_status,
                        completion_gate.pending_subagent_count,
                    );
                        completion_request = Some(request);
                        hook_turn_id
                    } else {
                        String::new()
                    };
                if completion_status.is_final() {
                    runtime_state
                        .context_injection_state
                        .consume_turn(&hook_turn_id, &now);
                    runtime_state.vmm_profile_sync.record_completed_turn();
                    let profile_sync = runtime_state.vmm_profile_sync.clone();
                    agent_runtime
                        .vmm_hook_runtime
                        .store_profile_sync_state(session_id, profile_sync);
                }
            } else if should_terminate_completion_hook
                && let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock()
            {
                hook_runtime.terminate_turn(
                    session_id,
                    project_id,
                    "execution_failed",
                    report.provider_request_count,
                );
            }
            if let Some(session) = workspace
                .sessions
                .iter_mut()
                .find(|candidate| candidate.id == session_id)
            {
                session.status = if apply_result.is_success {
                    SessionState::Completed
                } else {
                    SessionState::Blocked
                };
                session.last_updated_label = String::from("刚刚");
            }
            if workspace.selected_session != session_id {
                runtime_state.ui_state.unread = true;
            }
            let runtime_state_snapshot = runtime_state.clone();
            merge_runtime_state_records_into_display_messages(
                workspace,
                session_id,
                &runtime_state_snapshot,
            );
            workspace.session_run_phases.insert(
                session_id,
                if apply_result.is_success {
                    SessionRunPhase::Ready
                } else {
                    SessionRunPhase::Failed
                },
            );
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            if is_main_session {
                agent_runtime
                    .vmm_hook_runtime
                    .replace_snapshot(build_vmm_hook_config_snapshot(
                        &agent_runtime.provider_configuration,
                        workspace,
                    ));
            }
            let terminal_run_phase = if apply_result.is_success {
                SessionRunPhase::Ready
            } else {
                SessionRunPhase::Failed
            };
            let terminal_status_label = if apply_result.is_success {
                SessionState::Completed.label().to_string()
            } else {
                SessionState::Blocked.label().to_string()
            };
            (workspace.clone(), terminal_run_phase, terminal_status_label)
        }
    };
    webui_trace_if_slow(
        "execution_report_workspace_apply_slow",
        workspace_apply_started_at,
        format!("session={session_id}"),
    );
    emit_terminal_execution_report_workbench_deltas(
        app,
        state,
        session_id,
        terminal_run_phase,
        terminal_status_label.as_str(),
    )?;
    webui_trace(
        "execution_report_applied",
        format!(
            "session={session_id} is_main_session={} prompt_purpose={} runs_completed_turn_hooks={} completion_status={} pending_subagents={} has_active_hook_turn={} error={} provider_requests={} output_chars={}",
            workspace_snapshot
                .sessions
                .iter()
                .any(|candidate| candidate.id == session_id),
            prompt_purpose.diagnostics_label(),
            runs_completed_turn_hooks,
            completion_gate.completion_status().as_str(),
            completion_gate.pending_subagent_count,
            completion_gate.has_active_hook_turn,
            report.error.is_some(),
            report.provider_request_count,
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );
    Ok(TerminalReportMemoryApply {
        workspace_snapshot,
        completion_request,
    })
}

/// Emits terminal-report deltas without rebuilding the full workbench snapshot.
/// 在不重建完整工作台快照的情况下发出终态报告增量。
///
/// # Parameters
/// 参数
/// `app` emits delta events to connected WebUI clients.
/// `app` 将增量事件发给已连接的 WebUI 客户端。
/// `state` owns the retained workbench and workspace projections.
/// `state` 持有保留的工作台与工作区投影。
/// `session_id` identifies the terminal session.
/// `session_id` 标识进入终态的会话。
/// `run_phase` is the terminal run phase to expose.
/// `run_phase` 是需要展示的终态运行阶段。
/// `status_label` is the sidebar status label paired with the terminal phase.
/// `status_label` 是与终态阶段配套的侧栏状态标签。
///
/// # Returns
/// 返回值
/// `Ok(())` after all required deltas have been emitted.
/// 在发出所有必要增量后返回 `Ok(())`。
fn emit_terminal_execution_report_workbench_deltas(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    run_phase: SessionRunPhase,
    status_label: &str,
) -> Result<(), String> {
    let conversation_id = session_id.to_string();
    let events = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let surface_matches =
            workbench_surface_matches_conversation(&guard.workbench, &conversation_id);
        let before_visible_messages = guard
            .workbench
            .messages
            .iter()
            .filter(|message| message.conversation_id == conversation_id)
            .cloned()
            .collect::<Vec<_>>();
        let messages = if surface_matches {
            workspace
                .session_messages
                .get(&session_id)
                .map(|entries| conversation_messages_from_session(&conversation_id, entries, false))
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .unwrap_or_default();
        let ai_tool_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.ai_tool_configuration.clone())
            .unwrap_or_default();
        let context_usage = if surface_matches {
            context_usage_snapshot_from_runtime_state(
                session_id,
                workspace.session_runtime_state.get(&session_id),
                &guard.live_context_budgets,
                &provider_configuration,
                &ai_tool_configuration,
            )
        } else {
            None
        };
        let mut events = Vec::new();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let phase_generation = guard.workbench_generation;
        guard.workbench.revision = phase_generation;
        for conversation in &mut guard.workbench.conversations {
            if conversation.id == conversation_id {
                conversation.run_phase = run_phase.to_string();
                conversation.status = status_label.to_string();
                conversation.last_updated_label = String::from("刚刚");
                conversation.last_updated_at = current_unix_millis_text();
                break;
            }
        }
        events.push(WorkbenchDeltaEnvelope {
            generation: phase_generation,
            delta: WorkbenchDelta::UpdateConversationRunPhase(UpdateConversationRunPhaseDelta {
                conversation_id: conversation_id.clone(),
                run_phase: run_phase.to_string(),
                status: Some(status_label.to_string()),
                last_updated_label: Some(String::from("now")),
            }),
        });
        if surface_matches {
            if let Some(message_patch) = build_conversation_messages_patched_delta(
                &conversation_id,
                before_visible_messages.as_slice(),
                messages.as_slice(),
            ) {
                guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
                let message_generation = guard.workbench_generation;
                guard.workbench.revision = message_generation;
                replace_conversation_messages_in_workbench_window(
                    &mut guard.workbench.messages,
                    &conversation_id,
                    messages,
                );
                events.push(WorkbenchDeltaEnvelope {
                    generation: message_generation,
                    delta: WorkbenchDelta::ConversationMessagesPatched(message_patch),
                });
            }
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            let context_generation = guard.workbench_generation;
            guard.workbench.revision = context_generation;
            guard.workbench.context_usage = context_usage.clone();
            events.push(WorkbenchDeltaEnvelope {
                generation: context_generation,
                delta: WorkbenchDelta::ContextUsageChanged(ContextUsageChangedDelta {
                    context_usage,
                }),
            });
        }
        events
    };
    for event in events {
        emit_service_event_with_trace(
            app,
            "workbench_terminal_report_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    Ok(())
}

/// Spawns the completed-turn hook without blocking queued prompt dispatch.
/// 启动完成回合 hook，避免阻塞排队提示词分发。
///
/// # Parameters
/// 参数
/// `app` is the Tauri app handle used to emit any later VMM outbox updates.
/// `app` 是用于发出后续 VMM outbox 更新的 Tauri 应用句柄。
/// `state` owns the retained workspace snapshot that receives hook side effects.
/// `state` 持有接收 hook 副作用的保留工作区快照。
/// `controller_runtime` persists VMM outbox changes after the hook finishes.
/// `controller_runtime` 在 hook 完成后持久化 VMM outbox 变更。
/// `agent_runtime` provides VMM hook snapshot refresh state.
/// `agent_runtime` 提供 VMM hook 快照刷新状态。
/// `completion_request` is the prepared completed-turn hook dispatch request.
/// `completion_request` 是已准备的完成回合 hook 调度请求。
pub(super) fn spawn_conversation_completion_hook_task(
    _app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    completion_request: ConversationCompletionRequest,
) {
    spawn_service_task(async move {
        let outcome = completion_request.run().await;
        let session_id = outcome.session_id;
        if let Err(error) = outcome.report.as_ref() {
            eprintln!(
                "[webui-service] completed-turn hook failed for {}: {}",
                outcome.turn_id, error
            );
        }
        if outcome.vmm_writeback_effects.is_empty() {
            return;
        }
        let workspace_snapshot = {
            let mut guard = match state.lock() {
                Ok(guard) => guard,
                Err(error) => {
                    eprintln!("failed to lock service state: {error}");
                    return;
                }
            };
            let workspace = match guard.workspace_snapshot.as_mut() {
                Some(workspace) => workspace,
                None => {
                    eprintln!(
                        "[webui-service] apply completed-turn hook effects failed: workspace snapshot is not ready"
                    );
                    return;
                }
            };
            let now = current_unix_millis_text();
            if let Some(runtime_state) = workspace.session_runtime_state.get_mut(&session_id) {
                apply_vmm_writeback_effects_to_runtime_state(
                    runtime_state,
                    outcome.vmm_writeback_effects,
                    &now,
                );
                runtime_state.updated_at = now;
            }
            agent_runtime
                .vmm_hook_runtime
                .replace_snapshot(build_vmm_hook_config_snapshot(
                    &agent_runtime.provider_configuration,
                    workspace,
                ));
            workspace.clone()
        };
        if let Err(error) = persist_workspace_session_update(
            Some(&state),
            controller_runtime,
            &workspace_snapshot,
            session_id,
        )
        .await
        {
            eprintln!("[webui-service] persist completed-turn hook effects failed: {error}");
        }
    });
}
