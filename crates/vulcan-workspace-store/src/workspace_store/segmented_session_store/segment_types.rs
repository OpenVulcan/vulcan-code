//! Segment and runtime-log metadata types for segmented session storage.
//! 分段会话存储的分段与运行时日志元数据类型。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use vulcan_context::RuntimeLogEventLine;

use super::{DEFAULT_SEGMENT_MAX_BYTES, DEFAULT_SEGMENT_MAX_LINES};

/// Append-only event kind persisted in segmented session JSONL files.
/// 持久化到分段会话 JSONL 文件中的追加式事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(in crate::workspace_store) enum SegmentedSessionEventKind {
    /// Canonical session metadata audit event.
    /// 规范会话元数据审计事件。
    SessionMeta,
    /// User prompt event that may be displayed in the main transcript.
    /// 可展示在主转录中的用户提示词事件。
    UserPrompt,
    /// Assistant output event that may be displayed in the main transcript.
    /// 可展示在主转录中的助手输出事件。
    AssistantOutput,
    /// Generic display event used by the transcript reducer.
    /// 转录展示 reducer 使用的通用展示事件。
    DisplayEvent,
    /// Model-history response item used by context replay.
    /// 上下文重放使用的模型历史响应项。
    ResponseItem,
    /// Compaction anchor or compaction audit record.
    /// 压缩锚点或压缩审计记录。
    Compacted,
    /// Turn context baseline used by runtime restoration.
    /// 运行时恢复使用的轮次上下文基线。
    TurnContext,
    /// session-boundary runtime boundary event.
    /// 会话边界运行时边界事件。
    SessionBoundary,
    /// Append-only patch event for a previously visible record.
    /// 针对既有可见记录的追加式补丁事件。
    RecordPatched,
    /// Rollback marker that tells reducers to hide newer logical turns.
    /// 告诉 reducer 隐藏较新逻辑轮次的回滚标记。
    ThreadRolledBack,
    /// Turn started marker.
    /// 轮次开始标记。
    TurnStarted,
    /// Turn completed marker.
    /// 轮次完成标记。
    TurnComplete,
    /// Turn aborted marker.
    /// 轮次中止标记。
    TurnAborted,
    /// Canonical first event for a child task session.
    /// 子任务会话的规范首事件。
    SubagentTaskMeta,
    /// Parent-session event that links to a started child task session.
    /// 父会话中关联已启动子任务会话的事件。
    SubagentTaskStarted,
    /// Parent-session event that links to one child task progress update.
    /// 父会话中关联某个子任务进展更新的事件。
    SubagentTaskEventLinked,
    /// Parent-session event that links to a terminal child task state.
    /// 父会话中关联子任务终态的事件。
    SubagentTaskCompleted,
}

/// Segment rollover policy stored in session metadata.
/// 保存在会话元数据中的分段滚动策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentPolicy {
    /// Maximum JSONL line count allowed in one segment.
    /// 单个分段允许的最大 JSONL 行数。
    pub max_lines: u64,
    /// Maximum byte count allowed in one segment.
    /// 单个分段允许的最大字节数。
    pub max_bytes: u64,
}

impl Default for SegmentPolicy {
    /// Builds the production default segment policy.
    /// 构建生产默认分段策略。
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_SEGMENT_MAX_LINES,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        }
    }
}

/// Metadata for one physical JSONL segment.
/// 单个物理 JSONL 分段的元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentMetadata {
    /// Segment file name relative to the owning session directory.
    /// 相对所属会话目录的分段文件名。
    pub file: String,
    /// First sequence number expected in this segment.
    /// 当前分段内预期的首个序号。
    pub start_seq: u64,
    /// Last sequence number physically retained in this segment.
    /// 当前分段物理保留的最后序号。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_seq: Option<u64>,
    /// Number of JSONL lines currently written to this segment.
    /// 当前分段已经写入的 JSONL 行数。
    pub line_count: u64,
    /// Number of bytes currently written to this segment.
    /// 当前分段已经写入的字节数。
    pub byte_len: u64,
    /// Timestamp of the first event in this segment.
    /// 当前分段首个事件的时间戳。
    pub first_timestamp: Option<String>,
    /// Timestamp of the last event in this segment.
    /// 当前分段最后一个事件的时间戳。
    pub last_timestamp: Option<String>,
    /// First turn index represented by this segment when known.
    /// 已知时当前分段表示的首个轮次索引。
    pub first_turn_index: Option<i64>,
    /// Last turn index represented by this segment when known.
    /// 已知时当前分段表示的最后轮次索引。
    pub last_turn_index: Option<i64>,
    /// First transcript message index physically stored in this segment.
    /// 当前分段物理存储的首个转录消息索引。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_message_index: Option<u64>,
    /// Last transcript message index physically stored in this segment.
    /// 当前分段物理存储的最后转录消息索引。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_message_index: Option<u64>,
    /// Last compaction sequence observed in this segment when known.
    /// 已知时当前分段内最后一次压缩事件序号。
    pub last_compaction_seq: Option<u64>,
}

impl SegmentMetadata {
    /// Creates an empty segment metadata record.
    /// 创建一条空分段元数据记录。
    pub(in crate::workspace_store) fn empty(file: String, start_seq: u64) -> Self {
        Self {
            file,
            start_seq,
            last_seq: None,
            line_count: 0,
            byte_len: 0,
            first_timestamp: None,
            last_timestamp: None,
            first_turn_index: None,
            last_turn_index: None,
            first_message_index: None,
            last_message_index: None,
            last_compaction_seq: None,
        }
    }
}

/// Metadata for one provider-facing runtime JSONL segment.
/// 单个 Provider 可见运行时 JSONL 分段的元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct RuntimeSegmentMetadata {
    /// Segment file path relative to the owning session directory.
    /// 相对所属会话目录的分段文件路径。
    pub file: String,
    /// First sequence number expected in this runtime segment.
    /// 当前运行时分段内预期的首个序号。
    pub start_seq: u64,
    /// Last sequence number written to this runtime segment.
    /// 当前运行时分段已写入的最后序号。
    pub end_seq: Option<u64>,
    /// Number of JSONL lines currently written to this runtime segment.
    /// 当前运行时分段已经写入的 JSONL 行数。
    pub line_count: u64,
    /// Number of bytes currently written to this runtime segment.
    /// 当前运行时分段已经写入的字节数。
    pub byte_len: u64,
    /// Timestamp captured when this runtime segment was created.
    /// 创建该运行时分段时捕获的时间戳。
    pub created_at: String,
    /// Timestamp of the last event written to this runtime segment.
    /// 写入该运行时分段的最后事件时间戳。
    pub updated_at: Option<String>,
    /// Whether this segment has been sealed against further appends.
    /// 该分段是否已经封存且不再追加。
    pub sealed: bool,
}

impl RuntimeSegmentMetadata {
    /// Creates an empty runtime segment metadata record.
    /// 创建一条空运行时分段元数据记录。
    pub(in crate::workspace_store) fn empty(
        file: String,
        start_seq: u64,
        created_at: String,
    ) -> Self {
        Self {
            file,
            start_seq,
            end_seq: None,
            line_count: 0,
            byte_len: 0,
            created_at,
            updated_at: None,
            sealed: false,
        }
    }
}

/// Metadata document that indexes provider-facing runtime JSONL segments.
/// 为 Provider 可见运行时 JSONL 分段建立索引的元数据文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct RuntimeLogMetadata {
    /// Runtime metadata schema version.
    /// 运行时元数据结构版本。
    pub schema_version: u32,
    /// Active runtime segment path relative to the session directory.
    /// 相对会话目录的活动运行时分段路径。
    pub active_segment: String,
    /// Next runtime sequence number to assign.
    /// 下一个待分配的运行时序号。
    pub next_seq: u64,
    /// Timestamp captured when this runtime metadata was created.
    /// 创建该运行时元数据时捕获的时间戳。
    pub created_at: String,
    /// Timestamp captured when this runtime metadata was last updated.
    /// 最近更新该运行时元数据时捕获的时间戳。
    pub updated_at: String,
    /// Version of the bounded canonical-conversation projection checkpoint.
    /// 有界规范会话投影检查点版本。
    #[serde(default)]
    pub conversation_checkpoint_version: u32,
    /// Number of canonical conversation messages already projected into runtime JSONL.
    /// 已经投影进运行时 JSONL 的规范会话消息数量。
    #[serde(default)]
    pub conversation_message_count: u64,
    /// Number of conversation-projecting runtime events already persisted.
    /// 已经持久化且会投影为会话的运行时事件数量。
    #[serde(default)]
    pub conversation_event_count: u64,
    /// Message index where the currently open bounded projection anchor begins.
    /// 当前开放有界投影锚点开始处的消息索引。
    #[serde(default)]
    pub conversation_anchor_message_index: u64,
    /// Number of anchor-projection events already persisted.
    /// 已经持久化的锚点投影事件数量。
    #[serde(default)]
    pub conversation_anchor_event_count: u64,
    /// SHA-256 of the final canonical message covered by the checkpoint.
    /// 检查点覆盖的最后一条规范消息 SHA-256。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_tail_message_sha256: Option<String>,
    /// SHA-256 of persisted event payloads projected from the active anchor.
    /// 从活动锚点投影出的已持久化事件载荷 SHA-256。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_anchor_projection_sha256: Option<String>,
    /// Most recent context-compaction identifier already applied to this physical runtime log.
    /// 已经应用到当前物理运行时日志的最近上下文压缩标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_compaction_id: Option<String>,
    /// Ordered runtime segment metadata records.
    /// 有序运行时分段元数据记录。
    pub segments: Vec<RuntimeSegmentMetadata>,
}

/// Rebuildable index document used to accelerate runtime-log reads.
/// 用于加速运行时日志读取的可重建索引文档。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct RuntimeLogIndex {
    /// Runtime index schema version.
    /// 运行时索引结构版本。
    pub schema_version: u32,
    /// Last sequence observed while rebuilding the index.
    /// 重建索引时观察到的最后序号。
    pub last_seq: Option<u64>,
    /// Number of runtime JSONL lines indexed.
    /// 已索引的运行时 JSONL 行数。
    pub line_count: u64,
    /// Latest context compaction sequence when present.
    /// 存在时最近一次上下文压缩序号。
    pub latest_compaction_seq: Option<u64>,
    /// Message identifiers mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的消息标识。
    pub message_offsets: std::collections::BTreeMap<String, u64>,
    /// Tool-call identifiers mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的工具调用标识。
    pub tool_call_offsets: std::collections::BTreeMap<String, u64>,
    /// Provider conversion checkpoints mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的 Provider 转换检查点。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_checkpoint_offsets: std::collections::BTreeMap<String, u64>,
    /// Runtime repair records mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的运行时修复记录。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub repair_offsets: std::collections::BTreeMap<String, u64>,
    /// Injection fingerprints mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的注入指纹。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub injection_offsets: std::collections::BTreeMap<String, u64>,
    /// Chronological system-update identifiers mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的时间顺序系统更新标识。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub system_update_offsets: std::collections::BTreeMap<String, u64>,
    /// Cancelled injection identifiers mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的已取消注入标识。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub injection_cancelled_offsets: std::collections::BTreeMap<String, u64>,
    /// Merged injection identifiers mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的已合并注入标识。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub injection_merged_offsets: std::collections::BTreeMap<String, u64>,
    /// Provider response checkpoints mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的 Provider 响应检查点。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_response_checkpoint_offsets: std::collections::BTreeMap<String, u64>,
    /// Provider lowering repairs mapped to their latest runtime sequence.
    /// 映射到最近运行时序号的 Provider 转换修复记录。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub provider_lowering_repair_offsets: std::collections::BTreeMap<String, u64>,
}

/// Result returned by tolerant runtime-log reads.
/// 容错读取运行时日志时返回的结果。
#[derive(Debug, Clone, Default)]
pub(in crate::workspace_store) struct RuntimeLogRepairRead {
    /// Parsed runtime lines that are still valid.
    /// 仍然有效且已解析的运行时日志行。
    pub(in crate::workspace_store) lines: Vec<RuntimeLogEventLine>,
    /// One-based source line numbers that failed JSON decoding.
    /// JSON 解码失败的 1 基源文件行号。
    pub(in crate::workspace_store) corrupt_line_numbers: Vec<usize>,
}

/// Validated runtime-log state captured during one append-lock transaction.
/// 在一次追加锁事务中捕获的已验证运行时日志状态。
#[derive(Debug)]
pub(in crate::workspace_store) struct RuntimeLogSnapshot<'guard> {
    /// Authoritative metadata validated against the persisted runtime segment.
    /// 已与持久化运行时分段相互验证的权威元数据。
    pub(in crate::workspace_store) metadata: RuntimeLogMetadata,
    /// Ordered and sequence-contiguous runtime lines represented by the metadata.
    /// 元数据所表示的有序且序号连续的运行时日志行。
    pub(in crate::workspace_store) lines: Vec<RuntimeLogEventLine>,
    /// Lifetime proof that the append lock used to create this snapshot is still owned.
    /// 创建此快照时使用的追加锁仍被持有的生命周期证明。
    pub(in crate::workspace_store) _append_guard: std::marker::PhantomData<&'guard ()>,
}
