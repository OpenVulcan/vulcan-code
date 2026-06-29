//! Subagent panel WebUI service handlers.
//! 子代理面板 WebUI 服务处理器。

use super::*;

/// 打开当前父会话下的单个子代理转录界面。
pub(super) async fn service_open_subagent_panel(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    parent_conversation_id: String,
    task_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let _ = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid subagent task id '{task_id}': {error}"))?;
    webui_trace(
        "open_subagent_panel_requested",
        format!("parent={parent_session_id} task={task_session_id}"),
    );
    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if guard.active_subagent_panel
            == Some(SelectedSubagentTaskPanel {
                parent_session_id,
                task_id: task_session_id,
            })
        {
            webui_trace(
                "open_subagent_panel_noop_active",
                format!("parent={parent_session_id} task={task_session_id}"),
            );
            return Ok(guard.workbench.clone());
        }
    }
    let cache_open_started_at = current_unix_millis_value();
    let fast_path_workbench = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let mut workspace_snapshot = guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        restore_webui_subagent_surface_detail_cache_for_task(
            &mut guard,
            &mut workspace_snapshot,
            parent_session_id,
            task_session_id,
        );
        let opened_from_cache = match open_webui_subagent_panel_from_cached_state(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
            task_session_id,
        ) {
            Ok(opened_from_cache) => opened_from_cache,
            Err(error) => {
                guard.workspace_snapshot = Some(workspace_snapshot);
                return Err(error);
            }
        };
        let workbench = opened_from_cache.then(|| {
            build_subagent_display_route_workbench_snapshot_locked(&mut guard, &workspace_snapshot)
        });
        guard.workspace_snapshot = Some(workspace_snapshot);
        workbench
    };
    webui_trace_if_slow(
        "open_subagent_panel_cache_slow",
        cache_open_started_at,
        format!(
            "parent={parent_session_id} task={task_session_id} opened_from_cache={}",
            fast_path_workbench.is_some()
        ),
    );
    if let Some(workbench) = fast_path_workbench {
        let emit_started_at = current_unix_millis_value();
        emit_workbench_event(&app, &workbench);
        webui_trace_if_slow(
            "open_subagent_panel_emit_slow",
            emit_started_at,
            format!("parent={parent_session_id} task={task_session_id} source=cache"),
        );
        return Ok(workbench);
    }
    let mut workspace_snapshot =
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
    let detail_load_started_at = current_unix_millis_value();
    let detailed_state = segmented_webui_subagent_task_panel_state_for_task(
        &workspace_snapshot,
        parent_session_id,
        task_session_id,
    );
    webui_trace_if_slow(
        "open_subagent_panel_detail_load_slow",
        detail_load_started_at,
        format!(
            "parent={parent_session_id} task={task_session_id} found={}",
            detailed_state.is_some()
        ),
    );
    {
        let cache_merge_started_at = current_unix_millis_value();
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if let Some(detailed_state) = detailed_state {
            cache_webui_subagent_surface_detail_for_task(
                &mut guard,
                &mut workspace_snapshot,
                parent_session_id,
                detailed_state.clone(),
            );
            merge_webui_subagent_task_cache_for_session(
                &mut guard,
                parent_session_id,
                vec![detailed_state],
            );
        }
        if !open_webui_subagent_panel_from_cached_state(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
            task_session_id,
        )? {
            webui_trace(
                "open_subagent_panel_missing",
                format!("parent={parent_session_id} task={task_session_id}"),
            );
            return Err(String::from("没有找到可打开的子代理任务。"));
        }
        webui_trace_if_slow(
            "open_subagent_panel_cache_merge_slow",
            cache_merge_started_at,
            format!("parent={parent_session_id} task={task_session_id}"),
        );
    }
    let emit_started_at = current_unix_millis_value();
    let snapshot = emit_subagent_display_route_workbench_snapshot(&app, &state, workspace_snapshot);
    webui_trace_if_slow(
        "open_subagent_panel_emit_slow",
        emit_started_at,
        format!("parent={parent_session_id} task={task_session_id} source=detail"),
    );
    snapshot
}

/// Opens a WebUI subagent panel from the in-memory task cache without touching disk.
/// 从内存任务缓存打开 WebUI 子代理面板，不触碰磁盘。
pub(super) fn open_webui_subagent_panel_from_cached_state(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Result<bool, String> {
    let Some(task_state) = guard
        .session_subagent_tasks
        .get(&parent_session_id)
        .and_then(|tasks| tasks.get(&task_session_id))
        .cloned()
    else {
        return Ok(false);
    };
    let has_live_listener = guard
        .detached_subagent_task_listeners
        .contains(&task_session_id);
    let task_runtime = guard
        .agent_runtime
        .as_ref()
        .map(|runtime| &runtime.task_runtime);
    let is_live_running = webui_subagent_task_has_live_run_in_workspace(
        Some(workspace_snapshot),
        task_runtime,
        has_live_listener,
        task_session_id,
    );
    let normalized = normalize_subagent_task_panel_state(&task_state, is_live_running, None);
    let has_recoverable_detail =
        webui_subagent_panel_has_recoverable_detail(workspace_snapshot, &normalized);
    let is_openable = subagent_task_panel_is_openable(&normalized, has_recoverable_detail);
    webui_trace(
        "open_subagent_panel_evaluated",
        format!(
            "parent={parent_session_id} task={task_session_id} run_state={} has_recoverable_detail={has_recoverable_detail} is_openable={is_openable} messages={}",
            subagent_run_state_key(normalized.run_state),
            workspace_snapshot
                .session_messages
                .get(&task_session_id)
                .map(Vec::len)
                .unwrap_or(0)
        ),
    );
    if !is_openable {
        return Ok(false);
    }
    guard
        .session_subagent_tasks
        .entry(parent_session_id)
        .or_default()
        .insert(task_session_id, normalized);
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: task_session_id,
    });
    Ok(true)
}

/// Closes the active subagent transcript surface and returns to the parent conversation.
/// 关闭活动子代理转录界面并返回父会话。
pub(super) async fn service_close_subagent_panel(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let _ = runtime_parts_from_state(&state)?;
    let workspace_snapshot = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        webui_trace(
            "close_subagent_panel_requested",
            format!(
                "active_parent={} active_task={}",
                guard
                    .active_subagent_panel
                    .map(|selection| selection.parent_session_id.to_string())
                    .unwrap_or_else(|| String::from("<none>")),
                guard
                    .active_subagent_panel
                    .map(|selection| selection.task_id.to_string())
                    .unwrap_or_else(|| String::from("<none>"))
            ),
        );
        guard.active_subagent_panel = None;
        guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
    emit_subagent_display_route_workbench_snapshot(&app, &state, workspace_snapshot)
}

/// Stops one running subagent task through the direct service runtime path.
/// 通过独立服务运行时路径停止单个运行中的子代理任务。
pub(super) async fn service_stop_subagent_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    parent_conversation_id: String,
    task_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid subagent task id '{task_id}': {error}"))?;
    webui_trace(
        "stop_subagent_requested",
        format!("parent={parent_session_id} task={task_session_id}"),
    );
    let task_states_to_mark = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.take().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let root_state = guard
            .session_subagent_tasks
            .get(&parent_session_id)
            .and_then(|tasks| tasks.get(&task_session_id))
            .cloned();
        let mut task_states_to_mark = vec![(parent_session_id, task_session_id, root_state)];
        task_states_to_mark.extend(
            running_webui_subagent_task_states_for_tree_from_cache(
                &mut guard,
                &workspace_snapshot,
                task_session_id,
            )
            .into_iter()
            .map(|(direct_parent_session_id, task_state)| {
                (
                    direct_parent_session_id,
                    task_state.task_id,
                    Some(task_state),
                )
            }),
        );
        guard.workspace_snapshot = Some(workspace_snapshot);
        task_states_to_mark
    };
    let stopped_task_ids = agent_runtime.task_runtime.stop_task_tree(task_session_id)?;
    webui_trace(
        "stop_subagent_result",
        format!(
            "parent={parent_session_id} task={task_session_id} stopped_count={}",
            stopped_task_ids.len()
        ),
    );
    let stopped_task_id_set = stopped_task_ids.iter().copied().collect::<BTreeSet<_>>();
    let stopped_parent_session_ids = task_states_to_mark
        .iter()
        .filter(|(_, task_id, _)| stopped_task_id_set.contains(task_id))
        .map(|(direct_parent_session_id, _, _)| *direct_parent_session_id)
        .collect::<BTreeSet<_>>();
    let workspace_snapshot = if !stopped_task_id_set.is_empty() {
        with_workspace_mutation(&app, &state, |workspace| {
            for (direct_parent_session_id, task_id, task_state) in &task_states_to_mark {
                if !stopped_task_id_set.contains(task_id) {
                    continue;
                }
                mark_webui_subagent_task_stopped_in_workspace(
                    workspace,
                    *direct_parent_session_id,
                    *task_id,
                    task_state.as_ref(),
                );
            }
        })?
    } else {
        current_workspace_snapshot_from_state(&state)?.ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?
    };
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !stopped_task_id_set.is_empty() {
            let stopped_task_pairs = task_states_to_mark
                .iter()
                .filter(|(_, task_id, _)| stopped_task_id_set.contains(task_id))
                .map(|(direct_parent_session_id, task_id, _)| (*direct_parent_session_id, *task_id))
                .collect::<Vec<_>>();
            mark_webui_subagent_tasks_stopped_in_state(&mut guard, &stopped_task_pairs);
            enqueue_stopped_subagent_mailbox_notifications(
                &mut guard,
                &task_states_to_mark,
                &stopped_task_id_set,
            );
            for task_id in &stopped_task_id_set {
                guard.detached_subagent_task_listeners.remove(task_id);
                if guard
                    .active_subagent_panel
                    .is_some_and(|selection| selection.task_id == *task_id)
                {
                    guard.active_subagent_panel = None;
                }
            }
        } else {
            guard
                .detached_subagent_task_listeners
                .remove(&task_session_id);
        }
    }
    let workbench = store_current_workbench_snapshot(
        &app,
        &state,
        &controller_runtime,
        &agent_runtime,
        workspace_snapshot,
    )?;
    if !stopped_parent_session_ids.is_empty() {
        let state_for_persist = state.clone();
        let controller_runtime_for_persist = controller_runtime.clone();
        spawn_service_task(async move {
            let snapshot = match current_workspace_snapshot_from_shared_state(&state_for_persist) {
                Ok(Some(snapshot)) => snapshot,
                Ok(None) => return,
                Err(error) => {
                    eprintln!("[webui-service] stopped subagent persist snapshot failed: {error}");
                    return;
                }
            };
            for parent_session_id in stopped_parent_session_ids {
                if !snapshot
                    .sessions
                    .iter()
                    .any(|session| session.id == parent_session_id)
                {
                    continue;
                }
                if let Err(error) = persist_workspace_session_update(
                    Some(&state_for_persist),
                    controller_runtime_for_persist.clone(),
                    &snapshot,
                    parent_session_id,
                )
                .await
                {
                    eprintln!(
                        "[webui-service] stopped subagent background persist failed: {error}"
                    );
                }
            }
        });
    }
    Ok(workbench)
}

/// Enqueues stopped child-agent notifications into each owning parent mailbox once.
/// 将已停止子代理通知一次性加入其所属父会话 mailbox。
fn enqueue_stopped_subagent_mailbox_notifications(
    guard: &mut ServiceBridgeState,
    task_states_to_mark: &[(SessionId, SessionId, Option<SubagentTaskPanelState>)],
    stopped_task_id_set: &BTreeSet<SessionId>,
) {
    let updated_at = current_unix_millis_text();
    let mut notified_pairs = BTreeSet::new();
    for (direct_parent_session_id, task_id, task_state) in task_states_to_mark {
        if !stopped_task_id_set.contains(task_id)
            || !notified_pairs.insert((*direct_parent_session_id, *task_id))
        {
            continue;
        }
        let metadata = build_stopped_task_session_metadata(
            None,
            task_state.as_ref(),
            *task_id,
            SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
            &updated_at,
        );
        let accepted = enqueue_webui_subagent_mailbox_input(
            guard,
            *direct_parent_session_id,
            *task_id,
            &metadata,
        );
        webui_trace(
            "stopped_subagent_mailbox_notification",
            format!("parent={direct_parent_session_id} task={task_id} accepted={accepted}"),
        );
    }
}
