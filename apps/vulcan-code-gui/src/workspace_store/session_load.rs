//! Session-loading helpers extracted from the main workspace-store module.
//! 从主工作区存储模块中抽取的会话加载辅助逻辑。

#![allow(dead_code)]

use super::event_store::PersistedSessionEventRow;
use super::*;

/// Lightweight persisted session locator row used to resolve the file-backed event store.
/// 用于解析文件化事件存储的轻量持久化会话定位行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionEventStoreLocatorRow {
    /// Stable owning project identifier.
    /// 稳定所属项目标识。
    project_id: String,
    /// Latest persisted canonical turn index mirrored from the event store.
    /// 从事件存储镜像回数据库的最近规范轮次索引。
    latest_turn_index: i64,
}

/// Lightweight persisted session turn row used for transcript paging.
/// 用于转录分页的轻量持久化会话轮次行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionTurnRow {
    /// Stable turn identifier.
    /// 稳定轮次标识。
    turn_id: String,
    /// Stable turn index.
    /// 稳定轮次索引。
    turn_index: i64,
    /// First event sequence belonging to the turn.
    /// 归属于该轮次的首个事件序号。
    start_seq: i64,
    /// Last event sequence belonging to the turn.
    /// 归属于该轮次的最后一个事件序号。
    end_seq: i64,
    /// Stable terminal state label mirrored from the event store.
    /// 从事件存储镜像而来的稳定终态标签。
    turn_state: String,
    /// Optional summary message identifier that closed the turn.
    /// 闭合该轮次的可选摘要消息标识。
    summary_message_id: Option<String>,
}

/// Lazily loads the recent transcript window and lightweight runtime state for one session.
/// 为单个会话懒加载最近转录窗口与轻量运行时状态。
pub async fn load_session_hydration_snapshot(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    requested_limit: usize,
) -> Result<SessionHydrationSnapshot, String> {
    let requested_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let locator = query_session_event_store_locator(&controller, session_id).await?;
    let compactions = locator
        .as_ref()
        .map(|row| load_session_event_store_compactions(row.project_id.as_str(), session_id))
        .transpose()?
        .unwrap_or_default();
    let runtime_state_row = query_session_runtime_state_row(&controller, session_id).await?;
    let target_turn_count = resolve_initial_hydration_turn_count(
        requested_limit,
        &compactions,
        runtime_state_row.as_ref(),
        locator.as_ref(),
    );
    let turn_rows =
        query_session_turn_rows(&controller, session_id, None, target_turn_count).await?;
    let has_more_history = turn_rows.first().is_some_and(|row| row.turn_index > 0);
    let (messages, tool_detail_messages, earliest_ordinal, full_messages) =
        load_event_store_window_messages(&locator, session_id, &turn_rows)?;
    let runtime_state = hydrate_session_runtime_state(
        locator.as_ref(),
        session_id,
        runtime_state_row,
        &full_messages,
    )
    .await?;

    Ok(SessionHydrationSnapshot {
        session_id,
        messages,
        tool_detail_messages,
        compactions,
        runtime_state,
        earliest_ordinal,
        has_more_history,
    })
}

/// Lazily loads one older transcript page before the provided ordinal cursor.
/// 在给定序号游标之前懒加载一页更早的转录历史。
pub async fn load_session_history_page(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    before_ordinal: i64,
    requested_limit: usize,
) -> Result<SessionHistoryPage, String> {
    let requested_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let locator = query_session_event_store_locator(&controller, session_id).await?;
    let turn_rows = query_session_turn_rows_before_seq(
        &controller,
        session_id,
        before_ordinal,
        requested_limit,
    )
    .await?;
    let has_more_history = turn_rows.first().is_some_and(|row| row.turn_index > 0);
    let (messages, tool_detail_messages, earliest_ordinal, _) =
        load_event_store_window_messages(&locator, session_id, &turn_rows)?;

    Ok(SessionHistoryPage {
        session_id,
        messages,
        tool_detail_messages,
        earliest_ordinal,
        has_more_history,
    })
}

/// Lazily loads one tool-detail page for a specific conversation turn.
/// 为指定对话轮次懒加载一页工具明细。
pub async fn load_session_tool_detail_page(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
    before_ordinal: Option<i64>,
    requested_limit: usize,
) -> Result<SessionToolDetailPage, String> {
    let requested_limit = requested_limit.max(1).min(SESSION_TOOL_DETAIL_PAGE_LIMIT);
    let locator = query_session_event_store_locator(&controller, session_id).await?;
    let Some(locator) = locator else {
        return Ok(SessionToolDetailPage {
            messages: Vec::new(),
            earliest_ordinal: None,
            has_more_detail: false,
        });
    };
    let load_result = load_session_event_store(locator.project_id.as_str(), session_id)?;
    let mut tool_messages = load_result
        .rows
        .into_iter()
        .filter(|row| row.turn_index == turn_index)
        .filter(|row| {
            matches!(
                row.record_kind,
                SessionMessageRecordKind::ToolCallOutput
                    | SessionMessageRecordKind::ToolResultOutput
            )
        })
        .collect::<Vec<_>>();
    tool_messages.sort_by_key(|row| row.seq);
    if let Some(before_ordinal) = before_ordinal {
        tool_messages.retain(|row| row.seq < before_ordinal);
    }
    let total_count = tool_messages.len();
    let page_rows = if total_count > requested_limit {
        tool_messages
            .into_iter()
            .skip(total_count.saturating_sub(requested_limit))
            .collect::<Vec<_>>()
    } else {
        tool_messages
    };
    let earliest_ordinal = page_rows.first().map(|row| row.seq);
    let has_more_detail = total_count > requested_limit && earliest_ordinal.is_some();

    Ok(SessionToolDetailPage {
        messages: page_rows
            .into_iter()
            .map(persisted_event_row_to_session_message_entry)
            .collect(),
        earliest_ordinal,
        has_more_detail,
    })
}

/// Loads the complete persisted tool-detail stream for one conversation turn.
/// 加载单个对话轮次的完整持久化工具明细流。
pub async fn load_complete_session_tool_detail_turn(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
) -> Result<SessionToolDetailPage, String> {
    let locator = query_session_event_store_locator(&controller, session_id).await?;
    let Some(locator) = locator else {
        return Ok(SessionToolDetailPage {
            messages: Vec::new(),
            earliest_ordinal: None,
            has_more_detail: false,
        });
    };
    let load_result = load_session_event_store(locator.project_id.as_str(), session_id)?;
    let mut rows = load_result
        .rows
        .into_iter()
        .filter(|row| row.turn_index == turn_index)
        .filter(|row| {
            matches!(
                row.record_kind,
                SessionMessageRecordKind::ToolCallOutput
                    | SessionMessageRecordKind::ToolResultOutput
            )
        })
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.seq);
    let earliest_ordinal = rows.first().map(|row| row.seq);

    Ok(SessionToolDetailPage {
        messages: rows
            .into_iter()
            .map(persisted_event_row_to_session_message_entry)
            .collect(),
        earliest_ordinal,
        has_more_detail: false,
    })
}

/// Queries the lightweight session locator needed to open one file-backed event store.
/// 查询打开单个文件化事件存储所需的轻量会话定位信息。
async fn query_session_event_store_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<StoredSessionEventStoreLocatorRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT project_id, latest_turn_index FROM sessions WHERE id = ?1 LIMIT 1;",
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话事件存储定位信息失败：{error}"))?;

    Ok(
        decode_query_rows::<StoredSessionEventStoreLocatorRow>(&result.json_data)?
            .into_iter()
            .next(),
    )
}

/// Queries one session-scoped runtime-state row persisted in SQLite.
/// 查询持久化在 SQLite 中的单条会话级运行时状态行。
async fn query_session_runtime_state_row(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<StoredRuntimeStateRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT r.session_id, r.active_agent_id, r.active_provider_id, r.active_model_id, r.last_compaction_id, r.context_start_message_id, r.ui_state_json, r.provider_usage_json, r.conversation_state_json, r.continuation_json, r.updated_at
FROM session_runtime_state AS r
WHERE r.session_id = ?1
LIMIT 1;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话完整运行时状态失败：{error}"))?;

    Ok(
        decode_query_rows::<StoredRuntimeStateRow>(&result.json_data)?
            .into_iter()
            .next(),
    )
}

/// Queries the most recent session turn rows ordered by newest turn first.
/// 查询最近的会话 turn 行，并按最新轮次优先排序。
async fn query_session_turn_rows(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    before_turn_index: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredSessionTurnRow>, String> {
    let limit = limit.max(1).min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT) as i64;
    let (statement, params) = if let Some(before_turn_index) = before_turn_index {
        (
            r#"
SELECT turn_id, turn_index, start_seq, end_seq, turn_state, summary_message_id
FROM session_turn_index
WHERE session_id = ?1 AND turn_index < ?2
ORDER BY turn_index DESC
LIMIT ?3;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(before_turn_index),
                ControllerSqliteValue::Int64(limit),
            ],
        )
    } else {
        (
            r#"
SELECT turn_id, turn_index, start_seq, end_seq, turn_state, summary_message_id
FROM session_turn_index
WHERE session_id = ?1
ORDER BY turn_index DESC
LIMIT ?2;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(limit),
            ],
        )
    };
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            statement,
            params,
        )
        .await
        .map_err(|error| format!("查询会话 turn 索引失败：{error}"))?;

    decode_query_rows::<StoredSessionTurnRow>(&result.json_data)
}

/// Queries the most recent session turn rows that end strictly before the provided event sequence cursor.
/// 查询在给定事件序号游标之前严格结束的最近会话 turn 行。
async fn query_session_turn_rows_before_seq(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    before_seq: i64,
    limit: usize,
) -> Result<Vec<StoredSessionTurnRow>, String> {
    let limit = limit.max(1).min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT) as i64;
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT turn_id, turn_index, start_seq, end_seq, turn_state, summary_message_id
FROM session_turn_index
WHERE session_id = ?1 AND end_seq < ?2
ORDER BY turn_index DESC
LIMIT ?3;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(before_seq),
                ControllerSqliteValue::Int64(limit),
            ],
        )
        .await
        .map_err(|error| format!("查询更早会话 turn 索引失败：{error}"))?;

    decode_query_rows::<StoredSessionTurnRow>(&result.json_data)
}

/// Loads the selected transcript/tool windows from the file-backed event store according to the requested turn rows.
/// 按请求的 turn 行从文件化事件存储加载对应的转录与工具窗口。
fn load_event_store_window_messages(
    locator: &Option<StoredSessionEventStoreLocatorRow>,
    session_id: SessionId,
    turn_rows: &[StoredSessionTurnRow],
) -> Result<
    (
        Vec<SessionMessageEntry>,
        Vec<SessionMessageEntry>,
        Option<i64>,
        Vec<SessionMessageEntry>,
    ),
    String,
> {
    let Some(locator) = locator else {
        return Ok((Vec::new(), Vec::new(), None, Vec::new()));
    };
    let load_result = load_session_event_store(locator.project_id.as_str(), session_id)?;
    let mut selected_turn_ids = turn_rows
        .iter()
        .map(|row| row.turn_index)
        .collect::<BTreeSet<_>>();
    let earliest_ordinal = turn_rows.iter().map(|row| row.start_seq).min();
    let mut visible_rows = Vec::new();
    let mut tool_detail_rows = Vec::new();
    let mut full_rows = Vec::new();

    for row in load_result.rows {
        let in_selected_turn = selected_turn_ids.contains(&row.turn_index);
        full_rows.push(row.clone());
        if !in_selected_turn {
            continue;
        }
        if matches!(
            row.record_kind,
            SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
        ) {
            visible_rows.push(row.clone());
            tool_detail_rows.push(row);
        } else {
            visible_rows.push(row);
        }
    }

    visible_rows.sort_by_key(|row| row.seq);
    tool_detail_rows.sort_by_key(|row| row.seq);
    full_rows.sort_by_key(|row| row.seq);
    selected_turn_ids.clear();

    Ok((
        visible_rows
            .into_iter()
            .map(persisted_event_row_to_session_message_entry)
            .collect(),
        tool_detail_rows
            .into_iter()
            .map(persisted_event_row_to_session_message_entry)
            .collect(),
        earliest_ordinal,
        full_rows
            .into_iter()
            .map(persisted_event_row_to_session_message_entry)
            .collect(),
    ))
}

/// Restores the hydrated runtime state for one session from the small runtime-state cache plus the canonical event stream.
/// 通过小型运行时状态缓存和规范事件流为单个会话恢复水合后的运行时状态。
async fn hydrate_session_runtime_state(
    locator: Option<&StoredSessionEventStoreLocatorRow>,
    session_id: SessionId,
    runtime_state_row: Option<StoredRuntimeStateRow>,
    full_messages: &[SessionMessageEntry],
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let Some(locator) = locator else {
        return Ok(runtime_state_row.map(stored_runtime_state_row_to_entry));
    };
    let runtime_state_cache =
        load_session_event_store_runtime_state(locator.project_id.as_str(), session_id)
            .unwrap_or_default();
    let current_context_document = match load_session_event_store_context_current(
        locator.project_id.as_str(),
        session_id,
    ) {
        Ok(document) => document,
        Err(_) => load_latest_session_event_store_context_snapshot(
            locator.project_id.as_str(),
            session_id,
        )?
        .map(|snapshot| snapshot.context),
    };
    let mut runtime_state = runtime_state_row
        .map(stored_runtime_state_row_to_entry)
        .unwrap_or_default();
    runtime_state.active_agent_id = runtime_state_cache
        .active_agent_id
        .or(runtime_state.active_agent_id);
    runtime_state.active_provider_id = runtime_state_cache
        .active_provider_id
        .or(runtime_state.active_provider_id);
    runtime_state.active_model_id = runtime_state_cache
        .active_model_id
        .or(runtime_state.active_model_id);
    runtime_state.last_compaction_id = runtime_state_cache
        .last_compaction_id
        .or(runtime_state.last_compaction_id);
    runtime_state.context_start_message_id = runtime_state_cache
        .context_start_message_id
        .or(runtime_state.context_start_message_id);
    runtime_state.ui_state = runtime_state_cache.ui_state;
    runtime_state.last_provider_usage = runtime_state_cache
        .last_provider_usage
        .or(runtime_state.last_provider_usage);
    runtime_state.updated_at = if runtime_state_cache.updated_at.trim().is_empty() {
        current_unix_millis_text()
    } else {
        runtime_state_cache.updated_at
    };
    runtime_state.conversation_state = current_context_document
        .as_ref()
        .map(|document| document.conversation_state.clone())
        .unwrap_or_else(|| conversation_state_from_session_messages(full_messages));
    runtime_state.last_compaction_id = current_context_document
        .as_ref()
        .and_then(|document| document.last_compaction_id.clone())
        .or(runtime_state.last_compaction_id);
    runtime_state.context_start_message_id = current_context_document
        .as_ref()
        .and_then(|document| document.context_start_message_id.clone())
        .or(runtime_state.context_start_message_id);
    runtime_state.conversation_state.continuation =
        if runtime_state_cache.continuation.provider_id.is_some()
            || runtime_state_cache.continuation.model_id.is_some()
            || runtime_state_cache
                .continuation
                .last_provider_message_id
                .is_some()
            || runtime_state_cache.continuation.updated_at.is_some()
        {
            runtime_state_cache.continuation
        } else {
            runtime_state.conversation_state.continuation
        };

    Ok(Some(runtime_state))
}

/// Resolves the startup hydration turn target by taking the larger value between the configured page size and the overlap required to expose five turns before the active compaction boundary.
/// 通过取“配置分页大小”和“为暴露活动压缩边界前五轮所需的重叠范围”中的较大值，解析启动水合轮次目标。
fn resolve_initial_hydration_turn_count(
    requested_turn_count: usize,
    compactions: &[SessionCompactionEntry],
    runtime_state_row: Option<&StoredRuntimeStateRow>,
    locator: Option<&StoredSessionEventStoreLocatorRow>,
) -> usize {
    let mut effective_turn_count = requested_turn_count.max(1);
    let Some(boundary_turn_index) =
        active_compaction_overlap_boundary_turn_index(compactions, runtime_state_row)
    else {
        return effective_turn_count;
    };
    let Some(latest_turn_index) = locator
        .map(|row| row.latest_turn_index)
        .filter(|value| *value >= 0)
    else {
        return effective_turn_count;
    };
    let overlap_turn_count = SESSION_TRANSCRIPT_COMPACTION_OVERLAP_TURN_COUNT as i64;
    let earliest_required_turn_index = boundary_turn_index
        .saturating_sub(overlap_turn_count)
        .max(0);
    let turns_needed = latest_turn_index
        .saturating_sub(earliest_required_turn_index)
        .saturating_add(1) as usize;
    effective_turn_count = effective_turn_count.max(turns_needed);
    effective_turn_count
}

/// Resolves the active compaction overlap boundary from persisted runtime-state metadata and checkpoint rows.
/// 根据持久化运行时状态元数据与检查点行解析当前活动压缩重叠边界。
fn active_compaction_overlap_boundary_turn_index(
    compactions: &[SessionCompactionEntry],
    runtime_state_row: Option<&StoredRuntimeStateRow>,
) -> Option<i64> {
    let active_compaction = runtime_state_row
        .and_then(|row| row.last_compaction_id.as_deref())
        .and_then(|compaction_id| compactions.iter().find(|entry| entry.id == compaction_id))
        .or_else(|| compactions.last())?;

    active_compaction
        .retained_tail_start_turn_index
        .or_else(|| {
            active_compaction
                .source_until_turn_index
                .map(|turn_index| turn_index + 1)
        })
}

/// Converts one persisted event row into the in-memory transcript entry shape.
/// 将单条持久化事件行转换为内存中的转录消息结构。
fn persisted_event_row_to_session_message_entry(
    row: PersistedSessionEventRow,
) -> SessionMessageEntry {
    SessionMessageEntry {
        id: row.event_id,
        kind: row.kind,
        record_kind: row.record_kind,
        visible: row.visible,
        title: row.title,
        body: row.body,
        metadata: row.metadata,
        created_at: row.created_at,
    }
}

#[cfg(test)]
mod tests {
    //! Session-load tests covering transcript hydration behavior for event-store backed sessions.
    //! 面向事件流会话的转录水合行为测试。

    use super::*;
    use crate::conversation_protocol::append_assistant_text_message;

    /// Builds one transcript message entry for session-load regression tests.
    /// 为会话加载回归测试构建一条转录消息项。
    fn test_message(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
    ) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_string(),
            kind,
            record_kind,
            visible: true,
            title: id.to_string(),
            body: body.to_string(),
            metadata: None,
            created_at: String::from("1"),
        }
    }

    /// Verifies hydration keeps tool directives visible in the main transcript while also preserving full tool details out of band.
    /// 验证水合时会在主转录中保留工具指令可见卡片，并同时在带外缓存中保留完整工具详情。
    #[test]
    fn load_event_store_window_messages_keeps_tool_rows_visible() {
        let project_id = "session-load-project";
        let session_id = SessionId::from_u128(42);
        remove_session_event_store(project_id, session_id)
            .expect("cleanup test session event-store");

        let session = SessionSummary::new(
            session_id,
            Some(String::from(project_id)),
            String::from("测试会话"),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        );
        let messages = vec![
            test_message(
                "u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "读取文件",
            ),
            test_message(
                "call-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolCallOutput,
                "调用 read_file",
            ),
            test_message(
                "result-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
                "文件内容",
            ),
        ];
        persist_session_event_store(&session, &messages, None, &[])
            .expect("persist session event store");

        let locator = Some(StoredSessionEventStoreLocatorRow {
            project_id: String::from(project_id),
            latest_turn_index: 0,
        });
        let turn_rows = vec![StoredSessionTurnRow {
            turn_id: String::from("turn-000000"),
            turn_index: 0,
            start_seq: 0,
            end_seq: 2,
            turn_state: String::from("open"),
            summary_message_id: None,
        }];

        let (messages, tool_detail_messages, earliest_ordinal, _) =
            load_event_store_window_messages(&locator, session_id, &turn_rows)
                .expect("load event-store window");

        assert_eq!(earliest_ordinal, Some(0));
        assert_eq!(messages.len(), 3);
        assert!(messages.iter().any(|message| message.id == "call-1"));
        assert!(messages.iter().any(|message| message.id == "result-1"));
        assert_eq!(tool_detail_messages.len(), 2);
        assert!(tool_detail_messages.iter().all(|message| matches!(
            message.record_kind,
            SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
        )));

        remove_session_event_store(project_id, session_id).expect("cleanup persisted test session");
    }

    /// Verifies that hydration prefers the persisted current-context document instead of rebuilding the active conversation only from transcript rows.
    /// 验证水合时会优先使用持久化当前上下文文档，而不是只根据转录行重建活动会话状态。
    #[test]
    fn hydrate_session_runtime_state_prefers_current_context_document() {
        let project_id = "hydrate-context-project";
        let session_id = SessionId::from_u128(99);
        remove_session_event_store(project_id, session_id).expect("cleanup hydrate store");

        let session = SessionSummary::new(
            session_id,
            Some(String::from(project_id)),
            String::from("水合上下文测试"),
            String::new(),
            SessionState::Draft,
            0,
            0,
            String::from("刚刚"),
        );
        let transcript_messages = vec![test_message(
            "user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "仅转录消息",
        )];
        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_assistant_text_message(
            &mut runtime_state.conversation_state,
            String::from("assistant-ctx"),
            String::from("当前上下文正文"),
        );
        runtime_state.context_start_message_id = Some(String::from("summary-ctx"));

        persist_session_event_store(&session, &transcript_messages, Some(&runtime_state), &[])
            .expect("persist hydrate context store");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread tokio runtime");
        let hydrated = runtime
            .block_on(hydrate_session_runtime_state(
                Some(&StoredSessionEventStoreLocatorRow {
                    project_id: String::from(project_id),
                    latest_turn_index: 0,
                }),
                session_id,
                None,
                &transcript_messages,
            ))
        .expect("hydrate runtime state")
        .expect("expected hydrated runtime state");

        assert_eq!(hydrated.conversation_state.messages.len(), 1);
        assert_eq!(
            hydrated.context_start_message_id.as_deref(),
            Some("summary-ctx")
        );
        remove_session_event_store(project_id, session_id).expect("cleanup hydrate event-store");
    }

    /// Verifies that hydration falls back to the latest snapshot when the current-context document is unreadable.
    /// 验证当当前上下文文档不可读时，水合过程会回退到最近的上下文快照。
    #[test]
    fn hydrate_session_runtime_state_falls_back_to_latest_snapshot() {
        let project_id = "hydrate-snapshot-fallback";
        let session_id = SessionId::from_u128(119);
        remove_session_event_store(project_id, session_id).expect("cleanup fallback store");

        let mut runtime_state = SessionRuntimeStateEntry::default();
        append_assistant_text_message(
            &mut runtime_state.conversation_state,
            String::from("assistant-snapshot"),
            String::from("快照上下文正文"),
        );
        runtime_state.updated_at = String::from("777");
        snapshot_session_context_before_user_prompt(project_id, session_id, &runtime_state)
            .expect("capture fallback snapshot");
        let current_path = crate::data_plane::application_data_root()
            .join("sessions")
            .join(project_id)
            .join(session_id.to_string())
            .join("context")
            .join("current.json");
        std::fs::write(&current_path, "{invalid-json")
            .expect("write invalid current context document");

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread tokio runtime");
        let hydrated = runtime
            .block_on(hydrate_session_runtime_state(
                Some(&StoredSessionEventStoreLocatorRow {
                    project_id: String::from(project_id),
                    latest_turn_index: 0,
                }),
                session_id,
                None,
                &[],
            ))
            .expect("hydrate runtime state from snapshot")
            .expect("expected hydrated runtime state from snapshot");

        assert_eq!(hydrated.conversation_state.messages.len(), 1);
        remove_session_event_store(project_id, session_id).expect("cleanup fallback event-store");
    }
}
