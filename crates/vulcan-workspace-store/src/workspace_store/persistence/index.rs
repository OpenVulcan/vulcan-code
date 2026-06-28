use super::*;

/// Upserts all visible project rows while clearing the archive flag.
/// 在清理归档标记的同时上写全部可见项目行。
pub(super) async fn upsert_projects(
    controller: &ControllerRuntimeHandle,
    task_projects: &[(TaskProjectSummary, bool)],
) -> Result<(), String> {
    if task_projects.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = task_projects
        .iter()
        .map(|(project, archived)| {
            vec![
                ControllerSqliteValue::String(project.id.clone()),
                ControllerSqliteValue::String(project.title.clone()),
                ControllerSqliteValue::String(project.path.clone()),
                ControllerSqliteValue::Int64(i64::from(project.pinned)),
                ControllerSqliteValue::Int64(i64::from(*archived)),
                ControllerSqliteValue::Int64(project.order as i64),
                optional_sqlite_string(&project.icon_pack),
                optional_sqlite_string(&project.icon_key),
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(now.clone()),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO projects (
    id,
    name,
    root_path,
    pinned,
    archived,
    sort_order,
    icon_pack,
    icon_key,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
ON CONFLICT(id) DO UPDATE SET
    name = excluded.name,
    root_path = excluded.root_path,
    pinned = excluded.pinned,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
"#,
            params,
        )
        .await
        .map_err(|error| format!("写入项目列表失败：{error}"))?;

    Ok(())
}

/// Upserts one lightweight session locator row without storing transcript or runtime payload.
/// 写入或更新一条轻量会话定位行，不存储转录或运行态载荷。
pub(super) async fn upsert_session_index_row(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
    sort_order: usize,
    updated_at_override: Option<&str>,
) -> Result<(), String> {
    let created_at = normalized_session_created_at(session);
    let updated_at = updated_at_override
        .map(str::to_string)
        .or_else(|| {
            let trimmed = session.updated_at.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        })
        .unwrap_or_else(|| created_at.clone());
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO workspace_session_index (
    id,
    project_id,
    project_tag,
    title,
    title_source,
    archived,
    sort_order,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
ON CONFLICT(id) DO UPDATE SET
    project_id = excluded.project_id,
    project_tag = excluded.project_tag,
    title = excluded.title,
    title_source = excluded.title_source,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
"#,
            vec![vec![
                ControllerSqliteValue::String(session.id.to_string()),
                ControllerSqliteValue::String(project_id.to_string()),
                ControllerSqliteValue::String(project_id.to_string()),
                ControllerSqliteValue::String(session.title.clone()),
                ControllerSqliteValue::String(session.title_source.as_persisted_key().to_string()),
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::Int64(sort_order as i64),
                ControllerSqliteValue::String(created_at),
                ControllerSqliteValue::String(updated_at),
            ]],
        )
        .await
        .map_err(|error| format!("写入会话定位索引失败：{error}"))?;
    Ok(())
}

/// Deletes one lightweight session locator row after the backing session is permanently removed.
/// 在底层会话被永久删除后删除一条轻量会话定位行。
pub(super) async fn delete_session_index_row(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "DELETE FROM workspace_session_index WHERE id = ?1;",
            vec![vec![ControllerSqliteValue::String(session_id.to_string())]],
        )
        .await
        .map_err(|error| format!("删除会话定位索引失败：{error}"))?;
    Ok(())
}

/// Updates one lightweight session locator archive state without touching body files.
/// 只更新一条轻量会话定位行的归档状态，不触碰正文文件。
pub(super) async fn update_session_index_archive_state(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    archived: bool,
    updated_at: &str,
) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE workspace_session_index SET archived = ?1, updated_at = ?2 WHERE id = ?3;",
            vec![vec![
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::String(updated_at.to_string()),
                ControllerSqliteValue::String(session_id.to_string()),
            ]],
        )
        .await
        .map_err(|error| format!("更新会话定位索引归档状态失败：{error}"))?;
    Ok(())
}

/// Queries one persisted session locator row by session id and expected archive state.
/// 按会话标识和期望归档状态查询一条持久化会话定位行。
pub(super) async fn query_session_index_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    archived: bool,
) -> Result<Option<PersistedSessionIndexLocator>, String> {
    /// Raw session locator row decoded for deterministic filesystem operations.
    /// 为确定性文件系统操作解码的原始会话定位行。
    #[derive(Debug, Clone, Deserialize)]
    struct SessionLocatorRow {
        /// Owning project identifier.
        /// 所属项目标识。
        project_id: String,
        /// Directory project tag.
        /// 目录项目标签。
        project_tag: String,
        /// Session creation timestamp.
        /// 会话创建时间戳。
        created_at: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT project_id, project_tag, created_at
FROM workspace_session_index
WHERE id = ?1 AND archived = ?2
LIMIT 1;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(i64::from(archived)),
            ],
        )
        .await
        .map_err(|error| format!("查询会话定位索引失败：{error}"))?;
    Ok(decode_query_rows::<SessionLocatorRow>(&result.json_data)?
        .into_iter()
        .next()
        .map(|row| PersistedSessionIndexLocator {
            project_id: row.project_id,
            project_tag: row.project_tag,
            created_at: row.created_at,
            archived,
        }))
}

/// Lightweight persisted session locator used for exact filesystem operations.
/// 用于精确文件系统操作的轻量持久化会话定位信息。
pub(super) struct PersistedSessionIndexLocator {
    /// Owning project identifier.
    /// 所属项目标识。
    pub(super) project_id: String,
    /// Directory project tag.
    /// 目录项目标签。
    pub(super) project_tag: String,
    /// Session creation timestamp.
    /// 会话创建时间戳。
    pub(super) created_at: String,
    /// Whether the locator points to the archived tree.
    /// 该定位信息是否指向归档树。
    pub(super) archived: bool,
}

/// Returns a stable creation timestamp for a session locator row.
/// 返回会话定位行使用的稳定创建时间戳。
pub(super) fn normalized_session_created_at(session: &SessionSummary) -> String {
    let created_at = session.created_at.trim();
    if !created_at.is_empty() {
        return created_at.to_string();
    }
    let updated_at = session.updated_at.trim();
    if !updated_at.is_empty() {
        return updated_at.to_string();
    }
    current_unix_millis_text()
}

/// Soft-archives active projects missing from the current visible snapshot.
/// 将当前可见快照中缺失的激活项目软归档。
pub(super) async fn archive_missing_projects(
    controller: &ControllerRuntimeHandle,
    existing_active_ids: &BTreeSet<String>,
    visible_ids: &BTreeSet<String>,
) -> Result<(), String> {
    let missing_ids = existing_active_ids
        .iter()
        .filter(|project_id| !visible_ids.contains(*project_id))
        .cloned()
        .collect::<Vec<_>>();

    if missing_ids.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = missing_ids
        .into_iter()
        .map(|project_id| {
            vec![
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(project_id),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE projects SET archived = 1, updated_at = ?1 WHERE id = ?2;",
            params,
        )
        .await
        .map_err(|error| format!("软归档项目失败：{error}"))?;

    Ok(())
}

/// Builds a map from session id to owning project id and project-local sort order.
/// 构建从会话标识到所属项目标识及项目内排序位置的映射。
pub(super) fn session_owner_map_from_projects(
    task_projects: &[TaskProjectSummary],
) -> BTreeMap<SessionId, (String, usize)> {
    task_projects
        .iter()
        .flat_map(|project| {
            project
                .session_ids
                .iter()
                .enumerate()
                .map(move |(index, session_id)| (*session_id, (project.id.clone(), index)))
        })
        .collect()
}

/// Returns the explicit project owner for one session or fails without inventing a project.
/// 返回单个会话的显式项目归属，缺失时失败且不构造项目。
pub(super) fn required_session_project_owner(
    session_owner_map: &BTreeMap<SessionId, (String, usize)>,
    session: &SessionSummary,
    operation: &str,
) -> Result<(String, usize), String> {
    session_owner_map.get(&session.id).cloned().ok_or_else(|| {
        format!(
            "{operation}失败：未找到会话所属项目，拒绝写入占位项目。session={}",
            session.id
        )
    })
}

/// Builds one non-fatal warning describing that segmented session persistence lagged behind a successful snapshot save.
/// 构建一条非致命告警，用于描述分段会话持久化落后于一次成功的快照保存。
pub(super) fn segmented_session_persist_warning(
    operation: &str,
    session_count: usize,
    error: &str,
) -> String {
    format!(
        "{operation}：涉及 {session_count} 个会话。核心工作台状态已继续保存，但分段会话文件需要稍后重新同步。原始错误：{error}"
    )
}

/// Converts Unix-milliseconds text into the UTC date segment used by segmented session directories.
/// 将 Unix 毫秒文本转换为分段会话目录使用的 UTC 日期片段。
pub(super) fn persistence_segmented_session_date_from_unix_millis_text(value: &str) -> String {
    let millis = value.trim().parse::<i64>().unwrap_or_default();
    let seconds = millis.div_euclid(1_000);
    let days = seconds.div_euclid(86_400);
    let (year, month, day) = civil_date_from_unix_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Converts days since the Unix epoch into one proleptic Gregorian UTC date.
/// 将 Unix 纪元以来的天数转换为一个前推格里高利 UTC 日期。
pub(super) fn civil_date_from_unix_days(days_since_unix_epoch: i64) -> (i64, i64, i64) {
    let shifted_days = days_since_unix_epoch + 719_468;
    let era = if shifted_days >= 0 {
        shifted_days
    } else {
        shifted_days - 146_096
    } / 146_097;
    let day_of_era = shifted_days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    if month <= 2 {
        year += 1;
    }
    (year, month, day)
}
