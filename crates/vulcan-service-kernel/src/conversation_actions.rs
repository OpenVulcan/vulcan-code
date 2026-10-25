//! Conversation action service handlers.
//! 会话操作服务处理器。

use super::*;
use vulcan_session_core::v1::document::ToolCompletion;
use vulcan_session_core::v1::event::ToolCallStatus;

/// Submits answers for the oldest visible ask-tool request and resumes the blocked tool call.
/// 提交当前可见 ask 工具请求的答案，并恢复被阻塞的工具调用。
///
/// # Parameters
/// 参数
/// `state` contains the authoritative shared question runtime.
/// `state` 包含权威共享问题运行时。
/// `request_id` identifies the exact pending Ask request.
/// `request_id` 标识精确的待回答 Ask 请求。
/// `answers` contains ordered answers aligned with the registered prompts.
/// `answers` 包含与已注册问题对齐的有序答案。
///
/// # Returns
/// 返回值
/// `true` after the runtime commits the reply; otherwise an explicit service error.
/// 运行时提交回答后返回 `true`；否则返回明确的服务错误。
pub(crate) fn service_reply_pending_question(
    state: SharedServiceBridgeState,
    request_id: String,
    answers: Vec<Vec<String>>,
) -> Result<bool, String> {
    let agent_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.agent_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; question runtime is not ready")
        })?
    };
    // Values are stored by their own service call; a reply label alone would skip storing them.
    // 值由专用服务调用存储；仅凭回复标签会跳过存储。
    if agent_runtime
        .question_runtime
        .list_pending()?
        .iter()
        .any(|request| request.id == request_id && request.environment.is_some())
    {
        return Err(String::from(
            "Environment value requests are answered through `service_submit_environment_values`.",
        ));
    }
    // The hook observes only a committed answer, after the pending lock is released and before
    // the tool resumes, so the asking turn cannot end first.
    // hook 只观察已提交的回答，在 pending 锁释放后、工具恢复前运行，因此提问所在回合不会先结束。
    let answered = agent_runtime
        .question_runtime
        .reply_then(&request_id, answers, |request, answers| {
            if !request.is_model_question() {
                return;
            }
            record_question_lifecycle_hook(&agent_runtime.conversation_hook_runtime, |hooks| {
                hooks.record_question_answered(
                    request.session_id,
                    &request.id,
                    &request.questions,
                    answers,
                )
            });
        })
        .map_err(|error| format!("failed to submit ask answers: {error}"))?;
    if !answered {
        return Err(format!(
            "pending ask request `{request_id}` is no longer active"
        ));
    }
    Ok(true)
}

/// Rejects the visible ask-tool request after explicit user confirmation.
/// 在用户明确确认后拒绝当前可见 ask 工具请求。
///
/// # Parameters
/// 参数
/// `state` contains the authoritative shared question runtime.
/// `state` 包含权威共享问题运行时。
/// `request_id` identifies the exact pending Ask request.
/// `request_id` 标识精确的待回答 Ask 请求。
///
/// # Returns
/// 返回值
/// `true` after the runtime commits the rejection; otherwise an explicit service error.
/// 运行时提交拒绝后返回 `true`；否则返回明确的服务错误。
pub(crate) fn service_reject_pending_question(
    state: SharedServiceBridgeState,
    request_id: String,
) -> Result<bool, String> {
    let agent_runtime = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        guard.agent_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; question runtime is not ready")
        })?
    };
    // The hook observes only a committed rejection, after the pending lock is released and
    // before the tool resumes; a rejection ends the run, so recording afterwards could land after
    // the asking turn closed.
    // hook 只观察已提交的拒绝，在 pending 锁释放后、工具恢复前运行；拒绝会结束本次运行，
    // 之后再记录可能落在提问回合关闭之后。
    let rejected = agent_runtime
        .question_runtime
        .reject_then(&request_id, |request| {
            if !request.is_model_question() {
                return;
            }
            record_question_lifecycle_hook(&agent_runtime.conversation_hook_runtime, |hooks| {
                hooks.record_question_rejected(request.session_id, &request.id)
            });
        })
        .map_err(|error| format!("failed to reject ask request: {error}"))?;
    if !rejected {
        return Err(format!(
            "pending ask request `{request_id}` is no longer active"
        ));
    }
    Ok(true)
}

/// Reports one committed Ask lifecycle change to conversation hooks without failing the commit.
/// 将一次已提交的 Ask 生命周期变化上报给会话 hook，且不使该提交失败。
///
/// Hooks only observe the change; a poisoned hook runtime is traced instead of undoing a durable
/// Ask transition.
/// hook 仅观察该变化；hook 运行时中毒时只记录追踪，不撤销已可靠落盘的 Ask 转换。
///
/// # Parameters
/// 参数
/// `hook_runtime` is the shared conversation hook runtime and `record` performs one observation.
/// `hook_runtime` 是共享会话 hook 运行时，`record` 执行一次观察记录。
fn record_question_lifecycle_hook(
    hook_runtime: &Mutex<ConversationHookRuntime>,
    record: impl FnOnce(&mut ConversationHookRuntime),
) {
    match hook_runtime.lock() {
        Ok(mut hooks) => record(&mut hooks),
        Err(error) => webui_trace("question_lifecycle_hook_failed", format!("error={error}")),
    }
}

/// Emits only the ask-modal pending question projection as a workbench delta.
/// 仅以工作台增量发送 ask 弹窗待回答问题投影。
pub(crate) fn emit_pending_question_changed_delta(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
) -> Result<(), String> {
    let event = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let pending_questions =
            pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
        let pending_question = pending_questions.first().cloned();
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        let generation = guard.workbench_generation;
        guard.workbench.revision = generation;
        guard.workbench.pending_question = pending_question.clone();
        guard.workbench.pending_questions = pending_questions.clone();
        let ServiceBridgeState {
            workspace_snapshot,
            workbench,
            ..
        } = &mut *guard;
        let workspace_snapshot = workspace_snapshot.as_ref();
        for conversation in &mut workbench.conversations {
            let Ok(session_id) = conversation.id.parse::<SessionId>() else {
                continue;
            };
            let session_plan_mode = workspace_snapshot
                .and_then(|workspace| workspace.session_runtime_state.get(&session_id))
                .and_then(|runtime_state| {
                    session_plan_mode_summary_from_runtime_state(Some(runtime_state))
                });
            refresh_conversation_attention_summary(
                conversation,
                session_id,
                &pending_questions,
                session_plan_mode.as_ref(),
            );
        }
        WorkbenchDeltaEnvelope {
            generation,
            delta: WorkbenchDelta::PendingQuestionChanged(PendingQuestionChangedDelta {
                conversation_id: pending_question
                    .as_ref()
                    .map(|question| question.session_id.clone()),
                pending_question,
                pending_questions,
            }),
        }
    };
    emit_service_event_with_trace(
        app,
        "pending_question_delta_emit",
        ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

/// Installs the WebUI projection callback used by the ask-question runtime.
/// 安装 ask 提问运行时使用的 WebUI 投影回调。
///
/// # Parameters
/// 参数
/// `app` broadcasts authoritative projection events to every registered client sink.
/// `app` 向每个已注册客户端 sink 广播权威投影事件。
/// `state` owns the retained workbench projection updated by the callback.
/// `state` 持有由回调更新的保留工作台投影。
/// `agent_runtime` owns the question runtime and the conversation hook runtime it reports to.
/// `agent_runtime` 持有问题运行时及其上报的会话 hook 运行时。
///
/// # Returns
/// 返回值
/// Success after installing the single projection callback, or a callback-registry error.
/// 安装唯一投影回调后返回成功；失败时返回回调注册表错误。
pub(crate) fn install_webui_question_runtime_event_bridge(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    agent_runtime: &AgentRuntimeBridge,
) -> Result<(), String> {
    let question_runtime = &agent_runtime.question_runtime;
    let hook_runtime = Arc::clone(&agent_runtime.conversation_hook_runtime);
    // registry is captured once because reply and rejection invoke the durability callback while
    // holding the pending-question lock, and workbench projections read pending questions while
    // holding the service state lock; taking the state lock here would invert that order.
    // registry 只捕获一次：回答与拒绝在持有待回答问题锁时调用可靠性回调，而工作台投影在持有
    // 服务状态锁时读取待回答问题；在此获取状态锁会反转该加锁顺序。
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?
            .session_runtime_registry,
    );
    // durabilityCallback is the only bridge allowed to advance an Ask tool before process-local visibility or resumption.
    // durabilityCallback 是在进程内可见或恢复前推进 Ask 工具的唯一桥接。
    let durability_callback: PendingQuestionDurabilityCallback = Arc::new(move |action| {
        let session_id = match &action {
            PendingQuestionDurabilityAction::Waiting { request }
            | PendingQuestionDurabilityAction::Answered { request, .. }
            | PendingQuestionDurabilityAction::Resumed { request }
            | PendingQuestionDurabilityAction::Cancelled { request, .. } => request.session_id,
        };
        let handle =
            crate::session_goal_lifecycle::registered_session_v1_handle(&registry, session_id)?;
        let now_ms = crate::session_goal_lifecycle::current_v1_goal_unix_millis()?;
        match action {
            PendingQuestionDurabilityAction::Waiting { request } => {
                handle
                    .mark_tool_waiting_user(request.api_call_id, request.tool_call_id, now_ms)
                    .map_err(|error| error.to_string())?;
                // Approvals and value requests are not model questions, so they never reach
                // question hooks.
                // 审批与取值请求不是模型提问，因此不会进入提问 hook。
                if request.is_model_question() {
                    // Registration commits outside the pending lock, so the hook lock nests under nothing.
                    // 注册在 pending 锁之外提交，因此 hook 锁不嵌套在任何锁之下。
                    record_question_lifecycle_hook(&hook_runtime, |hooks| {
                        hooks.record_question_asked(
                            request.session_id,
                            &request.id,
                            &request.questions,
                        )
                    });
                }
                Ok(())
            }
            PendingQuestionDurabilityAction::Resumed { request } => handle
                .resume_waiting_tool(request.api_call_id, request.tool_call_id, now_ms)
                .map(|_| ())
                .map_err(|error| error.to_string()),
            PendingQuestionDurabilityAction::Answered { request, result } => {
                let output = persistent_tool_output_from_result(&result)
                    .map_err(|error| error.to_string())?;
                handle
                    .answer_waiting_tool(
                        request.api_call_id,
                        request.tool_call_id,
                        ToolCompletion {
                            output: Some(output),
                            changeset: None,
                            status: if result.is_error {
                                ToolCallStatus::Failed
                            } else {
                                ToolCallStatus::Completed
                            },
                            finished_at_ms: now_ms,
                            external_refs: Vec::new(),
                            generated_resources: Vec::new(),
                        },
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
            PendingQuestionDurabilityAction::Cancelled { request, result } => {
                let output = persistent_tool_output_from_result(&result)
                    .map_err(|error| error.to_string())?;
                handle
                    .cancel_waiting_tool(
                        request.api_call_id,
                        request.tool_call_id,
                        Some(output),
                        now_ms,
                    )
                    .map(|_| ())
                    .map_err(|error| error.to_string())
            }
        }
    });
    question_runtime.set_durability_callback(Some(durability_callback))?;
    let app = app.clone();
    let state = Arc::clone(state);
    // callback is the sole projection publisher for Ask registration, completion, and cancellation.
    // callback 是 Ask 注册、完成与取消的唯一投影发布者。
    let callback: PendingQuestionChangeCallback = Arc::new(move || {
        // projectionResult preserves explicit publication failures for runtime transaction handling.
        // projectionResult 为运行时事务处理保留明确的发布失败。
        let projection_result = emit_pending_question_changed_delta(&app, &state);
        if let Err(error) = &projection_result {
            webui_trace(
                "pending_question_projection_failed",
                format!("error={error}"),
            );
        }
        projection_result
    });
    question_runtime.set_change_callback(Some(callback))
}

/// Returns the retained workbench with live ask requests, or a pending-registry projection error.
/// 返回附加实时 ask 请求的保留工作台，失败时返回 pending 注册表投影错误。
pub(crate) fn workbench_with_live_pending_question(
    guard: &ServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let mut workbench = guard.workbench.clone();
    workbench.pending_questions =
        pending_question_snapshots_from_agent_runtime(guard.agent_runtime.as_ref())?;
    workbench.pending_question =
        workbench
            .selected_conversation_id
            .as_ref()
            .and_then(|conversation_id| {
                workbench
                    .pending_questions
                    .iter()
                    .find(|question| question.session_id == *conversation_id)
                    .cloned()
            });
    Ok(workbench)
}
