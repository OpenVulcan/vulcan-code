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
    apply_webui_prompt_begin_outer_state_steps(controller_runtime, workspace_snapshot, session_id);
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
    apply_webui_prompt_begin_outer_state_steps(controller_runtime, workspace_snapshot, session_id);
    spawn_webui_context_current_document_persist_task(
        controller_runtime,
        workspace_snapshot,
        session_id,
        session_title,
    );
}

/// Applies prompt begin state mutations that must complete before the provider request starts.
/// 应用 Provider 请求启动前必须完成的提示词开始状态变更。
///
/// # Parameters
/// 参数
/// `controller_runtime` provides audit storage for removed legacy context injections.
/// `controller_runtime` 提供移除旧上下文注入时使用的审计存储。
/// `workspace_snapshot` is the mutable WebUI workspace snapshot.
/// `workspace_snapshot` 是可变 WebUI 工作区快照。
/// `session_id` selects the prompt session.
/// `session_id` 选择提示词会话。
fn apply_webui_prompt_begin_outer_state_steps(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
) {
    if let Some(runtime_state) = workspace_snapshot
        .session_runtime_state
        .get_mut(&session_id)
    {
        let previous_persistent_fingerprint =
            persistent_injection_fingerprint(&runtime_state.context_injection_state);
        if remove_legacy_vmm_profile_context_injection_state(
            &mut runtime_state.context_injection_state,
        ) {
            let timestamp = current_unix_millis_text();
            runtime_state.updated_at = timestamp.clone();
            let event = legacy_vmm_injection_cancelled_event(
                previous_persistent_fingerprint,
                "legacy_vmm_profile_webui_prompt_cleanup",
            );
            if let Err(error) = record_webui_runtime_injection_cancelled(
                controller_runtime,
                session_id,
                event,
                &timestamp,
            ) {
                webui_trace(
                    "runtime_injection_cancelled_audit_failed",
                    format!("session={session_id} error={error}"),
                );
            }
        }
    }
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

/// Resolves or creates the session that should receive the submitted prompt.
/// 解析或创建应该接收本次提示词的会话。
pub(crate) fn materialize_prompt_session(
    workspace_snapshot: &mut WorkspaceSnapshot,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    prompt: &str,
) -> Result<(SessionId, String, String), String> {
    if let Some(session_id) = conversation_id.and_then(parse_session_id)
        && let Some(session) = workspace_snapshot
            .sessions
            .iter_mut()
            .find(|candidate| candidate.id == session_id)
    {
        session.status = SessionState::Running;
        session.last_updated_label = String::from("刚刚");
        workspace_snapshot.selected_session = session_id;
        let workspace_path = session
            .project_id
            .as_deref()
            .and_then(|owner_id| {
                workspace_snapshot
                    .task_projects
                    .iter()
                    .find(|project| project.id == owner_id)
            })
            .map(|project| project.path.clone())
            .unwrap_or_default();
        return Ok((session_id, session.title.clone(), workspace_path));
    }

    let project_id = project_id.ok_or_else(|| String::from("新建会话需要提供项目标识。"))?;
    let project = workspace_snapshot
        .task_projects
        .iter_mut()
        .find(|candidate| candidate.id == project_id)
        .ok_or_else(|| format!("未找到项目：{project_id}"))?;
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
    Ok((session_id, session_title, workspace_path))
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
