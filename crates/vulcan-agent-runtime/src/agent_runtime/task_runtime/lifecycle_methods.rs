use super::*;

impl TaskRuntime {
    /// Returns a cached tool result for an existing terminal task instead of starting the child again.
    /// 对已存在的终态任务返回缓存工具结果，而不是再次启动子代理。
    pub(super) fn cached_task_invocation_result_for_existing_run(
        &self,
        current_session_id: SessionId,
        resolved_identifier: &ResolvedTaskSessionIdentifier,
        incoming_prompt: &str,
    ) -> Result<Option<VulcanToolInvocationResult>, String> {
        if !resolved_identifier.is_resumed {
            return Ok(None);
        }

        if let Some(record) = self
            .sessions
            .read()
            .map_err(|_| {
                String::from(
                    "The task session registry is locked and cannot inspect the cached task result.",
                )
            })?
            .get(&resolved_identifier.task_id)
            .cloned()
            .filter(task_session_record_has_cached_status)
        {
            return Ok(Some(build_cached_task_invocation_result_from_record(
                current_session_id,
                &record,
            )));
        }

        if let Some(metadata) = resolved_identifier
            .persisted_metadata
            .as_ref()
            .filter(|metadata| task_session_metadata_has_cached_status(metadata.status))
        {
            return Ok(Some(build_cached_task_invocation_result_from_metadata(
                current_session_id,
                resolved_identifier,
                metadata,
                incoming_prompt,
            )));
        }

        Ok(None)
    }

    /// Stops one in-flight child task by its stable task identifier when the runtime still owns a live abort handle.
    /// 当运行时仍持有实时中止句柄时，按稳定任务标识停止一条进行中的子任务。
    pub fn stop_task(&self, task_id: SessionId) -> Result<bool, String> {
        let handle = self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot stop the subagent task.",
                )
            })?
            .get(&task_id)
            .cloned();
        if let Some(handle) = handle {
            handle.abort();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Stops one in-flight child task and every active descendant task in its TaskSession tree.
    /// 停止一条进行中的子任务及其 TaskSession 树中所有仍活动的后代任务。
    pub fn stop_task_tree(&self, task_id: SessionId) -> Result<Vec<SessionId>, String> {
        let task_ids = self.task_subtree_ids(task_id)?;
        let handles = {
            let active_task_runs = self.active_task_runs.read().map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot stop the subagent task tree.",
                )
            })?;
            task_ids
                .iter()
                .copied()
                .filter_map(|candidate_task_id| {
                    active_task_runs
                        .get(&candidate_task_id)
                        .cloned()
                        .map(|handle| (candidate_task_id, handle))
                })
                .collect::<Vec<_>>()
        };
        for (_, handle) in &handles {
            handle.abort();
        }
        let stopped_task_ids = handles
            .into_iter()
            .map(|(stopped_task_id, _)| stopped_task_id)
            .collect::<Vec<_>>();
        self.mark_stopped_tasks_in_registry(&stopped_task_ids)?;
        Ok(stopped_task_ids)
    }

    /// Marks stopped tasks in the in-memory record and Vulcan registry before background joins settle.
    /// 在后台 Join 落定前，将已停止任务同步标记到内存记录与 Vulcan 注册表。
    fn mark_stopped_tasks_in_registry(&self, task_ids: &[SessionId]) -> Result<(), String> {
        if task_ids.is_empty() {
            return Ok(());
        }
        let updated_at = current_unix_millis_text();
        let mut stopped_records = Vec::new();
        {
            let mut sessions = self.sessions.write().map_err(|_| {
                String::from(
                    "The task-session registry is locked and cannot mark stopped subagents.",
                )
            })?;
            for task_id in task_ids {
                let Some(record) = sessions.get_mut(task_id) else {
                    continue;
                };
                record.last_status =
                    status_name_for_task_session_status(TaskSessionStatus::Stopped).to_string();
                record.updated_at = updated_at.clone();
                if record.last_result_text.trim().is_empty()
                    || record.last_result_text == RUNNING_TASK_RESULT_SUMMARY
                {
                    record.last_result_text = String::from(STOPPED_TASK_RESULT_SUMMARY);
                }
                stopped_records.push(record.clone());
            }
        }
        for record in stopped_records {
            let result_summary = (!record.last_result_text.trim().is_empty())
                .then_some(record.last_result_text.as_str());
            self.upsert_subagent_registry_entry(
                &record,
                TaskSessionStatus::Stopped,
                result_summary,
            )?;
        }
        Ok(())
    }

    /// Returns whether one child task still owns an in-flight abort handle, which the WebUI can treat as the source of truth for active-running state.
    /// 返回某个子任务当前是否仍持有活动中的中止句柄，供WebUI将其视作真实运行态事实源。
    pub fn is_task_running(&self, task_id: SessionId) -> Result<bool, String> {
        self.task_has_active_or_starting_run(task_id)
    }

    /// Returns whether one task owns an active abort handle.
    /// 返回某个任务是否持有活动中止句柄。
    pub(super) fn task_has_active_or_starting_run(
        &self,
        task_id: SessionId,
    ) -> Result<bool, String> {
        Ok(self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot inspect the subagent run state.",
                )
            })?
            .contains_key(&task_id))
    }

    /// Collects one task identifier and all known descendant task identifiers from the runtime registry.
    /// 从运行时注册表中收集一个任务标识及其所有已知后代任务标识。
    fn task_subtree_ids(&self, task_id: SessionId) -> Result<Vec<SessionId>, String> {
        let sessions = self.sessions.read().map_err(|_| {
            String::from("The task-session registry is locked and cannot inspect descendants.")
        })?;
        let mut task_ids = Vec::new();
        let mut pending = vec![task_id];
        while let Some(current_task_id) = pending.pop() {
            if task_ids.contains(&current_task_id) {
                continue;
            }
            task_ids.push(current_task_id);
            pending.extend(
                sessions
                    .values()
                    .filter(|record| record.parent_task_id == Some(current_task_id))
                    .map(|record| record.task_id),
            );
        }
        Ok(task_ids)
    }

    /// Subscribes to one detached child-task event stream, replaying any buffered early events before switching to live delivery.
    /// 订阅某条分离子任务事件流，并在切换到实时投递之前先回放已经缓冲的早期事件。
    pub fn subscribe_detached_task_events(
        &self,
        task_id: SessionId,
    ) -> impl futures_util::Stream<Item = DetachedTaskRuntimeEvent> + Send + 'static {
        let (sender, mut receiver) = unbounded_channel::<DetachedTaskRuntimeEvent>();
        let terminal_task_without_stream = self.task_has_terminal_record(task_id);
        let (buffered_events, closed) = {
            let mut streams = self
                .detached_task_event_streams
                .write()
                .expect("task detached stream registry lock");
            if !streams.contains_key(&task_id) && terminal_task_without_stream {
                (Vec::new(), true)
            } else {
                let state = streams.entry(task_id).or_default();
                let buffered_events = std::mem::take(&mut state.buffered_events);
                let closed = state.closed;
                if !closed {
                    state.subscribers.push(sender.clone());
                } else {
                    streams.remove(&task_id);
                }
                (buffered_events, closed)
            }
        };

        for event in buffered_events {
            let _ = sender.send(event);
        }
        if closed {
            drop(sender);
        }

        stream! {
            while let Some(event) = receiver.recv().await {
                yield event;
            }
        }
    }

    /// Returns whether the task registry already knows this child task reached a terminal status.
    /// 返回任务注册表是否已经确认该子任务到达终态。
    fn task_has_terminal_record(&self, task_id: SessionId) -> bool {
        self.sessions
            .read()
            .map(|sessions| {
                sessions.get(&task_id).is_some_and(|record| {
                    matches!(
                        record.last_status.as_str(),
                        "completed" | "failed" | "stopped"
                    )
                })
            })
            .unwrap_or(false)
    }

    /// Returns one backend handler for the given controller session identifier when it exists.
    /// 当给定控制器会话标识存在时，返回对应的后端处理器。
    pub(super) fn backend_for_session(
        &self,
        session_id: SessionId,
    ) -> Option<TaskExecutionHandler> {
        self.backends
            .read()
            .expect("task backend registry lock")
            .get(&session_id)
            .cloned()
    }

    /// Removes one backend handler from the registry.
    /// 从注册表中移除一个后端处理器。
    pub(super) fn remove_backend(&self, session_id: SessionId) {
        self.backends
            .write()
            .expect("task backend registry lock")
            .remove(&session_id);
    }

    /// Installs one active child-task abort handle and returns a guard that unregisters it on drop.
    /// 安装一条活动子任务中止句柄，并返回一个在 Drop 时自动注销它的守卫。
    pub(super) fn install_active_task_run(
        self: &Arc<Self>,
        task_id: SessionId,
        handle: AbortHandle,
    ) -> ActiveTaskRunGuard {
        self.active_task_runs
            .write()
            .expect("task active run registry lock")
            .insert(task_id, handle);
        ActiveTaskRunGuard {
            runtime: Arc::clone(self),
            task_id,
        }
    }

    /// Removes one active child-task abort handle from the registry.
    /// 从注册表中移除一条活动子任务中止句柄。
    pub(super) fn remove_active_task_run(&self, task_id: SessionId) {
        self.active_task_runs
            .write()
            .expect("task active run registry lock")
            .remove(&task_id);
    }

    /// Returns whether any known descendant task still has an active provider run.
    /// 返回任一已知后代任务是否仍持有活动 Provider 运行。
    pub(super) fn task_has_active_descendant_runs(&self, task_id: SessionId) -> bool {
        let Ok(task_ids) = self.task_subtree_ids(task_id) else {
            return false;
        };
        let Ok(active_runs) = self.active_task_runs.read() else {
            return true;
        };
        task_ids
            .into_iter()
            .filter(|candidate_task_id| *candidate_task_id != task_id)
            .any(|candidate_task_id| active_runs.contains_key(&candidate_task_id))
    }

    /// Publishes one detached child-task event, buffering it when no subscriber has attached yet and replaying it later on first subscription.
    /// 发布一条分离子任务事件；若当前尚无订阅者，则先缓冲，待首个订阅建立后再回放。
    pub(super) fn publish_detached_task_event(
        &self,
        task_id: SessionId,
        event: DetachedTaskRuntimeEvent,
    ) {
        let mut streams = self
            .detached_task_event_streams
            .write()
            .expect("task detached stream registry lock");
        let state = streams.entry(task_id).or_default();
        if state.closed {
            return;
        }
        if state.subscribers.is_empty() {
            state.buffered_events.push(event);
            return;
        }

        let mut delivered = false;
        state.subscribers.retain(|sender| {
            let send_result = sender.send(event.clone());
            if send_result.is_ok() {
                delivered = true;
            }
            send_result.is_ok()
        });
        if !delivered {
            state.buffered_events.push(event);
        }
    }

    /// Marks one detached child-task event stream as terminal and drops active senders so WebUI listener streams can close naturally.
    /// 将某条分离子任务事件流标记为终态，并释放活动发送端，让WebUI监听流自然关闭。
    pub(super) fn close_detached_task_event_stream(&self, task_id: SessionId) {
        let mut streams = self
            .detached_task_event_streams
            .write()
            .expect("task detached stream registry lock");
        let should_remove = if let Some(state) = streams.get_mut(&task_id) {
            state.closed = true;
            !state.subscribers.is_empty() || state.buffered_events.is_empty()
        } else {
            false
        };
        if should_remove {
            streams.remove(&task_id);
        }
    }
}
