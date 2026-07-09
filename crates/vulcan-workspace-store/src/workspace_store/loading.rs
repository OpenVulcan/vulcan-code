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

/// Loads the current persisted workspace snapshot with an optional runtime-state pass.
/// 按可选运行态加载过程读取当前持久化工作区快照。
async fn load_workspace_snapshot_with_runtime_state(
    controller: ControllerRuntimeHandle,
    load_runtime_state: bool,
) -> Result<WorkspaceSnapshot, String> {
    let mut project_rows = query_project_rows(&controller).await?;
    if project_rows.iter().all(|row| row.archived != 0) {
        seed_default_workspace_project(&controller).await?;
        project_rows = query_project_rows(&controller).await?;
    }
    let session_rows = query_session_index_rows(&controller).await?;

    let mut task_projects = Vec::new();
    let mut archived_task_projects = Vec::new();
    for row in &project_rows {
        let project = TaskProjectSummary::new(
            row.id.clone(),
            row.name.clone(),
            row.root_path.clone(),
            row.pinned != 0,
            row.sort_order.max(0) as usize,
            row.icon_pack.clone(),
            row.icon_key.clone(),
            Vec::new(),
        );

        if row.archived != 0 {
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
    for row in &session_rows {
        let session_id = parse_session_id(&row.id)?;
        let project_id = if row.project_id.trim().is_empty() {
            row.project_tag.clone()
        } else {
            row.project_id.clone()
        };
        let project_tag = if row.project_tag.trim().is_empty() {
            project_id.clone()
        } else {
            row.project_tag.clone()
        };
        let archived = row.archived != 0;
        let session_dir = segmented_session_directory(
            &controller.data_root,
            archived,
            &project_tag,
            &segmented_session_date_from_unix_millis_text(&row.created_at),
            &row.id,
        );
        if !session_dir.exists() {
            continue;
        }
        let project_name = project_rows
            .iter()
            .find(|row| row.id == project_id)
            .map(|row| row.name.clone())
            .unwrap_or_else(|| project_id.clone());
        let project_row_exists = project_rows.iter().any(|row| row.id == project_id);
        if archived && project_row_exists {
            ensure_project_summary_for_session(
                &mut archived_task_projects,
                &project_id,
                &project_name,
                session_id,
            );
        } else if !archived {
            ensure_project_summary_for_session(
                &mut task_projects,
                &project_id,
                &project_name,
                session_id,
            );
        }
        if !archived && load_runtime_state {
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
            Some(project_id),
            title,
            focus,
            SessionState::Draft,
            0,
            0,
            relative_time_label_from_unix_millis_text(&row.updated_at),
        )
        .with_title_source(SessionTitleSource::from_persisted_key(
            row.title_source.as_deref().unwrap_or("manual"),
        ))
        .with_created_at(row.created_at.clone())
        .with_updated_at(row.updated_at.clone());
        session_updated_at.insert(
            session_id,
            row.updated_at.parse::<i64>().unwrap_or_default(),
        );

        if archived {
            archived_sessions.push(session_summary);
        } else {
            visible_sessions.push(session_summary);
        }
    }

    for project in &mut task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            -session_updated_at
                .get(session_id)
                .copied()
                .unwrap_or_default()
        });
    }

    for project in &mut archived_task_projects {
        project.session_ids.sort_unstable_by_key(|session_id| {
            -session_updated_at
                .get(session_id)
                .copied()
                .unwrap_or_default()
        });
    }

    visible_sessions.sort_by_key(|session| {
        -session_updated_at
            .get(&session.id)
            .copied()
            .unwrap_or_default()
    });

    archived_sessions.sort_by_key(|session| {
        -session_updated_at
            .get(&session.id)
            .copied()
            .unwrap_or_default()
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
    let normalized_root =
        std::fs::canonicalize(&project_root).unwrap_or_else(|_| project_root.clone());
    let project =
        default_workspace_project_summary(workspace_project_path_text_from_path(&normalized_root));
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
    let project_rows = query_project_rows(&controller).await?;
    if project_rows.iter().any(|row| row.archived == 0) {
        return Ok(());
    }
    seed_default_workspace_project(&controller).await
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
