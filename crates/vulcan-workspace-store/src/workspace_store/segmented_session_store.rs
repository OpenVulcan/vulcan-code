//! Segmented append-only session storage used by the new Vulcan session fact source.
//! Vulcan 新会话事实源使用的分段追加式会话存储。

use super::current_unix_millis_text;
use crate::conversation_protocol::{
    SessionConversationState, conversation_state_from_session_messages,
    conversation_state_to_session_messages,
};
use crate::model::{
    SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind, SessionRuntimeStateEntry,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher};
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use vulcan_context::{
    RuntimeContextCompactionAppliedEvent, RuntimeInjectionAppliedEvent, RuntimeLogEvent,
    RuntimeLogEventLine, RuntimeRepairEvent, conversation_state_from_runtime_log_lines,
    runtime_log_lines_from_conversation_state,
};
#[cfg(test)]
use vulcan_context::{
    RuntimeInjectionCancelledEvent, RuntimeInjectionMergedEvent,
    RuntimeProviderLoweringRepairEvent, RuntimeProviderNativeStateEnvelope,
    RuntimeProviderRequestCheckpointEvent, RuntimeProviderResponseCheckpointEvent,
    RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint, RuntimeSystemUpdateTrust,
};
use vulcan_session_boundary::SessionBoundaryEvent;

/// Directory name used for active sessions below the Vulcan code data root.
/// Vulcan Code 数据根目录下用于活动会话的目录名称。
pub(super) const SEGMENTED_SESSIONS_DIR: &str = "sessions";
/// Directory name used for archived sessions below the Vulcan code data root.
/// Vulcan Code 数据根目录下用于归档会话的目录名称。
pub(super) const SEGMENTED_ARCHIVED_SESSIONS_DIR: &str = "archived_sessions";
/// Metadata file name stored in every main session and task session directory.
/// 每个主会话与子任务会话目录内保存的元数据文件名。
pub(super) const SEGMENTED_METADATA_FILE: &str = "metadata.json";
/// Runtime state file name stored in every main session and task session directory.
/// 每个主会话与子任务会话目录内保存的运行状态文件名。
pub(super) const SEGMENTED_STATE_FILE: &str = "state.json";
/// Runtime metadata file name stored beside display transcript segments.
/// 保存在展示转录分段旁边的运行时元数据文件名。
pub(super) const SEGMENTED_RUNTIME_METADATA_FILE: &str = "runtime_metadata.json";
/// Runtime index file name rebuilt from runtime JSONL when needed.
/// 需要时可从 runtime JSONL 重建的运行时索引文件名。
pub(super) const SEGMENTED_RUNTIME_INDEX_FILE: &str = "runtime.index.json";
/// Runtime segment directory name used for provider-facing fact logs.
/// 用于 Provider 可见事实日志的运行时分段目录名。
pub(super) const SEGMENTED_RUNTIME_DIR: &str = "runtime";
/// Runtime backup directory name used before context compaction.
/// 上下文压缩前使用的运行时备份目录名。
pub(super) const SEGMENTED_RUNTIME_BACKUP_DIR: &str = "runtime_backup";
/// Child task directory name used by the new session layout.
/// 新会话布局中用于保存子任务的目录名称。
pub(super) const SEGMENTED_TASK_DIR: &str = "task";
/// Blob directory name used by the new session layout.
/// 新会话布局中用于保存外置大对象的目录名称。
pub(super) const SEGMENTED_BLOB_DIR: &str = "blobs";
/// Default maximum line count allowed in one segment before rollover.
/// 单个分段在滚动前默认允许的最大行数。
pub(super) const DEFAULT_SEGMENT_MAX_LINES: u64 = 3000;
/// Default maximum byte count allowed in one segment before rollover.
/// 单个分段在滚动前默认允许的最大字节数。
pub(super) const DEFAULT_SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum runtime-log backups retained after context compaction.
/// 上下文压缩后保留的运行时日志备份最大数量。
const RUNTIME_COMPACTION_BACKUP_LIMIT: usize = 5;
/// Process-local mutex protecting overwrite-only state read/modify/write operations.
/// 保护覆盖式状态读改写操作的进程内互斥锁。
static SEGMENTED_STATE_FILE_IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
/// Process-local mutex protecting segmented JSONL append read/modify/write operations.
/// 保护分段 JSONL 追加读改写操作的进程内互斥锁。
static SEGMENTED_APPEND_FILE_IO_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Append-only event kind persisted in segmented session JSONL files.
/// 持久化到分段会话 JSONL 文件中的追加式事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SegmentedSessionEventKind {
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
pub(super) struct SegmentPolicy {
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
pub(super) struct SegmentMetadata {
    /// Segment file name relative to the owning session directory.
    /// 相对所属会话目录的分段文件名。
    pub file: String,
    /// First sequence number expected in this segment.
    /// 当前分段内预期的首个序号。
    pub start_seq: u64,
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
    /// Last compaction sequence observed in this segment when known.
    /// 已知时当前分段内最后一次压缩事件序号。
    pub last_compaction_seq: Option<u64>,
}

impl SegmentMetadata {
    /// Creates an empty segment metadata record.
    /// 创建一条空分段元数据记录。
    fn empty(file: String, start_seq: u64) -> Self {
        Self {
            file,
            start_seq,
            line_count: 0,
            byte_len: 0,
            first_timestamp: None,
            last_timestamp: None,
            first_turn_index: None,
            last_turn_index: None,
            last_compaction_seq: None,
        }
    }
}

/// Metadata for one provider-facing runtime JSONL segment.
/// 单个 Provider 可见运行时 JSONL 分段的元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RuntimeSegmentMetadata {
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
    fn empty(file: String, start_seq: u64, created_at: String) -> Self {
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
pub(super) struct RuntimeLogMetadata {
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
    /// Ordered runtime segment metadata records.
    /// 有序运行时分段元数据记录。
    pub segments: Vec<RuntimeSegmentMetadata>,
}

/// Rebuildable index document used to accelerate runtime-log reads.
/// 用于加速运行时日志读取的可重建索引文档。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct RuntimeLogIndex {
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
struct RuntimeLogRepairRead {
    /// Parsed runtime lines that are still valid.
    /// 仍然有效且已解析的运行时日志行。
    lines: Vec<RuntimeLogEventLine>,
    /// One-based source line numbers that failed JSON decoding.
    /// JSON 解码失败的 1 基源文件行号。
    corrupt_line_numbers: Vec<usize>,
}

/// Agent profile source kind recorded for one subagent task session.
/// 为单个子代理任务会话记录的代理画像来源类型。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SubagentAgentKind {
    /// The task inherits the main agent prompt and runtime baseline.
    /// 该任务继承主代理提示词与运行时基线。
    MainClone,
    /// The task uses a specific agent.md or persisted agent profile.
    /// 该任务使用特定 agent.md 或持久化代理画像。
    AgentMd,
    /// The task uses a dynamically constructed profile.
    /// 该任务使用动态构造的代理画像。
    Custom,
}

/// Agent profile metadata recorded for a child task session.
/// 为子任务会话记录的代理画像元数据。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SubagentAgentProfile {
    /// Stable agent identifier when available.
    /// 可用时记录的稳定代理标识。
    pub agent_id: Option<String>,
    /// Human-readable agent name when available.
    /// 可用时记录的用户可读代理名称。
    pub agent_name: Option<String>,
    /// Normalized agent.md path or stable resource identifier when available.
    /// 可用时记录的规范化 agent.md 路径或稳定资源标识。
    pub agent_md_path: Option<String>,
    /// Prompt source label used to explain how the child agent was built.
    /// 用于说明子代理构建来源的提示词来源标签。
    pub prompt_source: Option<String>,
}

/// Spawn origin recorded in a child task metadata event.
/// 子任务元数据事件中记录的派生来源。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SubagentSpawnedBy {
    /// Parent turn identifier when known.
    /// 已知时记录的父会话轮次标识。
    pub parent_turn_id: Option<String>,
    /// Parent event sequence that spawned the task when known.
    /// 已知时记录的派生该任务的父会话事件序号。
    pub parent_event_seq: Option<u64>,
    /// Tool call identifier that spawned the task when known.
    /// 已知时记录的派生该任务的工具调用标识。
    pub tool_call_id: Option<String>,
}

/// Canonical metadata payload written as the first line of each child task session.
/// 写入每个子任务会话首行的规范元数据载荷。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SubagentTaskMetadataPayload {
    /// Child task session identifier.
    /// 子任务会话标识。
    pub task_session_id: String,
    /// Parent main session identifier.
    /// 父主会话标识。
    pub parent_session_id: String,
    /// Parent project tag used in the directory layout.
    /// 目录布局中使用的父项目标签。
    pub parent_project_tag: String,
    /// Agent source kind for this child task.
    /// 当前子任务的代理来源类型。
    pub agent_kind: SubagentAgentKind,
    /// Agent profile details for this child task.
    /// 当前子任务的代理画像详情。
    pub agent_profile: SubagentAgentProfile,
    /// Parent-side spawn information.
    /// 父侧派生信息。
    pub spawned_by: SubagentSpawnedBy,
    /// Creation timestamp for this child task.
    /// 当前子任务的创建时间戳。
    pub created_at: String,
}

/// Lightweight task mirror stored in the parent session metadata.
/// 保存在父会话元数据中的轻量任务镜像。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SubagentTaskMirror {
    /// Child task session identifier.
    /// 子任务会话标识。
    pub session_id: String,
    /// Child task directory relative to the parent session directory.
    /// 相对父会话目录的子任务目录。
    pub dir: String,
    /// Active child task segment path relative to the parent session directory.
    /// 相对父会话目录的活动子任务分段路径。
    pub active_segment: String,
    /// Child metadata file path relative to the parent session directory.
    /// 相对父会话目录的子任务元数据文件路径。
    pub metadata_file: String,
    /// Agent source kind for this child task.
    /// 当前子任务的代理来源类型。
    pub agent_kind: SubagentAgentKind,
    /// Agent profile details mirrored for list and navigation views.
    /// 为列表与导航视图镜像的代理画像详情。
    pub agent_profile: SubagentAgentProfile,
    /// Parent event sequence where the task starts when known.
    /// 已知时记录的子任务开始父事件序号。
    pub parent_start_seq: Option<u64>,
    /// Parent event sequence where the task ends when known.
    /// 已知时记录的子任务结束父事件序号。
    pub parent_end_seq: Option<u64>,
    /// Current task status label.
    /// 当前任务状态标签。
    pub status: String,
    /// Total child segment line count mirrored for quick inspection.
    /// 为快速检查镜像的子任务分段总行数。
    pub line_count: u64,
    /// Creation timestamp for this child task.
    /// 当前子任务的创建时间戳。
    pub created_at: String,
    /// Last update timestamp for this child task.
    /// 当前子任务的最后更新时间戳。
    pub updated_at: String,
}

/// Metadata document stored in each main segmented session directory.
/// 存储在每个主分段会话目录中的元数据文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SegmentedSessionMetadata {
    /// Metadata schema version.
    /// 元数据结构版本。
    pub schema_version: u32,
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Stable project tag used by the directory layout.
    /// 目录布局使用的稳定项目标签。
    pub project_tag: String,
    /// Creation timestamp.
    /// 创建时间戳。
    pub created_at: String,
    /// Last update timestamp.
    /// 最后更新时间戳。
    pub updated_at: String,
    /// Archive timestamp when archived.
    /// 归档时记录的归档时间戳。
    pub archived_at: Option<String>,
    /// Project identifier associated with the session.
    /// 该会话关联的项目标识。
    pub project_id: Option<String>,
    /// Current display title.
    /// 当前展示标题。
    pub title: Option<String>,
    /// Source label for the current title.
    /// 当前标题的来源标签。
    pub title_source: Option<String>,
    /// Canonical source marker for this storage layout.
    /// 当前存储布局的规范来源标记。
    pub canonical_source: String,
    /// Active segment file name relative to the session directory.
    /// 相对会话目录的活动分段文件名。
    pub active_segment: String,
    /// Child task directory name relative to the session directory.
    /// 相对会话目录的子任务目录名。
    pub task_dir: String,
    /// Next global sequence number for the main session.
    /// 主会话下一条全局事件序号。
    pub next_global_seq: u64,
    /// Number of canonical transcript messages already appended after the latest rollback boundary.
    /// 最新回滚边界之后已经追加的规范转录消息数量。
    #[serde(default)]
    pub message_count: u64,
    /// Identifier of the last canonical transcript message appended after the latest rollback boundary.
    /// 最新回滚边界之后最后一条已追加规范转录消息的标识。
    #[serde(default)]
    pub last_message_id: Option<String>,
    /// Serialized fingerprint of the last canonical transcript message for tail-boundary validation.
    /// 用于尾部边界校验的最后一条规范转录消息序列化指纹。
    #[serde(default)]
    pub last_message_fingerprint: Option<String>,
    /// Latest rollback marker sequence for fast display-window and append checks.
    /// 最新回滚标记序号，用于快速显示窗口与追加检查。
    pub latest_rollback_seq: Option<u64>,
    /// Segment rollover policy.
    /// 分段滚动策略。
    pub segment_policy: SegmentPolicy,
    /// Ordered segment metadata entries.
    /// 按顺序记录的分段元数据集合。
    pub segments: Vec<SegmentMetadata>,
    /// Lightweight child task mirrors.
    /// 子任务轻量镜像集合。
    pub tasks: Vec<SubagentTaskMirror>,
}

/// Metadata document stored in each child task session directory.
/// 存储在每个子任务会话目录中的元数据文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct SegmentedTaskSessionMetadata {
    /// Metadata schema version.
    /// 元数据结构版本。
    pub schema_version: u32,
    /// Child task session identifier.
    /// 子任务会话标识。
    pub task_session_id: String,
    /// Parent main session identifier.
    /// 父主会话标识。
    pub parent_session_id: String,
    /// Parent project tag used by the directory layout.
    /// 目录布局中使用的父项目标签。
    pub parent_project_tag: String,
    /// Creation timestamp.
    /// 创建时间戳。
    pub created_at: String,
    /// Last update timestamp.
    /// 最后更新时间戳。
    pub updated_at: String,
    /// Current task status label.
    /// 当前任务状态标签。
    pub status: String,
    /// Agent source kind for this child task.
    /// 当前子任务的代理来源类型。
    pub agent_kind: SubagentAgentKind,
    /// Agent profile details for this child task.
    /// 当前子任务的代理画像详情。
    pub agent_profile: SubagentAgentProfile,
    /// Parent-side spawn information.
    /// 父侧派生信息。
    pub spawned_by: SubagentSpawnedBy,
    /// Active task segment file name relative to the task directory.
    /// 相对子任务目录的活动分段文件名。
    pub active_segment: String,
    /// Next task-local sequence number.
    /// 子任务局部下一条事件序号。
    pub next_local_seq: u64,
    /// Segment rollover policy.
    /// 分段滚动策略。
    pub segment_policy: SegmentPolicy,
    /// Ordered task segment metadata entries.
    /// 按顺序记录的子任务分段元数据集合。
    pub segments: Vec<SegmentMetadata>,
}

/// Overwrite-only state document stored next to segmented JSONL files.
/// 存放在分段 JSONL 旁边的覆盖式状态文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedSessionStateDocument {
    /// State schema version.
    /// 状态结构版本。
    pub schema_version: u32,
    /// Owning session or task session identifier.
    /// 所属会话或子任务会话标识。
    pub session_id: String,
    /// Last update timestamp.
    /// 最后更新时间戳。
    pub updated_at: String,
    /// Serialized runtime state payload.
    /// 序列化后的运行时状态载荷。
    pub runtime_state: Value,
    /// Serialized current context payload.
    /// 序列化后的当前上下文载荷。
    pub current_context: Value,
    /// session-boundary thread state managed by the session runner.
    /// 由会话 runner 管理的 会话边界线程状态。
    #[serde(default)]
    pub session_boundary_state: Value,
}

/// One append-only JSONL event line.
/// 单条追加式 JSONL 事件行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedSessionEventLine {
    /// Monotonic sequence number within the owning session scope.
    /// 所属会话作用域内单调递增的序号。
    pub seq: u64,
    /// Event timestamp.
    /// 事件时间戳。
    pub timestamp: String,
    /// Event kind serialized as the JSON `type` field.
    /// 序列化为 JSON `type` 字段的事件类型。
    #[serde(rename = "type")]
    pub kind: SegmentedSessionEventKind,
    /// Event payload.
    /// 事件载荷。
    pub payload: Value,
}

/// Event append input before the store assigns a sequence number.
/// 存储分配序号之前的事件追加输入。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedSessionEventInput {
    /// Event timestamp.
    /// 事件时间戳。
    pub timestamp: String,
    /// Event kind.
    /// 事件类型。
    pub kind: SegmentedSessionEventKind,
    /// Event payload.
    /// 事件载荷。
    pub payload: Value,
    /// Optional display turn index carried into segment metadata.
    /// 写入分段元数据的可选展示轮次索引。
    pub turn_index: Option<i64>,
}

/// Main session creation request.
/// 主会话创建请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SegmentedSessionCreateRequest {
    /// Stable session identifier.
    /// 稳定会话标识。
    pub session_id: String,
    /// Project tag used in the directory layout.
    /// 目录布局使用的项目标签。
    pub project_tag: String,
    /// Session creation date used in the directory name.
    /// 会话目录名中使用的会话创建日期。
    pub created_date: String,
    /// Session creation timestamp.
    /// 会话创建时间戳。
    pub created_at: String,
    /// Project identifier associated with the session.
    /// 会话关联的项目标识。
    pub project_id: Option<String>,
    /// Initial display title.
    /// 初始展示标题。
    pub title: Option<String>,
    /// Initial title source label.
    /// 初始标题来源标签。
    pub title_source: Option<String>,
    /// Segment rollover policy.
    /// 分段滚动策略。
    pub segment_policy: SegmentPolicy,
}

/// Child task session creation request.
/// 子任务会话创建请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SegmentedTaskSessionCreateRequest {
    /// Child task session identifier.
    /// 子任务会话标识。
    pub task_session_id: String,
    /// Parent event sequence that started the task when known.
    /// 已知时记录的启动该任务的父事件序号。
    pub parent_start_seq: Option<u64>,
    /// Agent source kind for this child task.
    /// 当前子任务的代理来源类型。
    pub agent_kind: SubagentAgentKind,
    /// Agent profile details for this child task.
    /// 当前子任务的代理画像详情。
    pub agent_profile: SubagentAgentProfile,
    /// Parent-side spawn information.
    /// 父侧派生信息。
    pub spawned_by: SubagentSpawnedBy,
    /// Creation timestamp for this child task.
    /// 当前子任务的创建时间戳。
    pub created_at: String,
    /// Initial task status label.
    /// 初始任务状态标签。
    pub status: String,
    /// Segment rollover policy.
    /// 分段滚动策略。
    pub segment_policy: SegmentPolicy,
}

/// Parent-link update request for one child task session.
/// 某个子任务会话的父会话关联更新请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedTaskLinkUpdate {
    /// Child task session identifier.
    /// 子任务会话标识。
    pub task_session_id: String,
    /// Update timestamp.
    /// 更新时间戳。
    pub timestamp: String,
    /// Current task status label.
    /// 当前任务状态标签。
    pub status: String,
    /// Whether this update is terminal and should close the parent task mirror.
    /// 当前更新是否为终态并应关闭父任务镜像。
    pub terminal: bool,
    /// Additional parent-link payload details.
    /// 附加的父关联载荷详情。
    pub payload: Value,
    /// Optional display turn index carried into parent segment metadata.
    /// 写入父分段元数据的可选展示轮次索引。
    pub turn_index: Option<i64>,
}

/// Main session metadata patch with explicit optional updates.
/// 带有显式可选更新项的主会话元数据补丁。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct SegmentedSessionMetadataPatch {
    /// Last update timestamp applied by this patch.
    /// 当前补丁应用的最后更新时间戳。
    pub updated_at: Option<String>,
    /// Optional title replacement.
    /// 可选标题替换值。
    pub title: Option<Option<String>>,
    /// Optional title source replacement.
    /// 可选标题来源替换值。
    pub title_source: Option<Option<String>>,
    /// Optional archive timestamp replacement.
    /// 可选归档时间戳替换值。
    pub archived_at: Option<Option<String>>,
}

/// Latest compaction anchor discovered by reverse segment scanning.
/// 通过分段倒扫发现的最新压缩锚点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedCompactionAnchor {
    /// Sequence number of the compaction event.
    /// 压缩事件的序号。
    pub compaction_seq: u64,
    /// Segment file that contains the compaction event.
    /// 包含压缩事件的分段文件。
    pub segment_file: String,
    /// Zero-based line index inside the segment file.
    /// 分段文件内从零开始的行索引。
    pub line_index: usize,
    /// Replacement history payload used as the restored baseline.
    /// 用作恢复基线的 replacement history 载荷。
    pub replacement_history: Value,
}

/// Latest turn-context anchor discovered while reverse scanning.
/// 倒序扫描时发现的最新轮次上下文锚点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedTurnContextAnchor {
    /// Sequence number of the turn-context event.
    /// 轮次上下文事件的序号。
    pub seq: u64,
    /// Segment file that contains the turn-context event.
    /// 包含轮次上下文事件的分段文件。
    pub segment_file: String,
    /// Zero-based line index inside the segment file.
    /// 分段文件内从零开始的行索引。
    pub line_index: usize,
    /// Turn-context payload.
    /// 轮次上下文载荷。
    pub payload: Value,
}

/// Rollback marker discovered while reverse scanning.
/// 倒序扫描时发现的回滚标记。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedRollbackMarker {
    /// Sequence number of the rollback event.
    /// 回滚事件的序号。
    pub seq: u64,
    /// Segment file that contains the rollback event.
    /// 包含回滚事件的分段文件。
    pub segment_file: String,
    /// Zero-based line index inside the segment file.
    /// 分段文件内从零开始的行索引。
    pub line_index: usize,
    /// Rollback payload.
    /// 回滚载荷。
    pub payload: Value,
}

/// Result returned by a reverse compaction anchor scan.
/// 压缩锚点倒扫返回的结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedCompactionScanResult {
    /// Latest compaction anchor when one was found.
    /// 找到时记录的最新压缩锚点。
    pub anchor: Option<SegmentedCompactionAnchor>,
    /// Events after the anchor in forward replay order, or all scanned events when no anchor exists.
    /// 按正向重放顺序排列的锚点后事件；没有锚点时为全部已扫描事件。
    pub suffix: Vec<SegmentedSessionEventLine>,
    /// Latest turn-context anchor discovered during the same reverse scan.
    /// 同一次倒序扫描中发现的最新轮次上下文锚点。
    pub latest_turn_context: Option<SegmentedTurnContextAnchor>,
    /// Rollback markers discovered while scanning newer history.
    /// 扫描较新历史时发现的回滚标记集合。
    pub rollback_markers: Vec<SegmentedRollbackMarker>,
    /// Number of segment files read by the scan.
    /// 本次扫描读取的分段文件数量。
    pub scanned_segment_count: usize,
}

/// Rebuilt conversation state produced by replaying segmented compaction history.
/// 通过重放分段压缩历史得到的会话状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SegmentedSessionReplayResult {
    /// Rebuilt conversation state ready for runtime hydration.
    /// 可用于运行态水合的重建会话状态。
    pub conversation_state: SessionConversationState,
    /// Optional latest compaction marker id.
    /// 可选的最新压缩标记标识。
    pub last_compaction_id: Option<String>,
    /// Optional context start message id after replay.
    /// 重放后的可选上下文起始消息标识。
    pub context_start_message_id: Option<String>,
    /// Optional update timestamp inferred from the replay suffix.
    /// 从重放后缀推断出的可选更新时间戳。
    pub updated_at: Option<String>,
}

/// Tool-detail page read from segmented JSONL files.
/// 从分段 JSONL 文件读取出的工具详情页。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedSessionToolDetailWindow {
    /// Tool-detail lines in forward display order.
    /// 按正向展示顺序排列的工具详情行。
    pub lines: Vec<SegmentedSessionEventLine>,
    /// Whether older matching tool-detail lines exist before this page.
    /// 当前页之前是否仍存在更早的匹配工具详情行。
    pub has_more_detail: bool,
}

/// Transcript turn window read from segmented JSONL files.
/// 从分段 JSONL 文件读取出的转录轮次窗口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct SegmentedSessionTurnWindow {
    /// Event lines in forward display order.
    /// 按正向展示顺序排列的事件行。
    pub lines: Vec<SegmentedSessionEventLine>,
    /// Whether older message turns exist before this page.
    /// 当前页之前是否仍存在更早的消息轮次。
    pub has_more_history: bool,
}

/// One session directory discovered from the text metadata catalog.
/// 从文本元数据目录发现的一条会话目录记录。
#[derive(Debug, Clone)]
pub(super) struct SegmentedSessionCatalogEntry {
    /// Absolute segmented session directory path.
    /// 分段会话目录的绝对路径。
    pub session_dir: PathBuf,
    /// Whether the directory was found below archived_sessions.
    /// 该目录是否位于 archived_sessions 下。
    pub archived: bool,
    /// Normalized metadata read from metadata.json.
    /// 从 metadata.json 读取并归一化后的元数据。
    pub metadata: SegmentedSessionMetadata,
}

/// Builds an active or archived session directory path.
/// 构建活动或归档会话目录路径。
pub(super) fn segmented_session_directory(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
) -> PathBuf {
    // Keep project and session path components filesystem-safe before joining them.
    // 在拼接路径前确保项目与会话路径片段对文件系统安全。
    let safe_project_tag = sanitize_file_component(project_tag);
    // Keep the date visible while retaining the stable session id in the leaf name.
    // 保留日期可读性，同时在叶子目录名中保留稳定会话标识。
    let session_dir_name = format!(
        "{}-{}",
        sanitize_file_component(created_date),
        sanitize_file_component(session_id)
    );
    // Select the active or archived root without changing the leaf layout.
    // 选择活动或归档根目录，同时保持叶子布局不变。
    let root_name = if archived {
        SEGMENTED_ARCHIVED_SESSIONS_DIR
    } else {
        SEGMENTED_SESSIONS_DIR
    };
    data_root
        .join(root_name)
        .join(safe_project_tag)
        .join(session_dir_name)
}

/// Creates the initial main session directory and writes its first metadata event.
/// 创建初始主会话目录并写入首条元数据事件。
pub(super) fn create_segmented_session(
    data_root: &Path,
    request: SegmentedSessionCreateRequest,
) -> Result<PathBuf, String> {
    create_segmented_session_in_location(data_root, false, request)
}

/// Creates the initial main session directory in the active or archived tree.
/// 在活动树或归档树中创建初始主会话目录。
pub(super) fn create_segmented_session_in_location(
    data_root: &Path,
    archived: bool,
    request: SegmentedSessionCreateRequest,
) -> Result<PathBuf, String> {
    // Resolve the canonical active session directory for this request.
    // 为当前请求解析规范会话目录。
    let session_dir = segmented_session_directory(
        data_root,
        archived,
        &request.project_tag,
        &request.created_date,
        &request.session_id,
    );
    create_dir_all(&session_dir)?;
    create_dir_all(&session_dir.join(SEGMENTED_TASK_DIR))?;
    create_dir_all(&session_dir.join(SEGMENTED_BLOB_DIR))?;

    // Build the first segment and metadata document before writing any JSONL row.
    // 在写入任何 JSONL 行前构建首个分段与元数据文档。
    let first_segment = SegmentMetadata::empty(segment_file_name(1), 0);
    // Store metadata as the overwrite-only directory index.
    // 将元数据作为覆盖式目录索引保存。
    let mut metadata = SegmentedSessionMetadata {
        schema_version: 1,
        session_id: request.session_id.clone(),
        project_tag: request.project_tag.clone(),
        created_at: request.created_at.clone(),
        updated_at: request.created_at.clone(),
        project_id: request.project_id,
        title: request.title,
        title_source: request.title_source,
        canonical_source: "segmented_rollout".to_string(),
        archived_at: archived.then(|| request.created_at.clone()),
        active_segment: first_segment.file.clone(),
        task_dir: SEGMENTED_TASK_DIR.to_string(),
        next_global_seq: 0,
        message_count: 0,
        last_message_id: None,
        last_message_fingerprint: None,
        latest_rollback_seq: None,
        segment_policy: request.segment_policy,
        segments: vec![first_segment],
        tasks: Vec::new(),
    };
    // The first JSONL line stores the canonical SessionMeta audit record.
    // 首条 JSONL 行存储规范 SessionMeta 审计记录。
    let session_meta_event = SegmentedSessionEventInput {
        timestamp: request.created_at.clone(),
        kind: SegmentedSessionEventKind::SessionMeta,
        payload: json!({
            "session_id": request.session_id,
            "project_tag": request.project_tag,
            "canonical_source": "segmented_rollout"
        }),
        turn_index: None,
    };
    append_events_with_metadata(&session_dir, &mut metadata, &[session_meta_event])?;
    write_metadata(&session_dir, &metadata)?;
    write_state(
        &session_dir,
        &SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: metadata.session_id.clone(),
            updated_at: metadata.updated_at.clone(),
            runtime_state: Value::Null,
            current_context: Value::Null,
            session_boundary_state: Value::Null,
        },
    )?;
    Ok(session_dir)
}

/// Finds an existing segmented session directory by project tag and session id.
/// 按项目标签与会话标识查找既有分段会话目录。
pub(super) fn find_segmented_session_directory(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    session_id: &str,
) -> Result<Option<PathBuf>, String> {
    // Resolve the project root using the same sanitization as path construction.
    // 使用与路径构建相同的清理规则解析项目根目录。
    let root_name = if archived {
        SEGMENTED_ARCHIVED_SESSIONS_DIR
    } else {
        SEGMENTED_SESSIONS_DIR
    };
    let project_root = data_root
        .join(root_name)
        .join(sanitize_file_component(project_tag));
    if !project_root.exists() {
        return Ok(None);
    }
    let expected_suffix = format!("-{}", sanitize_file_component(session_id));
    for entry in fs::read_dir(&project_root).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.ends_with(&expected_suffix) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Finds one segmented session directory for task-session store registration.
/// 查找一个用于注册子任务会话存储的分段会话目录。
pub fn find_segmented_session_directory_for_task_store(
    data_root: &Path,
    project_tag: &str,
    session_id: &str,
) -> Result<Option<PathBuf>, String> {
    match find_segmented_session_directory(data_root, false, project_tag, session_id)? {
        Some(session_dir) => Ok(Some(session_dir)),
        None => find_segmented_session_directory(data_root, true, project_tag, session_id),
    }
}

/// Lists active and archived segmented sessions from text metadata.
/// 从文本元数据列出活动与归档分段会话。
pub(super) fn list_segmented_session_catalog(
    data_root: &Path,
) -> Result<Vec<SegmentedSessionCatalogEntry>, String> {
    let mut entries = Vec::new();
    entries.extend(list_segmented_session_catalog_for_root(data_root, false)?);
    entries.extend(list_segmented_session_catalog_for_root(data_root, true)?);
    Ok(entries)
}

/// Finds one segmented session directory by id without consulting the database.
/// 不查询数据库，仅按会话标识查找分段会话目录。
pub(super) fn find_segmented_session_directory_by_id(
    data_root: &Path,
    session_id: &str,
) -> Result<Option<PathBuf>, String> {
    for entry in list_segmented_session_catalog(data_root)? {
        if entry.metadata.session_id == session_id {
            return Ok(Some(entry.session_dir));
        }
    }
    Ok(None)
}

/// Appends events to a main segmented session.
/// 向主分段会话追加事件。
pub(super) fn append_segmented_session_events(
    session_dir: &Path,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Hold one process-local append lock across metadata and segment updates.
    // 在元数据与分段更新期间持有同一个进程内追加锁。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load metadata first so sequence numbers and active segment remain authoritative.
    // 先加载元数据，确保序号与活动分段以元数据为准。
    let mut metadata = read_session_metadata(session_dir)?;
    // Append all events and update the in-memory metadata only after file writes succeed.
    // 追加全部事件，并且只在文件写入成功后更新内存元数据。
    let lines = append_events_with_metadata(session_dir, &mut metadata, events)?;
    write_metadata(session_dir, &metadata)?;
    Ok(lines)
}

/// Appends session boundary events while making persisted payload sequence match JSONL sequence.
/// 追加 会话边界事件，并确保持久化 payload 序号与 JSONL 行序号一致。
pub(super) fn append_segmented_boundary_events(
    session_dir: &Path,
    events: &[SessionBoundaryEvent],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if events.is_empty() {
        return Ok(Vec::new());
    }
    // Serialize boundary and display transcript appends sharing the same active segment.
    // 串行化共享同一活动分段的边界事件与展示转录追加。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load metadata before every batch so the store remains the only sequence authority.
    // 批量追加前先加载元数据，确保存储层始终是唯一序号来源。
    let mut metadata = read_session_metadata(session_dir)?;
    let mut lines = Vec::new();
    for event in events {
        let assigned_sequence = metadata.next_global_seq;
        let event_input = SegmentedSessionEventInput {
            timestamp: event.timestamp_ms.to_string(),
            kind: SegmentedSessionEventKind::SessionBoundary,
            payload: session_boundary_payload(event, assigned_sequence),
            turn_index: None,
        };
        lines.extend(append_events_with_metadata(
            session_dir,
            &mut metadata,
            std::slice::from_ref(&event_input),
        )?);
    }
    write_metadata(session_dir, &metadata)?;
    Ok(lines)
}

/// Appends only the not-yet-persisted tail of a transcript message slice to the main segmented session.
/// 只将转录消息切片中尚未持久化的尾部追加到主分段会话。
pub(super) fn append_segmented_session_message_tail(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Keep transcript tail detection, sequence assignment, and file append atomic in-process.
    // 让转录尾部检测、序号分配与文件追加在进程内保持原子。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let mut metadata = read_session_metadata(session_dir)?;
    let first_new_index = usize::try_from(metadata.message_count).map_err(to_store_error)?;
    if messages.len() < first_new_index {
        return Err(format!(
            "Segmented session append rejected non-tail transcript change: persisted message count {} exceeds incoming message count {}.",
            first_new_index,
            messages.len()
        ));
    }
    if messages.len() == first_new_index {
        validate_segmented_message_tail_boundary(&metadata, messages)?;
        return Ok(Vec::new());
    }
    validate_segmented_message_tail_boundary(&metadata, messages)?;
    let mut new_message_ids = BTreeSet::new();
    for (index, message) in messages.iter().enumerate().skip(first_new_index) {
        let message_id = stable_segmented_message_id(index, message);
        if !new_message_ids.insert(message_id.clone()) {
            return Err(format!(
                "Segmented session append rejected duplicate tail message `{message_id}` after new message index {first_new_index}."
            ));
        }
    }
    let turn_indexes =
        assign_segmented_tail_message_turn_indexes(&metadata, &messages[first_new_index..]);
    let mut events = messages
        .iter()
        .enumerate()
        .skip(first_new_index)
        .zip(turn_indexes)
        .map(|((index, message), turn_index)| {
            segmented_event_input_from_message(index, message, turn_index)
        })
        .collect::<Result<Vec<_>, _>>()?;
    backfill_tool_detail_ref_sequences(&mut events, metadata.next_global_seq);
    let appended_lines = append_events_with_metadata(session_dir, &mut metadata, &events)?;
    update_segmented_message_checkpoint(&mut metadata, messages)?;
    write_metadata(session_dir, &metadata)?;
    Ok(appended_lines)
}

/// Appends a transcript update, converting non-tail changes into an append-only rollback boundary.
/// 追加一次转录更新，并将非尾部变化转换为追加式回滚边界。
pub(super) fn append_segmented_session_message_update(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    match append_segmented_session_message_tail(session_dir, messages) {
        Ok(lines) => Ok(lines),
        Err(error) if error.contains("non-tail transcript change") => {
            append_segmented_session_replacement(session_dir, messages)
        }
        Err(error) => Err(error),
    }
}

/// Appends a rollback marker followed by the full replacement transcript.
/// 追加回滚标记，然后写入完整替换转录。
fn append_segmented_session_replacement(
    session_dir: &Path,
    messages: &[SessionMessageEntry],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Rollback replacement rewrites metadata checkpoints, so it shares the append lock.
    // 回滚替换会重写元数据检查点，因此共用追加锁。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let mut metadata = read_session_metadata(session_dir)?;
    let turn_indexes = assign_segmented_message_turn_indexes(messages);
    let timestamp = messages
        .last()
        .map(|message| non_empty_timestamp_text(&message.created_at))
        .unwrap_or_else(current_unix_millis_text);
    let replacement_history = messages
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(to_store_error)?;
    let rollback_event = SegmentedSessionEventInput {
        timestamp: timestamp.clone(),
        kind: SegmentedSessionEventKind::ThreadRolledBack,
        payload: json!({
            "reason": "non_tail_transcript_update",
            "replacement_message_count": messages.len(),
            "replacement_history": replacement_history,
        }),
        turn_index: None,
    };
    let mut events = Vec::with_capacity(messages.len().saturating_add(1));
    events.push(rollback_event);
    events.extend(
        messages
            .iter()
            .enumerate()
            .map(|(index, message)| {
                segmented_event_input_from_message(index, message, turn_indexes[index])
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    backfill_tool_detail_ref_sequences(
        &mut events[1..],
        metadata.next_global_seq.saturating_add(1),
    );
    let lines = append_events_with_metadata(session_dir, &mut metadata, &events)?;
    update_segmented_message_checkpoint(&mut metadata, messages)?;
    write_metadata(session_dir, &metadata)?;
    Ok(lines)
}

/// Validates the persisted transcript tail boundary without scanning historical segment files.
/// 不扫描历史分段文件，仅校验已持久化转录尾部边界。
fn validate_segmented_message_tail_boundary(
    metadata: &SegmentedSessionMetadata,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    let message_count = usize::try_from(metadata.message_count).map_err(to_store_error)?;
    if message_count == 0 {
        return Ok(());
    }
    let Some(previous_message) = messages.get(message_count.saturating_sub(1)) else {
        return Ok(());
    };
    let previous_message_id =
        stable_segmented_message_id(message_count.saturating_sub(1), previous_message);
    if metadata
        .last_message_id
        .as_deref()
        .is_some_and(|stored_id| stored_id != previous_message_id)
    {
        return Err(format!(
            "Segmented session append rejected non-tail transcript change: persisted tail message id changed from `{}` to `{previous_message_id}`.",
            metadata.last_message_id.as_deref().unwrap_or_default()
        ));
    }
    let previous_message_fingerprint = segmented_message_fingerprint(previous_message)?;
    if metadata
        .last_message_fingerprint
        .as_deref()
        .is_some_and(|stored_fingerprint| stored_fingerprint != previous_message_fingerprint)
    {
        return Err(format!(
            "Segmented session append rejected non-tail transcript change: persisted tail message `{previous_message_id}` changed."
        ));
    }
    Ok(())
}

/// Stores the current persisted transcript tail checkpoint in metadata.
/// 将当前已持久化转录尾部检查点写入元数据。
fn update_segmented_message_checkpoint(
    metadata: &mut SegmentedSessionMetadata,
    messages: &[SessionMessageEntry],
) -> Result<(), String> {
    metadata.message_count = messages.len() as u64;
    if let Some((index, message)) = messages.iter().enumerate().next_back() {
        metadata.last_message_id = Some(stable_segmented_message_id(index, message));
        metadata.last_message_fingerprint = Some(segmented_message_fingerprint(message)?);
    } else {
        metadata.last_message_id = None;
        metadata.last_message_fingerprint = None;
    }
    Ok(())
}

/// Builds the lightweight persisted fingerprint for one transcript message.
/// 为单条转录消息构建轻量持久化指纹。
fn segmented_message_fingerprint(message: &SessionMessageEntry) -> Result<String, String> {
    serde_json::to_string(message).map_err(to_store_error)
}

/// Reads the latest events from the active segment without scanning older segment files.
/// 只从活动分段读取最新事件，不扫描更早的分段文件。
pub(super) fn read_segmented_session_active_tail(
    session_dir: &Path,
    max_lines: usize,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if max_lines == 0 {
        return Ok(Vec::new());
    }
    let metadata = read_session_metadata(session_dir)?;
    let mut lines = read_segment_event_lines(&session_dir.join(metadata.active_segment))?;
    if lines.len() > max_lines {
        lines = lines.split_off(lines.len().saturating_sub(max_lines));
    }
    Ok(lines)
}

/// Reads a backward transcript window ending before the optional sequence cursor.
/// 读取一个结束于可选序号游标之前的反向转录窗口。
pub(super) fn read_segmented_session_event_window(
    session_dir: &Path,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    let metadata = read_session_metadata(session_dir)?;
    let cursor = before_seq.unwrap_or(u64::MAX);
    let mut window = Vec::with_capacity(limit);
    for segment in metadata.segments.iter().rev() {
        let lines = read_segment_event_lines(&session_dir.join(&segment.file))?;
        for line in lines.into_iter().rev() {
            if line.seq >= cursor {
                continue;
            }
            window.push(line);
            if window.len() == limit {
                window.reverse();
                return Ok(window);
            }
        }
    }
    window.reverse();
    Ok(window)
}

/// Reads a backward transcript window by complete display turns instead of raw event count.
/// 按完整展示轮次而不是原始事件数量读取反向转录窗口。
pub(super) fn read_segmented_session_turn_window(
    session_dir: &Path,
    before_seq: Option<u64>,
    turn_limit: usize,
) -> Result<SegmentedSessionTurnWindow, String> {
    if turn_limit == 0 {
        return Ok(SegmentedSessionTurnWindow {
            lines: Vec::new(),
            has_more_history: false,
        });
    }
    let metadata = read_session_metadata(session_dir)?;
    let cursor = before_seq.unwrap_or(u64::MAX);
    let rollback_boundary = metadata.latest_rollback_seq;
    if before_seq.is_some_and(|seq| rollback_boundary.is_some_and(|boundary| seq <= boundary)) {
        return Ok(SegmentedSessionTurnWindow {
            lines: Vec::new(),
            has_more_history: false,
        });
    }
    let mut window = Vec::new();
    let mut seen_turn_indexes = BTreeSet::new();
    for segment in metadata.segments.iter().rev() {
        if segment.start_seq >= cursor {
            continue;
        }
        let lines = read_segment_event_lines(&session_dir.join(&segment.file))?;
        for line in lines.into_iter().rev() {
            if line.seq >= cursor {
                continue;
            }
            if rollback_boundary.is_some_and(|boundary| line.seq <= boundary) {
                window.reverse();
                return Ok(SegmentedSessionTurnWindow {
                    lines: window,
                    has_more_history: false,
                });
            }
            let Some(turn_index) = segmented_line_turn_index(&line) else {
                continue;
            };
            if seen_turn_indexes.len() >= turn_limit && !seen_turn_indexes.contains(&turn_index) {
                window.reverse();
                return Ok(SegmentedSessionTurnWindow {
                    lines: window,
                    has_more_history: true,
                });
            }
            seen_turn_indexes.insert(turn_index);
            window.push(line);
        }
    }
    window.reverse();
    Ok(SegmentedSessionTurnWindow {
        lines: window,
        has_more_history: false,
    })
}

/// Reads one tool-detail page for a specific turn from segmented JSONL files.
/// 从分段 JSONL 文件读取指定轮次的一页工具详情。
pub(super) fn read_segmented_session_tool_detail_window(
    session_dir: &Path,
    turn_index: i64,
    before_seq: Option<u64>,
    limit: usize,
) -> Result<SegmentedSessionToolDetailWindow, String> {
    if limit == 0 {
        return Ok(SegmentedSessionToolDetailWindow {
            lines: Vec::new(),
            has_more_detail: false,
        });
    }
    let metadata = read_session_metadata(session_dir)?;
    let cursor = before_seq.unwrap_or(u64::MAX);
    let mut window = Vec::with_capacity(limit);
    let mut has_more_detail = false;
    for segment in metadata.segments.iter().rev() {
        if segment.start_seq >= cursor || !segment_may_contain_turn(segment, turn_index) {
            continue;
        }
        let lines = read_segment_event_lines(&session_dir.join(&segment.file))?;
        for line in lines.into_iter().rev() {
            if line.seq >= cursor || !segmented_line_matches_tool_turn(&line, turn_index)? {
                continue;
            }
            if window.len() == limit {
                has_more_detail = true;
                window.reverse();
                return Ok(SegmentedSessionToolDetailWindow {
                    lines: window,
                    has_more_detail,
                });
            }
            window.push(line);
        }
    }
    window.reverse();
    Ok(SegmentedSessionToolDetailWindow {
        lines: window,
        has_more_detail,
    })
}

/// Reads the complete tool-detail stream for a specific turn from segmented JSONL files.
/// 从分段 JSONL 文件读取指定轮次的完整工具详情流。
pub(super) fn read_segmented_session_complete_tool_detail_turn(
    session_dir: &Path,
    turn_index: i64,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    let metadata = read_session_metadata(session_dir)?;
    let mut matching_lines = Vec::new();
    for segment in &metadata.segments {
        if !segment_may_contain_turn(segment, turn_index) {
            continue;
        }
        let lines = read_segment_event_lines(&session_dir.join(&segment.file))?;
        for line in lines {
            if segmented_line_matches_tool_turn(&line, turn_index)? {
                matching_lines.push(line);
            }
        }
    }
    Ok(matching_lines)
}

/// Returns whether segment turn metadata can contain the requested display turn.
/// 返回分段轮次元数据是否可能包含指定展示轮次。
fn segment_may_contain_turn(segment: &SegmentMetadata, turn_index: i64) -> bool {
    if segment
        .first_turn_index
        .is_some_and(|first_turn_index| first_turn_index > turn_index)
    {
        return false;
    }
    if segment
        .last_turn_index
        .is_some_and(|last_turn_index| last_turn_index < turn_index)
    {
        return false;
    }
    true
}

/// Projects segmented JSONL events back into displayable transcript messages.
/// 将分段 JSONL 事件投影回可展示的转录消息。
pub(super) fn segmented_lines_to_session_messages(
    lines: &[SegmentedSessionEventLine],
) -> Result<Vec<SessionMessageEntry>, String> {
    let mut messages = Vec::new();
    for line in lines {
        if let Some(message) = segmented_line_to_session_message(line)? {
            messages.push(message);
        }
    }
    Ok(messages)
}

/// Rebuilds the current conversation state from segmented compaction history and suffix events.
/// 根据分段压缩历史与后缀事件重建当前会话状态。
pub(super) fn replay_segmented_session_context(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionReplayResult>, String> {
    let metadata = read_session_metadata(session_dir)?;
    let scan = find_latest_compaction_anchor(session_dir, &metadata.segments)?;
    let mut messages = scan
        .anchor
        .as_ref()
        .map(|anchor| replacement_history_to_session_messages(&anchor.replacement_history))
        .transpose()?
        .unwrap_or_default();
    let suffix = suffix_after_latest_rollback(&scan);
    messages.extend(segmented_lines_to_session_messages(&suffix)?);
    if messages.is_empty() {
        return Ok(None);
    }
    let conversation_state = conversation_state_from_session_messages(&messages);
    let updated_at = suffix
        .last()
        .map(|line| line.timestamp.clone())
        .or_else(|| {
            scan.anchor
                .as_ref()
                .map(|anchor| anchor.compaction_seq.to_string())
        });
    let last_compaction_id = scan
        .anchor
        .as_ref()
        .map(|anchor| format!("segmented-compaction-{}", anchor.compaction_seq));
    let context_start_message_id = messages
        .first()
        .and_then(|message| (!message.id.trim().is_empty()).then(|| message.id.clone()));
    Ok(Some(SegmentedSessionReplayResult {
        conversation_state,
        last_compaction_id,
        context_start_message_id,
        updated_at,
    }))
}

/// Rebuilds provider-facing runtime JSONL from the canonical conversation state.
/// 根据规范会话状态重建 Provider 可见运行时 JSONL。
pub(super) fn rebuild_runtime_log_from_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
) -> Result<RuntimeLogMetadata, String> {
    rebuild_runtime_log_from_conversation_state_with_extra_events(
        session_dir,
        conversation_state,
        updated_at,
        Vec::new(),
    )
}

/// Rebuilds provider-facing runtime JSONL while preserving audit-only runtime events.
/// 在保留仅审计运行时事件的同时重建 Provider 可见运行时 JSONL。
fn rebuild_runtime_log_from_conversation_state_with_extra_events(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
    extra_events: Vec<RuntimeLogEventLine>,
) -> Result<RuntimeLogMetadata, String> {
    let runtime_dir = runtime_dir_path(session_dir);
    if runtime_dir.exists() {
        fs::remove_dir_all(&runtime_dir).map_err(to_store_error)?;
    }
    create_dir_all(&runtime_dir)?;
    let active_segment = runtime_segment_file_name(1);
    let active_segment_path = session_dir.join(&active_segment);
    let mut lines = runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1);
    append_runtime_extra_events_with_fresh_sequences(&mut lines, extra_events);
    write_json_lines_file_atomic(&active_segment_path, &lines)?;
    let byte_len = fs::metadata(&active_segment_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0);
    let mut segment =
        RuntimeSegmentMetadata::empty(active_segment.clone(), 1, updated_at.to_string());
    segment.line_count = lines.len() as u64;
    segment.byte_len = byte_len;
    segment.end_seq = lines.last().map(|line| line.seq);
    segment.updated_at = lines.last().map(|line| line.created_at.clone());
    let metadata = RuntimeLogMetadata {
        schema_version: 1,
        active_segment,
        next_seq: lines
            .last()
            .map(|line| line.seq.saturating_add(1))
            .unwrap_or(1),
        created_at: updated_at.to_string(),
        updated_at: updated_at.to_string(),
        segments: vec![segment],
    };
    write_runtime_metadata(session_dir, &metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&lines))?;
    Ok(metadata)
}

/// Appends runtime JSONL tail lines when possible and rebuilds when history no longer matches.
/// 在可能时追加 runtime JSONL 尾部，并在历史不再匹配时重建。
pub(super) fn sync_runtime_log_from_conversation_state(
    session_dir: &Path,
    conversation_state: &SessionConversationState,
    updated_at: &str,
) -> Result<RuntimeLogMetadata, String> {
    if !runtime_metadata_path(session_dir).exists() {
        return rebuild_runtime_log_from_conversation_state(
            session_dir,
            conversation_state,
            updated_at,
        );
    }
    let desired_lines =
        runtime_log_lines_from_conversation_state(conversation_state, updated_at, 1);
    let existing_lines = match read_runtime_log_lines(session_dir) {
        Ok(Some(lines)) => lines,
        Ok(None) => {
            return rebuild_runtime_log_from_conversation_state(
                session_dir,
                conversation_state,
                updated_at,
            );
        }
        Err(_) => {
            return rebuild_runtime_log_from_conversation_state(
                session_dir,
                conversation_state,
                updated_at,
            );
        }
    };
    let extra_events = runtime_extra_event_lines(&existing_lines);
    let existing_conversation_lines = runtime_conversation_event_lines(&existing_lines);
    if !runtime_log_prefix_matches(&existing_conversation_lines, &desired_lines) {
        return rebuild_runtime_log_from_conversation_state_with_extra_events(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
        );
    }
    if existing_conversation_lines.len() == desired_lines.len() {
        return read_runtime_metadata(session_dir);
    }
    if !extra_events.is_empty() {
        return rebuild_runtime_log_from_conversation_state_with_extra_events(
            session_dir,
            conversation_state,
            updated_at,
            extra_events,
        );
    }
    let tail = &desired_lines[existing_conversation_lines.len()..];
    append_runtime_log_tail(session_dir, tail)
}

/// Ensures a provider-facing runtime log exists, rebuilding it from display segments when needed.
/// 确保存在 Provider 可见运行时日志，缺失时从展示分段重建。
pub(super) fn ensure_runtime_log_from_segmented_context(
    session_dir: &Path,
) -> Result<Option<SessionConversationState>, String> {
    if runtime_metadata_path(session_dir).exists() {
        match read_runtime_conversation_state(session_dir) {
            Ok(Some(conversation_state)) if !conversation_state.messages.is_empty() => {
                return Ok(Some(conversation_state));
            }
            Ok(Some(conversation_state)) => {
                if replay_segmented_session_context(session_dir)?
                    .as_ref()
                    .map(|replay| replay.conversation_state.messages.is_empty())
                    .unwrap_or(true)
                {
                    return Ok(Some(conversation_state));
                }
            }
            Ok(None) => return Ok(None),
            Err(error) => {
                let Some(replay) = replay_segmented_session_context(session_dir)? else {
                    return Err(error);
                };
                let updated_at = replay
                    .updated_at
                    .clone()
                    .unwrap_or_else(current_unix_millis_text);
                rebuild_runtime_log_from_conversation_state(
                    session_dir,
                    &replay.conversation_state,
                    &updated_at,
                )?;
                return Ok(Some(replay.conversation_state));
            }
        }
    }
    let Some(replay) = replay_segmented_session_context(session_dir)? else {
        return Ok(None);
    };
    let updated_at = replay
        .updated_at
        .clone()
        .unwrap_or_else(current_unix_millis_text);
    rebuild_runtime_log_from_conversation_state(
        session_dir,
        &replay.conversation_state,
        &updated_at,
    )?;
    Ok(Some(replay.conversation_state))
}

/// Reads provider-facing runtime log lines in metadata segment order.
/// 按元数据分段顺序读取 Provider 可见运行时日志行。
pub(super) fn read_runtime_log_lines(
    session_dir: &Path,
) -> Result<Option<Vec<RuntimeLogEventLine>>, String> {
    let metadata_path = runtime_metadata_path(session_dir);
    if !metadata_path.exists() {
        return Ok(None);
    }
    let metadata = read_runtime_metadata(session_dir)?;
    let mut segments = metadata.segments.clone();
    segments.sort_by_key(|segment| segment.start_seq);
    let mut lines = Vec::new();
    let mut repair_summaries = Vec::new();
    for segment in segments {
        let segment_path = session_dir.join(&segment.file);
        let repair_read = read_runtime_segment_lines_tolerant(&segment_path)?;
        if !repair_read.corrupt_line_numbers.is_empty() {
            backup_corrupt_runtime_segment(&segment_path)?;
            write_json_lines_file_atomic(&segment_path, &repair_read.lines)?;
            repair_summaries.push(format!(
                "{} lines {}",
                segment.file,
                compact_runtime_line_number_summary(&repair_read.corrupt_line_numbers)
            ));
        }
        lines.extend(repair_read.lines);
    }
    if !repair_summaries.is_empty() {
        if lines.is_empty() {
            return Err(format!(
                "Runtime log is fully damaged after repairing segments: {}",
                repair_summaries.join(", ")
            ));
        }
        let repair_detail = format!(
            "Removed corrupt runtime JSONL rows from segments: {}",
            repair_summaries.join(", ")
        );
        lines.push(RuntimeLogEventLine::new(
            next_runtime_line_seq(&lines),
            current_unix_millis_text(),
            RuntimeLogEvent::RuntimeRepair(RuntimeRepairEvent {
                repair_id: format!(
                    "runtime-repair-{}-{}",
                    current_unix_millis_text(),
                    uuid::Uuid::new_v4()
                ),
                source: String::from("runtime_jsonl_tolerant_read"),
                detail: repair_detail,
                downgraded_tool_segments: 0,
            }),
        ));
        append_runtime_line_to_active_segment(
            session_dir,
            &metadata,
            lines.last().expect("repair line"),
        )?;
        refresh_runtime_metadata_from_lines(session_dir, &metadata, &lines)?;
        write_runtime_index(session_dir, &rebuild_runtime_log_index(&lines))?;
    }
    Ok(Some(lines))
}

/// Reads the provider-facing runtime log as a canonical conversation state.
/// 将 Provider 可见运行时日志读取为规范会话状态。
pub(super) fn read_runtime_conversation_state(
    session_dir: &Path,
) -> Result<Option<SessionConversationState>, String> {
    let Some(lines) = read_runtime_log_lines(session_dir)? else {
        return Ok(None);
    };
    Ok(Some(conversation_state_from_runtime_log_lines(&lines)))
}

/// Copies active runtime-log files before context compaction mutates the conversation window.
/// 在上下文压缩修改会话窗口前复制当前运行时日志文件。
pub(super) fn backup_runtime_log_before_compaction(
    session_dir: &Path,
    compaction_id: &str,
    timestamp: &str,
) -> Result<Vec<String>, String> {
    let metadata_path = runtime_metadata_path(session_dir);
    let index_path = runtime_index_path(session_dir);
    let runtime_dir = runtime_dir_path(session_dir);
    if !metadata_path.exists() && !index_path.exists() && !runtime_dir.exists() {
        return Ok(Vec::new());
    }

    let backup_root = runtime_backup_dir_path(session_dir);
    create_dir_all(&backup_root)?;
    let base_name = runtime_compaction_backup_dir_name(session_dir, compaction_id, timestamp);
    let mut backup_dir = backup_root.join(&base_name);
    if backup_dir.exists() {
        backup_dir = backup_root.join(format!("{base_name}-{}", uuid::Uuid::new_v4()));
    }
    create_dir_all(&backup_dir)?;

    let mut copied_files = Vec::new();
    copy_runtime_file_into_backup(session_dir, &metadata_path, &backup_dir, &mut copied_files)?;
    copy_runtime_file_into_backup(session_dir, &index_path, &backup_dir, &mut copied_files)?;
    if runtime_dir.exists() {
        copy_runtime_dir_into_backup(
            session_dir,
            &runtime_dir,
            &backup_dir.join(SEGMENTED_RUNTIME_DIR),
            &mut copied_files,
        )?;
    }
    prune_runtime_log_backups(session_dir, RUNTIME_COMPACTION_BACKUP_LIMIT)?;
    append_runtime_log_event_if_present(
        session_dir,
        timestamp,
        RuntimeLogEvent::ContextCompactionApplied(RuntimeContextCompactionAppliedEvent {
            compaction_id: compaction_id.to_string(),
            summary_message_id: None,
            backup_files: copied_files.clone(),
            compaction_fingerprint: Some(runtime_compaction_fingerprint(
                compaction_id,
                timestamp,
                &copied_files,
            )),
            merged_injection_fingerprint: None,
        }),
    )?;
    Ok(copied_files)
}

/// Records the injection view that was applied to one provider-facing turn.
/// 记录单个 Provider 可见回合已应用的注入视图。
pub(super) fn record_runtime_injection_applied(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeInjectionAppliedEvent,
) -> Result<bool, String> {
    append_runtime_log_event_if_present(
        session_dir,
        timestamp,
        RuntimeLogEvent::InjectionApplied(event),
    )
}

/// Records one runtime-only audit event without projecting it into the visible conversation.
/// 记录单个仅运行时审计事件，不将其投影进可见会话。
pub(super) fn record_runtime_audit_event(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeLogEvent,
) -> Result<bool, String> {
    append_runtime_log_event_if_present(session_dir, timestamp, event)
}

/// Appends one runtime event when the session already owns a runtime log.
/// 当会话已经拥有 runtime 日志时追加一条运行时事件。
fn append_runtime_log_event_if_present(
    session_dir: &Path,
    timestamp: &str,
    event: RuntimeLogEvent,
) -> Result<bool, String> {
    if !runtime_metadata_path(session_dir).exists() {
        let _ = ensure_runtime_log_from_segmented_context(session_dir)?;
    }
    if !runtime_metadata_path(session_dir).exists() {
        return Ok(false);
    }
    let metadata = read_runtime_metadata(session_dir)?;
    let line = RuntimeLogEventLine::new(metadata.next_seq, timestamp.to_string(), event);
    append_runtime_log_tail(session_dir, &[line])?;
    Ok(true)
}

/// Rebuilds the lightweight runtime index from runtime JSONL lines.
/// 从运行时 JSONL 行重建轻量运行时索引。
fn rebuild_runtime_log_index(lines: &[RuntimeLogEventLine]) -> RuntimeLogIndex {
    let mut index = RuntimeLogIndex {
        schema_version: 1,
        last_seq: lines.last().map(|line| line.seq),
        line_count: lines.len() as u64,
        latest_compaction_seq: None,
        message_offsets: Default::default(),
        tool_call_offsets: Default::default(),
        provider_checkpoint_offsets: Default::default(),
        repair_offsets: Default::default(),
        injection_offsets: Default::default(),
        system_update_offsets: Default::default(),
        injection_cancelled_offsets: Default::default(),
        injection_merged_offsets: Default::default(),
        provider_response_checkpoint_offsets: Default::default(),
        provider_lowering_repair_offsets: Default::default(),
    };
    for line in lines {
        match &line.event {
            vulcan_context::RuntimeLogEvent::UserMessage(event) => {
                index
                    .message_offsets
                    .insert(event.message_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::AssistantMessage(event) => {
                index
                    .message_offsets
                    .insert(event.message_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ToolCall(event) => {
                index
                    .tool_call_offsets
                    .insert(event.runtime_tool_call_id.clone(), line.seq);
                if let Some(provider_tool_call_id) = &event.provider_tool_call_id {
                    index
                        .tool_call_offsets
                        .insert(provider_tool_call_id.clone(), line.seq);
                }
            }
            vulcan_context::RuntimeLogEvent::ToolResult(event) => {
                index
                    .message_offsets
                    .insert(event.tool_result_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ContextCompactionApplied(_) => {
                index.latest_compaction_seq = Some(line.seq);
            }
            vulcan_context::RuntimeLogEvent::ProviderTurnCheckpoint(event) => {
                index.provider_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}",
                        event.provider_id, event.model_id, event.converter_version
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::RuntimeRepair(event) => {
                index
                    .repair_offsets
                    .insert(event.repair_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionApplied(event) => {
                if let Some(fingerprint) = event
                    .transient_fingerprint
                    .as_ref()
                    .or(event.persistent_fingerprint.as_ref())
                {
                    index
                        .injection_offsets
                        .insert(fingerprint.clone(), line.seq);
                }
            }
            vulcan_context::RuntimeLogEvent::SystemUpdate(event) => {
                index
                    .system_update_offsets
                    .insert(event.update_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionCancelled(event) => {
                index
                    .injection_cancelled_offsets
                    .insert(event.cancellation_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::InjectionMerged(event) => {
                index
                    .injection_merged_offsets
                    .insert(event.merge_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ProviderRequestCheckpoint(event) => {
                index.provider_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}:{}",
                        event.provider_family,
                        event.provider_id,
                        event.model_id,
                        event.checkpoint_id
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::ProviderResponseCheckpoint(event) => {
                index.provider_response_checkpoint_offsets.insert(
                    format!(
                        "{}:{}:{}:{}",
                        event.provider_family,
                        event.provider_id,
                        event.model_id,
                        event.checkpoint_id
                    ),
                    line.seq,
                );
            }
            vulcan_context::RuntimeLogEvent::ProviderLoweringRepair(event) => {
                index
                    .provider_lowering_repair_offsets
                    .insert(event.repair_id.clone(), line.seq);
            }
            vulcan_context::RuntimeLogEvent::ToolBatchStarted(_)
            | vulcan_context::RuntimeLogEvent::ToolBatchCompleted(_) => {}
        }
    }
    index
}

/// Returns runtime lines that rebuild the canonical provider conversation.
/// 返回用于重建规范 Provider 会话的运行时日志行。
fn runtime_conversation_event_lines(lines: &[RuntimeLogEventLine]) -> Vec<RuntimeLogEventLine> {
    lines
        .iter()
        .filter(|line| runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Returns runtime-only audit lines that must survive conversation projection rebuilds.
/// 返回必须在会话投影重建后继续保留的仅审计运行时日志行。
fn runtime_extra_event_lines(lines: &[RuntimeLogEventLine]) -> Vec<RuntimeLogEventLine> {
    lines
        .iter()
        .filter(|line| !runtime_event_projects_to_conversation(&line.event))
        .cloned()
        .collect()
}

/// Reports whether one runtime event contributes to canonical conversation replay.
/// 判断单条运行时事件是否参与规范会话回放。
fn runtime_event_projects_to_conversation(event: &RuntimeLogEvent) -> bool {
    matches!(
        event,
        RuntimeLogEvent::UserMessage(_)
            | RuntimeLogEvent::AssistantMessage(_)
            | RuntimeLogEvent::ToolBatchStarted(_)
            | RuntimeLogEvent::ToolCall(_)
            | RuntimeLogEvent::ToolResult(_)
            | RuntimeLogEvent::ToolBatchCompleted(_)
    )
}

/// Appends audit events after conversation lines while assigning fresh sequence numbers.
/// 将审计事件追加到会话日志之后，并分配新的连续序号。
fn append_runtime_extra_events_with_fresh_sequences(
    lines: &mut Vec<RuntimeLogEventLine>,
    extra_events: Vec<RuntimeLogEventLine>,
) {
    let mut next_seq = next_runtime_line_seq(lines);
    for event_line in extra_events {
        lines.push(RuntimeLogEventLine::new(
            next_seq,
            event_line.created_at,
            event_line.event,
        ));
        next_seq = next_seq.saturating_add(1);
    }
}

/// Computes the next runtime sequence number after the provided lines.
/// 根据已提供日志行计算下一个运行时序号。
fn next_runtime_line_seq(lines: &[RuntimeLogEventLine]) -> u64 {
    lines
        .iter()
        .map(|line| line.seq)
        .max()
        .map(|seq| seq.saturating_add(1))
        .unwrap_or(1)
}

/// Reads one runtime JSONL segment while preserving valid lines around corrupt records.
/// 读取单个 runtime JSONL 分段，并在损坏记录周围保留有效行。
fn read_runtime_segment_lines_tolerant(path: &Path) -> Result<RuntimeLogRepairRead, String> {
    if !path.exists() {
        return Ok(RuntimeLogRepairRead::default());
    }
    let file = File::open(path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut read = RuntimeLogRepairRead::default();
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index.saturating_add(1);
        let line = line.map_err(|error| {
            format!(
                "读取 runtime JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<RuntimeLogEventLine>(&line) {
            Ok(value) => read.lines.push(value),
            Err(_) => {
                read.corrupt_line_numbers.push(line_number);
            }
        }
    }
    Ok(read)
}

/// Compacts one-based line numbers into a short comma-separated repair summary.
/// 将 1 基行号压缩为简短的逗号分隔修复摘要。
fn compact_runtime_line_number_summary(line_numbers: &[usize]) -> String {
    const MAX_VISIBLE_LINES: usize = 6;
    if line_numbers.is_empty() {
        return "unknown".to_string();
    }
    let mut visible = line_numbers
        .iter()
        .take(MAX_VISIBLE_LINES)
        .map(|line_number| line_number.to_string())
        .collect::<Vec<_>>();
    if line_numbers.len() > MAX_VISIBLE_LINES {
        visible.push(format!("+{} more", line_numbers.len() - MAX_VISIBLE_LINES));
    }
    visible.join(",")
}

/// Preserves a damaged runtime segment before rewriting the cleaned JSONL file.
/// 在重写清洁 JSONL 文件前保留损坏的运行时分段。
fn backup_corrupt_runtime_segment(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Ok(());
    }
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("runtime.jsonl");
    let backup_name = format!(
        "{file_name}.corrupt-{}-{}.bak",
        current_unix_millis_text(),
        uuid::Uuid::new_v4()
    );
    fs::copy(path, path.with_file_name(backup_name))
        .map(|_| ())
        .map_err(to_store_error)
}

/// Refreshes runtime metadata after a tolerant read removed corrupt JSONL rows.
/// 在容错读取移除损坏 JSONL 行后刷新运行时元数据。
fn refresh_runtime_metadata_from_lines(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
    lines: &[RuntimeLogEventLine],
) -> Result<(), String> {
    let mut next_metadata = metadata.clone();
    next_metadata.next_seq = lines
        .last()
        .map(|line| line.seq.saturating_add(1))
        .unwrap_or(1);
    next_metadata.updated_at = lines
        .last()
        .map(|line| line.created_at.clone())
        .unwrap_or_else(current_unix_millis_text);
    for segment in &mut next_metadata.segments {
        let segment_path = session_dir.join(&segment.file);
        let segment_lines = read_json_lines::<RuntimeLogEventLine>(&segment_path)?;
        segment.line_count = segment_lines.len() as u64;
        segment.byte_len = fs::metadata(&segment_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        segment.end_seq = segment_lines.last().map(|line| line.seq);
        segment.updated_at = segment_lines.last().map(|line| line.created_at.clone());
    }
    write_runtime_metadata(session_dir, &next_metadata)
}

/// Appends one already-sequenced runtime line to the active segment without refreshing metadata.
/// 将一条已分配序号的运行时日志行追加到活动分段，但不刷新元数据。
fn append_runtime_line_to_active_segment(
    session_dir: &Path,
    metadata: &RuntimeLogMetadata,
    line: &RuntimeLogEventLine,
) -> Result<(), String> {
    // Runtime single-line appends use the same lock to avoid JSONL byte interleaving.
    // 运行时单行追加使用同一把锁，避免 JSONL 字节交错。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let active_segment_path = session_dir.join(&metadata.active_segment);
    if !active_segment_path.exists() {
        return Err(format!(
            "Runtime active segment is missing: {}",
            active_segment_path.display()
        ));
    }
    append_json_lines_file(&active_segment_path, std::slice::from_ref(line))
}

/// Appends new runtime lines to the active segment and refreshes metadata plus index.
/// 将新的运行时行追加到活动分段，并刷新元数据与索引。
fn append_runtime_log_tail(
    session_dir: &Path,
    tail: &[RuntimeLogEventLine],
) -> Result<RuntimeLogMetadata, String> {
    // Runtime tail append also refreshes metadata, so keep the batch serialized.
    // 运行时尾部追加也会刷新元数据，因此保持批量串行。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let mut metadata = read_runtime_metadata(session_dir)?;
    let active_segment_path = session_dir.join(&metadata.active_segment);
    if !active_segment_path.exists() {
        return Err(format!(
            "Runtime active segment is missing: {}",
            active_segment_path.display()
        ));
    }
    append_json_lines_file(&active_segment_path, tail)?;
    let all_lines = read_runtime_log_lines(session_dir)?.unwrap_or_default();
    metadata.next_seq = all_lines
        .last()
        .map(|line| line.seq.saturating_add(1))
        .unwrap_or(1);
    metadata.updated_at = tail
        .last()
        .map(|line| line.created_at.clone())
        .unwrap_or_else(current_unix_millis_text);
    for segment in &mut metadata.segments {
        if segment.file != metadata.active_segment {
            continue;
        }
        let segment_lines = read_json_lines::<RuntimeLogEventLine>(&active_segment_path)?;
        segment.line_count = segment_lines.len() as u64;
        segment.byte_len = fs::metadata(&active_segment_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        segment.end_seq = segment_lines.last().map(|line| line.seq);
        segment.updated_at = segment_lines.last().map(|line| line.created_at.clone());
        break;
    }
    write_runtime_metadata(session_dir, &metadata)?;
    write_runtime_index(session_dir, &rebuild_runtime_log_index(&all_lines))?;
    Ok(metadata)
}

/// Returns whether the existing runtime log is a provider-equivalent prefix of the desired log.
/// 返回现有运行时日志是否为目标日志的 Provider 等价前缀。
fn runtime_log_prefix_matches(
    existing_lines: &[RuntimeLogEventLine],
    desired_lines: &[RuntimeLogEventLine],
) -> bool {
    existing_lines.len() <= desired_lines.len()
        && existing_lines
            .iter()
            .zip(desired_lines)
            .all(|(existing, desired)| runtime_log_line_matches(existing, desired))
}

/// Compares two runtime lines while ignoring write timestamps that can differ across saves.
/// 比较两条运行时日志行，并忽略不同保存时可能变化的写入时间戳。
fn runtime_log_line_matches(existing: &RuntimeLogEventLine, desired: &RuntimeLogEventLine) -> bool {
    existing.seq == desired.seq
        && existing.schema_version == desired.schema_version
        && serde_json::to_value(&existing.event).ok() == serde_json::to_value(&desired.event).ok()
}

/// Reads runtime metadata for one segmented session directory.
/// 读取某个分段会话目录的运行时元数据。
fn read_runtime_metadata(session_dir: &Path) -> Result<RuntimeLogMetadata, String> {
    read_required_json::<RuntimeLogMetadata>(&runtime_metadata_path(session_dir))
}

/// Writes runtime metadata for one segmented session directory.
/// 写入某个分段会话目录的运行时元数据。
fn write_runtime_metadata(session_dir: &Path, metadata: &RuntimeLogMetadata) -> Result<(), String> {
    write_json_file_atomic(&runtime_metadata_path(session_dir), metadata)
}

/// Writes the rebuildable runtime index for one segmented session directory.
/// 写入某个分段会话目录的可重建运行时索引。
fn write_runtime_index(session_dir: &Path, index: &RuntimeLogIndex) -> Result<(), String> {
    write_json_file_atomic(&runtime_index_path(session_dir), index)
}

/// Returns the runtime metadata path for one session directory.
/// 返回某个会话目录的运行时元数据路径。
fn runtime_metadata_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE)
}

/// Returns the runtime index path for one session directory.
/// 返回某个会话目录的运行时索引路径。
fn runtime_index_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE)
}

/// Returns the runtime segment directory path for one session directory.
/// 返回某个会话目录的运行时分段目录路径。
fn runtime_dir_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_DIR)
}

/// Returns the runtime backup directory path for one session directory.
/// 返回某个会话目录的运行时备份目录路径。
fn runtime_backup_dir_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_RUNTIME_BACKUP_DIR)
}

/// Builds a runtime segment file name relative to the session directory.
/// 构建相对会话目录的运行时分段文件名。
fn runtime_segment_file_name(index: usize) -> String {
    format!("{SEGMENTED_RUNTIME_DIR}/r{index:03}.jsonl")
}

/// Overwrites the state document for one main session or task session directory.
/// 覆盖写入某个主会话或子任务会话目录的状态文档。
pub(super) fn write_state(
    session_dir: &Path,
    state: &SegmentedSessionStateDocument,
) -> Result<(), String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(|error| format!("锁定分段 state.json 失败：{error}"))?;
    let mut next_state = state.clone();
    if next_state.session_boundary_state.is_null() {
        if let Some(existing_state) = read_state_unlocked(session_dir)? {
            if !existing_state.session_boundary_state.is_null() {
                next_state.session_boundary_state = existing_state.session_boundary_state;
            }
        }
    }
    write_state_unlocked(session_dir, &next_state)
}

/// Reads the overwrite-only state document for one segmented session directory.
/// 读取某个分段会话目录的覆盖式状态文档。
pub(super) fn read_state(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionStateDocument>, String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(|error| format!("锁定分段 state.json 失败：{error}"))?;
    read_state_unlocked(session_dir)
}

/// Reads state.json while the caller already owns the state IO lock.
/// 在调用方已持有状态 IO 锁时读取 state.json。
fn read_state_unlocked(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionStateDocument>, String> {
    let path = state_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    read_required_json(&path).map(Some)
}

/// Writes state.json while the caller already owns the state IO lock.
/// 在调用方已持有状态 IO 锁时写入 state.json。
fn write_state_unlocked(
    session_dir: &Path,
    state: &SegmentedSessionStateDocument,
) -> Result<(), String> {
    write_json_file_atomic(&state_path(session_dir), state)
}

/// Replaces only the session-boundary thread state inside state.json.
/// 仅覆盖 state.json 内的 会话边界线程状态。
pub(super) fn replace_session_boundary_state(
    session_dir: &Path,
    state: Value,
) -> Result<(), String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(|error| format!("锁定分段 state.json 失败：{error}"))?;
    let mut state_document =
        read_state_unlocked(session_dir)?.unwrap_or_else(|| SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: String::new(),
            updated_at: super::current_unix_millis_text(),
            runtime_state: Value::Null,
            current_context: Value::Null,
            session_boundary_state: Value::Null,
        });
    state_document.session_boundary_state = state;
    state_document.updated_at = super::current_unix_millis_text();
    write_state_unlocked(session_dir, &state_document)
}

/// Loads the session-boundary thread state from state.json when present.
/// 从 state.json 读取存在的 会话边界线程状态。
pub(super) fn load_session_boundary_state(session_dir: &Path) -> Result<Option<Value>, String> {
    Ok(read_state(session_dir)?.and_then(|state| {
        (!state.session_boundary_state.is_null()).then_some(state.session_boundary_state)
    }))
}

/// Builds a segmented state document from the existing runtime-state entry.
/// 根据现有运行态条目构建分段状态文档。
pub(super) fn state_document_from_runtime_state(
    session_id: &str,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<SegmentedSessionStateDocument, String> {
    // Serialize only lightweight runtime metadata; canonical conversation history lives in append-only segments.
    // 仅序列化轻量运行时元数据；规范会话历史保存在追加式分段中。
    let mut runtime_state_value = serde_json::to_value(runtime_state).map_err(to_store_error)?;
    if let Some(runtime_state_object) = runtime_state_value.as_object_mut() {
        runtime_state_object.remove("conversation_state");
    }
    // Keep the current-context view explicit without duplicating the canonical conversation window.
    // 显式保留当前上下文视图，但不重复保存规范会话窗口。
    let current_context = json!({
        "context_start_message_id": runtime_state.context_start_message_id.clone(),
        "last_compaction_id": runtime_state.last_compaction_id.clone(),
        "anchor_context_budget_fingerprint": runtime_state
            .anchor_context_budget_fingerprint
            .clone(),
        "anchor_context_assembly_fingerprint": runtime_state
            .anchor_context_assembly_fingerprint
            .clone()
    });
    Ok(SegmentedSessionStateDocument {
        schema_version: 1,
        session_id: session_id.to_string(),
        updated_at: runtime_state.updated_at.clone(),
        runtime_state: runtime_state_value,
        current_context,
        session_boundary_state: Value::Null,
    })
}

/// Applies an overwrite-only metadata patch to a main segmented session.
/// 对主分段会话应用覆盖式元数据补丁。
pub(super) fn patch_segmented_session_metadata(
    session_dir: &Path,
    patch: SegmentedSessionMetadataPatch,
) -> Result<SegmentedSessionMetadata, String> {
    // Load the current metadata so omitted fields remain unchanged.
    // 加载当前元数据，确保未提供字段保持不变。
    let mut metadata = read_session_metadata(session_dir)?;
    if let Some(updated_at) = patch.updated_at {
        metadata.updated_at = updated_at;
    }
    if let Some(title) = patch.title {
        metadata.title = title;
    }
    if let Some(title_source) = patch.title_source {
        metadata.title_source = title_source;
    }
    if let Some(archived_at) = patch.archived_at {
        metadata.archived_at = archived_at;
    }
    write_metadata(session_dir, &metadata)?;
    Ok(metadata)
}

/// Moves one active session directory into the archived tree and marks metadata archived.
/// 将一个活动会话目录移动到归档树，并在元数据中标记归档。
pub(super) fn archive_segmented_session(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<PathBuf, String> {
    // Resolve both sides with the same leaf layout so segment paths remain unchanged.
    // 使用相同叶子布局解析两侧路径，确保分段路径保持不变。
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    // Resolve the archived destination before moving the directory.
    // 移动目录前解析归档目标路径。
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    if let Some(parent) = archived_dir.parent() {
        create_dir_all(parent)?;
    }
    fs::rename(&active_dir, &archived_dir).map_err(to_store_error)?;
    patch_segmented_session_metadata(
        &archived_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(archived_at.to_string()),
            archived_at: Some(Some(archived_at.to_string())),
            ..Default::default()
        },
    )?;
    Ok(archived_dir)
}

/// Finds and moves one active segmented session directory into the archived tree.
/// 查找并将一个活动分段会话目录移动到归档树。
pub(super) fn archive_segmented_session_by_id(
    data_root: &Path,
    project_tag: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(active_dir) =
        find_segmented_session_directory(data_root, false, project_tag, session_id)?
    else {
        return Ok(None);
    };
    let Some(leaf_name) = active_dir.file_name().map(|value| value.to_owned()) else {
        return Err(format!(
            "Segmented session directory has no leaf name: {}",
            active_dir.display()
        ));
    };
    let archived_dir = data_root
        .join(SEGMENTED_ARCHIVED_SESSIONS_DIR)
        .join(sanitize_file_component(project_tag))
        .join(leaf_name);
    move_segmented_session_directory(&active_dir, &archived_dir)?;
    patch_segmented_session_metadata(
        &archived_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(archived_at.to_string()),
            archived_at: Some(Some(archived_at.to_string())),
            ..Default::default()
        },
    )?;
    Ok(Some(archived_dir))
}

/// Moves one active segmented session directory into the archived tree by its exact storage date.
/// 根据精确存储日期将一个活动分段会话目录移动到归档树。
pub(super) fn archive_segmented_session_by_created_date(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    archived_at: &str,
) -> Result<Option<PathBuf>, String> {
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    if !active_dir.exists() {
        return Ok(None);
    }
    archive_segmented_session(
        data_root,
        project_tag,
        created_date,
        session_id,
        archived_at,
    )
    .map(Some)
}

/// Moves one archived session directory back into the active tree and clears archived metadata.
/// 将一个归档会话目录移回活动树，并清除元数据中的归档标记。
pub(super) fn restore_segmented_session(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    restored_at: &str,
) -> Result<PathBuf, String> {
    // Resolve both sides with the same leaf layout so segment paths remain unchanged.
    // 使用相同叶子布局解析两侧路径，确保分段路径保持不变。
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    // Resolve the active destination before moving the directory.
    // 移动目录前解析活动目标路径。
    let active_dir =
        segmented_session_directory(data_root, false, project_tag, created_date, session_id);
    if let Some(parent) = active_dir.parent() {
        create_dir_all(parent)?;
    }
    fs::rename(&archived_dir, &active_dir).map_err(to_store_error)?;
    patch_segmented_session_metadata(
        &active_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(restored_at.to_string()),
            archived_at: Some(None),
            ..Default::default()
        },
    )?;
    Ok(active_dir)
}

/// Finds and moves one archived segmented session directory back into the active tree.
/// 查找并将一个归档分段会话目录移回活动树。
pub(super) fn restore_segmented_session_by_id(
    data_root: &Path,
    project_tag: &str,
    session_id: &str,
    restored_at: &str,
) -> Result<Option<PathBuf>, String> {
    let Some(archived_dir) =
        find_segmented_session_directory(data_root, true, project_tag, session_id)?
    else {
        return Ok(None);
    };
    let Some(leaf_name) = archived_dir.file_name().map(|value| value.to_owned()) else {
        return Err(format!(
            "Segmented archived session directory has no leaf name: {}",
            archived_dir.display()
        ));
    };
    let active_dir = data_root
        .join(SEGMENTED_SESSIONS_DIR)
        .join(sanitize_file_component(project_tag))
        .join(leaf_name);
    move_segmented_session_directory(&archived_dir, &active_dir)?;
    patch_segmented_session_metadata(
        &active_dir,
        SegmentedSessionMetadataPatch {
            updated_at: Some(restored_at.to_string()),
            archived_at: Some(None),
            ..Default::default()
        },
    )?;
    Ok(Some(active_dir))
}

/// Moves one archived segmented session directory back by its exact storage date.
/// 根据精确存储日期将一个归档分段会话目录移回活动树。
pub(super) fn restore_segmented_session_by_created_date(
    data_root: &Path,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
    restored_at: &str,
) -> Result<Option<PathBuf>, String> {
    let archived_dir =
        segmented_session_directory(data_root, true, project_tag, created_date, session_id);
    if !archived_dir.exists() {
        return Ok(None);
    }
    restore_segmented_session(
        data_root,
        project_tag,
        created_date,
        session_id,
        restored_at,
    )
    .map(Some)
}

/// Deletes one active or archived segmented session directory by session identifier.
/// 按会话标识删除一个活动或归档分段会话目录。
pub(super) fn delete_segmented_session_by_id(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    session_id: &str,
) -> Result<bool, String> {
    let Some(session_dir) =
        find_segmented_session_directory(data_root, archived, project_tag, session_id)?
    else {
        return Ok(false);
    };
    fs::remove_dir_all(&session_dir).map_err(to_store_error)?;
    Ok(true)
}

/// Deletes one segmented session directory by its exact storage date.
/// 根据精确存储日期删除一个分段会话目录。
pub(super) fn delete_segmented_session_by_created_date(
    data_root: &Path,
    archived: bool,
    project_tag: &str,
    created_date: &str,
    session_id: &str,
) -> Result<bool, String> {
    let session_dir =
        segmented_session_directory(data_root, archived, project_tag, created_date, session_id);
    if !session_dir.exists() {
        return Ok(false);
    }
    fs::remove_dir_all(&session_dir).map_err(to_store_error)?;
    Ok(true)
}

/// Removes all active and archived segmented session roots below the Vulcan code data root.
/// 删除 Vulcan Code 数据根目录下全部活动与归档分段会话根目录。
pub(super) fn clear_all_segmented_session_roots(data_root: &Path) -> Result<(), String> {
    for root_name in [SEGMENTED_SESSIONS_DIR, SEGMENTED_ARCHIVED_SESSIONS_DIR] {
        let root = data_root.join(root_name);
        if root.exists() {
            fs::remove_dir_all(&root).map_err(to_store_error)?;
        }
        create_dir_all(&root)?;
    }
    Ok(())
}

/// Creates one child task session below a parent segmented session directory.
/// 在父分段会话目录下创建一个子任务会话。
pub(super) fn create_segmented_task_session(
    parent_session_dir: &Path,
    request: SegmentedTaskSessionCreateRequest,
) -> Result<PathBuf, String> {
    // Task creation appends a parent link and writes child metadata, so serialize it with session appends.
    // 子任务创建会追加父会话关联并写入子元数据，因此与会话追加串行化。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load parent metadata so task mirrors stay colocated with the parent directory index.
    // 加载父元数据，确保任务镜像与父目录索引保持一致。
    let mut parent_metadata = read_session_metadata(parent_session_dir)?;
    // Resolve the child task directory under the singular task directory required by the target layout.
    // 按目标布局要求，在单数 task 目录下解析子任务目录。
    let task_dir = parent_session_dir
        .join(SEGMENTED_TASK_DIR)
        .join(sanitize_file_component(&request.task_session_id));
    create_dir_all(&task_dir)?;

    // Build the first child segment before the canonical SubagentTaskMeta event is written.
    // 在写入规范 SubagentTaskMeta 事件前构建首个子任务分段。
    let first_segment = SegmentMetadata::empty(segment_file_name(1), 0);
    // Store child metadata with task-local sequence tracking.
    // 保存带有子任务局部序号跟踪的子元数据。
    let mut task_metadata = SegmentedTaskSessionMetadata {
        schema_version: 1,
        task_session_id: request.task_session_id.clone(),
        parent_session_id: parent_metadata.session_id.clone(),
        parent_project_tag: parent_metadata.project_tag.clone(),
        created_at: request.created_at.clone(),
        updated_at: request.created_at.clone(),
        status: request.status.clone(),
        agent_kind: request.agent_kind.clone(),
        agent_profile: request.agent_profile.clone(),
        spawned_by: request.spawned_by.clone(),
        active_segment: first_segment.file.clone(),
        next_local_seq: 0,
        segment_policy: request.segment_policy,
        segments: vec![first_segment],
    };
    // The first child line is the required canonical SubagentTaskMeta event.
    // 子任务首行是要求的规范 SubagentTaskMeta 事件。
    let meta_payload = SubagentTaskMetadataPayload {
        task_session_id: request.task_session_id.clone(),
        parent_session_id: parent_metadata.session_id.clone(),
        parent_project_tag: parent_metadata.project_tag.clone(),
        agent_kind: request.agent_kind.clone(),
        agent_profile: request.agent_profile.clone(),
        spawned_by: request.spawned_by.clone(),
        created_at: request.created_at.clone(),
    };
    let meta_event = SegmentedSessionEventInput {
        timestamp: request.created_at.clone(),
        kind: SegmentedSessionEventKind::SubagentTaskMeta,
        payload: serde_json::to_value(meta_payload).map_err(to_store_error)?,
        turn_index: None,
    };
    append_task_events_with_metadata(&task_dir, &mut task_metadata, &[meta_event])?;
    write_task_metadata(&task_dir, &task_metadata)?;
    write_state(
        &task_dir,
        &SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: request.task_session_id.clone(),
            updated_at: request.created_at.clone(),
            runtime_state: Value::Null,
            current_context: Value::Null,
            session_boundary_state: Value::Null,
        },
    )?;

    // Mirror lightweight task data into the parent metadata for routing and list views.
    // 将轻量任务数据镜像到父元数据中，供路由与列表视图使用。
    let task_relative_dir = format!(
        "{SEGMENTED_TASK_DIR}/{}",
        sanitize_file_component(&request.task_session_id)
    );
    // Parent JSONL receives a link event so transcript views can navigate to task details.
    // 父 JSONL 接收关联事件，让转录视图能够跳转到子任务详情。
    let parent_link_payload = json!({
        "task_session_id": request.task_session_id,
        "task_dir": task_relative_dir,
        "task_active_segment": format!("{}/{}", task_relative_dir, task_metadata.active_segment),
        "task_metadata_file": format!("{}/{}", task_relative_dir, SEGMENTED_METADATA_FILE),
        "agent_kind": request.agent_kind,
        "agent_profile": request.agent_profile,
        "parent_event_seq": request.parent_start_seq,
        "status": request.status
    });
    let parent_link_event = SegmentedSessionEventInput {
        timestamp: request.created_at.clone(),
        kind: SegmentedSessionEventKind::SubagentTaskStarted,
        payload: parent_link_payload,
        turn_index: None,
    };
    let parent_link_lines = append_events_with_metadata(
        parent_session_dir,
        &mut parent_metadata,
        &[parent_link_event],
    )?;
    let parent_link_seq = parent_link_lines.first().map(|line| line.seq);
    let task_mirror = SubagentTaskMirror {
        session_id: request.task_session_id.clone(),
        dir: task_relative_dir.clone(),
        active_segment: format!("{}/{}", task_relative_dir, task_metadata.active_segment),
        metadata_file: format!("{}/{}", task_relative_dir, SEGMENTED_METADATA_FILE),
        agent_kind: request.agent_kind,
        agent_profile: request.agent_profile,
        parent_start_seq: parent_link_seq.or(request.parent_start_seq),
        parent_end_seq: None,
        status: request.status,
        line_count: task_metadata
            .segments
            .iter()
            .map(|segment| segment.line_count)
            .sum(),
        created_at: request.created_at.clone(),
        updated_at: request.created_at,
    };
    parent_metadata.tasks.push(task_mirror);
    parent_metadata.updated_at = task_metadata.updated_at.clone();
    write_metadata(parent_session_dir, &parent_metadata)?;
    Ok(task_dir)
}

/// Appends events to one child task session.
/// 向一个子任务会话追加事件。
pub(super) fn append_segmented_task_events(
    task_dir: &Path,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Child task JSONL appends share the process-local append lock with parent sessions.
    // 子任务 JSONL 追加与父会话共用进程内追加锁。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load task metadata first so task-local sequence numbers stay authoritative.
    // 先加载子任务元数据，确保子任务局部序号保持权威。
    let mut metadata = read_task_session_metadata(task_dir)?;
    // Append the requested events and then atomically overwrite task metadata.
    // 追加请求事件，然后原子覆盖子任务元数据。
    let lines = append_task_events_with_metadata(task_dir, &mut metadata, events)?;
    write_task_metadata(task_dir, &metadata)?;
    Ok(lines)
}

/// Records a parent-session link update for one child task and refreshes task metadata.
/// 为一个子任务记录父会话关联更新，并刷新任务元数据。
pub(super) fn record_segmented_task_link_update(
    parent_session_dir: &Path,
    update: SegmentedTaskLinkUpdate,
) -> Result<SegmentedSessionEventLine, String> {
    // Parent task-link updates append to the parent JSONL segment and mutate metadata together.
    // 父任务关联更新会同时追加父 JSONL 分段并修改元数据。
    let _guard = segmented_append_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    // Load parent metadata so the task mirror remains the routing source for this update.
    // 加载父元数据，确保任务镜像仍是本次更新的路由来源。
    let mut parent_metadata = read_session_metadata(parent_session_dir)?;
    let task_index = parent_metadata
        .tasks
        .iter()
        .position(|task| task.session_id == update.task_session_id)
        .ok_or_else(|| {
            format!(
                "Task `{}` is not mirrored in parent metadata.",
                update.task_session_id
            )
        })?;
    // Clone stable routing fields before mutating parent metadata through append.
    // 在通过 append 修改父元数据前克隆稳定路由字段。
    let task_dir_relative = parent_metadata.tasks[task_index].dir.clone();
    let task_metadata_file = parent_metadata.tasks[task_index].metadata_file.clone();
    let task_dir = parent_session_dir.join(&task_dir_relative);

    // Update child task metadata first; parent link event then points at the latest task status.
    // 先更新子任务元数据；随后父关联事件指向最新任务状态。
    let mut task_metadata = read_task_session_metadata(&task_dir)?;
    task_metadata.status = update.status.clone();
    task_metadata.updated_at = update.timestamp.clone();
    write_task_metadata(&task_dir, &task_metadata)?;
    let task_active_segment = format!("{}/{}", task_dir_relative, task_metadata.active_segment);

    // Terminal updates get the completed event kind; non-terminal updates remain progress links.
    // 终态更新使用 completed 事件类型；非终态更新保持进度关联事件。
    let event_kind = if update.terminal {
        SegmentedSessionEventKind::SubagentTaskCompleted
    } else {
        SegmentedSessionEventKind::SubagentTaskEventLinked
    };
    let parent_payload = json!({
        "task_session_id": update.task_session_id,
        "task_dir": task_dir_relative,
        "task_active_segment": task_active_segment,
        "task_metadata_file": task_metadata_file,
        "agent_kind": task_metadata.agent_kind.clone(),
        "agent_profile": task_metadata.agent_profile.clone(),
        "status": update.status,
        "terminal": update.terminal,
        "detail": update.payload
    });
    let event_input = SegmentedSessionEventInput {
        timestamp: update.timestamp.clone(),
        kind: event_kind,
        payload: parent_payload,
        turn_index: update.turn_index,
    };
    let appended_lines =
        append_events_with_metadata(parent_session_dir, &mut parent_metadata, &[event_input])?;
    let appended_line = appended_lines
        .into_iter()
        .next()
        .ok_or_else(|| "Task link update did not append any parent event.".to_string())?;

    // Update the parent task mirror after the event sequence is known.
    // 在事件序号确定后更新父任务镜像。
    if let Some(task_mirror) = parent_metadata.tasks.get_mut(task_index) {
        task_mirror.status = task_metadata.status.clone();
        task_mirror.updated_at = task_metadata.updated_at.clone();
        task_mirror.active_segment = task_active_segment.clone();
        task_mirror.line_count = task_metadata
            .segments
            .iter()
            .map(|segment| segment.line_count)
            .sum();
        if update.terminal {
            task_mirror.parent_end_seq = Some(appended_line.seq);
        }
    }
    parent_metadata.updated_at = task_metadata.updated_at.clone();
    write_metadata(parent_session_dir, &parent_metadata)?;
    Ok(appended_line)
}

/// Builds one deterministic message identifier when the in-memory entry has not been assigned one yet.
/// 当内存消息项尚未分配标识时构建一个确定性消息标识。
fn stable_segmented_message_id(index: usize, message: &SessionMessageEntry) -> String {
    let message_id = message.id.trim();
    if message_id.is_empty() {
        format!("message-{index:06}")
    } else {
        message_id.to_string()
    }
}

/// Converts one transcript message into the segmented append event input shape.
/// 将一条转录消息转换为分段追加事件输入形态。
fn segmented_event_input_from_message(
    index: usize,
    message: &SessionMessageEntry,
    turn_index: i64,
) -> Result<SegmentedSessionEventInput, String> {
    let message_id = stable_segmented_message_id(index, message);
    let timestamp = non_empty_timestamp_text(&message.created_at);
    let kind = segmented_event_kind_from_message(message);
    let message_value = serde_json::to_value(message).map_err(to_store_error)?;
    Ok(SegmentedSessionEventInput {
        timestamp,
        kind,
        payload: json!({
            "message_id": message_id,
            "message_index": index,
            "turn_index": turn_index,
            "message": message_value,
            "message_kind": message.kind.label(),
            "record_kind": message.record_kind.as_persisted_key(),
            "visible": message.visible,
        }),
        turn_index: Some(turn_index),
    })
}

/// Backfills JSONL sequence numbers into visible tool-call detail references before append.
/// 追加前将 JSONL 序号回填到可见工具调用的详情引用中。
fn backfill_tool_detail_ref_sequences(
    events: &mut [SegmentedSessionEventInput],
    first_event_seq: u64,
) {
    let mut detail_seq_by_index = BTreeMap::new();
    let mut detail_seq_by_id = BTreeMap::new();
    for (offset, event) in events.iter().enumerate() {
        if event.kind != SegmentedSessionEventKind::DisplayEvent {
            continue;
        }
        if event.payload.get("record_kind").and_then(Value::as_str)
            != Some(SessionMessageRecordKind::ToolDetailOutput.as_persisted_key())
        {
            continue;
        }
        let seq = first_event_seq.saturating_add(offset as u64);
        if let Some(message_index) = event.payload.get("message_index").and_then(Value::as_u64) {
            detail_seq_by_index.insert(message_index, seq);
        }
        if let Some(message_id) = event
            .payload
            .get("message_id")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            detail_seq_by_id.insert(message_id.to_string(), seq);
        }
    }
    if detail_seq_by_index.is_empty() && detail_seq_by_id.is_empty() {
        return;
    }
    for event in events {
        let Some(detail_ref) = event
            .payload
            .get_mut("message")
            .and_then(Value::as_object_mut)
            .and_then(|message| message.get_mut("metadata"))
            .and_then(Value::as_object_mut)
            .and_then(|metadata| metadata.get_mut("result"))
            .and_then(Value::as_object_mut)
            .and_then(|result| result.get_mut("detail_ref"))
            .and_then(Value::as_object_mut)
        else {
            continue;
        };
        let seq = detail_ref
            .get("record_id")
            .and_then(Value::as_str)
            .and_then(|record_id| detail_seq_by_id.get(record_id).copied())
            .or_else(|| {
                detail_ref
                    .get("message_index")
                    .and_then(Value::as_u64)
                    .and_then(|message_index| detail_seq_by_index.get(&message_index).copied())
            });
        if let Some(seq) = seq {
            detail_ref.insert(
                String::from("seq"),
                Value::Number(serde_json::Number::from(seq)),
            );
        }
    }
}

/// Maps one transcript message onto the coarse segmented event kind used by reducers.
/// 将一条转录消息映射到 reducer 使用的粗粒度分段事件类型。
fn segmented_event_kind_from_message(message: &SessionMessageEntry) -> SegmentedSessionEventKind {
    match (message.kind, message.record_kind, message.visible) {
        (SessionMessageKind::User, SessionMessageRecordKind::Transcript, true) => {
            SegmentedSessionEventKind::UserPrompt
        }
        (SessionMessageKind::Assistant, SessionMessageRecordKind::AssistantOutput, true) => {
            SegmentedSessionEventKind::AssistantOutput
        }
        _ if message.visible => SegmentedSessionEventKind::DisplayEvent,
        _ if message.contributes_to_context() => SegmentedSessionEventKind::ResponseItem,
        _ => SegmentedSessionEventKind::DisplayEvent,
    }
}

/// Converts one segmented event line into a transcript message when the line carries a message payload.
/// 当分段事件行携带消息载荷时，将其转换为转录消息。
fn segmented_line_to_session_message(
    line: &SegmentedSessionEventLine,
) -> Result<Option<SessionMessageEntry>, String> {
    let Some(message_value) = line.payload.get("message").cloned() else {
        return Ok(None);
    };
    let mut message =
        serde_json::from_value::<SessionMessageEntry>(message_value).map_err(to_store_error)?;
    if message.id.trim().is_empty() {
        if let Some(message_id) = line
            .payload
            .get("message_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            message.id = message_id.to_string();
        }
    }
    if message.created_at.trim().is_empty() {
        message.created_at = line.timestamp.clone();
    }
    Ok(Some(message))
}

/// Returns whether one segmented event line is a tool-detail row for the requested turn.
/// 返回单条分段事件行是否是指定轮次的工具详情行。
fn segmented_line_matches_tool_turn(
    line: &SegmentedSessionEventLine,
    turn_index: i64,
) -> Result<bool, String> {
    if !line
        .payload
        .get("turn_index")
        .and_then(Value::as_i64)
        .is_some_and(|value| value == turn_index)
    {
        return Ok(false);
    }
    let Some(message) = segmented_line_to_session_message(line)? else {
        return Ok(false);
    };
    Ok(matches!(
        message.record_kind,
        SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
    ))
}

/// Extracts the display turn index carried by one segmented event line.
/// 提取单条分段事件行携带的展示轮次索引。
fn segmented_line_turn_index(line: &SegmentedSessionEventLine) -> Option<i64> {
    line.payload.get("message")?;
    line.payload.get("turn_index").and_then(Value::as_i64)
}

/// Returns suffix lines that remain valid after the newest rollback marker.
/// 返回最新回滚标记之后仍然有效的后缀事件行。
fn suffix_after_latest_rollback(
    scan: &SegmentedCompactionScanResult,
) -> Vec<SegmentedSessionEventLine> {
    let rollback_seq = scan.rollback_markers.iter().map(|marker| marker.seq).max();
    scan.suffix
        .iter()
        .filter(|line| rollback_seq.is_none_or(|seq| line.seq > seq))
        .cloned()
        .collect()
}

/// Converts a compaction replacement-history value into transcript messages.
/// 将压缩 replacement-history 值转换为转录消息。
fn replacement_history_to_session_messages(
    value: &Value,
) -> Result<Vec<SessionMessageEntry>, String> {
    if let Ok(state) = serde_json::from_value::<SessionConversationState>(value.clone()) {
        return Ok(conversation_state_to_session_messages(&state));
    }
    if let Some(messages_value) = value.get("messages") {
        if let Ok(messages) =
            serde_json::from_value::<Vec<SessionMessageEntry>>(messages_value.clone())
        {
            return Ok(messages);
        }
        if let Ok(state) = serde_json::from_value::<SessionConversationState>(value.clone()) {
            return Ok(conversation_state_to_session_messages(&state));
        }
    }
    if let Ok(messages) = serde_json::from_value::<Vec<SessionMessageEntry>>(value.clone()) {
        return Ok(messages);
    }
    let Some(items) = value.as_array() else {
        return Ok(Vec::new());
    };
    Ok(items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| replacement_history_item_to_message(index, item))
        .collect())
}

/// Converts one loose replacement-history item into a transcript message.
/// 将单个宽松 replacement-history 条目转换为转录消息。
fn replacement_history_item_to_message(index: usize, item: &Value) -> Option<SessionMessageEntry> {
    let object = item.as_object()?;
    let role = object
        .get("role")
        .or_else(|| object.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or("assistant");
    let content = object
        .get("content")
        .or_else(|| object.get("body"))
        .or_else(|| object.get("text"))
        .and_then(Value::as_str)?
        .to_string();
    let kind = if role.eq_ignore_ascii_case("user") {
        SessionMessageKind::User
    } else {
        SessionMessageKind::Assistant
    };
    let mut message = SessionMessageEntry::new(
        kind,
        if matches!(kind, SessionMessageKind::User) {
            "用户指令"
        } else {
            "AI"
        },
        content,
    );
    message.id = object
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| format!("replacement-history-{index:06}"));
    message.record_kind = if matches!(kind, SessionMessageKind::User) {
        SessionMessageRecordKind::Transcript
    } else {
        SessionMessageRecordKind::AssistantOutput
    };
    message.created_at = object
        .get("created_at")
        .or_else(|| object.get("timestamp"))
        .and_then(Value::as_str)
        .unwrap_or("0")
        .to_string();
    Some(message)
}

/// Assigns stable turn indexes to one ordered segmented transcript slice.
/// 为一段有序分段转录消息分配稳定轮次索引。
fn assign_segmented_message_turn_indexes(messages: &[SessionMessageEntry]) -> Vec<i64> {
    let mut turn_indexes = Vec::with_capacity(messages.len());
    let mut current_turn_index = 0_i64;
    let mut has_seen_user_turn = false;

    for message in messages {
        if segmented_message_starts_new_turn(message) {
            if has_seen_user_turn {
                current_turn_index = current_turn_index.saturating_add(1);
            } else {
                has_seen_user_turn = true;
            }
        }
        turn_indexes.push(current_turn_index);
    }

    turn_indexes
}

/// Assigns turn indexes only to newly appended transcript tail messages.
/// 仅为新追加的转录尾部消息分配轮次索引。
fn assign_segmented_tail_message_turn_indexes(
    metadata: &SegmentedSessionMetadata,
    tail_messages: &[SessionMessageEntry],
) -> Vec<i64> {
    let mut turn_indexes = Vec::with_capacity(tail_messages.len());
    let mut current_turn_index = metadata
        .segments
        .iter()
        .rev()
        .find_map(|segment| segment.last_turn_index)
        .unwrap_or(0);
    let mut has_seen_user_turn = metadata.message_count > 0;

    for message in tail_messages {
        if segmented_message_starts_new_turn(message) {
            if has_seen_user_turn {
                current_turn_index = current_turn_index.saturating_add(1);
            } else {
                has_seen_user_turn = true;
            }
        }
        turn_indexes.push(current_turn_index);
    }

    turn_indexes
}

/// Returns whether one segmented transcript message opens a user-driven turn.
/// 返回一条分段转录消息是否开启用户驱动轮次。
fn segmented_message_starts_new_turn(message: &SessionMessageEntry) -> bool {
    matches!(message.kind, SessionMessageKind::User)
        && message.visible
        && message.contributes_to_context()
}

/// Returns a non-empty timestamp for append-only events.
/// 为追加式事件返回非空时间戳。
fn non_empty_timestamp_text(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Reads all JSONL event lines from a segment path.
/// 从分段路径读取全部 JSONL 事件行。
pub(super) fn read_segment_event_lines(
    segment_path: &Path,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    read_segment_event_lines_tolerant(segment_path)
}

/// Reads segmented event lines while isolating unknown historical event types from the active workspace.
/// 读取分段事件行时隔离未知历史事件类型，避免单条旧事件拖垮当前工作区。
fn read_segment_event_lines_tolerant(
    segment_path: &Path,
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    if !segment_path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(segment_path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut lines = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index.saturating_add(1);
        let line = line.map_err(|error| {
            format!(
                "读取 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                segment_path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        let value = serde_json::from_str::<Value>(&line).map_err(|error| {
            format!(
                "解析 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                segment_path.display()
            )
        })?;
        if !segmented_event_value_has_known_kind(&value) {
            let event_type = value
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("<missing>");
            eprintln!(
                "[workspace-store] skipped unknown segmented session event type `{event_type}` at {}:{line_number}",
                segment_path.display()
            );
            continue;
        }
        lines.push(
            serde_json::from_value::<SegmentedSessionEventLine>(value).map_err(|error| {
                format!(
                    "解析 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                    segment_path.display()
                )
            })?,
        );
    }
    Ok(lines)
}

/// Returns whether one raw segment JSON value declares an event type accepted by the current schema.
/// 返回原始分段 JSON 值是否声明了当前 schema 接受的事件类型。
fn segmented_event_value_has_known_kind(value: &Value) -> bool {
    value
        .get("type")
        .and_then(Value::as_str)
        .and_then(|kind| {
            serde_json::from_value::<SegmentedSessionEventKind>(Value::String(kind.to_string()))
                .ok()
        })
        .is_some()
}

/// Reverse-scans ordered segment metadata for the latest compaction replacement-history anchor.
/// 按倒序扫描有序分段元数据，寻找最新的 replacement-history 压缩锚点。
pub(super) fn find_latest_compaction_anchor(
    session_dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<SegmentedCompactionScanResult, String> {
    // Keep newer segment lines while scanning from newest to oldest.
    // 从新到旧扫描时暂存较新分段的事件行。
    let mut newer_segment_lines: Vec<Vec<SegmentedSessionEventLine>> = Vec::new();
    // Count segment reads so tests and diagnostics can verify lazy behavior later.
    // 记录分段读取次数，方便测试与诊断后续验证懒加载行为。
    let mut scanned_segment_count = 0;
    // Capture the newest turn context encountered before the scan can stop at a compaction anchor.
    // 捕获扫描在压缩锚点停止前遇到的最新轮次上下文。
    let mut latest_turn_context: Option<SegmentedTurnContextAnchor> = None;
    // Capture rollback markers in reverse discovery order for reducer follow-up.
    // 以倒序发现顺序捕获回滚标记，供后续 reducer 使用。
    let mut rollback_markers: Vec<SegmentedRollbackMarker> = Vec::new();

    for segment in segments.iter().rev() {
        scanned_segment_count += 1;
        let segment_path = session_dir.join(&segment.file);
        let lines = read_segment_event_lines(&segment_path)?;
        for line_index in (0..lines.len()).rev() {
            let line = &lines[line_index];
            if line.kind == SegmentedSessionEventKind::TurnContext && latest_turn_context.is_none()
            {
                latest_turn_context = Some(SegmentedTurnContextAnchor {
                    seq: line.seq,
                    segment_file: segment.file.clone(),
                    line_index,
                    payload: line.payload.clone(),
                });
            }
            if line.kind == SegmentedSessionEventKind::ThreadRolledBack {
                rollback_markers.push(SegmentedRollbackMarker {
                    seq: line.seq,
                    segment_file: segment.file.clone(),
                    line_index,
                    payload: line.payload.clone(),
                });
            }
            if line.kind != SegmentedSessionEventKind::Compacted {
                continue;
            }
            let Some(replacement_history) = line.payload.get("replacement_history").cloned() else {
                continue;
            };
            // The suffix begins after the compaction line in the current segment.
            // suffix 从当前分段中压缩行之后开始。
            let mut suffix = lines[line_index + 1..].to_vec();
            // Newer segments were collected newest-first, so replay order needs the reverse.
            // 较新分段按最新优先收集，因此重放顺序需要反转。
            for newer_lines in newer_segment_lines.iter().rev() {
                suffix.extend(newer_lines.clone());
            }
            return Ok(SegmentedCompactionScanResult {
                anchor: Some(SegmentedCompactionAnchor {
                    compaction_seq: line.seq,
                    segment_file: segment.file.clone(),
                    line_index,
                    replacement_history,
                }),
                suffix,
                latest_turn_context,
                rollback_markers,
                scanned_segment_count,
            });
        }
        newer_segment_lines.push(lines);
    }

    // Without an anchor, return all scanned events in forward segment order.
    // 没有锚点时，按正向分段顺序返回所有已扫描事件。
    let mut suffix = Vec::new();
    for lines in newer_segment_lines.iter().rev() {
        suffix.extend(lines.clone());
    }
    Ok(SegmentedCompactionScanResult {
        anchor: None,
        suffix,
        latest_turn_context,
        rollback_markers,
        scanned_segment_count,
    })
}

/// Appends events using a main session metadata document already loaded by the caller.
/// 使用调用方已加载的主会话元数据文档追加事件。
fn append_events_with_metadata(
    session_dir: &Path,
    metadata: &mut SegmentedSessionMetadata,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Keep appended lines for tests and projection callers.
    // 为测试与投影调用方保留已追加行。
    let mut appended_lines = Vec::new();
    for event in events {
        // Allocate the next global sequence only when this event is about to be written.
        // 仅在事件即将写入时分配下一个全局序号。
        let line = SegmentedSessionEventLine {
            seq: metadata.next_global_seq,
            timestamp: event.timestamp.clone(),
            kind: event.kind,
            payload: event.payload.clone(),
        };
        append_line_to_active_segment(
            session_dir,
            &metadata.segment_policy,
            &mut metadata.active_segment,
            &mut metadata.segments,
            &line,
            event.turn_index,
        )?;
        metadata.next_global_seq += 1;
        metadata.updated_at = event.timestamp.clone();
        if event.kind == SegmentedSessionEventKind::ThreadRolledBack {
            metadata.latest_rollback_seq = Some(line.seq);
        }
        appended_lines.push(line);
    }
    Ok(appended_lines)
}

/// Builds the durable payload for one session boundary event with store-assigned sequence.
/// 使用存储层分配的序号构建单个 会话边界事件的持久化 payload。
fn session_boundary_payload(event: &SessionBoundaryEvent, assigned_sequence: u64) -> Value {
    json!({
        "event_id": event.event_id,
        "thread_id": event.thread_id,
        "turn_id": event.turn_id,
        "item_id": event.item_id,
        "kind": event.kind,
        "scope": event.scope,
        "visibility": event.visibility,
        "sequence": assigned_sequence,
        "payload": event.payload,
    })
}

/// Appends events using a task session metadata document already loaded by the caller.
/// 使用调用方已加载的子任务会话元数据文档追加事件。
fn append_task_events_with_metadata(
    task_dir: &Path,
    metadata: &mut SegmentedTaskSessionMetadata,
    events: &[SegmentedSessionEventInput],
) -> Result<Vec<SegmentedSessionEventLine>, String> {
    // Keep appended lines for tests and projection callers.
    // 为测试与投影调用方保留已追加行。
    let mut appended_lines = Vec::new();
    for event in events {
        // Allocate task-local sequence numbers independently from the parent session.
        // 独立于父会话分配子任务局部序号。
        let line = SegmentedSessionEventLine {
            seq: metadata.next_local_seq,
            timestamp: event.timestamp.clone(),
            kind: event.kind,
            payload: event.payload.clone(),
        };
        append_line_to_active_segment(
            task_dir,
            &metadata.segment_policy,
            &mut metadata.active_segment,
            &mut metadata.segments,
            &line,
            event.turn_index,
        )?;
        metadata.next_local_seq += 1;
        metadata.updated_at = event.timestamp.clone();
        appended_lines.push(line);
    }
    Ok(appended_lines)
}

/// Appends one line to the active segment, rolling over before the write when needed.
/// 向活动分段追加一行，并在需要时于写入前滚动到新分段。
fn append_line_to_active_segment(
    session_dir: &Path,
    segment_policy: &SegmentPolicy,
    active_segment: &mut String,
    segments: &mut Vec<SegmentMetadata>,
    line: &SegmentedSessionEventLine,
    turn_index: Option<i64>,
) -> Result<(), String> {
    // Serialize before rollover so the byte budget includes the final newline.
    // 先序列化再判断滚动，使字节预算包含最终换行。
    let line_text = serde_json::to_string(line).map_err(to_store_error)?;
    // Account for the newline byte appended by JSONL.
    // 计入 JSONL 追加的换行字节。
    let line_byte_len = line_text.as_bytes().len() as u64 + 1;
    if active_segment_needs_rollover(segments, segment_policy, line_byte_len) {
        // Create the next segment with a start sequence matching the line about to be written.
        // 创建下一个分段，其起始序号匹配即将写入的事件行。
        let next_file = segment_file_name(segments.len() + 1);
        segments.push(SegmentMetadata::empty(next_file.clone(), line.seq));
        *active_segment = next_file;
    }
    // The active segment must exist after initialization or rollover.
    // 初始化或滚动后活动分段必须存在。
    let Some(segment) = segments.last_mut() else {
        return Err("Segment metadata is empty.".to_string());
    };
    let segment_path = session_dir.join(&segment.file);
    append_json_line(&segment_path, line)?;
    update_segment_metadata(segment, line, line_byte_len, turn_index);
    Ok(())
}

/// Returns whether the active segment should roll over before appending a new line.
/// 返回追加新行前活动分段是否应该滚动。
fn active_segment_needs_rollover(
    segments: &[SegmentMetadata],
    segment_policy: &SegmentPolicy,
    next_line_byte_len: u64,
) -> bool {
    // An empty metadata list is repaired by the caller through initialization, so no rollover is possible.
    // 空元数据列表由调用方通过初始化修复，因此无法滚动。
    let Some(active_segment) = segments.last() else {
        return false;
    };
    if active_segment.line_count == 0 {
        return false;
    }
    active_segment.line_count >= segment_policy.max_lines
        || active_segment.byte_len + next_line_byte_len > segment_policy.max_bytes
}

/// Updates segment metadata after one line has been appended successfully.
/// 在一行成功追加后更新分段元数据。
fn update_segment_metadata(
    segment: &mut SegmentMetadata,
    line: &SegmentedSessionEventLine,
    line_byte_len: u64,
    turn_index: Option<i64>,
) {
    if segment.line_count == 0 {
        segment.first_timestamp = Some(line.timestamp.clone());
        segment.first_turn_index = turn_index;
    }
    segment.line_count += 1;
    segment.byte_len += line_byte_len;
    segment.last_timestamp = Some(line.timestamp.clone());
    if let Some(turn_index) = turn_index {
        segment.last_turn_index = Some(turn_index);
    }
    if line.kind == SegmentedSessionEventKind::Compacted {
        segment.last_compaction_seq = Some(line.seq);
    }
}

/// Reads one main segmented session metadata document.
/// 读取一份主分段会话元数据文档。
pub(super) fn read_session_metadata(
    session_dir: &Path,
) -> Result<SegmentedSessionMetadata, String> {
    read_required_json::<SegmentedSessionMetadata>(&metadata_path(session_dir))
}

/// Reads one child task metadata document.
/// 读取一份子任务元数据文档。
fn read_task_session_metadata(task_dir: &Path) -> Result<SegmentedTaskSessionMetadata, String> {
    read_required_json::<SegmentedTaskSessionMetadata>(&metadata_path(task_dir))
}

/// Lists one active or archived segmented-session root from text metadata.
/// 从文本元数据列出一个活动或归档分段会话根。
fn list_segmented_session_catalog_for_root(
    data_root: &Path,
    archived: bool,
) -> Result<Vec<SegmentedSessionCatalogEntry>, String> {
    let root_name = if archived {
        SEGMENTED_ARCHIVED_SESSIONS_DIR
    } else {
        SEGMENTED_SESSIONS_DIR
    };
    let root = data_root.join(root_name);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut entries = Vec::new();
    for project_entry in fs::read_dir(&root).map_err(to_store_error)? {
        let project_entry = project_entry.map_err(to_store_error)?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        for session_entry in fs::read_dir(&project_path).map_err(to_store_error)? {
            let session_entry = session_entry.map_err(to_store_error)?;
            let session_dir = session_entry.path();
            if !session_dir.is_dir() {
                continue;
            }
            let metadata_file = metadata_path(&session_dir);
            if !metadata_file.exists() {
                continue;
            }
            let metadata = read_session_metadata(&session_dir)?;
            entries.push(SegmentedSessionCatalogEntry {
                session_dir,
                archived,
                metadata,
            });
        }
    }
    Ok(entries)
}

/// Writes a main session metadata document atomically.
/// 原子写入主会话元数据文档。
fn write_metadata(session_dir: &Path, metadata: &SegmentedSessionMetadata) -> Result<(), String> {
    write_json_file_atomic(&metadata_path(session_dir), metadata)
}

/// Writes a task session metadata document atomically.
/// 原子写入子任务会话元数据文档。
fn write_task_metadata(
    task_dir: &Path,
    metadata: &SegmentedTaskSessionMetadata,
) -> Result<(), String> {
    write_json_file_atomic(&metadata_path(task_dir), metadata)
}

/// Returns a metadata path for a session-like directory.
/// 返回会话类目录的元数据路径。
fn metadata_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_METADATA_FILE)
}

/// Returns a state path for a session-like directory.
/// 返回会话类目录的状态路径。
fn state_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SEGMENTED_STATE_FILE)
}

/// Returns the shared process-local mutex used for state document read/modify/write operations.
/// 返回状态文档读改写操作共用的进程内互斥锁。
fn segmented_state_file_io_lock() -> &'static Mutex<()> {
    SEGMENTED_STATE_FILE_IO_LOCK.get_or_init(|| Mutex::new(()))
}

/// Returns the shared process-local mutex used for segmented JSONL append operations.
/// 返回分段 JSONL 追加操作共用的进程内互斥锁。
fn segmented_append_file_io_lock() -> &'static Mutex<()> {
    SEGMENTED_APPEND_FILE_IO_LOCK.get_or_init(|| Mutex::new(()))
}

/// Builds a stable segment file name from its one-based index.
/// 根据从一开始的索引构建稳定分段文件名。
fn segment_file_name(index: usize) -> String {
    format!("s{index:03}.jsonl")
}

/// Creates a directory and all missing parents.
/// 创建目录及所有缺失的父目录。
fn create_dir_all(path: &Path) -> Result<(), String> {
    fs::create_dir_all(path).map_err(to_store_error)
}

/// Appends one JSON value as a JSONL line.
/// 将一个 JSON 值作为 JSONL 行追加写入。
fn append_json_line<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    // Encode one complete JSONL row before touching the file handle.
    // 在接触文件句柄前先编码完整 JSONL 行。
    let mut encoded_line = serde_json::to_string(value).map_err(to_store_error)?;
    encoded_line.push('\n');
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(to_store_error)?;
    file.write_all(encoded_line.as_bytes())
        .map_err(to_store_error)?;
    file.flush().map_err(to_store_error)
}

/// Reads a required JSON document from disk.
/// 从磁盘读取一个必需的 JSON 文档。
fn read_required_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let file = File::open(path)
        .map_err(|error| format!("打开 JSON 文件失败 `{}`：{error}", path.display()))?;
    serde_json::from_reader(file)
        .map_err(|error| format!("解析 JSON 文件失败 `{}`：{error}", path.display()))
}

/// Writes one JSON document through a temporary file and rename.
/// 通过临时文件与重命名写入一个 JSON 文档。
fn write_json_file_atomic<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    // The temporary file lives next to the target so rename stays on the same filesystem.
    // 临时文件放在目标旁边，确保重命名位于同一文件系统。
    let temp_path = unique_json_temp_path(path);
    let mut file = File::create(&temp_path).map_err(to_store_error)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(to_store_error)?;
    file.write_all(b"\n").map_err(to_store_error)?;
    file.flush().map_err(to_store_error)?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        to_store_error(error)
    })
}

/// Writes JSONL values through a temporary file and rename.
/// 通过临时文件与重命名写入 JSONL 值集合。
fn write_json_lines_file_atomic<T>(path: &Path, values: &[T]) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let temp_path = unique_json_temp_path(path);
    let mut file = File::create(&temp_path).map_err(to_store_error)?;
    for value in values {
        serde_json::to_writer(&mut file, value).map_err(to_store_error)?;
        file.write_all(b"\n").map_err(to_store_error)?;
    }
    file.flush().map_err(to_store_error)?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        to_store_error(error)
    })
}

/// Appends JSONL values to an existing file, creating its parent directory when needed.
/// 将 JSONL 值追加到已有文件，并在需要时创建父目录。
fn append_json_lines_file<T>(path: &Path, values: &[T]) -> Result<(), String>
where
    T: Serialize,
{
    if values.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    // Build the full batch first so each append call emits one contiguous byte buffer.
    // 先构建完整批次，使每次追加调用输出一个连续字节缓冲。
    let mut encoded_batch = String::new();
    for value in values {
        encoded_batch.push_str(&serde_json::to_string(value).map_err(to_store_error)?);
        encoded_batch.push('\n');
    }
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .map_err(to_store_error)?;
    file.write_all(encoded_batch.as_bytes())
        .map_err(to_store_error)?;
    file.flush().map_err(to_store_error)
}

/// Builds a collision-free temporary JSON path next to the target file.
/// 在目标文件旁构建不会互相冲突的 JSON 临时路径。
fn unique_json_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("document.json");
    let temp_name = format!(".{file_name}.{}.tmp", uuid::Uuid::new_v4());
    path.with_file_name(temp_name)
}

/// Moves one segmented session directory without overwriting an existing destination.
/// 移动一个分段会话目录，且不覆盖已存在的目标目录。
fn move_segmented_session_directory(source: &Path, destination: &Path) -> Result<(), String> {
    if destination.exists() {
        return Err(format!(
            "Segmented session destination already exists: {}",
            destination.display()
        ));
    }
    if let Some(parent) = destination.parent() {
        create_dir_all(parent)?;
    }
    fs::rename(source, destination).map_err(to_store_error)
}

/// Reads JSONL values from a file, returning an empty list when the file is absent.
/// 从文件读取 JSONL 值，文件不存在时返回空列表。
fn read_json_lines<T>(path: &Path) -> Result<Vec<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line_number = line_index.saturating_add(1);
        let line = line.map_err(|error| {
            format!(
                "读取 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                path.display()
            )
        })?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line).map_err(|error| {
            format!(
                "解析 JSONL 文件 `{}` 第 {line_number} 行失败：{error}",
                path.display()
            )
        })?);
    }
    Ok(values)
}

/// Sanitizes one path component while preserving enough human-readable text for debugging.
/// 清理单个路径片段，同时保留足够的人类可读文本便于调试。
fn sanitize_file_component(value: &str) -> String {
    // Replace path separators and control-like punctuation with underscores.
    // 将路径分隔符与控制类标点替换为下划线。
    let sanitized: String = value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = sanitized.trim_matches('_');
    if trimmed.is_empty() {
        "untagged".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Builds a compaction backup directory name with enough runtime scope to audit source coverage.
/// 构建包含足够运行时范围信息的压缩备份目录名，便于审计源文件覆盖情况。
fn runtime_compaction_backup_dir_name(
    session_dir: &Path,
    compaction_id: &str,
    timestamp: &str,
) -> String {
    let (segment_id, event_range) = runtime_compaction_backup_scope(session_dir)
        .unwrap_or_else(|| ("segment-unknown".to_string(), "seq-unknown".to_string()));
    format!(
        "compact-{}-{}-{}-{}",
        sanitize_file_component(timestamp),
        sanitize_file_component(compaction_id),
        sanitize_file_component(&segment_id),
        sanitize_file_component(&event_range)
    )
}

/// Reads the active runtime segment and sequence range used in one compaction backup name.
/// 读取用于单个压缩备份目录名的活动运行时分段与序号范围。
fn runtime_compaction_backup_scope(session_dir: &Path) -> Option<(String, String)> {
    let metadata = read_runtime_metadata(session_dir).ok()?;
    let active_segment = metadata
        .segments
        .iter()
        .find(|segment| segment.file == metadata.active_segment)
        .or_else(|| metadata.segments.last())?;
    let raw_segment_file = metadata
        .active_segment
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(metadata.active_segment.as_str());
    let segment_stem = raw_segment_file
        .strip_suffix(".jsonl")
        .unwrap_or(raw_segment_file);
    let end_seq = active_segment
        .end_seq
        .unwrap_or_else(|| metadata.next_seq.saturating_sub(1));
    let event_range = if active_segment.line_count == 0 || end_seq < active_segment.start_seq {
        "seq-empty".to_string()
    } else {
        format!("seq{}-{}", active_segment.start_seq, end_seq)
    };
    Some((format!("segment-{segment_stem}"), event_range))
}

/// Builds a stable fingerprint for one compaction audit boundary and copied runtime files.
/// 为单个压缩审计边界及已复制 runtime 文件构建稳定指纹。
fn runtime_compaction_fingerprint(
    compaction_id: &str,
    timestamp: &str,
    copied_files: &[String],
) -> String {
    let mut hasher = DefaultHasher::new();
    compaction_id.hash(&mut hasher);
    timestamp.hash(&mut hasher);
    copied_files.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Copies one runtime file into a compaction backup directory when the source exists.
/// 当源文件存在时，将单个运行时文件复制到压缩备份目录。
fn copy_runtime_file_into_backup(
    session_dir: &Path,
    source: &Path,
    backup_dir: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), String> {
    if !source.exists() {
        return Ok(());
    }
    let file_name = source.file_name().ok_or_else(|| {
        format!(
            "Runtime backup source has no file name: {}",
            source.display()
        )
    })?;
    let destination = backup_dir.join(file_name);
    fs::copy(source, &destination).map_err(to_store_error)?;
    copied_files.push(relative_session_path(session_dir, &destination));
    Ok(())
}

/// Recursively copies the runtime segment directory into a compaction backup.
/// 将运行时分段目录递归复制到压缩备份中。
fn copy_runtime_dir_into_backup(
    session_dir: &Path,
    source_dir: &Path,
    destination_dir: &Path,
    copied_files: &mut Vec<String>,
) -> Result<(), String> {
    create_dir_all(destination_dir)?;
    for entry in fs::read_dir(source_dir).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let source_path = entry.path();
        let destination_path = destination_dir.join(entry.file_name());
        let file_type = entry.file_type().map_err(to_store_error)?;
        if file_type.is_dir() {
            copy_runtime_dir_into_backup(
                session_dir,
                &source_path,
                &destination_path,
                copied_files,
            )?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(to_store_error)?;
            copied_files.push(relative_session_path(session_dir, &destination_path));
        }
    }
    Ok(())
}

/// Removes old runtime-log backups while keeping the newest requested count.
/// 删除旧的运行时日志备份，并保留指定数量的最新备份。
fn prune_runtime_log_backups(session_dir: &Path, keep: usize) -> Result<(), String> {
    let backup_root = runtime_backup_dir_path(session_dir);
    if !backup_root.exists() {
        return Ok(());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(&backup_root).map_err(to_store_error)? {
        let entry = entry.map_err(to_store_error)?;
        let file_type = entry.file_type().map_err(to_store_error)?;
        if !file_type.is_dir() {
            continue;
        }
        let modified = entry
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        backups.push((modified, entry.path()));
    }
    backups.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
    let prune_count = backups.len().saturating_sub(keep);
    for (_, path) in backups.into_iter().take(prune_count) {
        fs::remove_dir_all(path).map_err(to_store_error)?;
    }
    Ok(())
}

/// Formats a path relative to the session directory for compact audit messages.
/// 将路径格式化为相对会话目录的紧凑审计文本。
fn relative_session_path(session_dir: &Path, path: &Path) -> String {
    path.strip_prefix(session_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Converts any displayable error into a storage error string.
/// 将任意可展示错误转换为存储错误字符串。
fn to_store_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionRuntimeStateEntry;
    use crate::provider_config::AiToolKind;
    use uuid::Uuid;

    /// Builds a unique temporary directory for one segmented-store test.
    /// 为单个分段存储测试构建唯一临时目录。
    fn test_root(test_name: &str) -> PathBuf {
        // Use a UUID so parallel test runs do not collide in the system temp directory.
        // 使用 UUID，避免并行测试在系统临时目录中冲突。
        let root = std::env::temp_dir()
            .join("vulcan-segmented-session-store-tests")
            .join(test_name)
            .join(Uuid::new_v4().to_string());
        fs::create_dir_all(&root).expect("create temp test root");
        root
    }

    /// Builds a minimal main session creation request.
    /// 构建最小主会话创建请求。
    fn test_session_request(policy: SegmentPolicy) -> SegmentedSessionCreateRequest {
        SegmentedSessionCreateRequest {
            session_id: "session-alpha".to_string(),
            project_tag: "Project Alpha".to_string(),
            created_date: "2026-06-06".to_string(),
            created_at: "2026-06-06T10:00:00Z".to_string(),
            project_id: Some("project-alpha".to_string()),
            title: Some("Alpha".to_string()),
            title_source: Some("user".to_string()),
            segment_policy: policy,
        }
    }

    /// Builds a simple append input.
    /// 构建简单追加输入。
    fn test_event(timestamp: &str, kind: SegmentedSessionEventKind) -> SegmentedSessionEventInput {
        SegmentedSessionEventInput {
            timestamp: timestamp.to_string(),
            kind,
            payload: json!({ "body": timestamp }),
            turn_index: Some(1),
        }
    }

    /// Builds a transcript message with stable runtime metadata for append-tail tests.
    /// 为尾部追加测试构建带稳定运行时元数据的转录消息。
    fn test_message(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
    ) -> SessionMessageEntry {
        let mut message = SessionMessageEntry::new(kind, "Title", body)
            .with_runtime_metadata(id, format!("2026-06-06T10:00:0{}Z", id.len()));
        message.record_kind = record_kind;
        message
    }

    /// Verifies the directory layout and first SessionMeta line for a new main session.
    /// 验证新主会话的目录布局与首条 SessionMeta 行。
    #[test]
    fn create_session_writes_metadata_state_and_first_segment() {
        let root = test_root("create_session_writes_metadata_state_and_first_segment");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create segmented session");

        assert!(session_dir.join(SEGMENTED_METADATA_FILE).exists());
        assert!(session_dir.join(SEGMENTED_STATE_FILE).exists());
        assert!(session_dir.join(SEGMENTED_TASK_DIR).is_dir());
        assert!(session_dir.join(SEGMENTED_BLOB_DIR).is_dir());

        let metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
        assert_eq!(metadata.active_segment, "s001.jsonl");
        assert_eq!(metadata.next_global_seq, 1);
        assert_eq!(metadata.segments[0].line_count, 1);

        let lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read first segment lines");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].seq, 0);
        assert_eq!(lines[0].kind, SegmentedSessionEventKind::SessionMeta);
    }

    /// Verifies append writes only new lines and updates sequence metadata.
    /// 验证追加只写入新行并更新序号元数据。
    #[test]
    fn append_session_events_advances_sequence_without_rewriting_first_line() {
        let root =
            test_root("append_session_events_advances_sequence_without_rewriting_first_line");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create segmented session");
        let before_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read lines before append");

        append_segmented_session_events(
            &session_dir,
            &[
                test_event(
                    "2026-06-06T10:01:00Z",
                    SegmentedSessionEventKind::UserPrompt,
                ),
                test_event(
                    "2026-06-06T10:02:00Z",
                    SegmentedSessionEventKind::AssistantOutput,
                ),
            ],
        )
        .expect("append events");

        let after_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read lines after append");
        assert_eq!(after_lines[0], before_lines[0]);
        assert_eq!(after_lines.len(), 3);
        assert_eq!(after_lines[1].seq, 1);
        assert_eq!(after_lines[2].seq, 2);

        let metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
        assert_eq!(metadata.next_global_seq, 3);
        assert_eq!(metadata.segments[0].line_count, 3);
    }

    /// Unknown historical segment event types are skipped instead of failing the whole session load.
    /// 未知历史分段事件类型会被跳过，而不是导致整个会话加载失败。
    #[test]
    fn read_segment_event_lines_skips_unknown_historical_event_types() {
        let root = test_root("read_segment_event_lines_skips_unknown_historical_event_types");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let segment_path = session_dir.join("s001.jsonl");
        let mut file = OpenOptions::new()
            .append(true)
            .open(&segment_path)
            .expect("open segment");
        writeln!(
            file,
            "{}",
            json!({
                "seq": 99,
                "timestamp": "2026",
                "type": "codex_boundary",
                "payload": {"kind": "turn_started"}
            })
        )
        .expect("append unknown line");

        let lines = read_segment_event_lines(&segment_path).expect("read tolerant lines");

        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].kind, SegmentedSessionEventKind::SessionMeta);
    }

    /// Verifies line-count based rollover creates ordered segments.
    /// 验证基于行数的滚动会创建有序分段。
    #[test]
    fn append_session_events_rolls_over_by_line_count() {
        let root = test_root("append_session_events_rolls_over_by_line_count");
        let policy = SegmentPolicy {
            max_lines: 2,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        };
        let session_dir =
            create_segmented_session(&root, test_session_request(policy)).expect("create session");

        append_segmented_session_events(
            &session_dir,
            &[
                test_event(
                    "2026-06-06T10:01:00Z",
                    SegmentedSessionEventKind::UserPrompt,
                ),
                test_event(
                    "2026-06-06T10:02:00Z",
                    SegmentedSessionEventKind::AssistantOutput,
                ),
            ],
        )
        .expect("append rollover events");

        let metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
        assert_eq!(metadata.active_segment, "s002.jsonl");
        assert_eq!(metadata.segments.len(), 2);
        assert_eq!(metadata.segments[0].line_count, 2);
        assert_eq!(metadata.segments[1].line_count, 1);
        assert_eq!(metadata.segments[1].start_seq, 2);
    }

    /// Verifies transcript tail appends are idempotent and preserve transcript-facing event kinds.
    /// 验证转录尾部追加具备幂等性，并保留面向转录的事件类型。
    #[test]
    fn append_message_tail_is_idempotent_and_maps_transcript_kinds() {
        let root = test_root("append_message_tail_is_idempotent_and_maps_transcript_kinds");
        let session_dir = create_segmented_session(
            &root,
            test_session_request(SegmentPolicy {
                max_lines: 4,
                max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
            }),
        )
        .expect("create session");
        let first_messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];

        let first_append = append_segmented_session_message_tail(&session_dir, &first_messages)
            .expect("append first tail");
        assert_eq!(first_append.len(), 2);
        assert_eq!(first_append[0].kind, SegmentedSessionEventKind::UserPrompt);
        assert_eq!(
            first_append[1].kind,
            SegmentedSessionEventKind::AssistantOutput
        );

        let second_append = append_segmented_session_message_tail(&session_dir, &first_messages)
            .expect("repeat append");
        assert!(second_append.is_empty());

        let mut second_messages = first_messages.clone();
        second_messages.push(test_message(
            "m-tool-1",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            "tool output",
        ));
        let tail_append = append_segmented_session_message_tail(&session_dir, &second_messages)
            .expect("append second tail");
        assert_eq!(tail_append.len(), 1);
        assert_eq!(tail_append[0].kind, SegmentedSessionEventKind::DisplayEvent);

        let metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");
        assert_eq!(metadata.next_global_seq, 4);
        assert_eq!(metadata.message_count, 3);
    }

    /// Verifies tool-call detail references are backfilled with assigned JSONL sequence numbers.
    /// 验证工具调用详情引用会回填已分配的 JSONL 序号。
    #[test]
    fn append_message_tail_backfills_tool_detail_ref_sequence() {
        let root = test_root("append_message_tail_backfills_tool_detail_ref_sequence");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let user = test_message(
            "m-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "prompt",
        );
        let mut call = test_message(
            "m-call",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolCallOutput,
            "",
        );
        call.metadata = Some(json!({
            "tool_call_id": "call-1",
            "tool_name": "shell_command",
            "result": {
                "status": "completed",
                "tool_result_id": "tool-result-call-1",
                "detail_ref": {
                    "kind": "session_record",
                    "record_id": "m-detail",
                    "message_index": 2
                }
            }
        }));
        let mut detail = test_message(
            "m-detail",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolDetailOutput,
            "result",
        );
        detail.visible = false;
        detail.metadata = Some(json!({
            "tool_detail": true,
            "tool_call_id": "call-1",
            "tool_result_id": "tool-result-call-1"
        }));

        let lines = append_segmented_session_message_tail(&session_dir, &[user, call, detail])
            .expect("append messages");

        assert_eq!(
            lines[1].payload["message"]["metadata"]["result"]["detail_ref"]["seq"],
            json!(3)
        );
        assert_eq!(lines[2].seq, 3);
    }

    /// Verifies non-tail transcript changes are rejected instead of being silently appended out of order.
    /// 验证非尾部转录变更会被拒绝，而不是被静默乱序追加。
    #[test]
    fn append_message_tail_rejects_non_tail_change() {
        let root = test_root("append_message_tail_rejects_non_tail_change");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let first_messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &first_messages)
            .expect("append first messages");

        let changed_messages = vec![
            first_messages[0].clone(),
            test_message(
                "m-inserted",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ThinkingOutput,
                "inserted thinking",
            ),
            first_messages[1].clone(),
        ];
        let error = append_segmented_session_message_tail(&session_dir, &changed_messages)
            .expect_err("reject non-tail change");

        assert!(error.contains("non-tail transcript change"));
    }

    /// Verifies shortened transcript updates become rollback-bounded replacement history.
    /// 验证缩短的转录更新会变成带回滚边界的替换历史。
    #[test]
    fn append_message_update_records_rollback_replacement_for_shortened_history() {
        let root =
            test_root("append_message_update_records_rollback_replacement_for_shortened_history");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let original_messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &original_messages)
            .expect("append original messages");
        let replacement_messages = vec![test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "edited prompt",
        )];

        let replacement_lines =
            append_segmented_session_message_update(&session_dir, &replacement_messages)
                .expect("append rollback replacement");

        assert_eq!(
            replacement_lines.first().map(|line| line.kind),
            Some(SegmentedSessionEventKind::ThreadRolledBack)
        );
        let replacement_metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read replacement metadata");
        assert_eq!(
            replacement_metadata.latest_rollback_seq,
            replacement_lines.first().map(|line| line.seq)
        );
        assert_eq!(replacement_metadata.message_count, 1);
        assert_eq!(replacement_lines.len(), replacement_messages.len() + 1);
        let latest_window =
            read_segmented_session_turn_window(&session_dir, None, 10).expect("read latest window");
        assert_eq!(
            latest_window
                .lines
                .iter()
                .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["m-user-1"]
        );
        let latest_messages =
            segmented_lines_to_session_messages(&latest_window.lines).expect("project messages");
        assert_eq!(latest_messages[0].body, "edited prompt");
        assert!(!latest_window.has_more_history);
        let earliest_seq = latest_window.lines.first().map(|line| line.seq);
        let previous_window = read_segmented_session_turn_window(&session_dir, earliest_seq, 1)
            .expect("read previous window");
        assert!(previous_window.lines.is_empty());

        let replay =
            replay_segmented_session_context(&session_dir).expect("replay segmented context");
        let replay_messages = conversation_state_to_session_messages(
            &replay.expect("replay result").conversation_state,
        );
        assert_eq!(replay_messages[0].body, "edited prompt");
        assert_eq!(replay_messages.len(), 1);
    }

    /// Verifies messages removed by rollback can be reintroduced with the same id later.
    /// 验证被回滚删除的消息后续可以用相同标识重新引入。
    #[test]
    fn append_after_rollback_ignores_removed_pre_rollback_messages() {
        let root = test_root("append_after_rollback_ignores_removed_pre_rollback_messages");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let original_messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
            test_message(
                "m-assistant-removed",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "removed answer",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &original_messages)
            .expect("append original messages");
        let replacement_messages = vec![original_messages[0].clone(), original_messages[1].clone()];
        let replacement_lines =
            append_segmented_session_message_update(&session_dir, &replacement_messages)
                .expect("append rollback replacement");
        assert_eq!(
            replacement_lines.first().map(|line| line.kind),
            Some(SegmentedSessionEventKind::ThreadRolledBack)
        );

        let restored_messages = vec![
            replacement_messages[0].clone(),
            replacement_messages[1].clone(),
            original_messages[2].clone(),
        ];
        let restored_lines =
            append_segmented_session_message_update(&session_dir, &restored_messages)
                .expect("append restored message");

        assert_eq!(restored_lines.len(), 1);
        assert_eq!(
            restored_lines[0].payload["message_id"],
            serde_json::json!("m-assistant-removed")
        );
        let latest_window =
            read_segmented_session_turn_window(&session_dir, None, 10).expect("read latest window");
        let latest_message_ids = latest_window
            .lines
            .iter()
            .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(
            latest_message_ids,
            vec!["m-user-1", "m-assistant-1", "m-assistant-removed"]
        );
    }

    /// Verifies segmented event windows can be read lazily and projected back to transcript messages.
    /// 验证分段事件窗口可以懒读取并投影回转录消息。
    #[test]
    fn read_event_window_projects_back_to_session_messages() {
        let root = test_root("read_event_window_projects_back_to_session_messages");
        let session_dir = create_segmented_session(
            &root,
            test_session_request(SegmentPolicy {
                max_lines: 3,
                max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
            }),
        )
        .expect("create session");
        let messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
            test_message(
                "m-user-2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "second prompt",
            ),
            test_message(
                "m-assistant-2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "second answer",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");

        let active_tail =
            read_segmented_session_active_tail(&session_dir, 1).expect("read active tail");
        assert_eq!(active_tail.len(), 1);
        assert_eq!(active_tail[0].payload["message_id"], "m-assistant-2");

        let window = read_segmented_session_event_window(&session_dir, Some(4), 2)
            .expect("read event window");
        assert_eq!(
            window.iter().map(|line| line.seq).collect::<Vec<_>>(),
            vec![2, 3]
        );
        let projected =
            segmented_lines_to_session_messages(&window).expect("project session messages");
        assert_eq!(
            projected
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec!["m-assistant-1", "m-user-2"]
        );
    }

    /// Verifies turn windows keep complete recent turns even when one turn has multiple detail rows.
    /// 验证轮次窗口在单轮包含多条详情行时仍保留完整最近轮次。
    #[test]
    fn read_turn_window_preserves_complete_display_turns() {
        let root = test_root("read_turn_window_preserves_complete_display_turns");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-tool-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolCallOutput,
                "first tool detail",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
            test_message(
                "m-user-2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "second prompt",
            ),
            test_message(
                "m-tool-2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
                "second tool detail",
            ),
            test_message(
                "m-assistant-2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "second answer",
            ),
            test_message(
                "m-user-3",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "third prompt",
            ),
            test_message(
                "m-assistant-3",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "third answer",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");

        let latest_window =
            read_segmented_session_turn_window(&session_dir, None, 2).expect("read turn window");
        assert_eq!(
            latest_window
                .lines
                .iter()
                .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec![
                "m-user-2",
                "m-tool-2",
                "m-assistant-2",
                "m-user-3",
                "m-assistant-3"
            ]
        );
        assert!(latest_window.has_more_history);
        let earliest_seq = latest_window.lines.first().map(|line| line.seq);

        let previous_window = read_segmented_session_turn_window(&session_dir, earliest_seq, 1)
            .expect("read previous turn window");
        assert_eq!(
            previous_window
                .lines
                .iter()
                .map(|line| line.payload["message_id"].as_str().unwrap_or_default())
                .collect::<Vec<_>>(),
            vec!["m-user-1", "m-tool-1", "m-assistant-1"]
        );
        assert!(!previous_window.has_more_history);
    }

    /// Verifies state documents are overwritten independently from JSONL event lines.
    /// 验证状态文档独立于 JSONL 事件行被覆盖写入。
    #[test]
    fn write_state_overwrites_without_touching_segment_lines() {
        let root = test_root("write_state_overwrites_without_touching_segment_lines");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let before_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read lines before state overwrite");

        write_state(
            &session_dir,
            &SegmentedSessionStateDocument {
                schema_version: 1,
                session_id: "session-alpha".to_string(),
                updated_at: "2026-06-06T11:00:00Z".to_string(),
                runtime_state: json!({ "phase": "checkpoint" }),
                current_context: json!({ "cursor": 7 }),
                session_boundary_state: Value::Null,
            },
        )
        .expect("write state");

        let after_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read lines after state overwrite");
        assert_eq!(before_lines, after_lines);
        let state: SegmentedSessionStateDocument =
            read_required_json(&session_dir.join(SEGMENTED_STATE_FILE)).expect("read state");
        assert_eq!(state.runtime_state["phase"], "checkpoint");
    }

    /// Verifies runtime-state writes preserve the independently managed session-boundary partition.
    /// 验证运行态写入会保留独立管理的会话边界状态分区。
    #[test]
    fn write_state_preserves_existing_session_boundary_partition() {
        let root = test_root("write_state_preserves_existing_session_boundary_partition");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");

        replace_session_boundary_state(
            &session_dir,
            json!({
                "status": "idle",
                "last_turn_id": "turn-alpha"
            }),
        )
        .expect("write boundary state");
        write_state(
            &session_dir,
            &SegmentedSessionStateDocument {
                schema_version: 1,
                session_id: "session-alpha".to_string(),
                updated_at: "2026-06-08T10:00:00Z".to_string(),
                runtime_state: json!({ "phase": "runtime-checkpoint" }),
                current_context: json!({ "cursor": 9 }),
                session_boundary_state: Value::Null,
            },
        )
        .expect("write runtime state");

        let state = read_state(&session_dir)
            .expect("read state")
            .expect("state exists");
        assert_eq!(state.runtime_state["phase"], "runtime-checkpoint");
        assert_eq!(state.session_boundary_state["status"], "idle");
        assert_eq!(state.session_boundary_state["last_turn_id"], "turn-alpha");
    }

    /// Verifies runtime state entries are mapped into overwrite-only state documents.
    /// 验证运行态条目会映射进覆盖式状态文档。
    #[test]
    fn runtime_state_entry_maps_to_segmented_state_document() {
        let mut runtime_state = SessionRuntimeStateEntry {
            active_model_id: Some("model-alpha".to_string()),
            context_start_message_id: Some("message-1".to_string()),
            last_compaction_id: Some("compaction-1".to_string()),
            updated_at: "2026-06-06T13:00:00Z".to_string(),
            ..SessionRuntimeStateEntry::default()
        };
        runtime_state.anchor_context_budget_fingerprint = Some("budget-a".to_string());
        runtime_state.ensure_ai_tool_configuration(&Default::default());
        runtime_state
            .ai_tool_configuration
            .as_mut()
            .expect("ai capability configuration")
            .disabled_capability_kinds
            .insert(AiToolKind::ImageGeneration);

        let state = state_document_from_runtime_state("session-alpha", &runtime_state)
            .expect("build state document");

        assert_eq!(state.session_id, "session-alpha");
        assert_eq!(state.updated_at, "2026-06-06T13:00:00Z");
        assert_eq!(state.runtime_state["active_model_id"], "model-alpha");
        assert_eq!(
            state.current_context["context_start_message_id"],
            "message-1"
        );
        assert_eq!(state.current_context["last_compaction_id"], "compaction-1");
        assert_eq!(
            state.current_context["anchor_context_budget_fingerprint"],
            "budget-a"
        );
        assert_eq!(
            state.runtime_state["ai_tool_configuration"]["disabledCapabilityKinds"][0],
            "image_generation"
        );
        assert!(state.runtime_state.get("conversation_state").is_none());
        assert!(state.current_context.get("conversation_state").is_none());
    }

    /// Verifies runtime JSONL can be rebuilt from canonical conversation state and read back.
    /// 验证 runtime JSONL 可从规范会话状态重建并读取还原。
    #[test]
    fn runtime_log_rebuilds_and_restores_conversation_state() {
        let root = test_root("runtime_log_rebuilds_and_restores_conversation_state");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];
        let conversation_state = conversation_state_from_session_messages(&messages);

        let metadata = rebuild_runtime_log_from_conversation_state(
            &session_dir,
            &conversation_state,
            "2026-06-09T10:00:00Z",
        )
        .expect("rebuild runtime log");
        let restored = read_runtime_conversation_state(&session_dir)
            .expect("read runtime conversation")
            .expect("runtime conversation exists");

        assert_eq!(metadata.active_segment, "runtime/r001.jsonl");
        assert!(session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE).exists());
        assert!(session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE).exists());
        assert!(session_dir.join("runtime").join("r001.jsonl").exists());
        assert_eq!(restored.messages.len(), conversation_state.messages.len());
    }

    /// Verifies missing runtime logs are rebuilt from metadata-indexed display segments.
    /// 验证缺失的 runtime 日志会从 metadata 索引的展示分段重建。
    #[test]
    fn runtime_log_missing_rebuilds_from_segmented_display_segments() {
        let root = test_root("runtime_log_missing_rebuilds_from_segmented_display_segments");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");

        let restored = ensure_runtime_log_from_segmented_context(&session_dir)
            .expect("ensure runtime log")
            .expect("restored conversation");

        assert!(session_dir.join(SEGMENTED_RUNTIME_METADATA_FILE).exists());
        assert_eq!(restored.messages.len(), 2);
        assert_eq!(restored.messages[0].id, "m-user-1");
    }

    /// Verifies runtime rebuild follows metadata-indexed display segments after rollover.
    /// 验证 runtime 重建会在展示分段滚动后继续按 metadata 索引读取。
    #[test]
    fn runtime_log_missing_rebuilds_from_non_initial_display_segments() {
        let root = test_root("runtime_log_missing_rebuilds_from_non_initial_display_segments");
        let policy = SegmentPolicy {
            max_lines: 2,
            ..SegmentPolicy::default()
        };
        let session_dir =
            create_segmented_session(&root, test_session_request(policy)).expect("create session");
        let messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
            test_message(
                "m-user-2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "second prompt",
            ),
            test_message(
                "m-assistant-2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "second answer",
            ),
            test_message(
                "m-user-3",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "third prompt",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");
        let metadata = read_session_metadata(&session_dir).expect("read metadata");

        assert_eq!(metadata.active_segment, "s003.jsonl");
        assert_eq!(metadata.segments.len(), 3);

        let restored = ensure_runtime_log_from_segmented_context(&session_dir)
            .expect("ensure runtime log")
            .expect("restored conversation");

        assert_eq!(restored.messages.len(), messages.len());
        assert_eq!(
            restored.messages.last().map(|message| message.id.as_str()),
            Some("m-user-3")
        );
        assert!(session_dir.join("runtime").join("r001.jsonl").exists());
    }

    /// Verifies a corrupt runtime JSONL row is backed up and removed without losing valid rows.
    /// 验证损坏 runtime JSONL 行会被备份并移除，同时不丢失有效行。
    #[test]
    fn runtime_log_tolerates_and_repairs_corrupt_jsonl_tail() {
        let root = test_root("runtime_log_tolerates_and_repairs_corrupt_jsonl_tail");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];
        let conversation_state = conversation_state_from_session_messages(&messages);
        rebuild_runtime_log_from_conversation_state(
            &session_dir,
            &conversation_state,
            "2026-06-09T10:00:00Z",
        )
        .expect("rebuild runtime log");
        let runtime_segment_path = session_dir.join("runtime").join("r001.jsonl");
        {
            let mut file = OpenOptions::new()
                .append(true)
                .open(&runtime_segment_path)
                .expect("open runtime segment");
            writeln!(file, "{{ broken").expect("append corrupt line");
        }

        let lines = read_runtime_log_lines(&session_dir)
            .expect("read repaired runtime lines")
            .expect("runtime lines exist");
        let cleaned = read_json_lines::<RuntimeLogEventLine>(&runtime_segment_path)
            .expect("read cleaned runtime segment");
        let corrupt_backups = fs::read_dir(session_dir.join("runtime"))
            .expect("read runtime dir")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"))
            .count();

        assert_eq!(lines.len(), cleaned.len());
        assert_eq!(lines.len(), 3);
        let repair_event = match lines.last().map(|line| &line.event) {
            Some(RuntimeLogEvent::RuntimeRepair(event)) => event,
            _ => panic!("expected runtime repair event"),
        };
        assert!(repair_event.detail.contains("runtime/r001.jsonl lines 3"));
        assert_eq!(corrupt_backups, 1);
        assert!(session_dir.join(SEGMENTED_RUNTIME_INDEX_FILE).exists());
    }

    /// Verifies fully damaged runtime logs are regenerated from display transcript segments.
    /// 验证完全损坏的 runtime 日志会从展示转录分段重新生成。
    #[test]
    fn runtime_log_full_corruption_falls_back_to_segmented_replay() {
        let root = test_root("runtime_log_full_corruption_falls_back_to_segmented_replay");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];
        append_segmented_session_message_tail(&session_dir, &messages).expect("append messages");
        let conversation_state = conversation_state_from_session_messages(&messages);
        rebuild_runtime_log_from_conversation_state(
            &session_dir,
            &conversation_state,
            "2026-06-09T10:00:00Z",
        )
        .expect("rebuild runtime log");
        fs::write(session_dir.join("runtime").join("r001.jsonl"), "{ broken\n")
            .expect("damage runtime segment");

        let restored = ensure_runtime_log_from_segmented_context(&session_dir)
            .expect("ensure runtime log")
            .expect("restored conversation");
        let lines = read_runtime_log_lines(&session_dir)
            .expect("read rebuilt runtime lines")
            .expect("runtime lines exist");

        assert_eq!(restored.messages.len(), 2);
        assert_eq!(restored.messages[0].id, "m-user-1");
        assert_eq!(lines.len(), 2);
    }

    /// Verifies compaction runtime backups copy active files and keep only the newest five backups.
    /// 验证压缩运行时备份会复制活动文件，并且仅保留最新五份备份。
    #[test]
    fn runtime_log_backup_before_compaction_copies_sources_and_keeps_latest_five() {
        let root =
            test_root("runtime_log_backup_before_compaction_copies_sources_and_keeps_latest_five");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let messages = vec![test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        )];
        let conversation_state = conversation_state_from_session_messages(&messages);
        rebuild_runtime_log_from_conversation_state(
            &session_dir,
            &conversation_state,
            "2026-06-09T10:00:00Z",
        )
        .expect("rebuild runtime log");

        for index in 0..6 {
            let copied_files = backup_runtime_log_before_compaction(
                &session_dir,
                &format!("compaction-{index}"),
                &format!("20260609{index:02}"),
            )
            .expect("backup runtime log");
            assert!(
                copied_files
                    .iter()
                    .any(|path| path.ends_with("runtime/r001.jsonl"))
            );
        }
        let mut backup_names = fs::read_dir(session_dir.join(SEGMENTED_RUNTIME_BACKUP_DIR))
            .expect("read backup dir")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        backup_names.sort();

        assert_eq!(backup_names.len(), 5);
        assert!(
            !backup_names
                .iter()
                .any(|name| name.contains("compaction-0"))
        );
        assert!(
            backup_names
                .iter()
                .any(|name| name.contains("compaction-5"))
        );
        assert!(
            backup_names
                .iter()
                .all(|name| name.contains("segment-r001"))
        );
        assert!(backup_names.iter().all(|name| name.contains("seq1-")));
        assert!(backup_names.iter().any(|name| name.contains("seq1-6")));

        let runtime_lines = read_runtime_log_lines(&session_dir)
            .expect("read runtime log")
            .expect("runtime log should exist");
        let latest_compaction = runtime_lines
            .iter()
            .rev()
            .find_map(|line| match &line.event {
                RuntimeLogEvent::ContextCompactionApplied(event) => Some(event),
                _ => None,
            })
            .expect("context compaction audit event should exist");
        assert!(
            latest_compaction
                .backup_files
                .iter()
                .any(|path| path.ends_with("runtime/r001.jsonl"))
        );
        assert!(
            latest_compaction
                .compaction_fingerprint
                .as_deref()
                .is_some_and(|fingerprint| !fingerprint.is_empty())
        );
    }

    /// Verifies runtime sync appends only new tail lines when the existing log is a prefix.
    /// 验证现有日志是前缀时 runtime 同步只追加新的尾部行。
    #[test]
    fn runtime_log_sync_appends_tail_when_prefix_matches() {
        let root = test_root("runtime_log_sync_appends_tail_when_prefix_matches");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let first_messages = vec![test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        )];
        let first_state = conversation_state_from_session_messages(&first_messages);
        let first_metadata =
            sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
                .expect("sync first runtime log");
        let second_messages = vec![
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "first prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "first answer",
            ),
        ];
        let second_state = conversation_state_from_session_messages(&second_messages);

        let second_metadata =
            sync_runtime_log_from_conversation_state(&session_dir, &second_state, "t2")
                .expect("append runtime tail");
        let lines = read_runtime_log_lines(&session_dir)
            .expect("read runtime lines")
            .expect("runtime lines exist");

        assert_eq!(first_metadata.created_at, "t1");
        assert_eq!(second_metadata.created_at, "t1");
        assert_eq!(second_metadata.updated_at, "t2");
        assert_eq!(lines.len(), 2);
    }

    /// Verifies runtime sync rebuilds when the desired history is not prefixed by the existing log.
    /// 验证目标历史不再以前缀匹配现有日志时 runtime 同步会重建。
    #[test]
    fn runtime_log_sync_rebuilds_when_prefix_mismatches() {
        let root = test_root("runtime_log_sync_rebuilds_when_prefix_mismatches");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let first_state = conversation_state_from_session_messages(&[test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        )]);
        sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
            .expect("sync first runtime log");
        let rewritten_state = conversation_state_from_session_messages(&[test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "rewritten prompt",
        )]);

        let rebuilt_metadata =
            sync_runtime_log_from_conversation_state(&session_dir, &rewritten_state, "t3")
                .expect("rebuild runtime log");
        let restored = read_runtime_conversation_state(&session_dir)
            .expect("read runtime conversation")
            .expect("runtime conversation exists");

        assert_eq!(rebuilt_metadata.created_at, "t3");
        let vulcan_context::SessionConversationPayload::UserText { text, .. } =
            &restored.messages[0].payload
        else {
            panic!("expected user text payload");
        };
        assert_eq!(text, "rewritten prompt");
    }

    /// Verifies runtime-only audit events survive a later canonical conversation rebuild.
    /// 验证仅审计 runtime 事件会在后续规范会话重建后继续保留。
    #[test]
    fn runtime_log_sync_preserves_audit_events_when_rebuilding() {
        let root = test_root("runtime_log_sync_preserves_audit_events_when_rebuilding");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let first_state = conversation_state_from_session_messages(&[test_message(
            "m-user-1",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "first prompt",
        )]);
        sync_runtime_log_from_conversation_state(&session_dir, &first_state, "t1")
            .expect("sync first runtime log");
        backup_runtime_log_before_compaction(&session_dir, "compaction-a", "t2")
            .expect("record compaction backup");
        record_runtime_injection_applied(
            &session_dir,
            "t3",
            RuntimeInjectionAppliedEvent {
                turn_id: Some(String::from("turn-a")),
                combined_fingerprint: Some(String::from("combined-a")),
                persistent_fingerprint: Some(String::from("persistent-a")),
                transient_fingerprint: Some(String::from("transient-a")),
                has_transient: true,
                merged_baseline_fingerprint: None,
                source_node_ids: vec![String::from("persistent:0:persistent-a")],
            },
        )
        .expect("record injection audit");
        append_runtime_log_event_if_present(
            &session_dir,
            "t3",
            RuntimeLogEvent::SystemUpdate(RuntimeSystemUpdateEvent {
                turn_id: Some(String::from("turn-a")),
                update_id: String::from("system-update-a"),
                effective_after_event_id: None,
                text: String::from("Use concise answers."),
                trust: RuntimeSystemUpdateTrust::TrustedHost,
                lowering_hint: RuntimeSystemUpdateLoweringHint::Auto,
                fingerprint: Some(String::from("system-update-fp")),
                source: Some(String::from("test")),
            }),
        )
        .expect("record system update audit");
        append_runtime_log_event_if_present(
            &session_dir,
            "t3",
            RuntimeLogEvent::InjectionCancelled(RuntimeInjectionCancelledEvent {
                turn_id: Some(String::from("turn-a")),
                cancellation_id: String::from("cancel-a"),
                source_node_ids: vec![String::from("node-a")],
                previous_persistent_fingerprint: Some(String::from("persistent-a")),
                previous_transient_fingerprint: Some(String::from("transient-a")),
                reason: Some(String::from("test cancellation")),
            }),
        )
        .expect("record injection cancellation audit");
        append_runtime_log_event_if_present(
            &session_dir,
            "t3",
            RuntimeLogEvent::InjectionMerged(RuntimeInjectionMergedEvent {
                turn_id: Some(String::from("turn-a")),
                merge_id: String::from("merge-a"),
                compaction_id: Some(String::from("compaction-a")),
                previous_persistent_fingerprint: Some(String::from("persistent-a")),
                merged_baseline_fingerprint: Some(String::from("merged-a")),
                source_node_ids: vec![String::from("node-a")],
            }),
        )
        .expect("record injection merge audit");
        append_runtime_log_event_if_present(
            &session_dir,
            "t3",
            RuntimeLogEvent::ProviderRequestCheckpoint(RuntimeProviderRequestCheckpointEvent {
                turn_id: Some(String::from("turn-a")),
                checkpoint_id: String::from("request-a"),
                provider_id: String::from("anthropic"),
                model_id: String::from("claude"),
                provider_family: String::from("anthropic"),
                capability_fingerprint: Some(String::from("cap-a")),
                prompt_fingerprint: Some(String::from("prompt-a")),
                injection_fingerprint: Some(String::from("inject-a")),
                request_fingerprint: Some(String::from("request-fp-a")),
                cache_policy_fingerprint: Some(String::from("cache-a")),
                cache_policy_diagnostic: Some(String::from("cache_hints_unsupported")),
                converter_version: 1,
            }),
        )
        .expect("record provider request checkpoint");
        append_runtime_log_event_if_present(
            &session_dir,
            "t3",
            RuntimeLogEvent::ProviderResponseCheckpoint(RuntimeProviderResponseCheckpointEvent {
                turn_id: Some(String::from("turn-a")),
                checkpoint_id: String::from("response-a"),
                provider_id: String::from("anthropic"),
                model_id: String::from("claude"),
                provider_family: String::from("anthropic"),
                finish_reason: Some(String::from("stop")),
                native_state: Some(RuntimeProviderNativeStateEnvelope::new(
                    "anthropic",
                    "anthropic",
                    "claude",
                    json!({ "content": [] }),
                )),
                usage: None,
            }),
        )
        .expect("record provider response checkpoint");
        append_runtime_log_event_if_present(
            &session_dir,
            "t3",
            RuntimeLogEvent::ProviderLoweringRepair(RuntimeProviderLoweringRepairEvent {
                turn_id: Some(String::from("turn-a")),
                repair_id: String::from("lowering-repair-a"),
                provider_id: String::from("anthropic"),
                model_id: String::from("claude"),
                provider_family: String::from("anthropic"),
                action: String::from("fallback"),
                reason: String::from("unsupported native system update"),
                affected_items: 1,
            }),
        )
        .expect("record provider lowering repair");
        let rewritten_state = conversation_state_from_session_messages(&[
            test_message(
                "m-user-1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "rewritten prompt",
            ),
            test_message(
                "m-assistant-1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "rewritten answer",
            ),
        ]);

        sync_runtime_log_from_conversation_state(&session_dir, &rewritten_state, "t4")
            .expect("rebuild runtime log");
        let lines = read_runtime_log_lines(&session_dir)
            .expect("read runtime lines")
            .expect("runtime lines exist");
        let restored = read_runtime_conversation_state(&session_dir)
            .expect("read runtime conversation")
            .expect("runtime conversation exists");

        assert_eq!(restored.messages.len(), 2);
        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.event, RuntimeLogEvent::ContextCompactionApplied(_)) })
        );
        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.event, RuntimeLogEvent::InjectionApplied(_)) })
        );
        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.event, RuntimeLogEvent::SystemUpdate(_)) })
        );
        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.event, RuntimeLogEvent::InjectionCancelled(_)) })
        );
        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.event, RuntimeLogEvent::InjectionMerged(_)) })
        );
        assert!(
            lines.iter().any(|line| {
                matches!(line.event, RuntimeLogEvent::ProviderRequestCheckpoint(_))
            })
        );
        assert!(
            lines.iter().any(|line| {
                matches!(line.event, RuntimeLogEvent::ProviderResponseCheckpoint(_))
            })
        );
        assert!(
            lines
                .iter()
                .any(|line| { matches!(line.event, RuntimeLogEvent::ProviderLoweringRepair(_)) })
        );
    }

    /// Verifies child task sessions use the same segment model and begin with SubagentTaskMeta.
    /// 验证子任务会话使用同一分段模型并以 SubagentTaskMeta 开头。
    #[test]
    fn create_task_session_writes_canonical_task_meta_first_line() {
        let root = test_root("create_task_session_writes_canonical_task_meta_first_line");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let task_dir = create_segmented_task_session(
            &session_dir,
            SegmentedTaskSessionCreateRequest {
                task_session_id: "task-alpha".to_string(),
                parent_start_seq: Some(1),
                agent_kind: SubagentAgentKind::AgentMd,
                agent_profile: SubagentAgentProfile {
                    agent_id: Some("reviewer".to_string()),
                    agent_name: Some("Reviewer".to_string()),
                    agent_md_path: Some("agents/reviewer/agent.md".to_string()),
                    prompt_source: Some("agent_md".to_string()),
                },
                spawned_by: SubagentSpawnedBy {
                    parent_turn_id: Some("turn-1".to_string()),
                    parent_event_seq: Some(1),
                    tool_call_id: Some("tool-1".to_string()),
                },
                created_at: "2026-06-06T10:03:00Z".to_string(),
                status: "running".to_string(),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("create task session");

        let task_metadata: SegmentedTaskSessionMetadata =
            read_required_json(&task_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read task metadata");
        assert_eq!(task_metadata.active_segment, "s001.jsonl");
        assert_eq!(task_metadata.next_local_seq, 1);
        let task_metadata_value: Value =
            read_required_json(&task_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read task metadata value");
        assert_eq!(task_metadata_value["agent_kind"], "agent_md");
        assert_eq!(
            task_metadata_value["agent_profile"]["prompt_source"],
            "agent_md"
        );

        let task_lines = read_segment_event_lines(&task_dir.join("s001.jsonl"))
            .expect("read task segment lines");
        assert_eq!(task_lines.len(), 1);
        assert_eq!(task_lines[0].seq, 0);
        assert_eq!(
            task_lines[0].kind,
            SegmentedSessionEventKind::SubagentTaskMeta
        );
        assert_eq!(task_lines[0].payload["agent_kind"], "agent_md");

        let parent_metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read parent metadata");
        assert_eq!(parent_metadata.tasks.len(), 1);
        assert_eq!(parent_metadata.tasks[0].dir, "task/task-alpha");
        assert_eq!(parent_metadata.tasks[0].line_count, 1);
        let parent_metadata_value: Value =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read parent metadata value");
        assert_eq!(parent_metadata_value["tasks"][0]["agent_kind"], "agent_md");

        let parent_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read parent segment lines");
        assert_eq!(parent_lines.len(), 2);
        assert_eq!(
            parent_lines[1].kind,
            SegmentedSessionEventKind::SubagentTaskStarted
        );
        assert_eq!(parent_lines[1].payload["task_dir"], "task/task-alpha");
        assert_eq!(parent_metadata.tasks[0].parent_start_seq, Some(1));
    }

    /// Builds a reusable task-session creation request for link-update tests.
    /// 为关联更新测试构建可复用的子任务会话创建请求。
    fn test_task_session_request() -> SegmentedTaskSessionCreateRequest {
        SegmentedTaskSessionCreateRequest {
            task_session_id: "task-alpha".to_string(),
            parent_start_seq: Some(1),
            agent_kind: SubagentAgentKind::MainClone,
            agent_profile: SubagentAgentProfile {
                agent_id: None,
                agent_name: Some("Main clone".to_string()),
                agent_md_path: None,
                prompt_source: Some("main_session".to_string()),
            },
            spawned_by: SubagentSpawnedBy {
                parent_turn_id: Some("turn-1".to_string()),
                parent_event_seq: Some(1),
                tool_call_id: Some("tool-1".to_string()),
            },
            created_at: "2026-06-06T10:03:00Z".to_string(),
            status: "running".to_string(),
            segment_policy: SegmentPolicy::default(),
        }
    }

    /// Verifies task metadata and parent mirrors write every required task field.
    /// 验证子任务元数据与父级镜像会写出所有必需任务字段。
    #[test]
    fn task_metadata_and_parent_mirror_include_required_fields() {
        let root = test_root("task_metadata_and_parent_mirror_include_required_fields");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let task_dir = create_segmented_task_session(&session_dir, test_task_session_request())
            .expect("create task session");
        let task_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
        let task_metadata: Value =
            read_required_json(&task_metadata_path).expect("read task metadata");
        assert_eq!(task_metadata["agent_kind"], "main_clone");
        assert!(task_metadata["agent_profile"].is_object());
        assert!(task_metadata["spawned_by"].is_object());
        assert_eq!(task_metadata["status"], "running");
        assert_eq!(task_metadata["active_segment"], "s001.jsonl");
        assert_eq!(task_metadata["next_local_seq"], 1);
        assert!(task_metadata["segment_policy"].is_object());
        assert_eq!(task_metadata["segments"][0]["line_count"], 1);

        append_segmented_task_events(
            &task_dir,
            &[SegmentedSessionEventInput {
                timestamp: "2026-06-06T10:04:00Z".to_string(),
                kind: SegmentedSessionEventKind::AssistantOutput,
                payload: json!({ "body": "progress" }),
                turn_index: Some(1),
            }],
        )
        .expect("append task event");

        let linked_line = record_segmented_task_link_update(
            &session_dir,
            SegmentedTaskLinkUpdate {
                task_session_id: "task-alpha".to_string(),
                timestamp: "2026-06-06T10:05:00Z".to_string(),
                status: "completed".to_string(),
                terminal: true,
                payload: json!({ "result": "done" }),
                turn_index: Some(1),
            },
        )
        .expect("record task link update");

        let parent_metadata_path = session_dir.join(SEGMENTED_METADATA_FILE);
        let parent_metadata: Value =
            read_required_json(&parent_metadata_path).expect("read parent metadata");
        assert_eq!(parent_metadata["tasks"][0]["agent_kind"], "main_clone");
        assert_eq!(
            parent_metadata["tasks"][0]["active_segment"],
            "task/task-alpha/s001.jsonl"
        );
        assert_eq!(
            parent_metadata["tasks"][0]["metadata_file"],
            "task/task-alpha/metadata.json"
        );
        assert_eq!(parent_metadata["tasks"][0]["status"], "completed");
        assert_eq!(parent_metadata["tasks"][0]["line_count"], 2);
        assert_eq!(
            parent_metadata["tasks"][0]["created_at"],
            "2026-06-06T10:03:00Z"
        );
        assert_eq!(
            parent_metadata["tasks"][0]["updated_at"],
            "2026-06-06T10:05:00Z"
        );
        assert!(parent_metadata["tasks"][0]["agent_profile"].is_object());
        assert_eq!(linked_line.payload["agent_kind"], "main_clone");
        assert!(linked_line.payload["agent_profile"].is_object());
    }

    /// Verifies linked and completed child-task updates are reflected in parent and child metadata.
    /// 验证子任务 linked 与 completed 更新会同步反映到父元数据与子元数据。
    #[test]
    fn task_link_updates_write_parent_events_and_refresh_metadata() {
        let root = test_root("task_link_updates_write_parent_events_and_refresh_metadata");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let task_dir = create_segmented_task_session(&session_dir, test_task_session_request())
            .expect("create task session");

        let linked_event = record_segmented_task_link_update(
            &session_dir,
            SegmentedTaskLinkUpdate {
                task_session_id: "task-alpha".to_string(),
                timestamp: "2026-06-06T10:04:00Z".to_string(),
                status: "running".to_string(),
                terminal: false,
                payload: json!({ "progress": "half" }),
                turn_index: Some(2),
            },
        )
        .expect("record linked event");
        assert_eq!(
            linked_event.kind,
            SegmentedSessionEventKind::SubagentTaskEventLinked
        );
        assert_eq!(linked_event.payload["detail"]["progress"], "half");
        assert_eq!(linked_event.payload["agent_kind"], "main_clone");
        assert!(linked_event.payload["agent_profile"].is_object());

        let completed_event = record_segmented_task_link_update(
            &session_dir,
            SegmentedTaskLinkUpdate {
                task_session_id: "task-alpha".to_string(),
                timestamp: "2026-06-06T10:05:00Z".to_string(),
                status: "completed".to_string(),
                terminal: true,
                payload: json!({ "result": "done" }),
                turn_index: Some(2),
            },
        )
        .expect("record completed event");
        assert_eq!(
            completed_event.kind,
            SegmentedSessionEventKind::SubagentTaskCompleted
        );
        assert_eq!(completed_event.payload["detail"]["result"], "done");
        assert_eq!(completed_event.payload["agent_kind"], "main_clone");
        assert!(completed_event.payload["agent_profile"].is_object());

        let parent_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read parent segment lines");
        assert_eq!(
            parent_lines
                .iter()
                .map(|line| line.kind)
                .collect::<Vec<_>>(),
            vec![
                SegmentedSessionEventKind::SessionMeta,
                SegmentedSessionEventKind::SubagentTaskStarted,
                SegmentedSessionEventKind::SubagentTaskEventLinked,
                SegmentedSessionEventKind::SubagentTaskCompleted,
            ]
        );

        let parent_metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read parent metadata");
        assert_eq!(parent_metadata.tasks[0].status, "completed");
        assert_eq!(
            parent_metadata.tasks[0].parent_end_seq,
            Some(completed_event.seq)
        );

        let task_metadata: SegmentedTaskSessionMetadata =
            read_required_json(&task_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read task metadata");
        assert_eq!(task_metadata.status, "completed");
        assert_eq!(task_metadata.updated_at, "2026-06-06T10:05:00Z");
    }

    /// Verifies parent task-link payloads point at the child's latest active segment after rollover.
    /// 验证子任务滚动分段后父任务关联载荷会指向子任务最新活动分段。
    #[test]
    fn task_link_update_uses_child_active_segment_after_rollover() {
        let root = test_root("task_link_update_uses_child_active_segment_after_rollover");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let mut request = test_task_session_request();
        request.segment_policy = SegmentPolicy {
            max_lines: 1,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        };
        let task_dir =
            create_segmented_task_session(&session_dir, request).expect("create task session");
        append_segmented_task_events(
            &task_dir,
            &[SegmentedSessionEventInput {
                timestamp: "2026-06-06T10:04:00Z".to_string(),
                kind: SegmentedSessionEventKind::DisplayEvent,
                payload: json!({ "progress": "rolled" }),
                turn_index: Some(2),
            }],
        )
        .expect("append task event after rollover");

        let linked_event = record_segmented_task_link_update(
            &session_dir,
            SegmentedTaskLinkUpdate {
                task_session_id: "task-alpha".to_string(),
                timestamp: "2026-06-06T10:05:00Z".to_string(),
                status: "running".to_string(),
                terminal: false,
                payload: json!({ "progress": "linked" }),
                turn_index: Some(2),
            },
        )
        .expect("record linked event");
        let parent_metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read parent metadata");

        assert_eq!(
            linked_event.payload["task_active_segment"],
            "task/task-alpha/s002.jsonl"
        );
        assert_eq!(
            parent_metadata.tasks[0].active_segment,
            "task/task-alpha/s002.jsonl"
        );
    }

    /// Verifies transcript mapping keeps thinking, tool, and hidden response records out of visible user/assistant kinds.
    /// 验证转录映射会将思考、工具与隐藏响应记录排除在可见用户/助手类型之外。
    #[test]
    fn transcript_mapping_marks_only_user_prompts_and_assistant_outputs_as_visible_turns() {
        let user = segmented_event_input_from_message(
            0,
            &test_message(
                "m-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "prompt",
            ),
            0,
        )
        .expect("map user prompt");
        let assistant = segmented_event_input_from_message(
            1,
            &test_message(
                "m-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "answer",
            ),
            0,
        )
        .expect("map assistant output");
        let thinking = segmented_event_input_from_message(
            2,
            &test_message(
                "m-thinking",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ThinkingOutput,
                "reasoning",
            ),
            0,
        )
        .expect("map thinking output");
        let tool = segmented_event_input_from_message(
            3,
            &test_message(
                "m-tool",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
                "tool result",
            ),
            0,
        )
        .expect("map tool output");
        let mut hidden = test_message(
            "m-hidden",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::AssistantOutput,
            "hidden answer",
        );
        hidden.visible = false;
        let hidden_response =
            segmented_event_input_from_message(4, &hidden, 0).expect("map hidden response");

        assert_eq!(user.kind, SegmentedSessionEventKind::UserPrompt);
        assert_eq!(assistant.kind, SegmentedSessionEventKind::AssistantOutput);
        assert_eq!(thinking.kind, SegmentedSessionEventKind::DisplayEvent);
        assert_eq!(tool.kind, SegmentedSessionEventKind::DisplayEvent);
        assert_eq!(
            hidden_response.kind,
            SegmentedSessionEventKind::ResponseItem
        );
    }

    /// Verifies segment turn-range pruning skips impossible segments and keeps unknown ranges safe.
    /// 验证分段轮次范围裁剪会跳过不可能命中的分段，并对未知范围保持保守。
    #[test]
    fn segment_may_contain_turn_uses_known_bounds_conservatively() {
        let mut segment = SegmentMetadata::empty("s001.jsonl".to_string(), 0);
        assert!(segment_may_contain_turn(&segment, 3));

        segment.first_turn_index = Some(2);
        segment.last_turn_index = Some(4);
        assert!(segment_may_contain_turn(&segment, 3));
        assert!(!segment_may_contain_turn(&segment, 1));
        assert!(!segment_may_contain_turn(&segment, 5));

        segment.first_turn_index = Some(6);
        segment.last_turn_index = None;
        assert!(!segment_may_contain_turn(&segment, 5));
        assert!(segment_may_contain_turn(&segment, 6));
    }

    /// Verifies metadata patching updates metadata without modifying segment files.
    /// 验证元数据补丁会更新元数据且不会修改分段文件。
    #[test]
    fn metadata_patch_does_not_touch_segment_lines() {
        let root = test_root("metadata_patch_does_not_touch_segment_lines");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        let before_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read segment before patch");

        let patched = patch_segmented_session_metadata(
            &session_dir,
            SegmentedSessionMetadataPatch {
                updated_at: Some("2026-06-06T12:00:00Z".to_string()),
                title: Some(Some("Renamed".to_string())),
                title_source: Some(Some("manual".to_string())),
                archived_at: None,
            },
        )
        .expect("patch metadata");

        let after_lines = read_segment_event_lines(&session_dir.join("s001.jsonl"))
            .expect("read segment after patch");
        assert_eq!(before_lines, after_lines);
        assert_eq!(patched.title.as_deref(), Some("Renamed"));
        assert_eq!(patched.title_source.as_deref(), Some("manual"));
        assert_eq!(patched.updated_at, "2026-06-06T12:00:00Z");
    }

    /// Verifies archive and restore move the session directory without rewriting segments.
    /// 验证归档与恢复只移动会话目录且不会重写分段。
    #[test]
    fn archive_and_restore_preserve_segment_lines() {
        let root = test_root("archive_and_restore_preserve_segment_lines");
        let request = test_session_request(SegmentPolicy::default());
        let session_dir = create_segmented_session(&root, request.clone()).expect("create session");
        append_segmented_session_events(
            &session_dir,
            &[test_event(
                "2026-06-06T10:01:00Z",
                SegmentedSessionEventKind::UserPrompt,
            )],
        )
        .expect("append event");
        let before_lines =
            read_segment_event_lines(&session_dir.join("s001.jsonl")).expect("read active lines");

        let archived_dir = archive_segmented_session(
            &root,
            &request.project_tag,
            &request.created_date,
            &request.session_id,
            "2026-06-06T12:00:00Z",
        )
        .expect("archive session");
        assert!(!session_dir.exists());
        assert!(archived_dir.exists());
        let archived_lines = read_segment_event_lines(&archived_dir.join("s001.jsonl"))
            .expect("read archived lines");
        assert_eq!(before_lines, archived_lines);
        let archived_metadata: SegmentedSessionMetadata =
            read_required_json(&archived_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read archived metadata");
        assert_eq!(
            archived_metadata.archived_at.as_deref(),
            Some("2026-06-06T12:00:00Z")
        );

        let restored_dir = restore_segmented_session(
            &root,
            &request.project_tag,
            &request.created_date,
            &request.session_id,
            "2026-06-06T12:30:00Z",
        )
        .expect("restore session");
        assert!(!archived_dir.exists());
        assert!(restored_dir.exists());
        let restored_lines = read_segment_event_lines(&restored_dir.join("s001.jsonl"))
            .expect("read restored lines");
        assert_eq!(before_lines, restored_lines);
        let restored_metadata: SegmentedSessionMetadata =
            read_required_json(&restored_dir.join(SEGMENTED_METADATA_FILE))
                .expect("read restored metadata");
        assert_eq!(restored_metadata.archived_at, None);
    }

    /// Verifies archive, restore, and delete can locate segmented directories by session id alone.
    /// 验证归档、恢复与删除可仅按会话标识定位分段目录。
    #[test]
    fn archive_restore_and_delete_by_id_locate_session_directory() {
        let root = test_root("archive_restore_and_delete_by_id_locate_session_directory");
        let request = test_session_request(SegmentPolicy::default());
        let session_dir = create_segmented_session(&root, request.clone()).expect("create session");

        let archived_dir = archive_segmented_session_by_id(
            &root,
            &request.project_tag,
            &request.session_id,
            "2026-06-06T12:00:00Z",
        )
        .expect("archive by id")
        .expect("archived path");
        assert!(!session_dir.exists());
        assert!(archived_dir.exists());

        let restored_dir = restore_segmented_session_by_id(
            &root,
            &request.project_tag,
            &request.session_id,
            "2026-06-06T12:30:00Z",
        )
        .expect("restore by id")
        .expect("restored path");
        assert!(!archived_dir.exists());
        assert!(restored_dir.exists());

        let archived_dir = archive_segmented_session_by_id(
            &root,
            &request.project_tag,
            &request.session_id,
            "2026-06-06T13:00:00Z",
        )
        .expect("archive again")
        .expect("archived path");
        assert!(archived_dir.exists());
        assert!(
            delete_segmented_session_by_id(&root, true, &request.project_tag, &request.session_id)
                .expect("delete archived by id")
        );
        assert!(!archived_dir.exists());
        assert!(
            !delete_segmented_session_by_id(&root, true, &request.project_tag, &request.session_id)
                .expect("missing delete is no-op")
        );
    }

    /// Verifies reverse scanning finds the latest replacement-history compaction anchor.
    /// 验证倒序扫描会找到最新的 replacement-history 压缩锚点。
    #[test]
    fn reverse_scan_finds_latest_compaction_anchor_and_suffix() {
        let root = test_root("reverse_scan_finds_latest_compaction_anchor_and_suffix");
        let policy = SegmentPolicy {
            max_lines: 2,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        };
        let session_dir =
            create_segmented_session(&root, test_session_request(policy)).expect("create session");
        append_segmented_session_events(
            &session_dir,
            &[
                test_event(
                    "2026-06-06T10:01:00Z",
                    SegmentedSessionEventKind::UserPrompt,
                ),
                SegmentedSessionEventInput {
                    timestamp: "2026-06-06T10:02:00Z".to_string(),
                    kind: SegmentedSessionEventKind::Compacted,
                    payload: json!({
                        "replacement_history": [
                            { "role": "system", "content": "summary" }
                        ]
                    }),
                    turn_index: Some(2),
                },
                test_event(
                    "2026-06-06T10:03:00Z",
                    SegmentedSessionEventKind::AssistantOutput,
                ),
                test_event(
                    "2026-06-06T10:04:00Z",
                    SegmentedSessionEventKind::UserPrompt,
                ),
            ],
        )
        .expect("append compaction events");
        let metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");

        let scan = find_latest_compaction_anchor(&session_dir, &metadata.segments)
            .expect("scan compaction anchor");

        let anchor = scan.anchor.expect("expected compaction anchor");
        assert_eq!(anchor.compaction_seq, 2);
        assert_eq!(anchor.segment_file, "s002.jsonl");
        assert_eq!(anchor.replacement_history[0]["content"], "summary");
        assert_eq!(scan.suffix.len(), 2);
        assert_eq!(scan.suffix[0].seq, 3);
        assert_eq!(scan.suffix[1].seq, 4);
        assert_eq!(scan.scanned_segment_count, 2);
    }

    /// Verifies reverse scanning returns all events in replay order when no anchor exists.
    /// 验证没有锚点时倒序扫描会按重放顺序返回全部事件。
    #[test]
    fn reverse_scan_without_anchor_returns_all_events_in_forward_order() {
        let root = test_root("reverse_scan_without_anchor_returns_all_events_in_forward_order");
        let policy = SegmentPolicy {
            max_lines: 2,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        };
        let session_dir =
            create_segmented_session(&root, test_session_request(policy)).expect("create session");
        append_segmented_session_events(
            &session_dir,
            &[
                test_event(
                    "2026-06-06T10:01:00Z",
                    SegmentedSessionEventKind::UserPrompt,
                ),
                test_event(
                    "2026-06-06T10:02:00Z",
                    SegmentedSessionEventKind::AssistantOutput,
                ),
                test_event(
                    "2026-06-06T10:03:00Z",
                    SegmentedSessionEventKind::UserPrompt,
                ),
            ],
        )
        .expect("append events");
        let metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");

        let scan = find_latest_compaction_anchor(&session_dir, &metadata.segments)
            .expect("scan without anchor");

        assert_eq!(scan.anchor, None);
        assert_eq!(
            scan.suffix.iter().map(|line| line.seq).collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
        assert_eq!(scan.scanned_segment_count, metadata.segments.len());
    }

    /// Verifies reverse scanning captures TurnContext and rollback markers found after an anchor.
    /// 验证倒序扫描会捕获锚点之后发现的 TurnContext 与回滚标记。
    #[test]
    fn reverse_scan_captures_turn_context_and_rollback_markers() {
        let root = test_root("reverse_scan_captures_turn_context_and_rollback_markers");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        append_segmented_session_events(
            &session_dir,
            &[
                SegmentedSessionEventInput {
                    timestamp: "2026-06-06T10:01:00Z".to_string(),
                    kind: SegmentedSessionEventKind::Compacted,
                    payload: json!({
                        "replacement_history": [
                            { "role": "assistant", "content": "compressed" }
                        ]
                    }),
                    turn_index: Some(1),
                },
                SegmentedSessionEventInput {
                    timestamp: "2026-06-06T10:02:00Z".to_string(),
                    kind: SegmentedSessionEventKind::TurnContext,
                    payload: json!({
                        "previous_turn_settings": {
                            "model": "qwen"
                        },
                        "reference_context_item": {
                            "id": "ctx-1"
                        }
                    }),
                    turn_index: Some(2),
                },
                SegmentedSessionEventInput {
                    timestamp: "2026-06-06T10:03:00Z".to_string(),
                    kind: SegmentedSessionEventKind::ThreadRolledBack,
                    payload: json!({ "drop_user_turns": 1 }),
                    turn_index: Some(3),
                },
            ],
        )
        .expect("append recovery events");
        let metadata: SegmentedSessionMetadata =
            read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read metadata");

        let scan = find_latest_compaction_anchor(&session_dir, &metadata.segments)
            .expect("scan recovery anchors");

        assert_eq!(
            scan.anchor.as_ref().map(|anchor| anchor.compaction_seq),
            Some(1)
        );
        let turn_context = scan
            .latest_turn_context
            .expect("expected latest turn context");
        assert_eq!(turn_context.seq, 2);
        assert_eq!(
            turn_context.payload["previous_turn_settings"]["model"],
            "qwen"
        );
        assert_eq!(scan.rollback_markers.len(), 1);
        assert_eq!(scan.rollback_markers[0].seq, 3);
        assert_eq!(scan.rollback_markers[0].payload["drop_user_turns"], 1);
    }

    /// Verifies replay uses replacement history as baseline and applies the newest rollback boundary.
    /// 验证重放会使用 replacement history 作为基线，并应用最新回滚边界。
    #[test]
    fn replay_segmented_context_applies_compaction_suffix_and_rollback() {
        let root = test_root("replay_segmented_context_applies_compaction_suffix_and_rollback");
        let session_dir =
            create_segmented_session(&root, test_session_request(SegmentPolicy::default()))
                .expect("create session");
        append_segmented_session_events(
            &session_dir,
            &[
                SegmentedSessionEventInput {
                    timestamp: "10".to_string(),
                    kind: SegmentedSessionEventKind::Compacted,
                    payload: json!({
                        "replacement_history": [
                            {
                                "id": "summary-user",
                                "role": "user",
                                "content": "compressed prompt",
                                "created_at": "10"
                            },
                            {
                                "id": "summary-assistant",
                                "role": "assistant",
                                "content": "compressed answer",
                                "created_at": "11"
                            }
                        ]
                    }),
                    turn_index: Some(0),
                },
                segmented_event_input_from_message(
                    1,
                    &test_message(
                        "rolled-back-user",
                        SessionMessageKind::User,
                        SessionMessageRecordKind::Transcript,
                        "rolled back prompt",
                    ),
                    1,
                )
                .expect("rolled back message"),
                SegmentedSessionEventInput {
                    timestamp: "30".to_string(),
                    kind: SegmentedSessionEventKind::ThreadRolledBack,
                    payload: json!({ "drop_user_turns": 1 }),
                    turn_index: Some(1),
                },
                segmented_event_input_from_message(
                    2,
                    &test_message(
                        "kept-user",
                        SessionMessageKind::User,
                        SessionMessageRecordKind::Transcript,
                        "kept prompt",
                    ),
                    2,
                )
                .expect("kept user"),
                segmented_event_input_from_message(
                    3,
                    &test_message(
                        "kept-assistant",
                        SessionMessageKind::Assistant,
                        SessionMessageRecordKind::AssistantOutput,
                        "kept answer",
                    ),
                    2,
                )
                .expect("kept assistant"),
            ],
        )
        .expect("append replay events");

        let replay = replay_segmented_session_context(&session_dir)
            .expect("replay segmented context")
            .expect("replay result");
        let messages = conversation_state_to_session_messages(&replay.conversation_state);

        assert_eq!(
            messages
                .iter()
                .map(|message| message.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "summary-user",
                "summary-assistant",
                "kept-user",
                "kept-assistant"
            ]
        );
        assert_eq!(
            replay.last_compaction_id.as_deref(),
            Some("segmented-compaction-1")
        );
        assert_eq!(
            replay.context_start_message_id.as_deref(),
            Some("summary-user")
        );
        assert!(
            replay
                .updated_at
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
        );
    }
}
