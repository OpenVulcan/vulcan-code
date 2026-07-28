//! Post-materialization activation for immediate conversation submissions.
//! 即时会话提交的物化后激活。

use vulcan_service_api::{
    ConversationSummary, ConversationSummaryChangedDelta, ServiceEvent, ServiceEventEmitter,
    WorkbenchDelta, WorkbenchDeltaEnvelope,
};
use vulcan_workbench_model::{SessionRunPhase, SessionState};
use vulcan_workspace_store::workspace_store::WorkspaceSnapshot;

use crate::{
    MaterializedPromptSession, PromptSessionMaterializationKind, SharedServiceBridgeState,
    conversation_fork_source_summary_from_session, emit_service_event_with_trace,
    ensure_webui_session_runtime_state, is_active_session_run_phase,
    pending_question_snapshots_from_agent_runtime, refresh_conversation_attention_summary,
    session_plan_mode_summary_from_runtime_state, session_summary_project_id_owned,
    workspace_session_summary_for_session,
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
    ensure_webui_session_runtime_state(workspace_snapshot, session_id, None)?;
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
    };
    // revision and event are produced from the same exclusive shared-state transaction.
    // revision 与 event 由同一个独占共享状态事务产生。
    let (revision, event) = {
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
        // event mirrors the already-committed conversation summary and selection.
        // event 镜像已经提交的会话摘要与选择状态。
        let event = WorkbenchDeltaEnvelope {
            generation: revision,
            delta: WorkbenchDelta::ConversationSummaryChanged(ConversationSummaryChangedDelta {
                conversation,
                selected_conversation_id: Some(session_id.to_string()),
            }),
        };
        (revision, event)
    };
    emit_service_event_with_trace(
        app,
        "workbench_conversation_summary_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(revision)
}
