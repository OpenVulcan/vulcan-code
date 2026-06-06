use crate::model::SessionNodeId;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// API-facing context document used to resume one session node.
/// 用于恢复单个会话节点的 API 实际上下文文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionContextDocument {
    /// Schema version for the context document.
    /// 上下文文档的结构版本。
    pub schema_version: u32,
    /// Owning session node identifier.
    /// 所属会话节点标识符。
    pub node_id: SessionNodeId,
    /// Provider identifier frozen for this context.
    /// 该上下文固化的供应商标识符。
    pub provider_id: Option<String>,
    /// Model identifier frozen for this context.
    /// 该上下文固化的模型标识符。
    pub model_id: Option<String>,
    /// Reasoning effort or profile frozen for this context.
    /// 该上下文固化的推理等级或配置。
    pub reasoning_effort: Option<String>,
    /// Ordered system prompt layers.
    /// 有序系统提示词层。
    pub system_prompt_layers: Vec<String>,
    /// Provider conversation state used for the next request.
    /// 下一次请求使用的供应商会话状态。
    pub conversation_state: Value,
    /// Optional compaction anchor identifier.
    /// 可选的压缩锚点标识符。
    pub compaction_anchor: Option<String>,
    /// Fingerprint for context budget decisions.
    /// 用于上下文预算判断的指纹。
    pub context_budget_fingerprint: Option<String>,
    /// Tool catalog snapshot visible to the node.
    /// 该节点可见的工具目录快照。
    pub tool_catalog_snapshot: Value,
    /// Optional provider request preview.
    /// 可选的供应商请求预览。
    pub provider_request_preview: Option<Value>,
}

/// Immutable context snapshot stored for recovery or audit.
/// 用于恢复或审计的不可变上下文快照。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionContextSnapshot {
    /// Stable snapshot identifier.
    /// 稳定快照标识符。
    pub snapshot_id: String,
    /// Snapshot timestamp in milliseconds.
    /// 快照毫秒时间戳。
    pub timestamp_ms: u64,
    /// Captured context document.
    /// 捕获的上下文文档。
    pub context: SessionContextDocument,
}

/// Checkpoint produced by context compaction.
/// 上下文压缩生成的检查点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionCompactionCheckpoint {
    /// Stable checkpoint identifier.
    /// 稳定检查点标识符。
    pub checkpoint_id: String,
    /// Owning session node identifier.
    /// 所属会话节点标识符。
    pub node_id: SessionNodeId,
    /// Event sequence included by this checkpoint.
    /// 该检查点覆盖到的事件序号。
    pub through_sequence: u64,
    /// Checkpoint timestamp in milliseconds.
    /// 检查点毫秒时间戳。
    pub timestamp_ms: u64,
    /// Compacted context summary.
    /// 压缩后的上下文摘要。
    pub summary: String,
}

/// Lightweight runtime checkpoint for resumable session state.
/// 可恢复会话状态的轻量运行时检查点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionRuntimeCheckpoint {
    /// Owning session node identifier.
    /// 所属会话节点标识符。
    pub node_id: SessionNodeId,
    /// Runtime status string owned by the caller.
    /// 调用方拥有的运行时状态字符串。
    pub status: String,
    /// Last persisted event sequence.
    /// 最近持久化的事件序号。
    pub last_sequence: u64,
    /// Runtime checkpoint payload.
    /// 运行时检查点载荷。
    pub payload: Value,
}
