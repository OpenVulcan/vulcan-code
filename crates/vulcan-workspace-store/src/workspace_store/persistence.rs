//! Workspace persistence write-path helpers extracted from the main workspace store module.
//! 从主工作台存储模块中抽取的写入持久化路径辅助实现。

#![allow(dead_code)]

use super::*;
use crate::conversation_protocol::conversation_state_to_session_messages;

/// Maintains the storage shape for one already hydrated session without scanning unrelated history.
/// 在不扫描无关历史的前提下维护单个已水合会话的存储形态。
pub async fn maintain_hydrated_session_storage(
    _controller: ControllerRuntimeHandle,
    session_id: SessionId,
    _messages: Vec<SessionMessageEntry>,
) -> Result<SessionMaintenanceReport, String> {
    Ok(SessionMaintenanceReport {
        session_id,
        upserted_message_count: 0,
        runtime_state_persistence_ok: true,
        legacy_chunk_migration: SessionLegacyChunkMigrationReport {
            session_id,
            ..SessionLegacyChunkMigrationReport::default()
        },
    })
}

/// Maintains legacy storage shape for one session without hydrating transcript messages.
/// 在不水合转录消息的前提下维护单个会话的旧存储形态。
pub async fn maintain_legacy_session_storage(
    _controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<SessionMaintenanceReport, String> {
    Ok(SessionMaintenanceReport {
        session_id,
        upserted_message_count: 0,
        runtime_state_persistence_ok: true,
        legacy_chunk_migration: SessionLegacyChunkMigrationReport {
            session_id,
            ..SessionLegacyChunkMigrationReport::default()
        },
    })
}

/// Migrates legacy oversized inline message bodies into chunk rows without returning full bodies to Rust.
/// 在不向 Rust 返回完整正文的前提下，将旧版超大内联消息正文迁移为分片行。
pub(super) async fn migrate_legacy_oversized_session_messages(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<SessionLegacyChunkMigrationReport, String> {
    let mut report = SessionLegacyChunkMigrationReport {
        session_id,
        ..SessionLegacyChunkMigrationReport::default()
    };

    while report.migrated_message_count < SESSION_LEGACY_INLINE_MIGRATION_MAX_MESSAGES_PER_PASS {
        let remaining_limit = SESSION_LEGACY_INLINE_MIGRATION_MAX_MESSAGES_PER_PASS
            .saturating_sub(report.migrated_message_count)
            .min(SESSION_LEGACY_INLINE_MIGRATION_CANDIDATE_LIMIT);
        let candidates =
            query_legacy_oversized_message_candidates(controller, session_id, remaining_limit)
                .await?;
        if candidates.is_empty() {
            break;
        }

        let candidate_count = candidates.len();
        for candidate in candidates {
            let chunk_count = legacy_inline_message_chunk_count(candidate.content_chars)?;
            if chunk_count > SESSION_LEGACY_INLINE_MIGRATION_MAX_CHUNKS_PER_MESSAGE {
                return Err(format!(
                    "旧版超大消息分片数量超出安全上限：session_id={session_id}, message_id={}, content_bytes={}, chunks={}, limit={}",
                    candidate.id,
                    candidate.content_bytes,
                    chunk_count,
                    SESSION_LEGACY_INLINE_MIGRATION_MAX_CHUNKS_PER_MESSAGE
                ));
            }
            let script = legacy_inline_message_chunk_migration_script(session_id, &candidate.id);
            controller
                .client
                .execute_sqlite_script_typed(
                    APP_SPACE_ID,
                    app_sqlite_binding_id(controller),
                    script,
                    Vec::<ControllerSqliteValue>::new(),
                )
                .await
                .map_err(|error| {
                    format!(
                        "迁移旧版超大内联消息失败：session_id={session_id}, message_id={}, error={error}",
                        candidate.id
                    )
                })?;
            report.migrated_message_count += 1;
            report.inserted_chunk_count = report.inserted_chunk_count.saturating_add(chunk_count);
        }

        if candidate_count < remaining_limit {
            break;
        }
    }

    Ok(report)
}

/// Returns whether the session still contains any legacy oversized inline message candidates that have not been chunk-migrated yet.
/// 返回该会话是否仍包含尚未迁移为分片存储的旧版超大内联消息候选。
pub(super) async fn session_has_legacy_oversized_message_candidates(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<bool, String> {
    Ok(
        !query_legacy_oversized_message_candidates(controller, session_id, 1)
            .await?
            .is_empty(),
    )
}

/// Queries legacy oversized inline-message candidates without selecting their full content.
/// 在不选择完整正文的前提下查询旧版超大内联消息候选。
async fn query_legacy_oversized_message_candidates(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    limit: usize,
) -> Result<Vec<StoredLegacyOversizedMessageCandidateRow>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            legacy_oversized_message_candidate_query(),
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(SESSION_MESSAGE_INLINE_CONTENT_BUDGET_BYTES as i64),
                ControllerSqliteValue::Int64(
                    limit.min(SESSION_LEGACY_INLINE_MIGRATION_CANDIDATE_LIMIT) as i64,
                ),
            ],
        )
        .await
        .map_err(|error| format!("查询旧版超大内联消息候选失败：{error}"))?;

    decode_query_rows::<StoredLegacyOversizedMessageCandidateRow>(&result.json_data)
}

/// Returns the lightweight candidate query used by legacy inline-message migration.
/// 返回旧版内联消息迁移使用的轻量候选查询。
pub(super) fn legacy_oversized_message_candidate_query() -> &'static str {
    r#"
SELECT
    m.id,
    length(CAST(m.content AS BLOB)) AS content_bytes,
    length(m.content) AS content_chars
FROM session_messages AS m
WHERE m.session_id = ?1
  AND length(CAST(m.content AS BLOB)) > ?2
  AND NOT EXISTS (
      SELECT 1
      FROM session_message_chunks AS c
      WHERE c.message_id = m.id
      LIMIT 1
  )
ORDER BY m.ordinal DESC
LIMIT ?3;
"#
}

/// Builds one SQLite-side migration script for a legacy oversized inline message.
/// 为单条旧版超大内联消息构建 SQLite 侧迁移脚本。
pub(super) fn legacy_inline_message_chunk_migration_script(
    session_id: SessionId,
    message_id: &str,
) -> String {
    let escaped_session_id = sqlite_escape_text(&session_id.to_string());
    let escaped_message_id = sqlite_escape_text(message_id);
    format!(
        r#"
BEGIN IMMEDIATE TRANSACTION;

WITH
digits(n) AS (
    VALUES (0), (1), (2), (3), (4), (5), (6), (7), (8), (9)
),
numbers(n) AS (
    SELECT d0.n + d1.n * 10 + d2.n * 100 + d3.n * 1000 + d4.n * 10000
    FROM digits AS d0
    CROSS JOIN digits AS d1
    CROSS JOIN digits AS d2
    CROSS JOIN digits AS d3
    CROSS JOIN digits AS d4
),
source(content_text, content_chars) AS (
    SELECT content, length(content)
    FROM session_messages
    WHERE session_id = '{session_id}'
      AND id = '{message_id}'
      AND length(CAST(content AS BLOB)) > {inline_budget_bytes}
      AND NOT EXISTS (
          SELECT 1
          FROM session_message_chunks
          WHERE message_id = '{message_id}'
          LIMIT 1
      )
)
INSERT OR REPLACE INTO session_message_chunks (
    session_id,
    message_id,
    chunk_index,
    content
)
SELECT
    '{session_id}',
    '{message_id}',
    numbers.n,
    substr(source.content_text, numbers.n * {chunk_chars} + 1, {chunk_chars})
FROM source
JOIN numbers ON numbers.n * {chunk_chars} < source.content_chars
ORDER BY numbers.n;

UPDATE session_messages
SET content = substr(content, 1, {preview_chars})
WHERE session_id = '{session_id}'
  AND id = '{message_id}'
  AND length(CAST(content AS BLOB)) > {inline_budget_bytes}
  AND EXISTS (
      SELECT 1
      FROM session_message_chunks
      WHERE message_id = '{message_id}'
      LIMIT 1
  );

COMMIT;
"#,
        session_id = escaped_session_id,
        message_id = escaped_message_id,
        chunk_chars = SESSION_LEGACY_INLINE_MIGRATION_CHUNK_CHARS,
        inline_budget_bytes = SESSION_MESSAGE_INLINE_CONTENT_BUDGET_BYTES,
        preview_chars = SESSION_MESSAGE_CHUNK_PREVIEW_CHARS
    )
}

/// Estimates how many chunk rows a legacy inline message will produce from its character length.
/// 根据旧版内联消息的字符长度估算其会生成多少分片行。
pub(super) fn legacy_inline_message_chunk_count(content_chars: i64) -> Result<usize, String> {
    if content_chars <= 0 {
        return Ok(0);
    }

    let content_chars = content_chars as u128;
    let chunk_chars = SESSION_LEGACY_INLINE_MIGRATION_CHUNK_CHARS.max(1) as u128;
    let chunk_count = content_chars.div_ceil(chunk_chars);
    if chunk_count > usize::MAX as u128 {
        return Err(format!("旧版超大消息字符长度无法安全转换：{content_chars}"));
    }

    Ok(chunk_count as usize)
}

/// Persists the current visible workspace snapshot back into the controller-backed SQLite database.
/// 将当前可见工作台快照回写到控制器承载的 SQLite 数据库。
pub async fn persist_workspace_snapshot(
    controller: ControllerRuntimeHandle,
    snapshot: &WorkspaceSnapshot,
) -> Result<WorkspaceSnapshotPersistOutcome, String> {
    let mut warnings = Vec::new();
    let existing_active_project_ids = query_active_project_ids(&controller).await?;
    let existing_active_session_ids = query_active_session_ids(&controller).await?;
    let visible_project_ids = snapshot
        .task_projects
        .iter()
        .map(|project| project.id.clone())
        .collect::<BTreeSet<_>>();
    let visible_session_ids = snapshot
        .sessions
        .iter()
        .map(|session| session.id.to_string())
        .collect::<BTreeSet<_>>();
    let all_projects = snapshot
        .task_projects
        .iter()
        .cloned()
        .map(|project| (project, false))
        .chain(
            snapshot
                .archived_task_projects
                .iter()
                .cloned()
                .map(|project| (project, true)),
        )
        .collect::<Vec<_>>();
    let all_sessions = snapshot
        .sessions
        .iter()
        .cloned()
        .map(|session| (session, false))
        .chain(
            snapshot
                .archived_sessions
                .iter()
                .cloned()
                .map(|session| (session, true)),
        )
        .collect::<Vec<_>>();
    let all_task_projects = snapshot
        .task_projects
        .iter()
        .cloned()
        .chain(snapshot.archived_task_projects.iter().cloned())
        .collect::<Vec<_>>();

    upsert_projects(&controller, &all_projects).await?;
    upsert_sessions(&controller, &all_task_projects, &all_sessions).await?;
    archive_missing_projects(
        &controller,
        &existing_active_project_ids,
        &visible_project_ids,
    )
    .await?;
    archive_missing_sessions(
        &controller,
        &existing_active_session_ids,
        &visible_session_ids,
        &mut warnings,
    )
    .await?;
    replace_session_event_stores(
        &controller,
        &snapshot.sessions,
        &snapshot.session_messages,
        &snapshot.session_compactions,
        &snapshot.session_runtime_state,
        &snapshot.archived_sessions,
        &snapshot.archived_session_messages,
        &snapshot.archived_session_compactions,
        &snapshot.archived_session_runtime_state,
        &mut warnings,
    )
    .await?;
    replace_session_compactions(
        &controller,
        &snapshot.sessions,
        &snapshot.session_compactions,
        &snapshot.archived_sessions,
        &snapshot.archived_session_compactions,
    )
    .await?;
    replace_session_runtime_state(
        &controller,
        &snapshot.sessions,
        &snapshot.session_runtime_state,
        &snapshot.archived_sessions,
        &snapshot.archived_session_runtime_state,
        &snapshot.hydrated_session_ids,
    )
    .await?;

    Ok(WorkspaceSnapshotPersistOutcome { warnings })
}

/// Queries one persisted session summary row so single-session maintenance can reuse the event-store writer without a full workspace snapshot.
/// 查询单条持久化会话摘要行，使单会话维护无需完整工作台快照也能复用事件存储写入器。
async fn query_persisted_session_summary(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<SessionSummary>, String> {
    #[derive(Debug, Deserialize)]
    struct PersistedSessionSummaryRow {
        /// Stable owning project identifier.
        /// 稳定所属项目标识。
        project_id: String,
        /// Visible session title.
        /// 可见会话标题。
        title: String,
        /// Persisted title source key.
        /// 持久化标题来源键。
        title_source: String,
        /// Updated-at timestamp stored as Unix milliseconds text.
        /// 以 Unix 毫秒文本存储的更新时间。
        updated_at: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT project_id, title, title_source, updated_at FROM sessions WHERE id = ?1 LIMIT 1;",
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话事件存储摘要失败：{error}"))?;
    let row = decode_query_rows::<PersistedSessionSummaryRow>(&result.json_data)?
        .into_iter()
        .next();

    Ok(row.map(|row| {
        SessionSummary::new(
            session_id,
            Some(row.project_id),
            row.title,
            String::new(),
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis_text(&row.updated_at),
        )
        .with_title_source(SessionTitleSource::from_persisted_key(&row.title_source))
    }))
}

/// Queries all compaction checkpoints currently mirrored for one session so single-session maintenance can persist the matching file-backed checkpoint set.
/// 查询单个会话当前镜像保存的全部压缩检查点，以便单会话维护时同步持久化对应的文件化检查点集合。
async fn query_session_compaction_entries(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Vec<SessionCompactionEntry>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT c.id, c.source_until_message_id, c.source_until_turn_index, c.context_start_message_id, c.retained_tail_start_turn_index, c.summary_message_id, c.created_at
FROM session_compactions AS c
WHERE c.session_id = ?1
ORDER BY c.created_at ASC;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话压缩检查点失败：{error}"))?;

    Ok(decode_query_rows::<StoredCompactionRow>(&result.json_data)?
        .into_iter()
        .map(stored_compaction_row_to_entry)
        .collect())
}

/// Queries one persisted runtime-state entry so single-session maintenance can mirror the small runtime cache beside the event stream.
/// 查询单条持久化运行时状态项，使单会话维护可以同步写出事件流旁边的小型运行时缓存。
async fn query_session_runtime_state_entry(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT r.session_id, r.active_agent_id, r.active_provider_id, r.active_model_id, r.last_compaction_id, r.context_start_message_id, r.ui_state_json, r.provider_usage_json, r.conversation_state_json, r.continuation_json, r.context_injection_json, r.vmm_json, r.vmm_writeback_outbox_json, r.updated_at
FROM session_runtime_state AS r
WHERE r.session_id = ?1
LIMIT 1;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话运行时状态失败：{error}"))?;

    Ok(
        decode_query_rows::<StoredRuntimeStateRow>(&result.json_data)?
            .into_iter()
            .next()
            .map(stored_runtime_state_row_to_entry),
    )
}

/// Mirrors one already persisted event-store rewrite back into SQLite indexes for single-session maintenance.
/// 将单个已写出的事件存储重写结果镜像回 SQLite 索引，用于单会话维护路径。
async fn persist_single_session_event_store_projection(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    persist_result: &SessionEventStorePersistResult,
) -> Result<(), String> {
    let session_id_text = session.id.to_string();
    let session_ids = vec![session_id_text];
    delete_workspace_history_search_documents_for_session_ids(
        controller,
        &session_ids,
        "删除旧工作台历史搜索索引失败",
    )
    .await?;
    delete_session_scoped_rows(
        controller,
        "workspace_history_search_docs",
        &session_ids,
        "删除旧工作台历史搜索投影失败",
    )
    .await?;
    delete_session_scoped_rows(
        controller,
        "session_turn_index",
        &session_ids,
        "删除旧会话 turn 索引失败",
    )
    .await?;
    delete_session_scoped_rows(
        controller,
        "session_message_chunks",
        &session_ids,
        "删除旧会话消息分片失败",
    )
    .await?;
    delete_session_scoped_rows(
        controller,
        "session_messages",
        &session_ids,
        "删除旧会话消息正文失败",
    )
    .await?;

    let single_result = vec![(session.clone(), persist_result.clone())];
    upsert_session_event_store_session_rows(controller, &single_result).await?;
    upsert_session_turn_index_rows(controller, &single_result).await?;
    upsert_session_event_search_documents(controller, &single_result).await
}

/// Queries the current project ownership and title needed to route one session into workspace-history search indexes.
/// 查询将某个会话路由到工作台历史搜索索引所需的当前项目归属与标题。
async fn query_session_workspace_history_search_routing(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<(String, String)>, String> {
    #[derive(Debug, Deserialize)]
    struct SessionRoutingRow {
        /// Stable project identifier owning the session.
        /// 拥有该会话的稳定项目标识。
        project_id: String,
        /// Human-readable session title.
        /// 人类可读的会话标题。
        title: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT project_id, title FROM sessions WHERE id = ?1 LIMIT 1;",
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话搜索索引归属失败：{error}"))?;

    Ok(decode_query_rows::<SessionRoutingRow>(&result.json_data)?
        .into_iter()
        .next()
        .map(|row| (row.project_id, row.title)))
}

/// Upserts all visible project rows while clearing the archive flag.
/// 在清理归档标记的同时上写全部可见项目行。
async fn upsert_projects(
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
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
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

/// Soft-archives active projects missing from the current visible snapshot.
/// 将当前可见快照中缺失的激活项目软归档。
async fn archive_missing_projects(
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

/// Upserts all visible session rows using the current project-to-session bindings.
/// 使用当前项目到会话的绑定关系上写全部可见会话行。
async fn upsert_sessions(
    controller: &ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    sessions: &[(SessionSummary, bool)],
) -> Result<(), String> {
    if sessions.is_empty() {
        return Ok(());
    }

    let session_owner_map = task_projects
        .iter()
        .flat_map(|project| {
            project
                .session_ids
                .iter()
                .enumerate()
                .map(move |(index, session_id)| (*session_id, (project.id.clone(), index)))
        })
        .collect::<BTreeMap<SessionId, (String, usize)>>();
    let now = current_unix_millis();
    let mut batch_params = Vec::new();

    for (session, archived) in sessions {
        if let Some((project_id, sort_order)) = session_owner_map.get(&session.id) {
            let updated_at =
                approximate_unix_millis_from_relative_label(&session.last_updated_label)
                    .unwrap_or(now)
                    .to_string();
            batch_params.push(vec![
                ControllerSqliteValue::String(session.id.to_string()),
                ControllerSqliteValue::String(project_id.clone()),
                ControllerSqliteValue::String(session.title.clone()),
                ControllerSqliteValue::String(session.title_source.as_persisted_key().to_string()),
                ControllerSqliteValue::Int64(i64::from(*archived)),
                ControllerSqliteValue::Int64(*sort_order as i64),
                ControllerSqliteValue::String(now.to_string()),
                ControllerSqliteValue::String(updated_at),
            ]);
        }
    }

    if batch_params.is_empty() {
        return Ok(());
    }

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO sessions (
    id,
    project_id,
    title,
    title_source,
    archived,
    sort_order,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(id) DO UPDATE SET
    project_id = excluded.project_id,
    title = excluded.title,
    title_source = excluded.title_source,
    archived = excluded.archived,
    sort_order = excluded.sort_order,
    updated_at = excluded.updated_at;
"#,
            batch_params,
        )
        .await
        .map_err(|error| format!("写入会话列表失败：{error}"))?;

    Ok(())
}

/// Soft-archives active sessions missing from the current visible snapshot.
/// 将当前可见快照中缺失的激活会话软归档。
async fn archive_missing_sessions(
    controller: &ControllerRuntimeHandle,
    existing_active_ids: &BTreeSet<String>,
    visible_ids: &BTreeSet<String>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let missing_ids = existing_active_ids
        .iter()
        .filter(|session_id| !visible_ids.contains(*session_id))
        .cloned()
        .collect::<Vec<_>>();

    if missing_ids.is_empty() {
        return Ok(());
    }

    let now = current_unix_millis_text();
    let params = missing_ids
        .iter()
        .cloned()
        .map(|session_id| {
            vec![
                ControllerSqliteValue::String(now.clone()),
                ControllerSqliteValue::String(session_id),
            ]
        })
        .collect::<Vec<_>>();

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "UPDATE sessions SET archived = 1, updated_at = ?1 WHERE id = ?2;",
            params,
        )
        .await
        .map_err(|error| format!("软归档会话失败：{error}"))?;
    if let Err(error) = delete_workspace_history_search_documents_for_session_ids(
        controller,
        &missing_ids,
        "删除已软归档会话的搜索索引失败",
    )
    .await
    {
        warnings.push(workspace_history_search_sync_warning(
            "软归档缺失会话后清理历史搜索索引失败",
            missing_ids.len(),
            &error,
        ));
    }

    Ok(())
}

/// Rewrites canonical session event stores, mirrors turn/search indexes into SQLite, and clears obsolete message-table payloads.
/// 重写规范会话事件存储，将 turn 与搜索索引镜像进 SQLite，并清理过时的消息表载荷。
async fn replace_session_event_stores(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    archived_sessions: &[SessionSummary],
    archived_session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    archived_session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    archived_session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    warnings: &mut Vec<String>,
) -> Result<(), String> {
    let all_sessions = sessions
        .iter()
        .cloned()
        .chain(archived_sessions.iter().cloned())
        .collect::<Vec<_>>();
    if all_sessions.is_empty() {
        return Ok(());
    }

    let rewrite_session_ids = collect_session_event_store_rewrite_ids(
        &all_sessions,
        session_messages,
        session_compactions,
        session_runtime_state,
        archived_session_messages,
        archived_session_compactions,
        archived_session_runtime_state,
    );
    if rewrite_session_ids.is_empty() {
        return Ok(());
    }

    let mut persisted_results = Vec::with_capacity(rewrite_session_ids.len());
    for session in all_sessions
        .iter()
        .filter(|session| rewrite_session_ids.contains(&session.id))
    {
        let messages = session_messages
            .get(&session.id)
            .or_else(|| archived_session_messages.get(&session.id))
            .map(Vec::as_slice);
        let runtime_state = session_runtime_state
            .get(&session.id)
            .or_else(|| archived_session_runtime_state.get(&session.id));
        let compactions = session_compactions
            .get(&session.id)
            .or_else(|| archived_session_compactions.get(&session.id))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let rebuilt_messages;
        let messages = if let Some(messages) = messages {
            messages
        } else if let Some(runtime_state) = runtime_state {
            rebuilt_messages =
                conversation_state_to_session_messages(&runtime_state.conversation_state);
            rebuilt_messages.as_slice()
        } else {
            &[]
        };
        let persist_result =
            persist_session_event_store(session, messages, runtime_state, compactions)?;
        persisted_results.push((session.clone(), persist_result));
    }

    let session_ids = all_sessions
        .iter()
        .filter(|session| rewrite_session_ids.contains(&session.id))
        .map(|session| session.id.to_string())
        .collect::<Vec<_>>();
    let mut search_projection_ready = true;
    if let Err(error) = delete_workspace_history_search_documents_for_session_ids(
        controller,
        &session_ids,
        "删除旧工作台历史搜索索引失败",
    )
    .await
    {
        warnings.push(workspace_history_search_sync_warning(
            "重写会话事件存储前清理历史搜索索引失败",
            session_ids.len(),
            &error,
        ));
        search_projection_ready = false;
    }
    delete_session_scoped_rows(
        controller,
        "workspace_history_search_docs",
        &session_ids,
        "删除旧工作台历史搜索投影失败",
    )
    .await?;
    delete_session_scoped_rows(
        controller,
        "session_turn_index",
        &session_ids,
        "删除旧会话 turn 索引失败",
    )
    .await?;
    delete_session_scoped_rows(
        controller,
        "session_message_chunks",
        &session_ids,
        "删除旧会话消息分片失败",
    )
    .await?;
    delete_session_scoped_rows(
        controller,
        "session_messages",
        &session_ids,
        "删除旧会话消息正文失败",
    )
    .await?;

    upsert_session_event_store_session_rows(controller, &persisted_results).await?;
    upsert_session_turn_index_rows(controller, &persisted_results).await?;
    if search_projection_ready {
        if let Err(error) =
            upsert_session_event_search_documents(controller, &persisted_results).await
        {
            warnings.push(workspace_history_search_sync_warning(
                "重写会话事件存储后补写历史搜索索引失败",
                session_ids.len(),
                &error,
            ));
        }
    }

    Ok(())
}

/// Builds one non-fatal warning describing that auxiliary workspace-history search synchronization lagged behind a successful snapshot save.
/// 构建一条非致命告警，用于描述辅助工作台历史搜索同步落后于一次成功的快照保存。
fn workspace_history_search_sync_warning(
    operation: &str,
    session_count: usize,
    error: &str,
) -> String {
    format!(
        "{operation}：涉及 {session_count} 个会话。核心工作台状态已继续保存，但历史搜索索引需要稍后重建。原始错误：{error}"
    )
}

/// Collects the session identifiers whose canonical event-store files must actually be rewritten during one snapshot flush.
/// 收集一次快照刷新期间确实需要重写规范事件存储文件的会话标识集合。
fn collect_session_event_store_rewrite_ids(
    all_sessions: &[SessionSummary],
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    archived_session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    archived_session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    archived_session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
) -> BTreeSet<SessionId> {
    all_sessions
        .iter()
        .filter(|session| {
            session_messages.contains_key(&session.id)
                || session_compactions.contains_key(&session.id)
                || session_runtime_state.contains_key(&session.id)
                || archived_session_messages.contains_key(&session.id)
                || archived_session_compactions.contains_key(&session.id)
                || archived_session_runtime_state.contains_key(&session.id)
        })
        .map(|session| session.id)
        .collect()
}

/// Permanently deletes one archived session together with all persisted transcript and runtime artifacts.
/// 彻底删除一个已归档会话及其全部已持久化的转录与运行时工件。
pub async fn permanently_delete_archived_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let session_id_text = session_id.to_string();
    let session_ids = vec![session_id_text.clone()];
    let project_id = query_persisted_session_summary(&controller, session_id)
        .await?
        .and_then(|session| session.project_id);

    delete_session_scoped_rows(
        &controller,
        "session_message_chunks",
        &session_ids,
        "删除归档会话转录消息分片失败",
    )
    .await?;
    delete_workspace_history_search_documents_for_session_ids(
        &controller,
        &session_ids,
        "删除归档会话搜索索引失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_messages",
        &session_ids,
        "删除归档会话转录消息失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_compactions",
        &session_ids,
        "删除归档会话压缩检查点失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_runtime_state",
        &session_ids,
        "删除归档会话运行时状态失败",
    )
    .await?;

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(&controller),
            "DELETE FROM sessions WHERE id = ?1 AND archived != 0;",
            vec![vec![ControllerSqliteValue::String(session_id_text)]],
        )
        .await
        .map_err(|error| format!("删除归档会话记录失败：{error}"))?;
    if let Some(project_id) = project_id.as_deref() {
        remove_session_event_store(project_id, session_id)?;
    }

    Ok(())
}

/// Permanently deletes one archived project together with all child archived sessions and persisted artifacts.
/// 彻底删除一个已归档项目及其全部子归档会话与已持久化工件。
pub async fn permanently_delete_archived_project(
    controller: ControllerRuntimeHandle,
    project_id: String,
    session_ids: Vec<SessionId>,
) -> Result<(), String> {
    let project_index_name = workspace_history_search_project_index_name(&project_id);
    let session_id_texts = session_ids
        .iter()
        .copied()
        .map(|session_id| session_id.to_string())
        .collect::<Vec<_>>();

    delete_session_scoped_rows(
        &controller,
        "session_message_chunks",
        &session_id_texts,
        "删除归档项目下的转录消息分片失败",
    )
    .await?;
    delete_workspace_history_search_documents_for_session_ids(
        &controller,
        &session_id_texts,
        "删除归档项目下的搜索索引失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_messages",
        &session_id_texts,
        "删除归档项目下的转录消息失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_compactions",
        &session_id_texts,
        "删除归档项目下的压缩检查点失败",
    )
    .await?;
    delete_session_scoped_rows(
        &controller,
        "session_runtime_state",
        &session_id_texts,
        "删除归档项目下的运行时状态失败",
    )
    .await?;

    if !session_id_texts.is_empty() {
        let session_delete_params = session_id_texts
            .into_iter()
            .map(|session_id| vec![ControllerSqliteValue::String(session_id)])
            .collect::<Vec<_>>();
        controller
            .client
            .execute_sqlite_batch_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(&controller),
                "DELETE FROM sessions WHERE id = ?1 AND archived != 0;",
                session_delete_params,
            )
            .await
            .map_err(|error| format!("删除归档项目下的会话记录失败：{error}"))?;
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
    drop_workspace_history_search_indexes(
        &controller,
        &[project_index_name],
        "删除归档项目对应的搜索索引失败",
    )
    .await?;
    for session_id in &session_ids {
        remove_session_event_store(project_id.as_str(), *session_id)?;
    }

    Ok(())
}

/// Upserts loaded transcript message rows while leaving not-yet-loaded history intact.
/// 在保留尚未加载历史的前提下上写已加载的转录消息行。
async fn replace_session_messages(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
    archived_sessions: &[SessionSummary],
    archived_session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
) -> Result<(), String> {
    let mut message_batch_params = Vec::new();
    let mut chunk_batch_params = Vec::new();
    let mut search_batch_params = Vec::new();
    push_session_message_batch_rows(
        controller,
        &mut message_batch_params,
        &mut chunk_batch_params,
        &mut search_batch_params,
        true,
        sessions,
        session_messages,
    )
    .await?;
    push_session_message_batch_rows(
        controller,
        &mut message_batch_params,
        &mut chunk_batch_params,
        &mut search_batch_params,
        false,
        archived_sessions,
        archived_session_messages,
    )
    .await?;

    upsert_session_message_batches(
        controller,
        message_batch_params,
        chunk_batch_params,
        search_batch_params,
    )
    .await
}

/// Mirrors latest turn/event positions from the file-backed event store back into the lightweight SQLite session rows.
/// 将文件化事件存储中的最近 turn 与事件位置镜像回轻量 SQLite 会话行。
async fn upsert_session_event_store_session_rows(
    controller: &ControllerRuntimeHandle,
    persisted_results: &[(SessionSummary, SessionEventStorePersistResult)],
) -> Result<(), String> {
    let params = persisted_results
        .iter()
        .map(|(session, result)| {
            vec![
                ControllerSqliteValue::Int64(result.latest_turn_index),
                ControllerSqliteValue::Int64(result.latest_event_seq),
                ControllerSqliteValue::String(current_unix_millis_text()),
                ControllerSqliteValue::String(session.id.to_string()),
            ]
        })
        .collect::<Vec<_>>();
    execute_sqlite_batch_safely(
        controller,
        "sessions",
        "UPDATE sessions SET latest_turn_index = ?1, latest_event_seq = ?2, updated_at = ?3 WHERE id = ?4;",
        params,
        "更新会话事件存储镜像位置失败",
    )
    .await
}

/// Mirrors canonical turn-index rows derived from the file-backed event store into SQLite for paging and checkpoint routing.
/// 将文件化事件存储派生出的规范 turn 索引行镜像到 SQLite，以支持分页与检查点路由。
async fn upsert_session_turn_index_rows(
    controller: &ControllerRuntimeHandle,
    persisted_results: &[(SessionSummary, SessionEventStorePersistResult)],
) -> Result<(), String> {
    let params = persisted_results
        .iter()
        .flat_map(|(session, result)| {
            result.turn_rows.iter().map(move |row| {
                vec![
                    ControllerSqliteValue::String(session.id.to_string()),
                    ControllerSqliteValue::String(row.turn_id.clone()),
                    ControllerSqliteValue::Int64(row.turn_index),
                    ControllerSqliteValue::Int64(row.start_seq),
                    ControllerSqliteValue::Int64(row.end_seq),
                    ControllerSqliteValue::String(row.turn_state.clone()),
                    optional_sqlite_string(&row.summary_message_id),
                ]
            })
        })
        .collect::<Vec<_>>();
    if params.is_empty() {
        return Ok(());
    }

    execute_sqlite_batch_safely(
        controller,
        "session_turn_index",
        r#"
INSERT INTO session_turn_index (
    session_id,
    turn_id,
    turn_index,
    start_seq,
    end_seq,
    turn_state,
    summary_message_id
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7);
"#,
        params,
        "写入会话事件 turn 索引失败",
    )
    .await
}

/// Mirrors search projection documents derived from the file-backed event store into controller-managed FTS and the lightweight metadata table.
/// 将文件化事件存储派生出的搜索投影文档镜像到控制器托管 FTS 与轻量元数据表。
async fn upsert_session_event_search_documents(
    controller: &ControllerRuntimeHandle,
    persisted_results: &[(SessionSummary, SessionEventStorePersistResult)],
) -> Result<(), String> {
    let documents = persisted_results
        .iter()
        .flat_map(|(_, result)| {
            result.search_documents.iter().cloned().map(|document| {
                WorkspaceHistorySearchDocumentUpsert {
                    message_id: document.document_id,
                    session_id: document.session_id,
                    project_id: document.project_id,
                    event_id: document.event_id,
                    turn_index: document.turn_index,
                    ordinal: document.ordinal,
                    role: document.role,
                    created_at: document.created_at,
                    content: document.content,
                }
            })
        })
        .collect::<Vec<_>>();
    upsert_workspace_history_search_documents(controller, documents).await
}

/// Upserts prepared transcript rows and rewrites only their own chunk rows.
/// 上写已预处理的转录行，并且只重写这些消息自身的分片行。
async fn upsert_session_message_batches(
    controller: &ControllerRuntimeHandle,
    message_batch_params: Vec<Vec<ControllerSqliteValue>>,
    chunk_batch_params: Vec<Vec<ControllerSqliteValue>>,
    search_batch_params: Vec<WorkspaceHistorySearchDocumentUpsert>,
) -> Result<(), String> {
    if message_batch_params.is_empty() {
        return Ok(());
    }

    let message_ids = message_batch_params
        .iter()
        .filter_map(|row| match row.first() {
            Some(ControllerSqliteValue::String(message_id)) => Some(message_id.clone()),
            _ => None,
        })
        .collect::<Vec<_>>();
    delete_session_message_chunks_for_message_ids(
        controller,
        &message_ids,
        "删除当前窗口消息的旧正文分片失败",
    )
    .await?;
    delete_workspace_history_search_documents_for_message_ids(
        controller,
        &message_ids,
        "删除当前窗口消息的旧搜索索引失败",
    )
    .await?;

    execute_sqlite_batch_safely(
        controller,
        "session_messages",
        session_message_upsert_statement(),
        message_batch_params,
        "写入会话转录消息失败",
    )
    .await?;

    if !chunk_batch_params.is_empty() {
        execute_sqlite_batch_safely(
            controller,
            "session_message_chunks",
            session_message_chunk_upsert_statement(),
            chunk_batch_params,
            "写入会话转录消息分片失败",
        )
        .await?;
    }

    upsert_workspace_history_search_documents(controller, search_batch_params).await?;

    Ok(())
}

/// Returns the upsert SQL used to persist chunk rows safely across overlapping maintenance tasks.
/// 返回用于在重叠维护任务间安全持久化分片行的 upsert SQL。
pub(super) fn session_message_chunk_upsert_statement() -> &'static str {
    r#"
INSERT INTO session_message_chunks (
    session_id,
    message_id,
    chunk_index,
    content
) VALUES (?1, ?2, ?3, ?4)
ON CONFLICT(message_id, chunk_index) DO UPDATE SET
    session_id = excluded.session_id,
    content = excluded.content;
"#
}

/// Replaces all compaction checkpoint rows for the currently visible sessions while leaving archived sessions intact.
/// 在保留已归档会话数据的前提下替换当前可见会话的全部压缩检查点行。
async fn replace_session_compactions(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
    archived_sessions: &[SessionSummary],
    archived_session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
) -> Result<(), String> {
    let active_session_ids = sessions
        .iter()
        .chain(archived_sessions.iter())
        .filter(|session| {
            session_compactions.contains_key(&session.id)
                || archived_session_compactions.contains_key(&session.id)
        })
        .map(|session| session.id.to_string())
        .collect::<Vec<_>>();

    delete_session_scoped_rows(
        controller,
        "session_compactions",
        &active_session_ids,
        "删除当前可见会话的旧压缩节点失败",
    )
    .await?;

    let mut batch_params = Vec::new();
    push_session_compaction_batch_rows(&mut batch_params, sessions, session_compactions);
    push_session_compaction_batch_rows(
        &mut batch_params,
        archived_sessions,
        archived_session_compactions,
    );

    if batch_params.is_empty() {
        return Ok(());
    }

    execute_sqlite_batch_safely(
        controller,
        "session_compactions",
        r#"
INSERT INTO session_compactions (
    id,
    session_id,
    source_until_message_id,
    source_until_turn_index,
    context_start_message_id,
    retained_tail_start_turn_index,
    summary_message_id,
    created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);
"#,
        batch_params,
        "写入会话压缩节点失败",
    )
    .await?;

    Ok(())
}

/// Replaces all runtime-state rows for the currently visible sessions while leaving archived sessions intact.
/// 在保留已归档会话数据的前提下替换当前可见会话的全部运行时状态行。
async fn replace_session_runtime_state(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    archived_sessions: &[SessionSummary],
    archived_session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
    hydrated_session_ids: &BTreeSet<SessionId>,
) -> Result<(), String> {
    let active_session_ids = sessions
        .iter()
        .chain(archived_sessions.iter())
        .filter(|session| hydrated_session_ids.contains(&session.id))
        .map(|session| session.id.to_string())
        .collect::<Vec<_>>();

    delete_session_scoped_rows(
        controller,
        "session_runtime_state",
        &active_session_ids,
        "删除当前可见会话的旧运行时状态失败",
    )
    .await?;

    let mut batch_params = Vec::new();
    push_session_runtime_state_batch_rows(&mut batch_params, sessions, session_runtime_state);
    push_session_runtime_state_batch_rows(
        &mut batch_params,
        archived_sessions,
        archived_session_runtime_state,
    );

    if batch_params.is_empty() {
        return Ok(());
    }

    execute_sqlite_batch_safely(
        controller,
        "session_runtime_state",
        r#"
INSERT INTO session_runtime_state (
    session_id,
    active_agent_id,
    active_provider_id,
    active_model_id,
    last_compaction_id,
    context_start_message_id,
    ui_state_json,
    provider_usage_json,
    conversation_state_json,
    continuation_json,
    context_injection_json,
    vmm_json,
    vmm_writeback_outbox_json,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14);
"#,
        batch_params,
        "写入会话运行时状态失败",
    )
    .await?;

    Ok(())
}

/// Queries existing ordinals and turn indexes for loaded entries so partial upserts keep their positions.
/// 查询已加载消息已有的序号与轮次索引，确保局部上写保留原位置。
async fn query_session_message_positions_for_entries(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    messages: &[SessionMessageEntry],
) -> Result<BTreeMap<String, StoredSessionMessagePositionRow>, String> {
    let message_ids = messages
        .iter()
        .filter_map(|message| {
            let message_id = message.id.trim();
            (!message_id.is_empty()).then(|| message_id.to_string())
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let mut positions = BTreeMap::new();
    for chunk in message_ids.chunks(SESSION_MESSAGE_POSITION_QUERY_ID_LIMIT) {
        let chunk_positions =
            query_session_message_positions_for_ids(controller, session_id, chunk).await?;
        positions.extend(
            chunk_positions
                .into_iter()
                .map(|position| (position.id.clone(), position)),
        );
    }

    Ok(positions)
}

/// Queries existing persisted positions for a bounded set of message identifiers.
/// 查询一组有界消息标识对应的已持久化位置。
async fn query_session_message_positions_for_ids(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_ids: &[String],
) -> Result<Vec<StoredSessionMessagePositionRow>, String> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (2..message_ids.len() + 2)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = format!(
        r#"
SELECT id, ordinal, turn_index
FROM session_messages
WHERE session_id = ?1 AND id IN ({placeholders})
ORDER BY ordinal ASC;
"#
    );
    let mut params = Vec::with_capacity(message_ids.len() + 1);
    params.push(ControllerSqliteValue::String(session_id.to_string()));
    params.extend(
        message_ids
            .iter()
            .cloned()
            .map(ControllerSqliteValue::String),
    );

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| format!("查询会话消息持久化位置失败：{error}"))?;

    decode_query_rows::<StoredSessionMessagePositionRow>(&result.json_data)
}

/// Queries the maximum persisted ordinal and turn index for one session without loading bodies.
/// 在不加载正文的前提下查询单个会话已持久化的最大序号与轮次索引。
async fn query_session_message_max_position(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(i64, i64), String> {
    #[derive(Deserialize)]
    struct MaxPositionRow {
        /// Maximum persisted ordinal or -1 when the session has no rows.
        /// 最大已持久化序号；会话没有行时为 -1。
        max_ordinal: i64,
        /// Maximum persisted turn index or -1 when the session has no rows.
        /// 最大已持久化轮次索引；会话没有行时为 -1。
        max_turn_index: i64,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT
    COALESCE(MAX(ordinal), -1) AS max_ordinal,
    COALESCE(MAX(turn_index), -1) AS max_turn_index
FROM session_messages
WHERE session_id = ?1;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话消息最大位置失败：{error}"))?;

    let rows = decode_query_rows::<MaxPositionRow>(&result.json_data)?;
    let row = rows.into_iter().next().unwrap_or(MaxPositionRow {
        max_ordinal: -1,
        max_turn_index: -1,
    });
    Ok((row.max_ordinal, row.max_turn_index))
}

/// Pushes transcript-message batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的转录消息批量行推入共享 SQLite 批处理载荷。
async fn push_session_message_batch_rows(
    controller: &ControllerRuntimeHandle,
    message_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    chunk_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    search_batch_params: &mut Vec<WorkspaceHistorySearchDocumentUpsert>,
    index_search_documents: bool,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<SessionId, Vec<SessionMessageEntry>>,
) -> Result<(), String> {
    for session in sessions {
        if let Some(messages) = session_messages.get(&session.id) {
            let persisted_positions =
                query_session_message_positions_for_entries(controller, session.id, messages)
                    .await?;
            let (max_ordinal, max_turn_index) =
                query_session_message_max_position(controller, session.id).await?;
            let project_id = session.project_id.as_deref().unwrap_or_default();
            push_session_message_rows_for_session(
                &persisted_positions,
                max_ordinal,
                max_turn_index,
                message_batch_params,
                chunk_batch_params,
                search_batch_params,
                project_id,
                index_search_documents && !project_id.trim().is_empty(),
                session.id,
                session.title.as_str(),
                messages,
            );
        }
    }

    Ok(())
}

/// Pushes transcript-message batch rows for one concrete session identifier.
/// 为一个明确的会话标识推入转录消息批处理行。
pub(super) fn push_session_message_rows_for_session(
    persisted_positions: &BTreeMap<String, StoredSessionMessagePositionRow>,
    max_persisted_ordinal: i64,
    max_persisted_turn_index: i64,
    message_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    chunk_batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    search_batch_params: &mut Vec<WorkspaceHistorySearchDocumentUpsert>,
    project_id: &str,
    index_search_documents: bool,
    session_id: SessionId,
    _session_title: &str,
    messages: &[SessionMessageEntry],
) {
    let mut next_new_ordinal = max_persisted_ordinal.saturating_add(1).max(0);
    let mut current_turn_index = max_persisted_turn_index.max(0);
    let mut has_prior_turn_context = max_persisted_turn_index >= 0;
    for (local_index, message) in messages.iter().enumerate() {
        let meta_json = serialize_session_message_meta_to_json(message);
        let message_id = if message.id.trim().is_empty() {
            format!("session-{}-message-{}", session_id, local_index)
        } else {
            message.id.clone()
        };
        let (ordinal, turn_index) = if let Some(position) = persisted_positions.get(&message_id) {
            current_turn_index = position.turn_index.max(0);
            has_prior_turn_context = true;
            (position.ordinal, position.turn_index)
        } else {
            if session_message_starts_new_turn(message) && has_prior_turn_context {
                current_turn_index = current_turn_index.saturating_add(1);
            }
            has_prior_turn_context = true;
            let ordinal = next_new_ordinal;
            next_new_ordinal = next_new_ordinal.saturating_add(1);
            (ordinal, current_turn_index)
        };
        let prepared_body = prepare_session_message_body_for_storage(&message.body);

        message_batch_params.push(vec![
            ControllerSqliteValue::String(message_id.clone()),
            ControllerSqliteValue::String(session_id.to_string()),
            ControllerSqliteValue::Int64(ordinal),
            ControllerSqliteValue::String(role_from_session_kind(message.kind).to_string()),
            ControllerSqliteValue::String(message.record_kind.as_persisted_key().to_string()),
            ControllerSqliteValue::Int64(i64::from(message.visible)),
            ControllerSqliteValue::Int64(turn_index),
            ControllerSqliteValue::String(prepared_body.inline_content),
            optional_sqlite_string(&meta_json),
            ControllerSqliteValue::String(non_empty_timestamp_text(&message.created_at)),
        ]);

        chunk_batch_params.extend(prepared_body.chunks.into_iter().enumerate().map(
            |(chunk_index, chunk)| {
                vec![
                    ControllerSqliteValue::String(session_id.to_string()),
                    ControllerSqliteValue::String(message_id.clone()),
                    ControllerSqliteValue::Int64(chunk_index as i64),
                    ControllerSqliteValue::String(chunk),
                ]
            },
        ));

        if index_search_documents
            && session_message_is_workspace_history_search_indexable(message)
            && !message.body.trim().is_empty()
        {
            search_batch_params.push(WorkspaceHistorySearchDocumentUpsert {
                message_id,
                session_id: session_id.to_string(),
                project_id: project_id.to_string(),
                event_id: String::new(),
                turn_index,
                ordinal,
                role: role_from_session_kind(message.kind).to_string(),
                created_at: non_empty_timestamp_text(&message.created_at),
                content: message.body.clone(),
            });
        }
    }
}

/// Returns the upsert SQL used to persist loaded transcript rows without deleting unloaded history.
/// 返回用于持久化已加载转录行且不删除未加载历史的 upsert SQL。
pub(super) fn session_message_upsert_statement() -> &'static str {
    r#"
INSERT INTO session_messages (
    id,
    session_id,
    ordinal,
    role,
    kind,
    visible,
    turn_index,
    content,
    meta_json,
    created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
ON CONFLICT(id) DO UPDATE SET
    session_id = excluded.session_id,
    ordinal = excluded.ordinal,
    role = excluded.role,
    kind = excluded.kind,
    visible = excluded.visible,
    turn_index = excluded.turn_index,
    content = excluded.content,
    meta_json = excluded.meta_json,
    created_at = excluded.created_at;
"#
}

/// Returns the upsert SQL used to persist workspace-history document ownership rows.
/// 返回用于持久化工作台历史文档归属状态行的 upsert SQL。
fn workspace_history_search_state_upsert_statement() -> &'static str {
    r#"
INSERT INTO workspace_history_search_docs (
    message_id,
    session_id,
    project_id,
    event_id,
    turn_index,
    ordinal,
    role,
    created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
ON CONFLICT(message_id) DO UPDATE SET
    session_id = excluded.session_id,
    project_id = excluded.project_id,
    event_id = excluded.event_id,
    turn_index = excluded.turn_index,
    ordinal = excluded.ordinal,
    role = excluded.role,
    created_at = excluded.created_at;
"#
}

/// Returns the insert SQL used to persist controller-managed workspace-history FTS documents.
/// 返回用于持久化控制器托管工作台历史 FTS 文档的插入 SQL。
fn workspace_history_search_index_insert_statement(index_name: &str) -> String {
    format!(
        r#"
INSERT INTO {} (
    id,
    file_path,
    title,
    content
) VALUES (?1, ?2, ?3, ?4);
"#,
        quote_sqlite_identifier(index_name)
    )
}

/// Returns the delete SQL used to remove controller-managed workspace-history FTS documents by id.
/// 返回用于按文档 ID 删除控制器托管工作台历史 FTS 文档的 SQL。
fn workspace_history_search_index_delete_statement(index_name: &str) -> String {
    format!(
        "DELETE FROM {} WHERE id = ?1;",
        quote_sqlite_identifier(index_name)
    )
}

/// Assigns stable turn indexes to one ordered transcript slice.
/// 为一段有序转录消息分配稳定轮次索引。
#[cfg(test)]
pub(super) fn assign_session_message_turn_indexes(messages: &[SessionMessageEntry]) -> Vec<i64> {
    let mut turn_indexes = Vec::with_capacity(messages.len());
    let mut current_turn_index = 0_i64;
    let mut has_seen_user_turn = false;

    for message in messages {
        if session_message_starts_new_turn(message) {
            if has_seen_user_turn {
                current_turn_index = current_turn_index.saturating_add(1);
            } else {
                has_seen_user_turn = true;
            }
        }
        turn_indexes.push(current_turn_index);
    }

    turn_indexes
}

/// Returns whether one transcript message should open a new user-driven dialogue turn.
/// 返回一条转录消息是否应开启新的用户驱动对话轮次。
fn session_message_starts_new_turn(message: &SessionMessageEntry) -> bool {
    matches!(message.kind, SessionMessageKind::User)
        && message.visible
        && message.contributes_to_context()
}

/// Returns whether one visible transcript message should participate in workspace BM25 search indexing.
/// 返回某条可见转录消息是否应参与工作台 BM25 搜索索引。
pub(super) fn session_message_is_workspace_history_search_indexable(
    message: &SessionMessageEntry,
) -> bool {
    matches!(
        (message.kind, message.record_kind, message.visible),
        (
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            true
        ) | (
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            true,
        )
    )
}

/// Prepared persistence payload for one transcript message body.
/// 单条转录消息正文的预处理持久化载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PreparedSessionMessageBody {
    /// Inline content stored directly on the lightweight `session_messages` row.
    /// 直接存储在轻量 `session_messages` 行上的内联内容。
    pub(super) inline_content: String,
    /// External body chunks stored in `session_message_chunks` when the body is oversized.
    /// 正文过大时存储在 `session_message_chunks` 中的外部分片。
    pub(super) chunks: Vec<String>,
}

/// Prepares one message body for inline storage or chunked external storage.
/// 将单条消息正文预处理为内联存储或外部分片存储。
pub(super) fn prepare_session_message_body_for_storage(body: &str) -> PreparedSessionMessageBody {
    if body.len() <= SESSION_MESSAGE_INLINE_CONTENT_BUDGET_BYTES {
        return PreparedSessionMessageBody {
            inline_content: body.to_string(),
            chunks: Vec::new(),
        };
    }

    PreparedSessionMessageBody {
        inline_content: truncate_preview_lines(body, 4, SESSION_MESSAGE_CHUNK_PREVIEW_CHARS),
        chunks: split_text_into_byte_chunks(body, SESSION_MESSAGE_CHUNK_CONTENT_BYTES),
    }
}

/// Splits UTF-8 text into chunks that do not exceed the requested byte limit.
/// 将 UTF-8 文本拆分为不超过指定字节上限的分片。
fn split_text_into_byte_chunks(value: &str, max_bytes: usize) -> Vec<String> {
    let max_bytes = max_bytes.max(4);
    let mut chunks = Vec::new();
    let mut chunk_start = 0usize;

    for (char_start, ch) in value.char_indices() {
        let char_end = char_start + ch.len_utf8();
        if char_end.saturating_sub(chunk_start) > max_bytes && char_start > chunk_start {
            chunks.push(value[chunk_start..char_start].to_string());
            chunk_start = char_start;
        }
    }

    if chunk_start < value.len() {
        chunks.push(value[chunk_start..].to_string());
    }

    chunks
}

/// Pushes compaction-checkpoint batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的压缩检查点批量行推入共享 SQLite 批处理载荷。
fn push_session_compaction_batch_rows(
    batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    sessions: &[SessionSummary],
    session_compactions: &BTreeMap<SessionId, Vec<SessionCompactionEntry>>,
) {
    for session in sessions {
        if let Some(compactions) = session_compactions.get(&session.id) {
            for entry in compactions {
                batch_params.push(vec![
                    ControllerSqliteValue::String(entry.id.clone()),
                    ControllerSqliteValue::String(session.id.to_string()),
                    optional_sqlite_string(&entry.source_until_message_id),
                    optional_sqlite_i64(entry.source_until_turn_index),
                    optional_sqlite_string(&entry.context_start_message_id),
                    optional_sqlite_i64(entry.retained_tail_start_turn_index),
                    optional_sqlite_string(&entry.summary_message_id),
                    ControllerSqliteValue::String(non_empty_timestamp_text(&entry.created_at)),
                ]);
            }
        }
    }
}

/// Pushes runtime-state batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的运行时状态批量行推入共享 SQLite 批处理载荷。
pub(super) fn push_session_runtime_state_batch_rows(
    batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    sessions: &[SessionSummary],
    session_runtime_state: &BTreeMap<SessionId, SessionRuntimeStateEntry>,
) {
    for session in sessions {
        if let Some(entry) = session_runtime_state.get(&session.id) {
            batch_params.push(vec![
                ControllerSqliteValue::String(session.id.to_string()),
                optional_sqlite_string(&entry.active_agent_id),
                optional_sqlite_string(&entry.active_provider_id),
                optional_sqlite_string(&entry.active_model_id),
                optional_sqlite_string(&entry.last_compaction_id),
                optional_sqlite_string(&entry.context_start_message_id),
                optional_sqlite_string(&serialize_session_ui_state_to_json(&entry.ui_state)),
                optional_sqlite_string(&serialize_provider_usage_to_json(
                    entry
                        .last_successful_provider_usage
                        .as_ref()
                        .or(entry.last_provider_usage.as_ref()),
                )),
                ControllerSqliteValue::Null,
                optional_sqlite_string(&serialize_session_continuation_to_json(
                    &entry.conversation_state.continuation,
                )),
                optional_sqlite_string(&serialize_context_injection_state_to_json(
                    &entry.context_injection_state,
                )),
                optional_sqlite_string(&serialize_vmm_config_layer_to_json(&entry.vmm)),
                optional_sqlite_string(&serialize_vmm_writeback_outbox_to_json(
                    &entry.vmm_writeback_outbox,
                )),
                ControllerSqliteValue::String(non_empty_timestamp_text(&entry.updated_at)),
            ]);
        }
    }
}

/// Deletes all rows from one session-scoped table for the provided active session identifiers.
/// 删除某个会话作用域表中指定激活会话标识对应的全部行。
async fn delete_session_scoped_rows(
    controller: &ControllerRuntimeHandle,
    table_name: &str,
    session_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if session_ids.is_empty() {
        return Ok(());
    }

    let statement = format!("DELETE FROM {table_name} WHERE session_id = ?1;");
    let params = session_ids
        .iter()
        .cloned()
        .map(|session_id| vec![ControllerSqliteValue::String(session_id)])
        .collect::<Vec<_>>();

    execute_sqlite_batch_safely(controller, table_name, &statement, params, error_prefix).await?;

    Ok(())
}

/// Deletes chunk rows for the provided message identifiers before those messages are upserted.
/// 在对应消息被上写前，删除这些消息标识关联的正文分片行。
async fn delete_session_message_chunks_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if message_ids.is_empty() {
        return Ok(());
    }

    let params = message_ids
        .iter()
        .cloned()
        .map(|message_id| vec![ControllerSqliteValue::String(message_id)])
        .collect::<Vec<_>>();

    execute_sqlite_batch_safely(
        controller,
        "session_message_chunks",
        "DELETE FROM session_message_chunks WHERE message_id = ?1;",
        params,
        error_prefix,
    )
    .await?;

    Ok(())
}

/// Upserts controller-managed workspace-history FTS documents together with their ownership state rows.
/// 上写控制器托管工作台历史 FTS 文档，并同步更新对应的归属状态行。
async fn upsert_workspace_history_search_documents(
    controller: &ControllerRuntimeHandle,
    documents: Vec<WorkspaceHistorySearchDocumentUpsert>,
) -> Result<(), String> {
    if documents.is_empty() {
        return Ok(());
    }

    ensure_workspace_history_search_index(controller, WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME)
        .await?;
    let mut project_index_rows = BTreeMap::<String, Vec<Vec<ControllerSqliteValue>>>::new();
    let mut global_index_rows = Vec::with_capacity(documents.len());
    let mut state_rows = Vec::with_capacity(documents.len());
    let mut ensured_project_indexes = BTreeSet::new();

    for document in documents {
        let project_index_name = workspace_history_search_project_index_name(&document.project_id);
        if ensured_project_indexes.insert(project_index_name.clone()) {
            ensure_workspace_history_search_index(controller, &project_index_name).await?;
        }

        let file_path =
            workspace_history_search_document_file_path(&document.project_id, &document.session_id);
        let index_row = vec![
            ControllerSqliteValue::String(document.message_id.clone()),
            ControllerSqliteValue::String(file_path),
            ControllerSqliteValue::String(String::new()),
            ControllerSqliteValue::String(document.content.clone()),
        ];
        global_index_rows.push(index_row.clone());
        project_index_rows
            .entry(project_index_name)
            .or_default()
            .push(index_row);
        state_rows.push(vec![
            ControllerSqliteValue::String(document.message_id),
            ControllerSqliteValue::String(document.session_id),
            ControllerSqliteValue::String(document.project_id),
            ControllerSqliteValue::String(document.event_id),
            ControllerSqliteValue::Int64(document.turn_index),
            ControllerSqliteValue::Int64(document.ordinal),
            ControllerSqliteValue::String(document.role),
            ControllerSqliteValue::String(document.created_at),
        ]);
    }

    execute_sqlite_batch_safely(
        controller,
        WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        &workspace_history_search_index_insert_statement(
            WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        ),
        global_index_rows,
        "写入全局工作台历史搜索索引失败",
    )
    .await?;
    for (index_name, params) in project_index_rows {
        execute_sqlite_batch_safely(
            controller,
            &index_name,
            &workspace_history_search_index_insert_statement(&index_name),
            params,
            "写入项目工作台历史搜索索引失败",
        )
        .await?;
    }
    execute_sqlite_batch_safely(
        controller,
        "workspace_history_search_docs",
        workspace_history_search_state_upsert_statement(),
        state_rows,
        "写入工作台历史搜索归属状态失败",
    )
    .await?;

    Ok(())
}

/// Deletes controller-managed workspace-history FTS documents for the provided message identifiers.
/// 删除这些消息标识关联的控制器托管工作台历史 FTS 文档。
async fn delete_workspace_history_search_documents_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    let state_rows =
        query_workspace_history_search_document_state_for_message_ids(controller, message_ids)
            .await?;
    delete_workspace_history_search_documents_from_rows(controller, &state_rows, error_prefix)
        .await?;
    delete_workspace_history_search_document_state_for_message_ids(
        controller,
        message_ids,
        error_prefix,
    )
    .await
}

/// Deletes controller-managed workspace-history FTS documents for the provided session identifiers.
/// 删除这些会话标识关联的控制器托管工作台历史 FTS 文档。
async fn delete_workspace_history_search_documents_for_session_ids(
    controller: &ControllerRuntimeHandle,
    session_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    let state_rows =
        query_workspace_history_search_document_state_for_session_ids(controller, session_ids)
            .await?;
    delete_workspace_history_search_documents_from_rows(controller, &state_rows, error_prefix)
        .await?;
    delete_workspace_history_search_document_state_for_session_ids(
        controller,
        session_ids,
        error_prefix,
    )
    .await
}

/// Queries workspace-history document ownership rows for the provided message identifiers.
/// 查询给定消息标识对应的工作台历史文档归属状态行。
async fn query_workspace_history_search_document_state_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
) -> Result<Vec<StoredWorkspaceHistorySearchDocumentStateRow>, String> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut rows = Vec::new();
    for chunk in message_ids.chunks(SESSION_MESSAGE_POSITION_QUERY_ID_LIMIT) {
        let placeholders = (1..=chunk.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let statement = format!(
            "SELECT message_id, session_id, project_id FROM workspace_history_search_docs WHERE message_id IN ({placeholders});"
        );
        let result = controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(controller),
                statement,
                chunk
                    .iter()
                    .cloned()
                    .map(ControllerSqliteValue::String)
                    .collect(),
            )
            .await
            .map_err(|error| format!("查询工作台历史搜索文档归属失败：{error}"))?;
        rows.extend(decode_query_rows::<
            StoredWorkspaceHistorySearchDocumentStateRow,
        >(&result.json_data)?);
    }

    Ok(rows)
}

/// Queries workspace-history document ownership rows for the provided session identifiers.
/// 查询给定会话标识对应的工作台历史文档归属状态行。
async fn query_workspace_history_search_document_state_for_session_ids(
    controller: &ControllerRuntimeHandle,
    session_ids: &[String],
) -> Result<Vec<StoredWorkspaceHistorySearchDocumentStateRow>, String> {
    if session_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut rows = Vec::new();
    for chunk in session_ids.chunks(SESSION_MESSAGE_POSITION_QUERY_ID_LIMIT) {
        let placeholders = (1..=chunk.len())
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let statement = format!(
            "SELECT message_id, session_id, project_id FROM workspace_history_search_docs WHERE session_id IN ({placeholders});"
        );
        let result = controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(controller),
                statement,
                chunk
                    .iter()
                    .cloned()
                    .map(ControllerSqliteValue::String)
                    .collect(),
            )
            .await
            .map_err(|error| format!("查询工作台历史搜索会话归属失败：{error}"))?;
        rows.extend(decode_query_rows::<
            StoredWorkspaceHistorySearchDocumentStateRow,
        >(&result.json_data)?);
    }

    Ok(rows)
}

/// Deletes controller-managed workspace-history FTS documents using preloaded ownership rows.
/// 基于预加载归属状态行删除控制器托管工作台历史 FTS 文档。
async fn delete_workspace_history_search_documents_from_rows(
    controller: &ControllerRuntimeHandle,
    state_rows: &[StoredWorkspaceHistorySearchDocumentStateRow],
    error_prefix: &str,
) -> Result<(), String> {
    if state_rows.is_empty() {
        return Ok(());
    }

    let global_params = state_rows
        .iter()
        .map(|row| vec![ControllerSqliteValue::String(row.message_id.clone())])
        .collect::<Vec<_>>();
    execute_sqlite_batch_safely(
        controller,
        WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        &workspace_history_search_index_delete_statement(
            WORKSPACE_HISTORY_SEARCH_GLOBAL_INDEX_NAME,
        ),
        global_params,
        error_prefix,
    )
    .await?;

    let mut project_params = BTreeMap::<String, Vec<Vec<ControllerSqliteValue>>>::new();
    for row in state_rows {
        project_params
            .entry(workspace_history_search_project_index_name(&row.project_id))
            .or_default()
            .push(vec![ControllerSqliteValue::String(row.message_id.clone())]);
    }
    for (index_name, params) in project_params {
        execute_sqlite_batch_safely(
            controller,
            &index_name,
            &workspace_history_search_index_delete_statement(&index_name),
            params,
            error_prefix,
        )
        .await?;
    }

    Ok(())
}

/// Deletes workspace-history document ownership rows for the provided message identifiers.
/// 删除给定消息标识对应的工作台历史文档归属状态行。
async fn delete_workspace_history_search_document_state_for_message_ids(
    controller: &ControllerRuntimeHandle,
    message_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if message_ids.is_empty() {
        return Ok(());
    }

    let params = message_ids
        .iter()
        .cloned()
        .map(|message_id| vec![ControllerSqliteValue::String(message_id)])
        .collect::<Vec<_>>();
    execute_sqlite_batch_safely(
        controller,
        "workspace_history_search_docs",
        "DELETE FROM workspace_history_search_docs WHERE message_id = ?1;",
        params,
        error_prefix,
    )
    .await
}

/// Deletes workspace-history document ownership rows for the provided session identifiers.
/// 删除给定会话标识对应的工作台历史文档归属状态行。
async fn delete_workspace_history_search_document_state_for_session_ids(
    controller: &ControllerRuntimeHandle,
    session_ids: &[String],
    error_prefix: &str,
) -> Result<(), String> {
    if session_ids.is_empty() {
        return Ok(());
    }

    let params = session_ids
        .iter()
        .cloned()
        .map(|session_id| vec![ControllerSqliteValue::String(session_id)])
        .collect::<Vec<_>>();
    execute_sqlite_batch_safely(
        controller,
        "workspace_history_search_docs",
        "DELETE FROM workspace_history_search_docs WHERE session_id = ?1;",
        params,
        error_prefix,
    )
    .await
}

/// Executes a typed SQLite batch in safe chunks estimated against the full gRPC protobuf frame.
/// 按完整 gRPC protobuf 帧估算将类型化 SQLite 批处理拆成安全小批执行。
async fn execute_sqlite_batch_safely(
    controller: &ControllerRuntimeHandle,
    table_name: &str,
    statement: &str,
    params: Vec<Vec<ControllerSqliteValue>>,
    error_prefix: &str,
) -> Result<(), String> {
    if params.is_empty() {
        return Ok(());
    }

    let statement_bytes = estimate_text_frame_bytes(statement);
    let mut batch_index = 0usize;
    let mut current_rows: Vec<Vec<ControllerSqliteValue>> = Vec::new();
    let mut current_estimated_bytes =
        SQLITE_BATCH_FRAME_OVERHEAD_BYTES.saturating_add(statement_bytes);

    for row in params {
        let row_estimated_bytes = estimate_sqlite_row_frame_bytes(&row);
        let row_would_overflow = !current_rows.is_empty()
            && (current_rows.len() >= SQLITE_BATCH_SAFE_ROW_LIMIT
                || current_estimated_bytes.saturating_add(row_estimated_bytes)
                    > SQLITE_BATCH_SAFE_FRAME_TARGET_BYTES);

        if row_would_overflow {
            batch_index = batch_index.saturating_add(1);
            execute_sqlite_batch_chunk(
                controller,
                table_name,
                statement,
                std::mem::take(&mut current_rows),
                batch_index,
                current_estimated_bytes,
                error_prefix,
            )
            .await?;
            current_estimated_bytes =
                SQLITE_BATCH_FRAME_OVERHEAD_BYTES.saturating_add(statement_bytes);
        }

        current_estimated_bytes = current_estimated_bytes.saturating_add(row_estimated_bytes);
        current_rows.push(row);
    }

    if !current_rows.is_empty() {
        batch_index = batch_index.saturating_add(1);
        execute_sqlite_batch_chunk(
            controller,
            table_name,
            statement,
            current_rows,
            batch_index,
            current_estimated_bytes,
            error_prefix,
        )
        .await?;
    }

    Ok(())
}

/// Executes one already-sized SQLite batch chunk and reports frame-budget context on failure.
/// 执行一段已经切好大小的 SQLite 批处理，并在失败时报告帧预算上下文。
async fn execute_sqlite_batch_chunk(
    controller: &ControllerRuntimeHandle,
    table_name: &str,
    statement: &str,
    params: Vec<Vec<ControllerSqliteValue>>,
    batch_index: usize,
    estimated_bytes: usize,
    error_prefix: &str,
) -> Result<(), String> {
    let row_count = params.len();
    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| {
            format!(
                "{error_prefix}：表 {table_name} 第 {batch_index} 批失败，行数 {row_count}，估算完整帧 {estimated_bytes} bytes：{error}"
            )
        })?;

    Ok(())
}

/// Estimates one SQLite row's contribution to the encoded request frame.
/// 估算一行 SQLite 参数对编码后请求帧的贡献。
fn estimate_sqlite_row_frame_bytes(row: &[ControllerSqliteValue]) -> usize {
    SQLITE_ROW_FRAME_OVERHEAD_BYTES
        + row
            .iter()
            .map(estimate_sqlite_value_frame_bytes)
            .sum::<usize>()
}

/// Estimates one SQLite scalar value's encoded request-frame contribution.
/// 估算一个 SQLite 标量值对编码后请求帧的贡献。
fn estimate_sqlite_value_frame_bytes(value: &ControllerSqliteValue) -> usize {
    SQLITE_VALUE_FRAME_OVERHEAD_BYTES
        + match value {
            ControllerSqliteValue::Int64(_) => 16,
            ControllerSqliteValue::Float64(_) => 16,
            ControllerSqliteValue::String(value) => estimate_text_frame_bytes(value),
            ControllerSqliteValue::Bytes(value) => value.len(),
            ControllerSqliteValue::Bool(_) => 8,
            ControllerSqliteValue::Null => 4,
        }
}

/// Estimates a text field after JSON/protobuf structural overhead and escaping slack are considered.
/// 在考虑 JSON/protobuf 结构开销与转义余量后估算文本字段大小。
fn estimate_text_frame_bytes(value: &str) -> usize {
    value.len().saturating_mul(2).saturating_add(32)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one compact session summary used by persistence helper tests.
    /// 为持久化辅助测试构建一条紧凑会话摘要。
    fn test_session_summary(id: SessionId, project_id: &str, title: &str) -> SessionSummary {
        SessionSummary::new(
            id,
            Some(project_id.to_string()),
            title.to_string(),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        )
    }

    /// Verifies event-store rewrites only target sessions that actually carry payload snapshots in the current persistence flush.
    /// 验证事件存储重写只会命中当前持久化刷新中确实携带载荷快照的会话。
    #[test]
    fn collect_session_event_store_rewrite_ids_skips_unloaded_sessions() {
        let session_a = test_session_summary(SessionId::from_u128(101), "project-a", "会话 A");
        let session_b = test_session_summary(SessionId::from_u128(202), "project-a", "会话 B");
        let all_sessions = vec![session_a.clone(), session_b.clone()];

        let mut session_runtime_state = BTreeMap::new();
        session_runtime_state.insert(
            session_a.id,
            SessionRuntimeStateEntry {
                updated_at: String::from("1"),
                ..SessionRuntimeStateEntry::default()
            },
        );

        let rewrite_ids = collect_session_event_store_rewrite_ids(
            &all_sessions,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &session_runtime_state,
            &BTreeMap::new(),
            &BTreeMap::new(),
            &BTreeMap::new(),
        );

        assert!(rewrite_ids.contains(&session_a.id));
        assert!(!rewrite_ids.contains(&session_b.id));
    }
}
