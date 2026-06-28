use super::*;

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
