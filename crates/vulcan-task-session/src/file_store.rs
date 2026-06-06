use crate::compaction::TaskSessionCompactionStore;
use crate::model::{
    TaskInvocationEvent, TaskSessionId, TaskSessionMetadata, TaskSessionStatus, TaskSessionSummary,
};
use crate::store::TaskSessionStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use vulcan_session_core::{
    ParentSessionRef, SessionCompactionCheckpoint, SessionContextDocument, SessionEventRecord,
    SessionNodeId, SessionNodeStore, SessionRuntimeCheckpoint, SessionStoreError,
};

/// File-backed TaskSession store rooted at one main-session directory.
/// 以单个主会话目录为根的文件态 TaskSession 存储。
#[derive(Debug, Clone)]
pub struct FileTaskSessionStore {
    /// Root main-session directory that owns the tasks domain.
    /// 拥有 tasks 文件域的根主会话目录。
    root_session_dir: PathBuf,
}

impl FileTaskSessionStore {
    /// Creates a file-backed store rooted at one main-session directory.
    /// 创建以单个主会话目录为根的文件态存储。
    pub fn new(root_session_dir: impl Into<PathBuf>) -> Self {
        Self {
            root_session_dir: root_session_dir.into(),
        }
    }

    /// Returns the root tasks directory for the main session.
    /// 返回主会话的根 tasks 目录。
    fn tasks_root_dir(&self) -> PathBuf {
        self.root_session_dir.join("tasks")
    }

    /// Returns the task-session index file path.
    /// 返回子代理会话索引文件路径。
    fn index_path(&self) -> PathBuf {
        self.tasks_root_dir().join("task-index.json")
    }

    /// Loads the task-session file index.
    /// 加载子代理会话文件索引。
    fn load_index(&self) -> Result<TaskSessionFileIndex, SessionStoreError> {
        read_json_file(&self.index_path()).map(|value| value.unwrap_or_default())
    }

    /// Saves the task-session file index.
    /// 保存子代理会话文件索引。
    fn save_index(&self, index: &TaskSessionFileIndex) -> Result<(), SessionStoreError> {
        write_json_file(&self.index_path(), index)
    }

    /// Returns the directory for one task session from the index.
    /// 从索引返回某个子代理会话目录。
    fn indexed_task_dir(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<PathBuf, SessionStoreError> {
        let index = self.load_index()?;
        let key = task_session_id.to_string();
        let entry = index.entries.get(&key).ok_or_else(|| {
            SessionStoreError::new(format!("TaskSession `{task_session_id}` is not indexed."))
        })?;
        Ok(self.root_session_dir.join(&entry.relative_dir))
    }

    /// Returns the task log path for one parent session reference.
    /// 返回某个父级会话引用的任务日志路径。
    fn task_log_path_for_parent(
        &self,
        parent: &ParentSessionRef,
    ) -> Result<PathBuf, SessionStoreError> {
        match parent {
            ParentSessionRef::None => Err(SessionStoreError::new(
                "A root session cannot own a task invocation log through TaskSessionStore.",
            )),
            ParentSessionRef::MainSession { .. } => Ok(self.tasks_root_dir().join("task.jsonl")),
            ParentSessionRef::TaskSession {
                parent_task_session_id,
                ..
            } => Ok(self
                .indexed_task_dir(*parent_task_session_id)?
                .join("tasks")
                .join("task.jsonl")),
        }
    }

    /// Returns the relative directory for a newly created task session.
    /// 返回新建子代理会话的相对目录。
    fn relative_dir_for_new_task(
        &self,
        metadata: &TaskSessionMetadata,
        index: &TaskSessionFileIndex,
    ) -> Result<PathBuf, SessionStoreError> {
        let task_dir_name = metadata.task_session_id.to_string();
        match &metadata.parent {
            ParentSessionRef::None => Err(SessionStoreError::new(
                "TaskSession metadata must have a main-session or task-session parent.",
            )),
            ParentSessionRef::MainSession { .. } => Ok(PathBuf::from("tasks").join(task_dir_name)),
            ParentSessionRef::TaskSession {
                parent_task_session_id,
                ..
            } => {
                let parent_key = parent_task_session_id.to_string();
                let parent_entry = index.entries.get(&parent_key).ok_or_else(|| {
                    SessionStoreError::new(format!(
                        "Parent TaskSession `{parent_task_session_id}` is not indexed."
                    ))
                })?;
                Ok(PathBuf::from(&parent_entry.relative_dir)
                    .join("tasks")
                    .join(task_dir_name))
            }
        }
    }

    /// Creates the standard directory layout for one task session.
    /// 为一个子代理会话创建标准目录布局。
    fn create_task_layout(task_dir: &Path) -> Result<(), SessionStoreError> {
        create_dir_all(task_dir)?;
        create_dir_all(&task_dir.join("context"))?;
        create_dir_all(&task_dir.join("context").join("snapshots"))?;
        create_dir_all(&task_dir.join("checkpoints"))?;
        create_dir_all(&task_dir.join("tasks"))?;
        Ok(())
    }

    /// Lists all indexed task-session summaries.
    /// 列出全部已索引的子代理会话摘要。
    pub fn list_indexed_task_sessions(&self) -> Result<Vec<TaskSessionSummary>, SessionStoreError> {
        let index = self.load_index()?;
        Ok(index
            .entries
            .values()
            .filter_map(TaskSessionFileIndexEntry::to_summary)
            .collect())
    }

    /// Lists active indexed task-session summaries.
    /// 列出已索引的活动子代理会话摘要。
    pub fn list_active_indexed_task_sessions(
        &self,
    ) -> Result<Vec<TaskSessionSummary>, SessionStoreError> {
        Ok(self
            .list_indexed_task_sessions()?
            .into_iter()
            .filter(|summary| summary.status.is_active())
            .collect())
    }
}

impl SessionNodeStore for FileTaskSessionStore {
    /// Appends one event record to a task-session events.jsonl file.
    /// 向子代理会话 events.jsonl 文件追加一条事件记录。
    fn append_event(&self, record: SessionEventRecord) -> Result<(), SessionStoreError> {
        let event_path = self.indexed_task_dir(record.node_id)?.join("events.jsonl");
        append_json_line(&event_path, &record)
    }

    /// Loads event records for one task session.
    /// 加载一个子代理会话的事件记录。
    fn load_events(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Vec<SessionEventRecord>, SessionStoreError> {
        let event_path = self.indexed_task_dir(node_id)?.join("events.jsonl");
        read_json_lines(&event_path)
    }

    /// Saves the current API-facing context document for one task session.
    /// 保存一个子代理会话当前 API 实际上下文文档。
    fn save_context(&self, context: SessionContextDocument) -> Result<(), SessionStoreError> {
        let context_path = self
            .indexed_task_dir(context.node_id)?
            .join("context")
            .join("current.json");
        write_json_file(&context_path, &context)
    }

    /// Loads the current API-facing context document for one task session.
    /// 加载一个子代理会话当前 API 实际上下文文档。
    fn load_context(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Option<SessionContextDocument>, SessionStoreError> {
        let context_path = self
            .indexed_task_dir(node_id)?
            .join("context")
            .join("current.json");
        read_json_file(&context_path)
    }

    /// Saves a lightweight runtime checkpoint for one task session.
    /// 保存一个子代理会话的轻量运行时检查点。
    fn save_runtime_checkpoint(
        &self,
        checkpoint: SessionRuntimeCheckpoint,
    ) -> Result<(), SessionStoreError> {
        let checkpoint_path = self
            .indexed_task_dir(checkpoint.node_id)?
            .join("runtime_state.json");
        write_json_file(&checkpoint_path, &checkpoint)
    }

    /// Loads a lightweight runtime checkpoint for one task session.
    /// 加载一个子代理会话的轻量运行时检查点。
    fn load_runtime_checkpoint(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Option<SessionRuntimeCheckpoint>, SessionStoreError> {
        let checkpoint_path = self.indexed_task_dir(node_id)?.join("runtime_state.json");
        read_json_file(&checkpoint_path)
    }
}

impl TaskSessionStore for FileTaskSessionStore {
    /// Creates a task-session directory and writes its metadata.
    /// 创建子代理会话目录并写入元数据。
    fn create_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError> {
        create_dir_all(&self.tasks_root_dir())?;
        let mut index = self.load_index()?;
        let key = metadata.task_session_id.to_string();
        if index.entries.contains_key(&key) {
            return Err(SessionStoreError::new(format!(
                "TaskSession `{}` already exists.",
                metadata.task_session_id
            )));
        }

        let relative_dir = self.relative_dir_for_new_task(&metadata, &index)?;
        let task_dir = self.root_session_dir.join(&relative_dir);
        Self::create_task_layout(&task_dir)?;
        write_json_file(&task_dir.join("task-session.json"), &metadata)?;
        index.entries.insert(
            key,
            TaskSessionFileIndexEntry::from_metadata(&metadata, relative_dir),
        );
        self.save_index(&index)?;

        let created_event = TaskInvocationEvent {
            event_id: format!("task-created-{}", metadata.task_session_id),
            root_session_id: metadata.root_session_id,
            parent: metadata.parent.clone(),
            task_session_id: metadata.task_session_id,
            sequence: 0,
            kind: crate::model::TaskInvocationEventKind::TaskCreated,
            status: metadata.status,
            prompt: None,
            timestamp_ms: metadata.created_at,
            payload: Value::Null,
        };
        self.append_task_invocation_event(metadata.parent, created_event)
    }

    /// Saves task-session metadata.
    /// 保存子代理会话元数据。
    fn save_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError> {
        let task_dir = self.indexed_task_dir(metadata.task_session_id)?;
        write_json_file(&task_dir.join("task-session.json"), &metadata)?;
        let mut index = self.load_index()?;
        let key = metadata.task_session_id.to_string();
        let Some(entry) = index.entries.get_mut(&key) else {
            return Err(SessionStoreError::new(format!(
                "TaskSession `{}` is not indexed.",
                metadata.task_session_id
            )));
        };
        entry.status = metadata.status;
        entry.public_task_id = metadata.public_task_id;
        entry.display_title = metadata.display_title;
        entry.updated_at = metadata.updated_at;
        self.save_index(&index)
    }

    /// Loads task-session metadata by identifier.
    /// 按标识符加载子代理会话元数据。
    fn load_task_session(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Option<TaskSessionMetadata>, SessionStoreError> {
        let task_dir = match self.indexed_task_dir(task_session_id) {
            Ok(task_dir) => task_dir,
            Err(error) if error.message.contains("is not indexed") => return Ok(None),
            Err(error) => return Err(error),
        };
        read_json_file(&task_dir.join("task-session.json"))
    }

    /// Finds one task session by direct parent and caller-facing task id.
    /// 按直接父级与调用方面向任务标识查找一个子代理会话。
    fn find_task_session_by_parent_and_public_id(
        &self,
        parent: ParentSessionRef,
        public_task_id: &str,
    ) -> Result<Option<TaskSessionMetadata>, SessionStoreError> {
        let normalized_public_task_id = public_task_id.trim();
        if normalized_public_task_id.is_empty() {
            return Ok(None);
        }
        let index = self.load_index()?;
        let matching_task_id = index
            .entries
            .values()
            .find(|entry| {
                entry.parent == parent && entry.public_task_id == normalized_public_task_id
            })
            .map(|entry| entry.task_session_id);
        matching_task_id
            .map(|task_session_id| self.load_task_session(task_session_id))
            .transpose()
            .map(Option::flatten)
    }

    /// Appends a task invocation event to the direct parent task log.
    /// 向直接父级任务日志追加一条子代理调用事件。
    fn append_task_invocation_event(
        &self,
        parent: ParentSessionRef,
        event: TaskInvocationEvent,
    ) -> Result<(), SessionStoreError> {
        let task_log_path = self.task_log_path_for_parent(&parent)?;
        append_json_line(&task_log_path, &event)
    }

    /// Lists task invocation events for one direct parent.
    /// 列出某个直接父级的子代理调用事件。
    fn list_task_invocation_events(
        &self,
        parent: ParentSessionRef,
    ) -> Result<Vec<TaskInvocationEvent>, SessionStoreError> {
        let task_log_path = self.task_log_path_for_parent(&parent)?;
        read_json_lines(&task_log_path)
    }

    /// Saves one compaction checkpoint in the task-session file domain.
    /// 在子代理会话文件域中保存一个压缩检查点。
    fn save_task_compaction_checkpoint(
        &self,
        checkpoint: SessionCompactionCheckpoint,
    ) -> Result<(), SessionStoreError> {
        let checkpoint_path = self
            .indexed_task_dir(checkpoint.node_id)?
            .join("checkpoints")
            .join(format!("{}.json", checkpoint.checkpoint_id));
        write_json_file(&checkpoint_path, &checkpoint)
    }

    /// Loads compaction checkpoints for one task session.
    /// 加载某个子代理会话的压缩检查点。
    fn load_task_compaction_checkpoints(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Vec<SessionCompactionCheckpoint>, SessionStoreError> {
        let checkpoints_dir = self.indexed_task_dir(task_session_id)?.join("checkpoints");
        if !checkpoints_dir.exists() {
            return Ok(Vec::new());
        }
        let mut checkpoints: Vec<SessionCompactionCheckpoint> = Vec::new();
        for entry in fs::read_dir(&checkpoints_dir).map_err(to_store_error)? {
            let entry = entry.map_err(to_store_error)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("json") {
                continue;
            }
            if let Some(checkpoint) = read_json_file(&path)? {
                checkpoints.push(checkpoint);
            }
        }
        checkpoints.sort_by_key(|checkpoint| checkpoint.through_sequence);
        Ok(checkpoints)
    }
}

impl TaskSessionCompactionStore for FileTaskSessionStore {
    /// Saves a compacted context document for a task session.
    /// 保存子代理会话的压缩后上下文文档。
    fn save_compacted_context(
        &self,
        context: SessionContextDocument,
    ) -> Result<(), SessionStoreError> {
        self.save_context(context)
    }

    /// Saves a compaction checkpoint for a task session.
    /// 保存子代理会话的压缩检查点。
    fn save_compaction_checkpoint(
        &self,
        checkpoint: SessionCompactionCheckpoint,
    ) -> Result<(), SessionStoreError> {
        self.save_task_compaction_checkpoint(checkpoint)
    }
}

/// File index that maps task-session identifiers to file locations.
/// 将子代理会话标识映射到文件位置的文件索引。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
struct TaskSessionFileIndex {
    /// Indexed task-session entries keyed by task_session_id.
    /// 按 task_session_id 索引的子代理会话条目。
    entries: BTreeMap<String, TaskSessionFileIndexEntry>,
}

/// Single task-session file index entry.
/// 单条子代理会话文件索引条目。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct TaskSessionFileIndexEntry {
    /// Task session identifier.
    /// 子代理会话标识符。
    task_session_id: TaskSessionId,
    /// Public task identifier.
    /// 公开任务标识符。
    public_task_id: String,
    /// Display title.
    /// 显示标题。
    display_title: String,
    /// Parent session reference.
    /// 父级会话引用。
    parent: ParentSessionRef,
    /// Relative directory from the root main-session directory.
    /// 相对于根主会话目录的相对目录。
    relative_dir: PathBuf,
    /// Current task-session status.
    /// 当前子代理会话状态。
    status: TaskSessionStatus,
    /// Last update timestamp in milliseconds.
    /// 最近更新时间毫秒值。
    updated_at: u64,
}

impl TaskSessionFileIndexEntry {
    /// Builds one index entry from task-session metadata.
    /// 从子代理会话元数据构建一条索引条目。
    fn from_metadata(metadata: &TaskSessionMetadata, relative_dir: PathBuf) -> Self {
        Self {
            task_session_id: metadata.task_session_id,
            public_task_id: metadata.public_task_id.clone(),
            display_title: metadata.display_title.clone(),
            parent: metadata.parent.clone(),
            relative_dir,
            status: metadata.status,
            updated_at: metadata.updated_at,
        }
    }

    /// Converts this index entry into a task-session summary.
    /// 将该索引条目转换为子代理会话摘要。
    fn to_summary(&self) -> Option<TaskSessionSummary> {
        Some(TaskSessionSummary {
            root_session_id: self.root_session_id()?,
            task_session_id: self.task_session_id,
            public_task_id: self.public_task_id.clone(),
            display_title: self.display_title.clone(),
            status: self.status,
            parent: self.parent.clone(),
            updated_at: self.updated_at,
            last_result_summary: None,
        })
    }

    /// Returns the root main-session identifier from the parent reference.
    /// 从父级引用返回根主会话标识符。
    fn root_session_id(&self) -> Option<SessionNodeId> {
        match self.parent {
            ParentSessionRef::None => None,
            ParentSessionRef::MainSession { root_session_id } => Some(root_session_id),
            ParentSessionRef::TaskSession {
                root_session_id, ..
            } => Some(root_session_id),
        }
    }
}

/// Creates a directory tree and converts IO errors into store errors.
/// 创建目录树并将 IO 错误转换为存储错误。
fn create_dir_all(path: &Path) -> Result<(), SessionStoreError> {
    fs::create_dir_all(path).map_err(to_store_error)
}

/// Writes one JSON file through a temporary path.
/// 通过临时路径写入一个 JSON 文件。
fn write_json_file<T>(path: &Path, value: &T) -> Result<(), SessionStoreError>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let temp_path = path.with_extension("tmp");
    let bytes = serde_json::to_vec_pretty(value).map_err(to_store_error)?;
    fs::write(&temp_path, bytes).map_err(to_store_error)?;
    if path.exists() {
        fs::remove_file(path).map_err(to_store_error)?;
    }
    fs::rename(&temp_path, path).map_err(to_store_error)
}

/// Reads one optional JSON file.
/// 读取一个可选 JSON 文件。
fn read_json_file<T>(path: &Path) -> Result<Option<T>, SessionStoreError>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(None);
    }
    let file = File::open(path).map_err(to_store_error)?;
    serde_json::from_reader(file)
        .map(Some)
        .map_err(to_store_error)
}

/// Appends one JSON value as one JSONL record.
/// 将一个 JSON 值作为一条 JSONL 记录追加。
fn append_json_line<T>(path: &Path, value: &T) -> Result<(), SessionStoreError>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(to_store_error)?;
    let line = serde_json::to_string(value).map_err(to_store_error)?;
    writeln!(file, "{line}").map_err(to_store_error)
}

/// Reads JSONL records from a file.
/// 从文件读取 JSONL 记录。
fn read_json_lines<T>(path: &Path) -> Result<Vec<T>, SessionStoreError>
where
    T: for<'de> Deserialize<'de>,
{
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path).map_err(to_store_error)?;
    let reader = BufReader::new(file);
    let mut records = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(to_store_error)?;
        if line.trim().is_empty() {
            continue;
        }
        records.push(serde_json::from_str(&line).map_err(to_store_error)?);
    }
    Ok(records)
}

/// Converts a lower-level error into a session store error.
/// 将底层错误转换为会话存储错误。
fn to_store_error(error: impl std::fmt::Display) -> SessionStoreError {
    SessionStoreError::new(error.to_string())
}
