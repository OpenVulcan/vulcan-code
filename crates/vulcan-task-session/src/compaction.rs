use crate::model::TaskSessionId;
use vulcan_session_core::{
    SessionCompactionCheckpoint, SessionContextCompactionRequest, SessionContextDocument,
    SessionStoreError,
};

/// Result produced after applying one compaction result to a TaskSession context document.
/// 将一次压缩结果应用到 TaskSession 上下文文档后产出的结果。
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSessionCompactionApplication {
    /// Compacted API-facing context document ready to be saved as context/current.json.
    /// 准备保存为 context/current.json 的压缩后 API 实际上下文文档。
    pub context: SessionContextDocument,
    /// File-backed checkpoint describing the applied compaction boundary.
    /// 描述已应用压缩边界的文件态检查点。
    pub checkpoint: SessionCompactionCheckpoint,
}

/// Task-session compaction job stored outside the UI layer.
/// 保存在 UI 层之外的子代理会话压缩任务。
#[derive(Debug, Clone, PartialEq)]
pub struct TaskSessionCompactionJob {
    /// Task session identifier.
    /// 子代理会话标识符。
    pub task_session_id: TaskSessionId,
    /// Shared compaction request.
    /// 共享压缩请求。
    pub request: SessionContextCompactionRequest,
}

/// Store contract for task-session compaction artifacts.
/// 子代理会话压缩产物的存储契约。
pub trait TaskSessionCompactionStore: Send + Sync {
    /// Saves a compacted context document for a task session.
    /// 保存子代理会话的压缩后上下文文档。
    fn save_compacted_context(
        &self,
        context: SessionContextDocument,
    ) -> Result<(), SessionStoreError>;

    /// Saves a compaction checkpoint for a task session.
    /// 保存子代理会话的压缩检查点。
    fn save_compaction_checkpoint(
        &self,
        checkpoint: SessionCompactionCheckpoint,
    ) -> Result<(), SessionStoreError>;
}

/// Applies one shared compaction result to a TaskSession context document without touching GUI state or database projection.
/// 将共享压缩结果应用到 TaskSession 上下文文档，不触碰 GUI 状态或数据库投影。
pub fn apply_task_session_compaction_result(
    mut context: SessionContextDocument,
    compaction_result: &vulcan_context_compaction::CompactionResult,
    through_sequence: u64,
) -> Result<TaskSessionCompactionApplication, SessionStoreError> {
    let summary_message_id = task_session_compaction_summary_message_id(compaction_result);
    let timestamp_ms = compaction_result
        .created_at
        .parse::<u64>()
        .unwrap_or_default();
    let mut conversation_state: vulcan_context::SessionConversationState =
        serde_json::from_value(context.conversation_state.clone()).map_err(to_store_error)?;

    vulcan_context_compaction::compact_conversation_with_summary(
        &mut conversation_state,
        summary_message_id.clone(),
        compaction_result.summary_record.kind,
        compaction_result.summary_record.body.clone(),
        compaction_result.created_at.clone(),
        compaction_result.policy,
    );
    vulcan_context_compaction::compact_conversation_records_after_summary(
        &mut conversation_state,
        &summary_message_id,
        compaction_result.policy,
        keep_change_set_host_result,
    );

    context.conversation_state =
        serde_json::to_value(conversation_state).map_err(to_store_error)?;
    context.compaction_anchor = Some(summary_message_id);
    let checkpoint = SessionCompactionCheckpoint {
        checkpoint_id: compaction_result.compaction_id.clone(),
        node_id: context.node_id,
        through_sequence,
        timestamp_ms,
        summary: compaction_result.summary_record.body.clone(),
    };

    Ok(TaskSessionCompactionApplication {
        context,
        checkpoint,
    })
}

/// Builds the stable synthetic summary message id used inside a TaskSession context document.
/// 构建 TaskSession 上下文文档内部使用的稳定合成摘要消息标识。
fn task_session_compaction_summary_message_id(
    compaction_result: &vulcan_context_compaction::CompactionResult,
) -> String {
    format!("task-summary-{}", compaction_result.compaction_id)
}

/// Keeps change-set host-result payloads unchanged when TaskSession runs generic record compaction.
/// TaskSession 执行通用记录压缩时保持 change-set 宿主结果载荷不变。
fn keep_change_set_host_result(value: &serde_json::Value) -> Option<serde_json::Value> {
    Some(value.clone())
}

/// Converts a lower-level serialization error into a session-store error.
/// 将底层序列化错误转换为会话存储错误。
fn to_store_error(error: impl std::fmt::Display) -> SessionStoreError {
    SessionStoreError::new(error.to_string())
}
