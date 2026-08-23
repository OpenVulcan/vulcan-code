//! Post-materialization activation for immediate conversation submissions.
//! 即时会话提交的物化后激活。

use vulcan_service_api::{
    ConversationSummary, ConversationSummaryChangedDelta, ServiceEvent, ServiceEventEmitter,
    WorkbenchDelta, WorkbenchDeltaEnvelope, WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionMessageEntry, SessionRunPhase, SessionState};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    MaterializedPromptSession, PromptSessionMaterializationKind,
    SessionContinuationRuntimeBlockers, SharedServiceBridgeState,
    conversation_continuation_summary, conversation_fork_source_summary_from_session,
    emit_service_event_with_trace, emit_workbench_event, ensure_webui_session_runtime_entry,
    is_active_session_run_phase, pending_question_snapshots_from_agent_runtime,
    refresh_conversation_attention_summary, restore_webui_runtime_controls_for_session,
    session_plan_mode_summary_from_runtime_state, session_summary_project_id_owned,
    sync_workbench_runtime_control_fields_locked, workspace_session_summary_for_session,
    workspace_session_summary_for_session_mut,
};

/// Initializes one materialized session inside the detached submission workspace.
/// 在分离的提交工作区中初始化一个已物化会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the materialized session and receives runtime defaults.
/// `workspace_snapshot` 持有已物化会话并接收运行态默认值。
/// `materialized_session` carries the exact existing-or-created outcome from materialization.
/// `materialized_session` 携带物化阶段产生的精确已有或新建结果。
///
/// # Returns
/// 返回值
/// Success after runtime defaults are ready, or the runtime-state initialization error.
/// 运行态默认值准备完成后返回成功，否则返回运行态初始化错误。
pub(crate) fn prepare_materialized_submission_session(
    workspace_snapshot: &mut WorkspaceSnapshot,
    materialized_session: &MaterializedPromptSession,
) -> Result<(), String> {
    // session_id is the single identity receiving runtime defaults.
    // session_id 是接收运行态默认值的唯一身份。
    let session_id = materialized_session.session_id;
    ensure_webui_session_runtime_entry(workspace_snapshot, session_id, None)?;
    Ok(())
}

/// Merges one newly accepted session slice into the retained full workspace without cloning historical sessions.
/// 将一个新接纳的会话切片合并到保留的完整工作区中，且不克隆历史会话。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained workspace displayed by the workbench.
/// `state` 持有工作台展示的权威保留工作区。
/// `workspace_slice` contains only the selected project and newly created session state.
/// `workspace_slice` 只包含所选项目与新建会话状态。
/// `materialized_session` identifies the exact created session and owning project.
/// `materialized_session` 标识精确的新建会话及其所属项目。
///
/// # Returns
/// 返回值
/// Success after the session-scoped fields are committed, or an explicit retained-state mismatch.
/// 会话范围字段提交后返回成功，否则返回明确的保留状态不一致错误。
pub(crate) fn commit_created_submission_workspace_slice(
    state: &SharedServiceBridgeState,
    workspace_slice: &WorkspaceSnapshot,
    materialized_session: &MaterializedPromptSession,
) -> Result<(), String> {
    if materialized_session.kind != PromptSessionMaterializationKind::Created {
        return Err(String::from(
            "Only a created conversation can be committed from a bounded workspace slice.",
        ));
    }
    // session_id is the sole identity copied from the bounded slice.
    // session_id 是从有界切片复制的唯一身份。
    let session_id = materialized_session.session_id;
    let project_id = materialized_session
        .project_id
        .as_deref()
        .ok_or_else(|| format!("Created conversation `{session_id}` has no owning project."))?;
    let session = workspace_session_summary_for_session(workspace_slice, session_id)
        .cloned()
        .ok_or_else(|| format!("Created conversation slice is missing `{session_id}`."))?;
    let messages = workspace_slice
        .session_messages
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let compactions = workspace_slice
        .session_compactions
        .get(&session_id)
        .cloned()
        .unwrap_or_default();
    let runtime_state = workspace_slice
        .session_runtime_state
        .get(&session_id)
        .cloned()
        .ok_or_else(|| format!("Created conversation `{session_id}` has no runtime state."))?;
    let run_phase = workspace_slice
        .session_run_phases
        .get(&session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Preparing);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    // Split disjoint retained fields once so cache retention can observe the committed workspace without cloning it.
    // 一次拆分互不相交的保留字段，使缓存保留逻辑无需克隆即可观察已提交工作区。
    let crate::ServiceBridgeState {
        workspace_snapshot,
        hydrated_conversation_windows,
        ..
    } = &mut *guard;
    let workspace = workspace_snapshot
        .as_mut()
        .ok_or_else(|| String::from("workspace is no longer initialized"))?;
    let project = workspace
        .task_projects
        .iter_mut()
        .find(|candidate| candidate.id == project_id)
        .ok_or_else(|| {
            format!("Project `{project_id}` left the retained workspace before submission commit.")
        })?;
    if workspace
        .sessions
        .iter()
        .any(|candidate| candidate.id == session_id)
    {
        return Err(format!(
            "Created conversation `{session_id}` already exists in the retained workspace."
        ));
    }
    project
        .session_ids
        .retain(|candidate| *candidate != session_id);
    project.session_ids.insert(0, session_id);
    workspace.sessions.insert(0, session);
    workspace.session_messages.insert(session_id, messages);
    if compactions.is_empty() {
        workspace.session_compactions.remove(&session_id);
    } else {
        workspace
            .session_compactions
            .insert(session_id, compactions);
    }
    workspace
        .session_runtime_state
        .insert(session_id, runtime_state);
    workspace.session_run_phases.insert(session_id, run_phase);
    workspace.hydrated_session_ids.insert(session_id);
    workspace.selected_session = session_id;
    crate::retain_hydrated_conversation_windows_for_workspace(
        hydrated_conversation_windows,
        workspace,
    );
    Ok(())
}

/// Merges new or updated transcript records into one retained session by stable message id.
/// 按稳定消息标识把新增或更新的转录记录合并到一个保留会话。
///
/// # Parameters
/// 参数
/// `retained_messages` owns the authoritative ordered transcript.
/// `retained_messages` 持有权威有序转录。
/// `incoming_messages` contains only records created or updated by the detached transaction.
/// `incoming_messages` 仅包含分离事务新建或更新的记录。
///
/// # Returns
/// 返回值
/// This function returns no value after replacing matching ids and appending new ids in transaction order.
/// 此函数按事务顺序替换匹配标识并追加新标识后不返回值。
pub(crate) fn merge_submission_message_entries(
    retained_messages: &mut Vec<SessionMessageEntry>,
    incoming_messages: Vec<SessionMessageEntry>,
) {
    // incoming_message is one transaction-owned record keyed by its stable persisted identity.
    // incoming_message 是一条由事务持有并按稳定持久身份索引的记录。
    for incoming_message in incoming_messages {
        if let Some(retained_message) = retained_messages
            .iter_mut()
            .find(|retained_message| retained_message.id == incoming_message.id)
        {
            *retained_message = incoming_message;
        } else {
            retained_messages.push(incoming_message);
        }
    }
}

/// Merges one admitted existing-session slice into the retained workspace by exact session key.
/// 按精确会话键把单个已准入已有会话切片合并到保留工作区。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative retained full workspace.
/// `state` 持有权威的保留完整工作区。
/// `workspace_slice` contains the admitted active session, its owning project, canonical runtime context, and new transcript records.
/// `workspace_slice` 包含已准入活动会话、所属项目、规范运行时上下文与新增转录记录。
/// `materialized_session` identifies the exact existing session being committed.
/// `materialized_session` 标识正在提交的精确已有会话。
///
/// # Returns
/// 返回值
/// Success after moving session-scoped runtime fields and merging transcript deltas, or an explicit retained-state mismatch.
/// 移动会话级运行字段并合并转录增量后返回成功，否则返回明确的保留状态不一致错误。
pub(crate) fn commit_existing_submission_workspace_slice(
    state: &SharedServiceBridgeState,
    workspace_slice: &mut WorkspaceSnapshot,
    materialized_session: &MaterializedPromptSession,
) -> Result<(), String> {
    if materialized_session.kind != PromptSessionMaterializationKind::Existing {
        return Err(String::from(
            "Only an existing conversation can be committed through the exact-session merge.",
        ));
    }
    let session_id = materialized_session.session_id;
    let session = workspace_session_summary_for_session(workspace_slice, session_id)
        .cloned()
        .ok_or_else(|| format!("Existing conversation slice is missing `{session_id}`."))?;
    // messages owns only transcript deltas created by this detached submission.
    // messages 只持有本次分离提交新建的转录增量。
    let messages = workspace_slice
        .session_messages
        .remove(&session_id)
        .unwrap_or_default();
    // compactions is present only when the detached transaction explicitly changed compaction projection state.
    // compactions 仅在分离事务明确修改压缩投影状态时存在。
    let compactions = workspace_slice.session_compactions.remove(&session_id);
    let runtime_state = workspace_slice
        .session_runtime_state
        .remove(&session_id)
        .ok_or_else(|| format!("Existing conversation `{session_id}` has no runtime state."))?;
    let run_phase = workspace_slice
        .session_run_phases
        .remove(&session_id)
        .unwrap_or(SessionRunPhase::Preparing);
    let is_hydrated = workspace_slice.hydrated_session_ids.contains(&session_id);
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let crate::ServiceBridgeState {
        workspace_snapshot,
        hydrated_conversation_windows,
        ..
    } = &mut *guard;
    let workspace = workspace_snapshot
        .as_mut()
        .ok_or_else(|| String::from("workspace is no longer initialized"))?;
    let retained_session = workspace_session_summary_for_session_mut(workspace, session_id)
        .ok_or_else(|| {
            format!(
                "Existing conversation `{session_id}` left the retained workspace before commit."
            )
        })?;
    *retained_session = session;
    if !messages.is_empty() {
        merge_submission_message_entries(
            workspace.session_messages.entry(session_id).or_default(),
            messages,
        );
    }
    // An absent compaction entry means the detached submission did not mutate compaction state.
    // 缺少压缩条目表示分离提交没有修改压缩状态。
    if let Some(compactions) = compactions {
        workspace
            .session_compactions
            .insert(session_id, compactions);
    }
    workspace
        .session_runtime_state
        .insert(session_id, runtime_state);
    workspace.session_run_phases.insert(session_id, run_phase);
    if is_hydrated {
        workspace.hydrated_session_ids.insert(session_id);
    } else {
        workspace.hydrated_session_ids.remove(&session_id);
    }
    workspace.selected_session = session_id;
    crate::retain_hydrated_conversation_windows_for_workspace(
        hydrated_conversation_windows,
        workspace,
    );
    Ok(())
}

/// Atomically commits one materialized session's sidebar activation after its kernel and workspace commit.
/// 在 kernel 与工作区提交后原子提交一个已物化会话的侧栏激活状态。
///
/// # Parameters
/// 参数
/// `app` emits the sidebar summary delta after shared state is committed.
/// `app` 在共享状态提交后发送侧栏摘要增量。
/// `state` owns the workbench revision and live subagent navigation selection.
/// `state` 持有工作台修订号与实时子代理导航选择。
/// `workspace_snapshot` is the already-committed prompt-turn workspace revision.
/// `workspace_snapshot` 是已经提交提示词回合的工作区版本。
/// `materialized_session` carries the exact existing-or-created outcome from materialization.
/// `materialized_session` 携带物化阶段产生的精确已有或新建结果。
///
/// # Returns
/// 返回值
/// The committed workbench revision, or an error when the summary or shared state is unavailable.
/// 返回已提交的工作台修订号；摘要或共享状态不可用时返回错误。
pub(crate) fn commit_materialized_submission_activation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    workspace_snapshot: &WorkspaceSnapshot,
    materialized_session: &MaterializedPromptSession,
) -> Result<u64, String> {
    // session_id is the single identity shared by committed workspace and live projection.
    // session_id 是已提交工作区与实时投影共享的唯一身份。
    let session_id = materialized_session.session_id;

    // The summary is built from the same workspace revision already accepted by shared state.
    // 摘要从共享状态已经接受的同一工作区版本构建。
    // session is the authoritative summary produced or selected during materialization.
    // session 是物化阶段生成或选择的权威摘要。
    let session = workspace_session_summary_for_session(workspace_snapshot, session_id)
        .ok_or_else(|| format!("submitted session was not materialized: {session_id}"))?;
    // run_phase is the exact local phase projected into the sidebar event.
    // run_phase 是投影到侧栏事件中的精确本地阶段。
    let run_phase = workspace_snapshot
        .session_run_phases
        .get(&session_id)
        .copied()
        .unwrap_or(SessionRunPhase::Idle);
    // is_running combines summary status with the authoritative active-phase classification.
    // is_running 合并摘要状态与权威活动阶段分类。
    let is_running =
        session.status == SessionState::Running || is_active_session_run_phase(run_phase);
    // unread is the session-scoped UI flag retained with runtime state.
    // unread 是随运行态保留的会话级界面标记。
    let unread = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .is_some_and(|runtime_state| runtime_state.ui_state.unread);
    // session_plan_mode supplies pending Plan attention without reading shared workbench state.
    // session_plan_mode 提供待处理计划注意状态，无需读取共享工作台状态。
    let session_plan_mode = session_plan_mode_summary_from_runtime_state(
        workspace_snapshot.session_runtime_state.get(&session_id),
    );
    // conversation is the complete summary value committed to both snapshot projection and event payload.
    // conversation 是同时提交到快照投影与事件载荷的完整摘要值。
    let mut conversation = ConversationSummary {
        id: session_id.to_string(),
        title: session.title.clone(),
        status: if is_running {
            SessionState::Running.label()
        } else {
            session.status.label()
        }
        .to_string(),
        run_phase: run_phase.to_string(),
        last_updated_label: session.last_updated_label.clone(),
        last_updated_at: session.updated_at.clone(),
        unread,
        attention: None,
        project_id: session_summary_project_id_owned(session),
        fork_source: conversation_fork_source_summary_from_session(session),
        continuation: workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .and_then(|runtime_state| {
                conversation_continuation_summary(
                    runtime_state,
                    run_phase,
                    SessionContinuationRuntimeBlockers::default(),
                )
            }),
    };
    // revision and event payloads are produced from the same exclusive shared-state transaction.
    // revision 与事件载荷由同一个独占共享状态事务产生。
    let (revision, delta_event, full_workbench_event) = {
        // guard serializes navigation ownership, attention refresh, and workbench generation.
        // guard 串行化导航所有权、注意状态刷新与工作台代际。
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // Existing-session activation starts a new parent coordination round and closes only that parent's child panel.
        // 已有会话激活会开始新的父协调轮次，并且只关闭该父会话的子任务面板。
        if materialized_session.kind == PromptSessionMaterializationKind::Existing {
            guard.session_subagent_navigation_rounds.remove(&session_id);
            if guard
                .active_subagent_panel
                .is_some_and(|selection| selection.parent_session_id == session_id)
            {
                guard.active_subagent_panel = None;
            }
        }
        // pending_questions is the live ask snapshot observed at the committed revision.
        // pending_questions 是已提交版本观察到的实时提问快照。
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
        refresh_conversation_attention_summary(
            &mut conversation,
            session_id,
            &pending_questions,
            session_plan_mode.as_ref(),
        );
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        // revision is the exact generation embedded in both retained workbench and emitted envelope.
        // revision 是同时写入保留工作台与发送信封的精确代际。
        let revision = guard.workbench_generation;
        guard.workbench.revision = revision;
        guard.workbench.selected_conversation_id = Some(conversation.id.clone());
        if let Some(existing) = guard
            .workbench
            .conversations
            .iter_mut()
            .find(|candidate| candidate.id == conversation.id)
        {
            *existing = conversation.clone();
        } else {
            guard
                .workbench
                .conversations
                .insert(0, conversation.clone());
        }
        if materialized_session.kind == PromptSessionMaterializationKind::Created {
            // A draft-to-session handoff changes both navigation identity and session-scoped model controls.
            // 草稿到会话的交接会同时改变导航身份与会话级模型控制项。
            let selected_runtime_state = guard
                .workspace_snapshot
                .as_ref()
                .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                .cloned();
            if let Some(agent_runtime) = guard.agent_runtime.as_mut() {
                restore_webui_runtime_controls_for_session(
                    &mut agent_runtime.provider_configuration,
                    selected_runtime_state.as_ref(),
                );
            }
            let provider_configuration = guard
                .agent_runtime
                .as_ref()
                .map(|agent_runtime| agent_runtime.provider_configuration.clone());
            if let Some(provider_configuration) = provider_configuration.as_ref() {
                sync_workbench_runtime_control_fields_locked(&mut guard, provider_configuration);
            }
            // The full snapshot makes conversation selection and model projection atomic for the WebUI.
            // 完整快照让会话选中与模型投影在 WebUI 中原子生效。
            (revision, None, Some(guard.workbench.clone()))
        } else {
            // Existing-session activation changes only the conversation summary and remains a compact delta.
            // 已有会话激活只改变会话摘要，继续使用紧凑增量。
            let event = WorkbenchDeltaEnvelope {
                generation: revision,
                delta: WorkbenchDelta::ConversationSummaryChanged(
                    ConversationSummaryChangedDelta {
                        conversation,
                        selected_conversation_id: Some(session_id.to_string()),
                    },
                ),
            };
            (revision, Some(event), None::<WorkbenchSnapshot>)
        }
    };
    if let Some(workbench) = full_workbench_event {
        emit_workbench_event(app, &workbench);
    }
    if let Some(event) = delta_event {
        emit_service_event_with_trace(
            app,
            "workbench_conversation_summary_delta_emit",
            ServiceEvent::WorkbenchDelta(Box::new(event)),
        );
    }
    Ok(revision)
}
