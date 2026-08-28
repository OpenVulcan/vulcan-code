//! Subagent workspace mutation helpers.
//! 子代理工作区变更辅助逻辑。

use super::*;

/// Hides the selected parent subagent navigation round once service-equivalent blocking work has settled.
/// 当与服务运行时等价的阻塞工作全部收敛后，隐藏所选父会话的子代理导航轮次。
///
/// # Parameters / 参数
///
/// - `state`: Shared service state owning runtime and navigation projections.
///   `state`：持有运行时与导航投影的共享服务状态。
/// - `parent_session_id`: Exact parent session whose navigation visibility is recomputed.
///   `parent_session_id`：需要重新计算导航可见性的精确父会话。
///
/// # Returns / 返回值
///
/// Success after the retained navigation projection is reconciled.
/// 保留导航投影完成协调后返回成功。
pub(crate) fn refresh_webui_subagent_navigation_round_visibility(
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let has_running =
        !running_webui_subagent_task_states_for_tree_from_live_cache(&mut guard, parent_session_id)
            .is_empty();
    let has_active_stream_task = guard.active_stream_tasks.contains_key(&parent_session_id);
    let has_active_kernel_turn = webui_session_kernel_has_active_turn(&guard, parent_session_id);
    let has_active_phase = guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace_snapshot| {
            workspace_snapshot
                .session_run_phases
                .get(&parent_session_id)
                .copied()
        })
        .is_some_and(is_active_session_run_phase);
    let should_remain_visible = subagent_navigation_round_should_remain_visible(
        has_active_stream_task || has_active_kernel_turn || has_active_phase,
        has_running,
    );
    webui_trace(
        "subagent_navigation_round_visibility",
        format!(
            "parent={parent_session_id} has_active_stream_task={has_active_stream_task} has_active_kernel_turn={has_active_kernel_turn} has_active_phase={has_active_phase} has_running={has_running} should_remain_visible={should_remain_visible}"
        ),
    );
    if !should_remain_visible {
        guard
            .session_subagent_navigation_rounds
            .remove(&parent_session_id);
    }
    Ok(())
}

/// Reprojects subagent navigation for one parent conversation and emits a focused navigation delta.
/// 为单个父会话重新投影子代理导航并发出聚焦导航增量。
///
/// # Parameters / 参数
///
/// - `app`: Service event emitter receiving the focused delta.
///   `app`：接收聚焦增量的服务事件发射器。
/// - `state`: Shared service state owning the retained navigation projection.
///   `state`：持有保留导航投影的共享服务状态。
/// - `parent_session_id`: Exact parent session to project.
///   `parent_session_id`：需要投影的精确父会话。
///
/// # Returns / 返回值
///
/// Returns the resulting optional navigation snapshot after emission.
/// 发射后返回结果导航快照（可为空）。
pub(crate) fn emit_webui_subagent_navigation_changed_delta_for_parent(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    parent_session_id: SessionId,
) -> Result<Option<SubagentNavigationSnapshot>, String> {
    let (generation, parent_conversation_id, subagent_navigation) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
        let selected_session_id = workspace_snapshot.selected_session;
        let focused_parent_session_id = if parent_session_id == SessionId::nil() {
            selected_session_id
        } else {
            parent_session_id
        };
        let use_active_panel = focused_parent_session_id == selected_session_id;
        let subagent_navigation = subagent_navigation_snapshot_for_parent_from_cached_state(
            &mut guard,
            focused_parent_session_id,
            use_active_panel,
        )?;
        let parent_conversation_id = subagent_navigation
            .as_ref()
            .map(|navigation| navigation.parent_conversation_id.clone())
            .or_else(|| {
                (focused_parent_session_id != SessionId::nil())
                    .then(|| focused_parent_session_id.to_string())
            });
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        if use_active_panel {
            guard.workbench.subagent_navigation = subagent_navigation.clone();
        }
        (generation, parent_conversation_id, subagent_navigation)
    };
    emit_service_event_with_trace(
        app,
        "subagent_navigation_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::SubagentNavigationChanged(SubagentNavigationChangedDelta {
                parent_conversation_id,
                subagent_navigation: subagent_navigation.clone(),
            }),
        })),
    );
    Ok(subagent_navigation)
}

/// Controls whether one workbench refresh rebuilds structural subagent navigation.
/// 控制一次工作台刷新是否重建结构化子代理导航。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SubagentNavigationRefreshMode {
    /// Rebuilds subagent navigation from the already-hydrated process-local cache.
    /// 从已经水合的进程内缓存重建子代理导航。
    Rebuild,
}

/// Removes stale or excessive loaded subagent detail cache entries.
/// 移除过期或超量的已加载子代理详情缓存项。
///
/// # Parameters / 参数
///
/// - `guard`: Mutable service state owning the process-local detail cache.
///   `guard`：持有进程内详情缓存的可变服务状态。
/// - `now_ms`: Current timestamp used only for cache eviction.
///   `now_ms`：仅用于缓存淘汰的当前时间戳。
pub(crate) fn prune_webui_subagent_surface_detail_cache(
    guard: &mut ServiceBridgeState,
    now_ms: i128,
) {
    guard.subagent_surface_detail_cache.retain(|_, entry| {
        now_ms.saturating_sub(entry.last_access_ms) <= SUBAGENT_SURFACE_DETAIL_CACHE_TTL_MS
    });
    if guard.subagent_surface_detail_cache.len() <= SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT {
        return;
    }
    let mut entries_by_age = guard
        .subagent_surface_detail_cache
        .iter()
        .map(|(task_id, entry)| (*task_id, entry.last_access_ms))
        .collect::<Vec<_>>();
    entries_by_age.sort_by_key(|(_, last_access_ms)| *last_access_ms);
    let overflow = guard
        .subagent_surface_detail_cache
        .len()
        .saturating_sub(SUBAGENT_SURFACE_DETAIL_CACHE_LIMIT);
    for (task_id, _) in entries_by_age.into_iter().take(overflow) {
        guard.subagent_surface_detail_cache.remove(&task_id);
    }
}
