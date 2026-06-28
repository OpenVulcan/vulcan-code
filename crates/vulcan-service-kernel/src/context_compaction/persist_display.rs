//! Context compaction persist display helpers.
//! 上下文压缩 persist display 辅助逻辑。

use super::*;

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

pub(crate) fn spawn_context_compaction_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: ControllerRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    section_id: String,
    reason: Option<String>,
) {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return;
    }
    let persist_version = match next_webui_session_persist_version(&state, session_id) {
        Ok(version) => version,
        Err(error) => {
            eprintln!("[webui-service] context-compaction persist version failed: {error}");
            return;
        }
    };
    spawn_service_task(async move {
        let persist_lock = webui_workspace_persist_lock();
        let _persist_guard = persist_lock.lock().await;
        if !webui_session_persist_version_is_current(&state, session_id, persist_version) {
            return;
        }
        let display_messages = context_compaction_complete_display_messages(
            &workspace_snapshot,
            session_id,
            &section_id,
        );
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_transcript_messages(
                controller_runtime.clone(),
                session_id,
                &display_messages,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("context_compaction");
            eprintln!(
                "[webui-service] background context-compaction transcript persist failed ({reason_label}): {error}"
            );
        }
        let Some(runtime_state) = workspace_snapshot
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .or_else(|| {
                workspace_snapshot
                    .archived_session_runtime_state
                    .get(&session_id)
                    .cloned()
            })
        else {
            return;
        };
        if let Err(error) =
            vulcan_workspace_store::workspace_store::persist_single_session_runtime_state_update(
                controller_runtime,
                session_id,
                &runtime_state,
            )
            .await
        {
            let reason_label = reason.as_deref().unwrap_or("context_compaction");
            eprintln!(
                "[webui-service] background context-compaction runtime-state persist failed ({reason_label}): {error}"
            );
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
