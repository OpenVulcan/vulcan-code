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
        let (_, lines) = read_repaired_task_segment_lines(&task_dir)?;
        let mut records = Vec::new();
        for line in lines {
            if let Some(value) = line.payload.get("session_event").cloned() {
                let mut record: SessionEventRecord =
                    serde_json::from_value(value).map_err(to_store_error)?;
                record.sequence = line.seq;
                records.push(record);
            }
        }
        records.sort_by_key(|record: &SessionEventRecord| record.sequence);
        Ok(records)
    }

    /// Saves the current API-facing context document into state.json.
    /// 将当前 API 实际上下文文档保存到 state.json。
    fn save_context(&self, context: SessionContextDocument) -> Result<(), SessionStoreError> {
        let task_dir = self.indexed_task_dir(context.node_id)?;
        // state_path is the shared document containing both context and runtime checkpoint fields.
        // state_path 是同时包含上下文与运行时检查点字段的共享文档。
        let state_path = task_dir.join(SEGMENTED_STATE_FILE);
        // transaction_lock serializes this complete read-modify-write with checkpoint saves.
        // transaction_lock 将该完整读改写周期与检查点保存串行化。
        let transaction_lock = json_file_transaction_lock(&state_path)?;
        // transaction_guard remains held until the merged state document is atomically published.
        // transaction_guard 保持到合并后的状态文档完成原子发布。
        let _transaction_guard = transaction_lock.lock().map_err(to_store_error)?;
        let mut state = read_task_state(&task_dir)?.unwrap_or_else(|| {
            SegmentedTaskStateDocument::new(context.node_id, current_unix_millis_u64())
        });
        state.updated_at = current_unix_millis_u64().to_string();
        state.runtime_state = task_main_runtime_state_from_context(&context, &state.updated_at);
        state.current_context = task_main_current_context_from_context(&context, &state.updated_at);
        state.task_context = Some(context);
        write_json_file(&state_path, &state)
    }

    /// Loads the current API-facing context document from state.json.
    /// 从 state.json 加载当前 API 实际上下文文档。
    fn load_context(
        &self,
        node_id: SessionNodeId,
    ) -> Result<Option<SessionContextDocument>, SessionStoreError> {
        let task_dir = self.indexed_task_dir(node_id)?;
        Ok(read_task_state(&task_dir)?.and_then(|state| state.task_context))
    }

    /// Saves a lightweight runtime checkpoint into state.json.
    /// 将轻量运行时检查点保存到 state.json。
    fn save_runtime_checkpoint(
        &self,
        checkpoint: SessionRuntimeCheckpoint,
    ) -> Result<(), SessionStoreError> {
        let task_dir = self.indexed_task_dir(checkpoint.node_id)?;
        // state_path is the same shared document written by save_context.
        // state_path 是 save_context 写入的同一共享文档。
        let state_path = task_dir.join(SEGMENTED_STATE_FILE);
        // transaction_lock prevents checkpoint saves from overwriting a concurrently merged context.
        // transaction_lock 防止检查点保存覆盖并发合并的上下文。
        let transaction_lock = json_file_transaction_lock(&state_path)?;
        // transaction_guard covers loading the latest state, updating checkpoint, and atomic publication.
        // transaction_guard 覆盖加载最新状态、更新检查点与原子发布。
        let _transaction_guard = transaction_lock.lock().map_err(to_store_error)?;
        let mut state = read_task_state(&task_dir)?.unwrap_or_else(|| {
            SegmentedTaskStateDocument::new(checkpoint.node_id, current_unix_millis_u64())
        });
        state.updated_at = current_unix_millis_u64().to_string();
        state.runtime_checkpoint = Some(checkpoint);
        write_json_file(&state_path, &state)
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
    /// Creates and atomically publishes a complete segmented task-session layout.
    /// 创建并原子发布完整的分段子任务会话布局。
    ///
    /// # Parameters
    /// 参数
    /// `metadata` is the authoritative metadata snapshot for the new task session.
    /// `metadata` 是新子任务会话的权威元数据快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the complete child layout and its parent invocation link are published.
    /// 完整子任务布局及其父级调用关联均发布后返回成功。
    fn create_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError> {
        // transaction_guard serializes existence checks, publication, and parent-link append.
        // transaction_guard 将存在性检查、发布与父级关联追加串行化。
        let _transaction_guard =
            vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
                .lock()
                .map_err(to_store_error)?;
        create_dir_all(&self.task_root_dir())?;
        // task_dir is the single authoritative directory for this stable task-session id.
        // task_dir 是该稳定子任务会话标识对应的唯一权威目录。
        let task_dir = self.new_task_dir(&metadata)?;
        match fs::symlink_metadata(&task_dir) {
            Ok(_) => {
                return Err(SessionStoreError::new(format!(
                    "TaskSession directory already exists: {}",
                    task_dir.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(to_store_error(error)),
        }
        // root_metadata seeds inherited layout fields without exposing the staged directory.
        // root_metadata 为继承布局字段提供初值，且不会暴露暂存目录。
        let root_metadata = read_root_metadata_value(&self.root_session_dir)?;
        // main_metadata indexes the child JSONL stream stored in the staged layout.
        // main_metadata 索引暂存布局中的子任务 JSONL 流。
        let mut main_metadata =
            SegmentedTaskSessionMainMetadataDocument::new(&metadata, root_metadata.as_ref());
        // document contains the API-facing authoritative task metadata projection.
        // document 包含面向 API 的权威子任务元数据投影。
        let mut document = SegmentedTaskMetadataDocument::new(metadata.clone());
        // meta_line is the first immutable event in every published task stream.
        // meta_line 是每个已发布子任务流中的首条不可变事件。
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
        // staged_task_dir remains invisible to task discovery until every child artifact is complete.
        // staged_task_dir 在全部子任务产物完成前始终对任务发现不可见。
        let staged_task_dir = create_task_directory_staging(&task_dir)?;
        // staging_result captures every fallible child write so failures can remove the whole layout.
        // staging_result 捕获全部可能失败的子任务写入，以便失败时删除整个布局。
        let staging_result = (|| -> Result<(), SessionStoreError> {
            // bytes records the exact initial JSONL footprint reflected in segment metadata.
            // bytes 记录分段元数据所反映的初始 JSONL 精确字节占用。
            let bytes = append_json_line(
                &staged_task_dir.join(&main_metadata.active_segment),
                &meta_line,
            )?;
            if let Some(segment) = main_metadata.segments.first_mut() {
                segment.line_count = 1;
                segment.byte_len = bytes;
                segment.start_seq = 0;
                segment.first_timestamp = Some(meta_line.timestamp.clone());
                segment.last_timestamp = Some(meta_line.timestamp.clone());
            }
            main_metadata.next_global_seq = 1;
            sync_task_metadata_segments_from_main(&mut document, &main_metadata);
            write_json_file(&task_main_metadata_path(&staged_task_dir), &main_metadata)?;
            write_json_file(&task_metadata_path(&staged_task_dir), &document)?;
            write_json_file(
                &staged_task_dir.join(SEGMENTED_STATE_FILE),
                &SegmentedTaskStateDocument::new(metadata.task_session_id, metadata.created_at),
            )?;
            Ok(())
        })();
        if let Err(error) = staging_result {
            remove_task_directory_staging(&staged_task_dir);
            return Err(error);
        }
        if let Err(error) = publish_task_directory(&staged_task_dir, &task_dir) {
            remove_task_directory_staging(&staged_task_dir);
            return Err(error);
        }

        // created_event links the now-complete child layout into its direct parent and root index.
        // created_event 将现已完整的子任务布局关联到直接父级与根索引。
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
        self.append_parent_invocation_event_with_owned_append_lock(&metadata.parent, &created_event)
    }

    /// Saves task-session metadata and its root mirror as one shared append transaction.
    /// 将子任务会话元数据及其根镜像作为一次共享追加事务保存。
    ///
    /// # Parameters
    /// 参数
    /// `metadata` is the authoritative task-session metadata snapshot to persist.
    /// `metadata` 是要持久化的权威子任务会话元数据快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after both child metadata documents and the root mirror are published.
    /// 两份子任务元数据文档与根镜像均发布后返回成功。
    fn save_task_session(&self, metadata: TaskSessionMetadata) -> Result<(), SessionStoreError> {
        // The append lock protects the complete read-modify-write set shared with event appends.
        // 追加锁保护与事件追加共享的完整读改写集合。
        let _transaction_guard =
            vulcan_session_core::segmented_file_io::segmented_append_file_io_lock()
                .lock()
                .map_err(to_store_error)?;
        let task_dir = self.indexed_task_dir(metadata.task_session_id)?;
        let metadata_path = task_metadata_path(&task_dir);
        let mut document = read_required_task_metadata_document(&metadata_path)?;
        document.task_session = metadata;
        document.status = document.task_session.status;
        document.updated_at = document.task_session.updated_at;
        document.agent_kind = segmented_agent_kind(&document.task_session);
        document.agent_profile = segmented_agent_profile(&document.task_session);
        let main_metadata_path = task_main_metadata_path(&task_dir);
        let mut main_metadata = read_required_task_main_metadata_document(&main_metadata_path)?;
        main_metadata.title = Some(document.task_session.display_title.clone());
        main_metadata.updated_at = document.task_session.updated_at.to_string();
        sync_task_metadata_segments_from_main(&mut document, &main_metadata);
        write_json_file(&main_metadata_path, &main_metadata)?;
        write_json_file(&metadata_path, &document)?;
        refresh_main_session_task_mirror_from_metadata_with_owned_append_lock(
            &self.root_session_dir,
            &document,
        )?;
        Ok(())
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
        Ok(read_task_metadata_document(&task_metadata_path(&task_dir))?
            .map(|document| document.task_session))
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
        let (mut open_matches, mut archived_matches): (Vec<_>, Vec<_>) = self
            .load_task_session_metadata_for_parent(parent)?
            .into_iter()
            .filter(|metadata| metadata.public_task_id == normalized_public_task_id)
            .partition(|metadata| metadata.status != TaskSessionStatus::Archived);
        if open_matches.len() > 1 {
            let duplicate_session_ids = open_matches
                .iter()
                .map(|metadata| metadata.task_session_id.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(SessionStoreError::new(format!(
                "Parent-scoped task id `{normalized_public_task_id}` has multiple open sessions: {duplicate_session_ids}."
            )));
        }
        if let Some(open_match) = open_matches.pop() {
            return Ok(Some(open_match));
        }
        archived_matches.sort_by(|left, right| {
            // Select only the newest archived history after proving that no open owner exists.
            // 仅在确认不存在开放所有者后，选择最新的归档历史。
            right
                .updated_at
                .cmp(&left.updated_at)
                .then_with(|| right.created_at.cmp(&left.created_at))
                .then_with(|| right.task_session_id.cmp(&left.task_session_id))
        });
        Ok(archived_matches.into_iter().next())
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
            "display_event",
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
        let (_, lines) = read_repaired_task_segment_lines(&task_dir)?;
        let mut checkpoints = Vec::new();
        for line in lines {
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
