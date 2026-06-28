//! Context compaction completion helpers.
//! 上下文压缩 completion 辅助逻辑。

use super::*;

/// Applies the terminal compaction result, writes its checkpoint, and closes the visible status section.
/// 应用终态压缩结果、写入检查点，并关闭可见状态分段。
pub(crate) fn finish_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    execution: PromptCompactionExecution,
) -> Result<WorkspaceSnapshot, String> {
    if execution.fallback_used {
        let retry_hint = if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
            "自动压缩未完成，已停止本次请求以避免预算溢出。"
        } else {
            "请重新触发 /compact。"
        };
        return fail_webui_context_compaction_execution(
            app,
            state,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            &format!(
                "API 未返回可用压缩结果，本次上下文压缩已失败。Provider 请求：{}。{retry_hint}",
                execution.provider_request_count,
            ),
        );
    }
    let summary_body = execution.compaction_result.summary_record.body.clone();
    let provider_request_count = execution.provider_request_count;
    let runtime_backup_controller = state
        .lock()
        .ok()
        .and_then(|guard| guard.controller_runtime.clone());
    if let Some(controller_runtime) = runtime_backup_controller.as_ref() {
        match vulcan_workspace_store::workspace_store::backup_session_runtime_log_before_compaction(
            &controller_runtime.data_root,
            session_id,
            &execution.compaction_result.compaction_id,
            &execution.compaction_result.created_at,
        ) {
            Ok(copied_files) if !copied_files.is_empty() => {
                webui_trace(
                    "context_compaction_runtime_backup",
                    format!("session={session_id} files={}", copied_files.len()),
                );
            }
            Ok(_) => {}
            Err(error) => {
                webui_trace(
                    "context_compaction_runtime_backup_failed",
                    format!("session={session_id} error={error}"),
                );
            }
        }
    }
    let mut injection_merge_event = None;
    let mut injection_merge_timestamp = String::new();
    let mut ttl_prune_report = InjectionPruneReport::default();
    let mut draft_error: Option<String> = None;
    let workspace =
        with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
            let now = current_unix_millis_text();
            let runtime_state = workspace
                .session_runtime_state
                .entry(session_id)
                .or_insert_with(|| SessionRuntimeStateEntry {
                    updated_at: now.clone(),
                    ..SessionRuntimeStateEntry::default()
                });
            let draft = match build_webui_context_compaction_draft(
                runtime_state,
                session_id,
                section_id,
                budget_note,
                trigger,
                provider_request_count,
                &summary_body,
                &execution,
            ) {
                Ok(draft) => draft,
                Err(error) => {
                    draft_error = Some(error);
                    return;
                }
            };
            if !draft.baseline.matches(runtime_state) {
                draft_error = Some(String::from(
                    "上下文压缩候选状态基线失配，已保留当前会话状态并取消提交。",
                ));
                return;
            }
            injection_merge_event = draft.injection_merge_event.clone();
            injection_merge_timestamp = draft.injection_merge_timestamp.clone();
            ttl_prune_report = draft.ttl_prune_report;
            let terminal_record_id = draft.terminal_record_id.clone();
            let checkpoint = draft.checkpoint;
            *runtime_state = draft.runtime_state;
            if trigger != AUTO_CONTEXT_COMPACTION_TRIGGER {
                let usage_segment =
                    context_compaction_usage_segment_from_execution(trigger, &execution);
                let _ = append_webui_session_turn_stats_to_state(
                    runtime_state,
                    session_id,
                    &terminal_record_id,
                    SessionTurnStatsStatus::Completed,
                    None,
                    SessionTurnStatsKind::ContextCompaction,
                    vec![usage_segment],
                );
            }
            workspace
                .session_compactions
                .entry(session_id)
                .or_default()
                .push(checkpoint);
            let runtime_state_snapshot = runtime_state.clone();
            merge_runtime_state_records_into_display_messages(
                workspace,
                session_id,
                &runtime_state_snapshot,
            );
            workspace
                .session_run_phases
                .insert(session_id, previous_phase);
            if let Some(session) = workspace
                .sessions
                .iter_mut()
                .find(|session| session.id == session_id)
            {
                session.status = SessionState::Completed;
                session.last_updated_label = String::from("刚刚");
                session.updated_at = current_unix_millis_text();
            }
            sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
            agent_runtime
                .vmm_hook_runtime
                .replace_snapshot(build_vmm_hook_config_snapshot(
                    &agent_runtime.provider_configuration,
                    workspace,
                ));
        })?;
    if let Some(error) = draft_error {
        return fail_webui_context_compaction_execution(
            app,
            state,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            &error,
        );
    }
    if ttl_prune_report.removed_item_count > 0 || ttl_prune_report.removed_group_count > 0 {
        webui_trace(
            "context_compaction_pruned_expired_injections",
            format!(
                "session={session_id} items={} groups={}",
                ttl_prune_report.removed_item_count, ttl_prune_report.removed_group_count
            ),
        );
    }
    if let (Some(controller_runtime), Some(event)) =
        (runtime_backup_controller.as_ref(), injection_merge_event)
        && let Err(error) = record_webui_runtime_injection_merged(
            controller_runtime,
            session_id,
            event,
            &injection_merge_timestamp,
        )
    {
        webui_trace(
            "runtime_injection_merged_audit_failed",
            format!("session={session_id} error={error}"),
        );
    }
    if let Ok(mut hook_runtime) = agent_runtime.conversation_hook_runtime.lock() {
        hook_runtime.record_compacted(
            session_id,
            owning_project_id_for_session(&workspace, session_id),
            trigger,
            false,
            provider_request_count,
            &summary_body,
        );
    }
    Ok(workspace)
}

/// Records a failed manual context-compaction attempt without moving the conversation anchor.
/// 记录失败的手动上下文压缩尝试，并且不移动会话上下文锚点。
pub(crate) fn fail_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) -> Result<WorkspaceSnapshot, String> {
    with_workspace_mutation_preserving_subagent_navigation(app, state, |workspace| {
        apply_webui_context_compaction_failure_to_workspace(
            workspace,
            session_id,
            previous_phase,
            section_id,
            budget_note,
            trigger,
            error,
        );
    })
}

/// Applies failed context-compaction state inside one retained workspace snapshot.
/// 在单个保留的工作区快照内应用上下文压缩失败状态。
pub(crate) fn apply_webui_context_compaction_failure_to_workspace(
    workspace: &mut WorkspaceSnapshot,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
    error: &str,
) {
    let now = current_unix_millis_text();
    let runtime_state = workspace
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: now.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    set_webui_context_compaction_section_metadata_in_state(
        runtime_state,
        section_id,
        section_id,
        "start",
        "failed",
        Some(budget_note),
        trigger,
    );
    update_webui_context_compaction_body_state(runtime_state, section_id, "failed");
    append_webui_context_compaction_failed_message_to_state(
        runtime_state,
        session_id,
        section_id,
        budget_note,
        trigger,
        error,
    );
    if trigger == AUTO_CONTEXT_COMPACTION_TRIGGER {
        runtime_state.auto_compaction_consecutive_failures = runtime_state
            .auto_compaction_consecutive_failures
            .saturating_add(1);
    }
    runtime_state.updated_at = now;
    let runtime_state_snapshot = runtime_state.clone();
    merge_runtime_state_records_into_display_messages(
        workspace,
        session_id,
        &runtime_state_snapshot,
    );
    workspace
        .session_run_phases
        .insert(session_id, previous_phase);
    if let Some(session) = workspace
        .sessions
        .iter_mut()
        .find(|session| session.id == session_id)
    {
        session.status = SessionState::Completed;
        session.last_updated_label = String::from("刚刚");
        session.updated_at = current_unix_millis_text();
    }
    sync_hydrated_session_dialogue_ready_marker(workspace, session_id);
}
