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
    let active_task_run_guard = task_runtime.claim_and_reserve_active_task_run(&request)?;
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
        let mut launch = prepare_task_runtime_stream_launch_with_preflight_compaction(
            Arc::clone(&background_task_runtime),
            &context,
            &mut background_request,
            &background_resolved_config,
        )
        .await?;
        let initial_tool_result_count =
            task_snapshot_tool_result_count(&background_request.snapshot);
        let initial_message_ids = background_request
            .snapshot
            .messages
            .iter()
            .map(|message| message.id.clone())
            .collect::<std::collections::BTreeSet<_>>();
        let mut mid_turn_compaction_attempted = false;
        let mut completed_provider_requests = 0usize;
        let mut completed_tool_results = 0usize;
        let mut completed_usage = None::<vulcan_core::types::ProviderUsageSnapshot>;
        let mut completed_last_usage = None::<vulcan_core::types::ProviderUsageSnapshot>;
        let report = loop {
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
            let mut terminal_report = None;
            let mut compaction_state = None;
            let mut checkpoint_provider_requests = 0usize;
            let mut checkpoint_usage = None::<vulcan_core::types::ProviderUsageSnapshot>;
            while let Some(event) = child_event_stream.next().await {
                match &event {
                    ExecutionStreamEvent::ConversationCheckpoint {
                        last_successful_provider_usage,
                        provider_request_count,
                        ..
                    } => {
                        checkpoint_provider_requests = *provider_request_count;
                        checkpoint_usage = last_successful_provider_usage.clone();
                    }
                    ExecutionStreamEvent::ContextCompactionRequired {
                        conversation_state,
                        reason,
                        ..
                    } => {
                        compaction_state = Some((conversation_state.clone(), reason.clone()));
                    }
                    ExecutionStreamEvent::Finished(finished_report) => {
                        terminal_report = Some(finished_report.clone());
                    }
                    _ => {}
                }
                background_task_runtime.publish_detached_task_event(
                    background_request.task_id,
                    &background_request.execution_id,
                    DetachedTaskRuntimeEvent::ChildExecution(event),
                )?;
            }
            if let Some(mut terminal_report) = terminal_report {
                merge_task_mid_turn_report(
                    &mut terminal_report,
                    completed_provider_requests,
                    completed_usage.as_ref(),
                    completed_last_usage.as_ref(),
                );
                break terminal_report;
            }
            let Some((conversation_state, reason)) = compaction_state else {
                return Err(format!(
                    "Task `{}` finished without a terminal execution report.",
                    background_request.task_id
                ));
            };
            if mid_turn_compaction_attempted {
                return Err(format!(
                    "Task `{}` requested repeated mid-turn context compaction after `{reason}`.",
                    background_request.task_id
                ));
            }
            let checkpoint_snapshot = build_core_conversation_snapshot(&conversation_state)?;
            completed_tool_results = task_snapshot_tool_result_count(&checkpoint_snapshot)
                .saturating_sub(initial_tool_result_count);
            completed_provider_requests =
                completed_provider_requests.saturating_add(checkpoint_provider_requests);
            let (snapshot_usage, snapshot_last_usage) =
                task_snapshot_provider_usage_after(&checkpoint_snapshot, &initial_message_ids);
            completed_usage = snapshot_usage.or_else(|| checkpoint_usage.clone());
            completed_last_usage = snapshot_last_usage.or(checkpoint_usage);
            background_request.snapshot = checkpoint_snapshot;
            let compacted_snapshot = run_task_session_context_compaction(
                Arc::clone(&background_task_runtime),
                context.clone(),
                background_request.clone(),
                background_resolved_config.clone(),
            )
            .await?
            .ok_or_else(|| {
                format!(
                    "Task `{}` required mid-turn context compaction after `{reason}`, but compaction produced no snapshot.",
                    background_request.task_id
                )
            })?;
            background_request.snapshot = compacted_snapshot;
            mid_turn_compaction_attempted = true;
            launch = prepare_task_runtime_stream_launch(
                &context,
                &background_request,
                &background_resolved_config,
            )
            .await?;
        };
        let budget_should_compact = report
            .preparation
            .as_ref()
            .and_then(|preparation| preparation.context_budget.as_ref())
            .is_some_and(|context_budget| context_budget.should_compact());
        let provider_limit_should_compact =
            task_provider_request_limit_should_compact_after_current_report(
                &background_resolved_config
                    .effective_config
                    .context_compaction,
                report.provider_request_count,
            );
        let should_compact_context = budget_should_compact || provider_limit_should_compact;
        let mut outcome = task_execution_outcome_from_report(
            &background_request,
            child_completion_controller_config,
            background_resolved_config,
            report,
        )?;
        outcome.tool_calls_processed = outcome
            .tool_calls_processed
            .saturating_add(completed_tool_results);
        if let Some(segment) = outcome.usage_segments.first_mut() {
            segment.tool_call_count = outcome.tool_calls_processed;
        }
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
                let stopped_snapshot =
                    stopped_task_snapshot_from_runtime_or_request(&task_runtime, task_id, &request)
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
                let stopped_snapshot =
                    stopped_task_snapshot_from_runtime_or_request(&task_runtime, task_id, &request)
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

/// Counts replayable tool results in one child-task conversation snapshot.
/// 统计一份子任务会话快照中可回放的工具结果数量。
///
/// # Parameters
/// 参数
/// `snapshot` is the exact canonical window before or after one mid-turn compaction boundary.
/// `snapshot` 是一次回合中压缩边界前后的精确规范窗口。
///
/// # Returns
/// 返回值
/// The number of completed tool-result messages retained by the snapshot.
/// 返回快照中保留的已完成工具结果消息数量。
fn task_snapshot_tool_result_count(snapshot: &CoreConversationSnapshot) -> usize {
    snapshot
        .messages
        .iter()
        .filter(|message| {
            matches!(
                message.payload,
                vulcan_core::types::ConversationPayload::ToolResult { .. }
            )
        })
        .count()
}

/// Accumulates successful provider usage from messages appended after the invocation baseline.
/// 累加调用基线之后追加消息中的成功 Provider 用量。
///
/// # Parameters
/// 参数
/// `snapshot` is the replay-safe pre-compaction state and `initial_message_ids` identifies every
/// message that existed before this logical task execution started.
/// `snapshot` 是压缩前可安全回放的状态，`initial_message_ids` 标识本次逻辑任务执行开始前已存在的全部消息。
///
/// # Returns
/// 返回值
/// Cumulative and latest successful usage for only the newly appended assistant messages.
/// 仅返回新追加助手消息的累计用量与最后一次成功用量。
fn task_snapshot_provider_usage_after(
    snapshot: &CoreConversationSnapshot,
    initial_message_ids: &std::collections::BTreeSet<String>,
) -> (
    Option<vulcan_core::types::ProviderUsageSnapshot>,
    Option<vulcan_core::types::ProviderUsageSnapshot>,
) {
    let mut cumulative = None::<vulcan_core::types::ProviderUsageSnapshot>;
    let mut last = None;
    for message in &snapshot.messages {
        if initial_message_ids.contains(&message.id) {
            continue;
        }
        let vulcan_core::types::ConversationPayload::Assistant {
            usage: Some(usage), ..
        } = &message.payload
        else {
            continue;
        };
        cumulative
            .get_or_insert_with(Default::default)
            .accumulate_from(usage);
        last = Some((**usage).clone());
    }
    (cumulative, last)
}

/// Merges pre-compaction provider accounting into the terminal continuation report.
/// 将压缩前的 Provider 计量合并到续跑终态报告中。
///
/// # Parameters
/// 参数
/// `report` is the terminal report produced after the compacted continuation; the remaining
/// arguments are successful facts captured before compaction.
/// `report` 是压缩续跑后生成的终态报告；其余参数是压缩前捕获的成功事实。
fn merge_task_mid_turn_report(
    report: &mut ExecutionRunReport,
    completed_provider_requests: usize,
    completed_usage: Option<&vulcan_core::types::ProviderUsageSnapshot>,
    completed_last_usage: Option<&vulcan_core::types::ProviderUsageSnapshot>,
) {
    report.provider_request_count = report
        .provider_request_count
        .saturating_add(completed_provider_requests);
    if let Some(completed_usage) = completed_usage {
        let cumulative = report
            .cumulative_successful_provider_usage
            .get_or_insert_with(Default::default);
        let current = cumulative.clone();
        *cumulative = completed_usage.clone();
        cumulative.accumulate_from(&current);
    }
    if report.last_successful_provider_usage.is_none() {
        report.last_successful_provider_usage = completed_last_usage.cloned();
    }
}

/// Determines whether a task session crossed the provider-request compaction threshold after the current run.
/// 判断子代理会话在当前执行后是否达到 Provider 请求数压缩阈值。
///
/// # Parameters
/// 参数
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
    policy: &ContextCompactionPolicySnapshot,
    current_provider_requests: usize,
) -> bool {
    task_provider_request_limit_is_enabled(policy)
        && task_provider_request_limit_reached(policy, current_provider_requests)
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

/// Reads the latest resident child-execution snapshot when it is available.
/// 在可用时读取最新的进程内子代理执行快照。
pub(super) fn current_resident_task_snapshot(
    task_runtime: &TaskRuntime,
    task_id: SessionId,
) -> Result<Option<CoreConversationSnapshot>, String> {
    let snapshot = task_runtime
        .sessions
        .read()
        .map_err(|_| {
            String::from(
                "The task session registry is locked and cannot read the resident conversation snapshot.",
            )
        })?
        .get(&task_id)
        .map(|record| record.snapshot.clone());
    Ok(snapshot)
}

/// Reads the latest stopped-task snapshot and falls back to the request snapshot on registry failure.
/// 读取最新停止任务快照，并在注册表失败时回退到请求快照。
fn stopped_task_snapshot_from_runtime_or_request(
    task_runtime: &TaskRuntime,
    task_id: SessionId,
    request: &TaskExecutionRequest,
) -> Option<CoreConversationSnapshot> {
    match current_resident_task_snapshot(task_runtime, task_id) {
        Ok(snapshot) => snapshot.or_else(|| Some(request.snapshot.clone())),
        Err(error) => {
            eprintln!(
                "[task-runtime] using request snapshot for stopped task `{}` because the resident snapshot could not be read: {error}",
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
        let compacted_snapshot = match run_task_session_context_compaction(
            Arc::clone(&task_runtime),
            context,
            request.clone(),
            resolved_config,
        )
        .await
        {
            Ok(compacted_snapshot) => compacted_snapshot,
            Err(error) => {
                eprintln!("[task-runtime] context compaction failed: {error}");
                return;
            }
        };
        let Some(compacted_snapshot) = compacted_snapshot else {
            return;
        };
        let mut sessions = match task_runtime.sessions.write() {
            Ok(sessions) => sessions,
            Err(_) => {
                eprintln!(
                    "[task-runtime] compacted snapshot could not update task {} because the resident registry is locked",
                    request.task_id
                );
                return;
            }
        };
        let Some(record) = sessions.get_mut(&request.task_id) else {
            return;
        };
        // A late compaction must never overwrite a newer execution generation.
        // 晚到的压缩结果绝不能覆盖更新的执行代次。
        if record.execution_id.as_deref() == Some(request.execution_id.as_str()) {
            record.snapshot = compacted_snapshot;
            record.updated_at = current_unix_millis_text();
        }
    });
}

/// Runs one child request-context compaction pass using the shared prompt-compaction runtime.
/// 使用共享提示词压缩运行时执行一次子代理请求上下文压缩。
///
/// # Parameters
/// 参数
/// `task_runtime` owns the process-local child execution registry.
/// `task_runtime` 持有进程内子代理执行注册表。
/// `context` supplies the exact provider and foundation context used for summary generation.
/// `context` 提供摘要生成使用的精确 Provider 与基础运行时上下文。
/// `request` identifies the completed child execution whose request context may be compacted.
/// `request` 标识其请求上下文可能被压缩的已完成子代理执行。
/// `resolved_config` supplies the effective context-compaction policy for that execution.
/// `resolved_config` 提供该次执行的有效上下文压缩策略。
///
/// # Returns
/// 返回值
/// The committed compacted snapshot, `None` when compaction is unnecessary or superseded, or an explicit failure.
/// 返回已提交的压缩快照；无需压缩或已被新状态替代时返回 `None`，失败时返回明确错误。
pub(crate) async fn run_task_session_context_compaction(
    _task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) -> Result<Option<CoreConversationSnapshot>, String> {
    let mut conversation_state = core_snapshot_to_session_conversation_state(&request.snapshot)?;
    let messages = conversation_state_to_compaction_source_messages(&conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }

    // The preparation request carries the exact provider overrides already resolved for this task.
    // 准备请求携带已为本次任务执行解析的精确供应商覆盖。
    let (provider_configuration, compaction_preparation_request) =
        build_task_runtime_preparation_request(&context, &request, &resolved_config)?;
    let compaction_session_id = compaction_preparation_request.session_id.to_string();
    let public_ai_runtime = context.foundation.public_ai_runtime().ok_or_else(|| {
        String::from("TaskSession context compaction requires the public AI runtime.")
    })?;
    let behavior_seed = task_context_compaction_policy_from_snapshot(
        &resolved_config.effective_config.context_compaction,
    );
    let model_target = resolve_prompt_compaction_model_target(
        public_ai_runtime.as_ref(),
        compaction_session_id.as_str(),
        compaction_preparation_request
            .provider_route_lease_identity
            .as_ref(),
        compaction_preparation_request
            .provider_id_override
            .as_deref(),
        compaction_preparation_request
            .connection_id_override
            .as_deref(),
        compaction_preparation_request.model_id_override.as_deref(),
        compaction_preparation_request.context_compaction_window_node_id,
        Some(behavior_seed),
    )?
    .ok_or_else(|| {
        String::from("TaskSession context compaction requires an exact model target.")
    })?;
    let Some(plan) = plan_context_compaction(CompactionPlanInput {
        session_label: &request.public_task_id,
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: None,
        source_until_message_id: None,
        policy: model_target.context_compaction_policy,
    }) else {
        return Ok(None);
    };
    let model_label = Some(model_target.label());
    let launch_context =
        PromptCompactionLaunchContext::from_execution_request(&compaction_preparation_request);
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
        model_target: Some(model_target),
        execution_agent_id: compaction_preparation_request.execution_agent_id.clone(),
        launch_context,
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
        let summary_message_id =
            format!("task-summary-{}", execution.compaction_result.compaction_id);
        vulcan_context_compaction::compact_conversation_with_compaction_result(
            &mut conversation_state,
            summary_message_id.clone(),
            &execution.compaction_result,
        )
        .map_err(|error| error.to_string())?;
        vulcan_context_compaction::append_remote_responses_compaction_context(
            &mut conversation_state,
            &summary_message_id,
            &execution.compaction_result,
        )
        .map_err(|error| error.to_string())?;
        vulcan_context_compaction::compact_conversation_records_after_summary(
            &mut conversation_state,
            &summary_message_id,
            execution.compaction_result.policy,
            |value| Some(value.clone()),
        );
        return Ok(Some(build_core_conversation_snapshot(&conversation_state)?));
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

#[cfg(test)]
mod mid_turn_compaction_tests {
    use super::*;

    /// Verifies continuation reports retain provider counts and usage accumulated before compaction.
    /// 验证续跑报告会保留压缩前累计的 Provider 次数与用量。
    #[test]
    fn mid_turn_report_merge_preserves_pre_compaction_accounting() {
        let current_usage = vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(5),
            total_tokens: Some(5),
            ..Default::default()
        };
        let completed_usage = vulcan_core::types::ProviderUsageSnapshot {
            input_tokens: Some(7),
            total_tokens: Some(7),
            ..Default::default()
        };
        let mut report = ExecutionRunReport {
            session_id: SessionId::nil(),
            preparation: None,
            assistant_output: Some(String::from("done")),
            thinking_output: None,
            conversation_state: None,
            provider_request_count: 2,
            last_successful_provider_usage: Some(current_usage.clone()),
            cumulative_successful_provider_usage: Some(current_usage.clone()),
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
            controller_events: Vec::new(),
            embedded_turn_usage_segments: Vec::new(),
        };

        merge_task_mid_turn_report(&mut report, 1, Some(&completed_usage), None);

        assert_eq!(report.provider_request_count, 3);
        let cumulative = report
            .cumulative_successful_provider_usage
            .expect("cumulative usage should remain available");
        assert_eq!(cumulative.input_tokens, Some(12));
        assert_eq!(cumulative.total_tokens, Some(12));
        assert_eq!(report.last_successful_provider_usage, Some(current_usage));
    }

    /// Verifies tool-result counting observes only completed replayable result messages.
    /// 验证工具结果计数只统计已完成且可回放的结果消息。
    #[test]
    fn task_snapshot_tool_result_count_ignores_non_result_messages() {
        let snapshot = CoreConversationSnapshot {
            messages: vec![
                vulcan_core::types::ConversationMessage {
                    id: String::from("user"),
                    payload: vulcan_core::types::ConversationPayload::UserText {
                        text: String::from("run"),
                        attachments: Vec::new(),
                    },
                },
                vulcan_core::types::ConversationMessage {
                    id: String::from("tool"),
                    payload: vulcan_core::types::ConversationPayload::ToolResult {
                        tool_call_id: Some(String::from("call")),
                        is_error: false,
                        text: String::from("ok"),
                    },
                },
            ],
            ..Default::default()
        };

        assert_eq!(task_snapshot_tool_result_count(&snapshot), 1);
    }

    /// Verifies pre-compaction accounting sums every newly appended assistant usage observation.
    /// 验证压缩前计量会累加每条新追加助手消息的用量观测。
    #[test]
    fn task_snapshot_usage_accumulates_all_new_assistant_messages() {
        let assistant = |id: &str, tokens: usize| vulcan_core::types::ConversationMessage {
            id: id.to_string(),
            payload: vulcan_core::types::ConversationPayload::Assistant {
                text: String::new(),
                reasoning: String::new(),
                provider_message_id: None,
                provider_message_state: None,
                usage: Some(Box::new(vulcan_core::types::ProviderUsageSnapshot {
                    input_tokens: Some(tokens),
                    uncached_input_tokens: Some(tokens),
                    total_tokens: Some(tokens),
                    audio_input_tokens: Some(tokens),
                    audio_output_tokens: Some(tokens),
                    ..Default::default()
                })),
                tool_calls: Vec::new(),
            },
        };
        let snapshot = CoreConversationSnapshot {
            messages: vec![
                assistant("existing", 100),
                assistant("one", 3),
                assistant("two", 4),
            ],
            ..Default::default()
        };
        let initial_ids = std::collections::BTreeSet::from([String::from("existing")]);

        let (cumulative, last) = task_snapshot_provider_usage_after(&snapshot, &initial_ids);

        let cumulative = cumulative.expect("new assistant usage should accumulate");
        assert_eq!(cumulative.total_tokens, Some(7));
        assert_eq!(cumulative.uncached_input_tokens, Some(7));
        assert_eq!(cumulative.audio_input_tokens, Some(7));
        assert_eq!(cumulative.audio_output_tokens, Some(7));
        assert_eq!(last.and_then(|usage| usage.total_tokens), Some(4));
    }
}
