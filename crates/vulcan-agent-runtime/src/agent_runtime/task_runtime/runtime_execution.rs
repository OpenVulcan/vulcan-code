use super::*;

/// Owns cleanup for one detached execution stream until the background finalizer takes over.
/// 在后台终态处理器接管前，持有一条分离执行流的清理责任。
pub(super) struct DetachedTaskStreamStartupGuard {
    /// Shared task runtime that owns the detached stream registry.
    /// 持有分离流注册表的共享任务运行时。
    runtime: Arc<TaskRuntime>,
    /// Stable child task whose exact execution stream was opened.
    /// 已打开精确执行流的稳定子任务。
    task_id: SessionId,
    /// Concrete execution identity isolated from every resumed run.
    /// 与所有续跑执行隔离的具体执行标识。
    execution_id: String,
    /// Whether this guard still owns responsibility for closing the opened stream.
    /// 本守卫是否仍持有关闭已打开事件流的责任。
    owns_cleanup: bool,
}

impl DetachedTaskStreamStartupGuard {
    /// Opens one exact detached stream and assumes startup cleanup ownership.
    /// 打开一条精确分离流并取得启动阶段清理所有权。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the detached stream registry and later background finalizer.
    /// `runtime` 持有分离流注册表以及后续后台终态处理器。
    /// `task_id` identifies the stable child task.
    /// `task_id` 标识稳定子任务。
    /// `execution_id` identifies the concrete execution attempt.
    /// `execution_id` 标识具体执行尝试。
    ///
    /// # Returns
    /// 返回值
    /// A cleanup owner after the exact stream opens, or the registry open error.
    /// 精确流打开后返回清理所有者，否则返回注册表打开错误。
    pub(super) fn open(
        runtime: Arc<TaskRuntime>,
        task_id: SessionId,
        execution_id: &str,
    ) -> Result<Self, String> {
        runtime.open_detached_task_event_stream_for_run(task_id, execution_id)?;
        Ok(Self {
            runtime,
            task_id,
            execution_id: execution_id.to_string(),
            owns_cleanup: true,
        })
    }

    /// Closes the startup-owned stream and preserves both the primary and cleanup failures.
    /// 关闭启动阶段持有的事件流，并同时保留主错误与清理错误。
    ///
    /// # Parameters
    /// 参数
    /// `primary_error` is the startup failure that requires the stream to close.
    /// `primary_error` 是要求关闭事件流的启动失败。
    ///
    /// # Returns
    /// 返回值
    /// The primary error when cleanup succeeds, otherwise a diagnostic containing both failures.
    /// 清理成功时返回主错误，否则返回同时包含两项失败的诊断。
    pub(super) fn close_after_error(mut self, primary_error: String) -> String {
        match self
            .runtime
            .close_detached_task_event_stream(self.task_id, self.execution_id.as_str())
        {
            Ok(()) => {
                self.owns_cleanup = false;
                primary_error
            }
            Err(close_error) => format!(
                "{primary_error} Detached stream cleanup for task `{}` execution `{}` also failed: {close_error}",
                self.task_id, self.execution_id
            ),
        }
    }

    /// Transfers stream-close responsibility to the successfully spawned background finalizer.
    /// 将事件流关闭责任移交给已经成功派生的后台终态处理器。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; consuming this guard prevents its Drop fallback from closing the live stream.
    /// 不返回值；消费本守卫会阻止其 Drop 兜底关闭仍在运行的事件流。
    pub(super) fn handoff_to_background_finalizer(mut self) {
        self.owns_cleanup = false;
    }
}

impl Drop for DetachedTaskStreamStartupGuard {
    /// Closes the startup-owned stream when an unhandled early exit unwinds the task start scope.
    /// 当未显式处理的提前退出展开子任务启动作用域时，关闭启动阶段持有的事件流。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; cleanup failure is emitted because Drop has no caller error channel.
    /// 不返回值；由于 Drop 没有调用方错误通道，清理失败会输出诊断。
    fn drop(&mut self) {
        if self.owns_cleanup
            && let Err(error) = self
                .runtime
                .close_detached_task_event_stream(self.task_id, self.execution_id.as_str())
        {
            eprintln!(
                "[task-runtime] detached stream startup guard cleanup failed for task `{}` execution `{}`: {error}",
                self.task_id, self.execution_id
            );
        }
    }
}

/// Runs one child task through the shared runtime preparation and provider-dispatch bridge.
/// 通过共享运行时准备与 Provider 分发桥执行一条子代理任务。
pub(crate) async fn run_task_with_shared_runtime_context(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    base_controller_config: CoreControllerConfig,
    parent_live_sender: ProviderLiveDeltaSender,
    request: TaskExecutionRequest,
) -> Result<TaskExecutionOutcome, String> {
    let mut request = request;
    let task_id = request.task_id;
    // active_task_run_guard reserves this protocol execution before any start event can escape.
    // active_task_run_guard 在任何启动事件逸出前预留本协议执行。
    let active_task_run_guard =
        task_runtime.claim_and_reserve_active_task_run(task_id, &request.execution_id)?;
    let trace_started_at = current_unix_millis_u64();
    task_runtime_trace(
        "shared_task_start",
        format!(
            "parent={} task={} public_task_id={} depth={}",
            request.current_session_id, request.task_id, request.public_task_id, request.depth
        ),
    );
    if provider_model_has_override(&request.invocation_config.provider_model) {
        validate_explicit_spawn_model_selector(
            &context.config,
            context.foundation.as_ref(),
            &request.invocation_config.provider_model,
        )?;
    }
    let fixed_parent_controller_config = request
        .controller_config
        .as_ref()
        .unwrap_or(&base_controller_config);
    let mut resolved_config = task_runtime.resolve_task_session_config_for_execution(
        fixed_parent_controller_config,
        &request.invocation_config,
    )?;
    if !provider_model_has_override(&request.invocation_config.provider_model) {
        clear_unavailable_profile_model_override(
            &context.config,
            context.foundation.as_ref(),
            &mut resolved_config,
        )?;
    }
    task_runtime_trace(
        "shared_task_config_resolved",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    let running_controller_config = build_task_controller_config(
        fixed_parent_controller_config,
        request.controller_config.clone(),
        &request.description,
        &request.subagent,
        &request.invocation_config,
        &resolved_config,
    );
    request.controller_config = Some(running_controller_config.clone());
    let started_payload = build_task_host_payload(
        &request,
        "running",
        0,
        0,
        None,
        "",
        &[],
        current_unix_millis_text(),
    );
    // detachedStreamStartupGuard owns stream cleanup until the background finalizer is installed.
    // detachedStreamStartupGuard 在后台终态处理器安装前持有事件流清理责任。
    let detached_stream_startup_guard = DetachedTaskStreamStartupGuard::open(
        Arc::clone(&task_runtime),
        task_id,
        &request.execution_id,
    )?;
    let (delegate_started_event, projection_barrier) =
        attach_runtime_event_projection_barrier(build_task_delegate_runtime_event(
            RuntimeEventKind::DelegateStarted,
            &request,
            "Running and waiting for the subagent to return a result.",
            started_payload,
        ))?;
    if parent_live_sender
        .send(Ok(ProviderStreamDelta::RuntimeEvent(
            delegate_started_event,
        )))
        .is_err()
    {
        // barrierCancellationError preserves cleanup failure without replacing the parent-stream failure.
        // barrierCancellationError 保留清理失败且不替换父实时流失败。
        let barrier_cancellation_error =
            cancel_runtime_event_projection_barrier(projection_barrier).err();
        // parentStreamError is the primary protocol failure that prevented the child run from starting.
        // parentStreamError 是阻止子运行启动的主要协议错误。
        let parent_stream_error = match barrier_cancellation_error {
            Some(cancel_error) => format!(
                "{} Projection barrier cancellation also failed: {cancel_error}",
                task_start_parent_stream_closed_error(&request.public_task_id)
            ),
            None => task_start_parent_stream_closed_error(&request.public_task_id),
        };
        task_runtime_trace(
            "shared_task_delegate_started_parent_stream_closed",
            format!("task={} public_task_id={}", task_id, request.public_task_id),
        );
        return Err(detached_stream_startup_guard.close_after_error(parent_stream_error));
    }
    // Wait until the parent workbench projection sees the subagent-start event before the child run advances.
    // 等待父级工作台投影看到子代理启动事件后，再推进子任务执行。
    let has_projection_barrier = projection_barrier.is_some();
    wait_runtime_event_projection_barrier(projection_barrier).await?;
    if !has_projection_barrier {
        tokio::task::yield_now().await;
    }
    task_runtime_trace(
        "shared_task_delegate_started",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );

    let background_task_runtime = Arc::clone(&task_runtime);
    let background_request = request.clone();
    let background_resolved_config = resolved_config.clone();
    let child_completion_controller_config = running_controller_config.clone();
    let stopped_controller_config = running_controller_config.clone();
    let compaction_runtime_context = context.clone();
    let running_outcome = build_running_task_execution_outcome(
        task_id,
        running_controller_config.clone(),
        resolved_config.clone(),
        request.snapshot.clone(),
    );
    if let Err(error) = task_runtime.store_running_task_outcome_in_memory(
        request.current_session_id,
        &request,
        &running_outcome,
    ) {
        return Err(detached_stream_startup_guard.close_after_error(error));
    }
    task_runtime_trace(
        "shared_task_running_stored_in_memory",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    let child_run_task = tokio::spawn(async move {
        let mut background_request = background_request;
        let launch = prepare_task_runtime_stream_launch_with_preflight_compaction(
            Arc::clone(&background_task_runtime),
            &context,
            &mut background_request,
            &background_resolved_config,
        )
        .await?;
        let mut terminal_report = None;
        let config = launch
            .task_execution_context
            .as_ref()
            .map(|context| context.config.clone())
            .unwrap_or_else(|| context.config.clone());
        let prompt_intervention = Some(build_child_agent_prompt_intervention(
            Arc::clone(&background_task_runtime),
            background_request.task_id,
        ));
        let mut child_event_stream = Box::pin(drive_runtime_stream_provider_loop(
            config,
            launch,
            prompt_intervention,
        ));
        while let Some(event) = child_event_stream.next().await {
            if let ExecutionStreamEvent::Finished(report) = &event {
                terminal_report = Some(report.clone());
            }
            background_task_runtime.publish_detached_task_event(
                background_request.task_id,
                &background_request.execution_id,
                DetachedTaskRuntimeEvent::ChildExecution(event),
            )?;
        }
        let report = terminal_report.ok_or_else(|| {
            format!(
                "Task `{}` finished without a terminal execution report.",
                background_request.task_id
            )
        })?;
        let budget_should_compact = report
            .preparation
            .as_ref()
            .and_then(|preparation| preparation.context_budget.as_ref())
            .is_some_and(|context_budget| context_budget.should_compact());
        let provider_limit_should_compact =
            task_provider_request_limit_should_compact_after_current_report(
                &background_task_runtime,
                background_request.root_session_id,
                background_request.task_id,
                &background_resolved_config.effective_config.context_compaction,
                report.provider_request_count,
            )
            .unwrap_or_else(|error| {
                eprintln!(
                    "[task-session-store] provider-request compaction check failed for task `{}`: {error}",
                    background_request.public_task_id
                );
                false
            });
        let should_compact_context = budget_should_compact || provider_limit_should_compact;
        let outcome = task_execution_outcome_from_report(
            &background_request,
            child_completion_controller_config,
            background_resolved_config,
            report,
        )?;
        Ok::<TaskExecutionCompletion, String>(TaskExecutionCompletion {
            outcome,
            should_compact_context,
        })
    });
    let child_run_abort_handle = child_run_task.abort_handle();
    let child_run_abort_guard = AbortChildRunOnDrop {
        handle: child_run_abort_handle.clone(),
    };
    if let Err(error) = active_task_run_guard.attach_abort_handle(child_run_abort_handle) {
        return Err(detached_stream_startup_guard.close_after_error(error));
    }
    task_runtime_trace(
        "shared_task_child_spawned",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    tokio::spawn(async move {
        let mut active_task_run_guard = Some(active_task_run_guard);
        let _child_run_abort_guard = child_run_abort_guard;
        let child_run_result = child_run_task.await;
        let resumed_queued_messages = match child_run_result {
            Ok(Ok(completion)) => {
                let outcome = completion.outcome;
                let store_result =
                    task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
                match combine_task_run_finalization_results(
                    store_result.map_err(|error| {
                        format!(
                            "The subagent task completed, but its terminal state could not be recorded: {error}"
                        )
                    }),
                    release_owned_active_task_run(&mut active_task_run_guard, task_id).map_err(
                        |error| {
                            format!(
                                "The subagent task completed, but its active-run reservation could not be released: {error}"
                            )
                        },
                    ),
                ) {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_released(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_finished_to_parent(
                            &task_runtime,
                            &request,
                            &outcome,
                            "The subagent task has completed.",
                        );
                        if completion.should_compact_context {
                            spawn_task_session_context_compaction(
                                Arc::clone(&task_runtime),
                                compaction_runtime_context,
                                request.clone(),
                                outcome.resolved_config.clone(),
                            );
                        }
                        resumed_queued_messages
                    }
                    Err(detail) => {
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Ok(Err(error)) if task_execution_error_is_stopped(error.as_str()) => {
                let stopped_snapshot = stopped_task_snapshot_from_store_or_request(
                    &task_runtime,
                    request.root_session_id,
                    task_id,
                    &request,
                )
                .unwrap_or_else(|| request.snapshot.clone());
                let outcome = build_stopped_task_execution_outcome(
                    task_id,
                    stopped_controller_config.clone(),
                    resolved_config,
                    stopped_snapshot,
                );
                let store_result =
                    task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
                match combine_task_run_finalization_results(
                    store_result.map_err(|error| {
                        format!(
                            "The stopped subagent task could not record its terminal state: {error}"
                        )
                    }),
                    release_owned_active_task_run(&mut active_task_run_guard, task_id).map_err(
                        |error| {
                            format!(
                                "The stopped subagent task could not release its active-run reservation: {error}"
                            )
                        },
                    ),
                ) {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_released(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_finished_to_parent(
                            &task_runtime,
                            &request,
                            &outcome,
                            outcome.final_output_text.as_str(),
                        );
                        resumed_queued_messages
                    }
                    Err(detail) => {
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Ok(Err(error)) => {
                let mark_result =
                    task_runtime.mark_task_failure(request.current_session_id, &request, &error);
                match combine_task_run_finalization_results(
                    mark_result.map_err(|record_error| {
                        format!(
                            "The subagent task failed with `{error}`, and recording that failure also failed: {record_error}"
                        )
                    }),
                    release_owned_active_task_run(&mut active_task_run_guard, task_id).map_err(
                        |release_error| {
                            format!(
                                "The subagent task failed with `{error}`, and releasing its active-run reservation also failed: {release_error}"
                            )
                        },
                    ),
                ) {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_released(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_failure_to_parent(&task_runtime, &request, &error);
                        resumed_queued_messages
                    }
                    Err(detail) => {
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Err(join_error) if join_error.is_cancelled() => {
                let stopped_snapshot = stopped_task_snapshot_from_store_or_request(
                    &task_runtime,
                    request.root_session_id,
                    task_id,
                    &request,
                )
                .unwrap_or_else(|| request.snapshot.clone());
                let outcome = build_stopped_task_execution_outcome(
                    task_id,
                    stopped_controller_config,
                    resolved_config,
                    stopped_snapshot,
                );
                let store_result =
                    task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
                match combine_task_run_finalization_results(
                    store_result.map_err(|error| {
                        format!(
                            "The cancelled subagent task could not record its stopped state: {error}"
                        )
                    }),
                    release_owned_active_task_run(&mut active_task_run_guard, task_id).map_err(
                        |error| {
                            format!(
                                "The cancelled subagent task could not release its active-run reservation: {error}"
                            )
                        },
                    ),
                ) {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_released(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_finished_to_parent(
                            &task_runtime,
                            &request,
                            &outcome,
                            outcome.final_output_text.as_str(),
                        );
                        resumed_queued_messages
                    }
                    Err(detail) => {
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
            Err(join_error) => {
                let error = format!(
                    "Task `{}` failed: {}",
                    task_id,
                    format_task_join_error(&join_error)
                );
                let mark_result =
                    task_runtime.mark_task_failure(request.current_session_id, &request, &error);
                match combine_task_run_finalization_results(
                    mark_result.map_err(|record_error| {
                        format!(
                            "The subagent task failed with `{error}`, and recording that failure also failed: {record_error}"
                        )
                    }),
                    release_owned_active_task_run(&mut active_task_run_guard, task_id).map_err(
                        |release_error| {
                            format!(
                                "The subagent task failed with `{error}`, and releasing its active-run reservation also failed: {release_error}"
                            )
                        },
                    ),
                ) {
                    Ok(()) => {
                        let resumed_queued_messages =
                            resume_queued_messages_after_active_run_released(
                                &task_runtime,
                                request.current_session_id,
                                task_id,
                            )
                            .await;
                        publish_task_failure_to_parent(&task_runtime, &request, &error);
                        resumed_queued_messages
                    }
                    Err(detail) => {
                        eprintln!("[task-runtime] {detail}");
                        publish_task_failure_to_parent(&task_runtime, &request, &detail);
                        false
                    }
                }
            }
        };
        if !resumed_queued_messages {
            match task_runtime.task_has_active_descendant_runs(task_id) {
                // A confirmed empty descendant set is the only authority allowed to close this execution stream.
                // 仅权威确认后代集合为空时，才允许关闭当前执行流。
                Ok(false) => {
                    // error exposes terminal stream-close failure because the background task has no caller return channel.
                    // error 暴露终态流关闭失败，因为后台任务没有调用方返回通道。
                    if let Err(error) = task_runtime
                        .close_detached_task_event_stream(task_id, &request.execution_id)
                    {
                        eprintln!(
                            "[task-runtime] detached stream close failed for completed task {task_id}: {error}"
                        );
                    }
                }
                // Active descendants keep the parent stream available for nested terminal delivery.
                // 存在活动后代时保留父流，以继续投递嵌套终态事件。
                Ok(true) => {}
                // error preserves the unavailable registry fact instead of converting it into a destructive close decision.
                // error 保留注册表事实不可用的诊断，不将其转换为破坏性关闭决策。
                Err(error) => {
                    eprintln!(
                        "[task-runtime] detached stream remains open because descendant-run state is unavailable for task {task_id}: {error}"
                    );
                }
            }
        }
        task_runtime_trace(
            "shared_task_background_completed",
            format!(
                "task={} elapsed_ms={}",
                task_id,
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
    });
    detached_stream_startup_guard.handoff_to_background_finalizer();
    task_runtime_trace(
        "shared_task_spawn_return",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    Ok(running_outcome)
}

/// Determines whether a task session crossed the provider-request compaction threshold after the current run.
/// 判断子代理会话在当前执行后是否达到 Provider 请求数压缩阈值。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the segmented TaskSession store roots.
/// `task_runtime` 持有分段 TaskSession 存储根目录。
/// `root_session_id` identifies the root workspace session that owns the task store.
/// `root_session_id` 标识拥有子任务存储的根工作区会话。
/// `task_id` identifies the child task session being measured.
/// `task_id` 标识正在计量的子代理会话。
/// `policy` is the resolved context-compaction policy for this child session.
/// `policy` 是该子代理会话解析后的上下文压缩策略。
/// `current_provider_requests` is the provider request count reported by the just-finished run.
/// `current_provider_requests` 是刚完成执行报告中的 Provider 请求次数。
///
/// # Returns
/// 返回值
/// `true` when automatic compaction is enabled and the cumulative count reaches the configured limit.
/// 当自动压缩启用且累计次数达到配置阈值时返回 `true`。
fn task_provider_request_limit_should_compact_after_current_report(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    task_id: SessionId,
    policy: &ContextCompactionPolicySnapshot,
    current_provider_requests: usize,
) -> Result<bool, String> {
    if !task_provider_request_limit_is_enabled(policy) {
        return Ok(false);
    }
    let previous_provider_requests =
        task_provider_requests_since_latest_compaction(task_runtime, root_session_id, task_id)?;
    Ok(task_provider_request_limit_reached(
        policy,
        previous_provider_requests.saturating_add(current_provider_requests),
    ))
}

/// Loads the provider request count accumulated after the latest completed task-context compaction.
/// 加载最近一次子代理上下文压缩完成后累计的 Provider 请求次数。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the segmented TaskSession store roots.
/// `task_runtime` 持有分段 TaskSession 存储根目录。
/// `root_session_id` identifies the root workspace session that owns the task store.
/// `root_session_id` 标识拥有子任务存储的根工作区会话。
/// `task_id` identifies the child task session being measured.
/// `task_id` 标识正在计量的子代理会话。
///
/// # Returns
/// 返回值
/// The sum of persisted provider turns after the latest completed compaction marker.
/// 返回最近一次完成压缩标记后的持久化 Provider 轮次数总和。
pub(super) fn task_provider_requests_since_latest_compaction(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    task_id: SessionId,
) -> Result<usize, String> {
    let Some(store) = task_runtime.task_session_store_for_root(root_session_id)? else {
        return Ok(0);
    };
    let events = store
        .load_events(SessionNodeId::from(task_id))
        .map_err(|error| error.message)?;
    Ok(task_provider_requests_since_latest_compaction_event(
        &events,
    ))
}

/// Sums provider requests from task events after the latest completed compaction marker.
/// 汇总最近一次完成压缩标记后的子任务事件 Provider 请求数。
///
/// # Parameters
/// 参数
/// `events` are the persisted events loaded for one TaskSession node.
/// `events` 是为单个子代理节点加载的持久化事件。
///
/// # Returns
/// 返回值
/// The provider turn count represented by task lifecycle events after the last compaction marker.
/// 返回最近一次压缩标记后由子任务生命周期事件表示的 Provider 轮次数。
pub(super) fn task_provider_requests_since_latest_compaction_event(
    events: &[SessionEventRecord],
) -> usize {
    let start_index = events
        .iter()
        .rposition(task_event_is_completed_context_compaction_marker)
        .map(|index| index.saturating_add(1))
        .unwrap_or(0);
    events[start_index..]
        .iter()
        .filter_map(task_event_provider_turns_executed)
        .sum()
}

/// Determines whether one task event is a completed context-compaction marker.
/// 判断单个子任务事件是否为已完成的上下文压缩标记。
///
/// # Parameters
/// 参数
/// `event` is the persisted TaskSession event being inspected.
/// `event` 是正在检查的持久化子代理事件。
///
/// # Returns
/// 返回值
/// `true` when the event marks a completed context compaction pass.
/// 当事件标记一次已完成的上下文压缩时返回 `true`。
fn task_event_is_completed_context_compaction_marker(event: &SessionEventRecord) -> bool {
    matches!(event.kind, SessionEventKind::RuntimeMarker)
        && event
            .payload
            .get("context_compaction_status")
            .and_then(Value::as_str)
            == Some("completed")
}

/// Extracts the persisted provider-turn count from one TaskSession lifecycle event.
/// 从单个子代理生命周期事件中提取持久化 Provider 轮次数。
///
/// # Parameters
/// 参数
/// `event` is the persisted TaskSession event being inspected.
/// `event` 是正在检查的持久化子代理事件。
///
/// # Returns
/// 返回值
/// The provider-turn count stored by terminal task status events.
/// 返回终态子任务状态事件保存的 Provider 轮次数。
fn task_event_provider_turns_executed(event: &SessionEventRecord) -> Option<usize> {
    let count = event.payload.get("provider_turns_executed")?.as_u64()?;
    usize::try_from(count).ok()
}

/// Determines whether the provider-request count reaches the resolved compaction limit.
/// 判断 Provider 请求数是否达到解析后的压缩阈值。
///
/// # Parameters
/// 参数
/// `policy` is the resolved context-compaction policy for this child session.
/// `policy` 是该子代理会话解析后的上下文压缩策略。
/// `provider_requests` is the count accumulated after the latest completed compaction.
/// `provider_requests` 是最近一次完成压缩后累计的请求次数。
///
/// # Returns
/// 返回值
/// `true` when automatic compaction is enabled and a positive request limit has been reached.
/// 当自动压缩启用且正数请求阈值已达到时返回 `true`。
pub(super) fn task_provider_request_limit_reached(
    policy: &ContextCompactionPolicySnapshot,
    provider_requests: usize,
) -> bool {
    task_provider_request_limit_is_enabled(policy)
        && policy
            .max_provider_requests
            .is_some_and(|limit| provider_requests >= limit)
}

/// Determines whether provider-request based context compaction is configured.
/// 判断是否已配置基于 Provider 请求次数的上下文压缩。
///
/// # Parameters
/// 参数
/// `policy` is the resolved context-compaction policy for this child session.
/// `policy` 是该子代理会话解析后的上下文压缩策略。
///
/// # Returns
/// 返回值
/// `true` when automatic compaction is enabled and a positive request limit exists.
/// 当自动压缩启用且存在正数请求阈值时返回 `true`。
pub(super) fn task_provider_request_limit_is_enabled(
    policy: &ContextCompactionPolicySnapshot,
) -> bool {
    policy.auto_compaction_enabled && policy.max_provider_requests.is_some_and(|limit| limit > 0)
}

/// Explicitly releases the active-run guard retained by the background finalizer.
/// 显式释放后台终态处理器保留的活动运行守卫。
///
/// # Parameters
/// 参数
/// `guard` owns the exact active-run generation until this function consumes it.
/// `guard` 在本函数消费前拥有精确的活动运行世代。
/// `task_id` identifies the stable task for invariant diagnostics.
/// `task_id` 标识用于不变量诊断的稳定任务。
///
/// # Returns
/// 返回值
/// Success after authoritative removal, otherwise a missing-guard or registry-lock error.
/// 权威移除完成后返回成功，否则返回守卫缺失或注册表锁错误。
fn release_owned_active_task_run(
    guard: &mut Option<ActiveTaskRunGuard>,
    task_id: SessionId,
) -> Result<(), String> {
    // activeTaskRunGuard is the single finalization owner transferred from the parent execution.
    // activeTaskRunGuard 是从父执行转移而来的唯一终态处理所有者。
    let active_task_run_guard = guard.take().ok_or_else(|| {
        format!(
            "The active-run guard for task `{task_id}` was already consumed before finalization."
        )
    })?;
    active_task_run_guard.release()
}

/// Combines terminal-state persistence and active-run release without discarding either failure.
/// 合并终态持久化与活动运行释放结果，且不丢弃任一失败。
///
/// # Parameters
/// 参数
/// `terminal_state_result` contains the fully contextualized terminal-state outcome.
/// `terminal_state_result` 包含已带完整上下文的终态写入结果。
/// `active_run_release_result` contains the fully contextualized active-run release outcome.
/// `active_run_release_result` 包含已带完整上下文的活动运行释放结果。
///
/// # Returns
/// 返回值
/// Success only when both operations succeed, otherwise one diagnostic containing every failure.
/// 仅当两项操作都成功时返回成功，否则返回包含全部失败的一条诊断。
pub(super) fn combine_task_run_finalization_results(
    terminal_state_result: Result<(), String>,
    active_run_release_result: Result<(), String>,
) -> Result<(), String> {
    match (terminal_state_result, active_run_release_result) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(terminal_state_error), Err(active_run_release_error)) => Err(format!(
            "{terminal_state_error} Additionally, {active_run_release_error}"
        )),
    }
}

/// Resumes queued child-agent messages after the current active-run guard has been released.
/// 在当前活动运行守卫释放后续跑排队的子代理消息。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the active-run registry and inter-agent inbox.
/// `task_runtime` 持有活动运行注册表与代理间 inbox。
/// `current_session_id` is the direct parent that owns the child agent.
/// `current_session_id` 是拥有该子代理的直接父级。
/// `task_id` identifies the child agent whose turn just settled.
/// `task_id` 标识刚刚收敛的子代理。
///
/// # Returns
/// 返回值
/// `true` when a queued message started another child-agent turn.
/// 当排队消息已启动另一轮子代理 turn 时返回 `true`。
async fn resume_queued_messages_after_active_run_released(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    task_id: SessionId,
) -> bool {
    match task_runtime
        .resume_agent_from_inbox_after_task_settled(current_session_id, task_id)
        .await
    {
        Ok(resumed) => resumed,
        Err(error) => {
            eprintln!(
                "[subagent-agent-control] failed to resume queued messages after task settled: {error}"
            );
            false
        }
    }
}

/// Loads the latest persisted TaskSession context as a core snapshot when it is available.
/// 在可用时将最新持久化 TaskSession 上下文加载为核心快照。
pub(super) fn current_task_snapshot_from_store(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    task_id: SessionId,
) -> Result<Option<CoreConversationSnapshot>, String> {
    let Some(store) = task_runtime
        .task_session_store_for_root(root_session_id)
        .map_err(|error| {
            format!("TaskSession store root lookup failed for `{root_session_id}`: {error}")
        })?
    else {
        return Ok(None);
    };
    let Some(context) = store
        .load_context(SessionNodeId::from(task_id))
        .map_err(|error| {
            format!("TaskSession context load failed for task `{task_id}`: {error}")
        })?
    else {
        return Ok(None);
    };
    let conversation_state: SessionConversationState = serde_json::from_value(
        context.conversation_state,
    )
    .map_err(|error| {
        format!(
            "TaskSession context for task `{task_id}` contains invalid conversation state: {error}"
        )
    })?;
    Ok(Some(build_core_conversation_snapshot(&conversation_state)?))
}

/// Loads the latest stopped-task snapshot and logs corrupted persisted context before falling back.
/// 加载最新停止任务快照，并在持久化上下文损坏时记录诊断后再回退。
fn stopped_task_snapshot_from_store_or_request(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    task_id: SessionId,
    request: &TaskExecutionRequest,
) -> Option<CoreConversationSnapshot> {
    match current_task_snapshot_from_store(task_runtime, root_session_id, task_id) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!(
                "[task-session-store] using request snapshot for stopped task `{}` because persisted context could not be loaded: {error}",
                request.public_task_id
            );
            Some(request.snapshot.clone())
        }
    }
}

/// Spawns one non-blocking TaskSession context compaction job after a child task has completed.
/// 在子任务完成后派生一个非阻塞 TaskSession 上下文压缩任务。
pub(super) fn spawn_task_session_context_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) {
    if !resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled
    {
        return;
    }

    tokio::spawn(async move {
        if let Err(error) =
            run_task_session_context_compaction(task_runtime, context, request, resolved_config)
                .await
        {
            eprintln!("[task-session-store] context compaction failed: {error}");
        }
    });
}

/// Runs one TaskSession context compaction pass using the shared prompt-compaction runtime and segmented store.
/// 使用共享提示词压缩运行时与分段存储执行一次 TaskSession 上下文压缩。
///
/// # Parameters
/// 参数
/// `task_runtime` resolves the segmented store and shared task runtime dependencies.
/// `task_runtime` 解析分段存储与共享子任务运行时依赖。
/// `context` supplies the exact provider and foundation context used for summary generation.
/// `context` 提供摘要生成使用的精确 Provider 与基础运行时上下文。
/// `request` identifies the completed task execution whose persisted context may be compacted.
/// `request` 标识其持久化上下文可能被压缩的已完成子任务执行。
/// `resolved_config` supplies the effective context-compaction policy for that execution.
/// `resolved_config` 提供该次执行的有效上下文压缩策略。
///
/// # Returns
/// 返回值
/// The committed compacted snapshot, `None` when compaction is unnecessary or superseded, or an explicit failure.
/// 返回已提交的压缩快照；无需压缩或已被新状态替代时返回 `None`，失败时返回明确错误。
pub(crate) async fn run_task_session_context_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) -> Result<Option<CoreConversationSnapshot>, String> {
    let Some(store) = task_runtime.task_session_store_for_root(request.root_session_id)? else {
        return Ok(None);
    };
    let task_node_id = SessionNodeId::from(request.task_id);
    let Some(context_document) = store
        .load_context(task_node_id)
        .map_err(|error| error.message)?
    else {
        return Ok(None);
    };
    let conversation_state: SessionConversationState =
        serde_json::from_value(context_document.conversation_state.clone())
            .map_err(|error| error.to_string())?;
    let messages = conversation_state_to_compaction_source_messages(&conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }
    let Some(plan) = plan_context_compaction(CompactionPlanInput {
        session_label: &request.public_task_id,
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: context_document.compaction_anchor.as_deref(),
        source_until_message_id: None,
        policy: task_context_compaction_policy_from_snapshot(
            &resolved_config.effective_config.context_compaction,
        ),
    }) else {
        return Ok(None);
    };
    let event_sequence = store
        .load_events(task_node_id)
        .map(|events| events.len() as u64)
        .unwrap_or(0);
    let (provider_configuration, _) =
        build_task_runtime_preparation_request(&context, &request, &resolved_config)?;
    let model_label = None;
    let stream = stream_prompt_compaction_foundation_job(PromptCompactionFoundationJobRequest {
        session_id: request.task_id,
        session_title: request.display_title.clone(),
        workspace_path: context.request_template.workspace_path.clone(),
        plan,
        full_context_state: conversation_state.clone(),
        input_budget: None,
        observed_input_tokens: None,
        estimated_input_tokens: None,
        provider_configuration,
        model_label,
        reasoning_effort: context.request_template.reasoning_effort,
        foundation: Arc::clone(&context.foundation),
        luaskills_runtime: context.luaskills_runtime.clone(),
    });
    futures_util::pin_mut!(stream);
    while let Some(event) = stream.next().await {
        let PromptCompactionStreamEvent::Finished(result) = event else {
            continue;
        };
        let execution = result?;
        // sourceContextDocument is the exact optimistic concurrency precondition for compaction publication.
        // sourceContextDocument 是发布压缩结果时使用的精确乐观并发前置条件。
        let source_context_document = context_document.clone();
        let application = apply_task_session_compaction_result(
            context_document,
            &execution.compaction_result,
            event_sequence,
        )
        .map_err(|error| error.message)?;
        let compacted_conversation_state: SessionConversationState =
            serde_json::from_value(application.context.conversation_state.clone())
                .map_err(|error| error.to_string())?;
        let compacted_snapshot = build_core_conversation_snapshot(&compacted_conversation_state)?;
        let checkpoint_id = application.checkpoint.checkpoint_id.clone();
        let timestamp_ms = application.checkpoint.timestamp_ms;
        let summary = application.checkpoint.summary.clone();
        if !store
            .save_compacted_context_if_current(&source_context_document, application.context)
            .map_err(|error| error.message)?
        {
            task_runtime_trace(
                "task_context_compaction_superseded",
                format!(
                    "task={} execution_id={}",
                    request.task_id, request.execution_id
                ),
            );
            return Ok(None);
        }
        store
            .save_compaction_checkpoint(application.checkpoint)
            .map_err(|error| error.message)?;
        store
            .append_event(SessionEventRecord::new(
                format!("task-context-compaction-{checkpoint_id}"),
                task_node_id,
                event_sequence,
                SessionEventKind::RuntimeMarker,
                timestamp_ms,
                json!({
                    "context_compaction_status": "completed",
                    "checkpoint_id": checkpoint_id,
                    "summary": summary,
                    "fallback_used": execution.fallback_used,
                    "provider_request_count": execution.provider_request_count,
                }),
            ))
            .map_err(|error| error.message)?;
        return Ok(Some(compacted_snapshot));
    }
    Ok(None)
}

/// Builds the shared compaction planning policy from a resolved TaskSession policy snapshot.
/// 根据已解析的 TaskSession 策略快照构建共享压缩规划策略。
pub(super) fn task_context_compaction_policy_from_snapshot(
    snapshot: &ContextCompactionPolicySnapshot,
) -> ContextCompactionPolicy {
    ContextCompactionPolicy {
        auto: snapshot.auto_compaction_enabled,
        ..ContextCompactionPolicy::default()
    }
}
