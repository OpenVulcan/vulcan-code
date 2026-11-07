//! Project and conversation RPC routing.
//! 项目与会话 RPC 路由。

use super::*;

/// Exact public RPC wire names owned by this domain router.
/// 本领域路由持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_open_project_directory_path",
    "service_open_project_directory",
    "service_browse_project_directories",
    "service_add_project_from_path",
    "service_prepare_self_fix_workspace",
    "service_toggle_project_pinned",
    "service_reorder_projects",
    "service_rename_project",
    "service_set_project_icon",
    "service_reset_project_icon",
    "service_set_project_session_sort_mode",
    "service_reorder_project_sessions",
    "service_set_expanded_task_projects",
    "service_create_project_group",
    "service_rename_project_group",
    "service_delete_project_group",
    "service_set_project_groups_layout",
    "service_set_project_tree_pinned",
    "service_set_project_tree_pins_order",
    "service_add_project_to_group",
    "service_archive_project_conversations",
    "service_remove_project",
    "service_rename_conversation",
    "service_archive_conversation",
    "service_restore_archived_conversations",
    "service_delete_archived_conversations",
    "service_delete_unsupported_conversation",
    "service_mark_conversation_read",
    "service_select_conversation",
    "service_list_collaboration_grants",
    "service_add_collaboration_grant",
    "service_remove_collaboration_grant",
    "service_cancel_collaboration_question",
    "service_open_external_url",
];

/// Dispatches one RPC method owned by this domain.
/// 分发本领域持有的单个 RPC 方法。
///
/// # Parameters
/// 参数
/// `app` emits lifecycle events, `state` owns service data, `method` is the exact wire name, and `params` is normalized.
/// `app` 发出生命周期事件，`state` 持有服务数据，`method` 是精确线协议名称，`params` 已归一化。
///
/// # Returns
/// 返回值
/// Returns `Handled` with the service result, `Unknown` for another domain, or a parameter parsing error.
/// 返回携带服务结果的 `Handled`、表示其他领域的 `Unknown`，或参数解析错误。
pub(super) async fn dispatch(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    // result preserves service failures inside Handled so the top-level return trace still runs.
    // result 把服务失败保留在 Handled 内，使顶层返回追踪仍会执行。
    let result = match method {
        "service_open_project_directory_path" => rpc_json_result(
            service_open_project_directory_path(state, rpc_required_param(params, "path")?),
        ),
        "service_open_project_directory" => rpc_json_result(service_open_project_directory(
            state,
            rpc_required_param(params, "projectId")?,
        )),
        "service_browse_project_directories" => rpc_json_result(
            service_browse_project_directories(state, rpc_optional_param(params, "path")?),
        ),
        "service_add_project_from_path" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_add_project_from_path(
                state,
                PathBuf::from(rpc_required_param::<String>(params, "path")?),
            )
            .await,
        )),
        "service_prepare_self_fix_workspace" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            crate::project_directories::service_prepare_self_fix_workspace(
                state,
                PathBuf::from(rpc_required_param::<String>(params, "parentDir")?),
                rpc_required_param(params, "folderName")?,
            )
            .await,
        )),
        "service_toggle_project_pinned" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_toggle_project_pinned(state, rpc_required_param(params, "projectId")?).await,
        )),
        "service_reorder_projects" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_reorder_projects(state, rpc_required_param(params, "projectIds")?).await,
        )),
        "service_rename_project" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_rename_project(
                state,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "name")?,
            )
            .await,
        )),
        "service_set_project_icon" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_set_project_icon(
                state,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "iconPack")?,
                rpc_required_param(params, "iconKey")?,
            )
            .await,
        )),
        "service_reset_project_icon" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_reset_project_icon(state, rpc_required_param(params, "projectId")?).await,
        )),
        "service_set_project_session_sort_mode" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_set_project_session_sort_mode(
                state,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "mode")?,
            )
            .await,
        )),
        "service_reorder_project_sessions" => rpc_json_result(broadcast_after_success(
            &app,
            &state.clone(),
            service_reorder_project_sessions(
                state,
                rpc_required_param(params, "projectId")?,
                rpc_required_param(params, "conversationIds")?,
            )
            .await,
        )),
        "service_set_expanded_task_projects" => rpc_json_result(
            service_set_expanded_task_projects(state, rpc_required_param(params, "projectIds")?)
                .await,
        ),
        "service_create_project_group" => rpc_json_result(
            service_create_project_group(
                app,
                state,
                rpc_required_param(params, "name")?,
                rpc_required_param(params, "projectIds")?,
                rpc_optional_param(params, "afterGroupId")?,
            )
            .await,
        ),
        "service_rename_project_group" => rpc_json_result(
            service_rename_project_group(
                app,
                state,
                rpc_required_param(params, "groupId")?,
                rpc_required_param(params, "name")?,
            )
            .await,
        ),
        "service_delete_project_group" => rpc_json_result(
            service_delete_project_group(app, state, rpc_required_param(params, "groupId")?).await,
        ),
        "service_set_project_groups_layout" => rpc_json_result(
            service_set_project_groups_layout(app, state, rpc_required_param(params, "groups")?)
                .await,
        ),
        "service_set_project_tree_pinned" => rpc_json_result(
            service_set_project_tree_pinned(
                app,
                state,
                rpc_required_param(params, "kind")?,
                rpc_required_param(params, "id")?,
                rpc_required_param(params, "pinned")?,
            )
            .await,
        ),
        "service_set_project_tree_pins_order" => rpc_json_result(
            service_set_project_tree_pins_order(app, state, rpc_required_param(params, "pins")?)
                .await,
        ),
        "service_add_project_to_group" => rpc_json_result(
            service_add_project_to_group(
                app,
                state,
                PathBuf::from(rpc_required_param::<String>(params, "path")?),
                rpc_required_param(params, "groupId")?,
            )
            .await,
        ),
        "service_archive_project_conversations" => rpc_json_result(
            service_archive_project_conversations(
                app,
                state,
                rpc_required_param(params, "projectId")?,
            )
            .await,
        ),
        "service_remove_project" => rpc_json_result(
            service_remove_project(app, state, rpc_required_param(params, "projectId")?).await,
        ),
        "service_rename_conversation" => rpc_json_result(
            service_rename_conversation(
                state,
                rpc_required_param(params, "conversationId")?,
                rpc_required_param(params, "name")?,
            )
            .await,
        ),
        "service_archive_conversation" => rpc_json_result(
            service_archive_conversation(app, state, rpc_required_param(params, "conversationId")?)
                .await,
        ),
        "service_restore_archived_conversations" => rpc_json_result(
            service_restore_archived_conversations(
                state,
                rpc_required_param(params, "conversationIds")?,
            )
            .await,
        ),
        "service_delete_archived_conversations" => rpc_json_result(
            service_delete_archived_conversations(
                state,
                rpc_required_param(params, "conversationIds")?,
            )
            .await,
        ),
        "service_delete_unsupported_conversation" => rpc_json_result(
            service_delete_unsupported_conversation(
                state,
                rpc_required_param(params, "conversationId")?,
            )
            .await,
        ),
        "service_mark_conversation_read" => rpc_json_result(
            service_mark_conversation_read(state, rpc_required_param(params, "conversationId")?)
                .await,
        ),
        "service_select_conversation" => rpc_json_result(
            service_select_conversation(state, rpc_required_param(params, "conversationId")?).await,
        ),
        "service_list_collaboration_grants" => rpc_json_result(
            service_list_collaboration_grants(
                state,
                rpc_required_param(params, "sourceSessionId")?,
            )
            .await,
        ),
        "service_add_collaboration_grant" => rpc_json_result(
            service_add_collaboration_grant(
                state,
                rpc_required_param(params, "sourceSessionId")?,
                rpc_required_param(params, "targetSessionId")?,
                rpc_required_param(params, "operationId")?,
                rpc_required_param(params, "actions")?,
            )
            .await,
        ),
        "service_remove_collaboration_grant" => rpc_json_result(
            service_remove_collaboration_grant(
                state,
                rpc_required_param(params, "sourceSessionId")?,
                rpc_required_param(params, "grantId")?,
                rpc_required_param(params, "operationId")?,
            )
            .await,
        ),
        "service_cancel_collaboration_question" => rpc_json_result(
            service_cancel_collaboration_question(
                state,
                rpc_required_param(params, "questionId")?,
                rpc_required_param(params, "sourceRootSessionId")?,
                rpc_required_param(params, "operationId")?,
                rpc_optional_param(params, "reason")?
                    .unwrap_or_else(|| String::from("user_cancelled")),
            )
            .await,
        ),
        "service_open_external_url" => rpc_json_result(service_open_external_url(
            state,
            rpc_required_param(params, "url")?,
        )),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}

/// Broadcasts the retained workbench after a successful shared project change so every open window follows;
/// the caller still receives its own result unchanged.
/// 共享项目变更成功后广播保留的工作台，使所有打开的窗口一同更新；调用方仍原样收到自身结果。
fn broadcast_after_success<T>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    result: Result<T, String>,
) -> Result<T, String> {
    if result.is_ok()
        && let Ok(guard) = state.lock()
    {
        emit_workbench_event(app, &guard.workbench);
    }
    result
}
