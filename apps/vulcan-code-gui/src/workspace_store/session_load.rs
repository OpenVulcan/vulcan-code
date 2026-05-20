//! Session-loading helpers extracted from the main workspace-store module.
//! 从主工作区存储模块中抽取的会话加载辅助逻辑。

use super::*;

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
    let compactions = query_session_compaction_rows_for_session(&controller, session_id)
        .await?
        .into_iter()
        .map(stored_compaction_row_to_entry)
        .collect::<Vec<_>>();
    let runtime_state_row = query_session_runtime_state_row(&controller, session_id).await?;
    let target_turn_count = resolve_initial_hydration_turn_count(
        &controller,
        session_id,
        requested_limit,
        &compactions,
        runtime_state_row.as_ref(),
    )
    .await?;
    let (message_rows, has_more_history) = query_session_message_window_rows_until_turn_target(
        &controller,
        session_id,
        target_turn_count,
        requested_limit,
    )
    .await?;
    let mut message_rows = message_rows;
    message_rows.sort_by_key(|row| row.ordinal);
    let tool_detail_turn_indices =
        query_tool_detail_turn_indices_for_turns(&controller, session_id, &message_rows).await?;
    let tool_detail_messages = load_complete_tool_detail_messages_for_turns(
        &controller,
        session_id,
        &tool_detail_turn_indices,
    )
    .await?;
    let generated_media_rows =
        query_generated_media_tool_result_rows_for_turns(&controller, session_id, &message_rows)
            .await?;
    let earliest_ordinal =
        merge_generated_media_rows_into_transcript_window(&mut message_rows, generated_media_rows);
    let messages =
        hydrate_transcript_window_messages(&controller, session_id, message_rows).await?;
    let runtime_state = hydrate_session_runtime_state(
        &controller,
        session_id,
        runtime_state_row,
        &messages,
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
    let (message_rows, effective_limit) = query_session_message_window_rows_with_retry(
        &controller,
        session_id,
        Some(before_ordinal),
        requested_limit,
    )
    .await?;
    let mut message_rows = message_rows;
    message_rows.sort_by_key(|row| row.ordinal);
    let loaded_turn_count = loaded_message_turn_count(&message_rows);
    let tool_detail_turn_indices =
        query_tool_detail_turn_indices_for_turns(&controller, session_id, &message_rows).await?;
    let tool_detail_messages = load_complete_tool_detail_messages_for_turns(
        &controller,
        session_id,
        &tool_detail_turn_indices,
    )
    .await?;
    let generated_media_rows =
        query_generated_media_tool_result_rows_for_turns(&controller, session_id, &message_rows)
            .await?;
    let earliest_ordinal =
        merge_generated_media_rows_into_transcript_window(&mut message_rows, generated_media_rows);
    let has_more_history =
        loaded_turn_count >= effective_limit && earliest_ordinal.is_some_and(|ordinal| ordinal > 0);
    let messages =
        hydrate_transcript_window_messages(&controller, session_id, message_rows).await?;

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
    let (message_rows, effective_limit) = query_session_tool_detail_rows_with_retry(
        &controller,
        session_id,
        turn_index,
        before_ordinal,
        requested_limit,
    )
    .await?;
    let mut message_rows = message_rows;
    message_rows.sort_by_key(|row| row.ordinal);
    let earliest_ordinal = message_rows.first().map(|row| row.ordinal);
    let has_more_detail = message_rows.len() >= effective_limit
        && earliest_ordinal.is_some_and(|ordinal| ordinal > 0);
    let message_body_chunks =
        query_session_message_body_chunks_for_rows(&controller, session_id, &message_rows).await?;
    let messages = message_rows
        .into_iter()
        .map(|row| stored_session_message_row_to_entry(row, &message_body_chunks))
        .collect::<Vec<_>>();

    Ok(SessionToolDetailPage {
        messages,
        earliest_ordinal,
        has_more_detail,
    })
}

/// Loads the complete persisted tool-detail stream for one conversation turn by walking every paged slice until no older tool rows remain.
/// 通过遍历每一页持久化分页切片直到不存在更早工具行，为单个对话轮次加载完整的工具明细流。
pub async fn load_complete_session_tool_detail_turn(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
) -> Result<SessionToolDetailPage, String> {
    let mut before_ordinal = None;
    let mut aggregated_messages = Vec::new();
    let mut earliest_ordinal = None;

    loop {
        let page = load_session_tool_detail_page(
            controller.clone(),
            session_id,
            turn_index,
            before_ordinal,
            SESSION_TOOL_DETAIL_PAGE_LIMIT,
        )
        .await?;
        let page_earliest_ordinal = page.earliest_ordinal;
        earliest_ordinal = page_earliest_ordinal.or(earliest_ordinal);

        if page.messages.is_empty() {
            break;
        }

        merge_tool_detail_page_into_aggregate(&mut aggregated_messages, page.messages);

        if !page.has_more_detail
            || page_earliest_ordinal.is_none()
            || page_earliest_ordinal == before_ordinal
        {
            break;
        }

        before_ordinal = page_earliest_ordinal;
    }

    Ok(SessionToolDetailPage {
        messages: aggregated_messages,
        earliest_ordinal,
        has_more_detail: false,
    })
}

/// Queries one bounded transcript window for a single session, ordered newest-first at SQL level.
/// 查询单个会话的一段有界转录窗口，SQL 层按新到旧排序。
async fn query_session_message_window_rows(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    before_ordinal: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredSessionMessageRow>, String> {
    let session_id_text = session_id.to_string();
    let limit = limit.max(1).min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT) as i64;
    let (statement, params) = if let Some(before_ordinal) = before_ordinal {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
INNER JOIN (
    SELECT DISTINCT turn_index
    FROM session_messages
    WHERE session_id = ?1 AND ordinal < ?2
    ORDER BY turn_index DESC
    LIMIT ?3
) AS selected_turns ON selected_turns.turn_index = m.turn_index
WHERE m.session_id = ?1 AND m.ordinal < ?2 AND m.kind NOT IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?4;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(before_ordinal),
                ControllerSqliteValue::Int64(limit),
                ControllerSqliteValue::Int64(
                    session_transcript_row_limit_for_turns(limit as usize) as i64,
                ),
            ],
        )
    } else {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
INNER JOIN (
    SELECT DISTINCT turn_index
    FROM session_messages
    WHERE session_id = ?1
    ORDER BY turn_index DESC
    LIMIT ?2
) AS selected_turns ON selected_turns.turn_index = m.turn_index
WHERE m.session_id = ?1 AND m.kind NOT IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?3;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(limit),
                ControllerSqliteValue::Int64(
                    session_transcript_row_limit_for_turns(limit as usize) as i64,
                ),
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
        .map_err(|error| format!("查询会话转录窗口失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageRow>(&result.json_data)
}

/// Counts distinct persisted turn indexes in one loaded message window.
/// 统计一个已加载消息窗口中的不同持久化轮次索引数量。
fn loaded_message_turn_count(message_rows: &[StoredSessionMessageRow]) -> usize {
    message_rows
        .iter()
        .map(|row| row.turn_index)
        .collect::<BTreeSet<_>>()
        .len()
}

/// Prepends one older tool-detail page into the aggregate turn snapshot while preserving ascending display order and stable message identifiers.
/// 将一页更早的工具明细预置到聚合轮次快照前方，同时保持正序展示与稳定消息标识。
fn merge_tool_detail_page_into_aggregate(
    aggregate: &mut Vec<SessionMessageEntry>,
    page_messages: Vec<SessionMessageEntry>,
) {
    if aggregate.is_empty() {
        *aggregate = page_messages;
        return;
    }

    let existing_ids = aggregate
        .iter()
        .map(|message| message.id.clone())
        .collect::<BTreeSet<_>>();
    let mut older_messages = page_messages
        .into_iter()
        .filter(|message| !existing_ids.contains(&message.id))
        .collect::<Vec<_>>();
    older_messages.append(aggregate);
    *aggregate = older_messages;
}

#[cfg(test)]
mod tool_detail_aggregation_tests {
    use super::*;

    /// Verifies that older tool-detail pages are prepended ahead of newer pages so the final aggregate stays in ascending transcript order.
    /// 验证更早工具明细分页会被预置到较新分页之前，确保最终聚合结果保持转录正序。
    #[test]
    fn merge_tool_detail_page_into_aggregate_prepends_older_rows() {
        let mut aggregate = vec![
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-3", "body-3")
                .with_runtime_metadata("tool-3", "3"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-4", "body-4")
                .with_runtime_metadata("tool-4", "4"),
        ];
        let older_page = vec![
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-1", "body-1")
                .with_runtime_metadata("tool-1", "1"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-2", "body-2")
                .with_runtime_metadata("tool-2", "2"),
        ];

        merge_tool_detail_page_into_aggregate(&mut aggregate, older_page);

        assert_eq!(
            aggregate
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["tool-1", "tool-2", "tool-3", "tool-4"]
        );
    }

    /// Verifies that repeated rows from overlapping tool-detail pages are deduplicated while preserving the earliest-first aggregate order.
    /// 验证来自重叠工具明细分页的重复消息会被去重，同时保持聚合结果的先早后晚顺序。
    #[test]
    fn merge_tool_detail_page_into_aggregate_deduplicates_repeated_rows() {
        let mut aggregate = vec![
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-3", "body-3")
                .with_runtime_metadata("tool-3", "3"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-4", "body-4")
                .with_runtime_metadata("tool-4", "4"),
        ];
        let older_page = vec![
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-2", "body-2")
                .with_runtime_metadata("tool-2", "2"),
            SessionMessageEntry::new(SessionMessageKind::Assistant, "tool-3", "stale-body-3")
                .with_runtime_metadata("tool-3", "3"),
        ];

        merge_tool_detail_page_into_aggregate(&mut aggregate, older_page);

        assert_eq!(
            aggregate
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["tool-2", "tool-3", "tool-4"]
        );
        assert_eq!(aggregate[1].body, "body-3");
    }
}

/// Queries and rebuilds chunked message bodies for one already-bounded transcript window.
/// 查询并重建一个已限界转录窗口中的分片消息正文。
async fn query_session_message_body_chunks_for_rows(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_rows: &[StoredSessionMessageRow],
) -> Result<BTreeMap<String, String>, String> {
    if message_rows.is_empty() {
        return Ok(BTreeMap::new());
    }

    let message_ids = message_rows
        .iter()
        .map(|row| row.id.as_str())
        .collect::<Vec<_>>();
    let mut rebuilt_bodies = BTreeMap::new();
    // Prefer one batched chunk lookup so a normal session switch pays a single storage round trip.
    // 优先使用一次批量分片查询，让普通会话切换只承担一次存储往返。
    let chunk_rows_result =
        query_session_message_chunk_rows_for_message_ids(controller, session_id, &message_ids)
            .await;
    let chunk_rows = match chunk_rows_result {
        Ok(chunk_rows) => chunk_rows,
        Err(batch_error) => {
            let mut fallback_rows = Vec::new();
            for message_id in &message_ids {
                fallback_rows.extend(
                    query_session_message_chunk_rows_for_message(
                        controller, session_id, message_id,
                    )
                    .await
                    .map_err(|fallback_error| {
                        format!(
                            "查询会话消息分片内容失败：批量读取失败后逐条回退仍未成功，session_id={session_id}, message_id={message_id}, batch_error={batch_error}, fallback_error={fallback_error}"
                        )
                    })?,
                );
            }
            fallback_rows
        }
    };

    let mut chunk_rows_by_message_id = BTreeMap::<String, Vec<StoredSessionMessageChunkRow>>::new();
    for chunk_row in chunk_rows {
        chunk_rows_by_message_id
            .entry(chunk_row.message_id.clone())
            .or_default()
            .push(chunk_row);
    }
    for (message_id, chunk_rows) in chunk_rows_by_message_id {
        let rebuilt_body = rebuild_session_message_body_from_chunks(&message_id, chunk_rows)?;
        rebuilt_bodies.insert(message_id, rebuilt_body);
    }

    Ok(rebuilt_bodies)
}

/// Queries all chunk rows for the visible transcript window in one controller round trip.
/// 通过一次控制器往返查询可见转录窗口内的全部正文分片行。
async fn query_session_message_chunk_rows_for_message_ids(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_ids: &[&str],
) -> Result<Vec<StoredSessionMessageChunkRow>, String> {
    if message_ids.is_empty() {
        return Ok(Vec::new());
    }

    let placeholders = (2..message_ids.len() + 2)
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(", ");
    let statement = format!(
        r#"
SELECT message_id, chunk_index, content
FROM session_message_chunks
WHERE session_id = ?1 AND message_id IN ({placeholders})
ORDER BY message_id ASC, chunk_index ASC;
"#
    );
    let mut params = Vec::with_capacity(message_ids.len() + 1);
    params.push(ControllerSqliteValue::String(session_id.to_string()));
    params.extend(
        message_ids
            .iter()
            .map(|message_id| ControllerSqliteValue::String((*message_id).to_string())),
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
        .map_err(|error| format!("查询会话消息分片内容失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageChunkRow>(&result.json_data)
}

/// Queries chunk rows for one oversized message in small pages to stay below gRPC response limits.
/// 以小分页查询单条超大消息的分片行，避免超过 gRPC 响应限制。
async fn query_session_message_chunk_rows_for_message(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_id: &str,
) -> Result<Vec<StoredSessionMessageChunkRow>, String> {
    let mut rows = Vec::new();
    let mut last_chunk_index = -1_i64;
    let mut effective_limit = SESSION_MESSAGE_CHUNK_QUERY_LIMIT.max(1);

    loop {
        match query_session_message_chunk_rows_after_index(
            controller,
            session_id,
            message_id,
            last_chunk_index,
            effective_limit,
        )
        .await
        {
            Ok(page) => {
                if page.is_empty() {
                    break;
                }
                last_chunk_index = page
                    .last()
                    .map(|row| row.chunk_index)
                    .unwrap_or(last_chunk_index);
                let page_len = page.len();
                rows.extend(page);
                if page_len < effective_limit {
                    break;
                }
            }
            Err(error) if effective_limit > 1 && is_grpc_message_frame_size_error(&error) => {
                effective_limit = (effective_limit / 2).max(1);
            }
            Err(error) => return Err(error),
        }
    }

    Ok(rows)
}

/// Queries one bounded chunk page after the provided chunk-index cursor.
/// 查询给定分片序号游标之后的一段有限分片页。
async fn query_session_message_chunk_rows_after_index(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_id: &str,
    after_chunk_index: i64,
    limit: usize,
) -> Result<Vec<StoredSessionMessageChunkRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT message_id, chunk_index, content
FROM session_message_chunks
WHERE session_id = ?1 AND message_id = ?2 AND chunk_index > ?3
ORDER BY chunk_index ASC
LIMIT ?4;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::String(message_id.to_string()),
                ControllerSqliteValue::Int64(after_chunk_index),
                ControllerSqliteValue::Int64(limit.max(1) as i64),
            ],
        )
        .await
        .map_err(|error| format!("查询会话消息分片内容失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageChunkRow>(&result.json_data)
}

/// Queries hidden compaction checkpoint rows for one session.
/// 查询单个会话对应的隐藏压缩检查点行。
async fn query_session_compaction_rows_for_session(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Vec<StoredCompactionRow>, String> {
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
        .map_err(|error| format!("查询会话压缩节点失败：{error}"))?;

    decode_query_rows::<StoredCompactionRow>(&result.json_data)
}

/// Queries the full runtime-state row for one hydrated session, including the persisted canonical conversation-state payload when available.
/// 查询单个已水合会话的完整运行时状态行，并在可用时包含持久化规范会话状态载荷。
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

/// Queries one transcript window with progressive limit reduction when gRPC frame limits are hit.
/// 查询单段转录窗口，并在触发 gRPC 帧限制时逐步缩小 limit 重试。
async fn query_session_message_window_rows_with_retry(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    before_ordinal: Option<i64>,
    requested_limit: usize,
) -> Result<(Vec<StoredSessionMessageRow>, usize), String> {
    let mut effective_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    // Defer legacy migration until a real frame-size failure proves it is needed on the hot path.
    // 将旧版迁移延后到真实帧大小失败后再执行，避免热路径每次切换会话都多一次查询。
    let mut attempted_legacy_migration = false;

    loop {
        match query_session_message_window_rows(
            controller,
            session_id,
            before_ordinal,
            effective_limit,
        )
        .await
        {
            Ok(rows) => return Ok((rows, effective_limit)),
            Err(error) => {
                let is_frame_size_error = is_grpc_message_frame_size_error(&error);
                let should_attempt_legacy_migration = if attempted_legacy_migration {
                    false
                } else if is_frame_size_error {
                    true
                } else if should_probe_legacy_oversized_message_candidates(
                    &error,
                    attempted_legacy_migration,
                ) {
                    session_has_legacy_oversized_message_candidates(controller, session_id).await?
                } else {
                    false
                };
                if should_attempt_legacy_migration {
                    attempted_legacy_migration = true;
                    let migration_report =
                        migrate_legacy_oversized_session_messages(controller, session_id).await?;
                    if migration_report.migrated_message_count > 0 {
                        continue;
                    }
                }
                if effective_limit > 1 && is_frame_size_error {
                    effective_limit = (effective_limit / 2).max(1);
                    continue;
                }
                return Err(error);
            }
        }
    }
}

/// Queries transcript pages repeatedly until the requested number of complete turns has been loaded or no older history remains.
/// 反复查询转录分页，直到已加载到请求数量的完整轮次，或数据库中已不存在更早历史。
async fn query_session_message_window_rows_until_turn_target(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    target_turn_count: usize,
    page_turn_limit: usize,
) -> Result<(Vec<StoredSessionMessageRow>, bool), String> {
    let normalized_page_turn_limit = page_turn_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT);
    let normalized_target_turn_count = target_turn_count.max(normalized_page_turn_limit);
    let mut before_ordinal = None;
    let mut aggregated_rows = Vec::new();
    let mut aggregated_ids = BTreeSet::new();
    let has_more_history = loop {
        let (page_rows, effective_limit) = query_session_message_window_rows_with_retry(
            controller,
            session_id,
            before_ordinal,
            normalized_page_turn_limit,
        )
        .await?;
        let mut page_rows = page_rows;
        page_rows.sort_by_key(|row| row.ordinal);
        let page_turn_count = loaded_message_turn_count(&page_rows);
        let page_earliest_ordinal = page_rows.first().map(|row| row.ordinal);
        let page_has_more_history = page_turn_count >= effective_limit
            && page_earliest_ordinal.is_some_and(|ordinal| ordinal > 0);

        for row in page_rows {
            if aggregated_ids.insert(row.id.clone()) {
                aggregated_rows.push(row);
            }
        }

        aggregated_rows.sort_by_key(|row| row.ordinal);
        if loaded_message_turn_count(&aggregated_rows) >= normalized_target_turn_count
            || !page_has_more_history
        {
            break page_has_more_history;
        }

        before_ordinal = page_earliest_ordinal;
    };

    Ok((aggregated_rows, has_more_history))
}

/// Queries one tool-detail window with progressive limit reduction when gRPC frame limits are hit.
/// 查询单段工具明细窗口，并在触发 gRPC 帧限制时逐步缩小 limit 重试。
async fn query_session_tool_detail_rows_with_retry(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
    before_ordinal: Option<i64>,
    requested_limit: usize,
) -> Result<(Vec<StoredSessionMessageRow>, usize), String> {
    let mut effective_limit = requested_limit
        .max(1)
        .min(SESSION_TRANSCRIPT_LEGACY_ROW_LIMIT);
    // Defer legacy migration until a real frame-size failure proves it is needed on the hot path.
    // 将旧版迁移延后到真实帧大小失败后再执行，避免热路径每次切换会话都多一次查询。
    let mut attempted_legacy_migration = false;

    loop {
        match query_session_tool_detail_rows(
            controller,
            session_id,
            turn_index,
            before_ordinal,
            effective_limit,
        )
        .await
        {
            Ok(rows) => return Ok((rows, effective_limit)),
            Err(error) => {
                let is_frame_size_error = is_grpc_message_frame_size_error(&error);
                let should_attempt_legacy_migration = if attempted_legacy_migration {
                    false
                } else if is_frame_size_error {
                    true
                } else if should_probe_legacy_oversized_message_candidates(
                    &error,
                    attempted_legacy_migration,
                ) {
                    session_has_legacy_oversized_message_candidates(controller, session_id).await?
                } else {
                    false
                };
                if should_attempt_legacy_migration {
                    attempted_legacy_migration = true;
                    let migration_report =
                        migrate_legacy_oversized_session_messages(controller, session_id).await?;
                    if migration_report.migrated_message_count > 0 {
                        continue;
                    }
                }
                if effective_limit > 1 && is_frame_size_error {
                    effective_limit = (effective_limit / 2).max(1);
                    continue;
                }
                return Err(error);
            }
        }
    }
}

/// Returns whether one failed transcript-read attempt should still probe for legacy oversized inline-message candidates before surfacing the original error.
/// 返回一次转录读取失败在上抛原始错误前是否仍应额外探测旧版超大内联消息候选。
fn should_probe_legacy_oversized_message_candidates(
    error: &str,
    attempted_legacy_migration: bool,
) -> bool {
    !attempted_legacy_migration && !is_grpc_message_frame_size_error(error)
}

/// Queries one bounded tool-detail window for a single turn, ordered newest-first at SQL level.
/// 查询单个轮次的一段有界工具明细窗口，SQL 层按新到旧排序。
async fn query_session_tool_detail_rows(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    turn_index: i64,
    before_ordinal: Option<i64>,
    limit: usize,
) -> Result<Vec<StoredSessionMessageRow>, String> {
    let session_id_text = session_id.to_string();
    let limit = limit.max(1).min(SESSION_TRANSCRIPT_LEGACY_ROW_LIMIT) as i64;
    let (statement, params) = if let Some(before_ordinal) = before_ordinal {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
WHERE m.session_id = ?1
  AND m.turn_index = ?2
  AND m.ordinal < ?3
  AND m.kind IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?4;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(turn_index),
                ControllerSqliteValue::Int64(before_ordinal),
                ControllerSqliteValue::Int64(limit),
            ],
        )
    } else {
        (
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
WHERE m.session_id = ?1
  AND m.turn_index = ?2
  AND m.kind IN ('tool-call-output', 'tool-result-output')
ORDER BY m.ordinal DESC
LIMIT ?3;
"#,
            vec![
                ControllerSqliteValue::String(session_id_text),
                ControllerSqliteValue::Int64(turn_index),
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
        .map_err(|error| format!("查询会话工具明细窗口失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageRow>(&result.json_data)
}

/// Queries generated-media tool-result rows for all turns already present in one transcript window.
/// 为一个已加载转录窗口中的全部轮次查询生成媒体工具结果行。
async fn query_generated_media_tool_result_rows_for_turns(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    transcript_rows: &[StoredSessionMessageRow],
) -> Result<Vec<StoredSessionMessageRow>, String> {
    let turn_indices = transcript_rows
        .iter()
        .map(|row| row.turn_index)
        .collect::<BTreeSet<_>>();
    if turn_indices.is_empty() {
        return Ok(Vec::new());
    }

    let session_id_text = session_id.to_string();
    let turn_indices = turn_indices.into_iter().collect::<Vec<_>>();
    let mut rows = Vec::new();
    for chunk in turn_indices.chunks(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT) {
        let placeholders = (2..=chunk.len() + 1)
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let statement = format!(
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, '' AS content, m.meta_json, m.created_at
FROM session_messages AS m
WHERE m.session_id = ?1
  AND m.visible != 0
  AND m.turn_index IN ({placeholders})
  AND m.kind = 'tool-result-output'
  AND m.meta_json IS NOT NULL
  AND m.meta_json LIKE '%generated_attachments%'
ORDER BY m.ordinal ASC;
"#
        );
        let mut params = Vec::with_capacity(chunk.len() + 1);
        params.push(ControllerSqliteValue::String(session_id_text.clone()));
        params.extend(chunk.iter().copied().map(ControllerSqliteValue::Int64));
        let result = controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(controller),
                statement,
                params,
            )
            .await
            .map_err(|error| format!("查询会话生成媒体工具结果窗口失败：{error}"))?;
        rows.extend(
            decode_query_rows::<StoredSessionMessageRow>(&result.json_data)?
                .into_iter()
                .filter(stored_session_message_row_contains_generated_attachment_host_result),
        );
    }

    Ok(rows)
}

/// Queries the distinct turn indices that own persisted tool-call or tool-result rows for the already loaded transcript turns.
/// 查询当前已加载转录轮次中仍然拥有持久化工具调用或工具结果行的去重轮次索引集合。
async fn query_tool_detail_turn_indices_for_turns(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    transcript_rows: &[StoredSessionMessageRow],
) -> Result<Vec<i64>, String> {
    let turn_indices = transcript_rows
        .iter()
        .map(|row| row.turn_index)
        .collect::<BTreeSet<_>>();
    if turn_indices.is_empty() {
        return Ok(Vec::new());
    }

    let session_id_text = session_id.to_string();
    let turn_indices = turn_indices.into_iter().collect::<Vec<_>>();
    let mut hydrated_tool_turn_indices = Vec::new();
    for chunk in turn_indices.chunks(SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT) {
        let placeholders = (2..=chunk.len() + 1)
            .map(|index| format!("?{index}"))
            .collect::<Vec<_>>()
            .join(", ");
        let statement = format!(
            r#"
SELECT DISTINCT m.turn_index
FROM session_messages AS m
WHERE m.session_id = ?1
  AND m.turn_index IN ({placeholders})
  AND m.kind IN ('tool-call-output', 'tool-result-output')
ORDER BY m.turn_index ASC;
"#
        );
        let mut params = Vec::with_capacity(chunk.len() + 1);
        params.push(ControllerSqliteValue::String(session_id_text.clone()));
        params.extend(chunk.iter().copied().map(ControllerSqliteValue::Int64));
        let result = controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(controller),
                &statement,
                params,
            )
            .await
            .map_err(|error| format!("查询会话工具明细轮次索引失败：{error}"))?;
        let rows = decode_query_rows::<StoredSessionTurnIndexRow>(&result.json_data)?;
        hydrated_tool_turn_indices.extend(rows.into_iter().map(|row| row.turn_index));
    }

    hydrated_tool_turn_indices.sort_unstable();
    hydrated_tool_turn_indices.dedup();
    Ok(hydrated_tool_turn_indices)
}

/// Loads every complete tool-detail message stream required by the provided turn indices so transcript hydration and tool payload hydration stay in sync.
/// 为给定轮次索引加载全部完整工具明细消息流，确保转录正文水合与工具载荷水合同步完成。
async fn load_complete_tool_detail_messages_for_turns(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    turn_indices: &[i64],
) -> Result<Vec<SessionMessageEntry>, String> {
    if turn_indices.is_empty() {
        return Ok(Vec::new());
    }

    let mut hydrated_messages = Vec::new();
    for turn_index in turn_indices {
        let page = load_complete_session_tool_detail_turn(
            controller.clone(),
            session_id,
            *turn_index,
        )
        .await?;
        hydrated_messages.extend(page.messages);
    }

    Ok(hydrated_messages)
}

/// Hydrates one transcript window into display entries while avoiding chunk fetches for synthetic media-only tool-result rows.
/// 将一个转录窗口水合为展示消息，同时避免为合成的仅媒体工具结果行加载正文分片。
async fn hydrate_transcript_window_messages(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    message_rows: Vec<StoredSessionMessageRow>,
) -> Result<Vec<SessionMessageEntry>, String> {
    if message_rows.is_empty() {
        return Ok(Vec::new());
    }

    let body_backed_rows = message_rows
        .iter()
        .filter(|row| !row.content.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let message_body_chunks =
        query_session_message_body_chunks_for_rows(controller, session_id, &body_backed_rows)
            .await?;

    Ok(message_rows
        .into_iter()
        .map(|row| stored_session_message_row_to_entry(row, &message_body_chunks))
        .collect())
}

/// Resolves the startup hydration turn target by taking the larger value between the configured page size and the overlap required to expose five turns before the active compaction boundary.
/// 通过取“配置分页大小”和“为暴露活动压缩边界前五轮所需的重叠范围”中的较大值，解析启动水合轮次目标。
async fn resolve_initial_hydration_turn_count(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    requested_turn_count: usize,
    compactions: &[SessionCompactionEntry],
    runtime_state_row: Option<&StoredRuntimeStateRow>,
) -> Result<usize, String> {
    let mut effective_turn_count = requested_turn_count.max(1);
    let Some(boundary_turn_index) =
        active_compaction_overlap_boundary_turn_index(compactions, runtime_state_row)
    else {
        return Ok(effective_turn_count);
    };
    let Some(latest_turn_index) = query_session_latest_turn_index(controller, session_id).await?
    else {
        return Ok(effective_turn_count);
    };
    let overlap_turn_count = SESSION_TRANSCRIPT_COMPACTION_OVERLAP_TURN_COUNT as i64;
    let earliest_required_turn_index =
        boundary_turn_index.saturating_sub(overlap_turn_count).max(0);
    let turns_needed = latest_turn_index
        .saturating_sub(earliest_required_turn_index)
        .saturating_add(1) as usize;
    effective_turn_count = effective_turn_count.max(turns_needed);
    Ok(effective_turn_count)
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
        .or_else(|| active_compaction.source_until_turn_index.map(|turn_index| turn_index + 1))
}

/// Queries the latest persisted user-turn index for one session so startup hydration can expand directly to the required overlap boundary.
/// 查询单个会话最新的持久化用户轮次索引，使启动水合能够直接扩展到所需的重叠边界。
async fn query_session_latest_turn_index(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<i64>, String> {
    #[derive(Debug, Deserialize)]
    struct SessionLatestTurnIndexRow {
        /// Optional latest persisted turn index.
        /// 可选的最新持久化轮次索引。
        latest_turn_index: Option<i64>,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT MAX(turn_index) AS latest_turn_index
FROM session_messages
WHERE session_id = ?1;
"#,
            vec![ControllerSqliteValue::String(session_id.to_string())],
        )
        .await
        .map_err(|error| format!("查询会话最新轮次索引失败：{error}"))?;
    let row = decode_query_rows::<SessionLatestTurnIndexRow>(&result.json_data)?
        .into_iter()
        .next();
    Ok(row.and_then(|entry| entry.latest_turn_index))
}

/// Restores the hydrated runtime state for one session, preferring the persisted canonical conversation state and falling back to a full session replay only when the large payload is still missing.
/// 为单个会话恢复已水合运行时状态，优先使用持久化规范会话状态，并仅在大载荷仍缺失时回退到完整会话重放。
async fn hydrate_session_runtime_state(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    runtime_state_row: Option<StoredRuntimeStateRow>,
    recent_messages: &[SessionMessageEntry],
) -> Result<Option<SessionRuntimeStateEntry>, String> {
    let runtime_state = if let Some(row) = runtime_state_row {
        let mut runtime_state = stored_runtime_state_row_to_entry(row);
        if runtime_state.conversation_state.records.is_empty()
            && runtime_state.conversation_state.messages.is_empty()
        {
            let continuation = runtime_state.conversation_state.continuation.clone();
            runtime_state.conversation_state =
                load_full_session_conversation_state(controller, session_id)
                    .await?
                    .unwrap_or_else(|| {
                        let mut rebuilt = conversation_state_from_session_messages(recent_messages);
                        rebuilt.continuation = continuation.clone();
                        rebuilt
                    });
            if runtime_state.conversation_state.continuation.provider_id.is_none() {
                runtime_state.conversation_state.continuation = continuation;
            }
        }
        Some(runtime_state)
    } else {
        load_full_session_conversation_state(controller, session_id)
            .await?
            .or_else(|| {
                (!recent_messages.is_empty())
                    .then(|| conversation_state_from_session_messages(recent_messages))
            })
            .map(|conversation_state| SessionRuntimeStateEntry {
                conversation_state,
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            })
    };

    Ok(runtime_state)
}

/// Loads the full canonical conversation state for one session by replaying all persisted message rows in bounded turn chunks when no prebuilt conversation-state payload is available.
/// 当预构建规范会话状态载荷不可用时，通过按轮次分块重放全部持久化消息行为单个会话加载完整规范会话状态。
async fn load_full_session_conversation_state(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<Option<SessionConversationState>, String> {
    let Some(latest_turn_index) = query_session_latest_turn_index(controller, session_id).await?
    else {
        return Ok(None);
    };

    let mut all_rows = Vec::new();
    let chunk_span = SESSION_TRANSCRIPT_WINDOW_MAX_TURN_LIMIT as i64;
    let mut start_turn_index = 0_i64;
    while start_turn_index <= latest_turn_index {
        let end_turn_index =
            (start_turn_index + chunk_span - 1).min(latest_turn_index);
        all_rows.extend(
            query_session_message_rows_for_turn_range(
                controller,
                session_id,
                start_turn_index,
                end_turn_index,
            )
            .await?,
        );
        start_turn_index = end_turn_index.saturating_add(1);
    }

    if all_rows.is_empty() {
        return Ok(None);
    }

    all_rows.sort_by_key(|row| row.ordinal);
    all_rows.dedup_by(|left, right| left.id == right.id);
    let messages = hydrate_transcript_window_messages(controller, session_id, all_rows).await?;
    Ok((!messages.is_empty()).then(|| conversation_state_from_session_messages(&messages)))
}

/// Queries all persisted session-message rows for one inclusive user-turn range while preserving original ordinal order.
/// 在保持原始序号顺序的前提下，查询某个闭区间用户轮次范围内的全部持久化会话消息行。
async fn query_session_message_rows_for_turn_range(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    start_turn_index: i64,
    end_turn_index: i64,
) -> Result<Vec<StoredSessionMessageRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT m.id, m.ordinal, m.role, m.kind, m.visible, m.turn_index, m.content, m.meta_json, m.created_at
FROM session_messages AS m
WHERE m.session_id = ?1
  AND m.turn_index >= ?2
  AND m.turn_index <= ?3
ORDER BY m.ordinal ASC;
"#,
            vec![
                ControllerSqliteValue::String(session_id.to_string()),
                ControllerSqliteValue::Int64(start_turn_index),
                ControllerSqliteValue::Int64(end_turn_index),
            ],
        )
        .await
        .map_err(|error| format!("查询会话完整转录轮次范围失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageRow>(&result.json_data)
}

#[cfg(test)]
mod tests {
    use super::should_probe_legacy_oversized_message_candidates;

    /// Keeps the explicit frame-size fast path from paying an extra candidate probe.
    /// 确保已知帧大小错误继续走显式快路径，而不会额外支付候选探测查询。
    #[test]
    fn legacy_probe_is_skipped_for_known_frame_size_errors() {
        assert!(!should_probe_legacy_oversized_message_candidates(
            "decoded message length too large",
            false,
        ));
    }

    /// Probes legacy oversized candidates for unknown read failures before giving up on recovery.
    /// 对未知读取失败先探测旧版超大消息候选，再决定是否放弃恢复路径。
    #[test]
    fn legacy_probe_is_enabled_for_unknown_errors_before_migration() {
        assert!(should_probe_legacy_oversized_message_candidates(
            "rpc failure without frame hint",
            false,
        ));
    }

    /// Avoids probing the same legacy migration path twice after one attempt has already been made.
    /// 在已经尝试过一次旧版迁移后，避免再次重复探测同一条恢复路径。
    #[test]
    fn legacy_probe_is_skipped_after_one_migration_attempt() {
        assert!(!should_probe_legacy_oversized_message_candidates(
            "rpc failure without frame hint",
            true,
        ));
    }
}
