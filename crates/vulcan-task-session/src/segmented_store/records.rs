use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SegmentedTaskMetadataDocument {
    /// Schema version for this segmented task metadata.
    /// 当前分段子任务元数据结构版本。
    pub(super) schema_version: u32,
    /// Embedded task-session metadata used by the existing runtime contract.
    /// 现有运行时契约使用的内嵌子任务会话元数据。
    pub(super) task_session: TaskSessionMetadata,
    /// Cached task-session status.
    /// 缓存的子任务会话状态。
    pub(super) status: TaskSessionStatus,
    /// Agent source kind used by this task.
    /// 当前子任务使用的代理来源类型。
    pub(super) agent_kind: String,
    /// Agent profile audit payload.
    /// 代理配置审计载荷。
    pub(super) agent_profile: Value,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    pub(super) updated_at: u64,
    /// Active JSONL segment file.
    /// 当前活动 JSONL 分段文件。
    pub(super) active_segment: String,
    /// Segment metadata list.
    /// 分段元数据列表。
    pub(super) segments: Vec<SegmentMetadata>,
    /// Next local sequence number for this task stream.
    /// 当前子任务流的下一个本地序号。
    pub(super) next_seq: u64,
    /// Rollover policy for this task stream.
    /// 当前子任务流的滚动策略。
    pub(super) segment_policy: SegmentPolicy,
}

impl SegmentedTaskMetadataDocument {
    /// Builds one metadata document from canonical task metadata.
    /// 根据规范子任务元数据构建元数据文档。
    pub(super) fn new(task_session: TaskSessionMetadata) -> Self {
        Self {
            schema_version: 1,
            status: task_session.status,
            agent_kind: segmented_agent_kind(&task_session),
            agent_profile: segmented_agent_profile(&task_session),
            updated_at: task_session.updated_at,
            active_segment: "s001.jsonl".to_string(),
            segments: vec![SegmentMetadata::empty("s001.jsonl", 0)],
            next_seq: 0,
            segment_policy: SegmentPolicy::default(),
            task_session,
        }
    }
}

/// Reads one optional segmented task metadata document.
/// 读取一份可选的分段子任务元数据文档。
pub(super) fn read_task_metadata_document(
    path: &Path,
) -> Result<Option<SegmentedTaskMetadataDocument>, SessionStoreError> {
    read_json_file::<SegmentedTaskMetadataDocument>(path)
}

/// Reads one required segmented task metadata document.
/// 读取一份必需的分段子任务元数据文档。
pub(super) fn read_required_task_metadata_document(
    path: &Path,
) -> Result<SegmentedTaskMetadataDocument, SessionStoreError> {
    read_required_json::<SegmentedTaskMetadataDocument>(path)
}

/// State document stored beside segmented task JSONL files.
/// 与分段子任务 JSONL 文件并列保存的状态文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SegmentedTaskStateDocument {
    /// Schema version for this state document.
    /// 当前状态文档结构版本。
    pub(super) schema_version: u32,
    /// Task-session identifier.
    /// 子任务会话标识符。
    pub(super) task_session_id: SessionNodeId,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    pub(super) updated_at: u64,
    /// Current API-facing context document.
    /// 当前 API 实际上下文文档。
    pub(super) context: Option<SessionContextDocument>,
    /// Lightweight runtime checkpoint.
    /// 轻量运行时检查点。
    pub(super) runtime_checkpoint: Option<SessionRuntimeCheckpoint>,
}

impl SegmentedTaskStateDocument {
    /// Builds an empty state document for one task session.
    /// 为一个子任务会话构建空状态文档。
    pub(super) fn new(task_session_id: SessionNodeId, updated_at: u64) -> Self {
        Self {
            schema_version: 1,
            task_session_id,
            updated_at,
            context: None,
            runtime_checkpoint: None,
        }
    }
}

/// Segment rollover policy for one task stream.
/// 单个子任务流的分段滚动策略。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SegmentPolicy {
    /// Maximum JSONL line count per segment.
    /// 单个分段的最大 JSONL 行数。
    pub(super) max_lines: u64,
    /// Maximum byte count per segment.
    /// 单个分段的最大字节数。
    pub(super) max_bytes: u64,
}

impl Default for SegmentPolicy {
    /// Builds the default segment policy.
    /// 构建默认分段策略。
    fn default() -> Self {
        Self {
            max_lines: DEFAULT_SEGMENT_MAX_LINES,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        }
    }
}

/// Metadata for one JSONL segment.
/// 单个 JSONL 分段的元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SegmentMetadata {
    /// Segment file name.
    /// 分段文件名。
    pub(super) file: String,
    /// First sequence number intended for this segment.
    /// 当前分段预期的首个序号。
    pub(super) start_seq: u64,
    /// Number of JSONL lines in this segment.
    /// 当前分段中的 JSONL 行数。
    pub(super) line_count: u64,
    /// Total bytes written to this segment.
    /// 当前分段已写入字节数。
    pub(super) byte_len: u64,
    /// First timestamp written to this segment.
    /// 当前分段首条时间戳。
    pub(super) first_timestamp: Option<String>,
    /// Last timestamp written to this segment.
    /// 当前分段末条时间戳。
    pub(super) last_timestamp: Option<String>,
    /// First turn index represented by this segment when known.
    /// 已知时当前分段表示的首个轮次索引。
    pub(super) first_turn_index: Option<i64>,
    /// Last turn index represented by this segment when known.
    /// 已知时当前分段表示的最后轮次索引。
    pub(super) last_turn_index: Option<i64>,
    /// Last compaction sequence observed in this segment when known.
    /// 已知时当前分段内最后一次压缩事件序号。
    pub(super) last_compaction_seq: Option<u64>,
}

impl SegmentMetadata {
    /// Builds an empty segment metadata record.
    /// 构建一个空分段元数据记录。
    pub(super) fn empty(file: impl Into<String>, start_seq: u64) -> Self {
        Self {
            file: file.into(),
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

/// JSONL event line shared by main and task segmented streams.
/// 主会话与子任务分段流共享的 JSONL 事件行。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct SegmentedEventLine {
    /// Stream-local sequence number.
    /// 流内本地序号。
    pub(super) seq: u64,
    /// Event timestamp text.
    /// 事件时间戳文本。
    pub(super) timestamp: String,
    /// Segmented event kind.
    /// 分段事件类型。
    #[serde(rename = "type")]
    pub(super) kind: String,
    /// Event payload.
    /// 事件载荷。
    pub(super) payload: Value,
    /// Optional turn index.
    /// 可选轮次索引。
    pub(super) turn_index: Option<i64>,
}
