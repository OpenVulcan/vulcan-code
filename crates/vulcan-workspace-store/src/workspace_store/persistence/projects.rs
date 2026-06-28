use super::*;

/// Persists one newly visible project row without rewriting unrelated workspace rows.
/// 持久化单个新出现的可见项目行，不重写无关工作区行。
pub async fn persist_new_project(
    controller: ControllerRuntimeHandle,
    project: &TaskProjectSummary,
) -> Result<(), String> {
    upsert_projects(&controller, &[(project.clone(), false)]).await
}

/// Updates the archived flag for one project row without changing its metadata.
/// 只更新单个项目行的归档标记，不改变其元数据。
pub(super) async fn update_project_archive_flag(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    archived: bool,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE projects SET archived = ?1, updated_at = ?2 WHERE id = ?3;",
            vec![vec![
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("更新项目归档状态失败：{error}"))?;

    Ok(())
}

/// Deletes one active project row while preserving archived session directories.
/// 删除单个活动项目记录，同时保留归档会话目录。
pub(super) async fn delete_active_project_row(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "DELETE FROM projects WHERE id = ?1 AND archived = 0;",
            vec![vec![ControllerSqliteValue::String(project_id.to_string())]],
        )
        .await
        .map_err(|error| format!("移除项目记录失败：{error}"))?;

    Ok(())
}

/// Toggles one active project's pinned flag without rewriting unrelated workspace rows.
/// 切换单个活动项目的置顶标记，不重写无关工作区行。
pub async fn toggle_project_pinned(
    controller: ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET pinned = CASE WHEN pinned = 0 THEN 1 ELSE 0 END, updated_at = ?1 WHERE id = ?2 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("切换项目置顶状态失败：{error}"))?;

    Ok(())
}

/// Persists one project's pinned flag and both project-section orders in a deterministic sequence.
/// 按确定顺序持久化单个项目的置顶标记以及两个项目分区的顺序。
pub async fn persist_project_pinned_and_order_update(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    pinned: bool,
    pinned_project_ids: &[String],
    regular_project_ids: &[String],
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET pinned = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::Int64(i64::from(pinned)),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("写入项目置顶状态失败：{error}"))?;

    persist_project_order_update(controller.clone(), pinned_project_ids).await?;
    persist_project_order_update(controller, regular_project_ids).await?;

    Ok(())
}

/// Persists one active project's display name without touching its sessions.
/// 持久化单个活动项目的显示名称，不触碰其会话数据。
pub async fn rename_project(
    controller: ControllerRuntimeHandle,
    project_id: &str,
    name: &str,
) -> Result<(), String> {
    let normalized_name = resolve_project_rename_target(&controller, project_id, name).await?;

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET name = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            vec![vec![
                ControllerSqliteValue::String(normalized_name.to_string()),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(project_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("重命名项目失败：{error}"))?;

    Ok(())
}

/// Resolves an empty project rename into the owning folder name.
/// 将空项目重命名解析为所属文件夹名称。
pub(super) async fn resolve_project_rename_target(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    name: &str,
) -> Result<String, String> {
    let normalized_name = name.trim();
    if !normalized_name.is_empty() && !is_path_separator_only_project_name(normalized_name) {
        return Ok(normalized_name.to_string());
    }

    /// Raw project path row decoded for empty-name reset.
    /// 为项目空名还原解码的原始项目路径行。
    #[derive(Debug, Clone, Deserialize)]
    struct ProjectPathRow {
        /// Absolute project root path stored in SQLite.
        /// SQLite 中存储的项目绝对根路径。
        root_path: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT root_path FROM projects WHERE id = ?1 AND archived = 0 LIMIT 1;",
            vec![ControllerSqliteValue::String(project_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询项目路径失败：{error}"))?;
    let row = decode_query_rows::<ProjectPathRow>(&result.json_data)?
        .into_iter()
        .next()
        .ok_or_else(|| String::from("没有找到要重命名的项目。"))?;
    let fallback_name = project_folder_name_from_path_text(&row.root_path)
        .ok_or_else(|| String::from("项目路径无法生成默认名称。"))?;
    Ok(fallback_name)
}

/// Detects invalid slash-only project display names and treats them as reset requests.
/// 检测仅由斜杠组成的无效项目显示名称，并将其视为还原请求。
pub(super) fn is_path_separator_only_project_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|character| matches!(character, '\\' | '/'))
}

/// Extracts the final folder segment from a Windows or Unix path string.
/// 从 Windows 或 Unix 路径字符串中提取最后一级文件夹名称。
pub(super) fn project_folder_name_from_path_text(path: &str) -> Option<String> {
    path.split(['\\', '/'])
        .filter(|segment| !segment.trim().is_empty())
        .next_back()
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .map(ToOwned::to_owned)
}

/// Persists the project order for one pinned or regular section.
/// 持久化单个置顶或普通分区内的项目顺序。
pub async fn persist_project_order_update(
    controller: ControllerRuntimeHandle,
    project_ids: &[String],
) -> Result<(), String> {
    if project_ids.is_empty() {
        return Ok(());
    }

    let updated_at = current_unix_millis_text();
    let rows = project_ids
        .iter()
        .enumerate()
        .map(|(index, project_id)| {
            vec![
                ControllerSqliteValue::Int64(index as i64),
                ControllerSqliteValue::String(updated_at.clone()),
                ControllerSqliteValue::String(project_id.clone()),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "UPDATE projects SET sort_order = ?1, updated_at = ?2 WHERE id = ?3 AND archived = 0;",
            rows,
        )
        .await
        .map_err(|error| format!("重排项目失败：{error}"))?;

    Ok(())
}

/// Persists the collapsed task-project identifiers as workspace-level UI state.
/// 将已折叠任务项目标识作为工作台级 UI 状态持久化。
pub async fn persist_collapsed_task_project_ids(
    controller: ControllerRuntimeHandle,
    collapsed_project_ids: &[String],
) -> Result<(), String> {
    let value = serde_json::to_string(collapsed_project_ids)
        .map_err(|error| format!("序列化工作台项目展开状态失败：{error}"))?;
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            r#"
INSERT INTO app_metadata (key, value)
VALUES (?1, ?2)
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
"#,
            vec![vec![
                ControllerSqliteValue::String(COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string()),
                ControllerSqliteValue::String(value),
            ]],
        )
        .await
        .map_err(|error| format!("写入工作台项目展开状态失败：{error}"))?;

    Ok(())
}
