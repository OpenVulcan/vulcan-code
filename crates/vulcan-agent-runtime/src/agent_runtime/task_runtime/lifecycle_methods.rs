use super::*;

impl TaskRuntime {
    /// Returns a cached tool result for an existing terminal task instead of starting the child again.
    /// 对已存在的终态任务返回缓存工具结果，而不是再次启动子代理。
    pub(super) fn cached_task_invocation_result_for_existing_run(
        &self,
        current_session_id: SessionId,
        resolved_identifier: &ResolvedTaskSessionIdentifier,
    ) -> Result<Option<VulcanToolInvocationResult>, String> {
        if !resolved_identifier.is_resumed {
            return Ok(None);
        }

        let record = self
                .sessions
                .read()
                .map_err(|_| {
                    String::from(
                        "The task session registry is locked and cannot inspect the cached task result.",
                    )
                })?
                .get(&resolved_identifier.task_id)
                .cloned()
                .filter(task_session_record_has_cached_status);
        Ok(record.map(|record| {
            build_cached_task_invocation_result_from_record(current_session_id, &record)
        }))
    }

    /// Stops one reserved or running child task by its stable task identifier.
    /// 按稳定任务标识停止一条已预留或正在运行的子任务。
    pub fn stop_task(&self, task_id: SessionId) -> Result<bool, String> {
        let handle = {
            let mut active_task_runs = self.active_task_runs.write().map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot stop the subagent task.",
                )
            })?;
            let Some(entry) = active_task_runs.get_mut(&task_id) else {
                return Ok(false);
            };
            entry.stop_requested = true;
            entry.abort_handle.clone()
        };
        if let Some(handle) = handle {
            handle.abort();
        }
        self.mark_stopped_tasks_in_registry(&[task_id], "user_interrupt")?;
        Ok(true)
    }

    /// Stops one in-flight child task and every active descendant task in its TaskSession tree.
    /// 停止一条进行中的子任务及其 TaskSession 树中所有仍活动的后代任务。
    pub fn stop_task_tree(&self, task_id: SessionId) -> Result<Vec<SessionId>, String> {
        let task_ids = self.task_subtree_ids(task_id)?;
        self.stop_selected_tasks(&task_ids)
    }

    /// Stops active runs for exactly the supplied stable tasks without expanding descendants.
    /// 仅停止精确给定稳定任务的活动运行，不扩展后代。
    ///
    /// # Parameters
    /// 参数
    /// - `task_ids`: Exact task scope selected by the owning lifecycle transaction.
    /// - `task_ids`：由所属生命周期事务选定的精确任务范围。
    ///
    /// # Returns
    /// 返回值
    /// Tasks that had resident active runs reserved and stopped.
    /// 存在常驻活动运行并已被预留停止的任务。
    pub(super) fn stop_selected_tasks(
        &self,
        task_ids: &[SessionId],
    ) -> Result<Vec<SessionId>, String> {
        let stopped_task_ids = self.reserve_and_abort_selected_task_runs(task_ids)?;
        self.mark_stopped_tasks_in_registry(&stopped_task_ids, "user_interrupt")?;
        Ok(stopped_task_ids)
    }

    /// Reserves stop ownership and aborts active runs for exactly the supplied stable tasks.
    /// 为精确给定的稳定任务预留停止所有权并中止活动运行。
    ///
    /// # Parameters
    /// 参数
    /// - `task_ids`: Stable task scope already protected by the caller's transaction boundary.
    /// - `task_ids`：已由调用方事务边界保护的稳定任务范围。
    ///
    /// # Returns
    /// 返回值
    /// Tasks that owned active reservations when this operation claimed stop ownership.
    /// 本操作声明停止所有权时持有活动预留的任务。
    pub(super) fn reserve_and_abort_selected_task_runs(
        &self,
        task_ids: &[SessionId],
    ) -> Result<Vec<SessionId>, String> {
        let reserved_runs = {
            let mut active_task_runs = self.active_task_runs.write().map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot stop the selected subagent tasks.",
                )
            })?;
            task_ids
                .iter()
                .copied()
                .filter_map(|candidate_task_id| {
                    active_task_runs.get_mut(&candidate_task_id).map(|entry| {
                        entry.stop_requested = true;
                        (candidate_task_id, entry.abort_handle.clone())
                    })
                })
                .collect::<Vec<_>>()
        };
        for (_, handle) in &reserved_runs {
            if let Some(handle) = handle {
                handle.abort();
            }
        }
        let stopped_task_ids = reserved_runs
            .into_iter()
            .map(|(stopped_task_id, _)| stopped_task_id)
            .collect::<Vec<_>>();
        Ok(stopped_task_ids)
    }

    /// Stops every active child-agent run during application shutdown and updates process-local projections.
    /// 在应用关闭期间停止所有活动子代理运行，并更新进程内投影。
    pub fn stop_active_tasks_for_app_shutdown(&self) -> Result<Vec<SessionId>, String> {
        let reserved_runs = {
            let mut active_task_runs = self.active_task_runs.write().map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot stop active subagent tasks.",
                )
            })?;
            active_task_runs
                .iter_mut()
                .map(|(task_id, entry)| {
                    entry.stop_requested = true;
                    (*task_id, entry.abort_handle.clone())
                })
                .collect::<Vec<_>>()
        };
        for (_, handle) in &reserved_runs {
            if let Some(handle) = handle {
                handle.abort();
            }
        }
        let stopped_task_ids = reserved_runs
            .into_iter()
            .map(|(stopped_task_id, _)| stopped_task_id)
            .collect::<Vec<_>>();
        self.mark_stopped_tasks_in_registry(&stopped_task_ids, "app_shutdown")?;
        Ok(stopped_task_ids)
    }

    /// Marks stopped tasks in the in-memory record and Vulcan registry before background joins settle.
    /// 在后台 Join 落定前，将已停止任务同步标记到内存记录与 Vulcan 注册表。
    pub(super) fn mark_stopped_tasks_in_registry(
        &self,
        task_ids: &[SessionId],
        reason: &str,
    ) -> Result<(), String> {
        if task_ids.is_empty() {
            return Ok(());
        }
        let updated_at = current_unix_millis_text();
        for task_id in task_ids {
            // transaction_lock serializes the explicit stop with every execution outcome commit.
            // transaction_lock 将显式停止与全部执行结果提交串行化。
            let transaction_lock = self.task_execution_state_lock(*task_id)?;
            // _transaction keeps the stopped record and registry projection in one task boundary.
            // _transaction 将 stopped 记录与注册表投影保持在同一任务边界内。
            let _transaction = transaction_lock.lock().map_err(|_| {
                String::from(
                    "The task execution transaction is locked and cannot mark the subagent stopped.",
                )
            })?;
            self.mark_stopped_task_projection_with_transaction_lock_held(
                *task_id,
                reason,
                &updated_at,
            )?;
        }
        Ok(())
    }

    /// Marks stopped projections while the caller holds every selected task transaction lock.
    /// 在调用方持有全部选中任务事务锁时标记 stopped 投影。
    ///
    /// # Parameters
    /// 参数
    /// - `task_ids`: Stable task ids whose transaction locks are already held in sorted order.
    /// - `task_ids`：事务锁已按排序顺序持有的稳定任务标识。
    /// - `reason`: Explicit lifecycle reason stored in session and registry projections.
    /// - `reason`：写入会话与注册表投影的显式生命周期原因。
    ///
    /// # Returns
    /// 返回值
    /// Success after all resident records are projected as stopped, or a lock/invariant error.
    /// 全部驻留记录投影为 stopped 后返回成功，否则返回锁或不变量错误。
    pub(super) fn mark_stopped_tasks_with_transaction_locks_held(
        &self,
        task_ids: &[SessionId],
        reason: &str,
    ) -> Result<(), String> {
        let updated_at = current_unix_millis_text();
        for task_id in task_ids {
            self.mark_stopped_task_projection_with_transaction_lock_held(
                *task_id,
                reason,
                &updated_at,
            )?;
        }
        Ok(())
    }

    /// Projects one stopped task after its execution transaction lock has already been acquired.
    /// 在对应执行事务锁已取得后投影单个 stopped 任务。
    ///
    /// # Parameters
    /// 参数
    /// - `task_id`: Stable task whose resident projections may be updated.
    /// - `task_id`：可能需要更新驻留投影的稳定任务。
    /// - `reason`: Explicit stop reason.
    /// - `reason`：显式停止原因。
    /// - `updated_at`: Shared transaction timestamp encoded for the resident record.
    /// - `updated_at`：为驻留记录编码的共享事务时间戳。
    ///
    /// # Returns
    /// 返回值
    /// Success when the task is absent/archived or both open resident projections were updated consistently.
    /// 任务不存在、已归档，或两个开放驻留投影已一致更新时返回成功。
    fn mark_stopped_task_projection_with_transaction_lock_held(
        &self,
        task_id: SessionId,
        reason: &str,
        updated_at: &str,
    ) -> Result<(), String> {
        // stopped_record is the exact stable record snapshot projected after mutation.
        // stopped_record 是修改后用于投影的精确稳定记录快照。
        let stopped_record = {
            let mut sessions = self.sessions.write().map_err(|_| {
                String::from(
                    "The task-session registry is locked and cannot mark stopped subagents.",
                )
            })?;
            let Some(record) = sessions.get_mut(&task_id) else {
                return Ok(());
            };
            let current_status =
                task_session_status_from_record_status(record.last_status.as_str())?;
            if current_status == TaskSessionStatus::Archived {
                return Ok(());
            }
            record.last_status =
                status_name_for_task_session_status(TaskSessionStatus::Stopped).to_string();
            record.updated_at = updated_at.to_string();
            record.stop_reason = Some(reason.to_string());
            if record.last_result_text.trim().is_empty()
                || record.last_result_text == RUNNING_TASK_RESULT_SUMMARY
            {
                record.last_result_text = String::from(STOPPED_TASK_RESULT_SUMMARY);
            }
            record.clone()
        };
        let result_summary = (!stopped_record.last_result_text.trim().is_empty())
            .then_some(stopped_record.last_result_text.as_str());
        self.upsert_subagent_registry_entry(
            &stopped_record,
            TaskSessionStatus::Stopped,
            result_summary,
        )
    }

    /// Returns whether one child task still owns an in-flight abort handle, which the WebUI can treat as the source of truth for active-running state.
    /// 返回某个子任务当前是否仍持有活动中的中止句柄，供WebUI将其视作真实运行态事实源。
    pub fn is_task_running(&self, task_id: SessionId) -> Result<bool, String> {
        self.task_has_active_or_starting_run(task_id)
    }

    /// Checks whether `task_id` is owned by the exact `execution_id`, including its startup phase.
    /// 检查 `task_id` 是否由精确 `execution_id` 拥有，包括其启动阶段。
    /// Returns false for a replacement execution and reports registry lock failures explicitly.
    /// 对替代执行返回假，并明确报告注册表锁错误。
    pub fn is_task_execution_running(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<bool, String> {
        Ok(self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from("The active task registry cannot inspect execution ownership.")
            })?
            .get(&task_id)
            .is_some_and(|entry| entry.execution_id == execution_id))
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
    pub(super) fn task_subtree_ids(&self, task_id: SessionId) -> Result<Vec<SessionId>, String> {
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

    /// Subscribes to one exact detached execution stream and replays only that execution's buffer.
    /// 订阅一条精确的分离执行流，并且仅回放该执行的缓冲事件。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the stable task session reused across execution attempts.
    /// `task_id` 是跨执行尝试复用的稳定任务会话标识。
    /// `execution_id` is the unique concrete execution attempt to observe.
    /// `execution_id` 是要观测的唯一具体执行尝试。
    ///
    /// # Returns
    /// 返回值
    /// A static asynchronous stream isolated from every other execution of the same task, or an explicit registry-lock error.
    /// 返回与同一任务其他执行完全隔离的静态异步流，或返回明确的注册表锁错误。
    pub fn subscribe_detached_task_events(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<impl futures_util::Stream<Item = DetachedTaskRuntimeEvent> + Send + 'static, String>
    {
        self.subscribe_detached_task_events_inner(task_id, execution_id, false)
    }

    /// Subscribes a startup owner only while its exact stream remains open under the stream lock.
    /// 仅在事件流锁内确认精确流仍开放时为启动所有者订阅。
    /// Returns a registered live stream, or an explicit error if cancellation already removed its owner.
    /// 返回已注册活动流；如果取消已移除所有者则返回明确错误。
    pub fn subscribe_active_detached_task_events(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<impl futures_util::Stream<Item = DetachedTaskRuntimeEvent> + Send + 'static, String>
    {
        self.subscribe_detached_task_events_inner(task_id, execution_id, true)
    }

    /// Serializes optional startup admission with buffered replay and subscriber registration.
    /// 将可选启动准入与缓冲回放和订阅者注册串行化。
    /// `require_open` rejects late startup owners; historical subscribers retain terminal replay behavior.
    /// `require_open` 拒绝迟到启动所有者；历史订阅者保留终态回放行为。
    fn subscribe_detached_task_events_inner(
        &self,
        task_id: SessionId,
        execution_id: &str,
        require_open: bool,
    ) -> Result<impl futures_util::Stream<Item = DetachedTaskRuntimeEvent> + Send + 'static, String>
    {
        // stream_key identifies the only execution this subscriber may consume.
        // stream_key 标识本订阅者唯一允许消费的执行。
        let stream_key = DetachedTaskExecutionKey::new(task_id, execution_id);
        // sender and receiver form the subscriber-local delivery channel.
        // sender 与 receiver 组成订阅者本地投递通道。
        let (sender, mut receiver) = unbounded_channel::<DetachedTaskRuntimeEvent>();
        // closed is derived only after buffered replay and live-subscriber registration are serialized under the registry lock.
        // closed 仅在注册表锁内串行完成缓冲回放与实时订阅者注册后派生。
        let closed = {
            // streams provides exclusive access for subscriber registration and buffered-event transfer.
            // streams 为订阅者注册与缓冲事件转移提供独占访问。
            let mut streams = self.detached_task_event_streams.write().map_err(|_| {
                format!(
                    "The detached task stream registry is locked and cannot subscribe task `{task_id}` execution `{execution_id}`."
                )
            })?;
            if require_open && streams.get(&stream_key).is_none_or(|stream| stream.closed) {
                return Err(format!(
                    "Task `{task_id}` execution `{execution_id}` no longer has an open startup stream."
                ));
            }
            // closed records whether this exact execution has already reached terminal state.
            // closed 记录当前精确执行是否已经到达终态。
            let closed = if let Some(state) = streams.get_mut(&stream_key) {
                let buffered_events = std::mem::take(&mut state.buffered_events);
                let closed = state.closed;
                // Buffered events enter the subscriber channel before the sender becomes visible to concurrent live publishers.
                // 缓冲事件先进入订阅者通道，随后 sender 才对并发实时发布者可见。
                for event in buffered_events {
                    let _ = sender.send(event);
                }
                if !closed {
                    state.subscribers.push(sender.clone());
                }
                closed
            } else {
                true
            };
            if closed {
                streams.remove(&stream_key);
            }
            closed
        };
        if closed {
            drop(sender);
        }

        Ok(stream! {
            while let Some(event) = receiver.recv().await {
                yield event;
            }
        })
    }

    /// Opens one execution-scoped detached event stream before the parent announces DelegateStarted.
    /// 在父级发布 DelegateStarted 之前打开一条执行级分离事件流。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the stable child task session.
    /// `task_id` 是稳定子任务会话。
    /// `execution_id` is the unique execution attempt being opened.
    /// `execution_id` 是正在打开的唯一执行尝试。
    ///
    /// # Returns
    /// 返回值
    /// Success after opening only the exact execution key, or a registry-lock error.
    /// 仅打开精确执行键后返回成功，或返回注册表锁错误。
    pub(super) fn open_detached_task_event_stream_for_run(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<(), String> {
        let mut streams = self.detached_task_event_streams.write().map_err(|_| {
            String::from(
                "The detached task stream registry is locked and cannot open the subagent stream.",
            )
        })?;
        let state = streams
            .entry(DetachedTaskExecutionKey::new(task_id, execution_id))
            .or_default();
        state.closed = false;
        // A resumed run reuses the same task id, so stale terminal output must not replay into the new live panel.
        // 续跑会复用同一任务标识，因此旧终态输出不能回放到新的实时面板。
        state.buffered_events.clear();
        Ok(())
    }

    /// Returns one backend handler for the given controller session identifier when it exists.
    /// 当给定控制器会话标识存在时，返回对应的后端处理器。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the controller session whose exact task backend is requested.
    /// `session_id` 标识需要读取精确任务后端的控制器会话。
    ///
    /// # Returns
    /// 返回值
    /// Returns the registered backend when present, `None` when genuinely absent, or an explicit registry-read error.
    /// 已注册时返回后端，确实不存在时返回 `None`，注册表读取失败时返回明确错误。
    pub(super) fn backend_for_session(
        &self,
        session_id: SessionId,
    ) -> Result<Option<TaskExecutionHandler>, String> {
        // backends is the only authoritative controller-session task backend registry.
        // backends 是唯一权威的控制器会话任务后端注册表。
        let backends = self.backends.read().map_err(|_| {
            format!(
                "The task backend registry is locked and cannot inspect session `{session_id}`."
            )
        })?;
        Ok(backends
            .get(&session_id)
            .map(|registration| Arc::clone(&registration.value)))
    }

    /// Removes one backend handler only when the caller owns the current registration generation.
    /// 仅当调用方拥有当前注册世代时，从注册表中移除一个后端处理器。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` identifies the controller-session registry key.
    /// `session_id` 标识控制器会话注册表键。
    /// `registration_id` identifies the exact backend generation owned by the dropping guard.
    /// `registration_id` 标识正在销毁的守卫所拥有的精确后端世代。
    ///
    /// # Returns
    /// 返回值
    /// Success when the owned generation is absent or removed, otherwise a registry-lock error.
    /// 所属世代已不存在或已移除时返回成功，否则返回注册表锁错误。
    pub(super) fn remove_backend(
        &self,
        session_id: SessionId,
        registration_id: Uuid,
    ) -> Result<(), String> {
        let mut backends = self.backends.write().map_err(|_| {
            format!(
                "The task backend registry is locked and cannot remove the backend for session `{session_id}`."
            )
        })?;
        // ownsCurrentRegistration prevents a stale guard from deleting a newer replacement.
        // ownsCurrentRegistration 防止过期守卫删除更新的替代注册项。
        let owns_current_registration = backends
            .get(&session_id)
            .is_some_and(|registration| registration.registration_id == registration_id);
        if owns_current_registration {
            backends.remove(&session_id);
        }
        Ok(())
    }

    /// Returns the transaction lock serializing mutable execution state for one stable task.
    /// 返回用于串行化单个稳定任务可变执行状态的事务锁。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the stable task transaction domain.
    /// `task_id` 标识稳定任务事务域。
    ///
    /// # Returns
    /// 返回值
    /// The shared per-task lock, or an error when the lock registry is poisoned.
    /// 返回共享的按任务锁，锁注册表污染时返回错误。
    pub(super) fn task_execution_state_lock(
        &self,
        task_id: SessionId,
    ) -> Result<Arc<Mutex<()>>, String> {
        let mut locks = self.task_execution_state_locks.lock().map_err(|_| {
            String::from(
                "The task execution transaction registry is locked and cannot coordinate state ownership.",
            )
        })?;
        locks.retain(|_, transaction_lock| transaction_lock.strong_count() > 0);
        if let Some(transaction_lock) = locks.get(&task_id).and_then(Weak::upgrade) {
            return Ok(transaction_lock);
        }
        let transaction_lock = Arc::new(Mutex::new(()));
        locks.insert(task_id, Arc::downgrade(&transaction_lock));
        Ok(transaction_lock)
    }

    /// Resolves deduplicated task transaction locks in stable identifier order.
    /// 按稳定标识顺序解析去重后的任务事务锁。
    ///
    /// # Parameters
    /// 参数
    /// - `task_ids`: Stable tasks that must share one multi-task transaction boundary.
    /// - `task_ids`：必须共享同一多任务事务边界的稳定任务。
    ///
    /// # Returns
    /// 返回值
    /// Lock handles ordered by task id so every caller acquires multi-task transactions consistently.
    /// 按任务标识排序的锁句柄，使全部调用方以一致顺序取得多任务事务。
    pub(super) fn ordered_task_execution_state_locks(
        &self,
        task_ids: &[SessionId],
    ) -> Result<Vec<Arc<Mutex<()>>>, String> {
        task_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|task_id| self.task_execution_state_lock(task_id))
            .collect()
    }

    /// Atomically validates prepared-record ownership and reserves the active execution owner.
    /// 原子校验准备记录所有权并预留活动执行所有者。
    ///
    /// # Parameters
    /// 参数
    /// `request` carries the stable task, concrete execution, and immutable V1 admission facts.
    /// `request` 携带稳定任务、具体执行与不可变 V1 准入事实。
    ///
    /// # Returns
    /// 返回值
    /// The active-run guard, or an ownership/lock error for a superseded execution.
    /// 返回活动运行守卫；执行已被取代或锁失败时返回错误。
    pub(super) fn claim_and_reserve_active_task_run(
        self: &Arc<Self>,
        request: &TaskExecutionRequest,
    ) -> Result<ActiveTaskRunGuard, String> {
        let task_id = request.task_id;
        let transaction_lock = self.task_execution_state_lock(task_id)?;
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from(
                "The task execution transaction is locked and cannot claim the prepared run.",
            )
        })?;
        self.claim_active_task_run_with_transaction_lock_held(request)
    }

    /// Claims the exact request while its caller retains the task transaction lock.
    /// 调用方保持任务事务锁时认领精确请求。
    /// `request` supplies execution facts; returns the exclusive lease without admitting another run.
    /// `request` 提供执行事实；返回排他租约，不准入另一运行。
    /// Recovery uses this entry to install its missing record in the same transaction.
    /// 恢复使用此入口在同一事务中安装缺失记录。
    pub(super) fn claim_active_task_run_with_transaction_lock_held(
        self: &Arc<Self>,
        request: &TaskExecutionRequest,
    ) -> Result<ActiveTaskRunGuard, String> {
        // Both ownership coordinates are taken from the same immutable request.
        // 两个归属坐标均来自同一不可变请求。
        let task_id = request.task_id;
        let execution_id = request.execution_id.as_str();
        self.ensure_no_auxiliary_execution_with_transaction_lock_held(task_id)?;
        // Duplicate delivery must not replace or abort the original physical owner, even for the same execution.
        // 重复投递不能替换或中止原物理所有者，即使执行标识相同。
        if self.task_has_active_or_starting_run(task_id)? {
            return Err(format!(
                "Task `{task_id}` already owns an active or starting execution."
            ));
        }
        self.claim_task_execution_record_owner(task_id, execution_id)?;
        let guard = self.reserve_active_task_run(task_id, execution_id)?;
        let mut active_task_runs = self.active_task_runs.write().map_err(|_| {
            String::from("The active task registry is locked and cannot retain V1 execution facts.")
        })?;
        let entry = active_task_runs.get_mut(&task_id).ok_or_else(|| {
            format!("Task `{task_id}` lost its active execution before V1 facts were retained.")
        })?;
        if entry.run_id != guard.run_id || entry.execution_id != execution_id {
            return Err(format!(
                "Task `{task_id}` active execution changed before V1 facts were retained."
            ));
        }
        entry.v1_execution_facts = Some(TaskV1ExecutionFacts::from_request(
            request,
            current_unix_millis_u64(),
        ));
        drop(active_task_runs);
        Ok(guard)
    }

    /// Installs one trusted durable collaboration binding for a concrete execution attempt.
    /// 为一次具体执行尝试安装一条可信可靠协作绑定。
    ///
    /// Replaying the same reservation is idempotent; a different reservation cannot replace an
    /// execution that may already be waiting for terminal settlement.
    /// 重放同一预留是幂等的；不同预留不能替换可能正在等待终态结算的执行。
    pub(crate) fn register_child_task_lifecycle_binding(
        &self,
        binding: ChildTaskLifecycleBinding,
    ) -> Result<(), String> {
        let execution_id = binding.reservation.execution.execution_id.clone();
        let mut bindings = self.collaboration_lifecycle_bindings.write().map_err(|_| {
            String::from(
                "The collaboration lifecycle binding registry is locked and cannot register the child execution.",
            )
        })?;
        if let Some(existing) = bindings.get(&execution_id) {
            if existing.reservation == binding.reservation
                && Arc::ptr_eq(&existing.port, &binding.port)
            {
                return Ok(());
            }
            return Err(format!(
                "Execution `{execution_id}` already owns a different collaboration lifecycle binding."
            ));
        }
        bindings.insert(execution_id, binding);
        Ok(())
    }

    /// Takes the exact lifecycle binding once the corresponding execution starts.
    /// 在对应执行启动时取出该执行的精确生命周期绑定。
    pub(crate) fn take_child_task_lifecycle_binding(
        &self,
        execution_id: &str,
    ) -> Result<Option<ChildTaskLifecycleBinding>, String> {
        self.collaboration_lifecycle_bindings
            .write()
            .map_err(|_| {
                String::from(
                    "The collaboration lifecycle binding registry is locked and cannot claim the child execution.",
                )
            })
            .map(|mut bindings| bindings.remove(execution_id))
    }

    /// Reserves one exact child execution before DelegateStarted is published.
    /// 在发布 DelegateStarted 前预留一个精确子执行。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the stable child task.
    /// `task_id` 标识稳定子任务。
    /// `execution_id` identifies the concrete protocol execution being started.
    /// `execution_id` 标识正在启动的具体协议执行。
    ///
    /// # Returns
    /// 返回值
    /// An ownership guard for the new execution, or an error when the registry is locked.
    /// 返回新执行的所有权守卫；注册表被锁定时返回错误。
    pub(super) fn reserve_active_task_run(
        self: &Arc<Self>,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<ActiveTaskRunGuard, String> {
        let run_id = Uuid::new_v4();
        let mut active_task_runs = self.active_task_runs.write().map_err(|_| {
            String::from("The active task registry is locked and cannot register the subagent run.")
        })?;
        let replaced_handle = active_task_runs
            .insert(
                task_id,
                ActiveTaskRunEntry {
                    run_id,
                    execution_id: execution_id.to_string(),
                    v1_execution_facts: None,
                    abort_handle: None,
                    stop_requested: false,
                },
            )
            .and_then(|entry| entry.abort_handle);
        drop(active_task_runs);
        if let Some(replaced_handle) = replaced_handle {
            replaced_handle.abort();
        }
        Ok(ActiveTaskRunGuard {
            runtime: Arc::clone(self),
            task_id,
            run_id,
            released: false,
        })
    }

    /// Claims the stable task record for one concrete execution before runtime start events escape.
    /// 在运行时启动事件逸出前，为一个具体执行认领稳定任务记录。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the stable task record.
    /// `task_id` 标识稳定任务记录。
    /// `execution_id` identifies the execution attempting to start.
    /// `execution_id` 标识尝试启动的执行。
    ///
    /// # Returns
    /// 返回值
    /// Success for the current or unowned record, or a prestart cancellation for stale ownership.
    /// 当前或无所有者记录返回成功；所有权过期时返回启动前取消错误。
    pub(super) fn claim_task_execution_record_owner(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from(
                "The task session registry is locked and cannot validate execution ownership.",
            )
        })?;
        let Some(record) = sessions.get_mut(&task_id) else {
            return Ok(());
        };
        if task_session_status_from_record_status(record.last_status.as_str())?
            == TaskSessionStatus::Archived
        {
            return Err(format!(
                "Task `{task_id}` is archived and cannot claim execution `{execution_id}`."
            ));
        }
        match record.execution_id.as_deref() {
            Some(current_execution_id) if current_execution_id != execution_id => {
                Err(task_execution_record_owner_conflict_error(
                    task_id,
                    execution_id,
                    current_execution_id,
                ))
            }
            Some(_) => Ok(()),
            None => {
                record.execution_id = Some(execution_id.to_string());
                Ok(())
            }
        }
    }

    /// Returns whether one invocation may commit a status to its stable record.
    /// 返回某次调用是否可以向其稳定记录提交指定状态。
    ///
    /// # Parameters
    /// 参数
    /// `invocation` carries the stable task and concrete execution identities.
    /// `invocation` 携带稳定任务与具体执行身份。
    /// `incoming_status` is the state this execution is attempting to commit.
    /// `incoming_status` 是本执行尝试提交的状态。
    ///
    /// # Returns
    /// 返回值
    /// Whether ownership and explicit lifecycle state allow the commit, or a registry-lock error.
    /// 返回所有权与显式生命周期状态是否允许提交，或返回注册表锁错误。
    pub(super) fn task_execution_can_commit_status(
        &self,
        invocation: &TaskExecutionRequest,
        incoming_status: TaskSessionStatus,
    ) -> Result<bool, String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from(
                "The task session registry is locked and cannot inspect execution ownership.",
            )
        })?;
        let Some(record) = sessions.get_mut(&invocation.task_id) else {
            return Ok(true);
        };
        let current_status = task_session_status_from_record_status(record.last_status.as_str())?;
        if current_status == TaskSessionStatus::Archived {
            return Ok(false);
        }
        let owns_record = match record.execution_id.as_deref() {
            Some(execution_id) => execution_id == invocation.execution_id,
            None => {
                record.execution_id = Some(invocation.execution_id.clone());
                true
            }
        };
        if !owns_record {
            return Ok(false);
        }
        Ok(match current_status {
            TaskSessionStatus::Archived => false,
            TaskSessionStatus::Stopped => incoming_status == TaskSessionStatus::Stopped,
            TaskSessionStatus::Created
            | TaskSessionStatus::Running
            | TaskSessionStatus::WaitingChildren
            | TaskSessionStatus::WaitingFollowup
            | TaskSessionStatus::Compacting
            | TaskSessionStatus::Completed
            | TaskSessionStatus::Failed => true,
        })
    }

    /// Attaches an abort handle to the exact reserved child execution.
    /// 将中止句柄附加到精确的已预留子执行。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the stable child task.
    /// `task_id` 标识稳定子任务。
    /// `run_id` identifies the internal reservation owner.
    /// `run_id` 标识内部预留所有者。
    /// `handle` aborts the concrete provider future.
    /// `handle` 用于中止具体 Provider future。
    ///
    /// # Returns
    /// 返回值
    /// Success when the reservation still matches, otherwise an identity or lock error.
    /// 预留仍匹配时返回成功，否则返回身份或锁错误。
    pub(super) fn attach_active_task_run_abort_handle(
        &self,
        task_id: SessionId,
        run_id: Uuid,
        handle: AbortHandle,
    ) -> Result<(), String> {
        let should_abort = {
            let mut active_task_runs = self.active_task_runs.write().map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot attach the subagent abort handle.",
                )
            })?;
            let entry = active_task_runs.get_mut(&task_id).ok_or_else(|| {
                format!("Task `{task_id}` no longer owns the reserved active execution.")
            })?;
            if entry.run_id != run_id {
                return Err(format!(
                    "Task `{task_id}` active execution reservation was replaced before its abort handle attached."
                ));
            }
            let should_abort = entry.stop_requested;
            entry.abort_handle = Some(handle.clone());
            should_abort
        };
        if should_abort {
            handle.abort();
        }
        Ok(())
    }

    /// Returns whether one exact execution currently owns the task active-run reservation.
    /// 返回某个精确执行当前是否持有任务活动运行预留。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the stable child task.
    /// `task_id` 标识稳定子任务。
    /// `execution_id` identifies the protocol execution to compare.
    /// `execution_id` 标识需要比较的协议执行。
    ///
    /// # Returns
    /// 返回值
    /// Whether the exact execution owns the current reservation, or a registry-lock error.
    /// 返回精确执行是否持有当前预留，或返回注册表锁错误。
    pub fn is_task_execution_current(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<bool, String> {
        Ok(self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot inspect the current subagent execution.",
                )
            })?
            .get(&task_id)
            .is_some_and(|entry| entry.execution_id == execution_id))
    }

    /// Returns immutable V1 admission facts for one exact current child execution.
    /// 返回一次精确当前子执行的不可变 V1 准入事实。
    ///
    /// # Parameters / 参数
    ///
    /// - `task_id`: Stable child TaskSession identity.
    ///   `task_id`：稳定子 TaskSession 身份。
    /// - `execution_id`: Concrete execution identity that must still own the run.
    ///   `execution_id`：必须仍拥有运行权的具体执行身份。
    ///
    /// # Returns / 返回值
    ///
    /// The exact facts retained before `DelegateStarted`, or an explicit stale/missing error.
    /// 在 `DelegateStarted` 前保留的精确事实；执行过期或事实缺失时返回显式错误。
    pub fn active_task_v1_execution_facts(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<TaskV1ExecutionFacts, String> {
        let active_task_runs = self.active_task_runs.read().map_err(|_| {
            String::from("The active task registry is locked and cannot read V1 execution facts.")
        })?;
        let entry = active_task_runs
            .get(&task_id)
            .ok_or_else(|| format!("Task `{task_id}` has no active execution for V1 admission."))?;
        if entry.execution_id != execution_id {
            return Err(format!(
                "Task `{task_id}` active execution is `{}`, not requested `{execution_id}`.",
                entry.execution_id
            ));
        }
        entry.v1_execution_facts.clone().ok_or_else(|| {
            format!(
                "Task `{task_id}` execution `{execution_id}` has no retained V1 admission facts."
            )
        })
    }

    /// Forwards a host event from compaction through the original child's exact detached listener.
    /// 通过原子任务的精确分离监听器转发来自压缩的宿主事件。
    /// `task_id` and `execution_id` bind the active owner; `event` retains its original barrier. Returns delivery failure explicitly.
    /// `task_id` 与 `execution_id` 绑定活动所有者；`event` 保留原屏障。投递失败明确返回。
    pub fn publish_child_compaction_host_event(
        &self,
        task_id: SessionId,
        execution_id: &str,
        event: RuntimeEvent,
    ) -> Result<(), String> {
        self.active_task_v1_execution_facts(task_id, execution_id)?;
        self.publish_detached_task_event(
            task_id,
            execution_id,
            DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
                session_id: task_id,
                event,
            }),
        )
    }

    /// Removes one active child-task run only when the caller owns the current reservation generation.
    /// 仅当调用方拥有当前预留世代时移除一条活动子任务运行。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` identifies the stable child task.
    /// `task_id` 标识稳定子任务。
    /// `run_id` identifies the exact reservation generation owned by the guard.
    /// `run_id` 标识守卫拥有的精确预留世代。
    ///
    /// # Returns
    /// 返回值
    /// Success when the owned generation is absent or removed, otherwise a registry-lock error.
    /// 所属世代已不存在或已移除时返回成功，否则返回注册表锁错误。
    pub(super) fn remove_active_task_run(
        &self,
        task_id: SessionId,
        run_id: Uuid,
    ) -> Result<(), String> {
        let mut active_task_runs = self.active_task_runs.write().map_err(|_| {
            format!(
                "The active task registry is locked and cannot remove the run for task `{task_id}`."
            )
        })?;
        let should_remove = active_task_runs
            .get(&task_id)
            .is_some_and(|entry| entry.run_id == run_id);
        if should_remove {
            active_task_runs.remove(&task_id);
        }
        Ok(())
    }

    /// Returns whether any known descendant task still has an active provider run.
    /// 返回任一已知后代任务是否仍持有活动 Provider 运行。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the completed parent task whose descendant runs control detached-stream lifetime.
    /// `task_id` 是已完成的父任务，其后代运行决定分离流生命周期。
    ///
    /// # Returns
    /// 返回值
    /// Returns the authoritative descendant-run fact, or an explicit registry-read error when that fact is unavailable.
    /// 返回权威后代运行事实；事实不可用时返回明确的注册表读取错误。
    pub(super) fn task_has_active_descendant_runs(
        &self,
        task_id: SessionId,
    ) -> Result<bool, String> {
        // task_ids is the authoritative subtree snapshot used for the descendant-only query.
        // task_ids 是仅查询后代运行时使用的权威子树快照。
        let task_ids = self.task_subtree_ids(task_id)?;
        // active_runs is the authoritative set of child provider runs still owned by the runtime.
        // active_runs 是运行时仍持有的子 Provider 运行权威集合。
        let active_runs = self.active_task_runs.read().map_err(|_| {
            String::from("The active task registry is locked and cannot inspect descendant runs.")
        })?;
        Ok(task_ids
            .into_iter()
            .filter(|candidate_task_id| *candidate_task_id != task_id)
            .any(|candidate_task_id| active_runs.contains_key(&candidate_task_id)))
    }

    /// Publishes one detached event to the exact execution stream, buffering it before subscription.
    /// 向精确执行流发布一条分离事件，并在订阅前进行缓冲。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the stable child task session.
    /// `task_id` 是稳定子任务会话。
    /// `execution_id` is the unique execution attempt that produced the event.
    /// `execution_id` 是产出事件的唯一执行尝试。
    /// `event` is the child or parent runtime event to deliver.
    /// `event` 是要投递的子级或父级运行时事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after buffering or delivering the event, or an explicit registry or unopened-stream error.
    /// 事件完成缓冲或投递后返回成功；注册表失败或流未打开时返回明确错误。
    pub(super) fn publish_detached_task_event(
        &self,
        task_id: SessionId,
        execution_id: &str,
        event: DetachedTaskRuntimeEvent,
    ) -> Result<(), String> {
        // streams provides exclusive authority for buffering and live delivery to the exact execution.
        // streams 为精确执行的缓冲与实时投递提供独占权限。
        let mut streams = self.detached_task_event_streams.write().map_err(|_| {
            format!(
                "The detached task stream registry is locked and cannot publish task `{task_id}` execution `{execution_id}`."
            )
        })?;
        // streamKey identifies the exact stream that must already have passed the explicit open boundary.
        // streamKey 标识必须已经通过显式打开边界的精确事件流。
        let stream_key = DetachedTaskExecutionKey::new(task_id, execution_id);
        // state is the existing execution stream; publication never creates lifecycle state implicitly.
        // state 是已有的执行流；发布操作绝不会隐式创建生命周期状态。
        let Some(state) = streams.get_mut(&stream_key) else {
            return Err(format!(
                "The detached task stream for task `{task_id}` execution `{execution_id}` is not open and cannot accept events."
            ));
        };
        if state.closed {
            return Err(format!(
                "The detached task stream for task `{task_id}` execution `{execution_id}` is closed and cannot accept events."
            ));
        }
        if state.subscribers.is_empty() {
            state.buffered_events.push(event);
            return Ok(());
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
        Ok(())
    }

    /// Emits startup failure only to existing listeners, then removes the exact stream atomically.
    /// 仅向已有监听器发送启动失败，随后原子移除精确事件流。
    /// The caller has released its active lease; absent listeners never leave an orphan buffer.
    /// 调用方已经释放活动租约；不存在监听器时绝不留下孤立缓冲。
    /// Returns success after ordered delivery/EOF, or an explicit registry-lock error.
    /// 有序投递与流结束后返回成功，否则返回明确注册表锁错误。
    pub(super) fn abort_detached_task_startup(
        &self,
        request: &TaskExecutionRequest,
        reason: &str,
    ) -> Result<(), String> {
        // Freeze the same typed host failure used by normal runtime terminal publication.
        // 冻结正常运行时终态发布所用同类强类型宿主失败。
        let event = DetachedTaskRuntimeEvent::StartupAborted {
            parent_session_id: request.current_session_id,
            event: parent_events::build_task_failure_parent_runtime_event(request, reason),
        };
        // Removal and delivery share one lock, so no late subscriber can inherit an orphan abort.
        // 移除与投递共用一把锁，防止迟到订阅者接收孤立中止事件。
        let mut streams = self.detached_task_event_streams.write().map_err(|_| format!(
            "The detached task stream registry is locked and cannot abort task `{}` execution `{}`.",
            request.task_id, request.execution_id,
        ))?;
        // Taking the exact row drops all senders only after the abort has entered each live queue.
        // 取走精确记录后，仅在中止进入每个活动队列之后释放全部发送端。
        if let Some(stream) = streams.remove(&DetachedTaskExecutionKey::new(
            request.task_id,
            &request.execution_id,
        )) {
            for sender in stream.subscribers {
                // A dropped receiver has no remaining host owner; never buffer for a future execution.
                // 已销毁接收端不再拥有宿主责任；绝不为未来执行缓冲。
                let _ = sender.send(event.clone());
            }
        }
        Ok(())
    }

    /// Closes only one exact detached execution stream and drops its active senders.
    /// 仅关闭一条精确分离执行流并释放其活动发送端。
    ///
    /// # Parameters
    /// 参数
    /// `task_id` is the stable child task session.
    /// `task_id` 是稳定子任务会话。
    /// `execution_id` is the unique execution attempt reaching terminal state.
    /// `execution_id` 是到达终态的唯一执行尝试。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the exact stream is closed or already absent, or an explicit registry-lock error.
    /// 精确流关闭或已经不存在时返回成功；注册表锁失败时返回明确错误。
    pub(super) fn close_detached_task_event_stream(
        &self,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<(), String> {
        // streams provides exclusive authority for the exact stream terminal transition.
        // streams 为精确流终态转换提供独占权限。
        let mut streams = self.detached_task_event_streams.write().map_err(|_| {
            format!(
                "The detached task stream registry is locked and cannot close task `{task_id}` execution `{execution_id}`."
            )
        })?;
        // stream_key isolates the exact execution whose terminal state may change.
        // stream_key 隔离允许变更终态的精确执行。
        let stream_key = DetachedTaskExecutionKey::new(task_id, execution_id);
        // should_remove records whether no buffered terminal replay state must remain in the registry.
        // should_remove 记录注册表中是否无需继续保留缓冲终态回放状态。
        let should_remove = if let Some(state) = streams.get_mut(&stream_key) {
            state.closed = true;
            !state.subscribers.is_empty() || state.buffered_events.is_empty()
        } else {
            false
        };
        if should_remove {
            streams.remove(&stream_key);
        }
        Ok(())
    }
}
