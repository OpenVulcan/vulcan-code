//! Controller-backed persistence helpers for projects, sessions, transcript history, and hidden context nodes.
//! 项目、会话、转录历史与隐藏上下文节点的控制器持久化辅助模块。

use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::model::{
    SessionCompactionEntry, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
    SessionRunPhase, SessionRuntimeStateEntry, SessionState, SessionSummary, SessionTitleSource,
    SessionTranscriptUiState, TaskProjectSummary,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use vldb_controller_client::ControllerSqliteValue;

/// Stable in-memory snapshot hydrated from the controller-backed workspace database.
/// 从控制器承载的工作台数据库中还原出的稳定内存快照。
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    /// Visible project nodes shown in the task tree.
    /// 展示在任务树中的可见项目节点集合。
    pub task_projects: Vec<TaskProjectSummary>,
    /// Archived project nodes retained outside the visible tree for later restoration.
    /// 保留在可见树之外、供后续恢复使用的归档项目节点集合。
    pub archived_task_projects: Vec<TaskProjectSummary>,
    /// Visible sessions shown by the workbench.
    /// 展示在工作台中的可见会话集合。
    pub sessions: Vec<SessionSummary>,
    /// Archived sessions retained outside the visible workbench for later restoration.
    /// 保留在可见工作台之外、供后续恢复使用的归档会话集合。
    pub archived_sessions: Vec<SessionSummary>,
    /// Session-scoped transcript history kept fully in memory for the current runtime.
    /// 当前运行时完整保留在内存中的按会话归属转录历史。
    pub session_messages: BTreeMap<usize, Vec<SessionMessageEntry>>,
    /// Archived session-scoped transcript history retained for restoration.
    /// 为后续恢复保留的按归档会话归属转录历史集合。
    pub archived_session_messages: BTreeMap<usize, Vec<SessionMessageEntry>>,
    /// Session-scoped hidden compaction checkpoints retained for future context reconstruction.
    /// 为未来上下文重建保留的按会话归属隐藏压缩检查点集合。
    pub session_compactions: BTreeMap<usize, Vec<SessionCompactionEntry>>,
    /// Archived hidden compaction checkpoints retained for restoration.
    /// 为后续恢复保留的按归档会话归属隐藏压缩检查点集合。
    pub archived_session_compactions: BTreeMap<usize, Vec<SessionCompactionEntry>>,
    /// Session-scoped runtime state used to rebuild context start points and active identities.
    /// 用于重建上下文起点与激活身份的按会话归属运行时状态集合。
    pub session_runtime_state: BTreeMap<usize, SessionRuntimeStateEntry>,
    /// Archived session-scoped runtime state retained for restoration.
    /// 为后续恢复保留的按归档会话归属运行时状态集合。
    pub archived_session_runtime_state: BTreeMap<usize, SessionRuntimeStateEntry>,
    /// Fresh execution-phase defaults reconstructed from the loaded sessions.
    /// 根据已加载会话重建出的全新执行阶段默认值集合。
    pub session_run_phases: BTreeMap<usize, SessionRunPhase>,
    /// Expanded project identifiers restored for the task tree.
    /// 为任务树恢复出的项目展开标识集合。
    pub expanded_task_projects: BTreeSet<String>,
    /// Selected session identifier restored after startup.
    /// 启动完成后恢复出的当前选中会话标识。
    pub selected_session: usize,
    /// Next numeric session identifier seed.
    /// 下一条数值型会话标识种子。
    pub next_session_id: usize,
}

/// Raw project row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始项目行。
#[derive(Debug, Clone, Deserialize)]
struct StoredProjectRow {
    /// Stable project identifier persisted in SQLite.
    /// 持久化到 SQLite 的稳定项目标识。
    id: String,
    /// Project display name stored in SQLite.
    /// 存储在 SQLite 中的项目展示名称。
    name: String,
    /// Absolute root path stored in SQLite.
    /// 存储在 SQLite 中的绝对根目录路径。
    root_path: String,
    /// SQLite numeric pinned flag.
    /// SQLite 数值型置顶标记。
    pinned: i64,
    /// Stable section order value.
    /// 稳定的分区顺序值。
    sort_order: i64,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    archived: i64,
}

/// Raw session row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始会话行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionRow {
    /// Stable text session identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定文本会话标识。
    id: String,
    /// Owning project identifier stored in SQLite.
    /// 存储在 SQLite 中的所属项目标识。
    project_id: String,
    /// Visible session title stored in SQLite.
    /// 存储在 SQLite 中的可见会话标题。
    title: String,
    /// Persisted title source key.
    /// 持久化标题来源键。
    title_source: String,
    /// Stable order value inside the owning project.
    /// 所属项目内部的稳定顺序值。
    sort_order: i64,
    /// SQLite numeric archived flag.
    /// SQLite 数值型归档标记。
    archived: i64,
    /// Last-updated timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的最近更新时间戳。
    updated_at: String,
}

/// Raw session message row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始会话消息行。
#[derive(Debug, Clone, Deserialize)]
struct StoredSessionMessageRow {
    /// Stable message identifier stored in SQLite.
    /// 存储在 SQLite 中的稳定消息标识。
    id: String,
    /// Owning session identifier stored in SQLite.
    /// 存储在 SQLite 中的所属会话标识。
    session_id: String,
    /// Stable ordinal inside one transcript stream.
    /// 单个转录流内部的稳定序号。
    ordinal: i64,
    /// Persisted message role used to reconstruct the message kind.
    /// 用于重建消息类型的持久化消息角色。
    role: String,
    /// Persisted record kind key.
    /// 持久化记录类型键。
    kind: String,
    /// SQLite numeric visible flag.
    /// SQLite 数值型可见标记。
    visible: i64,
    /// Persisted primary text body.
    /// 持久化后的主要正文文本。
    content: String,
    /// Optional structured JSON metadata.
    /// 可选的结构化 JSON 元数据。
    meta_json: Option<String>,
    /// Created-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的创建时间。
    created_at: String,
}

/// Raw compaction checkpoint row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始压缩检查点行。
#[derive(Debug, Clone, Deserialize)]
struct StoredCompactionRow {
    /// Stable compaction identifier.
    /// 稳定压缩标识。
    id: String,
    /// Owning session identifier.
    /// 所属会话标识。
    session_id: String,
    /// Optional source tail message identifier.
    /// 可选的源历史尾消息标识。
    source_until_message_id: Option<String>,
    /// Optional rebuilt context start message identifier.
    /// 可选的新上下文起始消息标识。
    context_start_message_id: Option<String>,
    /// Optional hidden summary message identifier.
    /// 可选的隐藏摘要消息标识。
    summary_message_id: Option<String>,
    /// Created-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的创建时间。
    created_at: String,
}

/// Raw runtime-state row decoded from the controller SQLite query payload.
/// 从控制器 SQLite 查询载荷中解码出的原始运行时状态行。
#[derive(Debug, Clone, Deserialize)]
struct StoredRuntimeStateRow {
    /// Stable session identifier.
    /// 稳定会话标识。
    session_id: String,
    /// Optional active agent identifier.
    /// 可选的激活代理标识。
    active_agent_id: Option<String>,
    /// Optional active provider identifier.
    /// 可选的激活供应商标识。
    active_provider_id: Option<String>,
    /// Optional active model identifier.
    /// 可选的激活模型标识。
    active_model_id: Option<String>,
    /// Optional latest compaction identifier.
    /// 可选的最近一次压缩标识。
    last_compaction_id: Option<String>,
    /// Optional current context start message identifier.
    /// 可选的当前上下文起始消息标识。
    context_start_message_id: Option<String>,
    /// Optional persisted transcript UI state payload.
    /// 可选的持久化转录界面状态载荷。
    ui_state_json: Option<String>,
    /// Updated-at timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本形式存储的更新时间。
    updated_at: String,
}

/// Loads the persisted workspace snapshot or seeds the controller database when it is still empty.
/// 加载持久化工作台快照，若控制器数据库仍为空则先写入种子数据。
pub async fn load_or_seed_workspace(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    let existing_project_count = query_project_count(&controller).await?;

    if existing_project_count == 0 {
        return Ok(build_empty_workspace_snapshot());
    }

    let snapshot = load_workspace_snapshot(controller.clone()).await?;
    if is_legacy_seed_workspace(&snapshot) {
        clear_workspace_storage(&controller).await?;
        return Ok(build_empty_workspace_snapshot());
    }

    Ok(snapshot)
}

/// Loads the current persisted workspace snapshot from the controller-backed SQLite database.
/// 从控制器承载的 SQLite 数据库中加载当前持久化工作台快照。
pub async fn load_workspace_snapshot(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    let project_rows = query_project_rows(&controller).await?;
    let session_rows = query_session_rows(&controller).await?;
    let message_rows = query_session_message_rows(&controller).await?;
    let compaction_rows = query_session_compaction_rows(&controller).await?;
    let runtime_state_rows = query_session_runtime_state_rows(&controller).await?;

    let mut task_projects = Vec::new();
    let mut archived_task_projects = Vec::new();
    for row in &project_rows {
        let project = TaskProjectSummary::new(
            row.id.clone(),
            row.name.clone(),
            row.root_path.clone(),
            row.pinned != 0,
            row.sort_order.max(0) as usize,
            Vec::new(),
        );

        if row.archived != 0 {
            archived_task_projects.push(project);
        } else {
            task_projects.push(project);
        }
    }

    let archived_session_ids = session_rows
        .iter()
        .filter(|row| row.archived != 0)
        .filter_map(|row| parse_session_id(&row.id).ok())
        .collect::<BTreeSet<_>>();

    let mut transcript_map: BTreeMap<usize, Vec<SessionMessageEntry>> = BTreeMap::new();
    let mut archived_transcript_map: BTreeMap<usize, Vec<SessionMessageEntry>> = BTreeMap::new();
    let mut ordered_message_rows = message_rows;
    ordered_message_rows.sort_by_key(|row| (row.session_id.clone(), row.ordinal));

    for row in ordered_message_rows {
        let session_id = parse_session_id(&row.session_id)?;
        let title = row
            .meta_json
            .as_deref()
            .and_then(parse_message_title_from_meta)
            .unwrap_or_else(|| default_message_title(&row.role).to_string());
        let entry = SessionMessageEntry {
            id: row.id,
            kind: session_kind_from_role(&row.role),
            record_kind: SessionMessageRecordKind::from_persisted_key(&row.kind),
            visible: row.visible != 0,
            title,
            body: row.content,
            created_at: row.created_at,
        };

        if archived_session_ids.contains(&session_id) {
            archived_transcript_map
                .entry(session_id)
                .or_default()
                .push(entry);
        } else {
            transcript_map.entry(session_id).or_default().push(entry);
        }
    }

    let mut compaction_map: BTreeMap<usize, Vec<SessionCompactionEntry>> = BTreeMap::new();
    let mut archived_compaction_map: BTreeMap<usize, Vec<SessionCompactionEntry>> = BTreeMap::new();
    for row in compaction_rows {
        let session_id = parse_session_id(&row.session_id)?;
        let entry = SessionCompactionEntry {
            id: row.id,
            source_until_message_id: row.source_until_message_id,
            context_start_message_id: row.context_start_message_id,
            summary_message_id: row.summary_message_id,
            created_at: row.created_at,
        };

        if archived_session_ids.contains(&session_id) {
            archived_compaction_map
                .entry(session_id)
                .or_default()
                .push(entry);
        } else {
            compaction_map.entry(session_id).or_default().push(entry);
        }
    }

    let mut runtime_state_map: BTreeMap<usize, SessionRuntimeStateEntry> = BTreeMap::new();
    let mut archived_runtime_state_map: BTreeMap<usize, SessionRuntimeStateEntry> = BTreeMap::new();
    for row in runtime_state_rows {
        let session_id = parse_session_id(&row.session_id)?;
        let entry = SessionRuntimeStateEntry {
            active_agent_id: row.active_agent_id,
            active_provider_id: row.active_provider_id,
            active_model_id: row.active_model_id,
            last_compaction_id: row.last_compaction_id,
            context_start_message_id: row.context_start_message_id,
            ui_state: row
                .ui_state_json
                .as_deref()
                .and_then(parse_session_ui_state_from_json)
                .unwrap_or_default(),
            updated_at: row.updated_at,
        };

        if archived_session_ids.contains(&session_id) {
            archived_runtime_state_map.insert(session_id, entry);
        } else {
            runtime_state_map.insert(session_id, entry);
        }
    }

    let mut visible_sessions = Vec::new();
    let mut archived_sessions = Vec::new();
    let mut next_session_id = 1usize;

    for row in &session_rows {
        let session_id = parse_session_id(&row.id)?;
        next_session_id = next_session_id.max(session_id.saturating_add(1));
        let focus = session_focus_preview(
            transcript_map
                .get(&session_id)
                .or_else(|| archived_transcript_map.get(&session_id))
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
        let session_summary = SessionSummary::new(
            session_id,
            Some(row.project_id.clone()),
            row.title.clone(),
            focus,
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis_text(&row.updated_at),
        )
        .with_title_source(SessionTitleSource::from_persisted_key(&row.title_source));

        if row.archived != 0 {
            if let Some(project) = archived_task_projects
                .iter_mut()
                .find(|project| project.id == row.project_id)
            {
                project.session_ids.push(session_id);
            }
            archived_sessions.push(session_summary);
        } else {
            if let Some(project) = task_projects
                .iter_mut()
                .find(|project| project.id == row.project_id)
            {
                project.session_ids.push(session_id);
            }
            visible_sessions.push(session_summary);
        }
    }

    for project in &mut task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            session_rows
                .iter()
                .find(|row| row.id == session_id.to_string())
                .map(|row| row.sort_order)
                .unwrap_or(i64::MAX)
        });
    }

    for project in &mut archived_task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            session_rows
                .iter()
                .find(|row| row.id == session_id.to_string())
                .map(|row| row.sort_order)
                .unwrap_or(i64::MAX)
        });
    }

    visible_sessions.sort_by_key(|session| {
        session_rows
            .iter()
            .find(|row| row.id == session.id.to_string())
            .and_then(|row| row.updated_at.parse::<i64>().ok())
            .map(|timestamp| -timestamp)
            .unwrap_or(i64::MIN)
    });

    archived_sessions.sort_by_key(|session| {
        session_rows
            .iter()
            .find(|row| row.id == session.id.to_string())
            .and_then(|row| row.updated_at.parse::<i64>().ok())
            .map(|timestamp| -timestamp)
            .unwrap_or(i64::MIN)
    });

    let expanded_task_projects = task_projects
        .iter()
        .map(|project| project.id.clone())
        .collect::<BTreeSet<_>>();
    let selected_session = visible_sessions
        .first()
        .map(|session| session.id)
        .unwrap_or(0);
    let session_run_phases = visible_sessions
        .iter()
        .map(|session| (session.id, SessionRunPhase::Idle))
        .collect::<BTreeMap<_, _>>();

    Ok(WorkspaceSnapshot {
        task_projects,
        archived_task_projects,
        sessions: visible_sessions,
        archived_sessions,
        session_messages: transcript_map,
        archived_session_messages: archived_transcript_map,
        session_compactions: compaction_map,
        archived_session_compactions: archived_compaction_map,
        session_runtime_state: runtime_state_map,
        archived_session_runtime_state: archived_runtime_state_map,
        session_run_phases,
        expanded_task_projects,
        selected_session,
        next_session_id,
    })
}

/// Persists the current visible workspace snapshot back into the controller-backed SQLite database.
/// 将当前可见工作台快照回写到控制器承载的 SQLite 数据库。
pub async fn persist_workspace_snapshot(
    controller: ControllerRuntimeHandle,
    snapshot: &WorkspaceSnapshot,
) -> Result<(), String> {
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
    )
    .await?;
    replace_session_messages(
        &controller,
        &snapshot.sessions,
        &snapshot.session_messages,
        &snapshot.archived_sessions,
        &snapshot.archived_session_messages,
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
    )
    .await
}

/// Builds the default empty workspace snapshot used before any real project has been added.
/// 构建在尚未添加任何真实项目之前使用的默认空工作台快照。
fn build_empty_workspace_snapshot() -> WorkspaceSnapshot {
    WorkspaceSnapshot {
        task_projects: Vec::new(),
        archived_task_projects: Vec::new(),
        sessions: Vec::new(),
        archived_sessions: Vec::new(),
        session_messages: BTreeMap::new(),
        archived_session_messages: BTreeMap::new(),
        session_compactions: BTreeMap::new(),
        archived_session_compactions: BTreeMap::new(),
        session_runtime_state: BTreeMap::new(),
        archived_session_runtime_state: BTreeMap::new(),
        session_run_phases: BTreeMap::new(),
        expanded_task_projects: BTreeSet::new(),
        selected_session: 0,
        next_session_id: 1,
    }
}

/// Returns whether one restored snapshot still exactly matches the old prototype seed workspace.
/// 返回一份已恢复快照是否仍然与旧原型种子工作台完全一致。
fn is_legacy_seed_workspace(snapshot: &WorkspaceSnapshot) -> bool {
    if !snapshot.archived_task_projects.is_empty()
        || !snapshot.archived_sessions.is_empty()
        || !snapshot.archived_session_messages.is_empty()
        || !snapshot.archived_session_compactions.is_empty()
    {
        return false;
    }

    let seeded_projects = TaskProjectSummary::seeded();
    let seeded_sessions = SessionSummary::seeded();

    snapshot.task_projects.len() == seeded_projects.len()
        && snapshot.sessions.len() == seeded_sessions.len()
        && snapshot
            .task_projects
            .iter()
            .zip(seeded_projects.iter())
            .all(|(current, seeded)| {
                current.id == seeded.id
                    && current.title == seeded.title
                    && current.path == seeded.path
                    && current.session_ids == seeded.session_ids
            })
        && snapshot
            .sessions
            .iter()
            .zip(seeded_sessions.iter())
            .all(|(current, seeded)| {
                current.id == seeded.id
                    && current.project_id == seeded.project_id
                    && current.title == seeded.title
                    && current.title_source == seeded.title_source
            })
}

/// Clears all workspace business tables so the application can rebuild from a clean empty state.
/// 清空全部工作台业务表，以便应用从干净的空状态重新开始。
async fn clear_workspace_storage(controller: &ControllerRuntimeHandle) -> Result<(), String> {
    controller
        .client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
DELETE FROM session_runtime_state;
DELETE FROM session_compactions;
DELETE FROM session_messages;
DELETE FROM sessions;
DELETE FROM projects;
"#,
            Vec::<ControllerSqliteValue>::new(),
        )
        .await
        .map_err(|error| format!("清理旧原型工作台数据失败：{error}"))?;

    Ok(())
}

/// Returns the current controller-scoped SQLite binding identifier used by the application workspace tables.
/// 返回应用工作台表当前使用的控制器作用域 SQLite 绑定标识符。
fn app_sqlite_binding_id(controller: &ControllerRuntimeHandle) -> &str {
    controller.sqlite_binding_id.as_str()
}

/// Queries the number of visible project rows currently stored in the application database.
/// 查询当前应用数据库中已存储的可见项目行数量。
async fn query_project_count(controller: &ControllerRuntimeHandle) -> Result<u64, String> {
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
async fn query_project_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredProjectRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, name, root_path, pinned, sort_order, archived
FROM projects
ORDER BY pinned DESC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台项目列表失败：{error}"))?;

    decode_query_rows::<StoredProjectRow>(&result.json_data)
}

/// Queries active project identifiers used for soft-archive reconciliation.
/// 查询用于软归档对账的激活项目标识集合。
async fn query_active_project_ids(
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
async fn query_session_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredSessionRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT id, project_id, title, title_source, sort_order, archived, updated_at
FROM sessions
ORDER BY project_id ASC, sort_order ASC, updated_at DESC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询工作台会话列表失败：{error}"))?;

    decode_query_rows::<StoredSessionRow>(&result.json_data)
}

/// Queries active session identifiers used for soft-archive reconciliation.
/// 查询用于软归档对账的激活会话标识集合。
async fn query_active_session_ids(
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
            "SELECT id FROM sessions WHERE archived = 0;",
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询激活会话标识失败：{error}"))?;

    Ok(decode_query_rows::<IdRow>(&result.json_data)?
        .into_iter()
        .map(|row| row.id)
        .collect())
}

/// Queries all transcript message rows, including hidden summary and compaction nodes, for both visible and archived sessions.
/// 查询可见与归档会话的全部转录消息行，包含隐藏摘要与压缩节点。
async fn query_session_message_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredSessionMessageRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT m.id, m.session_id, m.ordinal, m.role, m.kind, m.visible, m.content, m.meta_json, m.created_at
FROM session_messages AS m
INNER JOIN sessions AS s ON s.id = m.session_id
ORDER BY m.session_id ASC, m.ordinal ASC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询会话转录消息失败：{error}"))?;

    decode_query_rows::<StoredSessionMessageRow>(&result.json_data)
}

/// Queries hidden compaction checkpoint rows for both visible and archived sessions.
/// 查询可见与归档会话对应的隐藏压缩检查点行。
async fn query_session_compaction_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredCompactionRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT c.id, c.session_id, c.source_until_message_id, c.context_start_message_id, c.summary_message_id, c.created_at
FROM session_compactions AS c
INNER JOIN sessions AS s ON s.id = c.session_id
ORDER BY c.session_id ASC, c.created_at ASC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询会话压缩节点失败：{error}"))?;

    decode_query_rows::<StoredCompactionRow>(&result.json_data)
}

/// Queries runtime-state rows for both visible and archived sessions.
/// 查询可见与归档会话对应的运行时状态行。
async fn query_session_runtime_state_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<StoredRuntimeStateRow>, String> {
    let result = controller
        .client
        .query_sqlite_json_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
SELECT r.session_id, r.active_agent_id, r.active_provider_id, r.active_model_id, r.last_compaction_id, r.context_start_message_id, r.ui_state_json, r.updated_at
FROM session_runtime_state AS r
INNER JOIN sessions AS s ON s.id = r.session_id
ORDER BY r.session_id ASC;
"#,
            Vec::new(),
        )
        .await
        .map_err(|error| format!("查询会话运行时状态失败：{error}"))?;

    decode_query_rows::<StoredRuntimeStateRow>(&result.json_data)
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
        .collect::<BTreeMap<usize, (String, usize)>>();
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
        .into_iter()
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

    Ok(())
}

/// Replaces all transcript message rows for the currently visible sessions while leaving archived sessions intact.
/// 在保留已归档会话数据的前提下替换当前可见会话的全部转录消息行。
async fn replace_session_messages(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<usize, Vec<SessionMessageEntry>>,
    archived_sessions: &[SessionSummary],
    archived_session_messages: &BTreeMap<usize, Vec<SessionMessageEntry>>,
) -> Result<(), String> {
    let active_session_ids = sessions
        .iter()
        .chain(archived_sessions.iter())
        .map(|session| session.id.to_string())
        .collect::<Vec<_>>();

    delete_session_scoped_rows(
        controller,
        "session_messages",
        &active_session_ids,
        "删除当前可见会话的旧转录消息失败",
    )
    .await?;

    let mut batch_params = Vec::new();
    push_session_message_batch_rows(&mut batch_params, sessions, session_messages);
    push_session_message_batch_rows(
        &mut batch_params,
        archived_sessions,
        archived_session_messages,
    );

    if batch_params.is_empty() {
        return Ok(());
    }

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO session_messages (
    id,
    session_id,
    ordinal,
    role,
    kind,
    visible,
    content,
    meta_json,
    created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9);
"#,
            batch_params,
        )
        .await
        .map_err(|error| format!("写入会话转录消息失败：{error}"))?;

    Ok(())
}

/// Replaces all compaction checkpoint rows for the currently visible sessions while leaving archived sessions intact.
/// 在保留已归档会话数据的前提下替换当前可见会话的全部压缩检查点行。
async fn replace_session_compactions(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_compactions: &BTreeMap<usize, Vec<SessionCompactionEntry>>,
    archived_sessions: &[SessionSummary],
    archived_session_compactions: &BTreeMap<usize, Vec<SessionCompactionEntry>>,
) -> Result<(), String> {
    let active_session_ids = sessions
        .iter()
        .chain(archived_sessions.iter())
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

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO session_compactions (
    id,
    session_id,
    source_until_message_id,
    context_start_message_id,
    summary_message_id,
    created_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6);
"#,
            batch_params,
        )
        .await
        .map_err(|error| format!("写入会话压缩节点失败：{error}"))?;

    Ok(())
}

/// Replaces all runtime-state rows for the currently visible sessions while leaving archived sessions intact.
/// 在保留已归档会话数据的前提下替换当前可见会话的全部运行时状态行。
async fn replace_session_runtime_state(
    controller: &ControllerRuntimeHandle,
    sessions: &[SessionSummary],
    session_runtime_state: &BTreeMap<usize, SessionRuntimeStateEntry>,
    archived_sessions: &[SessionSummary],
    archived_session_runtime_state: &BTreeMap<usize, SessionRuntimeStateEntry>,
) -> Result<(), String> {
    let active_session_ids = sessions
        .iter()
        .chain(archived_sessions.iter())
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

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            r#"
INSERT INTO session_runtime_state (
    session_id,
    active_agent_id,
    active_provider_id,
    active_model_id,
    last_compaction_id,
    context_start_message_id,
    ui_state_json,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);
"#,
            batch_params,
        )
        .await
        .map_err(|error| format!("写入会话运行时状态失败：{error}"))?;

    Ok(())
}

/// Pushes transcript-message batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的转录消息批量行推入共享 SQLite 批处理载荷。
fn push_session_message_batch_rows(
    batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    sessions: &[SessionSummary],
    session_messages: &BTreeMap<usize, Vec<SessionMessageEntry>>,
) {
    for session in sessions {
        if let Some(messages) = session_messages.get(&session.id) {
            for (ordinal, message) in messages.iter().enumerate() {
                let meta_json = serde_json::json!({
                    "title": message.title,
                })
                .to_string();

                batch_params.push(vec![
                    ControllerSqliteValue::String(if message.id.trim().is_empty() {
                        format!("session-{}-message-{}", session.id, ordinal)
                    } else {
                        message.id.clone()
                    }),
                    ControllerSqliteValue::String(session.id.to_string()),
                    ControllerSqliteValue::Int64(ordinal as i64),
                    ControllerSqliteValue::String(role_from_session_kind(message.kind).to_string()),
                    ControllerSqliteValue::String(
                        message.record_kind.as_persisted_key().to_string(),
                    ),
                    ControllerSqliteValue::Int64(i64::from(message.visible)),
                    ControllerSqliteValue::String(message.body.clone()),
                    ControllerSqliteValue::String(meta_json),
                    ControllerSqliteValue::String(non_empty_timestamp_text(&message.created_at)),
                ]);
            }
        }
    }
}

/// Pushes compaction-checkpoint batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的压缩检查点批量行推入共享 SQLite 批处理载荷。
fn push_session_compaction_batch_rows(
    batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    sessions: &[SessionSummary],
    session_compactions: &BTreeMap<usize, Vec<SessionCompactionEntry>>,
) {
    for session in sessions {
        if let Some(compactions) = session_compactions.get(&session.id) {
            for entry in compactions {
                batch_params.push(vec![
                    ControllerSqliteValue::String(entry.id.clone()),
                    ControllerSqliteValue::String(session.id.to_string()),
                    optional_sqlite_string(&entry.source_until_message_id),
                    optional_sqlite_string(&entry.context_start_message_id),
                    optional_sqlite_string(&entry.summary_message_id),
                    ControllerSqliteValue::String(non_empty_timestamp_text(&entry.created_at)),
                ]);
            }
        }
    }
}

/// Pushes runtime-state batch rows for one session slice into the shared SQLite batch payload.
/// 将某个会话切片的运行时状态批量行推入共享 SQLite 批处理载荷。
fn push_session_runtime_state_batch_rows(
    batch_params: &mut Vec<Vec<ControllerSqliteValue>>,
    sessions: &[SessionSummary],
    session_runtime_state: &BTreeMap<usize, SessionRuntimeStateEntry>,
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

    controller
        .client
        .execute_sqlite_batch_typed(
            APP_SPACE_ID,
            app_sqlite_binding_id(controller),
            &statement,
            params,
        )
        .await
        .map_err(|error| format!("{error_prefix}：{error}"))?;

    Ok(())
}

/// Decodes one controller JSON row-set payload into typed Rust rows.
/// 将一条控制器 JSON 行集载荷解码为带类型的 Rust 行集合。
fn decode_query_rows<T>(json_data: &str) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_str::<Vec<T>>(json_data)
        .map_err(|error| format!("解析控制器 SQLite 查询结果失败：{error}"))
}

/// Parses one persisted session identifier back into the in-memory numeric session key.
/// 将一条持久化会话标识解析回内存中的数值型会话键。
fn parse_session_id(value: &str) -> Result<usize, String> {
    value.parse::<usize>().map_err(|error| {
        format!("解析持久化会话标识 `{value}` 失败，当前实现要求数值型会话标识：{error}")
    })
}

/// Converts one persisted message-role string back into the session message kind.
/// 将一条持久化消息角色字符串转换回会话消息类型。
fn session_kind_from_role(role: &str) -> SessionMessageKind {
    match role {
        "user" => SessionMessageKind::User,
        "assistant" => SessionMessageKind::Assistant,
        "error" => SessionMessageKind::Error,
        _ => SessionMessageKind::Assistant,
    }
}

/// Returns the persisted text role used for one session message kind.
/// 返回某个会话消息类型使用的持久化文本角色。
fn role_from_session_kind(kind: SessionMessageKind) -> &'static str {
    match kind {
        SessionMessageKind::User => "user",
        SessionMessageKind::Assistant => "assistant",
        SessionMessageKind::Error => "error",
    }
}

/// Parses the optional message title from one persisted metadata JSON payload.
/// 从一条持久化元数据 JSON 载荷中解析可选的消息标题。
fn parse_message_title_from_meta(meta_json: &str) -> Option<String> {
    let value = serde_json::from_str::<Value>(meta_json).ok()?;
    value
        .get("title")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Parses one persisted transcript UI-state JSON payload into the typed runtime-state structure.
/// 将一条持久化转录界面状态 JSON 载荷解析为带类型的运行时状态结构。
fn parse_session_ui_state_from_json(value: &str) -> Option<SessionTranscriptUiState> {
    serde_json::from_str(value).ok()
}

/// Serializes one typed transcript UI-state structure into the persisted JSON payload when needed.
/// 在需要时将一条带类型的转录界面状态结构序列化为持久化 JSON 载荷。
fn serialize_session_ui_state_to_json(state: &SessionTranscriptUiState) -> Option<String> {
    if state.collapsed_message_ids.is_empty() {
        None
    } else {
        serde_json::to_string(state).ok()
    }
}

/// Returns one default message title when no explicit title is stored in the metadata payload.
/// 当元数据载荷中未存储显式标题时返回默认消息标题。
fn default_message_title(role: &str) -> &'static str {
    match role {
        "user" => "用户指令",
        "assistant" => "AI 输出",
        "error" => "执行失败",
        _ => "运行消息",
    }
}

/// Builds one short focus preview from the latest visible transcript row.
/// 根据最近一条可见转录行构建一段简短聚焦预览。
fn session_focus_preview(messages: &[SessionMessageEntry]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.visible)
        .map(|message| truncate_preview_lines(&message.body, 1, 36))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| String::from("等待新的上下文与执行结果"))
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn current_unix_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as i64
}

/// Returns the current Unix timestamp in milliseconds as one text value.
/// 以文本值形式返回当前 Unix 毫秒时间戳。
fn current_unix_millis_text() -> String {
    current_unix_millis().to_string()
}

/// Returns the provided timestamp text or falls back to the current time when it is empty.
/// 返回给定时间戳文本，若为空则回退到当前时间。
fn non_empty_timestamp_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        current_unix_millis_text()
    } else {
        trimmed.to_string()
    }
}

/// Converts one optional string into the nullable SQLite value expected by the controller SDK.
/// 将一个可选字符串转换为控制器 SDK 期望的可空 SQLite 值。
fn optional_sqlite_string(value: &Option<String>) -> ControllerSqliteValue {
    value
        .as_ref()
        .map(|value| ControllerSqliteValue::String(value.clone()))
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Builds one relative-time label from one persisted Unix-milliseconds text value.
/// 根据一条持久化 Unix 毫秒文本值构建相对时间标签。
fn relative_time_label_from_unix_millis_text(value: &str) -> String {
    let parsed = value
        .parse::<i64>()
        .unwrap_or_else(|_| current_unix_millis());
    relative_time_label_from_unix_millis(parsed)
}

/// Builds one relative-time label from one Unix-milliseconds timestamp.
/// 根据一条 Unix 毫秒时间戳构建相对时间标签。
fn relative_time_label_from_unix_millis(timestamp_millis: i64) -> String {
    let now = current_unix_millis();
    let delta_millis = now.saturating_sub(timestamp_millis).max(0);
    let delta_secs = delta_millis / 1_000;

    if delta_secs < 60 {
        return String::from("刚刚");
    }

    let delta_minutes = delta_secs / 60;
    if delta_minutes < 60 {
        return format!("{delta_minutes}分钟前");
    }

    let delta_hours = delta_minutes / 60;
    if delta_hours < 24 {
        return format!("{delta_hours}小时前");
    }

    let delta_days = delta_hours / 24;
    if delta_days < 7 {
        return format!("{delta_days}天前");
    }

    if delta_days < 30 {
        return format!("{}周前", (delta_days / 7).max(1));
    }

    format!("{}月前", (delta_days / 30).max(1))
}

/// Approximates one Unix-milliseconds timestamp from the compact relative label stored by the UI.
/// 根据界面存储的紧凑相对标签近似还原一条 Unix 毫秒时间戳。
fn approximate_unix_millis_from_relative_label(label: &str) -> Option<i64> {
    if label.trim().is_empty() {
        return None;
    }

    let now = current_unix_millis();
    if label == "刚刚" {
        return Some(now);
    }

    let number_text = label
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let amount = number_text.parse::<i64>().ok()?;

    let millis = if label.contains("分钟前") {
        amount * 60 * 1_000
    } else if label.contains("小时前") {
        amount * 60 * 60 * 1_000
    } else if label.contains("天前") {
        amount * 24 * 60 * 60 * 1_000
    } else if label.contains("周前") {
        amount * 7 * 24 * 60 * 60 * 1_000
    } else if label.contains("月前") {
        amount * 30 * 24 * 60 * 60 * 1_000
    } else {
        0
    };

    Some(now.saturating_sub(millis))
}

/// Truncates one multi-line preview into a single compact snippet used by persistence-derived summaries.
/// 将一段多行预览截断为单条紧凑片段，供基于持久化的摘要使用。
fn truncate_preview_lines(value: &str, max_lines: usize, max_chars: usize) -> String {
    let joined = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .take(max_lines)
        .collect::<Vec<_>>()
        .join(" / ");
    let normalized = if joined.is_empty() {
        value.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        joined
    };

    if normalized.chars().count() <= max_chars {
        normalized
    } else {
        let truncated = normalized
            .chars()
            .take(max_chars.saturating_sub(1))
            .collect::<String>();
        format!("{truncated}…")
    }
}
