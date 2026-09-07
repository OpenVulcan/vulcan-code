use super::*;

const DEFAULT_WORKSPACE_PROJECT_NAME: &str = "VulcanProject";
const DEFAULT_WORKSPACE_PROJECT_DIRECTORY: &str = "vulcan_project";
const DEFAULT_WORKSPACE_PROJECT_ID_PREFIX: &str = "vulcanproject";

/// Loads or seeds the full workspace snapshot used by runtime startup.
/// 加载或初始化运行时启动使用的完整工作区快照。
///
/// # Parameters
/// 参数
/// `controller` provides access to the runtime data paths and shared controller services.
/// `controller` 提供运行时数据路径与共享控制器服务访问能力。
///
/// # Returns
/// 返回值
/// Returns the hydrated workspace snapshot, or a descriptive storage error.
/// 返回已水合的工作区快照，或描述性的存储错误。
pub async fn load_or_seed_workspace(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_snapshot(controller).await
}

/// Loads the persisted workspace index without eagerly hydrating every session runtime state.
/// 加载持久化工作区索引，但不急切水合每个会话运行态。
pub async fn load_or_seed_workspace_index(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_index_snapshot(controller).await
}

/// Loads the current persisted workspace snapshot from the project rows and V1 session index.
/// 从项目记录与 V1 会话索引加载当前持久化工作台快照。
pub async fn load_workspace_snapshot(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_snapshot_with_runtime_state(controller, true).await
}

/// Loads the current persisted workspace index without full per-session runtime state.
/// 加载当前持久化工作区索引，不包含每个会话的完整运行态。
pub async fn load_workspace_index_snapshot(
    controller: ControllerRuntimeHandle,
) -> Result<WorkspaceSnapshot, String> {
    load_workspace_snapshot_with_runtime_state(controller, false).await
}

/// Validated project row used by deterministic workspace loading and default-project decisions.
/// 确定性工作区加载与默认项目决策使用的已验证项目行。
#[derive(Debug)]
struct ValidatedProjectRow {
    /// Original decoded SQLite row retained for project projection.
    /// 为项目投影保留的原始 SQLite 解码行。
    stored: StoredProjectRow,
    /// Exact pinned state decoded from the numeric protocol flag.
    /// 从数值协议标记解码出的精确置顶状态。
    pinned: bool,
    /// Exact Codebase Memory enablement decoded from the numeric protocol flag.
    /// 从数值协议标记解码出的精确 Codebase Memory 启用状态。
    codebase_memory_enabled: bool,
    /// Validated project-owned native code-semantic settings.
    /// 经过校验的项目自有原生代码语义设置。
    vce: ProjectVceSettings,
    /// Exact archive state decoded from the numeric protocol flag.
    /// 从数值协议标记解码出的精确归档状态。
    archived: bool,
    /// Non-negative section order converted without clamping or truncation.
    /// 未经钳制或截断转换的非负分区顺序。
    sort_order: usize,
}

/// Validates one raw SQLite project row before it can trigger writes or enter workspace projection.
/// 在原始 SQLite 项目行触发写入或进入工作区投影前对其进行验证。
///
/// # Parameters
/// 参数
/// `row` is the complete project row decoded from the Controller response.
/// `row` 是从 Controller 响应解码出的完整项目行。
///
/// # Returns
/// 返回值
/// Returns a validated row or an explicit identity, path, flag, or order corruption error.
/// 返回已验证行；身份、路径、标记或顺序损坏时返回显式错误。
fn validate_project_row(row: StoredProjectRow) -> Result<ValidatedProjectRow, String> {
    if row.id.trim().is_empty() {
        return Err(String::from("项目索引包含空白项目标识。"));
    }
    if row.name.trim().is_empty() {
        return Err(format!("项目索引 `{}` 包含空白展示名称。", row.id));
    }
    if row.root_path.trim().is_empty() {
        return Err(format!("项目索引 `{}` 缺少根目录路径。", row.id));
    }
    if !std::path::Path::new(&row.root_path).is_absolute() {
        return Err(format!(
            "项目索引 `{}` 的根目录不是绝对路径：`{}`。",
            row.id, row.root_path
        ));
    }
    // pinned accepts only the two values written by the project persistence protocol.
    // pinned 只接受项目持久化协议写入的两个值。
    let pinned = match row.pinned {
        0 => false,
        1 => true,
        value => {
            return Err(format!(
                "项目索引 `{}` 包含无效置顶标记 `{value}`。",
                row.id
            ));
        }
    };
    // codebase_memory_enabled accepts only values produced by the project persistence protocol.
    // codebase_memory_enabled 只接受项目持久化协议产生的值。
    let codebase_memory_enabled = match row.codebase_memory_enabled {
        0 => false,
        1 => true,
        value => {
            return Err(format!(
                "项目索引 `{}` 包含无效 Codebase Memory 启用标记 `{value}`。",
                row.id
            ));
        }
    };
    let vce_enabled = validate_project_boolean(&row.id, "VCE 启用", row.vce_enabled)?;
    let vce_migrated_from_codebase_memory = validate_project_boolean(
        &row.id,
        "VCE CBM 迁移",
        row.vce_migrated_from_codebase_memory,
    )?;
    let vce_revision = u64::try_from(row.vce_revision).map_err(|error| {
        format!(
            "项目索引 `{}` 包含无效 VCE 修订号 `{}`：{error}",
            row.id, row.vce_revision
        )
    })?;
    if vce_revision == 0 {
        return Err(format!("项目索引 `{}` 的 VCE 修订号必须大于零。", row.id));
    }
    let vce_max_file_bytes =
        positive_project_u64(&row.id, "VCE 最大文件字节数", row.vce_max_file_bytes)?;
    let vce_embedding_max_concurrency =
        positive_project_usize(&row.id, "VCE 向量并发数", row.vce_embedding_max_concurrency)?;
    let vce_query_max_concurrency =
        positive_project_usize(&row.id, "VCE 查询并发数", row.vce_query_max_concurrency)?;
    let vce_max_results = positive_project_usize(&row.id, "VCE 结果数量", row.vce_max_results)?;
    let vce_max_context_chars =
        positive_project_usize(&row.id, "VCE 上下文字符数", row.vce_max_context_chars)?;
    let vce_extra_ignore_patterns =
        serde_json::from_str::<Vec<String>>(&row.vce_extra_ignore_patterns_json)
            .map_err(|error| format!("项目索引 `{}` 的 VCE 忽略规则 JSON 无效：{error}", row.id))?;
    let vce = ProjectVceSettings {
        enabled: vce_enabled,
        update_mode: match row.vce_update_mode.as_deref() {
            None | Some("manual") => vulcan_workbench_model::ProjectVceUpdateMode::Manual,
            Some("automatic") => vulcan_workbench_model::ProjectVceUpdateMode::Automatic,
            Some(invalid) => {
                return Err(format!(
                    "项目索引 `{}` 的语义更新模式无效：`{invalid}`。",
                    row.id
                ));
            }
        },
        auto_update_min_interval_seconds: match row.vce_auto_update_min_interval_seconds {
            Some(seconds) => u32::try_from(seconds)
                .map_err(|error| format!("项目索引 `{}` 的语义更新间隔无效：{error}", row.id))?,
            None => vulcan_workbench_model::default_vce_auto_update_min_interval_seconds(),
        },
        revision: vce_revision,
        migrated_from_codebase_memory: vce_migrated_from_codebase_memory,
        max_file_bytes: vce_max_file_bytes,
        extra_ignore_patterns: vce_extra_ignore_patterns,
        allow_query_rewrite: validate_project_boolean(
            &row.id,
            "VCE 查询改写",
            row.vce_allow_query_rewrite,
        )?,
        allow_rerank: validate_project_boolean(&row.id, "VCE 重排", row.vce_allow_rerank)?,
        embedding_max_concurrency: vce_embedding_max_concurrency,
        query_max_concurrency: vce_query_max_concurrency,
        max_results: vce_max_results,
        max_context_chars: vce_max_context_chars,
    };
    vce.validate()
        .map_err(|error| format!("项目索引 `{}` 的 VCE 设置无效：{error}", row.id))?;
    // archived must be exact before it can decide whether default-project seeding writes data.
    // archived 在决定默认项目播种是否写入数据前必须保持精确。
    let archived = match row.archived {
        0 => false,
        1 => true,
        value => {
            return Err(format!(
                "项目索引 `{}` 包含无效归档标记 `{value}`。",
                row.id
            ));
        }
    };
    // sort_order rejects negative or platform-overflowing values instead of projecting order zero.
    // sort_order 拒绝负值或平台溢出值，而不是投影为顺序零。
    let sort_order = usize::try_from(row.sort_order).map_err(|error| {
        format!(
            "项目索引 `{}` 包含无效分区顺序 `{}`：{error}",
            row.id, row.sort_order
        )
    })?;
    Ok(ValidatedProjectRow {
        stored: row,
        pinned,
        codebase_memory_enabled,
        vce,
        archived,
        sort_order,
    })
}

/// Decodes one strict SQLite 0/1 project flag.
/// 解码一个严格的 SQLite 0/1 项目标记。
///
/// # Parameters
/// 参数
/// `project_id` and `field_name` identify diagnostics; `value` is the raw SQLite integer.
/// `project_id` 与 `field_name` 标识诊断；`value` 是原始 SQLite 整数。
///
/// # Returns
/// 返回值
/// The exact boolean value or an explicit corruption error.
/// 返回精确布尔值或明确的损坏错误。
fn validate_project_boolean(
    project_id: &str,
    field_name: &str,
    value: i64,
) -> Result<bool, String> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(format!(
            "项目索引 `{project_id}` 包含无效 {field_name} 标记 `{value}`。"
        )),
    }
}

/// Converts one positive SQLite integer into a project-owned `u64` setting.
/// 将一个正数 SQLite 整数转换为项目自有 `u64` 设置。
fn positive_project_u64(project_id: &str, field_name: &str, value: i64) -> Result<u64, String> {
    let value = u64::try_from(value).map_err(|error| {
        format!("项目索引 `{project_id}` 包含无效 {field_name} `{value}`：{error}")
    })?;
    if value == 0 {
        return Err(format!(
            "项目索引 `{project_id}` 的 {field_name} 必须大于零。"
        ));
    }
    Ok(value)
}

/// Converts one positive SQLite integer into a platform-sized project setting.
/// 将一个正数 SQLite 整数转换为平台宽度的项目设置。
fn positive_project_usize(project_id: &str, field_name: &str, value: i64) -> Result<usize, String> {
    let value = usize::try_from(value).map_err(|error| {
        format!("项目索引 `{project_id}` 包含无效 {field_name} `{value}`：{error}")
    })?;
    if value == 0 {
        return Err(format!(
            "项目索引 `{project_id}` 的 {field_name} 必须大于零。"
        ));
    }
    Ok(value)
}

/// Queries and validates every project row before any workspace-level decision uses it.
/// 在任何工作区级决策使用项目行前查询并验证全部项目行。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative project SQLite binding.
/// `controller` 持有权威项目 SQLite 绑定。
///
/// # Returns
/// 返回值
/// Returns all validated rows in query order or the first explicit corruption/query error.
/// 按查询顺序返回全部已验证行，或返回首个显式损坏/查询错误。
async fn query_validated_project_rows(
    controller: &ControllerRuntimeHandle,
) -> Result<Vec<ValidatedProjectRow>, String> {
    query_project_rows(controller)
        .await?
        .into_iter()
        .map(validate_project_row)
        .collect()
}

/// Returns whether validated project state requires creating or restoring the built-in default.
/// 返回已验证项目状态是否需要创建或恢复内置默认项目。
///
/// # Parameters
/// 参数
/// `project_rows` contains every validated active and archived project row.
/// `project_rows` 包含全部已验证活动与归档项目行。
///
/// # Returns
/// 返回值
/// Returns `true` only when no active project row exists.
/// 仅在不存在活动项目行时返回 `true`。
fn workspace_requires_default_project(project_rows: &[ValidatedProjectRow]) -> bool {
    project_rows.iter().all(|row| row.archived)
}

/// Resolves the retained project row that authorizes one session to enter workspace projection.
/// 解析授权单个会话进入工作区投影的保留项目行。
///
/// # Parameters
/// 参数
/// `project_rows` contains every project row still retained by the project lifecycle protocol.
/// `project_rows` 包含项目生命周期协议仍保留的全部项目行。
/// `project_id` is the exact owner persisted on the session locator.
/// `project_id` 是会话定位行持久化的精确所属项目标识。
///
/// # Returns
/// 返回值
/// Returns the retained owner, or `None` when project removal intentionally tombstoned the owner row.
/// 返回保留的所属项目；项目移除已按协议删除所属行时返回 `None`。
fn retained_project_row_for_session<'a>(
    project_rows: &'a [ValidatedProjectRow],
    project_id: &str,
) -> Option<&'a ValidatedProjectRow> {
    project_rows
        .iter()
        .find(|candidate| candidate.stored.id == project_id)
}

/// Builds an overflow-safe descending update-time key for one session identity.
/// 为单个会话身份构建无溢出的更新时间降序键。
///
/// # Parameters
/// 参数
/// `updated_at_by_session` maps loaded session identities to strictly parsed timestamps.
/// `updated_at_by_session` 将已加载会话身份映射到严格解析时间戳。
/// `session_id` is the session identity whose ordering key is requested.
/// `session_id` 是需要排序键的会话身份。
///
/// # Returns
/// 返回值
/// Returns a reverse-order key without negating signed timestamps; absent identities use zero.
/// 返回不对有符号时间戳取负的逆序键；缺失身份使用零。
fn descending_session_updated_at_key(
    updated_at_by_session: &BTreeMap<SessionId, i64>,
    session_id: &SessionId,
) -> std::cmp::Reverse<i64> {
    std::cmp::Reverse(
        updated_at_by_session
            .get(session_id)
            .copied()
            .unwrap_or_default(),
    )
}

/// Loads the current persisted workspace snapshot with an optional runtime-state pass.
/// 按可选运行态加载过程读取当前持久化工作区快照。
async fn load_workspace_snapshot_with_runtime_state(
    controller: ControllerRuntimeHandle,
    _load_runtime_state: bool,
) -> Result<WorkspaceSnapshot, String> {
    // project_rows are validated before their archive flags are allowed to trigger default seeding.
    // project_rows 在其归档标记获准触发默认播种前完成验证。
    let mut project_rows = query_validated_project_rows(&controller).await?;
    if workspace_requires_default_project(&project_rows) {
        seed_default_workspace_project(&controller).await?;
        project_rows = query_validated_project_rows(&controller).await?;
        if workspace_requires_default_project(&project_rows) {
            return Err(String::from("默认项目播种完成后仍不存在活动项目。"));
        }
    }
    // Session-index bootstrap is non-essential to the desktop shell: an old profile without the
    // V1 table must start with an empty V1 index instead of failing the whole application.
    // 会话索引初始化并不是桌面壳启动的必要条件：缺少 V1 表的旧配置必须以空 V1 索引启动，不能让整个应用失败。
    let session_index_available = match ensure_session_v1_storage_schema(&controller).await {
        Ok(()) => true,
        Err(error) => {
            eprintln!(
                "[workspace-store][session-index-v1-ignored] schema bootstrap failed; starting without persisted sessions: {error}"
            );
            false
        }
    };
    let mut indexed_sessions = Vec::new();
    if session_index_available {
        for validated_project in &project_rows {
            let project_id = validated_project.stored.id.as_str();
            match list_active_sessions_v1(&controller, project_id).await {
                Ok(sessions) => {
                    indexed_sessions.extend(sessions.into_iter().map(|session| (session, false)))
                }
                Err(error) => eprintln!(
                    "[workspace-store][session-index-v1-ignored] active list failed for project {project_id}; ignoring only that session list: {error}"
                ),
            }
            match list_archived_sessions_v1(&controller, project_id).await {
                Ok(sessions) => {
                    indexed_sessions.extend(sessions.into_iter().map(|session| (session, true)))
                }
                Err(error) => eprintln!(
                    "[workspace-store][session-index-v1-ignored] archived list failed for project {project_id}; ignoring only that session list: {error}"
                ),
            }
        }
    }

    let mut task_projects = Vec::new();
    let mut archived_task_projects = Vec::new();
    for validated_row in &project_rows {
        // row retains project display and path fields after scalar protocol validation succeeds.
        // row 在标量协议验证成功后保留项目展示与路径字段。
        let row = &validated_row.stored;
        // project is built only from validated boolean and non-negative order fields.
        // project 只从已验证布尔值与非负顺序字段构建。
        let project = TaskProjectSummary::new(
            row.id.clone(),
            row.name.clone(),
            row.root_path.clone(),
            validated_row.pinned,
            validated_row.sort_order,
            row.icon_pack.clone(),
            row.icon_key.clone(),
            Vec::new(),
        )
        .with_codebase_memory_enabled(validated_row.codebase_memory_enabled)
        .with_vce_settings(validated_row.vce.clone());

        if validated_row.archived {
            archived_task_projects.push(project);
        } else {
            task_projects.push(project);
        }
    }

    let runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let archived_runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let mut visible_sessions = Vec::new();
    let mut archived_sessions = Vec::new();
    let mut session_updated_at = BTreeMap::new();
    for (row, archived) in indexed_sessions {
        // session_id is the strictly parsed V1 identity shared by project ownership and runtime maps.
        // session_id 是项目归属与运行态映射共享的严格 V1 身份。
        let session_id = match row.session_id.parse::<SessionId>() {
            Ok(session_id) => session_id,
            Err(error) => {
                eprintln!(
                    "[workspace-store][session-index-v1-ignored] invalid session_id {}; ignoring that session: {error}",
                    row.session_id
                );
                continue;
            }
        };
        let project_id = row.project_id.as_str();
        // project_row proves the V1 locator does not reference a missing project record.
        // project_row 证明 V1 定位信息没有引用缺失的项目记录。
        let Some(project_row) = retained_project_row_for_session(&project_rows, project_id) else {
            continue;
        };
        if !archived && project_row.archived {
            eprintln!(
                "[workspace-store][session-index-v1-ignored] active session {session_id} belongs to archived project {project_id}; ignoring that session"
            );
            continue;
        }
        // project_name is the authoritative persisted display name of the existing project row.
        // project_name 是现有项目行的权威持久化展示名称。
        let project_name = project_row.stored.name.as_str();
        if archived {
            ensure_project_summary_for_session(
                &mut archived_task_projects,
                project_id,
                project_name,
                session_id,
            );
        } else {
            ensure_project_summary_for_session(
                &mut task_projects,
                project_id,
                project_name,
                session_id,
            );
        }
        let session_summary = SessionSummary::new(
            session_id,
            Some(project_id.to_string()),
            row.title,
            String::new(),
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis(row.updated_at_ms),
        )
        .with_title_source(SessionTitleSource::Manual)
        .with_created_at(row.created_at_ms.to_string())
        .with_updated_at(row.updated_at_ms.to_string());
        session_updated_at.insert(session_id, row.updated_at_ms);

        if archived {
            archived_sessions.push(session_summary);
        } else {
            visible_sessions.push(session_summary);
        }
    }

    for project in &mut task_projects {
        project.session_ids.sort_by_key(|session_id| {
            (
                descending_session_updated_at_key(&session_updated_at, session_id),
                *session_id,
            )
        });
    }

    for project in &mut archived_task_projects {
        project.session_ids.sort_by_key(|session_id| {
            (
                descending_session_updated_at_key(&session_updated_at, session_id),
                *session_id,
            )
        });
    }

    visible_sessions.sort_by_key(|session| {
        (
            descending_session_updated_at_key(&session_updated_at, &session.id),
            session.id,
        )
    });

    archived_sessions.sort_by_key(|session| {
        (
            descending_session_updated_at_key(&session_updated_at, &session.id),
            session.id,
        )
    });

    let collapsed_task_projects = query_collapsed_task_project_ids(&controller)
        .await?
        .unwrap_or_default();
    let expanded_task_projects = task_projects
        .iter()
        .filter(|project| !collapsed_task_projects.contains(&project.id))
        .map(|project| project.id.clone())
        .collect::<BTreeSet<_>>();
    let selected_session = visible_sessions
        .first()
        .map(|session| session.id)
        .unwrap_or_default();
    let session_run_phases = visible_sessions
        .iter()
        .map(|session| (session.id, SessionRunPhase::Idle))
        .collect::<BTreeMap<_, _>>();

    Ok(WorkspaceSnapshot {
        task_projects,
        archived_task_projects,
        sessions: visible_sessions,
        archived_sessions,
        session_runtime_state: runtime_state_map,
        archived_session_runtime_state: archived_runtime_state_map,
        session_run_phases,
        expanded_task_projects,
        selected_session,
        hydrated_session_ids: BTreeSet::new(),
    })
}

/// Seeds the persistent default project when the workspace has no project rows at all.
/// 当工作区完全没有项目记录时，持久化创建默认项目。
///
/// # Parameters
/// 参数
/// `controller` is the controller-backed persistence handle used by workspace loading.
/// `controller` 是工作区加载过程使用的控制器持久化句柄。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after the default directory and project row are ready.
/// 默认目录与项目记录准备完成后返回 `Ok(())`。
async fn seed_default_workspace_project(
    controller: &ControllerRuntimeHandle,
) -> Result<(), String> {
    let project_root = default_workspace_project_root()?;
    std::fs::create_dir_all(&project_root).map_err(|error| {
        format!(
            "创建默认项目目录 `{}` 失败：{error}",
            workspace_project_path_text_from_path(&project_root)
        )
    })?;
    // Canonicalization proves the newly created logical root resolves, while persisted identity remains path-stable.
    // 规范化证明新建逻辑根可解析，同时持久化身份保持路径稳定。
    std::fs::canonicalize(&project_root).map_err(|error| {
        format!(
            "规范化默认项目目录 `{}` 失败：{error}",
            workspace_project_path_text_from_path(&project_root)
        )
    })?;
    // project uses the logical default root shared by stable id generation and removal protection.
    // project 使用稳定标识生成与移除保护共享的逻辑默认根。
    let project =
        default_workspace_project_summary(workspace_project_path_text_from_path(&project_root));
    persist_new_project(controller.clone(), &project).await
}

/// Ensures the workspace keeps at least one active project by restoring the default project when needed.
/// 当工作区没有任何活动项目时恢复默认项目，确保至少保留一个活动项目。
///
/// # Parameters
/// 参数
/// `controller` provides access to the controller-backed project index.
/// `controller` 提供对控制器项目索引的访问能力。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after an active project exists or a storage error.
/// 活动项目存在后返回 `Ok(())`，否则返回存储错误。
pub async fn ensure_default_workspace_project(
    controller: ControllerRuntimeHandle,
) -> Result<(), String> {
    // project_rows are validated before archive state can suppress or trigger seeding.
    // project_rows 在归档状态抑制或触发播种前完成验证。
    let project_rows = query_validated_project_rows(&controller).await?;
    if !workspace_requires_default_project(&project_rows) {
        return Ok(());
    }
    seed_default_workspace_project(&controller).await?;
    // refreshed_rows prove the write actually published one active project.
    // refreshed_rows 证明写入实际发布了一个活动项目。
    let refreshed_rows = query_validated_project_rows(&controller).await?;
    if workspace_requires_default_project(&refreshed_rows) {
        return Err(String::from("默认项目播种完成后仍不存在活动项目。"));
    }
    Ok(())
}

/// Returns whether a task project is the built-in default workspace project.
/// 返回任务项目是否为内置默认工作区项目。
///
/// # Parameters
/// 参数
/// `project` is the persisted project summary to classify.
/// `project` 是需要分类的持久化项目摘要。
///
/// # Returns
/// 返回值
/// Returns `true` when the project path equals the canonical default project path.
/// 当项目路径等于标准默认项目路径时返回 `true`。
pub fn is_default_workspace_project(project: &TaskProjectSummary) -> Result<bool, String> {
    Ok(normalize_workspace_project_path_text(&project.path)
        == default_workspace_project_path_text()?)
}

/// Returns the normalized persisted path text for the default workspace project.
/// 返回默认工作区项目的规范化持久化路径文本。
///
/// # Returns
/// 返回值
/// Returns `<USER>/.vulcan/projects/vulcan_project` using the current host path style.
/// 使用当前宿主路径风格返回 `<USER>/.vulcan/projects/vulcan_project`。
pub fn default_workspace_project_path_text() -> Result<String, String> {
    Ok(workspace_project_path_text_from_path(
        &default_workspace_project_root()?,
    ))
}

/// Returns the default project root under the current user's Vulcan directory.
/// 返回当前用户 Vulcan 目录下的默认项目根目录。
///
/// # Returns
/// 返回值
/// Returns the default project path, or an error when the user home directory is unavailable.
/// 返回默认项目路径；当用户主目录不可用时返回错误。
fn default_workspace_project_root() -> Result<std::path::PathBuf, String> {
    dirs::home_dir()
        .map(|home| default_workspace_project_root_from_home(&home))
        .ok_or_else(|| String::from("无法解析用户主目录，无法创建默认项目。"))
}

/// Builds the default project root from an explicit user home path.
/// 根据明确的用户主目录构建默认项目根目录。
///
/// # Parameters
/// 参数
/// `home` is the user home directory that owns the profile-specific Vulcan data root.
/// `home` 是拥有按构建配置区分的 Vulcan 数据根目录的用户主目录。
///
/// # Returns
/// 返回值
/// Returns `<home>/<user-vulcan-root>/projects/vulcan_project`.
/// 返回 `<home>/<user-vulcan-root>/projects/vulcan_project`。
fn default_workspace_project_root_from_home(home: &std::path::Path) -> std::path::PathBuf {
    vulcan_runtime_assets::user_vulcan_root_for_home(home)
        .join("projects")
        .join(DEFAULT_WORKSPACE_PROJECT_DIRECTORY)
}

/// Builds the persistent project summary for the default workspace project.
/// 构建默认工作区项目的持久化项目摘要。
///
/// # Parameters
/// 参数
/// `root_path_text` is the normalized root path stored in the project index.
/// `root_path_text` 是写入项目索引的规范化根路径。
///
/// # Returns
/// 返回值
/// Returns the project summary inserted into the active project list.
/// 返回插入活动项目列表的项目摘要。
fn default_workspace_project_summary(root_path_text: String) -> TaskProjectSummary {
    TaskProjectSummary::new(
        default_workspace_project_id(&root_path_text),
        DEFAULT_WORKSPACE_PROJECT_NAME,
        root_path_text,
        false,
        0,
        None,
        None,
        Vec::new(),
    )
}

/// Builds a stable default-project id from the normalized root path.
/// 根据规范化根路径构建稳定的默认项目标识。
///
/// # Parameters
/// 参数
/// `root_path_text` is the normalized root path used as the hash source.
/// `root_path_text` 是作为哈希来源的规范化根路径。
///
/// # Returns
/// 返回值
/// Returns the stable `vulcanproject-<hash>` identifier.
/// 返回稳定的 `vulcanproject-<hash>` 标识。
fn default_workspace_project_id(root_path_text: &str) -> String {
    let mut hasher = DefaultHasher::new();
    root_path_text.hash(&mut hasher);
    let path_suffix = format!("{:x}", hasher.finish());
    let path_suffix = &path_suffix[..path_suffix.len().min(8)];
    format!("{DEFAULT_WORKSPACE_PROJECT_ID_PREFIX}-{path_suffix}")
}

/// Converts a filesystem path into the normalized project path text used by workspace persistence.
/// 将文件系统路径转换为工作区持久化使用的规范化项目路径文本。
///
/// # Parameters
/// 参数
/// `path` is the filesystem path to persist.
/// `path` 是要持久化的文件系统路径。
///
/// # Returns
/// 返回值
/// Returns normalized native path text with Windows verbatim prefixes removed.
/// 返回移除 Windows verbatim 前缀后的原生路径文本。
fn workspace_project_path_text_from_path(path: &std::path::Path) -> String {
    vulcan_runtime_assets::native_host_path_text_from_path(path)
}

/// Normalizes one raw project path string using the same native rules as project creation.
/// 使用与项目创建一致的原生规则规范化单个原始项目路径字符串。
///
/// # Parameters
/// 参数
/// `path` is raw path text produced by the host filesystem.
/// `path` 是宿主文件系统产出的原始路径文本。
///
/// # Returns
/// 返回值
/// Returns stable project path text for storage and duplicate checks.
/// 返回用于存储与重复检查的稳定项目路径文本。
fn normalize_workspace_project_path_text(path: &str) -> String {
    vulcan_runtime_assets::native_host_path_text(path)
}

/// Ensures a project summary exists and owns the provided session id exactly once.
/// 确保项目摘要存在，并且只拥有一次给定会话标识。
pub(super) fn ensure_project_summary_for_session(
    projects: &mut Vec<TaskProjectSummary>,
    project_id: &str,
    project_name: &str,
    session_id: SessionId,
) {
    if let Some(project) = projects.iter_mut().find(|project| project.id == project_id) {
        if !project.session_ids.contains(&session_id) {
            project.session_ids.push(session_id);
        }
        return;
    }
    projects.push(TaskProjectSummary::new(
        project_id.to_string(),
        project_name.to_string(),
        String::new(),
        false,
        projects.len(),
        None,
        None,
        vec![session_id],
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one canonical raw project row for strict loading tests.
    /// 为严格加载测试构建一条规范原始项目行。
    ///
    /// # Returns
    /// 返回值
    /// A complete active project row with valid identity, path, flags, and order.
    /// 返回身份、路径、标记与顺序均有效的完整活动项目行。
    fn stored_project_row_for_loading_test() -> StoredProjectRow {
        StoredProjectRow {
            id: String::from("project-load"),
            name: String::from("严格项目加载"),
            root_path: std::env::temp_dir()
                .join("vulcan-project-load")
                .to_string_lossy()
                .into_owned(),
            pinned: 0,
            sort_order: 0,
            icon_pack: None,
            icon_key: None,
            codebase_memory_enabled: 0,
            vce_enabled: 0,
            vce_revision: 1,
            vce_update_mode: None,
            vce_auto_update_min_interval_seconds: None,
            vce_migrated_from_codebase_memory: 1,
            vce_max_file_bytes: 1_048_576,
            vce_extra_ignore_patterns_json: String::from("[]"),
            vce_allow_query_rewrite: 0,
            vce_allow_rerank: 1,
            vce_embedding_max_concurrency: 4,
            vce_query_max_concurrency: 3,
            vce_max_results: 10,
            vce_max_context_chars: 32_000,
            archived: 0,
        }
    }

    /// Verify missing project update policy defaults without accepting malformed explicit values.
    /// 验证缺失的项目更新策略使用默认值，同时不接受格式错误的明确值。
    ///
    /// Takes no parameters and returns nothing; checks loading, isolation and bounds.
    /// 无参数、无返回值；检查加载、隔离与边界。
    #[test]
    fn project_vce_policy_loading_preserves_missing_and_explicit_choices() {
        // legacy has no policy columns populated by an earlier application.
        // legacy 的策略列尚未由旧应用填充。
        let legacy = stored_project_row_for_loading_test();
        // loaded receives in-memory defaults without modifying the stored input.
        // loaded 接收内存默认值而不修改存储输入。
        let loaded = validate_project_row(legacy.clone()).expect("legacy policy");
        assert_eq!(
            loaded.vce.update_mode,
            vulcan_workbench_model::ProjectVceUpdateMode::Manual
        );
        assert_eq!(loaded.vce.auto_update_min_interval_seconds, 300);
        assert!(legacy.vce_update_mode.is_none());
        // explicit selects automatic updates and an independently persisted interval.
        // explicit 选择自动更新及独立持久化间隔。
        let mut explicit = legacy.clone();
        explicit.vce_update_mode = Some(String::from("automatic"));
        explicit.vce_auto_update_min_interval_seconds = Some(600);
        // selected is the exact settings row after normal loading validation.
        // selected 是通过常规加载校验后的精确设置记录。
        let selected = validate_project_row(explicit).expect("explicit policy");
        assert_eq!(
            selected.vce.update_mode,
            vulcan_workbench_model::ProjectVceUpdateMode::Automatic
        );
        assert_eq!(selected.vce.auto_update_min_interval_seconds, 600);
        for interval in [-1, 0, 59, 86_401, i64::MAX] {
            // invalid tests explicit invalid values rather than treating them as absent.
            // invalid 测试明确无效值，不能将其当成缺省。
            let mut invalid = legacy.clone();
            invalid.vce_auto_update_min_interval_seconds = Some(interval);
            assert!(validate_project_row(invalid).is_err());
        }
        // invalid_mode must not be converted into manual or automatic by a fallback.
        // invalid_mode 不得被兜底转换为手动或自动。
        let mut invalid_mode = legacy;
        invalid_mode.vce_update_mode = Some(String::from("true"));
        assert!(validate_project_row(invalid_mode).is_err());
    }

    /// Verifies project validation rejects every scalar state that previously used coercion or clamping.
    /// 验证项目校验会拒绝此前使用强制转换或钳制处理的每种标量状态。
    #[test]
    fn project_row_validation_rejects_fallback_identity_path_flag_and_order_states() {
        // valid_row proves the canonical project protocol still decodes successfully.
        // valid_row 证明规范项目协议仍可成功解码。
        let valid_row = stored_project_row_for_loading_test();
        assert!(validate_project_row(valid_row.clone()).is_ok());

        // missing_id removes the stable project identity.
        // missing_id 移除稳定项目身份。
        let mut missing_id = valid_row.clone();
        missing_id.id = String::from("   ");
        // missing_name removes the required display identity.
        // missing_name 移除必需展示身份。
        let mut missing_name = valid_row.clone();
        missing_name.name = String::from("   ");
        // missing_root removes the required filesystem identity.
        // missing_root 移除必需文件系统身份。
        let mut missing_root = valid_row.clone();
        missing_root.root_path = String::from("   ");
        // relative_root supplies a non-authoritative process-relative path.
        // relative_root 提供非权威的进程相对路径。
        let mut relative_root = valid_row.clone();
        relative_root.root_path = String::from("relative/project");
        // invalid_pinned supplies a boolean flag outside the 0/1 protocol.
        // invalid_pinned 提供 0/1 协议之外的布尔标记。
        let mut invalid_pinned = valid_row.clone();
        invalid_pinned.pinned = 2;
        // invalid_codebase_memory supplies an enablement flag outside the strict 0/1 protocol.
        // invalid_codebase_memory 提供严格 0/1 协议之外的启用标记。
        let mut invalid_codebase_memory = valid_row.clone();
        invalid_codebase_memory.codebase_memory_enabled = 2;
        // invalid_archived supplies a lifecycle flag outside the 0/1 protocol.
        // invalid_archived 提供 0/1 协议之外的生命周期标记。
        let mut invalid_archived = valid_row.clone();
        invalid_archived.archived = -1;
        // invalid_order supplies the negative value that old loading clamped to zero.
        // invalid_order 提供旧加载逻辑钳制为零的负值。
        let mut invalid_order = valid_row;
        invalid_order.sort_order = -1;

        assert!(
            validate_project_row(missing_id)
                .expect_err("missing project id should fail")
                .contains("空白项目标识")
        );
        assert!(
            validate_project_row(missing_name)
                .expect_err("missing project name should fail")
                .contains("空白展示名称")
        );
        assert!(
            validate_project_row(missing_root)
                .expect_err("missing project root should fail")
                .contains("缺少根目录路径")
        );
        assert!(
            validate_project_row(relative_root)
                .expect_err("relative project root should fail")
                .contains("不是绝对路径")
        );
        assert!(
            validate_project_row(invalid_pinned)
                .expect_err("invalid pinned flag should fail")
                .contains("无效置顶标记")
        );
        assert!(
            validate_project_row(invalid_codebase_memory)
                .expect_err("invalid Codebase Memory flag should fail")
                .contains("Codebase Memory 启用标记")
        );
        assert!(
            validate_project_row(invalid_archived)
                .expect_err("invalid archived flag should fail")
                .contains("无效归档标记")
        );
        assert!(
            validate_project_row(invalid_order)
                .expect_err("negative project order should fail")
                .contains("无效分区顺序")
        );
    }

    /// Verifies default-project seeding depends only on validated active lifecycle state.
    /// 验证默认项目播种只依赖已验证的活动生命周期状态。
    #[test]
    fn default_project_requirement_uses_validated_archive_state() {
        // active_row is one validated visible project.
        // active_row 是一条已验证可见项目。
        let active_row = validate_project_row(stored_project_row_for_loading_test())
            .expect("active project row should validate");
        // archived_raw differs only by the exact archive protocol flag.
        // archived_raw 仅在精确归档协议标记上不同。
        let mut archived_raw = stored_project_row_for_loading_test();
        archived_raw.id = String::from("project-archived");
        archived_raw.archived = 1;
        // archived_row is one validated archived project.
        // archived_row 是一条已验证归档项目。
        let archived_row =
            validate_project_row(archived_raw).expect("archived project row should validate");

        assert!(workspace_requires_default_project(&[]));
        assert!(workspace_requires_default_project(&[archived_row]));
        assert!(!workspace_requires_default_project(&[active_row]));
    }

    /// Verifies project removal tombstones session ownership instead of creating a synthetic project summary.
    /// 验证项目移除会墓碑化会话归属，而不是创建合成项目摘要。
    #[test]
    fn retained_project_lookup_excludes_removed_project_sessions() {
        // retained_project is the only project row allowed to authorize session projection.
        // retained_project 是唯一允许授权会话投影的项目行。
        let retained_project = validate_project_row(stored_project_row_for_loading_test())
            .expect("retained project row should validate");
        // project_rows model the complete retained project table after another project was removed.
        // project_rows 模拟另一个项目被移除后的完整保留项目表。
        let project_rows = vec![retained_project];

        assert!(retained_project_row_for_session(&project_rows, "project-load").is_some());
        assert!(retained_project_row_for_session(&project_rows, "removed-project").is_none());
    }

    /// Verifies descending timestamp ordering covers the complete signed range without negation overflow.
    /// 验证时间戳降序排序无需取负即可覆盖完整有符号范围。
    #[test]
    fn descending_session_updated_at_key_handles_full_i64_range() {
        // newest_session_id owns the largest representable timestamp.
        // newest_session_id 持有最大可表示时间戳。
        let newest_session_id = SessionId::from_u128(0x4501);
        // oldest_session_id owns the smallest representable timestamp that cannot be negated safely.
        // oldest_session_id 持有无法安全取负的最小可表示时间戳。
        let oldest_session_id = SessionId::from_u128(0x4502);
        // updated_at_by_session provides both signed extremes to the production key helper.
        // updated_at_by_session 向生产排序键辅助函数提供两个有符号极值。
        let updated_at_by_session =
            BTreeMap::from([(newest_session_id, i64::MAX), (oldest_session_id, i64::MIN)]);
        // ordered_ids starts in the opposite order so sorting must move the newest identity first.
        // ordered_ids 以相反顺序开始，使排序必须把最新身份移到首位。
        let mut ordered_ids = vec![oldest_session_id, newest_session_id];
        ordered_ids.sort_by_key(|session_id| {
            descending_session_updated_at_key(&updated_at_by_session, session_id)
        });

        assert_eq!(ordered_ids, vec![newest_session_id, oldest_session_id]);
    }

    #[test]
    fn default_workspace_project_root_uses_user_vulcan_projects_directory() {
        let root = default_workspace_project_root_from_home(std::path::Path::new("example-home"));
        let expected_suffix =
            std::path::PathBuf::from(vulcan_runtime_assets::USER_VULCAN_ROOT_DIRECTORY_NAME)
                .join("projects")
                .join("vulcan_project");

        assert!(root.ends_with(expected_suffix));
    }

    #[test]
    fn default_workspace_project_summary_uses_requested_name_and_stable_id() {
        let project = default_workspace_project_summary(String::from(
            r"C:\Users\example\.vulcan\projects\vulcan_project",
        ));

        assert_eq!(project.title, DEFAULT_WORKSPACE_PROJECT_NAME);
        assert!(project.id.starts_with("vulcanproject-"));
        assert_eq!(project.order, 0);
        assert!(project.session_ids.is_empty());
    }

    #[test]
    fn is_default_workspace_project_matches_canonical_default_path() {
        let project = default_workspace_project_summary(
            default_workspace_project_path_text()
                .expect("test host should expose a home directory"),
        );

        assert!(
            is_default_workspace_project(&project)
                .expect("default project identity should be computable")
        );
    }

    #[test]
    fn workspace_project_path_text_strips_windows_verbatim_prefix() {
        let normalized = normalize_workspace_project_path_text(
            r"\\?\C:\Users\example\.vulcan\projects\vulcan_project",
        );

        assert_eq!(
            normalized,
            r"C:\Users\example\.vulcan\projects\vulcan_project"
        );
    }
}
