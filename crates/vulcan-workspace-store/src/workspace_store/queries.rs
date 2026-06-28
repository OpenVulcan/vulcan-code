use super::*;

pub(super) fn app_sqlite_binding_id(controller: &ControllerRuntimeHandle) -> &str {
    controller.sqlite_binding_id.as_str()
}

/// Queries the number of visible project rows currently stored in the application database.
/// 查询当前应用数据库中已存储的可见项目行数量。
pub(super) async fn query_project_count(
    controller: &ControllerRuntimeHandle,
) -> Result<u64, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT COUNT(*) AS total FROM projects WHERE archived = 0;",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目数量失败：{error}"))?;

    #[derive(Deserialize)]
    struct CountRow {
        /// Numeric total returned by SQLite.
        /// SQLite 返回的数值总量。
        total: u64,
    }

    let rows = decode_query_rows::<CountRow>(&result.json_data)?;
    Ok(rows.first().map(|row| row.total).unwrap_or(0))
}

/// Queries visible project rows ordered by pinned section and stable order.
/// 查询按置顶分区与稳定顺序排序后的可见项目行。
pub(super) async fn query_project_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredProjectRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, name, root_path, pinned, sort_order, icon_pack, icon_key, archived
FROM projects
ORDER BY pinned DESC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目列表失败：{error}"))?;

    decode_query_rows::<StoredProjectRow>(&result.json_data)
}

/// Queries lightweight session locator rows ordered without scanning segmented directories.
/// 查询轻量会话定位行，不扫描分段目录。
pub(super) async fn query_session_index_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredSessionIndexRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT
    id,
    project_id,
    project_tag,
    title,
    title_source,
    archived,
    sort_order,
    created_at,
    updated_at
FROM workspace_session_index
ORDER BY archived ASC, project_id ASC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台会话定位索引失败：{error}"))?;

    decode_query_rows::<StoredSessionIndexRow>(&result.json_data)
}

/// Queries the persisted collapsed project identifiers, if a workspace UI state row exists.
/// 查询已持久化的折叠项目标识集合；若工作台 UI 状态行不存在则返回空。
pub(super) async fn query_collapsed_task_project_ids(
    controller: &ControllerRuntimeHandle,
) -> Result<Option<BTreeSet<String>>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT value FROM app_metadata WHERE key = ?1;",
            vec![ControllerSqliteValue::String(
                COLLAPSED_TASK_PROJECTS_METADATA_KEY.to_string(),
            )],
        )
        .await
        .map_err(|error| format!("查询工作台项目展开状态失败：{error}"))?;

    let rows = decode_query_rows::<StoredAppMetadataRow>(&result.json_data)?;
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<Vec<String>>(&row.value)
        .map_err(|error| format!("解析工作台项目展开状态失败：{error}"))?;
    Ok(Some(parsed.into_iter().collect()))
}

/// Queries active project identifiers used for soft-archive reconciliation.
/// 查询用于软归档对账的激活项目标识集合。
pub(super) async fn query_active_project_ids(
    controller: &ControllerRuntimeHandle,
) -> Result<BTreeSet<String>, String> {
    #[derive(Deserialize)]
    struct IdRow {
        /// Persisted identifier.
        /// 持久化标识。
        id: String,
    }

    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            "SELECT id FROM projects WHERE archived = 0;",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询激活项目标识失败：{error}"))?;

    Ok(decode_query_rows::<IdRow>(&result.json_data)?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

/// Queries visible session rows ordered by project and stable order.
/// 查询按项目与稳定顺序排序后的可见会话行。
/// Returns the maximum message rows fetched for one turn-level transcript window.
/// 返回单个轮次级转录窗口允许拉取的最大消息行数。
pub(super) fn session_transcript_row_limit_for_turns(turn_limit: usize) -> usize {
    SESSION_TRANSCRIPT_MIN_ROW_LIMIT
        .max(turn_limit.saturating_mul(SESSION_TRANSCRIPT_ROWS_PER_TURN_LIMIT))
}

/// Returns whether one persisted record kind should be part of the default transcript window.
/// 返回某个持久化记录类型是否应进入默认转录窗口。
#[cfg(test)]
pub(super) fn persisted_kind_in_default_transcript_window(kind: &str) -> bool {
    !matches!(kind, "tool-call-output" | "tool-result-output")
}

/// Returns whether one persisted record kind belongs to the on-demand tool-detail stream.
/// 返回某个持久化记录类型是否属于按需工具明细流。
#[cfg(test)]
pub(super) fn persisted_kind_in_tool_detail_stream(kind: &str) -> bool {
    matches!(kind, "tool-call-output" | "tool-result-output")
}

/// Returns no controller runtime documents because segmented state files are now the startup source.
/// 不再返回控制器运行态文档，因为分段 state 文件已经是启动事实源。
pub(super) async fn query_session_runtime_state_shell_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredRuntimeStateRow>, String> {
    let _ = controller;
    Ok(Vec::new())
}

/// Returns whether one controller error looks like tonic's protobuf message-size guard.
/// 判断一条控制器错误是否类似 tonic 的 protobuf 消息大小保护。
pub(super) fn is_grpc_message_frame_size_error(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("decoded message length too large")
        || normalized.contains("message length too large")
        || normalized.contains("limit is: 4194304")
}

/// Decodes one controller JSON row-set payload into typed Rust rows.
/// 将一条控制器 JSON 行集载荷解码为带类型的 Rust 行集合。
pub(super) fn decode_query_rows<T>(json_data: &str) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str::<Vec<T>>(json_data)
        .map_err(|error| format!("解析控制器 SQLite 查询结果失败：{error}"))
}

/// Converts one legacy runtime document into the in-memory runtime-state shape.
/// 将一个旧运行态文档转换为内存中的运行时状态结构。
pub(super) fn stored_runtime_state_row_to_entry(
    row: StoredRuntimeStateRow,
) -> SessionRuntimeStateEntry {
    let provider_usage = row
        .provider_usage_json
        .as_deref()
        .and_then(parse_provider_usage_from_json);
    SessionRuntimeStateEntry {
        active_agent_id: row.active_agent_id,
        active_provider_id: row.active_provider_id,
        active_model_id: row.active_model_id,
        active_openrouter_endpoint_route: None,
        active_reasoning_effort: None,
        active_temperature: None,
        active_work_mode: None,
        ai_tool_configuration: None,
        prompt_skill_overrides: Default::default(),
        context_injection_state: row
            .context_injection_json
            .as_deref()
            .and_then(parse_context_injection_state_from_json)
            .unwrap_or_default(),
        vmm: row
            .vmm_json
            .as_deref()
            .and_then(parse_vmm_config_layer_from_json)
            .unwrap_or_default(),
        todolist: Default::default(),
        plan_mode: Default::default(),
        vmm_writeback_outbox: row
            .vmm_writeback_outbox_json
            .as_deref()
            .and_then(parse_vmm_writeback_outbox_from_json)
            .unwrap_or_default(),
        vmm_profile_sync: row
            .vmm_profile_sync_json
            .as_deref()
            .and_then(parse_vmm_profile_sync_from_json)
            .unwrap_or_default(),
        last_compaction_id: row.last_compaction_id,
        auto_compaction_consecutive_failures: 0,
        context_start_message_id: row.context_start_message_id,
        live_context_budget_fingerprint: None,
        live_context_assembly_fingerprint: None,
        anchor_context_budget_fingerprint: None,
        anchor_context_assembly_fingerprint: None,
        last_provider_request_preview_markdown: None,
        ui_state: row
            .ui_state_json
            .as_deref()
            .and_then(parse_session_ui_state_from_json)
            .unwrap_or_default(),
        conversation_state: persisted_conversation_state_or_continuation(
            row.conversation_state_json,
            row.continuation_json,
        ),
        tool_call_record_ids: Default::default(),
        last_provider_usage: provider_usage.clone(),
        last_successful_provider_usage: provider_usage,
        last_safe_provider_request_count: None,
        active_precheck_turn_id: None,
        updated_at: row.updated_at,
    }
}

/// Builds a minimal conversation state from one persisted continuation payload.
/// 根据一条持久化 continuation 载荷构建最小会话状态。
pub(super) fn conversation_state_with_continuation(
    continuation_json: Option<String>,
) -> SessionConversationState {
    SessionConversationState {
        continuation: continuation_json
            .as_deref()
            .and_then(parse_session_continuation_from_json)
            .unwrap_or_default(),
        ..SessionConversationState::default()
    }
}

/// Restores one persisted canonical conversation state when available and otherwise falls back to the lightweight continuation-only payload.
/// 在可用时恢复一条持久化规范会话状态，否则回退到仅包含轻量 continuation 的载荷。
pub(super) fn persisted_conversation_state_or_continuation(
    conversation_state_json: Option<String>,
    continuation_json: Option<String>,
) -> SessionConversationState {
    parse_session_conversation_state_from_json(conversation_state_json.as_deref())
        .unwrap_or_else(|| conversation_state_with_continuation(continuation_json))
}
