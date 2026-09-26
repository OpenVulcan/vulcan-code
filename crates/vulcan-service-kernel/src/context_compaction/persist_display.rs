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

/// Persists one committed compaction snapshot and reports whether the durable barrier completed.
/// 持久化一次已提交的压缩快照，并报告持久化屏障是否完成。
///
/// The automatic continuation path awaits this function so a rebuilt request can never race the
/// transcript write that establishes the new context boundary.
/// 自动续接路径会等待此函数，确保重建请求不会与建立新上下文边界的转录写入竞态。
pub(crate) async fn persist_context_compaction_snapshot(
    state: SharedServiceBridgeState,
    _controller_runtime: HostRuntimeHandle,
    workspace_snapshot: WorkspaceSnapshot,
    session_id: SessionId,
    _section_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    if !workspace_snapshot_contains_regular_session(&workspace_snapshot, session_id) {
        return Ok(());
    }
    // The V1 handle is the sole durable owner of compaction, runtime-lock, and selection facts.
    // V1 handle 是压缩、运行时锁定与选择配置事实的唯一持久化所有者。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(&state, session_id)?;
    handle.flush_all().map_err(|error| {
        let reason_label = reason.as_deref().unwrap_or("context_compaction");
        format!("V1 context-compaction flush failed ({reason_label}): {error}")
    })
}

/// Spawns a background compaction persistence task for non-blocking manual flows.
/// 为不阻塞手动流程启动后台压缩持久化任务。
pub(crate) fn spawn_context_compaction_persist_task(
    state: SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
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
            tracing::warn!("[webui-service] context-compaction persistence failed: {error}");
        }
    });
}
