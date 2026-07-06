//! Subagent panel WebUI service handlers.
//! 子代理面板 WebUI 服务处理器。

use super::*;
use vulcan_tools_manager::VulcanToolInvocationResult;

/// Emits the navigation part of one subagent panel route response.
/// 发出单个子代理面板路由响应中的导航部分。
fn emit_subagent_navigation_delta_from_route_response(
    app: &ServiceEventEmitter,
    response: &SubagentPanelRouteResponse,
) {
    emit_service_event_with_trace(
        app,
        "subagent_navigation_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(WorkbenchDeltaEnvelope {
            generation: response.revision,
            delta: WorkbenchDelta::SubagentNavigationChanged(SubagentNavigationChangedDelta {
                parent_conversation_id: response.parent_conversation_id.clone().or_else(|| {
                    response
                        .subagent_navigation
                        .as_ref()
                        .map(|navigation| navigation.parent_conversation_id.clone())
                }),
                subagent_navigation: response.subagent_navigation.clone(),
            }),
        })),
    );
}

/// Direct parent, child task id, and optional cached panel state used when marking stopped subagents.
/// 标记已停止子代理时使用的直接父会话、子任务标识与可选缓存面板状态。
type SubagentTaskStopMark = (SessionId, SessionId, Option<SubagentTaskPanelState>);

/// Opens one subagent transcript panel from the already-hydrated in-memory session state.
/// 从已水合的内存会话状态中打开单个子智能体转录面板。
///
/// # Parameters
/// 参数
/// `app` emits the navigation delta after the panel route is resolved.
/// `app` 在面板路由解析完成后发出导航增量。
/// `state` owns the shared service caches and selected workspace snapshot.
/// `state` 持有共享服务缓存与当前选中的工作区快照。
/// `parent_conversation_id` is the WebUI parent conversation identifier.
/// `parent_conversation_id` 是 WebUI 父会话标识。
/// `task_id` is the child task session identifier requested by the panel tab.
/// `task_id` 是面板标签请求打开的子任务会话标识。
///
/// # Returns
/// 返回值
/// A route response built only from memory, or a friendly initialization/closed-state error.
/// 返回仅由内存构建的路由响应，或友好的未初始化/已关闭状态错误。
pub(super) async fn service_open_subagent_panel(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    parent_conversation_id: String,
    task_id: String,
) -> Result<SubagentPanelRouteResponse, String> {
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
    let cache_open_started_at = current_unix_millis_value();
    let response = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let opened_parent_session_id = match open_webui_subagent_panel_from_authoritative_state(
            &guard,
            &workspace_snapshot,
            parent_session_id,
            task_session_id,
        ) {
            Ok(opened_parent_session_id) => opened_parent_session_id,
            Err(error) => {
                return Err(error);
            }
        };
        let Some(resolved_parent_session_id) = opened_parent_session_id else {
            webui_trace(
                "open_subagent_panel_missing",
                format!("parent={parent_session_id} task={task_session_id} source=registry"),
            );
            return Err(String::from(
                "子智能体会话尚未完成初始化，或该智能体已经关闭。",
            ));
        };
        build_subagent_panel_route_response_for_task_locked(
            &mut guard,
            &workspace_snapshot,
            resolved_parent_session_id,
            task_session_id,
        )
    };
    webui_trace_if_slow(
        "open_subagent_panel_cache_slow",
        cache_open_started_at,
        format!("parent={parent_session_id} task={task_session_id} opened_from_memory=true"),
    );
    let emit_started_at = current_unix_millis_value();
    emit_subagent_navigation_delta_from_route_response(&app, &response);
    webui_trace_if_slow(
        "open_subagent_panel_emit_slow",
        emit_started_at,
        format!("parent={parent_session_id} task={task_session_id} source=memory"),
    );
    Ok(response)
}

/// Resolves an open WebUI subagent panel against the authoritative runtime registry.
/// 通过权威运行时注册表解析一个可打开的 WebUI 子智能体面板。
///
/// # Parameters
/// 参数
/// `guard` provides the shared runtime registry and render-only detail cache.
/// `guard` 提供共享运行时注册表与仅用于渲染的详情缓存。
/// `workspace_snapshot` provides already-hydrated transcript messages for the selected session.
/// `workspace_snapshot` 提供当前选中会话已水合的转录消息。
/// `root_parent_session_id` is the selected root or direct parent session id.
/// `root_parent_session_id` 是选中根会话或直接父会话标识。
/// `task_session_id` identifies the child task panel requested by the WebUI.
/// `task_session_id` 标识 WebUI 请求打开的子任务面板。
///
/// # Returns
/// 返回值
/// The direct parent session id when the registry says the task is open and reviewable.
/// 当注册表确认该任务未关闭且可复看时返回直接父会话标识。
fn open_webui_subagent_panel_from_authoritative_state(
    guard: &ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    root_parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Result<Option<SessionId>, String> {
    let Some(entry) =
        open_subagent_registry_entry_for_task(guard, root_parent_session_id, task_session_id)?
    else {
        return Ok(None);
    };
    let has_workspace_messages =
        display_session_messages_from_workspace(workspace_snapshot, task_session_id)
            .is_some_and(|messages| !messages.is_empty());
    let has_cached_detail = guard
        .subagent_surface_detail_cache
        .get(&task_session_id)
        .is_some_and(|cached| {
            !cached.messages.is_empty()
                || subagent_task_panel_state_has_recoverable_conversation(&cached.task_state)
        });
    if !entry.status.is_running() && !has_workspace_messages && !has_cached_detail {
        return Ok(None);
    }
    Ok(Some(entry.parent_session_id.as_uuid()))
}

/// Builds a subagent route response with a temporary active task selection.
/// 使用临时活动任务选择构建子代理路由响应。
///
/// # Parameters
/// 参数
/// `guard` is the shared service state guard being projected.
/// `guard` 是正在投影的共享服务状态锁。
/// `workspace_snapshot` is the workspace snapshot used for transcript lookup.
/// `workspace_snapshot` 是用于查找转录的工作区快照。
/// `parent_session_id` is the parent conversation that owns the child task.
/// `parent_session_id` 是拥有子任务的父会话。
/// `task_session_id` is the child task conversation to render.
/// `task_session_id` 是需要渲染的子任务会话。
///
/// # Returns
/// 返回值
/// A route response for the requested child task without persisting UI active state globally.
/// 返回请求子任务的路由响应，且不会全局持久化 UI 活动状态。
fn build_subagent_panel_route_response_for_task_locked(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> SubagentPanelRouteResponse {
    let previous_active_panel = guard.active_subagent_panel;
    let previous_subagent_navigation = guard.workbench.subagent_navigation.clone();
    let previous_messages = guard.workbench.messages.clone();
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: task_session_id,
    });
    let response = build_subagent_panel_route_response_locked(guard, workspace_snapshot);
    guard.active_subagent_panel = previous_active_panel;
    guard.workbench.subagent_navigation = previous_subagent_navigation;
    guard.workbench.messages = previous_messages;
    response
}

/// Handles the legacy close-subagent RPC without mutating global panel state.
/// 处理旧版关闭子代理 RPC，且不变更全局面板状态。
pub(super) async fn service_close_subagent_panel(
    _app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<SubagentPanelRouteResponse, String> {
    let _ = runtime_parts_from_state(&state)?;
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    webui_trace(
        "close_subagent_panel_local_noop",
        String::from("scope=client"),
    );
    Ok(SubagentPanelRouteResponse {
        parent_conversation_id: guard.workbench.selected_conversation_id.clone(),
        active_conversation_id: guard.workbench.selected_conversation_id.clone(),
        subagent_navigation: None,
        messages: Vec::new(),
        revision: guard.workbench.revision,
    })
}

/// Stops one running subagent task through the direct service runtime path.
/// 通过独立服务运行时路径停止单个运行中的子代理任务。
pub(super) async fn service_stop_subagent_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    parent_conversation_id: String,
    task_id: String,
) -> Result<SubagentPanelRouteResponse, String> {
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
    let (resolved_parent_session_id, task_states_to_mark) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let resolved_parent_session_id =
            open_subagent_registry_entry_for_task(&guard, parent_session_id, task_session_id)?
                .map(|entry| entry.parent_session_id.as_uuid())
                .unwrap_or(parent_session_id);
        let root_state = guard
            .subagent_surface_detail_cache
            .get(&task_session_id)
            .map(|entry| entry.task_state.clone());
        // Capture stop candidates before aborting runtime tasks, because aborting updates the live registry.
        // 在中止运行时任务前捕获停止候选，因为中止会更新实时注册表。
        let mut task_states_to_mark: Vec<SubagentTaskStopMark> =
            vec![(resolved_parent_session_id, task_session_id, root_state)];
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
        (resolved_parent_session_id, task_states_to_mark)
    };
    let stopped_task_ids = agent_runtime.task_runtime.stop_task_tree(task_session_id)?;
    webui_trace(
        "stop_subagent_result",
        format!(
            "parent={parent_session_id} resolved_parent={resolved_parent_session_id} task={task_session_id} stopped_count={}",
            stopped_task_ids.len()
        ),
    );
    let stopped_task_id_set = stopped_task_ids.iter().copied().collect::<BTreeSet<_>>();
    // Reuse one filtered list for every cleanup path that needs the cached panel state.
    // 复用同一份过滤结果，供所有需要缓存面板状态的清理路径使用。
    let stopped_task_marks = task_states_to_mark
        .into_iter()
        .filter(|(_, task_id, _)| stopped_task_id_set.contains(task_id))
        .collect::<Vec<_>>();
    let stopped_parent_session_ids = stopped_task_marks
        .iter()
        .map(|(direct_parent_session_id, _, _)| *direct_parent_session_id)
        .collect::<BTreeSet<_>>();
    let response = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !stopped_task_id_set.is_empty() {
            {
                let workspace = guard
                    .workspace_snapshot
                    .as_mut()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
                for (direct_parent_session_id, task_id, task_state) in &stopped_task_marks {
                    mark_webui_subagent_task_stopped_in_workspace(
                        workspace,
                        *direct_parent_session_id,
                        *task_id,
                        task_state.as_ref(),
                    );
                }
            }
            let stopped_task_pairs = stopped_task_marks
                .iter()
                .map(|(direct_parent_session_id, task_id, _)| (*direct_parent_session_id, *task_id))
                .collect::<Vec<_>>();
            mark_webui_subagent_tasks_stopped_in_state(&mut guard, &stopped_task_pairs);
            enqueue_stopped_subagent_mailbox_notifications(&mut guard, &stopped_task_marks);
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
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        build_subagent_panel_route_response_for_task_locked(
            &mut guard,
            &workspace_snapshot,
            resolved_parent_session_id,
            task_session_id,
        )
    };
    emit_subagent_navigation_delta_from_route_response(&app, &response);
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
    Ok(response)
}

/// Closes one open subagent task through the canonical runtime close_agent path.
/// 通过规范运行时 close_agent 路径关闭一个未关闭的子智能体任务。
pub(super) async fn service_close_subagent_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    parent_conversation_id: String,
    task_id: String,
) -> Result<SubagentPanelRouteResponse, String> {
    let (_, agent_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid subagent task id '{task_id}': {error}"))?;
    let (resolved_parent_session_id, public_task_id) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let entry = open_subagent_registry_entry_for_task(
            &guard,
            parent_session_id,
            task_session_id,
        )?
        .ok_or_else(|| {
            format!(
                "subagent task `{task_session_id}` is not open under parent `{parent_session_id}`"
            )
        })?;
        (entry.parent_session_id.as_uuid(), entry.agent_id)
    };
    webui_trace(
        "close_subagent_task_requested",
        format!(
            "parent={parent_session_id} resolved_parent={resolved_parent_session_id} task={task_session_id} public_task_id={public_task_id}"
        ),
    );
    let close_result = agent_runtime
        .task_runtime
        .invoke_close_agent(
            resolved_parent_session_id,
            json!({
                "target": public_task_id,
                "cascade": true,
            }),
        )
        .await?;
    let closed_task_ids = closed_subagent_task_ids_from_close_result(&close_result)?;
    webui_trace(
        "close_subagent_task_result",
        format!(
            "parent={parent_session_id} resolved_parent={resolved_parent_session_id} task={task_session_id} closed_count={}",
            closed_task_ids.len()
        ),
    );
    let response = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        remove_webui_closed_subagent_tasks_in_state(&mut guard, &closed_task_ids);
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        build_subagent_panel_route_response_locked(&mut guard, &workspace_snapshot)
    };
    emit_subagent_navigation_delta_from_route_response(&app, &response);
    Ok(response)
}

/// Extracts closed task session ids from the close_agent host result contract.
/// 从 close_agent 宿主结果契约中提取已关闭任务会话标识。
fn closed_subagent_task_ids_from_close_result(
    // Runtime close_agent invocation result.
    // 运行时 close_agent 调用结果。
    result: &VulcanToolInvocationResult,
) -> Result<Vec<SessionId>, String> {
    let payload = result
        .host_result
        .as_ref()
        .filter(|host_result| host_result.kind == "subagent_control")
        .map(|host_result| &host_result.payload)
        .ok_or_else(|| String::from("close_agent did not return a subagent_control host result"))?;
    let matched_agents = payload
        .get("matched_agents")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("close_agent host result is missing matched_agents"))?;
    let closed_task_ids = matched_agents
        .iter()
        .map(|agent| {
            agent
                .get(SUBAGENT_TASK_ID_KEY)
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    String::from("close_agent matched agent is missing subagent_task_id")
                })
                .and_then(|task_id| {
                    task_id.parse::<SessionId>().map_err(|error| {
                        format!(
                            "close_agent returned invalid subagent_task_id `{task_id}`: {error}"
                        )
                    })
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    if closed_task_ids.is_empty() {
        return Err(String::from(
            "close_agent did not report any closed subagent tasks",
        ));
    }
    Ok(closed_task_ids)
}

/// Enqueues stopped child-agent notifications into each owning parent mailbox once.
/// 将已停止子代理通知一次性加入其所属父会话 mailbox。
///
/// `stopped_task_marks` contains only stopped tasks with known direct-parent routing.
/// `stopped_task_marks` 仅包含已停止且具备直接父级路由信息的任务。
fn enqueue_stopped_subagent_mailbox_notifications(
    guard: &mut ServiceBridgeState,
    stopped_task_marks: &[SubagentTaskStopMark],
) {
    let updated_at = current_unix_millis_text();
    let mut notified_pairs = BTreeSet::new();
    for (direct_parent_session_id, task_id, task_state) in stopped_task_marks {
        if !notified_pairs.insert((*direct_parent_session_id, *task_id)) {
            continue;
        }
        let metadata = build_stopped_task_session_metadata(
            None,
            task_state.as_ref(),
            *task_id,
            SUBAGENT_TASK_STOPPED_SUMMARY_TEXT,
            &updated_at,
        );
        let outcome = enqueue_webui_subagent_mailbox_input(
            guard,
            *direct_parent_session_id,
            *task_id,
            &metadata,
        );
        webui_trace(
            "stopped_subagent_mailbox_notification",
            format!(
                "parent={direct_parent_session_id} task={task_id} outcome={}",
                outcome.trace_label()
            ),
        );
    }
}
