use super::*;

/// Inserts or reactivates one active project without rewriting unrelated rows.
/// 插入或重新激活一个活动项目，不重写无关行。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
/// `project` is the exact active project summary to insert or reactivate.
/// `project` 是需要插入或重新激活的精确活动项目摘要。
///
/// # Returns
/// 返回值
/// Returns success after one typed upsert is accepted, or an explicit order-range/database error.
/// 单条类型化 upsert 被接受后返回成功；顺序越界或数据库失败时返回显式错误。
pub(super) async fn upsert_active_project(
    controller: &ControllerRuntimeHandle,
    project: &TaskProjectSummary,
) -> Result<(), String> {
    // now is shared by created_at and updated_at for a newly inserted project row.
    // now 由新插入项目行的 created_at 与 updated_at 共享。
    let now = current_unix_millis_text();
    // sort_order uses a checked conversion so project order cannot truncate on wide usize hosts.
    // sort_order 使用受检查转换，避免项目顺序在宽 usize 宿主上发生截断。
    let sort_order = i64::try_from(project.order)
        .map_err(|_| String::from("写入活动项目失败：项目顺序超出 SQLite 整数范围。"))?;
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
            vec![vec![
                ControllerSqliteValue::String(project.id.clone()),
                ControllerSqliteValue::String(project.title.clone()),
                ControllerSqliteValue::String(project.path.clone()),
                ControllerSqliteValue::Int64(i64::from(project.pinned)),
                ControllerSqliteValue::Int64(0),
                ControllerSqliteValue::Int64(sort_order),
                optional_sqlite_string(&project.icon_pack),
                optional_sqlite_string(&project.icon_key),
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(now),
            ]],
        )
        .await
        .map_err(|error| format!("写入活动项目失败：{error}"))?;

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
    // Archive state is intentionally omitted from the conflict update so only lifecycle APIs can change it.
    // 冲突更新刻意不修改归档状态，确保只有生命周期接口可以变更该字段。
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

/// Commits one exact session-locator archive transition after its directory move succeeds.
/// 在会话目录移动成功后提交一次精确定位行归档转换。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
/// `session_id` is the exact persisted session identity being transitioned.
/// `session_id` 是正在转换的精确持久化会话身份。
/// `locator` is the complete expected filesystem locator read before the directory move.
/// `locator` 是目录移动前读取的完整预期文件系统定位信息。
/// `archived` is the distinct resulting archive state.
/// `archived` 是不同于预期状态的结果归档状态。
/// `updated_at` is the timestamp already written into moved session metadata.
/// `updated_at` 是已经写入移动后会话元数据的时间戳。
///
/// # Returns
/// 返回值
/// Returns success only when one matching expected locator returns the exact resulting state.
/// 仅在一个匹配预期定位行返回精确结果状态时成功。
pub(super) async fn update_session_index_archive_state(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    locator: &PersistedSessionIndexLocator,
    archived: bool,
    updated_at: &str,
) -> Result<(), String> {
    if locator.archived == archived {
        return Err(format!(
            "会话 `{session_id}` 的归档转换预期状态与结果状态不能相同。"
        ));
    }
    if updated_at.trim().is_empty() {
        return Err(format!("会话 `{session_id}` 的归档转换时间不能为空。"));
    }
    // result proves the row still matches the complete locator used for the directory move.
    // result 证明该行仍匹配目录移动所使用的完整定位信息。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
UPDATE workspace_session_index
SET archived = ?1, updated_at = ?2
WHERE id = ?3
  AND project_id = ?4
  AND project_tag = ?5
  AND created_at = ?6
  AND archived = ?7
RETURNING id, archived;
"#,
            vec![
                ControllerSqliteValue::Int64(i64::from(archived)),
                ControllerSqliteValue::String(updated_at.to_string()),
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::String(locator.project_id.clone()),
                ControllerSqliteValue::String(locator.project_tag.clone()),
                ControllerSqliteValue::String(locator.created_at.clone()),
                ControllerSqliteValue::Int64(i64::from(locator.archived)),
            ],
        )
        .await
        .map_err(|error| format!("更新会话定位索引归档状态失败：{error}"))?;
    // rows retain SQLite's exact resulting identity and archive scalar for validation.
    // rows 保留 SQLite 的精确结果身份与归档标量供校验。
    let rows = decode_query_rows::<SessionArchiveStateUpdateRow>(&result.json_data)?;
    validate_session_archive_state_update_rows(session_id, archived, &rows)
}

/// Updates only the manual title fields of one active session locator row.
/// 仅更新一条活动会话定位行的手动标题字段。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
/// `session_id` is the exact active session identity previously resolved from the locator index.
/// `session_id` 是先前从定位索引解析出的精确活动会话身份。
/// `title` is the normalized manual display title.
/// `title` 是规范化后的手动展示标题。
/// `updated_at` is the timestamp committed with the title change.
/// `updated_at` 是随标题变更提交的时间戳。
///
/// # Returns
/// 返回值
/// Returns success after SQLite accepts the field-scoped active-row update.
/// SQLite 接受限定字段的活动行更新后返回成功。
pub(super) async fn update_active_session_index_title(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    title: &str,
    updated_at: &str,
) -> Result<(), String> {
    // result must contain the exact row returned by SQLite after the active-state predicate succeeds.
    // result 必须包含活动状态谓词命中后由 SQLite 返回的精确行。
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
UPDATE workspace_session_index
SET title = ?1, title_source = ?2, updated_at = ?3
WHERE id = ?4 AND archived = 0
RETURNING id;
"#,
            vec![
                ControllerSqliteValue::String(title.to_string()),
                ControllerSqliteValue::String(
                    SessionTitleSource::Manual.as_persisted_key().to_string(),
                ),
                ControllerSqliteValue::String(updated_at.to_string()),
                ControllerSqliteValue::String(session_id.to_string()),
            ],
        )
        .await
        .map_err(|error| format!("更新活动会话定位索引标题失败：{error}"))?;
    // updated_rows proves the guarded update changed exactly one authoritative locator.
    // updated_rows 证明受保护更新恰好变更了一条权威定位行。
    let updated_rows = decode_query_rows::<ActiveSessionTitleUpdateRow>(&result.json_data)?;
    validate_active_session_title_update_rows(session_id, &updated_rows)
}

/// Queries one persisted session locator row by session id and expected archive state.
/// 按会话标识和期望归档状态查询一条持久化会话定位行。
pub(super) async fn query_session_index_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    archived: bool,
) -> Result<Option<PersistedSessionIndexLocator>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, project_id, project_tag, created_at, updated_at
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
        .map(|row| persisted_session_index_entry_from_row(row, archived))
        .transpose()?
        .map(|entry| entry.locator))
}

/// Queries every persisted session locator owned by one project in the requested archive state.
/// 查询单个项目在指定归档状态下拥有的全部持久化会话定位行。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite binding.
/// `controller` 持有权威 SQLite 绑定。
/// `project_id` is the exact persisted owner identifier used to scope the query.
/// `project_id` 是用于限定查询范围的精确持久化所属项目标识。
/// `archived` selects active or archived locator rows.
/// `archived` 选择活动或归档定位行。
///
/// # Returns
/// 返回值
/// Returns deterministically ordered session identities together with their exact filesystem locators.
/// 返回按确定顺序排列的会话身份及其精确文件系统定位信息。
pub(super) async fn query_session_index_locators_for_project(
    controller: &ControllerRuntimeHandle,
    project_id: &str,
    archived: bool,
) -> Result<Vec<PersistedSessionIndexEntry>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, project_id, project_tag, created_at, updated_at
FROM workspace_session_index
WHERE project_id = ?1 AND archived = ?2
ORDER BY sort_order ASC, updated_at DESC, id ASC;
"#,
            vec![
                ControllerSqliteValue::String(project_id.to_string()),
                ControllerSqliteValue::Int64(i64::from(archived)),
            ],
        )
        .await
        .map_err(|error| format!("查询项目会话定位索引失败：{error}"))?;
    decode_query_rows::<SessionLocatorRow>(&result.json_data)?
        .into_iter()
        .map(|row| persisted_session_index_entry_from_row(row, archived))
        .collect()
}

/// Raw session locator row decoded for deterministic filesystem operations.
/// 为确定性文件系统操作解码的原始会话定位行。
#[derive(Debug, Clone, Deserialize)]
struct SessionLocatorRow {
    /// Stable session identifier.
    /// 稳定会话标识。
    id: String,
    /// Owning project identifier.
    /// 所属项目标识。
    project_id: String,
    /// Directory project tag.
    /// 目录项目标签。
    project_tag: String,
    /// Session creation timestamp.
    /// 会话创建时间戳。
    created_at: String,
    /// Session update timestamp captured before a lifecycle transition.
    /// 生命周期转换前捕获的会话更新时间戳。
    updated_at: String,
}

/// Row identity returned by a guarded active-session title update.
/// 受保护的活动会话标题更新返回的行身份。
#[derive(Debug, Clone, Deserialize)]
struct ActiveSessionTitleUpdateRow {
    /// Stable session identifier returned by SQLite.
    /// SQLite 返回的稳定会话标识。
    id: String,
}

/// Identity and archive scalar returned by one guarded session lifecycle transition.
/// 单次受保护会话生命周期转换返回的身份与归档标量。
#[derive(Debug, Clone, Deserialize)]
struct SessionArchiveStateUpdateRow {
    /// Stable session identifier returned by SQLite.
    /// SQLite 返回的稳定会话标识。
    id: String,
    /// Resulting archive flag returned as a strict SQLite integer boolean.
    /// 以严格 SQLite 整数布尔值返回的结果归档标记。
    archived: i64,
}

/// Validates that a guarded title update returned exactly the requested active session row.
/// 验证受保护的标题更新恰好返回请求的活动会话行。
///
/// # Parameters
/// 参数
/// `session_id` is the exact active session identity submitted to SQLite.
/// `session_id` 是提交给 SQLite 的精确活动会话身份。
/// `rows` contains identities returned by the `UPDATE ... RETURNING` statement.
/// `rows` 包含 `UPDATE ... RETURNING` 语句返回的身份。
///
/// # Returns
/// 返回值
/// Returns success for one matching row, or an explicit missing, concurrent-state, or corruption error.
/// 单条匹配行返回成功；缺失、并发状态变化或损坏时返回显式错误。
fn validate_active_session_title_update_rows(
    session_id: SessionId,
    rows: &[ActiveSessionTitleUpdateRow],
) -> Result<(), String> {
    if rows.len() != 1 {
        return Err(format!(
            "活动会话 `{session_id}` 在标题提交前已不存在或状态已改变。"
        ));
    }
    if rows[0].id != session_id.to_string() {
        return Err(format!(
            "活动会话标题更新返回了意外标识 `{}`，期望 `{session_id}`。",
            rows[0].id
        ));
    }
    Ok(())
}

/// Validates that one guarded lifecycle update returned the exact session and archive state.
/// 验证一次受保护生命周期更新返回了精确会话与归档状态。
///
/// # Parameters
/// 参数
/// `session_id` is the exact session identity submitted to SQLite.
/// `session_id` 是提交给 SQLite 的精确会话身份。
/// `archived` is the exact resulting archive state requested by the caller.
/// `archived` 是调用方请求的精确结果归档状态。
/// `rows` contains the `UPDATE ... RETURNING` result rows.
/// `rows` 包含 `UPDATE ... RETURNING` 结果行。
///
/// # Returns
/// 返回值
/// Returns success only for one matching identity with a strict matching boolean scalar.
/// 仅在一条匹配身份具有严格匹配布尔标量时成功。
fn validate_session_archive_state_update_rows(
    session_id: SessionId,
    archived: bool,
    rows: &[SessionArchiveStateUpdateRow],
) -> Result<(), String> {
    if rows.len() != 1 {
        return Err(format!(
            "会话 `{session_id}` 的定位信息或归档状态在提交前已改变。"
        ));
    }
    if rows[0].id != session_id.to_string() {
        return Err(format!(
            "会话归档状态更新返回了意外标识 `{}`，期望 `{session_id}`。",
            rows[0].id
        ));
    }
    // returned_archived rejects corrupted non-boolean SQLite response states.
    // returned_archived 拒绝损坏的非布尔 SQLite 响应状态。
    let returned_archived = match rows[0].archived {
        0 => false,
        1 => true,
        value => {
            return Err(format!(
                "会话 `{session_id}` 的归档状态更新返回非法标记 `{value}`。"
            ));
        }
    };
    if returned_archived != archived {
        return Err(format!(
            "会话 `{session_id}` 的归档状态更新返回 `{}`，期望 `{}`。",
            rows[0].archived,
            i64::from(archived)
        ));
    }
    Ok(())
}

/// Lightweight persisted session locator used for exact filesystem operations.
/// 用于精确文件系统操作的轻量持久化会话定位信息。
pub(super) struct PersistedSessionIndexLocator {
    /// Owning project identifier committed with the locator.
    /// 与定位信息一同提交的所属项目标识。
    pub(super) project_id: String,
    /// Directory project tag.
    /// 目录项目标签。
    pub(super) project_tag: String,
    /// Session creation timestamp.
    /// 会话创建时间戳。
    pub(super) created_at: String,
    /// Session update timestamp that lifecycle rollback must restore.
    /// 生命周期回滚必须恢复的会话更新时间戳。
    pub(super) updated_at: String,
    /// Whether the locator points to the archived tree.
    /// 该定位信息是否指向归档树。
    pub(super) archived: bool,
}

/// Persisted session identity paired with the locator needed for filesystem operations.
/// 持久化会话身份及文件系统操作所需定位信息的组合。
pub(super) struct PersistedSessionIndexEntry {
    /// Stable parsed session identifier.
    /// 已解析的稳定会话标识。
    pub(super) session_id: SessionId,
    /// Exact persisted locator for the session directory.
    /// 会话目录的精确持久化定位信息。
    pub(super) locator: PersistedSessionIndexLocator,
}

/// Converts one raw SQLite locator row into a validated session identity and filesystem locator.
/// 将一条原始 SQLite 定位行转换为已验证的会话身份与文件系统定位信息。
///
/// # Parameters
/// 参数
/// `row` is the complete raw locator returned by SQLite.
/// `row` 是 SQLite 返回的完整原始定位行。
/// `archived` is the archive predicate used by the owning query.
/// `archived` 是所属查询使用的归档谓词。
///
/// # Returns
/// 返回值
/// Returns a validated entry or an explicit corruption error when the stored id is not a UUID.
/// 返回已验证条目；若已存标识不是 UUID，则返回显式损坏错误。
fn persisted_session_index_entry_from_row(
    row: SessionLocatorRow,
    archived: bool,
) -> Result<PersistedSessionIndexEntry, String> {
    // session_id is parsed once at the persistence boundary so lifecycle operations never guess an identity.
    // session_id 在持久化边界只解析一次，使生命周期操作绝不猜测身份。
    let session_id = SessionId::parse_str(&row.id)
        .map_err(|error| format!("会话定位索引包含无效标识 `{}`：{error}", row.id))?;
    if row.project_id.trim().is_empty() {
        return Err(format!("会话定位索引 `{session_id}` 缺少所属项目标识。"));
    }
    if row.project_tag.trim().is_empty() {
        return Err(format!("会话定位索引 `{session_id}` 缺少目录项目标签。"));
    }
    if row.updated_at.trim().is_empty() {
        return Err(format!("会话定位索引 `{session_id}` 缺少更新时间戳。"));
    }
    Ok(PersistedSessionIndexEntry {
        session_id,
        locator: PersistedSessionIndexLocator {
            project_id: row.project_id,
            project_tag: row.project_tag,
            created_at: row.created_at,
            updated_at: row.updated_at,
            archived,
        },
    })
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies raw locator decoding preserves the exact persisted identity and archive predicate.
    /// 验证原始定位行解码会保留精确持久化身份与归档谓词。
    #[test]
    fn persisted_session_index_entry_preserves_identity_and_archive_state() {
        // session_id is the stable identity encoded into the raw SQLite row.
        // session_id 是编码到原始 SQLite 行中的稳定身份。
        let session_id = SessionId::from_u128(0x41);
        // entry is the validated lifecycle target produced at the persistence boundary.
        // entry 是持久化边界生成的已验证生命周期目标。
        let entry = persisted_session_index_entry_from_row(
            SessionLocatorRow {
                id: session_id.to_string(),
                project_id: String::from("project-a"),
                project_tag: String::from("project-tag-a"),
                created_at: String::from("1783652000000"),
                updated_at: String::from("1783652999000"),
            },
            true,
        )
        .expect("valid locator row should decode");

        assert_eq!(entry.session_id, session_id);
        assert_eq!(entry.locator.project_id, "project-a");
        assert_eq!(entry.locator.project_tag, "project-tag-a");
        assert_eq!(entry.locator.created_at, "1783652000000");
        assert_eq!(entry.locator.updated_at, "1783652999000");
        assert!(entry.locator.archived);
    }

    /// Verifies malformed persisted identities stop lifecycle-locator construction explicitly.
    /// 验证格式错误的持久化身份会显式停止生命周期定位构造。
    #[test]
    fn persisted_session_index_entry_rejects_invalid_session_id() {
        // result captures the corruption boundary without inventing a replacement UUID.
        // result 捕获损坏边界，且不会构造替代 UUID。
        let result = persisted_session_index_entry_from_row(
            SessionLocatorRow {
                id: String::from("not-a-session-id"),
                project_id: String::from("project-a"),
                project_tag: String::from("project-tag-a"),
                created_at: String::from("1783652000000"),
                updated_at: String::from("1783652999000"),
            },
            true,
        );
        // error is the explicit corruption diagnostic returned to the owning query.
        // error 是返回给所属查询的显式损坏诊断。
        let error = result.err().expect("invalid locator id should fail");

        assert!(error.contains("not-a-session-id"));
    }

    /// Verifies locator decoding rejects missing lifecycle fields instead of inventing directory fallbacks.
    /// 验证定位行解码会拒绝缺失的生命周期字段，而不是构造目录兜底身份。
    #[test]
    fn persisted_session_index_entry_rejects_missing_lifecycle_fields() {
        // session_id is the valid session identity shared by all corrupted rows.
        // session_id 是全部损坏行共享的有效会话身份。
        let session_id = SessionId::from_u128(0x43);
        // missing_project_id_result captures a row without its authoritative owner identity.
        // missing_project_id_result 捕获缺少权威所属项目身份的行。
        let missing_project_id_result = persisted_session_index_entry_from_row(
            SessionLocatorRow {
                id: session_id.to_string(),
                project_id: String::from("   "),
                project_tag: String::from("project-tag-a"),
                created_at: String::from("1783652000000"),
                updated_at: String::from("1783652999000"),
            },
            false,
        );
        // missing_project_tag_result captures a row without its authoritative directory identity.
        // missing_project_tag_result 捕获缺少权威目录身份的行。
        let missing_project_tag_result = persisted_session_index_entry_from_row(
            SessionLocatorRow {
                id: session_id.to_string(),
                project_id: String::from("project-a"),
                project_tag: String::from("   "),
                created_at: String::from("1783652000000"),
                updated_at: String::from("1783652999000"),
            },
            false,
        );
        // missing_updated_at_result captures a row without its rollback timestamp.
        // missing_updated_at_result 捕获缺少回滚时间戳的行。
        let missing_updated_at_result = persisted_session_index_entry_from_row(
            SessionLocatorRow {
                id: session_id.to_string(),
                project_id: String::from("project-a"),
                project_tag: String::from("project-tag-a"),
                created_at: String::from("1783652000000"),
                updated_at: String::from("   "),
            },
            false,
        );
        // field errors retain the exact corruption diagnostics.
        // 字段错误保留精确损坏诊断。
        let project_id_error = missing_project_id_result
            .err()
            .expect("missing project id should fail");
        let project_tag_error = missing_project_tag_result
            .err()
            .expect("missing project tag should fail");
        let updated_at_error = missing_updated_at_result
            .err()
            .expect("missing update timestamp should fail");

        assert!(project_id_error.contains("缺少所属项目标识"));
        assert!(project_tag_error.contains("缺少目录项目标签"));
        assert!(updated_at_error.contains("缺少更新时间戳"));
    }

    /// Verifies guarded archive-state transitions require one exact identity and strict result flag.
    /// 验证受保护归档状态转换必须返回一个精确身份与严格结果标记。
    #[test]
    fn session_archive_state_update_rows_require_exact_identity_and_state() {
        // session_id is the exact lifecycle identity submitted to SQLite.
        // session_id 是提交给 SQLite 的精确生命周期身份。
        let session_id = SessionId::from_u128(0x46);
        // matching_rows model the only successful active-to-archived transition result.
        // matching_rows 模拟唯一成功的活动到归档转换结果。
        let matching_rows = vec![SessionArchiveStateUpdateRow {
            id: session_id.to_string(),
            archived: 1,
        }];
        // mismatched_rows model a corrupted response identity.
        // mismatched_rows 模拟损坏的响应身份。
        let mismatched_rows = vec![SessionArchiveStateUpdateRow {
            id: SessionId::from_u128(0x47).to_string(),
            archived: 1,
        }];
        // wrong_state_rows model a response that did not commit the requested result.
        // wrong_state_rows 模拟未提交请求结果的响应。
        let wrong_state_rows = vec![SessionArchiveStateUpdateRow {
            id: session_id.to_string(),
            archived: 0,
        }];
        // invalid_state_rows model a corrupted non-boolean SQLite scalar.
        // invalid_state_rows 模拟损坏的非布尔 SQLite 标量。
        let invalid_state_rows = vec![SessionArchiveStateUpdateRow {
            id: session_id.to_string(),
            archived: 2,
        }];

        assert!(
            validate_session_archive_state_update_rows(session_id, true, &matching_rows).is_ok()
        );
        assert!(
            validate_session_archive_state_update_rows(session_id, true, &[])
                .expect_err("missing transition row should fail")
                .contains("定位信息或归档状态")
        );
        assert!(
            validate_session_archive_state_update_rows(session_id, true, &mismatched_rows)
                .expect_err("mismatched transition identity should fail")
                .contains("意外标识")
        );
        assert!(
            validate_session_archive_state_update_rows(session_id, true, &wrong_state_rows)
                .expect_err("wrong transition result should fail")
                .contains("期望 `1`")
        );
        assert!(
            validate_session_archive_state_update_rows(session_id, true, &invalid_state_rows)
                .expect_err("invalid transition flag should fail")
                .contains("非法标记")
        );
    }

    /// Verifies guarded title updates require one matching active session identity.
    /// 验证受保护的标题更新必须返回一条匹配的活动会话身份。
    #[test]
    fn active_session_title_update_rows_require_one_matching_identity() {
        // session_id is the exact identity submitted to the guarded update.
        // session_id 是提交给受保护更新的精确身份。
        let session_id = SessionId::from_u128(0x44);
        // matching_rows model the only successful `UPDATE ... RETURNING` result.
        // matching_rows 模拟唯一成功的 `UPDATE ... RETURNING` 结果。
        let matching_rows = vec![ActiveSessionTitleUpdateRow {
            id: session_id.to_string(),
        }];
        // missing_rows model a locator removed or archived before title commit.
        // missing_rows 模拟标题提交前已被删除或归档的定位行。
        let missing_rows = Vec::new();
        // mismatched_rows model a corrupted controller response identity.
        // mismatched_rows 模拟损坏的控制器响应身份。
        let mismatched_rows = vec![ActiveSessionTitleUpdateRow {
            id: SessionId::from_u128(0x45).to_string(),
        }];

        assert!(validate_active_session_title_update_rows(session_id, &matching_rows).is_ok());
        assert!(
            validate_active_session_title_update_rows(session_id, &missing_rows)
                .expect_err("missing returned row should fail")
                .contains("状态已改变")
        );
        assert!(
            validate_active_session_title_update_rows(session_id, &mismatched_rows)
                .expect_err("mismatched returned row should fail")
                .contains("意外标识")
        );
    }
}
