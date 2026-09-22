use super::*;
use crate::agent_runtime::prompt_compaction::ChildTaskCompactionPhase;

/// Owns cleanup for one detached execution stream until the background finalizer takes over.
/// 在后台终态处理器接管前，持有一条分离执行流的清理责任。
pub(super) struct DetachedTaskStreamStartupGuard {
    /// Shared immutable request identity; sharing avoids another conversation snapshot clone.
    /// 共享不可变请求身份，避免再次克隆对话快照。
    request: Arc<TaskExecutionRequest>,
    /// Explicit observed startup failure; absent means ownership ended without a returned error.
    /// 已观测明确启动失败；不存在表示所有权结束时没有返回错误。
    failure_reason: Option<String>,
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
    /// Active run ownership must be released before a rejected startup closes its stream.
    /// 被拒启动关闭其事件流之前，必须先释放活动运行所有权。
    active_run: Option<ActiveTaskRunGuard>,
}

impl DetachedTaskStreamStartupGuard {
    /// Opens one exact detached stream and assumes startup cleanup ownership.
    /// 打开一条精确分离流并取得启动阶段清理所有权。
    ///
    /// # Parameters
    /// 参数
    /// `runtime` owns the detached stream registry and later background finalizer.
    /// `runtime` 持有分离流注册表以及后续后台终态处理器。
    /// `request` retains exact task, execution and parent identity without duplicating history.
    /// `request` 保留精确任务、执行与父身份，不重复复制历史。
    /// `active_run` transfers the exact active lease into ordered startup cleanup.
    /// `active_run` 将精确活动租约移交给有序启动清理。
    ///
    /// # Returns
    /// 返回值
    /// A cleanup owner after the exact stream opens, or the registry open error.
    /// 精确流打开后返回清理所有者，否则返回注册表打开错误。
    pub(super) fn open(
        runtime: Arc<TaskRuntime>,
        request: Arc<TaskExecutionRequest>,
        active_run: ActiveTaskRunGuard,
    ) -> Result<Self, String> {
        // All cleanup coordinates come from the same immutable admitted request.
        // 所有清理坐标均来自同一不可变已准入请求。
        let task_id = request.task_id;
        // Keep the stream identity distinct from the reusable physical task.
        // 将事件流身份与可复用物理任务区分。
        let execution_id = &request.execution_id;
        if active_run.task_id != task_id || !Arc::ptr_eq(&active_run.runtime, &runtime) {
            return Err(String::from(
                "Detached startup does not own the supplied active task run.",
            ));
        }
        runtime.open_detached_task_event_stream_for_run(task_id, execution_id)?;
        Ok(Self {
            runtime,
            task_id,
            execution_id: execution_id.to_string(),
            request,
            failure_reason: None,
            owns_cleanup: true,
            active_run: Some(active_run),
        })
    }

    /// Preserves the original fallible startup result so Drop can report the actual cause.
    /// 保留原始可失败启动结果，使析构能报告实际原因。
    /// Returns the input result unchanged; cancellation without a result uses an explicit ownership-end cause.
    /// 原样返回输入结果；未返回结果的取消使用明确所有权结束原因。
    pub(super) fn observe<T>(&mut self, result: Result<T, String>) -> Result<T, String> {
        if let Err(reason) = &result {
            self.failure_reason = Some(reason.clone());
        }
        result
    }

    /// Releases the exact lease, records process state and sends the explicit host abort in order.
    /// 依次释放精确租约、记录进程状态并发送明确宿主中止。
    /// `reason` is the original cause; returns combined cleanup errors without hiding independent failures.
    /// `reason` 是原始原因；返回组合清理错误，不隐藏独立失败。
    fn abort_startup(&mut self, reason: &str) -> Result<(), String> {
        self.release_active_run()?;
        // Failure projection cannot substitute for durable host cleanup, so attempt both independently.
        // 失败投影不能替代可靠宿主清理，因此独立尝试两者。
        let projection = self
            .runtime
            .mark_existing_startup_failure(&self.request, reason);
        // This event is emitted after lease release even when the process-local projection failed.
        // 即使进程内投影失败，此事件仍在租约释放之后发出。
        let aborted = self
            .runtime
            .abort_detached_task_startup(&self.request, reason);
        if aborted.is_ok() {
            self.owns_cleanup = false;
        }
        match (projection, aborted) {
            (Ok(_), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
            (Err(projection), Err(abort)) => Err(format!(
                "{projection} Startup abort publication also failed: {abort}"
            )),
        }
    }

    /// Attaches the Provider abort handle while startup still owns the active run.
    /// 启动阶段仍拥有活动运行时附加 Provider 中止句柄。
    /// `handle` belongs to the just-spawned Provider; returns an ownership or registry error.
    /// `handle` 属于刚派生的 Provider；返回归属或注册表错误。
    pub(super) fn attach_abort_handle(&self, handle: AbortHandle) -> Result<(), String> {
        self.active_run
            .as_ref()
            .ok_or_else(|| String::from("Detached startup already transferred its active run."))?
            .attach_abort_handle(handle)
    }

    /// Transfers the active run once into the background finalizer immediately before spawn.
    /// 在派生前将活动运行一次性移交给后台终态处理器。
    /// Returns the exact owned guard or rejects an attempted duplicate transfer.
    /// 返回精确所属守卫，或拒绝重复移交。
    pub(super) fn take_active_run(&mut self) -> Result<ActiveTaskRunGuard, String> {
        self.active_run
            .take()
            .ok_or_else(|| String::from("Detached startup already transferred its active run."))
    }

    /// Releases the active lease before stream closure, preserving failed lease removal for retry.
    /// 在事件流关闭之前释放活动租约，保留失败的租约删除以便重试。
    /// Returns success if ownership was transferred or released, otherwise the registry failure.
    /// 归属已移交或已释放时返回成功，否则返回注册表失败。
    fn release_active_run(&mut self) -> Result<(), String> {
        if let Some(active_run) = self.active_run.as_mut() {
            active_run.release_in_place()?;
        }
        Ok(())
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
        self.failure_reason = Some(primary_error.clone());
        match self.abort_startup(&primary_error) {
            Ok(()) => primary_error,
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
    /// Returns success after lease transfer, or an ownership error and cleans up the rejected handoff.
    /// 租约移交后返回成功，否则返回归属错误并清理被拒绝的交接。
    pub(super) fn handoff_to_background_finalizer(mut self) -> Result<(), String> {
        if self.active_run.is_some() {
            return Err(String::from(
                "Detached startup must transfer its active run before stream handoff.",
            ));
        }
        self.owns_cleanup = false;
        Ok(())
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
        if !self.owns_cleanup {
            return;
        }
        // Missing error means the startup future ended before handoff, including cancellation or unwinding.
        // 缺失错误表示启动 Future 在交接前结束，包括取消或栈展开。
        let reason = self.failure_reason.clone().unwrap_or_else(|| {
            String::from("Child startup ended before background ownership was established.")
        });
        if let Err(error) = self.abort_startup(&reason) {
            eprintln!(
                "[task-runtime] detached stream startup guard cleanup failed for task {} execution {}: {error}",
                self.task_id, self.execution_id
            );
        }
    }
}

/// Runs one child task through the shared runtime preparation and provider-dispatch bridge.
/// 通过共享运行时准备与 Provider 分发桥执行一条子代理任务。
pub(crate) async fn run_task_with_shared_runtime_context(
    task_runtime: Arc<TaskRuntime>,
    mut context: TaskExecutionRuntimeContext,
    base_controller_config: CoreControllerConfig,
    parent_live_sender: ProviderLiveDeltaSender,
    request: TaskExecutionRequest,
) -> Result<TaskExecutionOutcome, String> {
    // A new delegation never inherits its parent's recovery identity.
    // 新委派绝不继承父级的恢复身份。
    context.recovered_child_profile = None;
    // Claim trusted coordinates before publishing the start; reserve after the host creates the child document.
    // 发布启动前认领可信坐标；在宿主创建子文档后预留。
    let lifecycle_binding =
        task_runtime.take_child_task_lifecycle_binding(request.execution_id.as_str())?;
    if let Some(binding) = &lifecycle_binding {
        if binding.reservation.execution.task_session_id != request.task_id
            || binding.reservation.execution.execution_id != request.execution_id
        {
            return Err(format!(
                "Child lifecycle binding does not match task `{}` execution `{}`.",
                request.task_id, request.execution_id
            ));
        }
    }
    // The startup guard owns explicit abort publication; the host settles any durable reservation.
    // 启动守卫拥有明确中止发布责任；宿主结算已有持久预留。
    run_task_with_shared_runtime_context_inner(
        task_runtime,
        context,
        base_controller_config,
        parent_live_sender,
        request,
        lifecycle_binding,
    )
    .await
}

/// Runs one child execution after its durable reservation has been acknowledged.
/// 在可靠预留确认后运行一条子执行。
async fn run_task_with_shared_runtime_context_inner(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    base_controller_config: CoreControllerConfig,
    parent_live_sender: ProviderLiveDeltaSender,
    request: TaskExecutionRequest,
    lifecycle_binding: Option<ChildTaskLifecycleBinding>,
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
    // Startup and the finalizer share the finalized request without another history clone.
    // 启动与终态处理器共享最终请求，不再克隆一份历史。
    let request = Arc::new(request);
    let started_payload =
        build_task_host_payload(&request, "running", None, "", current_unix_millis_text());
    // detachedStreamStartupGuard owns stream cleanup until the background finalizer is installed.
    // detachedStreamStartupGuard 在后台终态处理器安装前持有事件流清理责任。
    let mut detached_stream_startup_guard = DetachedTaskStreamStartupGuard::open(
        Arc::clone(&task_runtime),
        Arc::clone(&request),
        active_task_run_guard,
    )?;
    // Initial creation and resumption resolve once, records parent ownership first and installs the host sink before UI projection.
    // 初始创建与续跑只解析一次，先记录父归属，并在界面投影之前安装宿主接收器。
    let initial_preparation = if let Some(binding) = &lifecycle_binding {
        detached_stream_startup_guard.observe(
            context
                .request_template
                .native_agent_collaboration_runtime
                .as_ref()
                .ok_or_else(|| {
                    String::from("Durable child admission requires a profile capture host.")
                }),
        )?;
        let prepared = detached_stream_startup_guard.observe(
            runtime_launch::prepare_task_runtime_execution(&context, &request, &resolved_config),
        )?;
        let creation = detached_stream_startup_guard
            .observe(prepared.creation.as_ref().ok_or_else(|| {
                String::from("Child admission requires captured creation settings.")
            }))?
            .clone();
        detached_stream_startup_guard.observe(
            binding
                .port
                .create_child_task(binding.reservation.clone(), creation)
                .await,
        )?;
        Some(prepared)
    } else {
        None
    };
    let (delegate_started_event, projection_barrier) = detached_stream_startup_guard.observe(
        attach_runtime_event_projection_barrier(build_task_delegate_runtime_event(
            RuntimeEventKind::DelegateStarted,
            &request,
            "Running and waiting for the subagent to return a result.",
            started_payload,
        )),
    )?;
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
    detached_stream_startup_guard.observe(
        wait_runtime_event_projection_barrier(projection_barrier)
            .await
            .map_err(|error| error.to_string()),
    )?;
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

    // Capture the recoverable profile before activation, then hand off this exact prepared launch.
    // 激活前捕获可恢复配置，再交接这份精确准备的启动结果。
    let initial_launch = if let Some(binding) = &lifecycle_binding {
        // All durable executions have already published through their single prepared admission.
        // 所有可靠执行均已通过唯一准备准入完成发布。
        let prepared = detached_stream_startup_guard
            .observe(initial_preparation.ok_or_else(|| {
                String::from("Durable child launch lost its admitted preparation.")
            }))?;
        let launch = detached_stream_startup_guard.observe(
            Box::pin(runtime_launch::launch_prepared_task_runtime_execution(
                &context, prepared,
            ))
            .await,
        )?;
        let active_at_ms =
            detached_stream_startup_guard.observe(current_unix_millis_u64().try_into().map_err(
                |_| String::from("Child lifecycle activation timestamp exceeds the durable range."),
            ))?;
        let activation_request =
            detached_stream_startup_guard.observe(binding.activation_request(active_at_ms))?;
        detached_stream_startup_guard.observe(
            binding
                .port
                .activate_child_task(activation_request)
                .await
                .map_err(|error| {
                    format!(
                        "Child task `{}` durable activation failed before provider launch: {error}",
                        request.public_task_id
                    )
                }),
        )?;
        Some(launch)
    } else {
        None
    };

    spawn_admitted_task_execution(
        task_runtime,
        context,
        request,
        resolved_config,
        running_controller_config,
        detached_stream_startup_guard,
        trace_started_at,
        initial_launch,
    )
}

/// Starts the shared provider loop and finalizer after the host has admitted its exact listener.
/// 宿主已准入精确监听器后，启动共享 Provider 循环与终态处理器。
/// The guard owns startup cleanup; an optional prepared launch belongs to a recovered execution.
/// 守卫持有启动清理；可选已准备载荷属于恢复执行。
/// Returns the running acknowledgement after abort and finalizer ownership is installed.
/// 安装中止及终态所有权后返回运行接纳回执。
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_admitted_task_execution(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: Arc<TaskExecutionRequest>,
    resolved_config: TaskSessionResolvedConfig,
    running_controller_config: CoreControllerConfig,
    mut detached_stream_startup_guard: DetachedTaskStreamStartupGuard,
    trace_started_at: u64,
    initial_launch: Option<
        crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch,
    >,
) -> Result<TaskExecutionOutcome, String> {
    // Both admission routes retain the same concrete child identity for every terminal path.
    // 两种准入路径在每条终态路径中保留同一个具体子身份。
    let task_id = request.task_id;
    let background_task_runtime = Arc::clone(&task_runtime);
    let background_request = request.as_ref().clone();
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
    // Gate all preparation and Provider writes until the abort handle and finalizer both own the run.
    // 在中止句柄和终态处理器共同接管运行前，阻止所有准备与 Provider 写入。
    // Heap-own the execution state so synchronous recovery admission does not carry the provider future.
    // 在堆上持有执行状态，避免同步恢复准入携带 Provider Future。
    let (child_run_start, child_run_task) = spawn_child_run_after_handoff(Box::pin(async move {
        let mut background_request = background_request;
        let mut launch = match initial_launch {
            Some(launch) => {
                if background_resolved_config
                    .effective_config
                    .context_compaction
                    .auto_compaction_enabled
                    && task_launch_should_compact(&launch)
                    && let Some(snapshot) = Box::pin(run_task_session_context_compaction(
                        Arc::clone(&background_task_runtime),
                        context.clone(),
                        background_request.clone(),
                        background_resolved_config.clone(),
                        ChildTaskCompactionPhase::ActiveBusiness,
                    ))
                    .await?
                {
                    background_request.snapshot = snapshot;
                    Box::pin(prepare_task_runtime_stream_launch(
                        &context,
                        &background_request,
                        &background_resolved_config,
                    ))
                    .await?
                } else {
                    launch
                }
            }
            None => {
                Box::pin(
                    prepare_task_runtime_stream_launch_with_preflight_compaction(
                        Arc::clone(&background_task_runtime),
                        &context,
                        &mut background_request,
                        &background_resolved_config,
                    ),
                )
                .await?
            }
        };
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
            // Only the successor execution may drain the child inbox into its durable V1 root input.
            // 只有后继执行可以取走子代理 inbox 消息，并将其写入可靠 V1 根输入。
            // Legacy controller intervention is overwritten by V1 physical-request reconstruction.
            // 旧控制器干预会被 V1 物理请求重建覆盖，不能充当消息消费入口。
            let mut child_event_stream =
                Box::pin(drive_runtime_stream_provider_loop(config, launch, None));
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
            let compacted_snapshot = Box::pin(run_task_session_context_compaction(
                Arc::clone(&background_task_runtime),
                context.clone(),
                background_request.clone(),
                background_resolved_config.clone(),
                ChildTaskCompactionPhase::ActiveBusiness,
            ))
            .await?
            .ok_or_else(|| {
                format!(
                    "Task `{}` required mid-turn context compaction after `{reason}`, but compaction produced no snapshot.",
                    background_request.task_id
                )
            })?;
            background_request.snapshot = compacted_snapshot;
            mid_turn_compaction_attempted = true;
            launch = Box::pin(prepare_task_runtime_stream_launch(
                &context,
                &background_request,
                &background_resolved_config,
            ))
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
    }));
    let child_run_abort_handle = child_run_task.abort_handle();
    let child_run_abort_guard = AbortChildRunOnDrop {
        handle: child_run_abort_handle.clone(),
    };
    if let Err(error) = detached_stream_startup_guard.attach_abort_handle(child_run_abort_handle) {
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
    // Move the lease into the finalizer without an await gap before installing that finalizer.
    // 将租约移入终态处理器，安装该处理器之前不留 await 间隙。
    let active_task_run_guard = detached_stream_startup_guard.take_active_run()?;
    tokio::spawn(async move {
        let mut active_task_run_guard = Some(active_task_run_guard);
        let _child_run_abort_guard = child_run_abort_guard;
        let child_run_result = child_run_task.await;
        match child_run_result {
            Ok(Ok(completion)) => {
                let outcome = completion.outcome;
                // outcome_committed keeps stale physical executions from publishing deferred terminal feedback.
                // outcome_committed 防止过期物理执行发布延迟的终态反馈。
                let mut outcome_committed = false;
                let store_result = task_runtime
                    .store_task_outcome_without_parent_mailbox(
                        request.current_session_id,
                        &request,
                        &outcome,
                    )
                    .map(|committed| outcome_committed = committed);
                // The durable parent return publisher settles success together with the exact content.
                // 父侧可靠回流发布器将成功终态与精确正文一起结算。
                // Keep the durable child obligation active while the result is still in transit.
                // 结果仍在传输时保留持久子任务完成约束。
                let store_and_lifecycle_result = store_result.map_err(|error| {
                    format!(
                        "The subagent task completed, but its terminal state could not be recorded: {error}"
                    )
                });
                match combine_task_run_finalization_results(
                    store_and_lifecycle_result,
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
                                &request,
                            )
                            .await;
                        publish_task_finished_to_parent_after_resume_decision(
                            &task_runtime,
                            &request,
                            &outcome,
                            "The subagent task has completed.",
                            outcome_committed,
                            resumed_queued_messages,
                        );
                        if completion.should_compact_context
                            && outcome_committed
                            && !resumed_queued_messages
                        {
                            spawn_task_session_context_compaction(
                                Arc::clone(&task_runtime),
                                compaction_runtime_context,
                                request.as_ref().clone(),
                                outcome,
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
                // outcome_committed keeps stale stopped executions from publishing deferred terminal feedback.
                // outcome_committed 防止过期的已停止执行发布延迟终态反馈。
                let mut outcome_committed = false;
                let store_result = task_runtime
                    .store_task_outcome_without_parent_mailbox(
                        request.current_session_id,
                        &request,
                        &outcome,
                    )
                    .map(|committed| outcome_committed = committed);
                // The host publishes stop evidence and releases the owned obligation atomically.
                // 宿主原子发布停止证据并解除所属任务义务。
                let store_and_lifecycle_result = store_result.map_err(|error| {
                    format!(
                        "The stopped subagent task could not record its terminal state: {error}"
                    )
                });
                match combine_task_run_finalization_results(
                    store_and_lifecycle_result,
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
                                &request,
                            )
                            .await;
                        publish_task_finished_to_parent_after_resume_decision(
                            &task_runtime,
                            &request,
                            &outcome,
                            outcome.final_output_text.as_str(),
                            outcome_committed,
                            resumed_queued_messages,
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
                // failure_committed keeps stale failed executions from publishing deferred terminal feedback.
                // failure_committed 防止过期失败执行发布延迟终态反馈。
                let mut failure_committed = false;
                let mark_result = task_runtime
                    .mark_task_failure_without_parent_mailbox(
                        request.current_session_id,
                        &request,
                        &error,
                    )
                    .map(|committed| failure_committed = committed);
                // Failure remains an owned obligation until the host durably records its report.
                // 在宿主可靠记录报告之前，失败执行仍保留所属任务义务。
                let mark_and_lifecycle_result = mark_result.map_err(|record_error| {
                        format!(
                            "The subagent task failed with `{error}`, and recording that failure also failed: {record_error}"
                        )
                    });
                match combine_task_run_finalization_results(
                    mark_and_lifecycle_result,
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
                                &request,
                            )
                            .await;
                        publish_task_failure_to_parent_after_resume_decision(
                            &task_runtime,
                            &request,
                            &error,
                            failure_committed,
                            resumed_queued_messages,
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
                // outcome_committed keeps stale cancelled executions from publishing deferred terminal feedback.
                // outcome_committed 防止过期的已取消执行发布延迟终态反馈。
                let mut outcome_committed = false;
                let store_result = task_runtime
                    .store_task_outcome_without_parent_mailbox(
                        request.current_session_id,
                        &request,
                        &outcome,
                    )
                    .map(|committed| outcome_committed = committed);
                // Cancellation uses the same durable stopped-result publisher as an explicit stop.
                // 取消与明确停止使用相同的可靠停止结果发布器。
                let store_and_lifecycle_result = store_result.map_err(|error| {
                    format!(
                        "The cancelled subagent task could not record its stopped state: {error}"
                    )
                });
                match combine_task_run_finalization_results(
                    store_and_lifecycle_result,
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
                                &request,
                            )
                            .await;
                        publish_task_finished_to_parent_after_resume_decision(
                            &task_runtime,
                            &request,
                            &outcome,
                            outcome.final_output_text.as_str(),
                            outcome_committed,
                            resumed_queued_messages,
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
                // failure_committed keeps stale joined executions from publishing deferred terminal feedback.
                // failure_committed 防止过期的 join 失败执行发布延迟终态反馈。
                let mut failure_committed = false;
                let mark_result = task_runtime
                    .mark_task_failure_without_parent_mailbox(
                        request.current_session_id,
                        &request,
                        &error,
                    )
                    .map(|committed| failure_committed = committed);
                // A panicked child also needs its durable report before parent terminal settlement.
                // 崩溃子任务同样需要可靠报告先于父侧终态结算。
                let mark_and_lifecycle_result = mark_result.map_err(|record_error| {
                        format!(
                            "The subagent task failed with `{error}`, and recording that failure also failed: {record_error}"
                        )
                    });
                match combine_task_run_finalization_results(
                    mark_and_lifecycle_result,
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
                                &request,
                            )
                            .await;
                        publish_task_failure_to_parent_after_resume_decision(
                            &task_runtime,
                            &request,
                            &error,
                            failure_committed,
                            resumed_queued_messages,
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
        };
        match task_runtime.task_has_active_descendant_runs(task_id) {
            // A confirmed empty descendant set is the only authority allowed to close this physical execution stream.
            // 仅权威确认后代集合为空时，才允许关闭当前物理执行流。
            Ok(false) => {
                // A queued successor owns a new execution id, so closing this old stream cannot close the resumed run.
                // 排队后继执行持有新的执行标识，因此关闭该旧流不会关闭续跑。
                if let Err(error) =
                    task_runtime.close_detached_task_event_stream(task_id, &request.execution_id)
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
        task_runtime_trace(
            "shared_task_background_completed",
            format!(
                "task={} elapsed_ms={}",
                task_id,
                current_unix_millis_u64().saturating_sub(trace_started_at)
            ),
        );
    });
    detached_stream_startup_guard.handoff_to_background_finalizer()?;
    // A receiver already cancelled by stop is settled by its installed finalizer; never reopen startup.
    // 接收端若已被停止操作取消，将由已安装终态处理器结算，绝不重新打开启动阶段。
    let _ = child_run_start.send(());
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

/// Spawns a child future behind explicit handoff permission.
/// 在明确交接许可之后派生子任务 Future。
/// `run` may prepare or write child state only after the returned sender grants permission.
/// 只有返回的发送端授予许可后，`run` 才能准备或写入子状态。
/// Returns the one-use permission sender and the finalizer-owned join handle; withdrawal skips `run`.
/// 返回一次性许可发送端及终态处理器所属的连接句柄；撤回许可会跳过 `run`。
pub(super) fn spawn_child_run_after_handoff<F, T>(
    run: F,
) -> (
    tokio::sync::oneshot::Sender<()>,
    tokio::task::JoinHandle<Result<T, String>>,
)
where
    F: std::future::Future<Output = Result<T, String>> + Send + 'static,
    T: Send + 'static,
{
    // Dropping permission is an explicit failed handoff, never implicit authorization to launch.
    // 丢弃许可明确表示交接失败，绝不隐式授权启动。
    let (permission, admitted) = tokio::sync::oneshot::channel();
    // The finalizer receives this handle before the caller may release permission.
    // 调用方释放许可之前，终态处理器先取得此句柄。
    let task = tokio::spawn(async move {
        admitted
            .await
            .map_err(|_| String::from("Child startup handoff permission was withdrawn."))?;
        run.await
    });
    (permission, task)
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
/// `request` identifies the exact settled execution and its direct parent.
/// `request` 标识精确的已收敛执行及其直接父级。
///
/// # Returns
/// 返回值
/// `true` when a successor run, pending message, or explicit resume failure owns parent feedback.
/// 当后继运行、待处理消息或显式续跑失败拥有父级反馈时返回 `true`。
pub(super) async fn resume_queued_messages_after_active_run_released(
    task_runtime: &Arc<TaskRuntime>,
    request: &TaskExecutionRequest,
) -> bool {
    match task_runtime
        .resume_agent_from_inbox_after_task_settled(request.current_session_id, request.task_id)
        .await
    {
        Ok(true) => true,
        Ok(false) => {
            // successor_active covers an already-drained inbox whose new execution won the start race.
            // successor_active 覆盖 inbox 已被取走且新执行赢得启动竞态的情况。
            let successor_active =
                match task_runtime.task_has_active_or_starting_run(request.task_id) {
                    Ok(active) => active,
                    Err(error) => {
                        let detail = format!(
                            "Queued child-agent successor ownership could not be verified: {error}"
                        );
                        eprintln!("[subagent-agent-control] {detail}");
                        publish_task_failure_to_parent(task_runtime, request, &detail);
                        return true;
                    }
                };
            if successor_active {
                return true;
            }
            // queued_message_count distinguishes a true no-op from an admitted message that failed to start.
            // queued_message_count 区分真正的空操作与已准入但未能启动的消息。
            match task_runtime.inter_agent_message_count_for_task(request.task_id) {
                Ok(0) => false,
                Ok(queued_message_count) => {
                    let detail = format!(
                        "{queued_message_count} queued child-agent message(s) remain after the previous execution settled, but no successor execution started."
                    );
                    eprintln!("[subagent-agent-control] {detail}");
                    publish_task_failure_to_parent(task_runtime, request, &detail);
                    true
                }
                Err(error) => {
                    let detail = format!(
                        "Queued child-agent message ownership could not be verified after the previous execution settled: {error}"
                    );
                    eprintln!("[subagent-agent-control] {detail}");
                    publish_task_failure_to_parent(task_runtime, request, &detail);
                    true
                }
            }
        }
        Err(error) => {
            let detail = format!(
                "Queued child-agent messages could not resume after the previous execution settled: {error}"
            );
            eprintln!("[subagent-agent-control] {detail}");
            publish_task_failure_to_parent(task_runtime, request, &detail);
            true
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
/// `task_runtime` owns publication, `context` owns execution dependencies, `request` binds identity,
/// and `outcome` supplies the committed terminal history and configuration; returns after scheduling.
/// `task_runtime`持有发布，`context`持有执行依赖，`request`绑定身份，
/// `outcome`提供已提交终态历史和配置；调度后返回。
pub(super) fn spawn_task_session_context_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    mut request: TaskExecutionRequest,
    outcome: TaskExecutionOutcome,
) {
    if !outcome
        .resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled
    {
        return;
    }

    // Maintenance must summarize the committed terminal history, including all calls and the final assistant.
    // 维护必须总结已提交终态历史，包括全部调用及最终助手回复。
    request.snapshot = outcome.snapshot;
    // Preserve the exact final controller configuration rather than the initial resume configuration.
    // 保留精确最终控制器配置，而不是初始续跑配置。
    request.controller_config = Some(outcome.controller_config);
    // The terminal outcome owns the effective policy used for this maintenance attempt.
    // 终态结果持有本次维护尝试使用的有效策略。
    let resolved_config = outcome.resolved_config;

    // Native maintenance publishes its canonical document; its later result must not overwrite a concurrently resumed resident mirror.
    // 原生维护发布规范文档；其迟到结果不能覆盖同时续跑的常驻镜像。
    let uses_native_host = context
        .request_template
        .native_agent_collaboration_runtime
        .is_some();

    tokio::spawn(async move {
        // Native hosts own publication waiting together with cancellation and durable cleanup.
        // 原生宿主将发布等待、取消及可靠清理一起持有。
        let compacted_snapshot = match run_task_session_context_compaction(
            Arc::clone(&task_runtime),
            context,
            request.clone(),
            resolved_config,
            ChildTaskCompactionPhase::CompletedBusinessMaintenance,
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
        // The host's auxiliary exclusion has already been released; only standalone maintenance owns this resident update.
        // 宿主辅助互斥已经释放；仅独立运行时维护持有此常驻更新。
        if uses_native_host {
            return;
        }
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
/// `phase` is chosen by the lifecycle caller and distinguishes an active lease from completed maintenance.
/// `phase` 由生命周期调用方选择，区分活动租约与完成后维护。
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
    phase: ChildTaskCompactionPhase,
) -> Result<Option<CoreConversationSnapshot>, String> {
    if let Some(host) = context
        .request_template
        .native_agent_collaboration_runtime
        .as_ref()
    {
        // A native child must never plan against the resident snapshot that the host will replace on dispatch.
        // 原生子代理绝不能依据派发时会被宿主替换的驻留快照规划。
        let (provider_configuration, preparation, model_target) =
            task_compaction_launch_inputs(&context, &request, &resolved_config)?;
        return host
            .compact_child_task(
                crate::agent_runtime::prompt_compaction::ChildTaskCompactionRequest {
                    phase,
                    invocation: request,
                    preparation,
                    provider_configuration,
                    model_target,
                    foundation: Arc::clone(&context.foundation),
                    luaskills_runtime: context.luaskills_runtime.clone(),
                },
            )
            .await;
    }
    // Select active host instructions once for both summary input and frozen-tail planning.
    // 为摘要输入和冻结尾段规划一次性选择有效宿主指令。
    let mut conversation_state = vulcan_context::runtime_host::prepare_runtime_host_compaction(
        &core_snapshot_to_session_conversation_state(&request.snapshot)?,
    )?;
    let messages = conversation_state_to_compaction_source_messages(&conversation_state)?;
    if messages.is_empty() {
        return Ok(None);
    }

    // The preparation request carries the exact provider overrides already resolved for this task.
    // 准备请求携带已为本次任务执行解析的精确供应商覆盖。
    let (provider_configuration, compaction_preparation_request, model_target) =
        task_compaction_launch_inputs(&context, &request, &resolved_config)?;
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
        ownership: crate::agent_runtime::prompt_compaction::PromptCompactionOwnership::RuntimeOnly,
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

/// Resolves the original child's exact provider preparation and summary target for either durable or standalone execution.
/// 为可靠或独立执行解析原子任务的精确供应商准备及摘要目标。
/// The context, invocation and resolved configuration retain the original scope; returns exact launch inputs or an error.
/// 上下文、调用与已解析配置保留原范围；返回精确启动输入或错误。
fn task_compaction_launch_inputs(
    context: &TaskExecutionRuntimeContext,
    request: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<
    (
        ProviderConfigurationFile,
        ExecutionPreparationRequest,
        crate::agent_runtime::prompt_compaction::PromptCompactionModelTarget,
    ),
    String,
> {
    // Reuse the same preparation path as the actual child request, including its exact route overrides.
    // 复用实际子请求的相同准备路径，包括精确路由覆盖。
    let (provider_configuration, preparation) = if context
        .request_template
        .native_agent_collaboration_runtime
        .is_some()
    {
        super::runtime_launch::build_task_runtime_compaction_controls(
            context,
            request,
            resolved_config,
        )?
    } else {
        build_task_runtime_preparation_request(context, request, resolved_config)?
    };
    // The declared runtime is the only provider resolver; no alternate protocol is selected.
    // 声明的运行时是唯一供应商解析器；不选择替代协议。
    let public_ai_runtime = context.foundation.public_ai_runtime().ok_or_else(|| {
        String::from("TaskSession context compaction requires the public AI runtime.")
    })?;
    // Behavior policy is resolved together with this exact task's model window.
    // 行为策略与此精确任务的模型窗口一同解析。
    let behavior_seed = task_context_compaction_policy_from_snapshot(
        &resolved_config.effective_config.context_compaction,
    );
    // Freeze the route before the host reads any compaction source.
    // 在宿主读取任何压缩来源之前冻结路由。
    let target = resolve_prompt_compaction_model_target(
        public_ai_runtime.as_ref(),
        &preparation.session_id.to_string(),
        preparation.provider_route_lease_identity.as_ref(),
        preparation.provider_id_override.as_deref(),
        preparation.connection_id_override.as_deref(),
        preparation.model_id_override.as_deref(),
        preparation.context_compaction_window_node_id,
        Some(behavior_seed),
    )?
    .ok_or_else(|| {
        String::from("TaskSession context compaction requires an exact model target.")
    })?;
    Ok((provider_configuration, preparation, target))
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
