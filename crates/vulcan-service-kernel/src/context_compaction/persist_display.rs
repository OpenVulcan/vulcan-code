//! Context compaction persist display helpers.
//! 上下文压缩 persist display 辅助逻辑。

use super::*;
use crate::{
    next_webui_session_addon_persist_version, persist_workspace_session_addon_state_update,
    persist_workspace_session_update_unlocked, webui_session_addon_persist_versions_are_current,
    workspace_session_with_archive_status,
};
use vulcan_workbench_model::{SessionAddonNamespace, SessionAddonState};

/// Builds one usage segment from a completed context-compaction execution.
/// 从已完成的上下文压缩执行结果构建一段用量统计。
///
/// # Parameters
/// # 参数
/// `trigger` is the stable compaction trigger label stored in metadata.
/// `trigger` 是写入元数据的稳定压缩触发标签。
/// `execution` is the completed compaction execution carrying provider usage.
/// `execution` 是携带 Provider 用量的已完成压缩执行结果。
///
/// # Returns
/// # 返回值
/// Returns one context-compaction usage segment ready to merge into turn statistics.
/// 返回一段可合并进轮次统计的上下文压缩用量。
pub(crate) fn context_compaction_usage_segment_from_execution(
    trigger: &str,
    execution: &PromptCompactionExecution,
) -> SessionTurnUsageSegment {
    let mut segment = SessionTurnUsageSegment::from_provider_usage(
        SessionTurnUsageSegmentKind::ContextCompaction,
        execution.provider_request_count,
        execution.last_successful_provider_usage.as_ref(),
    );
    segment.trigger = Some(trigger.to_string());
    segment.execution_profile = Some(execution.execution_profile.label().to_string());
    segment.provider_id = execution.provider_id.clone();
    segment.model_id = execution.model_id.clone();
    segment
}

/// Persists one committed compaction snapshot and reports whether the durable barrier completed.
/// 持久化一次已提交的压缩快照，并报告持久化屏障是否完成。
///
/// The automatic continuation path awaits this function so a rebuilt request can never race the
/// transcript write that establishes the new context boundary.
/// 自动续接路径会等待此函数，确保重建请求不会与建立新上下文边界的转录写入竞态。
pub(crate) async fn persist_context_compaction_snapshot(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    section_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return Ok(());
    }
    let output_style_persist_version = workspace_snapshot
        .session_runtime_state
        .get(&session_id)
        .or_else(|| {
            workspace_snapshot
                .archived_session_runtime_state
                .get(&session_id)
        })
        .filter(|runtime_state| !runtime_state.output_style.is_empty())
        .map(|_| {
            next_webui_session_addon_persist_version(
                &state,
                session_id,
                SessionAddonNamespace::OutputStyle,
            )
        });
    let output_style_persist_version = match output_style_persist_version {
        Some(Ok(version)) => Some(version),
        Some(Err(error)) => {
            return Err(format!(
                "context-compaction output-style persist version failed: {error}"
            ));
        }
        None => None,
    };
    // session_persist_version is the shared runtime/transcript ticket for this snapshot.
    // session_persist_version 是此快照共享的运行态/转录持久化票据。
    let session_persist_version =
        match next_webui_session_runtime_persist_version(&state, session_id) {
            Ok(version) => version,
            Err(error) => {
                return Err(format!(
                    "context-compaction persist version failed: {error}"
                ));
            }
        };
    let persist_lock = webui_session_persist_lock(session_id);
    let _persist_guard = persist_lock.lock().await;
    if !webui_session_runtime_persist_version_is_current(
        &state,
        session_id,
        session_persist_version,
    ) {
        return Err(format!(
            "context-compaction persistence was superseded before write: session={session_id}"
        ));
    }
    let display_messages =
        context_compaction_complete_display_messages(&workspace_snapshot, session_id, &section_id);
    // persistenceSnapshot is already owned by this task, so its exact session row can move without
    // cloning the workspace or canonical conversation history a second time.
    // persistenceSnapshot 已由当前任务独占，因此其精确会话行可以直接移动，无需再次复制工作区
    // 或规范会话历史。
    let mut persistence_snapshot = workspace_snapshot;
    let archived = workspace_session_with_archive_status(&persistence_snapshot, session_id)
        .map(|(_, archived)| archived)
        .ok_or_else(|| format!("context-compaction session disappeared: {session_id}"))?;
    let runtime_state = if archived {
        persistence_snapshot
            .archived_session_runtime_state
            .remove(&session_id)
    } else {
        persistence_snapshot
            .session_runtime_state
            .remove(&session_id)
    };
    let Some(mut runtime_state) = runtime_state else {
        return Err(format!(
            "context-compaction runtime state is missing: session={session_id}"
        ));
    };
    // current_output_style preserves a newer user selection made after the compaction snapshot.
    // current_output_style 确保压缩快照之后发生的用户风格切换不会被后台任务覆盖。
    let current_output_style = match state.lock() {
        Ok(guard) => guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| {
                workspace
                    .session_runtime_state
                    .get(&session_id)
                    .or_else(|| workspace.archived_session_runtime_state.get(&session_id))
            })
            .map(|current_runtime_state| current_runtime_state.output_style.clone()),
        Err(error) => {
            return Err(format!(
                "context-compaction current output-style read failed: {error}"
            ));
        }
    };
    if let Some(current_output_style) = current_output_style {
        runtime_state.output_style = current_output_style;
    }
    // outputStyleAddonState is constant-size and captured before runtimeState moves back into its exact map.
    // outputStyleAddonState 为常量大小，并在 runtimeState 移回精确映射前完成捕获。
    let output_style_addon_state = if runtime_state.output_style.is_empty() {
        None
    } else {
        let mut addon_state = SessionAddonState::new(session_id);
        if !addon_state.set_typed_namespace(
            SessionAddonNamespace::OutputStyle,
            &runtime_state.output_style,
            runtime_state.updated_at.clone(),
        ) {
            return Err(String::from(
                "context-compaction output-style state build failed",
            ));
        }
        Some(addon_state)
    };
    // Compose the exact transcript and moved runtime state into one session snapshot before persistence.
    // 在持久化前将精确转录与移动后的运行态组合成同一份会话快照。
    if archived {
        persistence_snapshot
            .archived_session_messages
            .insert(session_id, display_messages);
        persistence_snapshot
            .archived_session_runtime_state
            .insert(session_id, runtime_state);
    } else {
        persistence_snapshot
            .session_messages
            .insert(session_id, display_messages);
        persistence_snapshot
            .session_runtime_state
            .insert(session_id, runtime_state);
    }
    // The workspace-store single-session API writes segmented transcript, runtime state, and its
    // locator in one ordered operation, so a rebuilt turn cannot observe a half-written boundary.
    // workspace-store 单会话 API 按顺序一次写入转录、运行态和定位行，重建回合不会读到半写边界。
    persist_workspace_session_update_unlocked(
        controller_runtime.clone(),
        &persistence_snapshot,
        session_id,
    )
    .await
    .map_err(|error| {
        let reason_label = reason.as_deref().unwrap_or("context_compaction");
        format!("background context-compaction session persist failed ({reason_label}): {error}")
    })?;
    if let Some(addon_state) = output_style_addon_state {
        let output_style_persist_is_current = output_style_persist_version
            .map(|version| {
                webui_session_addon_persist_versions_are_current(
                    &state,
                    session_id,
                    &[(SessionAddonNamespace::OutputStyle, version)],
                )
            })
            .unwrap_or(true);
        if !output_style_persist_is_current {
            return Err(format!(
                "context-compaction output-style persistence was superseded: session={session_id}"
            ));
        }
        persist_workspace_session_addon_state_update(
            &state,
            controller_runtime,
            session_id,
            addon_state,
        )
        .await
        .map_err(|error| {
            let reason_label = reason.as_deref().unwrap_or("context_compaction");
            format!(
                "background context-compaction output-style addon persist failed ({reason_label}): {error}"
            )
        })?;
    }
    Ok(())
}

/// Spawns a background compaction persistence task for non-blocking manual flows.
/// 为不阻塞手动流程启动后台压缩持久化任务。
pub(crate) fn spawn_context_compaction_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    section_id: String,
    reason: Option<String>,
) {
    spawn_service_task(async move {
        if let Err(error) = persist_context_compaction_snapshot(
            state,
            controller_runtime,
            workspace_snapshot,
            session_id,
            section_id,
            reason,
        )
        .await
        {
            eprintln!("[webui-service] context-compaction persistence failed: {error}");
        }
    });
}

/// Returns the complete visible display window that must survive runtime compaction.
/// 返回必须在运行态压缩后仍可保留的完整可见显示窗口。
pub(crate) fn context_compaction_complete_display_messages(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    section_id: &str,
) -> Vec<SessionMessageEntry> {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .or_else(|| {
            workspace_snapshot
                .archived_session_messages
                .get(&session_id)
        })
        .cloned()
        .filter(|messages| !messages.is_empty())
        .unwrap_or_else(|| {
            context_compaction_section_display_messages(workspace_snapshot, session_id, section_id)
        })
}

/// Returns the visible display rows that belong to one context-compaction section.
/// 返回归属于单个上下文压缩分段的可见显示行。
pub(crate) fn context_compaction_section_display_messages(
    workspace_snapshot: &WorkspaceSnapshot,
    session_id: SessionId,
    section_id: &str,
) -> Vec<SessionMessageEntry> {
    workspace_snapshot
        .session_messages
        .get(&session_id)
        .into_iter()
        .flat_map(|messages| messages.iter())
        .filter(|message| context_compaction_message_belongs_to_section(message, section_id))
        .cloned()
        .collect()
}

/// Returns whether one display row belongs to the requested context-compaction section.
/// 返回单条显示行是否归属于请求的上下文压缩分段。
pub(crate) fn context_compaction_message_belongs_to_section(
    message: &SessionMessageEntry,
    section_id: &str,
) -> bool {
    message.id == section_id
        || message
            .metadata
            .as_ref()
            .and_then(|metadata| metadata.get("context_compaction_section_id"))
            .and_then(Value::as_str)
            == Some(section_id)
}
