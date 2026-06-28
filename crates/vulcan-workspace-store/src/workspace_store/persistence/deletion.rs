use super::*;

/// Permanently deletes one archived session together with all persisted transcript and runtime artifacts.
/// 彻底删除一个已归档会话及其全部已持久化的转录与运行时工件。
pub async fn permanently_delete_archived_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    if let Some(locator) = query_session_index_locator(&controller, session_id, true).await? {
        delete_segmented_session_directory_for_locator_if_present(
            &controller,
            session_id,
            &locator,
        )?;
    }
    delete_session_index_row(&controller, session_id).await?;
    Ok(())
}

/// Permanently deletes one archived project together with all child archived sessions and persisted artifacts.
/// 彻底删除一个已归档项目及其全部子归档会话与已持久化工件。
pub async fn permanently_delete_archived_project(
    controller: ControllerRuntimeHandle,
    project_id: String,
    session_ids: Vec<SessionId>,
) -> Result<(), String> {
    for session_id in &session_ids {
        if let Some(locator) = query_session_index_locator(&controller, *session_id, true).await? {
            delete_segmented_session_directory_for_locator_if_present(
                &controller,
                *session_id,
                &locator,
            )?;
        }
        delete_session_index_row(&controller, *session_id).await?;
    }

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "DELETE FROM projects WHERE id = ?1 AND archived != 0;",
            vec![vec![ControllerSqliteValue::String(project_id.clone())]],
        )
        .await
        .map_err(|error| format!("删除归档项目记录失败：{error}"))?;
    Ok(())
}
