//! Event, request, replay, and catalog record types for segmented session storage.
//! 分段会话存储的事件、请求、重放与目录记录类型。

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::conversation_protocol::SessionConversationState;

use super::{SegmentPolicy, SegmentedSessionEventKind, SegmentedSessionMetadata};

/// Overwrite-only state document stored next to segmented JSONL files.
/// 存放在分段 JSONL 旁边的覆盖式状态文档。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedSessionStateDocument {
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
pub(in crate::workspace_store) struct SegmentedSessionEventLine {
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
pub(in crate::workspace_store) struct SegmentedSessionEventInput {
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
pub(in crate::workspace_store) struct SegmentedSessionCreateRequest {
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

/// Request used to fork one persisted segmented session directory.
/// 用于分叉单个持久化分段会话目录的请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::workspace_store) struct SegmentedSessionForkRequest {
    /// Source session directory archive state.
    /// 源会话目录的归档状态。
    pub source_archived: bool,
    /// Source project tag used by the existing directory layout.
    /// 既有目录布局使用的源项目标签。
    pub source_project_tag: String,
    /// Source session identifier.
    /// 源会话标识。
    pub source_session_id: String,
    /// Target project tag used by the new directory layout.
    /// 新目录布局使用的目标项目标签。
    pub target_project_tag: String,
    /// Target session identifier.
    /// 目标会话标识。
    pub target_session_id: String,
    /// Target creation date used in the directory name.
    /// 目标会话目录名中使用的创建日期。
    pub target_created_date: String,
    /// Target creation timestamp.
    /// 目标创建时间戳。
    pub target_created_at: String,
    /// Target visible title.
    /// 目标可见标题。
    pub target_title: String,
    /// Target title-source key.
    /// 目标标题来源键。
    pub target_title_source: String,
}

/// Main session metadata patch with explicit optional updates.
/// 带有显式可选更新项的主会话元数据补丁。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::workspace_store) struct SegmentedSessionMetadataPatch {
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

/// Latest replacement-history anchor discovered by reverse segment scanning.
/// 通过分段倒扫发现的最新 replacement-history 锚点。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedCompactionAnchor {
    /// Sequence number of the replacement anchor event.
    /// replacement 锚点事件的序号。
    pub anchor_seq: u64,
    /// Event kind that supplied the replacement history.
    /// 提供 replacement history 的事件类型。
    pub anchor_kind: SegmentedSessionEventKind,
    /// Segment file that contains the replacement anchor event.
    /// 包含 replacement 锚点事件的分段文件。
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
pub(in crate::workspace_store) struct SegmentedTurnContextAnchor {
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
pub(in crate::workspace_store) struct SegmentedRollbackMarker {
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

/// Result returned by a reverse replacement anchor scan.
/// replacement 锚点倒扫返回的结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedCompactionScanResult {
    /// Latest replacement-history anchor when one was found.
    /// 找到时记录的最新 replacement-history 锚点。
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

/// Report produced after rewriting an invalid display transcript.
/// 重写无效显示转录后生成的报告。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedDisplayMaintenanceReport {
    /// Maintenance reason chosen by the detector.
    /// 检测器选择的维护原因。
    pub reason: String,
    /// Backup directory containing the original session files.
    /// 包含原始会话文件的备份目录。
    pub backup_dir: String,
    /// Number of physical segment files before rebuilding.
    /// 重建前的物理分段文件数量。
    pub original_segment_count: usize,
    /// Number of physical segment files after rebuilding.
    /// 重建后的物理分段文件数量。
    pub rebuilt_segment_count: usize,
    /// Number of effective display messages restored into clean segments.
    /// 恢复到干净分段中的有效显示消息数量。
    pub effective_message_count: usize,
}

/// Temporary output produced while building repaired display segments.
/// 构建修复后显示分段时产生的临时输出。
pub(in crate::workspace_store) struct RepairedDisplayBuild {
    /// Metadata that matches the repaired temporary segment files.
    /// 与修复后临时分段文件匹配的元数据。
    pub(in crate::workspace_store) metadata: SegmentedSessionMetadata,
    /// Temporary directory containing repaired segment files.
    /// 包含修复后分段文件的临时目录。
    pub(in crate::workspace_store) temp_dir: PathBuf,
}

/// Rebuilt conversation state produced by replaying segmented compaction history.
/// 通过重放分段压缩历史得到的会话状态。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedSessionReplayResult {
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

/// Transcript turn window read from segmented JSONL files.
/// 从分段 JSONL 文件读取出的转录轮次窗口。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(in crate::workspace_store) struct SegmentedSessionTurnWindow {
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
pub(in crate::workspace_store) struct SegmentedSessionCatalogEntry {
    /// Absolute segmented session directory path.
    /// 分段会话目录的绝对路径。
    pub session_dir: PathBuf,
    /// Normalized metadata read from metadata.json.
    /// 从 metadata.json 读取并归一化后的元数据。
    pub metadata: SegmentedSessionMetadata,
}
