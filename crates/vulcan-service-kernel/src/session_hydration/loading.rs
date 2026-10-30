//! Session hydration loading helpers.
//! 会话水合 loading 辅助逻辑。

use super::*;

pub(crate) async fn load_workspace_for_submission(
    state: &SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    session_id: Option<SessionId>,
) -> Result<WorkspaceSnapshot, String> {
    // Measures lightweight workspace index loading for prompt submission.
    // 衡量提示词提交时轻量工作区索引载入耗时。
    let workspace_load_started_at = current_unix_millis_value();
    let mut workspace_snapshot =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(
            controller_runtime.clone(),
        )
        .await?;
    webui_trace_if_slow(
        "submit_prompt_workspace_snapshot_load_slow",
        workspace_load_started_at,
        format!(
            "conversation_id={}",
            session_id
                .map(|session_id| session_id.to_string())
                .as_deref()
                .unwrap_or("<draft>")
        ),
    );
    if let Some(session_id) = session_id {
        // The V1 document is the only restart authority; the workbench runtime entry is presentation-only.
        // V1 文档是唯一重启权威；工作台运行态条目仅用于展示适配。
        let hydration_started_at = current_unix_millis_value();
        crate::sync_selected_session_v1_projection_from_store(
            state,
            controller_runtime.clone(),
            session_id,
            &mut workspace_snapshot,
        )
        .await?;
        webui_trace_if_slow(
            "submit_prompt_session_v1_projection_slow",
            hydration_started_at,
            format!("session={session_id}"),
        );
    }
    Ok(workspace_snapshot)
}

/// Returns whether a retained WebUI snapshot can be used as the submit base snapshot.
/// 判断保留的 WebUI 快照能否作为本次提交的基础快照。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the in-memory snapshot retained by the service bridge.
/// `workspace_snapshot` 是服务桥保留的内存工作区快照。
/// `session_id` optionally selects an existing submitted conversation.
/// `session_id` 可选选择本次提交的既有会话。
///
/// # Returns
/// 返回值
/// `true` for a draft submission or when the existing session is active in the retained snapshot.
/// 草稿提交或既有会话在保留快照中处于活动列表时返回 `true`。
pub(crate) fn retained_submission_snapshot_is_usable(
    workspace_snapshot: &WorkspaceSnapshot,
    project_id: Option<&str>,
    session_id: Option<SessionId>,
) -> bool {
    match session_id {
        Some(session_id) => {
            workspace_session_summary_for_session(workspace_snapshot, session_id).is_some()
        }
        None => project_id.map_or_else(
            || !workspace_snapshot.task_projects.is_empty(),
            |project_id| workspace_snapshot_contains_active_project(workspace_snapshot, project_id),
        ),
    }
}

/// Projects the submitted retained session from its exact V1 document.
/// 根据精确 V1 文档投影本次提交的保留会话。
///
/// # Parameters
/// 参数
/// `state` owns process-local admission and execution registries used to distinguish live claims.
/// `state` 持有用于区分实时领取的进程内准入与执行注册表。
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the retained WebUI workspace snapshot being submitted.
/// `workspace_snapshot` 是本次提交正在使用的保留 WebUI 工作区快照。
/// `session_id` selects the submitted existing session.
/// `session_id` 选择本次提交的既有会话。
/// `preserved_state` retains volatile execution state, applied before authoritative V1 controls.
/// `preserved_state` 保留易失执行状态，先于权威 V1 控制项应用。
///
/// # Returns
/// 返回值
/// `Ok(())` after the session either already had context or has been hydrated.
/// 当会话已具备上下文或已完成水合后返回 `Ok(())`。
pub(crate) async fn hydrate_retained_submission_session_if_needed(
    state: &SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    preserved_state: Option<&PreservedWorkspaceRunState>,
) -> Result<(), String> {
    // A metadata-only old row must never overwrite the persisted node after hydration.
    // 纯元数据旧行绝不能在水合后覆盖持久节点。
    preserve_active_session_run_state(preserved_state, workspace_snapshot);
    let hydration_started_at = current_unix_millis_value();
    crate::sync_selected_session_v1_projection_from_store(
        state,
        controller_runtime,
        session_id,
        workspace_snapshot,
    )
    .await?;
    webui_trace_if_slow(
        "submit_prompt_retained_session_v1_projection_slow",
        hydration_started_at,
        format!("session={session_id}"),
    );
    Ok(())
}

/// Projects selected-session runtime controls from the exact V1 document.
/// 根据精确 V1 文档投影选中会话运行态控制项。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `workspace_snapshot` is the retained WebUI workspace snapshot that receives runtime memory.
/// `workspace_snapshot` 是接收运行态记忆的保留 WebUI 工作区快照。
/// `session_id` selects the session whose runtime JSONL should be hydrated.
/// `session_id` 选择需要从运行时 JSONL 水合的会话。
///
/// # Returns
/// 返回值
/// `Ok(())` after runtime context exists or a concrete hydration error has been reported.
/// 当运行态上下文已存在或已报告明确水合错误后返回 `Ok(())`。
pub(crate) async fn hydrate_selected_session_runtime_if_needed(
    state: &SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) -> Result<(), String> {
    crate::sync_selected_session_v1_projection_from_store(
        state,
        controller_runtime,
        session_id,
        workspace_snapshot,
    )
    .await?;
    Ok(())
}
