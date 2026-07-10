#[cfg(test)]
use super::segmented_session_store::read_session_metadata;
use super::segmented_session_store::{
    archive_segmented_session_by_created_date, restore_segmented_session_by_created_date,
};
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

/// Loads the current persisted workspace snapshot from project config and segmented text metadata.
/// 从项目配置与分段文本元数据加载当前持久化工作台快照。
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
        archived,
        sort_order,
    })
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

/// Validated session index row used by deterministic workspace loading.
/// 确定性工作区加载使用的已验证会话索引行。
#[derive(Debug)]
struct ValidatedSessionIndexRow {
    /// Original decoded SQLite row retained for display-field projection.
    /// 为展示字段投影保留的原始 SQLite 解码行。
    stored: StoredSessionIndexRow,
    /// Strictly parsed stable session identity.
    /// 严格解析后的稳定会话身份。
    session_id: SessionId,
    /// Exact lifecycle state decoded from the numeric archive flag.
    /// 从数值归档标记解码出的精确生命周期状态。
    archived: bool,
    /// Canonical directory date derived from the persisted creation timestamp.
    /// 从持久化创建时间戳派生的规范目录日期。
    created_date: String,
    /// Strictly parsed update timestamp used for ordering and relative labels.
    /// 用于排序与相对时间标签的严格解析更新时间戳。
    updated_at_millis: i64,
}

/// Validates one raw SQLite session row before it can influence paths or workspace projection.
/// 在原始 SQLite 会话行影响路径或工作区投影前对其进行验证。
///
/// # Parameters
/// 参数
/// `row` is the complete session index row decoded from the Controller response.
/// `row` 是从 Controller 响应解码出的完整会话索引行。
///
/// # Returns
/// 返回值
/// Returns a validated row or an explicit identity, lifecycle, or timestamp corruption error.
/// 返回已验证行；身份、生命周期或时间戳损坏时返回显式错误。
fn validate_session_index_row(
    row: StoredSessionIndexRow,
) -> Result<ValidatedSessionIndexRow, String> {
    // session_id is parsed before any path component is accepted.
    // session_id 在接受任何路径组成部分前完成解析。
    let session_id = parse_session_id(&row.id)?;
    if row.project_id.trim().is_empty() {
        return Err(format!("会话索引 `{session_id}` 缺少所属项目标识。"));
    }
    if row.project_tag.trim().is_empty() {
        return Err(format!("会话索引 `{session_id}` 缺少目录项目标签。"));
    }
    // archived accepts only the two persisted lifecycle states defined by the schema protocol.
    // archived 只接受 Schema 协议定义的两种持久化生命周期状态。
    let archived = match row.archived {
        0 => false,
        1 => true,
        value => {
            return Err(format!(
                "会话索引 `{session_id}` 包含无效归档标记 `{value}`。"
            ));
        }
    };
    // created_date rejects blank or malformed locator identity instead of selecting 1970-01-01.
    // created_date 拒绝空白或格式错误的定位身份，而不是选择 1970-01-01。
    let created_date = session_directory_date_from_unix_millis_text(&row.created_at)
        .map_err(|error| format!("会话索引 `{session_id}` 的创建时间无效：{error}"))?;
    // updated_at_millis is shared by ordering and relative-label projection to prevent divergent parsing.
    // updated_at_millis 由排序与相对时间投影共享，防止解析规则漂移。
    let updated_at_millis = parse_unix_millis_text(&row.updated_at)
        .map_err(|error| format!("会话索引 `{session_id}` 的更新时间无效：{error}"))?;
    Ok(ValidatedSessionIndexRow {
        stored: row,
        session_id,
        archived,
        created_date,
        updated_at_millis,
    })
}

/// Resolves the one real directory allowed by a validated session lifecycle locator.
/// 解析已验证会话生命周期定位信息唯一允许的真实目录。
///
/// # Parameters
/// 参数
/// `data_root` is the workspace persistence root containing both lifecycle trees.
/// `data_root` 是包含两个生命周期树的工作区持久化根。
/// `row` is the validated SQLite locator and lifecycle state.
/// `row` 是已验证的 SQLite 定位信息与生命周期状态。
///
/// # Returns
/// 返回值
/// Returns the expected real directory after reconciling one interrupted move to the SQLite lifecycle state.
/// 将一次中断移动校正到 SQLite 生命周期状态后返回预期真实目录。
fn required_session_directory_for_index_row(
    data_root: &std::path::Path,
    row: &ValidatedSessionIndexRow,
) -> Result<std::path::PathBuf, String> {
    // expected_dir is the only path consistent with the persisted archive flag.
    // expected_dir 是唯一与持久化归档标记一致的路径。
    let expected_dir = segmented_session_directory(
        data_root,
        row.archived,
        &row.stored.project_tag,
        &row.created_date,
        &row.stored.id,
    );
    // opposite_dir detects an interrupted archive or restore without treating it as normal state.
    // opposite_dir 用于检测中断的归档或恢复，且不会把它视为正常状态。
    let opposite_dir = segmented_session_directory(
        data_root,
        !row.archived,
        &row.stored.project_tag,
        &row.created_date,
        &row.stored.id,
    );
    // expected_exists and opposite_exists establish the complete two-tree locator state.
    // expected_exists 与 opposite_exists 建立完整的双树定位状态。
    let expected_exists = segmented_session_directory_exists(&expected_dir)?;
    let opposite_exists = segmented_session_directory_exists(&opposite_dir)?;
    match (expected_exists, opposite_exists) {
        (true, false) => Ok(expected_dir),
        (false, true) => {
            eprintln!(
                "[workspace-load] recovering interrupted session lifecycle: id={} archived={} source={} destination={}",
                row.session_id,
                row.archived,
                opposite_dir.display(),
                expected_dir.display()
            );
            // recovered_dir is produced only by moving the sole filesystem copy back to the authoritative SQLite state.
            // recovered_dir 只通过把唯一文件系统副本移回权威 SQLite 状态来生成。
            let recovered_dir = if row.archived {
                archive_segmented_session_by_created_date(
                    data_root,
                    &row.stored.project_tag,
                    &row.created_date,
                    &row.stored.id,
                    &row.stored.updated_at,
                )?
            } else {
                restore_segmented_session_by_created_date(
                    data_root,
                    &row.stored.project_tag,
                    &row.created_date,
                    &row.stored.id,
                    &row.stored.updated_at,
                )?
            };
            if recovered_dir != expected_dir {
                return Err(format!(
                    "会话索引 `{}` 的生命周期恢复返回意外目录 `{}`，期望 `{}`。",
                    row.session_id,
                    recovered_dir.display(),
                    expected_dir.display()
                ));
            }
            Ok(recovered_dir)
        }
        (false, false) => Err(format!(
            "会话索引 `{}` 的活动与归档目录均不存在：预期 `{}`，相反 `{}`。",
            row.session_id,
            expected_dir.display(),
            opposite_dir.display()
        )),
        (true, true) => Err(format!(
            "会话索引 `{}` 同时存在活动与归档目录，无法确定权威副本：预期 `{}`，相反 `{}`。",
            row.session_id,
            expected_dir.display(),
            opposite_dir.display()
        )),
    }
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
    load_runtime_state: bool,
) -> Result<WorkspaceSnapshot, String> {
    eprintln!("[workspace-load] begin: load_runtime_state={load_runtime_state}");
    // project_rows are validated before their archive flags are allowed to trigger default seeding.
    // project_rows 在其归档标记获准触发默认播种前完成验证。
    let mut project_rows = query_validated_project_rows(&controller).await?;
    eprintln!(
        "[workspace-load] project query complete: rows={}",
        project_rows.len()
    );
    if workspace_requires_default_project(&project_rows) {
        eprintln!("[workspace-load] default project seed begin");
        seed_default_workspace_project(&controller).await?;
        eprintln!("[workspace-load] default project seed complete");
        project_rows = query_validated_project_rows(&controller).await?;
        if workspace_requires_default_project(&project_rows) {
            return Err(String::from("默认项目播种完成后仍不存在活动项目。"));
        }
    }
    let session_rows = query_session_index_rows(&controller).await?;
    eprintln!(
        "[workspace-load] session index query complete: rows={}",
        session_rows.len()
    );

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
        );

        if validated_row.archived {
            archived_task_projects.push(project);
        } else {
            task_projects.push(project);
        }
    }

    let transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let archived_transcript_map: BTreeMap<SessionId, Vec<SessionMessageEntry>> = BTreeMap::new();
    let compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();
    let archived_compaction_map: BTreeMap<SessionId, Vec<SessionCompactionEntry>> = BTreeMap::new();

    let mut runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let archived_runtime_state_map: BTreeMap<SessionId, SessionRuntimeStateEntry> = BTreeMap::new();
    let mut visible_sessions = Vec::new();
    let mut archived_sessions = Vec::new();
    let mut session_updated_at = BTreeMap::new();
    // removed_project_session_count records valid tombstones excluded from the live workspace projection.
    // removed_project_session_count 记录从实时工作区投影中排除的合法项目删除墓碑。
    let mut removed_project_session_count = 0usize;
    for stored_row in session_rows {
        // validated_row is the only representation allowed to address directories or build summaries.
        // validated_row 是唯一允许用于目录寻址或摘要构建的表示。
        let validated_row = validate_session_index_row(stored_row)?;
        // row retains the display fields after identity and timestamp validation succeeds.
        // row 在身份与时间戳验证成功后保留展示字段。
        let row = &validated_row.stored;
        // session_id is the strictly parsed UUID shared by project ownership and runtime maps.
        // session_id 是项目归属与运行态映射共享的严格解析 UUID。
        let session_id = validated_row.session_id;
        // project_id is the exact persisted owner and never falls back to the directory tag.
        // project_id 是精确持久化所属项目，绝不会回退到目录标签。
        let project_id = row.project_id.as_str();
        // project_row proves the locator does not reference a missing project record.
        // project_row 证明定位信息没有引用缺失的项目记录。
        let Some(project_row) = retained_project_row_for_session(&project_rows, project_id) else {
            removed_project_session_count += 1;
            continue;
        };
        if !validated_row.archived && project_row.archived {
            return Err(format!(
                "活动会话索引 `{session_id}` 仍属于归档项目 `{project_id}`，项目恢复状态未提交。"
            ));
        }
        // project_name is the authoritative persisted display name of the existing project row.
        // project_name 是现有项目行的权威持久化展示名称。
        let project_name = project_row.stored.name.as_str();
        // session_dir must be the one real directory consistent with the exact archive flag.
        // session_dir 必须是与精确归档标记一致的唯一真实目录。
        let session_dir =
            required_session_directory_for_index_row(&controller.data_root, &validated_row)?;
        if validated_row.archived {
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
        if !validated_row.archived && load_runtime_state {
            let mut runtime_state = if let Some(state_document) = read_state(&session_dir)? {
                if state_document.runtime_state.is_null() {
                    None
                } else {
                    let mut runtime_state = serde_json::from_value::<SessionRuntimeStateEntry>(
                        state_document.runtime_state,
                    )
                    .map_err(|error| {
                        format!("解析分段会话 `{}` 的 state.json 失败：{error}", row.id)
                    })?;
                    if runtime_state.updated_at.trim().is_empty()
                        && !state_document.updated_at.trim().is_empty()
                    {
                        runtime_state.updated_at = state_document.updated_at;
                    }
                    Some(runtime_state)
                }
            } else {
                None
            };
            if let Some(addon_state) = read_session_addon_state(&session_dir)? {
                addon_state.apply_to_runtime_state(
                    runtime_state.get_or_insert_with(SessionRuntimeStateEntry::default),
                );
            }
            if let Some(mut runtime_state) = runtime_state {
                let _ = sanitize_loaded_runtime_state_for_fresh_process(&mut runtime_state);
                runtime_state_map.insert(session_id, runtime_state);
            }
        }
        let focus = session_focus_preview(&[]);
        let title = row
            .title
            .clone()
            .unwrap_or_else(|| String::from("Untitled"));
        let session_summary = SessionSummary::new(
            session_id,
            Some(project_id.to_string()),
            title,
            focus,
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis(validated_row.updated_at_millis),
        )
        .with_title_source(SessionTitleSource::from_persisted_key(
            row.title_source.as_deref().unwrap_or("manual"),
        ))
        .with_created_at(row.created_at.clone())
        .with_updated_at(row.updated_at.clone());
        session_updated_at.insert(session_id, validated_row.updated_at_millis);

        if validated_row.archived {
            archived_sessions.push(session_summary);
        } else {
            visible_sessions.push(session_summary);
        }
    }

    eprintln!(
        "[workspace-load] session projection complete: loaded={} removed_project_tombstones={removed_project_session_count}",
        visible_sessions.len() + archived_sessions.len()
    );

    for project in &mut task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            descending_session_updated_at_key(&session_updated_at, session_id)
        });
    }

    for project in &mut archived_task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            descending_session_updated_at_key(&session_updated_at, session_id)
        });
    }

    visible_sessions
        .sort_by_key(|session| descending_session_updated_at_key(&session_updated_at, &session.id));

    archived_sessions
        .sort_by_key(|session| descending_session_updated_at_key(&session_updated_at, &session.id));

    let collapsed_task_projects = query_collapsed_task_project_ids(&controller)
        .await?
        .unwrap_or_default();
    eprintln!(
        "[workspace-load] collapsed-project query complete: rows={}",
        collapsed_task_projects.len()
    );
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

    eprintln!("[workspace-load] snapshot assembly complete");
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
/// `home` is the user home directory that owns the `.vulcan` data root.
/// `home` 是拥有 `.vulcan` 数据根目录的用户主目录。
///
/// # Returns
/// 返回值
/// Returns `<home>/.vulcan/projects/vulcan_project`.
/// 返回 `<home>/.vulcan/projects/vulcan_project`。
fn default_workspace_project_root_from_home(home: &std::path::Path) -> std::path::PathBuf {
    home.join(".vulcan")
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
    normalize_workspace_project_path_text(&path.to_string_lossy())
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
    let trimmed_path = path.trim();
    if trimmed_path.is_empty() {
        return String::new();
    }
    let path_without_verbatim_prefix = strip_windows_verbatim_prefix(trimmed_path);
    if looks_like_windows_path(trimmed_path, &path_without_verbatim_prefix) {
        path_without_verbatim_prefix.replace('/', "\\")
    } else {
        path_without_verbatim_prefix
    }
}

/// Returns whether one path should be represented with Windows separators.
/// 返回单个路径是否应使用 Windows 分隔符表示。
///
/// # Parameters
/// 参数
/// `original_path` is the path before verbatim-prefix normalization.
/// `original_path` 是移除 verbatim 前缀前的路径。
/// `normalized_path` is the path after verbatim-prefix normalization.
/// `normalized_path` 是移除 verbatim 前缀后的路径。
///
/// # Returns
/// 返回值
/// Returns `true` when the path has Windows drive or UNC semantics.
/// 当路径具备 Windows 盘符或 UNC 语义时返回 `true`。
fn looks_like_windows_path(original_path: &str, normalized_path: &str) -> bool {
    let bytes = normalized_path.as_bytes();
    (bytes.len() >= 3
        && bytes[1] == b':'
        && bytes[0].is_ascii_alphabetic()
        && matches!(bytes[2], b'/' | b'\\'))
        || normalized_path.starts_with(r"\\")
        || original_path.starts_with(r"\\?\UNC\")
        || original_path.starts_with("//?/UNC/")
        || (cfg!(windows) && normalized_path.starts_with("//"))
}

/// Removes Windows verbatim path prefixes while preserving regular path addressing.
/// 移除 Windows verbatim 路径前缀，同时保留常规路径寻址语义。
///
/// # Parameters
/// 参数
/// `path` is raw host path text.
/// `path` 是原始宿主路径文本。
///
/// # Returns
/// 返回值
/// Returns the path text without a Windows verbatim prefix.
/// 返回不含 Windows verbatim 前缀的路径文本。
fn strip_windows_verbatim_prefix(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix("//?/UNC/") {
        return format!("//{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    if let Some(rest) = path.strip_prefix("//?/") {
        return rest.to_string();
    }
    path.to_string()
}

/// Clears process-scoped runtime latches that must never survive an application restart.
/// 清理绝不能跨应用重启保留的进程级运行态闩锁。
///
/// # Parameters
/// 参数
/// `runtime_state` is the freshly loaded persisted session runtime state.
/// `runtime_state` 是刚从持久化内容加载出的会话运行态。
///
/// # Returns
/// 返回值
/// Whether any volatile field was cleared.
/// 是否清理了任何临时字段。
pub(super) fn sanitize_loaded_runtime_state_for_fresh_process(
    runtime_state: &mut SessionRuntimeStateEntry,
) -> bool {
    runtime_state.active_precheck_turn_id.take().is_some()
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
            archived: 0,
        }
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

    /// Builds one canonical raw session index row for strict loading tests.
    /// 为严格加载测试构建一条规范原始会话索引行。
    ///
    /// # Returns
    /// 返回值
    /// A complete active locator row with valid identity and timestamps.
    /// 返回身份与时间戳均有效的完整活动定位行。
    fn stored_session_index_row_for_loading_test() -> StoredSessionIndexRow {
        StoredSessionIndexRow {
            id: SessionId::from_u128(0x45).to_string(),
            project_id: String::from("project-load"),
            project_tag: String::from("project-load-tag"),
            title: Some(String::from("严格加载测试")),
            title_source: Some(String::from("manual")),
            archived: 0,
            created_at: String::from("0"),
            updated_at: String::from("1000"),
        }
    }

    /// Verifies raw session index validation rejects every locator field that previously used a fallback.
    /// 验证原始会话索引校验会拒绝此前使用兜底逻辑的每个定位字段。
    #[test]
    fn session_index_validation_rejects_fallback_identity_and_timestamp_states() {
        // valid_row proves the canonical protocol still decodes successfully.
        // valid_row 证明规范协议仍可成功解码。
        let valid_row = stored_session_index_row_for_loading_test();
        assert!(validate_session_index_row(valid_row.clone()).is_ok());

        // missing_project_id removes the authoritative ownership identity.
        // missing_project_id 移除权威所属项目身份。
        let mut missing_project_id = valid_row.clone();
        missing_project_id.project_id = String::from("   ");
        // missing_project_tag removes the authoritative directory identity.
        // missing_project_tag 移除权威目录身份。
        let mut missing_project_tag = valid_row.clone();
        missing_project_tag.project_tag = String::from("   ");
        // invalid_archive_flag supplies a lifecycle state outside the 0/1 protocol.
        // invalid_archive_flag 提供 0/1 协议之外的生命周期状态。
        let mut invalid_archive_flag = valid_row.clone();
        invalid_archive_flag.archived = 2;
        // invalid_created_at supplies a path identity that cannot be decoded.
        // invalid_created_at 提供无法解码的路径身份。
        let mut invalid_created_at = valid_row.clone();
        invalid_created_at.created_at = String::from("not-created-at");
        // invalid_updated_at supplies an ordering identity that cannot be decoded.
        // invalid_updated_at 提供无法解码的排序身份。
        let mut invalid_updated_at = valid_row;
        invalid_updated_at.updated_at = String::from("not-updated-at");

        assert!(
            validate_session_index_row(missing_project_id)
                .expect_err("missing project id should fail")
                .contains("缺少所属项目标识")
        );
        assert!(
            validate_session_index_row(missing_project_tag)
                .expect_err("missing project tag should fail")
                .contains("缺少目录项目标签")
        );
        assert!(
            validate_session_index_row(invalid_archive_flag)
                .expect_err("invalid archive flag should fail")
                .contains("无效归档标记")
        );
        assert!(
            validate_session_index_row(invalid_created_at)
                .expect_err("invalid created timestamp should fail")
                .contains("创建时间无效")
        );
        assert!(
            validate_session_index_row(invalid_updated_at)
                .expect_err("invalid updated timestamp should fail")
                .contains("更新时间无效")
        );
    }

    /// Verifies loading restores an interrupted filesystem move to the authoritative active index state.
    /// 验证加载会把中断的文件系统移动恢复到权威活动索引状态。
    #[test]
    fn required_session_directory_recovers_active_index_and_rejects_invalid_states() {
        // root isolates every filesystem state exercised by this locator.
        // root 隔离该定位信息覆盖的每一种文件系统状态。
        let root =
            std::env::temp_dir().join(format!("vulcan-strict-session-load-{}", Uuid::new_v4()));
        // validated_row provides the exact active lifecycle identity.
        // validated_row 提供精确活动生命周期身份。
        let validated_row = validate_session_index_row(stored_session_index_row_for_loading_test())
            .expect("canonical row should validate");
        // expected_dir is the only directory allowed by archived=false.
        // expected_dir 是 archived=false 唯一允许的目录。
        let expected_dir = segmented_session_directory(
            &root,
            false,
            &validated_row.stored.project_tag,
            &validated_row.created_date,
            &validated_row.stored.id,
        );
        // opposite_dir represents an interrupted transition into the archived tree.
        // opposite_dir 表示进入归档树但尚未提交的中断转换。
        let opposite_dir = segmented_session_directory(
            &root,
            true,
            &validated_row.stored.project_tag,
            &validated_row.created_date,
            &validated_row.stored.id,
        );
        // created_dir contains valid metadata required by lifecycle rollback.
        // created_dir 包含生命周期回滚所需的有效元数据。
        let created_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: validated_row.stored.id.clone(),
                project_tag: validated_row.stored.project_tag.clone(),
                created_date: validated_row.created_date.clone(),
                created_at: validated_row.stored.created_at.clone(),
                project_id: Some(validated_row.stored.project_id.clone()),
                title: validated_row.stored.title.clone(),
                title_source: validated_row.stored.title_source.clone(),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("create expected active session directory");
        assert_eq!(created_dir, expected_dir);
        assert_eq!(
            required_session_directory_for_index_row(&root, &validated_row)
                .expect("expected-only directory should load"),
            expected_dir
        );

        std::fs::create_dir_all(&opposite_dir).expect("create ambiguous archived directory");
        // ambiguous_error proves simultaneous copies never select a winner.
        // ambiguous_error 证明副本同时存在时绝不会选择胜者。
        let ambiguous_error = required_session_directory_for_index_row(&root, &validated_row)
            .expect_err("simultaneous lifecycle directories should fail");
        std::fs::remove_dir_all(&opposite_dir).expect("remove opposite directory");
        archive_segmented_session_by_created_date(
            &root,
            &validated_row.stored.project_tag,
            &validated_row.created_date,
            &validated_row.stored.id,
            "2000",
        )
        .expect("move session into interrupted archived state");
        // recovered_dir proves SQLite archived=false deterministically rolls the sole copy back to active.
        // recovered_dir 证明 SQLite archived=false 会确定性地把唯一副本回滚到活动树。
        let recovered_dir = required_session_directory_for_index_row(&root, &validated_row)
            .expect("opposite-only directory should recover");
        let recovered_metadata =
            read_session_metadata(&recovered_dir).expect("read recovered active metadata");
        assert_eq!(recovered_dir, expected_dir);
        assert!(expected_dir.is_dir());
        assert!(!opposite_dir.exists());
        assert_eq!(
            recovered_metadata.updated_at,
            validated_row.stored.updated_at
        );
        assert_eq!(recovered_metadata.archived_at, None);

        std::fs::remove_dir_all(&expected_dir).expect("remove expected directory");
        // missing_error proves absent copies cannot disappear from the workspace silently.
        // missing_error 证明两个副本缺失时不能从工作台静默消失。
        let missing_error = required_session_directory_for_index_row(&root, &validated_row)
            .expect_err("missing lifecycle directories should fail");
        // expected_parent is retained so a regular file can occupy the canonical locator path.
        // expected_parent 被保留以便普通文件占用规范定位路径。
        let expected_parent = expected_dir
            .parent()
            .expect("expected directory should have parent");
        std::fs::create_dir_all(expected_parent).expect("create expected directory parent");
        std::fs::write(&expected_dir, b"not-a-directory").expect("create conflicting locator file");
        // non_directory_error proves path type validation happens before state projection.
        // non_directory_error 证明路径类型校验发生在状态投影前。
        let non_directory_error = required_session_directory_for_index_row(&root, &validated_row)
            .expect_err("non-directory locator should fail");
        let _ = std::fs::remove_dir_all(&root);

        assert!(ambiguous_error.contains("同时存在活动与归档目录"));
        assert!(missing_error.contains("活动与归档目录均不存在"));
        assert!(non_directory_error.contains("not a real directory"));
    }

    /// Verifies loading commits an interrupted active-directory move to the authoritative archived index state.
    /// 验证加载会把中断的活动目录移动提交到权威归档索引状态。
    #[test]
    fn required_session_directory_recovers_archived_index_state() {
        // root isolates the archived-index recovery fixture.
        // root 隔离归档索引恢复夹具。
        let root =
            std::env::temp_dir().join(format!("vulcan-archived-session-load-{}", Uuid::new_v4()));
        // stored_row selects archived state while the fixture intentionally starts in the active tree.
        // stored_row 选择归档状态，而夹具故意从活动树开始。
        let mut stored_row = stored_session_index_row_for_loading_test();
        stored_row.archived = 1;
        // validated_row is the authoritative archived locator used by recovery.
        // validated_row 是恢复过程使用的权威归档定位信息。
        let validated_row =
            validate_session_index_row(stored_row).expect("archived row should validate");
        // active_dir is the interrupted filesystem state that must be moved to archived.
        // active_dir 是必须移入归档树的中断文件系统状态。
        let active_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id: validated_row.stored.id.clone(),
                project_tag: validated_row.stored.project_tag.clone(),
                created_date: validated_row.created_date.clone(),
                created_at: validated_row.stored.created_at.clone(),
                project_id: Some(validated_row.stored.project_id.clone()),
                title: validated_row.stored.title.clone(),
                title_source: validated_row.stored.title_source.clone(),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("create interrupted active session directory");
        // recovered_dir is the exact archived directory selected by SQLite state.
        // recovered_dir 是 SQLite 状态选中的精确归档目录。
        let recovered_dir = required_session_directory_for_index_row(&root, &validated_row)
            .expect("active-only directory should archive during recovery");
        let recovered_metadata =
            read_session_metadata(&recovered_dir).expect("read recovered archived metadata");

        assert!(!active_dir.exists());
        assert!(recovered_dir.is_dir());
        assert_eq!(
            recovered_metadata.updated_at,
            validated_row.stored.updated_at
        );
        assert_eq!(
            recovered_metadata.archived_at.as_deref(),
            Some(validated_row.stored.updated_at.as_str())
        );
        let _ = std::fs::remove_dir_all(&root);
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
        let expected_suffix = std::path::PathBuf::from(".vulcan")
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
