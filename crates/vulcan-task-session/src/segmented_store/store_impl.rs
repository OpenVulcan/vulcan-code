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
        self.load_task_session_metadata_for_parent(parent)
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

    /// Loads all task-session metadata grouped by direct parent with one metadata-only scan.
    /// 通过一次仅元数据扫描加载按直接父级分组的全部子任务会话元数据。
    pub fn load_task_session_metadata_by_parent(
        &self,
    ) -> Result<BTreeMap<ParentSessionRef, Vec<TaskSessionMetadata>>, SessionStoreError> {
        let mut metadata_by_parent: BTreeMap<ParentSessionRef, Vec<TaskSessionMetadata>> =
            BTreeMap::new();
        for (_, document) in self.load_all_task_documents()? {
            metadata_by_parent
                .entry(document.task_session.parent.clone())
                .or_default()
                .push(document.task_session);
        }
        for metadata in metadata_by_parent.values_mut() {
            metadata.sort_by_key(|metadata| metadata.task_session_id);
            metadata.dedup_by_key(|metadata| metadata.task_session_id);
        }
        Ok(metadata_by_parent)
    }

    /// Loads root task mirror summaries without scanning child task directories.
    /// 加载根任务镜像摘要，且不扫描子任务目录。
    pub fn load_task_session_mirror_summaries(
        &self,
    ) -> Result<Vec<TaskSessionMirrorSummary>, SessionStoreError> {
        load_main_session_task_mirror_summaries(&self.root_session_dir)
    }

    /// Returns whether the root main-session metadata document exists.
    /// 返回根主会话元数据文档是否存在。
    pub fn root_session_metadata_exists(&self) -> bool {
        self.root_session_dir.join(SEGMENTED_METADATA_FILE).exists()
    }

    /// Returns whether the root metadata contains the task mirror array.
    /// 返回根元数据是否包含子任务镜像数组。
    pub fn root_session_task_mirror_exists(&self) -> Result<bool, SessionStoreError> {
        let metadata_path = self.root_session_dir.join(SEGMENTED_METADATA_FILE);
        if !metadata_path.exists() {
            return Ok(false);
        }
        let metadata = read_required_json::<Value>(&metadata_path)?;
        Ok(metadata.get("tasks").is_some_and(Value::is_array))
    }

    /// Returns whether the root session has a task-session directory.
    /// 返回根会话是否存在子任务会话目录。
    pub fn task_session_directory_exists(&self) -> bool {
        self.root_session_dir.join(SEGMENTED_TASK_DIR).is_dir()
    }

    /// Resolves the on-disk directory for one existing task session.
    /// 解析一个已存在子任务会话的磁盘目录。
    ///
    /// # Parameters
    /// 参数
    /// `task_session_id` is the task session identifier whose directory should be returned.
    /// `task_session_id` 是需要返回目录的子任务会话标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns the indexed task-session directory or an error when it cannot be found.
    /// 返回已索引的子任务会话目录；无法找到时返回错误。
    pub fn task_session_directory(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<PathBuf, SessionStoreError> {
        self.indexed_task_dir(task_session_id)
    }

    /// Loads root task mirror summaries grouped by direct parent.
    /// 加载按直接父级分组的根任务镜像摘要。
    pub fn load_task_session_mirror_summaries_by_parent(
        &self,
    ) -> Result<BTreeMap<ParentSessionRef, Vec<TaskSessionMirrorSummary>>, SessionStoreError> {
        let mut mirrors_by_parent: BTreeMap<ParentSessionRef, Vec<TaskSessionMirrorSummary>> =
            BTreeMap::new();
        for mirror in self.load_task_session_mirror_summaries()? {
            mirrors_by_parent
                .entry(mirror.parent.clone())
                .or_default()
                .push(mirror);
        }
        for mirrors in mirrors_by_parent.values_mut() {
            mirrors.sort_by_key(|mirror| mirror.task_session_id);
            mirrors.dedup_by_key(|mirror| mirror.task_session_id);
        }
        Ok(mirrors_by_parent)
    }

    /// Loads root task mirror summaries grouped by reachable parent for one root session tree.
    /// 按单个根会话树的可达父级分组加载根任务镜像摘要。
    pub fn load_task_session_mirror_summaries_by_parent_for_root_tree(
        &self,
        root_session_id: RootSessionId,
    ) -> Result<BTreeMap<ParentSessionRef, Vec<TaskSessionMirrorSummary>>, SessionStoreError> {
        load_main_session_task_mirror_summaries_by_parent_for_root_tree(
            &self.root_session_dir,
            root_session_id,
        )
    }

    /// Loads direct child task-session metadata for one parent with one metadata-only scan.
    /// 通过一次仅元数据扫描加载某个父级的直属子任务会话元数据。
    pub fn load_task_session_metadata_for_parent(
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
        let (_, lines) = read_repaired_task_segment_lines(&task_dir)?;
        for line in lines.into_iter().rev() {
            let Some(value) = line.payload.get("session_event").cloned() else {
                continue;
            };
            let record =
                serde_json::from_value::<SessionEventRecord>(value).map_err(to_store_error)?;
            if !record.payload.is_null() {
                return Ok(Some(record.payload));
            }
        }
        Ok(None)
    }

    /// Loads task segment summaries after repairing the task segment files.
    /// 修复子任务分段文件后加载分段摘要。
    pub fn load_task_segment_summaries(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Vec<SessionSegmentSummary>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(task_session_id)?;
        let (metadata, _) = read_repaired_task_segment_lines(&task_dir)?;
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
        let expected_id = task_session_id.to_string();
        let direct_task_dir = self
            .task_root_dir()
            .join(sanitize_file_component(&expected_id));
        let direct_metadata_path = task_metadata_path(&direct_task_dir);
        if direct_metadata_path.is_file() {
            return Ok(direct_task_dir);
        }
        self.find_task_dir(task_session_id)?.ok_or_else(|| {
            SessionStoreError::new(format!("TaskSession `{task_session_id}` is not indexed."))
        })
    }

    /// Finds one task directory by directly checking `task/<task_id>/task_metadata.json`.
    /// 通过直接检查 `task/<task_id>/task_metadata.json` 查找一个任务目录。
    pub(super) fn find_task_dir(
        &self,
        task_session_id: TaskSessionId,
    ) -> Result<Option<PathBuf>, SessionStoreError> {
        let expected_id = task_session_id.to_string();
        let direct_task_dir = self
            .task_root_dir()
            .join(sanitize_file_component(&expected_id));
        let direct_metadata_path = task_metadata_path(&direct_task_dir);
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
            let metadata_path = task_metadata_path(&task_dir);
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
        let _guard = vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
            .lock()
            .map_err(to_store_error)?;
        let task_dir = self.indexed_task_dir(task_session_id)?;
        let main_metadata_path = task_main_metadata_path(&task_dir);
        let mut main_metadata = read_required_task_main_metadata_document(&main_metadata_path)?;
        repair_task_segment_jsonl_files_if_needed_locked(&task_dir, &mut main_metadata)?;
        main_metadata.next_global_seq = main_metadata.next_global_seq.max(
            read_segment_lines_with_owned_append_lock(&task_dir, &main_metadata.segments)?
                .into_iter()
                .map(|line| line.seq)
                .max()
                .map(|seq| seq.saturating_add(1))
                .unwrap_or(0),
        );
        let mut payload = payload;
        backfill_task_segment_payload_sequence(&mut payload, main_metadata.next_global_seq);
        let segment_policy = main_metadata.segment_policy.clone();
        let line = append_segment_event(
            SegmentEventAppendTarget {
                dir: &task_dir,
                segments: &mut main_metadata.segments,
                active_segment: &mut main_metadata.active_segment,
                next_seq: &mut main_metadata.next_global_seq,
                segment_policy: &segment_policy,
            },
            SegmentEventAppendRequest {
                kind: kind.into(),
                timestamp: timestamp_ms.to_string(),
                payload,
                turn_index: None,
            },
        )?;
        main_metadata.updated_at = timestamp_ms.to_string();
        write_json_file(&main_metadata_path, &main_metadata)?;
        let metadata_path = task_metadata_path(&task_dir);
        let mut metadata = read_required_task_metadata_document(&metadata_path)?;
        metadata.updated_at = timestamp_ms;
        metadata.task_session.updated_at = timestamp_ms;
        sync_task_metadata_segments_from_main(&mut metadata, &main_metadata);
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
                    "subagent_task_event_linked",
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

/// Backfills store-assigned sequence numbers into payloads that contain nested task events.
/// 将存储层分配的序号回填到包含嵌套任务事件的载荷中。
///
/// # Parameters
/// 参数
/// `payload` is the mutable JSON payload about to be written as one segment line.
/// `payload` 是即将写入单条分段行的可变 JSON 载荷。
/// `assigned_sequence` is the JSONL sequence assigned by the segmented store.
/// `assigned_sequence` 是分段存储分配的 JSONL 序号。
fn backfill_task_segment_payload_sequence(payload: &mut Value, assigned_sequence: u64) {
    if let Some(session_event) = payload
        .get_mut("session_event")
        .and_then(Value::as_object_mut)
    {
        session_event.insert(String::from("sequence"), json!(assigned_sequence));
    }
    if let Some(task_invocation_event) = payload
        .get_mut("task_invocation_event")
        .and_then(Value::as_object_mut)
    {
        task_invocation_event.insert(String::from("sequence"), json!(assigned_sequence));
    }
}
