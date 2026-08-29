//! Subagent panel WebUI service handlers.
//! 子代理面板 WebUI 服务处理器。

use super::*;
use vulcan_tools_manager::VulcanToolInvocationResult;
use vulcan_workspace_store::workspace_store::{
    locate_embedded_task_session_v1, locate_session_v1_storage,
};

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

/// Opens one subagent transcript panel from its exact embedded TaskSession storage.
/// 从精确的嵌入式 TaskSession 存储中打开单个子智能体转录面板。
///
/// # Parameters
/// 参数
/// `state` owns the shared service caches and selected workspace snapshot.
/// `state` 持有共享服务缓存与当前选中的工作区快照。
/// `parent_conversation_id` is the WebUI parent conversation identifier.
/// `parent_conversation_id` 是 WebUI 父会话标识。
/// `task_id` is the child task session identifier requested by the panel tab.
/// `task_id` 是面板标签请求打开的子任务会话标识。
///
/// # Returns
/// 返回值
/// A route response after the exact child file is loaded, including for closed runtime agents.
/// 在精确子文件加载后返回路由响应，包括运行时已关闭的智能体。
pub(super) async fn service_open_subagent_panel(
    state: SharedServiceBridgeState,
    parent_conversation_id: String,
    task_id: String,
) -> Result<SubagentPanelRouteResponse, String> {
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid agent task id '{task_id}': {error}"))?;
    webui_trace(
        "open_subagent_panel_requested",
        format!("parent={parent_session_id} task={task_session_id}"),
    );
    let cache_open_started_at = current_unix_millis_value();
    let controller_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // controller_runtime resolves the indexed parent without admitting the closed child into memory.
        // controller_runtime 解析已索引父会话，且不会把已关闭子会话重新准入内存。
        guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?
    };
    let parent_locator =
        locate_session_v1_storage(&controller_runtime, &parent_session_id.to_string())
            .await
            .map_err(|error| format!("failed to locate parent V1 session: {error}"))?;
    let task_locator =
        locate_embedded_task_session_v1(&parent_locator, &task_session_id.to_string())
            .map_err(|error| format!("failed to locate embedded V1 task session: {error}"))?;
    // Reading the event-window state validates the exact file without creating or registering a SessionHandle.
    // 读取事件窗口状态会验证精确文件，但不会创建或注册 SessionHandle。
    task_locator
        .event_window_state()
        .map_err(|error| format!("failed to validate embedded V1 task session: {error}"))?;
    // response is route-only; the visible panel requests one immutable recent window separately.
    // response 仅负责路由；可见面板会另行请求一份不可变的最近窗口。
    let mut response = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.clone().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        build_readonly_subagent_panel_route_response_for_task_locked(
            &mut guard,
            &workspace_snapshot,
            parent_session_id,
            task_session_id,
        )?
    };
    // active_conversation_id is route-local transcript identity even when the closed task is absent from live navigation.
    // active_conversation_id 是路由局部转录身份，即使已关闭任务不在实时导航中也必须指向该子任务。
    response.active_conversation_id = Some(task_session_id.to_string());
    webui_trace_if_slow(
        "open_subagent_panel_cache_slow",
        cache_open_started_at,
        format!(
            "parent={parent_session_id} task={task_session_id} validated_exact_storage=true registered=false"
        ),
    );
    Ok(response)
}

/// Builds a subagent response with a temporary active task selection.
/// 使用临时活动任务选择构建子代理响应。
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
/// A route response for the requested child task without persisting the temporary active UI surface.
/// 返回请求子任务的路由响应，且不会持久化临时活动界面。
fn build_subagent_panel_route_response_for_task_locked(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Result<SubagentPanelRouteResponse, String> {
    // previous_active_panel preserves the route selection owned by the shared service state.
    // previous_active_panel 保留共享服务状态持有的路由选择。
    let previous_active_panel = guard.active_subagent_panel;
    // previous_subagent_navigation preserves the shared navigation projection across the temporary route build.
    // previous_subagent_navigation 在临时路由构建期间保留共享导航投影。
    let previous_subagent_navigation = guard.workbench.subagent_navigation.clone();
    guard.active_subagent_panel = Some(SelectedSubagentTaskPanel {
        parent_session_id,
        task_id: task_session_id,
    });
    // response captures the requested child projection before every temporary global field is restored.
    // response 在恢复全部临时全局字段前捕获请求的子任务投影。
    let response = build_subagent_panel_route_response_locked(guard, workspace_snapshot);
    guard.active_subagent_panel = previous_active_panel;
    guard.workbench.subagent_navigation = previous_subagent_navigation;
    response
}

/// Builds a read-only route-local subagent response without advancing global event ordering.
/// 构建不推进全局事件顺序的只读路由局部子代理响应。
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
/// A route-local response that inherits the unchanged global revision.
/// 返回继承未变化全局版本的路由局部响应。
pub(super) fn build_readonly_subagent_panel_route_response_for_task_locked(
    guard: &mut ServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    parent_session_id: SessionId,
    task_session_id: SessionId,
) -> Result<SubagentPanelRouteResponse, String> {
    // previous_workbench_generation prevents a read-only route response from advancing global event ordering.
    // previous_workbench_generation 防止只读路由响应推进全局事件顺序。
    let previous_workbench_generation = guard.workbench_generation;
    // previous_workbench_revision preserves the revision visible before the temporary projection.
    // previous_workbench_revision 保留临时投影前可见的版本号。
    let previous_workbench_revision = guard.workbench.revision;
    // response builds through the mutation-capable projector before its revision side effects are rolled back.
    // response 通过支持变更的投影器构建，随后回滚其版本副作用。
    let mut response = build_subagent_panel_route_response_for_task_locked(
        guard,
        workspace_snapshot,
        parent_session_id,
        task_session_id,
    );
    guard.workbench_generation = previous_workbench_generation;
    guard.workbench.revision = previous_workbench_revision;
    if let Ok(response) = response.as_mut() {
        // The route-local result inherits the unchanged global revision so later real deltas remain strictly newer.
        // 路由局部结果继承未变化的全局版本，使后续真实增量保持严格更新。
        response.revision = previous_workbench_revision;
    }
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
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid agent task id '{task_id}': {error}"))?;
    webui_trace(
        "stop_subagent_requested",
        format!("parent={parent_session_id} task={task_session_id}"),
    );
    let (resolved_parent_session_id, task_states_to_mark) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
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
            running_webui_subagent_task_states_for_tree_from_live_cache(
                &mut guard,
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
    let response = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        if !stopped_task_id_set.is_empty() {
            let stopped_task_pairs = stopped_task_marks
                .iter()
                .map(|(direct_parent_session_id, task_id, _)| (*direct_parent_session_id, *task_id))
                .collect::<Vec<_>>();
            mark_webui_subagent_tasks_stopped_in_state(&mut guard, &stopped_task_pairs);
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
    }?;
    emit_subagent_navigation_delta_from_route_response(&app, &response);
    // Child V1 runtime finalization owns durable stop facts; this workspace mutation is navigation-only.
    // 子会话 V1 运行时终结拥有持久化停止事实；此 workspace 变更仅用于导航显示。
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
    let (_, agent_runtime, _, _, _) = runtime_parts_from_state(&state)?;
    let parent_session_id = parent_conversation_id
        .parse::<SessionId>()
        .map_err(|error| {
            format!("invalid parent conversation id '{parent_conversation_id}': {error}")
        })?;
    let task_session_id = task_id
        .parse::<SessionId>()
        .map_err(|error| format!("invalid agent task id '{task_id}': {error}"))?;
    let (resolved_parent_session_id, public_task_id, session_registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let entry = open_subagent_registry_entry_for_task(
            &guard,
            parent_session_id,
            task_session_id,
        )?
        .ok_or_else(|| {
            format!("agent task `{task_session_id}` is not open under parent `{parent_session_id}`")
        })?;
        (
            entry.parent_session_id.as_uuid(),
            entry.agent_id,
            std::sync::Arc::clone(&guard.session_runtime_registry),
        )
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
            close_agent_request_arguments(public_task_id.as_str()),
        )
        .await?;
    let closed_task_ids = closed_subagent_task_ids_from_close_result(&close_result)?;
    for closed_task_id in &closed_task_ids {
        session_registry
            .unregister_lifecycle_session(*closed_task_id)
            .map_err(|error| {
                format!("failed to unload closed subagent V1 session '{closed_task_id}': {error}")
            })?;
    }
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
    }?;
    emit_subagent_navigation_delta_from_route_response(&app, &response);
    Ok(response)
}

/// Builds the exact argument object accepted by the current close_agent tool contract.
/// 构建当前 close_agent 工具契约接受的精确参数对象。
///
/// # Parameters
/// 参数
/// `public_task_id` is the child-agent id resolved from the open registry entry.
/// `public_task_id` 是从开放注册表条目解析出的子代理标识。
///
/// # Returns
/// 返回值
/// Returns a target-only payload because close_agent always archives the selected subtree.
/// 返回仅包含 target 的载荷，因为 close_agent 始终归档选中的子树。
pub(super) fn close_agent_request_arguments(public_task_id: &str) -> Value {
    json!({ "target": public_task_id })
}

/// Extracts closed task session ids from the close_agent host result contract.
/// 从 close_agent 宿主结果契约中提取已关闭任务会话标识。
fn closed_subagent_task_ids_from_close_result(
    // Runtime close_agent invocation result.
    // 运行时 close_agent 调用结果。
    result: &VulcanToolInvocationResult,
) -> Result<Vec<SessionId>, String> {
    let host_result = result
        .host_result
        .as_ref()
        .ok_or_else(|| String::from("close_agent did not return a subagent_control host result"))?;
    let projection =
        subagent_close_agent_projection_from_host_result(&host_result.kind, &host_result.payload)?
            .ok_or_else(|| String::from("close_agent did not return a close_agent host result"))?;
    Ok(projection.matched_task_ids)
}
