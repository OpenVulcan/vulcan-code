use super::*;

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

    /// Loads every task-session metadata record with one metadata-only directory scan.
    /// 通过一次仅元数据目录扫描加载所有子任务会话元数据记录。
    pub fn load_all_task_session_metadata(
        &self,
    ) -> Result<Vec<TaskSessionMetadata>, SessionStoreError> {
        let mut metadata = self
            .load_all_task_documents()?
            .into_iter()
            .map(|(_, document)| document.task_session)
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

    /// Loads task segment summaries from metadata.json without reading JSONL bodies.
    /// 仅从 metadata.json 加载子任务分段摘要，不读取 JSONL 正文。
    pub fn load_task_segment_summaries(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Vec<SessionSegmentSummary>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(task_session_id)?;
        let metadata =
            read_required_task_metadata_document(&task_dir.join(SEGMENTED_METADATA_FILE))?;
        Ok(metadata
            .segments
            .into_iter()
            .map(|segment| SessionSegmentSummary::new(segment.file, segment.line_count))
            .collect())
    }

    /// Returns the direct child task directory root.
    /// 返回直属子任务目录根。
    pub(super) fn task_root_dir(&self) -> PathBuf {
        self.root_session_dir.join(SEGMENTED_TASK_DIR)
    }

    /// Resolves the directory for one existing task session by its fixed task id path.
    /// 通过固定子任务标识路径解析一个已存在子任务会话的目录。
    pub(super) fn indexed_task_dir(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<PathBuf, SessionStoreError> {
        self.find_task_dir(task_session_id)?.ok_or_else(|| {
            SessionStoreError::new(format!("TaskSession `{task_session_id}` is not indexed."))
        })
    }

    /// Finds one task directory by directly checking `task/<task_id>/metadata.json`.
    /// 通过直接检查 `task/<task_id>/metadata.json` 查找一个任务目录。
    pub(super) fn find_task_dir(
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
    pub(super) fn load_all_task_documents(
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
    pub(super) fn new_task_dir(
        &self,
        metadata: &TaskSessionMetadata,
    ) -> Result<PathBuf, SessionStoreError> {
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
    pub(super) fn create_task_layout(task_dir: &Path) -> Result<(), SessionStoreError> {
        create_dir_all(task_dir)?;
        Ok(())
    }

    /// Appends one generic event line to a task session segment.
    /// 向一个子任务会话分段追加一条通用事件行。
    pub(super) fn append_task_segment_event(
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
    pub(super) fn append_parent_invocation_event(
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
    pub(super) fn read_parent_invocation_events(
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
