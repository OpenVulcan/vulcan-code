use crate::compaction::TaskSessionCompactionStore;
use crate::model::{
    TaskInvocationEvent, TaskInvocationEventKind, TaskSessionId, TaskSessionMetadata,
    TaskSessionStatus,
};
use crate::store::TaskSessionStore;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use vulcan_session_core::{
    ParentSessionRef, SessionCompactionCheckpoint, SessionContextDocument, SessionEventRecord,
    SessionNodeId, SessionNodeStore, SessionRuntimeCheckpoint, SessionStoreError,
};

const SEGMENTED_TASK_DIR: &str = "task";
const SEGMENTED_METADATA_FILE: &str = "metadata.json";
const SEGMENTED_STATE_FILE: &str = "state.json";
const DEFAULT_SEGMENT_MAX_LINES: u64 = 3000;
const DEFAULT_SEGMENT_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Segmented TaskSession store rooted at one main segmented session directory.
/// 以一个主分段会话目录为根的分段子任务会话存储。
#[derive(Debug, Clone)]
pub struct SegmentedTaskSessionStore {
    /// Root segmented main-session directory.
    /// 根分段主会话目录。
    root_session_dir: PathBuf,
}

impl SegmentedTaskSessionStore {
    /// Creates a segmented task-session store rooted at one main session directory.
    /// 创建一个以主会话目录为根的分段子任务会话存储。
    pub fn new(root_session_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_session_dir: root_session_dir.into(),
        }
    }

    /// Lists direct child task-session identifiers by scanning task metadata only.
    /// 仅通过扫描子任务元数据列出直接子任务会话标识符。
    pub fn list_child_task_session_ids(
        &self,
        parent: ParentSessionRef,
    ) -> Result<Vec<TaskSessionId>, SessionStoreError> {
        let mut task_ids = self
            .list_child_task_sessions(parent)?
            .into_iter()
            .map(|metadata| metadata.task_session_id)
            .collect::<Vec<_>>();
        task_ids.sort();
        task_ids.dedup();
        Ok(task_ids)
    }

    /// Lists direct child task-session metadata with one metadata-only directory scan.
    /// 通过一次仅元数据目录扫描列出直接子任务会话元数据。
    pub fn list_child_task_sessions(
        &self,
        parent: ParentSessionRef,
    ) -> Result<Vec<TaskSessionMetadata>, SessionStoreError> {
        let mut metadata = self
            .load_all_task_documents()?
            .into_iter()
            .map(|(_, document)| document.task_session)
            .filter(|metadata| metadata.parent == parent)
            .collect::<Vec<_>>();
        metadata.sort_by_key(|metadata| metadata.task_session_id);
        metadata.dedup_by_key(|metadata| metadata.task_session_id);
        Ok(metadata)
    }

    /// Loads the newest task-local event payload by reverse-scanning segmented JSONL files.
    /// 通过反向扫描分段 JSONL 文件加载最新的子任务本地事件载荷。
    pub fn load_latest_event_payload(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Option<Value>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(task_session_id)?;
        let metadata =
            read_required_task_metadata_document(&task_dir.join(SEGMENTED_METADATA_FILE))?;
        for segment in metadata.segments.iter().rev() {
            let path = task_dir.join(&segment.file);
            if !path.exists() {
                continue;
            }
            for line in read_json_lines::<SegmentedEventLine>(&path)?
                .into_iter()
                .rev()
            {
                let Some(value) = line.payload.get("session_event").cloned() else {
                    continue;
                };
                let record =
                    serde_json::from_value::<SessionEventRecord>(value).map_err(to_store_error)?;
                if !record.payload.is_null() {
                    return Ok(Some(record.payload));
                }
            }
        }
        Ok(None)
    }

    /// Returns the direct child task directory root.
    /// 返回直属子任务目录根。
    fn task_root_dir(&self) -> PathBuf {
        self.root_session_dir.join(SEGMENTED_TASK_DIR)
    }

    /// Resolves the directory for one existing task session by its fixed task id path.
    /// 通过固定子任务标识路径解析一个已存在子任务会话的目录。
    fn indexed_task_dir(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<PathBuf, SessionStoreError> {
        self.find_task_dir(task_session_id)?.ok_or_else(|| {
            SessionStoreError::new(format!("TaskSession `{task_session_id}` is not indexed."))
        })
    }

    /// Finds one task directory by directly checking `task/<task_id>/metadata.json`.
    /// 通过直接检查 `task/<task_id>/metadata.json` 查找一个任务目录。
    fn find_task_dir(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Option<PathBuf>, SessionStoreError> {
        let expected_id = task_session_id.to_string();
        let direct_task_dir = self
            .task_root_dir()
            .join(sanitize_file_component(&expected_id));
        let direct_metadata_path = direct_task_dir.join(SEGMENTED_METADATA_FILE);
        if direct_metadata_path.exists()
            && let Some(document) = read_task_metadata_document(&direct_metadata_path)?
            && document.task_session.task_session_id.to_string() == expected_id
        {
            return Ok(Some(direct_task_dir));
        }
        Ok(None)
    }

    /// Loads every task metadata document under this root session.
    /// 加载该根会话下的全部子任务元数据文档。
    fn load_all_task_documents(
        &self,
    ) -> Result<Vec<(PathBuf, SegmentedTaskMetadataDocument)>, SessionStoreError> {
        let mut documents = Vec::new();
        let task_root = self.task_root_dir();
        if !task_root.exists() {
            return Ok(documents);
        }
        for entry in fs::read_dir(&task_root).map_err(to_store_error)? {
            let entry = entry.map_err(to_store_error)?;
            let task_dir = entry.path();
            if !task_dir.is_dir() {
                continue;
            }
            let metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
            if let Some(document) = read_task_metadata_document(&metadata_path)? {
                documents.push((task_dir, document));
            }
        }
        Ok(documents)
    }

    /// Resolves the directory path for a newly created task session.
    /// 解析一个新建子任务会话的目录路径。
    fn new_task_dir(&self, metadata: &TaskSessionMetadata) -> Result<PathBuf, SessionStoreError> {
        let task_dir_name = sanitize_file_component(&metadata.task_session_id.to_string());
        match &metadata.parent {
            ParentSessionRef::None => Err(SessionStoreError::new(
                "TaskSession metadata must have a main-session or task-session parent.",
            )),
            ParentSessionRef::MainSession { .. } => Ok(self.task_root_dir().join(task_dir_name)),
            ParentSessionRef::TaskSession { .. } => Ok(self.task_root_dir().join(task_dir_name)),
        }
    }

    /// Creates the target segmented layout for one task session.
    /// 为一个子任务会话创建目标分段布局。
    fn create_task_layout(task_dir: &Path) -> Result<(), SessionStoreError> {
        create_dir_all(task_dir)?;
        Ok(())
    }

    /// Appends one generic event line to a task session segment.
    /// 向一个子任务会话分段追加一条通用事件行。
    fn append_task_segment_event(
        &self,
        task_session_id: TaskSessionId,
        kind: impl Into<String>,
        timestamp_ms: u64,
        payload: Value,
    ) -> Result<SegmentedEventLine, SessionStoreError> {
        let task_dir = self.indexed_task_dir(task_session_id)?;
        let metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
        let mut metadata = read_required_task_metadata_document(&metadata_path)?;
        let segment_policy = metadata.segment_policy.clone();
        let line = append_segment_event(
            &task_dir,
            &mut metadata.segments,
            &mut metadata.active_segment,
            &mut metadata.next_seq,
            &segment_policy,
            kind.into(),
            timestamp_ms.to_string(),
            payload,
            None,
        )?;
        metadata.updated_at = timestamp_ms;
        metadata.task_session.updated_at = timestamp_ms;
        write_json_file(&metadata_path, &metadata)?;
        Ok(line)
    }

    /// Appends one parent invocation event into the owning parent stream and root session index when needed.
    /// 将一条父级调用事件追加到所属父级流中，并在需要时同步写入根会话索引。
    fn append_parent_invocation_event(
        &self,
        parent: &ParentSessionRef,
        event: &TaskInvocationEvent,
    ) -> Result<(), SessionStoreError> {
        match parent {
            ParentSessionRef::None => Err(SessionStoreError::new(
                "A root session cannot own a task invocation log through TaskSessionStore.",
            )),
            ParentSessionRef::MainSession { .. } => {
                let _ = append_main_session_task_link_event(&self.root_session_dir, event)?;
                Ok(())
            }
            ParentSessionRef::TaskSession {
                parent_task_session_id,
                ..
            } => {
                self.append_task_segment_event(
                    *parent_task_session_id,
                    "task_invocation",
                    event.timestamp_ms,
                    json!({ "task_invocation_event": event }),
                )?;
                let _ = append_main_session_task_link_event(&self.root_session_dir, event)?;
                Ok(())
            }
        }
    }

    /// Reads parent invocation events from a segmented parent stream.
    /// 从分段父级流中读取父级调用事件。
    fn read_parent_invocation_events(
        &self,
        parent: &ParentSessionRef,
    ) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
        match parent {
            ParentSessionRef::None => Ok(Vec::new()),
            ParentSessionRef::MainSession { .. } => {
                read_task_invocations_from_main_segments(&self.root_session_dir, parent)
            }
            ParentSessionRef::TaskSession {
                parent_task_session_id,
                ..
            } => {
                let task_dir = self.indexed_task_dir(*parent_task_session_id)?;
                read_task_invocations_from_task_segments(&task_dir, parent)
            }
        }
    }
}

impl SessionNodeStore for SegmentedTaskSessionStore {
    /// Appends one event record to a segmented task-session stream.
    /// 向分段子任务会话流追加一条事件记录。
    fn append_event(&self, record: SessionEventRecord) -> Result<(), SessionStoreError> {
        self.append_task_segment_event(
            record.node_id,
            session_event_kind_string(&record.kind),
            record.timestamp_ms,
            json!({ "session_event": record }),
        )?;
        Ok(())
    }

    /// Loads event records for one segmented task session.
    /// 加载一个分段子任务会话的事件记录。
    fn load_events(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Vec<SessionEventRecord>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(node_id)?;
        let metadata =
            read_required_task_metadata_document(&task_dir.join(SEGMENTED_METADATA_FILE))?;
        let mut records = Vec::new();
        for line in read_segment_lines(&task_dir, &metadata.segments)? {
            if let Some(value) = line.payload.get("session_event").cloned() {
                records.push(serde_json::from_value(value).map_err(to_store_error)?);
            }
        }
        records.sort_by_key(|record: &SessionEventRecord| record.sequence);
        Ok(records)
    }

    /// Saves the current API-facing context document into state.json.
    /// 将当前 API 实际上下文文档保存到 state.json。
    fn save_context(&self, context: SessionContextDocument) -> Result<(), SessionStoreError> {
        let task_dir = self.indexed_task_dir(context.node_id)?;
        let mut state = read_task_state(&task_dir)?.unwrap_or_else(|| {
            SegmentedTaskStateDocument::new(context.node_id, current_unix_millis_u64())
        });
        state.updated_at = current_unix_millis_u64();
        state.context = Some(context);
        write_json_file(&task_dir.join(SEGMENTED_STATE_FILE), &state)
    }

    /// Loads the current API-facing context document from state.json.
    /// 从 state.json 加载当前 API 实际上下文文档。
    fn load_context(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Option<SessionContextDocument>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(node_id)?;
        Ok(read_task_state(&task_dir)?.and_then(|state| state.context))
    }

    /// Saves a lightweight runtime checkpoint into state.json.
    /// 将轻量运行时检查点保存到 state.json。
    fn save_runtime_checkpoint(
        &self,
        checkpoint: SessionRuntimeCheckpoint,
    ) -> Result<(), SessionStoreError> {
        let task_dir = self.indexed_task_dir(checkpoint.node_id)?;
        let mut state = read_task_state(&task_dir)?.unwrap_or_else(|| {
            SegmentedTaskStateDocument::new(checkpoint.node_id, current_unix_millis_u64())
        });
        state.updated_at = current_unix_millis_u64();
        state.runtime_checkpoint = Some(checkpoint);
        write_json_file(&task_dir.join(SEGMENTED_STATE_FILE), &state)
    }

    /// Loads a lightweight runtime checkpoint from state.json.
    /// 从 state.json 加载轻量运行时检查点。
    fn load_runtime_checkpoint(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Option<SessionRuntimeCheckpoint>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(node_id)?;
        Ok(read_task_state(&task_dir)?.and_then(|state| state.runtime_checkpoint))
    }
}

impl TaskSessionStore for SegmentedTaskSessionStore {
    /// Creates a task session using the segmented task layout.
    /// 使用分段子任务布局创建子任务会话。
    fn create_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError> {
        create_dir_all(&self.task_root_dir())?;
        if self.find_task_dir(metadata.task_session_id)?.is_some() {
            return Err(SessionStoreError::new(format!(
                "TaskSession `{}` already exists.",
                metadata.task_session_id
            )));
        }
        let task_dir = self.new_task_dir(&metadata)?;
        Self::create_task_layout(&task_dir)?;
        let mut document = SegmentedTaskMetadataDocument::new(metadata.clone());
        let meta_line = SegmentedEventLine {
            seq: 0,
            timestamp: metadata.created_at.to_string(),
            kind: "subagent_task_meta".to_string(),
            payload: json!({
                "task_session_id": metadata.task_session_id,
                "root_session_id": metadata.root_session_id,
                "parent": metadata.parent,
                "public_task_id": metadata.public_task_id,
                "display_title": metadata.display_title,
                "description": metadata.description,
                "agent_kind": document.agent_kind,
                "agent_profile": document.agent_profile,
                "created_at": metadata.created_at,
                "status": metadata.status
            }),
            turn_index: None,
        };
        let bytes = append_json_line(&task_dir.join("s001.jsonl"), &meta_line)?;
        if let Some(segment) = document.segments.first_mut() {
            segment.line_count = 1;
            segment.byte_len = bytes;
            segment.start_seq = 0;
            segment.first_timestamp = Some(meta_line.timestamp.clone());
            segment.last_timestamp = Some(meta_line.timestamp.clone());
        }
        document.next_seq = 1;
        write_json_file(&task_dir.join(SEGMENTED_METADATA_FILE), &document)?;
        write_json_file(
            &task_dir.join(SEGMENTED_STATE_FILE),
            &SegmentedTaskStateDocument::new(metadata.task_session_id, metadata.created_at),
        )?;

        let created_event = TaskInvocationEvent {
            event_id: format!("task-created-{}", metadata.task_session_id),
            root_session_id: metadata.root_session_id,
            parent: metadata.parent.clone(),
            task_session_id: metadata.task_session_id,
            sequence: 0,
            kind: TaskInvocationEventKind::TaskCreated,
            status: metadata.status,
            prompt: None,
            timestamp_ms: metadata.created_at,
            payload: json!({
                "public_task_id": metadata.public_task_id,
                "display_title": metadata.display_title,
                "description": metadata.description,
                "agent_kind": document.agent_kind,
                "agent_profile": document.agent_profile,
                "subagent": {
                    "profile_id": metadata.subagent_profile_id,
                    "inherit_parent_config": metadata.inherit_parent_config
                }
            }),
        };
        self.append_task_invocation_event(metadata.parent, created_event)
    }

    /// Saves task-session metadata into metadata.json.
    /// 将子任务会话元数据保存到 metadata.json。
    fn save_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError> {
        let task_dir = self.indexed_task_dir(metadata.task_session_id)?;
        let metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
        let mut document = read_required_task_metadata_document(&metadata_path)?;
        document.task_session = metadata;
        document.status = document.task_session.status;
        document.updated_at = document.task_session.updated_at;
        document.agent_kind = segmented_agent_kind(&document.task_session);
        document.agent_profile = segmented_agent_profile(&document.task_session);
        write_json_file(&metadata_path, &document)
    }

    /// Loads task-session metadata by identifier.
    /// 按标识符加载子任务会话元数据。
    fn load_task_session(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Option<TaskSessionMetadata>, SessionStoreError> {
        let Some(task_dir) = self.find_task_dir(task_session_id)? else {
            return Ok(None);
        };
        Ok(
            read_task_metadata_document(&task_dir.join(SEGMENTED_METADATA_FILE))?
                .map(|document| document.task_session),
        )
    }

    /// Finds one task session by direct parent and public task id.
    /// 按直接父级与公开任务标识查找一个子任务会话。
    fn find_task_session_by_parent_and_public_id(
        &self,
        parent: ParentSessionRef,
        public_task_id: &str,
    ) -> Result<Option<TaskSessionMetadata>, SessionStoreError> {
        let normalized_public_task_id = public_task_id.trim();
        if normalized_public_task_id.is_empty() {
            return Ok(None);
        }
        Ok(self
            .load_all_task_documents()?
            .into_iter()
            .map(|(_, document)| document.task_session)
            .find(|metadata| {
                metadata.parent == parent && metadata.public_task_id == normalized_public_task_id
            }))
    }

    /// Appends a task invocation event to the direct parent segmented stream.
    /// 向直接父级分段流追加一条子任务调用事件。
    fn append_task_invocation_event(
        &self,
        parent: ParentSessionRef,
        event: TaskInvocationEvent,
    ) -> Result<(), SessionStoreError> {
        self.append_parent_invocation_event(&parent, &event)
    }

    /// Lists task invocation events for one direct parent.
    /// 列出某个直接父级的子任务调用事件。
    fn list_task_invocation_events(
        &self,
        parent: ParentSessionRef,
    ) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
        self.read_parent_invocation_events(&parent)
    }

    /// Saves one compaction checkpoint in the task-session stream.
    /// 在子任务会话流中保存一个压缩检查点。
    fn save_task_compaction_checkpoint(
        &self,
        checkpoint: SessionCompactionCheckpoint,
    ) -> Result<(), SessionStoreError> {
        self.append_task_segment_event(
            checkpoint.node_id,
            "compaction_marker",
            checkpoint.timestamp_ms,
            json!({ "compaction_checkpoint": checkpoint }),
        )?;
        Ok(())
    }

    /// Loads compaction checkpoints for one task session.
    /// 加载某个子任务会话的压缩检查点。
    fn load_task_compaction_checkpoints(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Vec<SessionCompactionCheckpoint>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(task_session_id)?;
        let metadata =
            read_required_task_metadata_document(&task_dir.join(SEGMENTED_METADATA_FILE))?;
        let mut checkpoints = Vec::new();
        for line in read_segment_lines(&task_dir, &metadata.segments)? {
            if let Some(value) = line.payload.get("compaction_checkpoint").cloned() {
                checkpoints.push(serde_json::from_value(value).map_err(to_store_error)?);
            }
        }
        checkpoints
            .sort_by_key(|checkpoint: &SessionCompactionCheckpoint| checkpoint.through_sequence);
        Ok(checkpoints)
    }
}

impl TaskSessionCompactionStore for SegmentedTaskSessionStore {
    /// Saves a compacted context document for a task session.
    /// 保存子任务会话的压缩后上下文文档。
    fn save_compacted_context(
        &self,
        context: SessionContextDocument,
    ) -> Result<(), SessionStoreError> {
        self.save_context(context)
    }

    /// Saves a compaction checkpoint for a task session.
    /// 保存子任务会话的压缩检查点。
    fn save_compaction_checkpoint(
        &self,
        checkpoint: SessionCompactionCheckpoint,
    ) -> Result<(), SessionStoreError> {
        self.save_task_compaction_checkpoint(checkpoint)
    }
}

/// Metadata document stored in one segmented task-session directory.
/// 保存在单个分段子任务会话目录中的元数据文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SegmentedTaskMetadataDocument {
    /// Schema version for this segmented task metadata.
    /// 当前分段子任务元数据结构版本。
    schema_version: u32,
    /// Embedded task-session metadata used by the existing runtime contract.
    /// 现有运行时契约使用的内嵌子任务会话元数据。
    task_session: TaskSessionMetadata,
    /// Cached task-session status.
    /// 缓存的子任务会话状态。
    status: TaskSessionStatus,
    /// Agent source kind used by this task.
    /// 当前子任务使用的代理来源类型。
    agent_kind: String,
    /// Agent profile audit payload.
    /// 代理配置审计载荷。
    agent_profile: Value,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    updated_at: u64,
    /// Active JSONL segment file.
    /// 当前活动 JSONL 分段文件。
    active_segment: String,
    /// Segment metadata list.
    /// 分段元数据列表。
    segments: Vec<SegmentMetadata>,
    /// Next local sequence number for this task stream.
    /// 当前子任务流的下一个本地序号。
    next_seq: u64,
    /// Rollover policy for this task stream.
    /// 当前子任务流的滚动策略。
    segment_policy: SegmentPolicy,
}

impl SegmentedTaskMetadataDocument {
    /// Builds one metadata document from canonical task metadata.
    /// 根据规范子任务元数据构建元数据文档。
    fn new(task_session: TaskSessionMetadata) -> Self {
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
fn read_task_metadata_document(
    path: &Path,
) -> Result<Option<SegmentedTaskMetadataDocument>, SessionStoreError> {
    read_json_file::<SegmentedTaskMetadataDocument>(path)
}

/// Reads one required segmented task metadata document.
/// 读取一份必需的分段子任务元数据文档。
fn read_required_task_metadata_document(
    path: &Path,
) -> Result<SegmentedTaskMetadataDocument, SessionStoreError> {
    read_required_json::<SegmentedTaskMetadataDocument>(path)
}

/// State document stored beside segmented task JSONL files.
/// 与分段子任务 JSONL 文件并列保存的状态文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
struct SegmentedTaskStateDocument {
    /// Schema version for this state document.
    /// 当前状态文档结构版本。
    schema_version: u32,
    /// Task-session identifier.
    /// 子任务会话标识符。
    task_session_id: SessionNodeId,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    updated_at: u64,
    /// Current API-facing context document.
    /// 当前 API 实际上下文文档。
    context: Option<SessionContextDocument>,
    /// Lightweight runtime checkpoint.
    /// 轻量运行时检查点。
    runtime_checkpoint: Option<SessionRuntimeCheckpoint>,
}

impl SegmentedTaskStateDocument {
    /// Builds an empty state document for one task session.
    /// 为一个子任务会话构建空状态文档。
    fn new(task_session_id: SessionNodeId, updated_at: u64) -> Self {
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
struct SegmentPolicy {
    /// Maximum JSONL line count per segment.
    /// 单个分段的最大 JSONL 行数。
    max_lines: u64,
    /// Maximum byte count per segment.
    /// 单个分段的最大字节数。
    max_bytes: u64,
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
struct SegmentMetadata {
    /// Segment file name.
    /// 分段文件名。
    file: String,
    /// First sequence number intended for this segment.
    /// 当前分段预期的首个序号。
    start_seq: u64,
    /// Number of JSONL lines in this segment.
    /// 当前分段中的 JSONL 行数。
    line_count: u64,
    /// Total bytes written to this segment.
    /// 当前分段已写入字节数。
    byte_len: u64,
    /// First timestamp written to this segment.
    /// 当前分段首条时间戳。
    first_timestamp: Option<String>,
    /// Last timestamp written to this segment.
    /// 当前分段末条时间戳。
    last_timestamp: Option<String>,
    /// First turn index represented by this segment when known.
    /// 已知时当前分段表示的首个轮次索引。
    first_turn_index: Option<i64>,
    /// Last turn index represented by this segment when known.
    /// 已知时当前分段表示的最后轮次索引。
    last_turn_index: Option<i64>,
    /// Last compaction sequence observed in this segment when known.
    /// 已知时当前分段内最后一次压缩事件序号。
    last_compaction_seq: Option<u64>,
}

impl SegmentMetadata {
    /// Builds an empty segment metadata record.
    /// 构建一个空分段元数据记录。
    fn empty(file: impl Into<String>, start_seq: u64) -> Self {
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
struct SegmentedEventLine {
    /// Stream-local sequence number.
    /// 流内本地序号。
    seq: u64,
    /// Event timestamp text.
    /// 事件时间戳文本。
    timestamp: String,
    /// Segmented event kind.
    /// 分段事件类型。
    #[serde(rename = "type")]
    kind: String,
    /// Event payload.
    /// 事件载荷。
    payload: Value,
    /// Optional turn index.
    /// 可选轮次索引。
    turn_index: Option<i64>,
}

/// Appends one JSONL event line while updating segment metadata.
/// 追加一条 JSONL 事件行并更新分段元数据。
fn append_segment_event(
    dir: &Path,
    segments: &mut Vec<SegmentMetadata>,
    active_segment: &mut String,
    next_seq: &mut u64,
    segment_policy: &SegmentPolicy,
    kind: String,
    timestamp: String,
    payload: Value,
    turn_index: Option<i64>,
) -> Result<SegmentedEventLine, SessionStoreError> {
    if segments.is_empty() {
        segments.push(SegmentMetadata::empty("s001.jsonl", *next_seq));
        *active_segment = "s001.jsonl".to_string();
    }
    let line = SegmentedEventLine {
        seq: *next_seq,
        timestamp,
        kind,
        payload,
        turn_index,
    };
    let encoded = serde_json::to_string(&line).map_err(to_store_error)?;
    let encoded_len = encoded.as_bytes().len() as u64 + 1;
    if should_roll_segment(
        segments.last().expect("segment exists"),
        segment_policy,
        encoded_len,
    ) {
        let next_file = segment_file_name(segments.len() + 1);
        segments.push(SegmentMetadata::empty(next_file.clone(), *next_seq));
        *active_segment = next_file;
    }
    let active_path = dir.join(active_segment.as_str());
    let byte_len = append_encoded_json_line(&active_path, &encoded)?;
    if let Some(segment) = segments.last_mut() {
        segment.line_count = segment.line_count.saturating_add(1);
        segment.byte_len = segment.byte_len.saturating_add(byte_len);
        if segment.first_timestamp.is_none() {
            segment.first_timestamp = Some(line.timestamp.clone());
        }
        segment.last_timestamp = Some(line.timestamp.clone());
        if let Some(turn_index) = turn_index {
            if segment.first_turn_index.is_none() {
                segment.first_turn_index = Some(turn_index);
            }
            segment.last_turn_index = Some(turn_index);
        }
        if line.kind == "compacted" {
            segment.last_compaction_seq = Some(line.seq);
        }
    }
    *next_seq = next_seq.saturating_add(1);
    Ok(line)
}

/// Returns whether a segment should roll before appending a new line.
/// 返回追加新行前是否应滚动分段。
fn should_roll_segment(
    segment: &SegmentMetadata,
    segment_policy: &SegmentPolicy,
    next_line_bytes: u64,
) -> bool {
    segment.line_count >= segment_policy.max_lines
        || segment.byte_len.saturating_add(next_line_bytes) > segment_policy.max_bytes
}

/// Appends a task-link event to the main session JSONL stream when metadata exists.
/// 当主会话元数据存在时向主会话 JSONL 流追加一条子任务关联事件。
fn append_main_session_task_link_event(
    root_session_dir: &Path,
    event: &TaskInvocationEvent,
) -> Result<bool, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(false);
    }
    let mut metadata = read_required_json::<Value>(&metadata_path)?;
    let mut segments = serde_json::from_value::<Vec<SegmentMetadata>>(
        metadata
            .get("segments")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(to_store_error)?;
    let mut active_segment = metadata
        .get("active_segment")
        .and_then(Value::as_str)
        .unwrap_or("s001.jsonl")
        .to_string();
    let mut next_seq = metadata
        .get("next_global_seq")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let segment_policy = serde_json::from_value::<SegmentPolicy>(
        metadata
            .get("segment_policy")
            .cloned()
            .unwrap_or_else(|| json!({})),
    )
    .unwrap_or_default();
    let task_dir = format!(
        "task/{}",
        sanitize_file_component(&event.task_session_id.to_string())
    );
    let task_metadata_file = format!("{task_dir}/{SEGMENTED_METADATA_FILE}");
    let task_projection = main_session_task_metadata_projection(root_session_dir, &task_dir)
        .unwrap_or_else(|_| TaskMetadataProjection::fallback());
    let task_active_segment = format!("{task_dir}/{}", task_projection.active_segment);
    let kind = match event.kind {
        TaskInvocationEventKind::TaskCreated | TaskInvocationEventKind::TaskStarted => {
            "subagent_task_started"
        }
        TaskInvocationEventKind::TaskCompleted => "subagent_task_completed",
        _ => "subagent_task_event_linked",
    };
    append_segment_event(
        root_session_dir,
        &mut segments,
        &mut active_segment,
        &mut next_seq,
        &segment_policy,
        kind.to_string(),
        event.timestamp_ms.to_string(),
        json!({
            "task_session_id": event.task_session_id,
            "task_dir": task_dir,
            "task_active_segment": task_active_segment,
            "task_metadata_file": task_metadata_file,
            "agent_kind": task_projection.agent_kind.clone(),
            "agent_profile": task_projection.agent_profile.clone(),
            "line_count": task_projection.line_count,
            "task_invocation_event": event,
            "status": event.status
        }),
        None,
    )?;
    metadata["segments"] = serde_json::to_value(segments).map_err(to_store_error)?;
    metadata["active_segment"] = Value::String(active_segment);
    metadata["next_global_seq"] = Value::from(next_seq);
    metadata["updated_at"] = Value::String(event.timestamp_ms.to_string());
    refresh_main_session_task_mirror(
        &mut metadata,
        event,
        &task_active_segment,
        task_projection.line_count,
        &task_projection.agent_kind,
        &task_projection.agent_profile,
    );
    write_json_file(&metadata_path, &metadata)?;
    Ok(true)
}

/// Parent-facing projection loaded from one child task metadata document.
/// 从单个子任务元数据文档加载的父级可见投影。
struct TaskMetadataProjection {
    /// Active child segment file name relative to the child task directory.
    /// 相对子任务目录的活动子分段文件名。
    active_segment: String,
    /// Total number of JSONL lines across child task segments.
    /// 子任务所有 JSONL 分段的总行数。
    line_count: u64,
    /// Agent source kind recorded by the child task metadata.
    /// 子任务元数据记录的代理来源类型。
    agent_kind: String,
    /// Agent profile payload recorded by the child task metadata.
    /// 子任务元数据记录的代理画像载荷。
    agent_profile: Value,
}

impl TaskMetadataProjection {
    /// Builds a strict fallback used only when parent-link metadata cannot be read.
    /// 构建仅在无法读取父关联元数据时使用的严格回退投影。
    fn fallback() -> Self {
        Self {
            active_segment: "s001.jsonl".to_string(),
            line_count: 0,
            agent_kind: "main_clone".to_string(),
            agent_profile: json!({
                "agent_id": null,
                "agent_name": null,
                "agent_md_path": null,
                "prompt_source": "main_clone",
                "inherit_parent_config": true
            }),
        }
    }
}

/// Reads the child task metadata projection used by parent-link payloads.
/// 读取父会话关联载荷所需的子任务元数据投影。
fn main_session_task_metadata_projection(
    root_session_dir: &Path,
    task_dir: &str,
) -> Result<TaskMetadataProjection, SessionStoreError> {
    let metadata = read_required_task_metadata_document(
        &root_session_dir
            .join(task_dir)
            .join(SEGMENTED_METADATA_FILE),
    )?;
    Ok(TaskMetadataProjection {
        active_segment: metadata.active_segment,
        line_count: metadata
            .segments
            .iter()
            .map(|segment| segment.line_count)
            .sum(),
        agent_kind: metadata.agent_kind,
        agent_profile: metadata.agent_profile,
    })
}

/// Refreshes the parent metadata task mirror for one task invocation event.
/// 根据一条子任务调用事件刷新父元数据中的任务镜像。
fn refresh_main_session_task_mirror(
    metadata: &mut Value,
    event: &TaskInvocationEvent,
    task_active_segment: &str,
    line_count: u64,
    agent_kind: &str,
    agent_profile: &Value,
) {
    if !metadata.get("tasks").is_some_and(Value::is_array) {
        metadata["tasks"] = json!([]);
    }
    let Some(tasks) = metadata.get_mut("tasks").and_then(Value::as_array_mut) else {
        return;
    };
    let task_session_id = event.task_session_id.to_string();
    let task_dir = format!(
        "task/{}",
        sanitize_file_component(&event.task_session_id.to_string())
    );
    let task_metadata_file = format!("{task_dir}/{SEGMENTED_METADATA_FILE}");
    let existing_index = tasks.iter().position(|task| {
        task.get("session_id")
            .and_then(Value::as_str)
            .is_some_and(|value| value == task_session_id)
    });
    let payload = event.payload.as_object();
    let display_title = payload
        .and_then(|payload| payload.get("display_title"))
        .and_then(Value::as_str)
        .map(str::to_string);
    let description = payload
        .and_then(|payload| payload.get("description"))
        .cloned()
        .unwrap_or(Value::Null);
    let subagent = payload
        .and_then(|payload| payload.get("subagent"))
        .cloned()
        .unwrap_or(Value::Null);
    let result = payload
        .and_then(|payload| payload.get("result"))
        .cloned()
        .unwrap_or(Value::Null);
    let error = payload
        .and_then(|payload| payload.get("error"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut mirror = existing_index
        .and_then(|index| tasks.get(index).cloned())
        .unwrap_or_else(|| {
            json!({
                "session_id": task_session_id,
                "dir": task_dir,
                "metadata_file": task_metadata_file,
                "active_segment": task_active_segment,
                "created_at": event.timestamp_ms.to_string()
            })
        });
    mirror["session_id"] = Value::String(event.task_session_id.to_string());
    mirror["dir"] = Value::String(task_dir);
    mirror["metadata_file"] = Value::String(task_metadata_file);
    mirror["active_segment"] = Value::String(task_active_segment.to_string());
    mirror["status"] = serde_json::to_value(event.status).unwrap_or(Value::Null);
    mirror["agent_kind"] = Value::String(agent_kind.to_string());
    mirror["agent_profile"] = agent_profile.clone();
    mirror["line_count"] = Value::from(line_count);
    mirror["updated_at"] = Value::String(event.timestamp_ms.to_string());
    if let Some(display_title) = display_title {
        mirror["display_title"] = Value::String(display_title);
    }
    mirror["description"] = description;
    mirror["subagent"] = subagent;
    mirror["last_result_summary"] = result;
    mirror["last_error"] = error;
    match existing_index {
        Some(index) => tasks[index] = mirror,
        None => tasks.push(mirror),
    }
}

/// Reads task invocations from a main segmented session stream.
/// 从主分段会话流读取子任务调用事件。
fn read_task_invocations_from_main_segments(
    root_session_dir: &Path,
    parent: &ParentSessionRef,
) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
    let metadata_path = root_session_dir.join(SEGMENTED_METADATA_FILE);
    if !metadata_path.exists() {
        return Ok(Vec::new());
    }
    let metadata = read_required_json::<Value>(&metadata_path)?;
    let segments = serde_json::from_value::<Vec<SegmentMetadata>>(
        metadata
            .get("segments")
            .cloned()
            .unwrap_or_else(|| json!([])),
    )
    .map_err(to_store_error)?;
    read_task_invocations_from_lines(read_segment_lines(root_session_dir, &segments)?, parent)
}

/// Reads task invocations from a task segmented stream.
/// 从子任务分段流读取子任务调用事件。
fn read_task_invocations_from_task_segments(
    task_dir: &Path,
    parent: &ParentSessionRef,
) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
    let metadata = read_required_task_metadata_document(&task_dir.join(SEGMENTED_METADATA_FILE))?;
    read_task_invocations_from_lines(read_segment_lines(task_dir, &metadata.segments)?, parent)
}

/// Extracts task invocation events from segment lines.
/// 从分段事件行中提取子任务调用事件。
fn read_task_invocations_from_lines(
    lines: Vec<SegmentedEventLine>,
    parent: &ParentSessionRef,
) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
    let mut events = Vec::new();
    for line in lines {
        let Some(value) = line.payload.get("task_invocation_event").cloned() else {
            continue;
        };
        let event = serde_json::from_value::<TaskInvocationEvent>(value).map_err(to_store_error)?;
        if &event.parent == parent {
            events.push(event);
        }
    }
    events.sort_by_key(|event| event.sequence);
    Ok(events)
}

/// Reads every JSONL segment line described by metadata.
/// 读取元数据描述的全部 JSONL 分段行。
fn read_segment_lines(
    dir: &Path,
    segments: &[SegmentMetadata],
) -> Result<Vec<SegmentedEventLine>, SessionStoreError> {
    let mut lines = Vec::new();
    for segment in segments {
        let path = dir.join(&segment.file);
        if !path.exists() {
            continue;
        }
        lines.extend(read_json_lines::<SegmentedEventLine>(&path)?);
    }
    lines.sort_by_key(|line| line.seq);
    Ok(lines)
}

/// Reads a task state document from a task directory.
/// 从子任务目录读取状态文档。
fn read_task_state(
    task_dir: &Path,
) -> Result<Option<SegmentedTaskStateDocument>, SessionStoreError> {
    read_json_file(&task_dir.join(SEGMENTED_STATE_FILE))
}

/// Builds an agent kind label for task metadata.
/// 为子任务元数据构建代理类型标签。
fn segmented_agent_kind(metadata: &TaskSessionMetadata) -> String {
    if metadata.subagent_profile_id.is_some() {
        "agent_md".to_string()
    } else if metadata.inherit_parent_config {
        "main_clone".to_string()
    } else {
        "custom".to_string()
    }
}

/// Builds an agent profile audit payload for task metadata.
/// 为子任务元数据构建代理配置审计载荷。
fn segmented_agent_profile(metadata: &TaskSessionMetadata) -> Value {
    json!({
        "agent_id": metadata.subagent_profile_id,
        "agent_name": metadata.display_title,
        "agent_md_path": metadata.effective_config.subagent_profile_id,
        "prompt_source": segmented_agent_kind(metadata),
        "inherit_parent_config": metadata.inherit_parent_config
    })
}

/// Converts a shared session event kind into a stable segmented kind label.
/// 将共享会话事件类型转换为稳定的分段类型标签。
fn session_event_kind_string(kind: &vulcan_session_core::SessionEventKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(ToOwned::to_owned))
        .unwrap_or_else(|| "runtime_marker".to_string())
}

/// Returns a stable segment file name for one-based segment indexes.
/// 根据从一开始的分段索引返回稳定分段文件名。
fn segment_file_name(index: usize) -> String {
    format!("s{index:03}.jsonl")
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn current_unix_millis_u64() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

/// Sanitizes one filesystem path component.
/// 清理单个文件系统路径片段。
fn sanitize_file_component(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "_".to_string()
    } else {
        sanitized
    }
}

/// Creates a directory tree.
/// 创建目录树。
fn create_dir_all(path: &Path) -> Result<(), SessionStoreError> {
    fs::create_dir_all(path).map_err(to_store_error)
}

/// Appends one JSON object as a JSONL line and returns the bytes written.
/// 将一个 JSON 对象作为 JSONL 行追加并返回写入字节数。
fn append_json_line<T: Serialize>(path: &Path, value: &T) -> Result<u64, SessionStoreError> {
    let encoded = serde_json::to_string(value).map_err(to_store_error)?;
    append_encoded_json_line(path, &encoded)
}

/// Appends one pre-encoded JSONL line and returns the bytes written.
/// 追加一条已编码 JSONL 行并返回写入字节数。
fn append_encoded_json_line(path: &Path, encoded: &str) -> Result<u64, SessionStoreError> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(to_store_error)?;
    file.write_all(encoded.as_bytes()).map_err(to_store_error)?;
    file.write_all(b"\n").map_err(to_store_error)?;
    Ok(encoded.as_bytes().len() as u64 + 1)
}

/// Reads one optional JSON document.
/// 读取一个可选 JSON 文档。
fn read_json_file<T: for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Option<T>, SessionStoreError> {
    if !path.exists() {
        return Ok(None);
    }
    let file = File::open(path).map_err(to_store_error)?;
    serde_json::from_reader(file)
        .map(Some)
        .map_err(to_store_error)
}

/// Reads one required JSON document.
/// 读取一个必需 JSON 文档。
fn read_required_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, SessionStoreError> {
    read_json_file(path)?.ok_or_else(|| {
        SessionStoreError::new(format!(
            "Required JSON file does not exist: {}",
            path.display()
        ))
    })
}

/// Writes one JSON document atomically enough for local file updates.
/// 以足够本地文件更新使用的原子方式写入一个 JSON 文档。
fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<(), SessionStoreError> {
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("json.tmp");
    {
        let file = File::create(&temp_path).map_err(to_store_error)?;
        serde_json::to_writer_pretty(file, value).map_err(to_store_error)?;
    }
    fs::rename(&temp_path, path).map_err(to_store_error)
}

/// Reads JSONL records from one path.
/// 从指定路径读取 JSONL 记录。
fn read_json_lines<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>, SessionStoreError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut values = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(to_store_error)?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str(&line).map_err(to_store_error)?);
    }
    Ok(values)
}

/// Converts any displayable error into a session store error.
/// 将任意可显示错误转换为会话存储错误。
fn to_store_error(error: impl std::fmt::Display) -> SessionStoreError {
    SessionStoreError::new(error.to_string())
}

#[cfg(test)]
mod tests {
    //! Tests for the segmented TaskSession store layout.
    //! 分段子任务会话存储布局测试。

    use super::*;
    use serde_json::json;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_session_core::{SessionEffectiveConfig, SessionEventKind, SessionRequestedConfig};

    /// Builds a unique temporary root directory for segmented store tests.
    /// 为分段存储测试构建唯一临时根目录。
    fn test_root(test_name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir()
            .join("vulcan-segmented-task-session-store-tests")
            .join(format!("{test_name}-{suffix}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    /// Creates the minimal root main-session metadata needed for parent link events.
    /// 创建父级关联事件所需的最小根主会话元数据。
    fn create_root_session_metadata(root: &Path, root_session_id: SessionNodeId) {
        let metadata = json!({
            "schema_version": 1,
            "session_id": root_session_id.to_string(),
            "project_tag": "project-alpha",
            "created_at": "100",
            "updated_at": "100",
            "project_id": "project-alpha",
            "title": "Root",
            "title_source": "manual",
            "canonical_source": "segmented_test",
            "archived_at": null,
            "active_segment": "s001.jsonl",
            "task_dir": "task",
            "next_global_seq": 1,
            "latest_rollback_seq": null,
            "segments": [
                {
                    "file": "s001.jsonl",
                    "start_seq": 0,
                    "line_count": 1,
                    "byte_len": 0,
                    "first_timestamp": "100",
                    "last_timestamp": "100",
                    "first_turn_index": null,
                    "last_turn_index": null,
                    "last_compaction_seq": null
                }
            ],
            "segment_policy": {
                "max_lines": 3000,
                "max_bytes": 8388608
            },
            "tasks": []
        });
        write_json_file(&root.join(SEGMENTED_METADATA_FILE), &metadata)
            .expect("write root metadata");
        append_json_line(
            &root.join("s001.jsonl"),
            &SegmentedEventLine {
                seq: 0,
                timestamp: "100".to_string(),
                kind: "session_meta".to_string(),
                payload: json!({ "session_id": root_session_id }),
                turn_index: None,
            },
        )
        .expect("write root segment");
    }

    /// Verifies the segmented store writes task metadata, state, segments, and parent links without old tasks layout files.
    /// 验证分段存储会写入子任务元数据、状态、分段和父关联，同时不产生旧 tasks 布局文件。
    #[test]
    fn segmented_store_round_trips_task_session_artifacts() {
        let root = test_root("segmented_store_round_trips_task_session_artifacts");
        let store = SegmentedTaskSessionStore::new(&root);
        let root_session_id = SessionNodeId::new_v4();
        let task_session_id = SessionNodeId::new_v4();
        create_root_session_metadata(&root, root_session_id);
        let parent = ParentSessionRef::MainSession { root_session_id };
        let mut effective_config = SessionEffectiveConfig::default();
        effective_config.subagent_profile_id = Some("reviewer".to_string());
        let mut metadata = TaskSessionMetadata::new(
            root_session_id,
            task_session_id,
            parent.clone(),
            "review-task",
            "Review task",
            SessionRequestedConfig::default(),
            effective_config,
            1,
            200,
        );
        metadata.description = Some("Inspect persistence".to_string());

        store
            .create_task_session(metadata.clone())
            .expect("create segmented task");
        metadata.status = TaskSessionStatus::Running;
        metadata.updated_at = 250;
        store
            .save_task_session(metadata.clone())
            .expect("save segmented metadata");
        store
            .append_event(SessionEventRecord::new(
                "event-1",
                task_session_id,
                0,
                SessionEventKind::AssistantOutput,
                260,
                json!({ "body": "done" }),
            ))
            .expect("append task event");
        store
            .save_context(SessionContextDocument {
                schema_version: 1,
                node_id: task_session_id,
                provider_id: Some("provider".to_string()),
                model_id: Some("model".to_string()),
                reasoning_effort: None,
                system_prompt_layers: vec!["system".to_string()],
                conversation_state: json!({ "messages": [] }),
                compaction_anchor: None,
                context_budget_fingerprint: None,
                tool_catalog_snapshot: Value::Null,
                provider_request_preview: None,
            })
            .expect("save context");
        store
            .save_runtime_checkpoint(SessionRuntimeCheckpoint {
                node_id: task_session_id,
                status: "running".to_string(),
                last_sequence: 1,
                payload: json!({ "ok": true }),
            })
            .expect("save checkpoint");

        let task_dir = root
            .join(SEGMENTED_TASK_DIR)
            .join(task_session_id.to_string());
        assert!(task_dir.join(SEGMENTED_METADATA_FILE).exists());
        assert!(task_dir.join(SEGMENTED_STATE_FILE).exists());
        assert!(task_dir.join("s001.jsonl").exists());
        assert!(!root.join("tasks").exists());
        let loaded = store
            .load_task_session(task_session_id)
            .expect("load segmented task")
            .expect("task exists");
        assert_eq!(loaded.status, TaskSessionStatus::Running);
        assert_eq!(
            store
                .load_context(task_session_id)
                .expect("load context")
                .and_then(|context| context.provider_id),
            Some("provider".to_string())
        );
        assert_eq!(
            store
                .load_events(task_session_id)
                .expect("load events")
                .len(),
            1
        );
        assert_eq!(
            store
                .list_child_task_session_ids(parent.clone())
                .expect("list child ids"),
            vec![task_session_id]
        );
        assert_eq!(
            store
                .load_latest_event_payload(task_session_id)
                .expect("load latest payload"),
            Some(json!({ "body": "done" }))
        );
        let root_metadata = read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE))
            .expect("root metadata");
        assert_eq!(root_metadata["next_global_seq"], 2);
        let task_metadata = read_required_json::<Value>(&task_dir.join(SEGMENTED_METADATA_FILE))
            .expect("task metadata");
        assert_eq!(task_metadata["agent_kind"], "agent_md");
        assert_eq!(task_metadata["agent_profile"]["prompt_source"], "agent_md");
        let task_segment_lines =
            read_json_lines::<Value>(&task_dir.join("s001.jsonl")).expect("read task segment");
        assert_eq!(task_segment_lines[0]["type"], "subagent_task_meta");
        assert!(task_segment_lines[0].get("kind").is_none());
        let task_mirror = root_metadata["tasks"]
            .as_array()
            .and_then(|tasks| tasks.first())
            .expect("task mirror");
        assert_eq!(task_mirror["session_id"], task_session_id.to_string());
        assert_eq!(task_mirror["dir"], format!("task/{}", task_session_id));
        assert_eq!(
            task_mirror["metadata_file"],
            format!("task/{}/metadata.json", task_session_id)
        );
        assert_eq!(task_mirror["display_title"], "Review task");
        assert_eq!(task_mirror["description"], "Inspect persistence");
        assert_eq!(task_mirror["subagent"]["profile_id"], "reviewer");
        assert_eq!(task_mirror["agent_kind"], "agent_md");
        assert_eq!(task_mirror["agent_profile"]["prompt_source"], "agent_md");
        assert_eq!(task_mirror["line_count"], 1);
        assert_eq!(
            task_mirror["active_segment"],
            format!("task/{}/s001.jsonl", task_session_id)
        );
        assert_eq!(
            store
                .list_task_invocation_events(parent)
                .expect("load parent links")
                .len(),
            1
        );
        let root_segment_lines =
            read_json_lines::<Value>(&root.join("s001.jsonl")).expect("read root raw segment");
        assert_eq!(root_segment_lines[0]["type"], "session_meta");
        assert!(root_segment_lines[0].get("kind").is_none());
        assert!(root_segment_lines[1].get("type").is_some());
        assert!(root_segment_lines[1].get("kind").is_none());
        let root_lines = read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl"))
            .expect("read root segment");
        assert_eq!(root_lines[1].seq, 1);
        assert_eq!(
            root_lines[1].payload["task_dir"],
            format!("task/{}", task_session_id)
        );
        assert_eq!(
            root_lines[1].payload["task_active_segment"],
            format!("task/{}/s001.jsonl", task_session_id)
        );
        assert_eq!(
            root_lines[1].payload["task_metadata_file"],
            format!("task/{}/metadata.json", task_session_id)
        );
        assert_eq!(root_lines[1].payload["agent_kind"], "agent_md");
        assert_eq!(
            root_lines[1].payload["agent_profile"]["prompt_source"],
            "agent_md"
        );
        assert_eq!(root_lines[1].payload["line_count"], 1);
    }

    /// Verifies task metadata writes every required agent field without relying on read repair.
    /// 验证子任务元数据会直接写出所有必需代理字段，而不依赖读取修复。
    #[test]
    fn task_metadata_write_includes_required_agent_fields() {
        let root = test_root("task_metadata_write_includes_required_agent_fields");
        let store = SegmentedTaskSessionStore::new(&root);
        let root_session_id = SessionNodeId::new_v4();
        let task_session_id = SessionNodeId::new_v4();
        create_root_session_metadata(&root, root_session_id);
        let parent = ParentSessionRef::MainSession { root_session_id };
        let mut effective_config = SessionEffectiveConfig::default();
        effective_config.subagent_profile_id = Some("reviewer".to_string());
        let metadata = TaskSessionMetadata::new(
            root_session_id,
            task_session_id,
            parent,
            "review-task",
            "Review task",
            SessionRequestedConfig::default(),
            effective_config,
            1,
            200,
        );
        store.create_task_session(metadata).expect("create task");

        let task_dir = root
            .join(SEGMENTED_TASK_DIR)
            .join(task_session_id.to_string());
        let task_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
        let written_metadata =
            read_required_json::<Value>(&task_metadata_path).expect("read written metadata");
        assert_eq!(written_metadata["agent_kind"], "agent_md");
        assert_eq!(
            written_metadata["agent_profile"]["prompt_source"],
            "agent_md"
        );
        assert!(written_metadata.get("task_session").is_some());
        assert!(written_metadata.get("segments").is_some());
        assert!(written_metadata.get("segment_policy").is_some());
    }

    /// Verifies nested task sessions stay under the root task directory while parent metadata preserves hierarchy.
    /// 验证嵌套子代理会话仍位于根 task 目录下，并通过父级元数据保留层级关系。
    #[test]
    fn nested_task_session_uses_flat_root_task_directory() {
        let root = test_root("nested_task_session_uses_flat_root_task_directory");
        let store = SegmentedTaskSessionStore::new(&root);
        let root_session_id = SessionNodeId::new_v4();
        let parent_task_id = SessionNodeId::new_v4();
        let child_task_id = SessionNodeId::new_v4();
        create_root_session_metadata(&root, root_session_id);

        let parent_ref = ParentSessionRef::MainSession { root_session_id };
        store
            .create_task_session(TaskSessionMetadata::new(
                root_session_id,
                parent_task_id,
                parent_ref,
                "parent-task",
                "Parent task",
                SessionRequestedConfig::default(),
                SessionEffectiveConfig::default(),
                1,
                200,
            ))
            .expect("create parent task");

        let child_parent = ParentSessionRef::TaskSession {
            root_session_id,
            parent_task_session_id: parent_task_id,
        };
        store
            .create_task_session(TaskSessionMetadata::new(
                root_session_id,
                child_task_id,
                child_parent.clone(),
                "child-task",
                "Child task",
                SessionRequestedConfig::default(),
                SessionEffectiveConfig::default(),
                2,
                220,
            ))
            .expect("create child task");

        let flat_child_dir = root
            .join(SEGMENTED_TASK_DIR)
            .join(child_task_id.to_string());
        let nested_child_dir = root
            .join(SEGMENTED_TASK_DIR)
            .join(parent_task_id.to_string())
            .join(SEGMENTED_TASK_DIR)
            .join(child_task_id.to_string());
        assert!(flat_child_dir.join(SEGMENTED_METADATA_FILE).exists());
        assert!(!nested_child_dir.exists());
        assert_eq!(
            store
                .list_child_task_session_ids(child_parent)
                .expect("list nested child ids"),
            vec![child_task_id]
        );
        assert!(
            store
                .load_task_session(child_task_id)
                .expect("load child task")
                .is_some()
        );

        let root_lines =
            read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl")).expect("read root");
        assert!(
            root_lines.iter().any(|line| {
                line.payload
                    .get("task_session_id")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value == child_task_id.to_string())
            }),
            "root session JSONL must link to nested task sessions"
        );
        let root_metadata = read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE))
            .expect("root metadata");
        assert!(
            root_metadata["tasks"].as_array().is_some_and(|tasks| {
                tasks.iter().any(|task| {
                    task.get("session_id")
                        .and_then(Value::as_str)
                        .is_some_and(|value| value == child_task_id.to_string())
                })
            }),
            "root metadata task mirror must include nested task sessions"
        );
    }

    /// Verifies parent task links point at the child task's current active segment after rollover.
    /// 验证子任务滚动分段后父级任务关联会指向子任务当前活动分段。
    #[test]
    fn parent_task_link_uses_current_child_active_segment_after_rollover() {
        let root = test_root("parent_task_link_uses_current_child_active_segment_after_rollover");
        let store = SegmentedTaskSessionStore::new(&root);
        let root_session_id = SessionNodeId::new_v4();
        let task_session_id = SessionNodeId::new_v4();
        create_root_session_metadata(&root, root_session_id);
        let parent = ParentSessionRef::MainSession { root_session_id };
        let metadata = TaskSessionMetadata::new(
            root_session_id,
            task_session_id,
            parent.clone(),
            "long-task",
            "Long task",
            SessionRequestedConfig::default(),
            SessionEffectiveConfig::default(),
            1,
            200,
        );

        store
            .create_task_session(metadata)
            .expect("create segmented task");
        let task_dir = root
            .join(SEGMENTED_TASK_DIR)
            .join(task_session_id.to_string());
        let task_metadata_path = task_dir.join(SEGMENTED_METADATA_FILE);
        let mut task_metadata =
            read_required_json::<SegmentedTaskMetadataDocument>(&task_metadata_path)
                .expect("task metadata");
        task_metadata.segment_policy = SegmentPolicy {
            max_lines: 2,
            max_bytes: DEFAULT_SEGMENT_MAX_BYTES,
        };
        write_json_file(&task_metadata_path, &task_metadata).expect("write task policy");

        store
            .append_event(SessionEventRecord::new(
                "event-1",
                task_session_id,
                0,
                SessionEventKind::AssistantOutput,
                260,
                json!({ "body": "first" }),
            ))
            .expect("append first task event");
        store
            .append_event(SessionEventRecord::new(
                "event-2",
                task_session_id,
                1,
                SessionEventKind::AssistantOutput,
                270,
                json!({ "body": "second" }),
            ))
            .expect("append second task event");
        let task_metadata =
            read_required_json::<SegmentedTaskMetadataDocument>(&task_metadata_path)
                .expect("task metadata after rollover");
        assert_eq!(task_metadata.active_segment, "s002.jsonl");
        assert!(task_dir.join("s002.jsonl").exists());

        store
            .append_task_invocation_event(
                parent.clone(),
                TaskInvocationEvent {
                    event_id: "task-completed".to_string(),
                    root_session_id,
                    parent: parent.clone(),
                    task_session_id,
                    sequence: 2,
                    kind: TaskInvocationEventKind::TaskCompleted,
                    status: TaskSessionStatus::Completed,
                    prompt: None,
                    timestamp_ms: 300,
                    payload: json!({ "result": "done" }),
                },
            )
            .expect("append parent completion link");

        let root_lines = read_json_lines::<SegmentedEventLine>(&root.join("s001.jsonl"))
            .expect("read root segment");
        let last_root_line = root_lines.last().expect("parent link event");
        assert_eq!(
            last_root_line.payload["task_active_segment"],
            format!("task/{}/s002.jsonl", task_session_id)
        );
        assert_eq!(last_root_line.payload["line_count"], 3);
        let root_metadata = read_required_json::<Value>(&root.join(SEGMENTED_METADATA_FILE))
            .expect("root metadata");
        let task_mirror = root_metadata["tasks"]
            .as_array()
            .and_then(|tasks| tasks.first())
            .expect("task mirror");
        assert_eq!(
            task_mirror["active_segment"],
            format!("task/{}/s002.jsonl", task_session_id)
        );
        assert_eq!(task_mirror["line_count"], 3);
        assert_eq!(task_mirror["agent_kind"], "main_clone");
        assert!(task_mirror["agent_profile"].is_object());
    }
}
