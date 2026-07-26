//! WebUI prompt context persistence helpers.
//! WebUI 提示词上下文持久化辅助函数。

use super::*;

pub(crate) fn snapshot_webui_context_before_user_prompt(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    _source: PendingSessionPromptSource,
) -> Result<(), String> {
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return Ok(());
    };
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return Ok(());
    };
    vulcan_workspace_store::workspace_store::snapshot_segmented_session_context_before_user_prompt(
        &controller_runtime.data_root,
        &project_id,
        session_id,
        runtime_state,
    )
}

/// Applies service-equivalent outer steps after one WebUI prompt begin reducer has run.
/// 在 WebUI 提示词 begin reducer 执行后应用与服务运行时等价的外层步骤。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the mutable WebUI workspace snapshot.
/// `workspace_snapshot` 是可变 WebUI 工作区快照。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
pub(crate) fn apply_webui_prompt_begin_outer_steps(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    persist_webui_context_current_document(
        controller_runtime,
        workspace_snapshot,
        session_id,
        session_title,
    );
}

/// Applies prompt begin state steps and defers current context persistence to a background task.
/// 应用提示词开始状态步骤，并将当前上下文持久化延后到后台任务。
///
/// # Parameters
/// 参数
/// `controller_runtime` provides workspace persistence paths and audit storage.
/// `controller_runtime` 提供工作区持久化路径与审计存储。
/// `workspace_snapshot` is the mutable WebUI workspace snapshot.
/// `workspace_snapshot` 是可变 WebUI 工作区快照。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
pub(crate) fn apply_webui_prompt_begin_outer_steps_deferred_context_persist(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    spawn_webui_context_current_document_persist_task(
        controller_runtime,
        workspace_snapshot,
        session_id,
        session_title,
    );
}

/// Spawns one background task that persists the current segmented context document.
/// 启动一个后台任务来持久化当前分段上下文文档。
///
/// # Parameters
/// 参数
/// `controller_runtime` provides the data root used by segmented session storage.
/// `controller_runtime` 提供分段会话存储使用的数据根目录。
/// `workspace_snapshot` contains the current runtime state.
/// `workspace_snapshot` 包含当前运行态。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
fn spawn_webui_context_current_document_persist_task(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return;
    };
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return;
    };
    let data_root = controller_runtime.data_root.clone();
    let runtime_state = runtime_state.clone();
    let session_title = session_title.to_string();
    spawn_service_task(async move {
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_segmented_session_context_current_document(
                &data_root,
                &project_id,
                session_id,
                &runtime_state,
            )
        {
            eprintln!(
                "[webui-service] current context persist failed for `{session_title}`: {error}"
            );
        }
    });
}

/// Persists the service-equivalent current context document for a WebUI session when possible.
/// 在可行时为 WebUI 会话持久化与服务运行时等价的当前上下文文档。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` contains the current runtime state.
/// `workspace_snapshot` 包含当前运行态。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
/// `session_title` labels diagnostics.
/// `session_title` 标记诊断信息。
pub(crate) fn persist_webui_context_current_document(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    session_title: &str,
) {
    let Some(project_id) = owning_project_id_for_session(workspace_snapshot, session_id) else {
        return;
    };
    let Some(runtime_state) = workspace_snapshot.session_runtime_state.get(&session_id) else {
        return;
    };
    if let Err(error) =
        vulcan_workspace_store::workspace_store::persist_segmented_session_context_current_document(
            &controller_runtime.data_root,
            &project_id,
            session_id,
            runtime_state,
        )
    {
        eprintln!("[webui-service] current context persist failed for `{session_title}`: {error}");
    }
}

/// Classifies whether prompt-session materialization selected or created its target.
/// 对提示词会话物化阶段选择或创建目标进行分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptSessionMaterializationKind {
    /// The submission selected an existing active session.
    /// 本次提交选择了已有活动会话。
    Existing,
    /// The submission created a new session in the selected project.
    /// 本次提交在所选项目中创建了新会话。
    Created,
}

/// Explicit conversation target selected before submission performs asynchronous work.
/// 提交执行异步工作前选择的显式会话目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PromptSessionTarget {
    /// Create one new session under the admitted project.
    /// 在已准入项目下创建一个新会话。
    Create,
    /// Continue one exact active session.
    /// 继续一个精确活动会话。
    Existing(SessionId),
}

impl PromptSessionTarget {
    /// Returns the existing session identity carried by this target, when applicable.
    /// 返回此目标携带的已有会话身份（若适用）。
    ///
    /// # Returns
    /// 返回值
    /// The exact existing session id, or `None` for explicit creation.
    /// 返回精确已有会话标识；显式新建时返回 `None`。
    pub(crate) const fn existing_session_id(self) -> Option<SessionId> {
        match self {
            Self::Create => None,
            Self::Existing(session_id) => Some(session_id),
        }
    }
}

/// Parses the optional transport field into an explicit create-or-continue target.
/// 将可选传输字段解析为显式的新建或继续目标。
///
/// # Parameters
/// 参数
/// `conversation_id` is absent only for an intentional new-session draft submission.
/// `conversation_id` 仅在有意提交新会话草稿时缺失。
///
/// # Returns
/// 返回值
/// A strong target, or an explicit error for blank, malformed, or nil identifiers.
/// 返回强类型目标；空白、畸形或 nil 标识返回显式错误。
pub(crate) fn parse_prompt_session_target(
    conversation_id: Option<&str>,
) -> Result<PromptSessionTarget, String> {
    let Some(raw_conversation_id) = conversation_id else {
        return Ok(PromptSessionTarget::Create);
    };
    // normalized_conversation_id is the only textual value accepted by UUID parsing and diagnostics.
    // normalized_conversation_id 是 UUID 解析与诊断接受的唯一文本值。
    let normalized_conversation_id = raw_conversation_id.trim();
    if normalized_conversation_id.is_empty() {
        return Err(String::from("Conversation id must not be blank"));
    }
    // session_id is the exact persisted identity requested by the transport caller.
    // session_id 是传输调用方请求的精确持久化身份。
    let session_id = parse_session_id(normalized_conversation_id)
        .ok_or_else(|| format!("Invalid conversation id: `{normalized_conversation_id}`"))?;
    if session_id.is_nil() {
        return Err(String::from("Conversation id must not be nil"));
    }
    Ok(PromptSessionTarget::Existing(session_id))
}

/// Materialized session identity and ownership facts used by submission activation.
/// 提交激活阶段使用的已物化会话身份与所有权事实。
pub(crate) struct MaterializedPromptSession {
    /// Exact session identifier selected or created by materialization.
    /// 物化阶段选择或创建的精确会话标识。
    pub(crate) session_id: SessionId,
    /// Authoritative owning project when the materialized session has a valid project identity.
    /// 已物化会话拥有有效项目身份时的权威所属项目。
    pub(crate) project_id: Option<String>,
    /// Human-readable session title captured from the authoritative summary.
    /// 从权威摘要捕获的人类可读会话标题。
    pub(crate) session_title: String,
    /// Workspace path owned by the materialized session project.
    /// 已物化会话所属项目拥有的工作区路径。
    pub(crate) workspace_path: String,
    /// Whether materialization selected an existing session or created a new one.
    /// 物化阶段选择已有会话还是创建新会话。
    pub(crate) kind: PromptSessionMaterializationKind,
}

/// Resolves or creates the session that should receive the submitted prompt.
/// 解析或创建应该接收本次提示词的会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` is the authoritative workspace revision mutated by materialization.
/// `workspace_snapshot` 是物化阶段修改的权威工作区版本。
/// `project_id` optionally selects the project used when a new session is required.
/// `project_id` 可选选择需要新建会话时使用的项目。
/// `target` explicitly selects creation or one exact existing active session.
/// `target` 显式选择新建或一个精确已有活动会话。
/// `prompt` supplies the generated title when a new session is created.
/// `prompt` 在创建新会话时提供生成标题所需内容。
///
/// # Returns
/// 返回值
/// The exact materialized session facts, or an error when no valid project can own a new session.
/// 返回精确的已物化会话事实；没有有效项目可承载新会话时返回错误。
pub(crate) fn materialize_prompt_session(
    workspace_snapshot: &mut WorkspaceSnapshot,
    project_id: Option<&str>,
    target: PromptSessionTarget,
    prompt: &str,
) -> Result<MaterializedPromptSession, String> {
    if let PromptSessionTarget::Existing(session_id) = target {
        let Some(session) =
            active_session_summary_for_session_mut(&mut workspace_snapshot.sessions, session_id)
        else {
            if workspace_snapshot
                .archived_sessions
                .iter()
                .any(|session| session.id == session_id)
            {
                return Err(format!("Conversation `{session_id}` is archived"));
            }
            return Err(format!("Conversation `{session_id}` was not found"));
        };
        session.status = SessionState::Running;
        session.last_updated_label = String::from("刚刚");
        workspace_snapshot.selected_session = session_id;
        let session_title = session.title.clone();
        let owner_project_id = session_summary_project_id(session).map(str::to_string);
        let workspace_path = owner_project_id
            .as_deref()
            .and_then(|owner_id| {
                workspace_task_project_for_project_id(workspace_snapshot, owner_id)
            })
            .map(|project| project.path.clone())
            .unwrap_or_default();
        return Ok(MaterializedPromptSession {
            session_id,
            project_id: owner_project_id,
            session_title,
            workspace_path,
            kind: PromptSessionMaterializationKind::Existing,
        });
    }

    let project_id = project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            workspace_snapshot
                .task_projects
                .first()
                .map(|project| project.id.clone())
        })
        .ok_or_else(|| String::from("新建会话需要提供项目标识。"))?;
    let project = workspace_snapshot
        .task_projects
        .iter_mut()
        .find(|candidate| candidate.id == project_id.as_str())
        .ok_or_else(|| format!("未找到项目：{project_id}"))?;
    // materialized_project_id captures the exact active project selected before its mutable membership update.
    // materialized_project_id 在项目成员关系变更前捕获选中的精确活动项目。
    let materialized_project_id = project.id.clone();
    let session_id = SessionId::new_v4();
    let now = current_unix_millis_text();
    let session_title = prompt_to_session_title(prompt);
    let session = SessionSummary::new(
        session_id,
        Some(project.id.clone()),
        session_title.clone(),
        "正在执行 WebUI 提交的任务",
        SessionState::Running,
        0,
        0,
        "刚刚",
    )
    .with_title_source(SessionTitleSource::Generated)
    .with_created_at(now.clone())
    .with_updated_at(now);

    project
        .session_ids
        .retain(|existing| *existing != session_id);
    project.session_ids.insert(0, session_id);
    let workspace_path = project.path.clone();
    workspace_snapshot.sessions.insert(0, session);
    workspace_snapshot
        .session_run_phases
        .insert(session_id, SessionRunPhase::Preparing);
    workspace_snapshot.selected_session = session_id;
    Ok(MaterializedPromptSession {
        session_id,
        project_id: Some(materialized_project_id),
        session_title,
        workspace_path,
        kind: PromptSessionMaterializationKind::Created,
    })
}

/// Loads project-local Vulcan Code configuration for one WebUI execution workspace.
/// 为一个 WebUI 执行工作区加载项目级 Vulcan Code 配置。
///
/// # Parameters
/// 参数
/// `workspace_path` is the absolute project workspace path.
/// `workspace_path` 是项目工作区绝对路径。
///
/// # Returns
/// 返回值
/// The project-local configuration, or the default configuration when loading fails.
/// 项目级配置；加载失败时返回默认配置。
pub(crate) fn load_project_configuration(
    workspace_path: &str,
) -> vulcan_provider_config::config::ProjectVulcanCodeConfigurationFile {
    vulcan_provider_config::config::load_project_configuration_file(&PathBuf::from(workspace_path))
        .unwrap_or_default()
}
