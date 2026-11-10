//! Context compaction streaming helpers.
//! 上下文压缩 streaming 辅助逻辑。

use super::*;
use crate::{
    ActiveStreamTaskRegistrationMode, ProviderTelemetryHookGuard, ProviderV1FinishedDurability,
    ProviderV1SinkError, ProviderV1StreamSink, ProviderV1TerminalOutcome,
    ProviderV1ToolBoundaryOutcome, Uuid, emit_context_compaction_preview,
    emit_transient_reasoning_status, provider_v1_stream_sink_for_session,
    spawn_registered_active_stream_task_with_mode,
};
use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryEventKind;
use vulcan_agent_runtime::agent_runtime::provider_v1_bridge::ProviderV1LifecycleFacts;
use vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::{
    acknowledge_runtime_event_projection_barrier_from_event,
    acknowledge_tool_batch_projection_barrier_from_event,
    reject_runtime_event_projection_barrier_from_event_with_reason,
};
use vulcan_context_compaction::{
    extract_compaction_model_summary, preview_compaction_model_output,
};

/// Prompt-compaction result paired with the exact durable V1 semantic and API receipts.
/// 提示词压缩结果及其精确可靠 V1 语义与 API 回执。
pub(crate) struct WebuiContextCompactionExecution {
    /// Validated compaction result returned by the Agent runtime.
    /// Agent 运行时返回的已校验压缩结果。
    pub(crate) execution: PromptCompactionExecution,
    /// Inclusive durable boundary retained by the next context generation.
    /// 下一上下文代际保留的可靠包含式边界。
    pub(crate) retained_from_seq: Seq,
    /// Durable V1 facts produced by the same physical compaction API.
    /// 同一次物理压缩 API 产生的可靠 V1 事实。
    pub(crate) durability: ProviderV1FinishedDurability,
}

/// Streams one prompt-compaction request and mirrors provider deltas into WebUI state.
/// 流式执行一次提示词压缩请求，并将 Provider 增量镜像到 WebUI 状态。
///
/// # Parameters
/// # 参数
/// `app` emits WebUI workbench updates.
/// `app` 发出 WebUI 工作台更新。
/// `state` owns the retained workspace snapshot.
/// `state` 持有保留的工作区快照。
/// `request` is the ready compaction job request.
/// `request` 是已就绪的压缩任务请求。
/// `session_id` selects the compacted session.
/// `session_id` 选择被压缩的会话。
/// `section_id` identifies the visible compaction status section.
/// `section_id` 标识可见压缩状态分段。
/// `budget_note` describes the triggering context budget.
/// `budget_note` 描述触发压缩的上下文预算。
/// `trigger` labels the manual or automatic compaction source.
/// `trigger` 标记手动或自动压缩来源。
///
/// # Returns
/// # 返回值
/// The terminal compaction execution when a durable summary was produced.
/// 当产出持久摘要时返回终态压缩执行结果。
// Compaction keeps durable identity, UI presentation and borrowed execution owners separate.
// 压缩流程分别保留持久化身份、界面展示及借用的执行所有者。
#[expect(
    clippy::too_many_arguments,
    reason = "Compaction keeps durable identity, UI presentation and borrowed execution owners separate."
)]
pub(crate) async fn stream_webui_context_compaction_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    request: PromptCompactionJobRequest,
    session_id: SessionId,
    retained_from_seq: Seq,
    section_id: &str,
    budget_note: &str,
    trigger: &str,
) -> Result<WebuiContextCompactionExecution, String> {
    // outputMessageId identifies the visible compaction status output row.
    // outputMessageId 标识可见的压缩状态输出行。
    let mut output_message_id: Option<String> = None;
    // rawAssistantOutput retains the exact wire text until terminal normalization.
    // rawAssistantOutput 保留精确线缆文本，直到终态规范化。
    let mut raw_assistant_output = String::new();
    // previewApiCallId is the compaction API whose live preview must be cleared after the attempt.
    // previewApiCallId 是本次尝试结束后必须清除实时预览的压缩 API。
    let mut preview_api_call_id: Option<u64> = None;
    // terminalResult retains the Agent runtime result without becoming a persistence authority.
    // terminalResult 保留 Agent 运行时结果，但不成为持久化权威。
    let mut terminal_result = None;
    // projectionError stops the stream after the first V1 ownership or projection failure.
    // projectionError 在首次 V1 所有权或投影失败后停止流。
    let mut projection_error = None;
    // providerV1Sink is the unique durable owner for this session compaction request.
    // providerV1Sink 是当前会话压缩请求的唯一可靠所有者。
    let mut provider_v1_sink = provider_v1_stream_sink_for_session(state, session_id)?
        .ok_or_else(|| {
            String::from(
                "Context compaction requires a registered V1 session owner before Provider dispatch.",
            )
        })?;
    provider_v1_sink.bind_compaction_source(&request.full_context_state)?;
    // providerTelemetryGuard closes an active compaction request if the stream future is dropped.
    // providerTelemetryGuard 在压缩流 Future 被丢弃时关闭活动请求。
    let mut provider_telemetry_guard = ProviderTelemetryHookGuard::new_with_persistence(
        state.clone(),
        Some(controller_runtime.clone()),
    );
    // stream is the single physical compaction execution.
    // stream 是唯一的物理压缩执行。
    let mut stream = Box::pin(stream_prompt_compaction_job(request));
    while let Some(event) = stream.next().await {
        match event {
            PromptCompactionStreamEvent::PreparedSource { source } => {
                if let Err(error) = provider_v1_sink.select_compaction_source(&source) {
                    projection_error = Some(error);
                    break;
                }
                clear_context_compaction_preview(app, state, session_id, &mut preview_api_call_id);
                // A retry's durable summary must contain only that attempt's output.
                // 重试的可靠摘要只能包含本次尝试的输出。
                raw_assistant_output.clear();
            }
            PromptCompactionStreamEvent::Progress(progress) => {
                if let Err(error) = append_webui_context_compaction_progress(
                    app,
                    state,
                    session_id,
                    section_id,
                    budget_note,
                    trigger,
                    &progress,
                ) {
                    tracing::warn!(
                        "[webui-service] context compaction progress projection failed: {error}"
                    );
                }
            }
            PromptCompactionStreamEvent::Stream(event) => {
                // outputDelta marks events that grow the live summary preview.
                // outputDelta 标记会增长实时摘要预览的事件。
                let output_delta = matches!(event, ExecutionStreamEvent::OutputDelta { .. });
                if let Err(error) = apply_context_compaction_stream_event(
                    app,
                    state,
                    Some(CompactionStreamPresentation {
                        session_id,
                        section_id,
                        budget_note,
                        trigger,
                    }),
                    &mut output_message_id,
                    &mut raw_assistant_output,
                    &mut provider_v1_sink,
                    &mut provider_telemetry_guard,
                    event,
                )
                .await
                {
                    projection_error = Some(error);
                    break;
                }
                if output_delta {
                    publish_context_compaction_preview(
                        app,
                        state,
                        session_id,
                        &provider_v1_sink,
                        &raw_assistant_output,
                        &mut preview_api_call_id,
                    );
                }
            }
            PromptCompactionStreamEvent::Finished(result) => {
                terminal_result = Some(result.and_then(|execution| {
                    if execution.fallback_used {
                        return Err(format!(
                            "The API returned no usable compaction result; this context compaction failed. Provider request: {}.",
                            execution.provider_request_count
                        ));
                    }
                    Ok(execution)
                }));
                break;
            }
        }
    }
    // The durable summary or terminal state owns the card from here on.
    // 从这里开始由持久摘要或终态接管卡片。
    clear_context_compaction_preview(app, state, session_id, &mut preview_api_call_id);
    if let Some(event) = provider_telemetry_guard.take_cancelled_event() {
        provider_telemetry_guard.forward(event);
        if let Err(error) =
            crate::session_plan_runtime::emit_session_addons_changed_delta(app, state, session_id)
        {
            tracing::warn!("[session-usage] cancellation projection: {error}");
        }
    }
    let result = (|| {
        if let Some(error) = projection_error {
            return Err(error);
        }
        // execution is the validated Agent-level compaction result.
        // execution 是已校验的 Agent 层压缩结果。
        let execution = terminal_result.unwrap_or_else(|| {
            Err(String::from(
                "The model summary stream ended early; this prompt compaction was cancelled.",
            ))
        })?;
        // durability contains the exact V1 API and normalized assistant semantic receipts.
        // durability 包含精确的 V1 API 与规范化 assistant 语义回执。
        let durability = provider_v1_sink.take_finished_durability();
        super::completion::verified_compaction_commit_input(
            &execution,
            &durability,
            retained_from_seq,
        )?;
        Ok(WebuiContextCompactionExecution {
            execution,
            retained_from_seq,
            durability,
        })
    })();
    // The sink, not the latest document request, identifies an early failed summary API.
    // 提前失败摘要的 API 由接收器标识，不从文档最近请求推断。
    if trigger == MANUAL_CONTEXT_COMPACTION_TRIGGER
        && let Err(error) = &result
        && let Ok(api_id) = provider_v1_sink.request_preview_api_id()
    {
        let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
            state, session_id,
        )?;
        let api_id = vulcan_session_core::v1::ids::ApiCallId::try_new(api_id)
            .map_err(|error| error.to_string())?;
        let now_ms = crate::current_v1_unix_millis()?;
        handle
            .mutate_durable_if_changed(
                crate::session_runtime_registry_v1::SessionMutationAdmission::ExistingTurn,
                crate::session_runtime_registry_v1::SessionFlushReason::ApiCompleted,
                |document| {
                    use crate::session_runtime_registry_v1::SessionMutationChange;
                    Ok(
                        if document.fail_manual_compaction_api(api_id, error, now_ms)? {
                            SessionMutationChange::Changed(())
                        } else {
                            SessionMutationChange::Unchanged(())
                        },
                    )
                },
            )
            .map_err(|settlement| {
                format!("{error}; manual summary API settlement failed: {settlement}")
            })?;
    }
    result
}

/// Publishes the display-only summary preview of the active compaction API.
/// 发布活动压缩 API 的仅显示摘要预览。
/// `raw_assistant_output` is the cumulative output of this attempt; `preview_api_call_id` records the
/// API whose preview must be cleared later. Preview failures are logged and never fail the compaction.
/// `raw_assistant_output` 是本次尝试的累计输出；`preview_api_call_id` 记录稍后需要清除预览的 API。
/// 预览失败只记录日志，绝不导致压缩失败。
fn publish_context_compaction_preview(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    provider_v1_sink: &ProviderV1StreamSink,
    raw_assistant_output: &str,
    preview_api_call_id: &mut Option<u64>,
) {
    let api_call_id = match provider_v1_sink.request_preview_api_id() {
        Ok(api_call_id) => api_call_id,
        Err(error) => {
            tracing::warn!("[webui-service] context compaction preview has no active API: {error}");
            return;
        }
    };
    *preview_api_call_id = Some(api_call_id);
    let preview = preview_compaction_model_output(raw_assistant_output);
    if let Err(error) =
        emit_context_compaction_preview(app, state, session_id, api_call_id, Some(preview))
    {
        tracing::warn!("[webui-service] context compaction preview emit failed: {error}");
    }
}

/// Clears the live summary preview recorded in `preview_api_call_id`, if any.
/// 清除 `preview_api_call_id` 中记录的实时摘要预览（如有）。
fn clear_context_compaction_preview(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    preview_api_call_id: &mut Option<u64>,
) {
    let Some(api_call_id) = preview_api_call_id.take() else {
        return;
    };
    if let Err(error) = emit_context_compaction_preview(app, state, session_id, api_call_id, None) {
        tracing::warn!("[webui-service] context compaction preview clear failed: {error}");
    }
}

/// Spawns the selected-session context-compaction stream and mirrors it into WebUI state.
/// 启动当前会话的上下文压缩流，并将流式事件镜像到 WebUI 状态。
/// `model_target` is the exact route and adaptive tail policy frozen before planning.
/// `model_target` 是规划前冻结的精确路由与自适应尾部策略。
/// `source` binds manual admission to the same durable history used by the plan and media projection.
/// `source` 将手动准入绑定到规划及媒体投影所用的同一可靠历史。
/// Returns after task registration; asynchronous failures are published as durable session errors.
/// 任务登记后返回；异步失败发布为可靠会话错误。
// Compaction keeps durable identity, UI presentation and borrowed execution owners separate.
// 压缩流程分别保留持久化身份、界面展示及借用的执行所有者。
#[expect(
    clippy::too_many_arguments,
    reason = "Compaction keeps durable identity, UI presentation and borrowed execution owners separate."
)]
pub(crate) fn spawn_webui_context_compaction_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    session_id: SessionId,
    previous_phase: SessionRunPhase,
    status_message_id: String,
    budget_note: String,
    trigger: String,
    plan: CompactionPlan,
    full_context_state: SessionConversationState,
    media_projection: Arc<
        dyn vulcan_agent_runtime::prompt_media_projection::PromptMediaProjectionRuntime,
    >,
    retained_from_seq: Seq,
    source: vulcan_session_core::v1::context_maintenance::ManualContextMaintenanceSource,
    model_target: PromptCompactionModelTarget,
) -> Result<(), String> {
    let request = build_webui_context_compaction_job_request(
        &state,
        &agent_runtime,
        session_id,
        plan,
        full_context_state,
        media_projection,
        None,
        model_target,
    );
    let task_state = state.clone();
    let task_app = app.clone();
    // registration_failure_controller_runtime persists a formal failure after task-body ownership moves its clone.
    // registration_failure_controller_runtime 在任务体取得其克隆所有权后持久化正式注册失败。
    let registration_failure_controller_runtime = controller_runtime.clone();
    // registration_failure_status_message_id retains the committed section identity for launch compensation.
    // registration_failure_status_message_id 为启动补偿保留已提交分段身份。
    let registration_failure_status_message_id = status_message_id.clone();
    // registration_failure_budget_note retains the exact visible budget diagnosis for failure persistence.
    // registration_failure_budget_note 为失败持久化保留精确可见预算诊断。
    let registration_failure_budget_note = budget_note.clone();
    // registration_failure_trigger retains the authoritative manual or automatic trigger label.
    // registration_failure_trigger 保留权威的手动或自动触发标签。
    let registration_failure_trigger = trigger.clone();
    let active_stream_task_id = Uuid::new_v4();
    // expected_kernel_generation binds registration failure cleanup to this exact compaction turn.
    // expected_kernel_generation 将注册失败清理绑定到当前精确压缩回合。
    let expected_kernel_generation = state
        .lock()
        .ok()
        .and_then(|guard| webui_session_kernel_generation(&guard, session_id));
    let task_future = async move {
        // Bind the exact V1 owner before preparing the summary request.
        // 在准备摘要请求前绑定精确的 V1 所有者。
        let admission =
            super::manual::begin(&task_state, session_id, active_stream_task_id, source).await;
        // Retain the admitted mode even when later request preparation fails.
        // 即使后续请求准备失败，也保留已准入的结算模式。
        let (admitted_request, commit_mode) = match admission {
            Ok(mode) => (request, mode),
            Err(error) => (
                Err(error),
                ContextCompactionCommitMode::FinishStandalone {
                    task_id: active_stream_task_id,
                },
            ),
        };
        // Both manual owners need the ordinary-turn declarations to reuse the provider prompt cache.
        // 两种手动压缩归属都需要普通回合的工具声明，才能复用供应商提示缓存。
        let admitted_request = match admitted_request {
            Ok(mut request) => {
                super::manual_tool_visibility::apply_manual_compaction_tool_visibility(
                    &task_app,
                    &task_state,
                    &agent_runtime,
                    &mut request,
                )
                .await
                .map(|()| request)
            }
            Err(error) => Err(error),
        };
        let terminal_result = match admitted_request {
            Ok(request) => {
                stream_webui_context_compaction_execution(
                    &task_app,
                    &task_state,
                    &controller_runtime,
                    request,
                    session_id,
                    retained_from_seq,
                    &status_message_id,
                    &budget_note,
                    &trigger,
                )
                .await
            }
            Err(error) => Err(error),
        };
        // commit_mode keeps ownership validation, domain mutation, task removal, and kernel closure in one transaction.
        // commit_mode 将所有权校验、领域修改、任务移除与 kernel 关闭保留在同一事务中。
        // finish_result classifies both provider failures and fallback summaries before kernel closure.
        // finish_result 在关闭内核前同时分类 Provider 失败与回退摘要。
        let finish_result = match terminal_result {
            Ok(execution) => {
                finish_webui_context_compaction_execution(
                    &task_app,
                    &task_state,
                    &agent_runtime,
                    session_id,
                    previous_phase,
                    &status_message_id,
                    &budget_note,
                    &trigger,
                    commit_mode,
                    execution,
                )
                .await
            }
            Err(error) => fail_webui_context_compaction_execution(
                &task_app,
                &task_state,
                session_id,
                previous_phase,
                &status_message_id,
                &budget_note,
                &trigger,
                commit_mode,
                &error,
            )
            .map(
                |workspace_snapshot| ContextCompactionFinishOutcome::Rejected {
                    workspace_snapshot,
                    error,
                },
            ),
        };
        match finish_result {
            Ok(outcome) => {
                // rejection_error determines the durable reason without discarding its snapshot.
                // rejection_error 在不丢弃快照的前提下决定持久化原因。
                let rejection_error = outcome.rejection_error().map(str::to_owned);
                // snapshot contains either the successful checkpoint or the rejected-attempt record.
                // snapshot 包含成功检查点或被拒绝尝试记录。
                let snapshot = outcome.into_workspace_snapshot();
                if let Some(error) = rejection_error.as_ref() {
                    tracing::warn!("[webui-service] context compaction rejected: {error}");
                }
                if let Err(error) = persist_context_compaction_snapshot(
                    task_state,
                    controller_runtime,
                    snapshot,
                    session_id,
                    status_message_id.clone(),
                    Some(String::from(if rejection_error.is_some() {
                        "context_compaction_rejected"
                    } else {
                        "context_compaction"
                    })),
                )
                .await
                {
                    tracing::warn!(
                        "[webui-service] context compaction persistence failed: {error}"
                    );
                }
            }
            Err(error) => {
                tracing::warn!("[webui-service] context compaction finish failed: {error}");
                // A post-stream publication failure must settle the auxiliary owner instead of leaving a busy kernel.
                // 流结束后的发布失败必须结算辅助所有者，不能遗留忙碌内核。
                if let Err(settlement_error) = fail_webui_context_compaction_execution(
                    &task_app,
                    &task_state,
                    session_id,
                    previous_phase,
                    &status_message_id,
                    &budget_note,
                    &trigger,
                    commit_mode,
                    &error,
                ) {
                    tracing::warn!(
                        "[webui-service] manual compaction failure settlement failed: {settlement_error}"
                    );
                }
            }
        }
    };
    if let Err(error) = spawn_registered_active_stream_task_with_mode(
        &state,
        session_id,
        active_stream_task_id,
        None,
        ActiveStreamTaskRegistrationMode::RejectIfOccupied,
        task_future,
    ) {
        tracing::warn!("[webui-service] register context compaction stream task failed: {error}");
        // registration_error_text is committed into the durable failed compaction boundary.
        // registration_error_text 被提交到可持久化的压缩失败边界。
        let registration_error_text = error.to_string();
        // failure_commit combines exact launch ownership, domain failure state, and kernel closure.
        // failure_commit 组合精确启动所有权、领域失败状态与内核关闭。
        let failure_commit = commit_stream_task_registration_failure(
            &state,
            session_id,
            expected_kernel_generation,
            &error,
            |service_state| {
                // workspace owns the Running compaction section installed by the cold commit.
                // workspace 持有冷提交安装的 Running 压缩分段。
                let workspace = service_state
                    .workspace_snapshot
                    .as_mut()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
                apply_webui_context_compaction_failure_to_workspace(
                    workspace,
                    session_id,
                    previous_phase,
                    &registration_failure_status_message_id,
                    &registration_failure_budget_note,
                    &registration_failure_trigger,
                    active_stream_task_id,
                    &registration_error_text,
                )?;
                // phase_event is generated inside the exact launch-ownership transaction.
                // phase_event 在精确启动所有权事务内部生成。
                apply_session_phase_to_state(service_state, session_id, previous_phase)
            },
            |service_state, phase_event| {
                // workspace_snapshot is projected only after the generic boundary terminalizes the exact kernel generation.
                // workspace_snapshot 仅在通用边界终结精确内核代际后投影。
                let workspace_snapshot = service_state
                    .workspace_snapshot
                    .as_ref()
                    .ok_or_else(|| String::from("workspace snapshot is not ready"))?
                    .clone();
                Ok((workspace_snapshot, phase_event))
            },
        );
        match failure_commit {
            Ok(StreamTaskLaunchFailureCommit::Committed((snapshot, phase_event))) => {
                project_committed_context_compaction_surfaces(
                    &app,
                    &state,
                    session_id,
                    phase_event,
                );
                spawn_context_compaction_persist_task(
                    state.clone(),
                    registration_failure_controller_runtime,
                    snapshot,
                    session_id,
                    registration_failure_status_message_id,
                    Some(String::from("context_compaction_registration_failed")),
                );
                webui_trace(
                    "context_compaction_registration_failure_handled",
                    format!("session={session_id} outcome=domain_failure_committed"),
                );
            }
            Ok(StreamTaskLaunchFailureCommit::Skipped(outcome)) => webui_trace(
                "context_compaction_registration_failure_handled",
                format!("session={session_id} outcome={outcome:?}"),
            ),
            Err(cleanup_error) => tracing::warn!(
                "[webui-service] context compaction registration failure projection failed: {cleanup_error}"
            ),
        }
        return Err(error.to_string());
    }
    Ok(())
}

/// Optional root-session display identity; persistence is always owned by the supplied V1 sink.
/// 可选根会话展示身份；持久化始终由传入的 V1 接收器持有。
pub(crate) struct CompactionStreamPresentation<'a> {
    /// Exact root session whose transient display should change.
    /// 瞬态展示需要变化的精确根会话。
    pub(crate) session_id: SessionId,
    /// Existing display section, never used as API or task authority.
    /// 既有展示分段，绝不作为 API 或任务权威。
    pub(crate) section_id: &'a str,
    /// User-facing reason for the current input budget.
    /// 面向用户的当前输入预算原因。
    pub(crate) budget_note: &'a str,
    /// User-facing trigger label retained by the original root caller.
    /// 原根调用方保留的面向用户触发标签。
    pub(crate) trigger: &'a str,
}

/// Applies `event` through the exact `provider_v1_sink`, using `presentation` only for optional root UI.
/// 通过精确`provider_v1_sink`处理`event`，`presentation`仅用于可选根界面。
/// `app` and `state` own publication, buffers retain this attempt, and the telemetry guard owns cancellation.
/// `app`及`state`持有发布，缓冲保存本次尝试，遥测守卫持有取消处理。
/// Returns None for handled events; without root UI, non-tool host events are returned to the owning child listener.
/// 已处理事件返回 None；没有根界面时，非工具宿主事件交回所属子监听器。
/// Deferred host events retain their barrier; presentation absence never authorizes a child lifecycle transition.
/// 延迟宿主事件保留屏障；缺少展示绝不授权子生命周期转换。
// Compaction keeps durable identity, UI presentation and borrowed execution owners separate.
// 压缩流程分别保留持久化身份、界面展示及借用的执行所有者。
#[expect(
    clippy::too_many_arguments,
    reason = "Compaction keeps durable identity, UI presentation and borrowed execution owners separate."
)]
pub(crate) async fn apply_context_compaction_stream_event(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    presentation: Option<CompactionStreamPresentation<'_>>,
    output_message_id: &mut Option<String>,
    raw_assistant_output: &mut String,
    provider_v1_sink: &mut ProviderV1StreamSink,
    provider_telemetry_guard: &mut ProviderTelemetryHookGuard,
    mut event: ExecutionStreamEvent,
) -> Result<Option<ExecutionStreamEvent>, String> {
    // Bind formation before projection or handoff so a detached listener cannot claim an unbound compaction event.
    // 在投影或转交前绑定构造事件，避免分离监听器接管尚未绑定的压缩事件。
    if let ExecutionStreamEvent::RuntimeEvent { event, .. } = &mut event {
        provider_v1_sink
            .bind_tool_request_preview(event)
            .map_err(|error| format!("Failed to bind compaction tool request preview: {error}"))?;
    }
    if presentation.is_none()
        && matches!(&event, ExecutionStreamEvent::RuntimeEvent { event, .. }
            if event.tool_execution_boundary.is_none())
    {
        return Ok(Some(event));
    }
    // Only the exact sink handles physical preparation and semantic persistence.
    // 仅由精确接收器处理物理准备与语义持久化。
    let result = match event {
        ExecutionStreamEvent::Prepared(_)
        | ExecutionStreamEvent::ProviderLoweringRepair { .. }
        | ExecutionStreamEvent::RequestReady { .. } => Ok(()),
        ExecutionStreamEvent::ProviderRequestTelemetry(event) => {
            if event.kind == ProviderRequestTelemetryEventKind::FirstToken {
                // First-token evidence updates only the already admitted request with the same identity.
                // 首 Token 证据仅更新身份相同且已准入的请求。
                provider_telemetry_guard.observe_durable_boundary(&event);
            }
            provider_telemetry_guard.forward(event);
            Ok(())
        }
        ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion } => {
            // eventKind freezes the physical boundary classification before the telemetry handler consumes the event.
            // eventKind 在遥测处理器消费事件前冻结物理边界分类。
            let event_kind = event.kind;
            // lifecycle carries the only typed V1 request transition accepted by this session compaction.
            // lifecycle 携带当前会话压缩唯一接受的强类型 V1 请求转换。
            let lifecycle = event.v1_lifecycle.clone();
            if event_kind == ProviderRequestTelemetryEventKind::Completed {
                provider_telemetry_guard.observe_durable_boundary(&event);
            }
            let v1_result: Result<(), String> = async {
                match (event_kind, lifecycle) {
                    (
                        ProviderRequestTelemetryEventKind::Started,
                        Some(ProviderV1LifecycleFacts::RequestPrepared(_)),
                    ) => Err(
                        ProviderV1SinkError::UnexpectedRequestStartDurabilityBoundary.to_string(),
                    ),
                    (
                        ProviderRequestTelemetryEventKind::Dispatched,
                        Some(ProviderV1LifecycleFacts::TransportObserved(facts)),
                    ) => provider_v1_sink
                        .mark_transport_observed(facts)
                        .await
                        .map_err(|error| error.to_string()),
                    (
                        ProviderRequestTelemetryEventKind::Completed,
                        Some(ProviderV1LifecycleFacts::ApiTerminal(facts)),
                    ) => {
                        // Even an unusable summary consumed one physical API request.
                        // 即使摘要不可用，也已经消耗了一次物理 API 请求。
                        provider_v1_sink
                            .record_telemetry_terminal(&event, app, state)
                            .map_err(|error| error.to_string())?;
                        if facts.tool_call_count > 0 {
                            // Internal summary tools are policy-rejected, but their real batch must settle durably.
                            // 内部摘要工具由策略拒绝，但其真实批次仍须可靠结算。
                            // Preserve actual output; the final tool boundary rejects this attempt after API closure.
                            // 保留实际输出；最后工具边界将在 API 闭合后拒绝本次尝试。
                            if !raw_assistant_output.is_empty() {
                                provider_v1_sink
                                    .append_assistant_delta(raw_assistant_output, facts.completed_at_ms)
                                    .map_err(|error| error.to_string())?;
                            }
                            return match provider_v1_sink
                                .accept_terminal(facts)
                                .await
                                .map_err(|error| error.to_string())?
                            {
                                ProviderV1TerminalOutcome::AwaitingTools => Ok(()),
                                ProviderV1TerminalOutcome::Completed => Err(String::from(
                                    "Context compaction tool batch completed before its durable tool boundaries.",
                                )),
                            };
                        }
                        if facts.status == vulcan_session_core::v1::event::ApiCallStatus::Completed
                        {
                            // normalizedSummary is the sole Provider-visible compaction summary persisted in V1.
                            // normalizedSummary 是 V1 中唯一持久化且 Provider 可见的压缩摘要。
                            let Some(normalized_summary) =
                                extract_compaction_model_summary(raw_assistant_output)
                            else {
                                // Retain rejected physical output so its Provider source has an honest semantic owner.
                                // 保留被拒绝的物理输出，让 Provider 来源拥有真实语义归属。
                                // The error below prevents this output from becoming a committed compaction summary.
                                // 下方错误阻止此输出成为已提交压缩摘要。
                                provider_v1_sink
                                    .append_assistant_delta(
                                        raw_assistant_output,
                                        facts.completed_at_ms,
                                    )
                                    .map_err(|error| error.to_string())?;
                                // Unusable content does not erase the completed physical API or leave its permit open.
                                // 不可用内容不能抹去已完成物理 API，也不能让其许可保持开放。
                                provider_v1_sink
                                    .accept_terminal(facts)
                                    .await
                                    .map_err(|error| error.to_string())?;
                                return Err(String::from(
                                    "Provider output did not contain a valid compaction summary.",
                                ));
                            };
                            provider_v1_sink
                                .append_assistant_delta(&normalized_summary, facts.completed_at_ms)
                                .map_err(|error| error.to_string())?;
                        }
                        match provider_v1_sink
                            .accept_terminal(facts)
                            .await
                            .map_err(|error| error.to_string())?
                        {
                            ProviderV1TerminalOutcome::Completed => Ok(()),
                            ProviderV1TerminalOutcome::AwaitingTools => Err(String::from(
                                "Context compaction cannot complete with pending tools.",
                            )),
                        }
                    }
                    (
                        ProviderRequestTelemetryEventKind::Started
                        | ProviderRequestTelemetryEventKind::Dispatched
                        | ProviderRequestTelemetryEventKind::Completed,
                        Some(ProviderV1LifecycleFacts::ExcludedNonSession),
                    ) => Err(String::from(
                        "Session context compaction cannot use a non-session Provider lifecycle.",
                    )),
                    (ProviderRequestTelemetryEventKind::Started, _) => {
                        Err(ProviderV1SinkError::MissingTypedRequestFacts.to_string())
                    }
                    (ProviderRequestTelemetryEventKind::Completed, _) => {
                        Err(ProviderV1SinkError::MissingTypedTerminalFacts.to_string())
                    }
                    _ => Ok(()),
                }
            }
            .await;
            if let Err(error) = v1_result {
                let _ = completion.complete(Err(error.clone()));
                return Err(error);
            }
            provider_telemetry_guard.observe_durable_boundary(&event);
            provider_telemetry_guard.forward(event);
            let result = Ok(());
            let _ = completion.complete(result.clone());
            result
        }
        ExecutionStreamEvent::ProviderRequestPreparationBoundary {
            event,
            candidate,
            completion,
        } => {
            // permitResult is the exact V1 request rebuilt and durably started by the session owner.
            // permitResult 是由会话所有者精确重建并可靠启动的 V1 请求。
            let permit_result = match event.v1_lifecycle.clone() {
                Some(ProviderV1LifecycleFacts::RequestPrepared(facts)) => provider_v1_sink
                    .prepare_request(state, facts, candidate)
                    .await
                    .map_err(|error| error.to_string()),
                Some(ProviderV1LifecycleFacts::ExcludedNonSession) => Err(String::from(
                    "Session context compaction cannot use a non-session Provider preparation.",
                )),
                _ => Err(ProviderV1SinkError::MissingTypedRequestFacts.to_string()),
            };
            // Arm before releasing the producer, including cancellation before its Started echo.
            // 在放行生产者之前激活跟踪，覆盖其 Started 回送之前发生的取消。
            provider_telemetry_guard.observe_preparation_result(&event, &permit_result);
            let response = permit_result.as_ref().map(|_| ()).map_err(Clone::clone);
            let _ = completion.complete(permit_result);
            response
        }
        ExecutionStreamEvent::RequestStarted { .. } => {
            if let Some(presentation) = presentation {
                apply_session_phase(
                    app,
                    state,
                    presentation.session_id,
                    SessionRunPhase::Requesting,
                )
            } else {
                Ok(())
            }
        }
        ExecutionStreamEvent::ThinkingDelta {
            delta,
            display_only,
            ..
        } => {
            // nowMs binds the reasoning mutation to the observed stream boundary.
            // nowMs 将思考变更绑定到观测到的流边界。
            let now_ms = i64::try_from(current_unix_millis_value())
                .map_err(|_| String::from("Current timestamp exceeds the V1 i64 range."))?;
            provider_v1_sink
                .append_reasoning_delta(&delta, display_only, now_ms)
                .map_err(|error| error.to_string())
        }
        ExecutionStreamEvent::TransientReasoningStatusChanged { status, .. } => {
            if let Some(presentation) = presentation {
                emit_transient_reasoning_status(app, state, presentation.session_id, status)
            } else {
                Ok(())
            }
        }
        ExecutionStreamEvent::ThinkingCompleted { .. } => {
            // nowMs binds the reasoning terminal state to the observed stream boundary.
            // nowMs 将思考终态绑定到观测到的流边界。
            let now_ms = i64::try_from(current_unix_millis_value())
                .map_err(|_| String::from("Current timestamp exceeds the V1 i64 range."))?;
            provider_v1_sink
                .complete_reasoning(now_ms)
                .map_err(|error| error.to_string())
        }
        ExecutionStreamEvent::OutputDelta { delta, .. } => {
            raw_assistant_output.push_str(&delta);
            if let Some(presentation) = presentation {
                append_webui_context_compaction_output_delta(
                    app,
                    state,
                    presentation.session_id,
                    presentation.section_id,
                    presentation.budget_note,
                    presentation.trigger,
                    output_message_id,
                    &delta,
                )
            } else {
                Ok(())
            }
        }
        ExecutionStreamEvent::RuntimeEvent { event, .. } => {
            if event.tool_execution_boundary.is_some() {
                // tool_boundary_result retains both timestamp and sink failures so neither path
                // can leave the mandatory compaction projection barrier pending.
                // tool_boundary_result 同时保留时间戳与 sink 失败，使两条路径都不会遗留
                // 强制压缩投影屏障。
                let tool_boundary_result = match i64::try_from(current_unix_millis_value()) {
                    Ok(now_ms) => provider_v1_sink
                        .apply_tool_event(app, state, &event, now_ms)
                        .await
                        .map_err(|error| error.to_string()),
                    Err(_) => Err(String::from("Current timestamp exceeds the V1 i64 range.")),
                };
                // outcome is available only after the same compaction sink accepted the boundary.
                // outcome 仅在同一压缩 sink 接纳边界后可用。
                let outcome = match tool_boundary_result {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        let _ = reject_runtime_event_projection_barrier_from_event_with_reason(
                            &event, &error,
                        );
                        return Err(error);
                    }
                };
                if provider_v1_sink.has_durable_api_terminal() {
                    // Wait for every policy-rejected result before aborting, so peer tools never remain in flight.
                    // 等待每个策略拒绝结果落盘后才中止，避免同批工具遗留在途状态。
                    let error = String::from(
                        "Context compaction rejected a tool-bearing response after settling its tool batch.",
                    );
                    reject_runtime_event_projection_barrier_from_event_with_reason(&event, &error)?;
                    return Err(error);
                }
                let acknowledgement = match outcome {
                    ProviderV1ToolBoundaryOutcome::Applied => {
                        acknowledge_runtime_event_projection_barrier_from_event(&event)
                    }
                    ProviderV1ToolBoundaryOutcome::BatchPrepared(identities) => {
                        acknowledge_tool_batch_projection_barrier_from_event(&event, identities)
                    }
                };
                acknowledgement?;
                return Ok(None);
            }
            if let Some(presentation) = presentation {
                append_runtime_event_projection_and_release_barrier(
                    app,
                    state,
                    presentation.session_id,
                    &event,
                )?;
            }
            Ok(())
        }
        ExecutionStreamEvent::ConversationCheckpoint { .. } => Ok(()),
        ExecutionStreamEvent::ContextCompactionRequired { .. } => Ok(()),
        ExecutionStreamEvent::ContextCompactionPolicyRefreshRequired { .. } => Ok(()),
        ExecutionStreamEvent::ProviderRouteSwitchRequired { .. } => Ok(()),
        ExecutionStreamEvent::Finished(_) => Ok(()),
    };
    result.map(|()| None)
}

/// Returns the retained selected-session title used by compaction diagnostics.
/// 返回压缩诊断使用的保留会话标题。
pub(crate) fn current_session_title_from_state(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> String {
    state
        .lock()
        .ok()
        .and_then(|guard| {
            guard.workspace_snapshot.as_ref().and_then(|workspace| {
                workspace
                    .sessions
                    .iter()
                    .find(|session| session.id == session_id)
                    .map(|session| session.title.clone())
            })
        })
        .unwrap_or_else(|| format!("Session #{session_id}"))
}
