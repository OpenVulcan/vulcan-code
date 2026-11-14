//! Publishes one recovered session into the retained workbench.
//! 将单个恢复会话发布到保留的工作台。

use super::*;
use crate::session_runtime_registry_v1::SessionRuntimeRegistry;
use vulcan_service_api::ConversationSummaryChangedDelta;

/// Reads the indexed `session_id` with `controller` and `registry`, then updates `state` and emits through `app`.
/// 用 `controller` 和 `registry` 读取已索引的 `session_id`，再更新 `state` 并通过 `app` 发出事件。
/// Returns after the session is visible, or reports a missing owner or projection failure.
/// 会话可见后返回，所属项目缺失或投影失败时返回错误。
pub(crate) async fn refresh_recovered_session(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
) -> Result<(), String> {
    // Reuse the normal index projection, including sidebar ordering and unread facts.
    // 复用正常的索引投影，包括侧栏排序和未读信息。
    let indexed =
        vulcan_workspace_store::workspace_store::load_workspace_index_snapshot(controller.clone())
            .await?;
    publish_indexed_recovered_session(app, state, controller, registry, session_id, indexed).await
}

/// Publishes `session_id` from `indexed` using `controller` and `registry`, committing to `state` before notifying `app`.
/// 通过 `controller` 和 `registry` 发布 `indexed` 中的 `session_id`，提交到 `state` 后通知 `app`。
/// Returns the result of the current ownership check and exact conversation update.
/// 返回当前归属校验及精确会话更新的结果。
pub(crate) async fn publish_indexed_recovered_session(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller: &HostRuntimeHandle,
    registry: &SessionRuntimeRegistry,
    session_id: SessionId,
    indexed: WorkspaceSnapshot,
) -> Result<(), String> {
    // Only the recovered session is carried forward from this read.
    // 本次读取仅向后续提交传递恢复会话。
    let session = workspace_session_summary_for_session(&indexed, session_id)
        .cloned()
        .ok_or_else(|| format!("Recovered session `{session_id}` is no longer active."))?;
    // Hold the session lifecycle guard from the active-state recheck through the retained commit.
    // 从活动状态复核到保留状态提交完成，持续持有会话生命周期守卫。
    let model = crate::session_model_projection_v1::read_active_session_model_projection_v1(
        controller, registry, session_id,
    )
    .await?;

    // Build the new runtime entry without mutating retained state on a metadata error.
    // 在构建新运行态条目时，元数据错误不会改动保留状态。
    let mut runtime_states = indexed.session_runtime_state;
    model.apply(registry, &mut runtime_states)?;
    // The model projection inserts the exact session adapter even for a cold retry.
    // 即使冷重试，模型投影也会写入精确会话的适配器。
    let runtime = runtime_states
        .remove(&session_id)
        .ok_or_else(|| format!("Recovered session `{session_id}` has no model projection."))?;
    // A cold indexed session may have no resident usage projection.
    // 尚未加载的索引会话可以没有常驻用量投影。
    let usage = session_provider_usage_summaries_from_v1_registry(registry, [session_id])?
        .remove(&session_id);
    publish_recovered_session_summary(app, state, session, runtime, usage)
}

/// Commits `session` and `runtime` under their current owning project in `state`, then emits through `app`.
/// 在 `state` 中按当前所属项目提交 `session` 与 `runtime`，再通过 `app` 发出事件。
/// `usage` is the optional resident usage projection. Returns an error before insertion if the project is gone.
/// `usage` 是可选的常驻用量投影。项目消失时在写入前返回错误。
fn publish_recovered_session_summary(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session: SessionSummary,
    runtime: SessionRuntimeStateEntry,
    usage: Option<SessionProviderUsageSummary>,
) -> Result<(), String> {
    // Only this session can be inserted or updated by recovery.
    // 恢复仅可插入或更新此会话。
    let session_id = session.id;
    // The indexed summary binds the session to exactly one project.
    // 索引摘要将会话绑定到唯一项目。
    let project_id = session_summary_project_id(&session)
        .ok_or_else(|| format!("Recovered session `{session_id}` has no owning project."))?;
    // The lock assigns a generation to the exact mutation, after checking current project ownership.
    // 加锁后二次检查当前项目归属，并为精确变更分配版本号。
    let mut guard = state.lock().map_err(|error| error.to_string())?;
    // Ask attention can change independently of the index, so use the current runtime.
    // 提问状态可以独立于索引变化，因此读取当前运行态。
    let pending_questions =
        pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
    // Modify the current workbench membership in place.
    // 原地更新当前工作台成员关系。
    let workspace = guard
        .workspace_snapshot
        .as_mut()
        .ok_or_else(|| String::from("The workspace snapshot is not loaded yet."))?;
    // Current membership is authoritative when a project was removed during the read.
    // 读取期间发生项目移除时，以当前成员关系为准。
    let project = workspace
        .task_projects
        .iter_mut()
        .find(|project| project.id == project_id)
        .ok_or_else(|| format!("Project `{project_id}` is no longer active."))?;
    // Keep existing projection and navigation after an idempotent retry or a concurrent discovery.
    // 幂等重试或并发刷新已发现该会话时，保留现有投影与导航。
    if !workspace
        .sessions
        .iter()
        .any(|existing| existing.id == session_id)
    {
        if !project.session_ids.contains(&session_id) {
            project.session_ids.insert(0, session_id);
        }
        workspace.sessions.insert(0, session);
        workspace
            .session_run_phases
            .insert(session_id, SessionRunPhase::Idle);
    }
    workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert(runtime);
    // Use the latest retained row, so a retry cannot overwrite a renamed or running conversation.
    // 使用当前保留行，避免重试覆盖已重命名或正在运行的会话。
    let session = workspace_session_summary_for_session(workspace, session_id)
        .expect("recovery added or retained the session");
    // The delta and cache use an identical summary constructed from current state.
    // 增量和缓存共用一份根据当前状态构造的摘要。
    let conversation = workbench_conversation_summary(workspace, session, &pending_questions);
    // Preserve the same default project-layer VMM controls as a complete index refresh.
    // 保留与完整索引刷新相同的默认项目层 VMM 控件。
    let project_vmm_layer = cached_project_vmm_layer_for_session(workspace, session);
    if let Some(existing) = guard
        .workbench
        .conversations
        .iter_mut()
        .find(|row| row.id == conversation.id)
    {
        *existing = conversation.clone();
    } else {
        guard
            .workbench
            .conversations
            .insert(0, conversation.clone());
        // A newly recovered draft inherits the current global and project policy.
        // 新恢复草稿继承当前系统及项目策略。
        let vmm = guard.agent_runtime.as_ref().map_or_else(
            || {
                session_vmm_policy_summary_from_layers(
                    &Default::default(),
                    project_vmm_layer.as_ref(),
                    None,
                )
            },
            |agent| {
                session_vmm_policy_summary_from_layers(
                    &agent.provider_configuration.vmm,
                    project_vmm_layer.as_ref(),
                    None,
                )
            },
        );
        guard
            .workbench
            .session_vmm_policies_by_conversation
            .push(SessionVmmPolicyByConversation {
                conversation_id: conversation.id.clone(),
                session_vmm_policy: vmm,
            });
        if let Some(addons) = session_addons_changed_delta_for_retained_list(
            conversation.id.clone(),
            None,
            None,
            None,
            None,
            None,
            None,
            usage,
        ) {
            guard.workbench.session_addons_by_conversation.push(addons);
        }
    }
    guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
    guard.workbench.revision = guard.workbench_generation;
    // Invalidate generic rebuilds reading the index before this exact publication.
    // 使本次精确发布前读取索引的通用重建失效。
    let epoch = &mut guard.service_lifecycle_state.recovery_publication_epoch;
    *epoch = epoch.wrapping_add(1);
    // A conversation delta leaves the client's other live streams and its current selection intact.
    // 会话增量保留客户端其他实时流和当前选择。
    let event = ServiceEvent::WorkbenchDelta(Box::new(WorkbenchDeltaEnvelope {
        generation: guard.workbench_generation,
        delta: WorkbenchDelta::ConversationSummaryChanged(ConversationSummaryChangedDelta {
            conversation,
            selected_conversation_id: None,
        }),
    }));
    drop(guard);
    emit_service_event_with_trace(app, "workbench_recovered_session_delta_emit", event);
    Ok(())
}
