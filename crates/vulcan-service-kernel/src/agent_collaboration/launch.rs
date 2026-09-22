//! Dispatches accepted questions into an explicitly owned auxiliary execution.
//! 将已接纳问题分发到具有显式所有者的辅助执行。

use super::*;
use crate::agent_collaboration_execution::QuestionExecutionLaunchDisposition;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_session_core::{
    ActiveTurnKind, v1::collaboration::CollaborationQuestionExecutionRecord,
};
use vulcan_workbench_model::{PendingSessionPromptSource, SessionRunPhase};

/// Schedules wakeup after durable acceptance without holding the sender's tool result open.
/// 在可靠接纳后调度唤醒，不占用发送方工具结果等待。
/// `app` publishes diagnostics; `invocation` locates the source; `question_id` selects durable work.
/// `app` 发布诊断；`invocation` 定位来源；`question_id` 选择可靠工作。
pub(super) fn schedule_question_wakeup(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    question_id: CollaborationQuestionId,
) {
    // The accepted record survives the original API; wakeup never borrows its expired tool authority.
    // 已接纳记录独立于原 API 存续；唤醒不能借用过期工具权限。
    let app = app.clone();
    let invocation = invocation.clone();
    crate::spawn_service_task(async move {
        if let Err(error) = wake_accepted_question(&app, &invocation, &question_id).await {
            eprintln!(
                "[webui-service] question wakeup failed for {}: {error}",
                question_id.as_str()
            );
            if let Err(diagnostic_error) = crate::push_webui_service_diagnostic(
                &invocation.state,
                format!("代理问题 {} 启动失败：{error}", question_id.as_str()),
            ) {
                eprintln!("[webui-service] question wakeup diagnostic failed: {diagnostic_error}");
            }
        }
    });
}

/// Launches one accepted question, or reports why the existing execution owns its delivery.
/// 启动一个已接纳问题，或报告现有执行为何继续拥有该投递。
/// Uses fresh source state and target admission; returns after preparation has been scheduled.
/// 使用最新来源状态与目标准入；准备任务调度后返回。
pub(super) async fn wake_accepted_question(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    question_id: &CollaborationQuestionId,
) -> Result<QuestionExecutionLaunchDisposition, String> {
    // A source can finish its business turn after sending; the durable question remains authoritative.
    // 来源可以在发送后结束业务回合；可靠问题仍是权威。
    let (question, _, _) = read_question_source(&invocation.source_handle, question_id)?;
    if question.state != CollaborationQuestionState::Pending {
        return Ok(QuestionExecutionLaunchDisposition::AlreadyHandled);
    }
    let (session_id, target) = resolve_question_target(invocation, &question).await?;
    let (control, active_api, undispatched_business, completed) = target
        .handle
        .read(|document| {
            (
                document.collaboration_run_control(),
                document.active_api().is_some(),
                // A pending first business request owns its position ahead of an auxiliary wakeup.
                // 等待首次请求的业务在辅助唤醒之前拥有执行位置。
                document.active_turn().is_some_and(|turn| {
                    !turn.closed
                        && turn.origin != TurnOrigin::QuestionExecution
                        && !document.events().iter().any(|event| {
                            event.turn_id == Some(turn.turn_id)
                                && matches!(&event.event, EventBody::ApiStarted(started)
                                    if !matches!(started.purpose,
                                        ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
                        })
                }),
                document
                    .collaboration_state()
                    .question_executions
                    .values()
                    .any(|execution| {
                        execution.question_id == *question_id
                            && execution.state == CollaborationQuestionExecutionState::Completed
                    }),
            )
        })
        .map_err(|error| error.to_string())?;
    if completed {
        return Ok(QuestionExecutionLaunchDisposition::AlreadyHandled);
    }
    crate::agent_collaboration_execution::admit_collaboration_execution(
        control,
        question.execution_purpose,
    )?;
    // Running work will consume the question at its normal request boundary; MainOnly retains its explicit QA exception.
    // 运行中业务会在正常请求边界消费问题；MainOnly 保留其显式答疑例外。
    if active_api || (control == CollaborationRunControl::Running && undispatched_business) {
        return Ok(QuestionExecutionLaunchDisposition::DeferredToParentTurn);
    }
    // This reservation also excludes user Stop and competing prompt dispatch during asynchronous loading.
    // 此预留也在异步加载期间排除用户停止与竞争提示词分发。
    let Some(admission) =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &invocation.state,
            session_id,
        )?
    else {
        return Ok(QuestionExecutionLaunchDisposition::DeferredWhileBusy);
    };
    let authorization = admission.existing_submission_authorization(session_id)?;
    if question.target.task_id.is_some() {
        return super::child_question_launch::wake_embedded_question(
            app,
            invocation,
            question_id,
            session_id,
            &target,
            authorization.reservation_id,
        )
        .await;
    }
    // Preserve retained UI records; cold sessions are loaded through the canonical indexed-session path.
    // 保留已驻留界面记录；冷会话通过规范索引会话入口加载。
    let retained = invocation
        .state
        .lock()
        .map_err(|error| error.to_string())?
        .workspace_snapshot
        .clone()
        .filter(|workspace| {
            workspace
                .sessions
                .iter()
                .any(|session| session.id == session_id)
        });
    let mut workspace = match retained {
        Some(mut workspace) => {
            crate::sync_selected_session_v1_projection_from_store(
                &invocation.state,
                invocation.controller_runtime.clone(),
                session_id,
                &mut workspace,
            )
            .await?;
            workspace
        }
        None => {
            crate::session_hydration::load_workspace_for_submission(
                &invocation.state,
                invocation.controller_runtime.clone(),
                Some(session_id),
            )
            .await?
        }
    };
    let session_title = workspace
        .sessions
        .iter()
        .find(|session| session.id == session_id)
        .map(|session| session.title.clone())
        .ok_or_else(|| {
            String::from("Question target is missing from the workspace session index.")
        })?;
    let workspace_path = crate::workspace_path_for_session(&workspace, session_id);
    let agent_runtime = invocation
        .state
        .lock()
        .map_err(|error| error.to_string())?
        .agent_runtime
        .clone()
        .ok_or_else(|| String::from("Question dispatch requires an initialized agent runtime."))?;
    let readiness = crate::webui_prompt_lifecycle::webui_vmm_runtime_readiness(&invocation.state)?;
    // Loading can yield to cancellation or grant revocation; read the original authority again.
    // 加载可能让出执行给撤销问题或撤销授权；再次读取原始权威。
    let (question, _, _) = read_question_source(&invocation.source_handle, question_id)?;
    if question.state != CollaborationQuestionState::Pending {
        return Ok(QuestionExecutionLaunchDisposition::AlreadyHandled);
    }
    if question.source.project_id != question.target.project_id
        && !source_handle_grant_allows(
            &invocation.source_handle,
            invocation.source_session_id,
            session_id,
            CollaborationGrantAction::Ask,
        )?
    {
        return Err(String::from("Question wakeup authorization was revoked."));
    }
    // Shared QA setup creates no business Hook or human input; run it before durable execution admission.
    // 共享答疑设置不创建业务 Hook 或人类输入；在可靠执行准入之前完成。
    let start = {
        let _state_guard = invocation.state.lock().map_err(|error| error.to_string())?;
        let mut hook_runtime = agent_runtime
            .conversation_hook_runtime
            .lock()
            .map_err(|error| error.to_string())?;
        begin_shared_prompt_turn(
            &mut workspace,
            &mut hook_runtime,
            SharedPromptTurnBeginInput {
                session_id,
                session_title: &session_title,
                workspace_path: &workspace_path,
                prompt_entry_id: question.delivery_id.as_str().to_owned(),
                continuation_action_id: None,
                prompt: "",
                hidden_model_guidance: None,
                provider_visible_agent_messages: Vec::new(),
                attachments: Vec::new(),
                provider_attachments: Some(Vec::new()),
                existing_canonical_user_message_id: None,
                append_canonical_user_message: false,
                append_visible_user_prompt_record: false,
                source: PendingSessionPromptSource::QuestionExecution,
                vmm_runtime_ready: readiness.service_ready,
                vmm_precheck_ready: readiness.precheck_ready,
                execution_mode: String::from("build"),
                reasoning_effort: None,
            },
        )?
    };
    let started_at_ms = now_ms()?;
    let execution = mutate_target(invocation, &target, session_id, move |document| {
        participants::require_document_message_admission(
            document,
            &question.source,
            &question.target,
        )?;
        document.prepare_question_execution_before_api(question, started_at_ms)
    })
    .await?
    .value
    .execution;
    // Once durable admission succeeds, every later error must settle this execution rather than the business turn.
    // 可靠准入成功后，每个后续错误都必须结算此执行，而不是业务回合。
    let registration = (|| {
        let mut guard = invocation.state.lock().map_err(|error| error.to_string())?;
        crate::start_reserved_webui_session_kernel_turn(
            &mut guard,
            session_id,
            &start.preparation_turn_id,
            ActiveTurnKind::QuestionExecution {
                execution_id: execution.execution_id.clone(),
            },
            authorization.reservation_id,
        )?;
        crate::webui_session_kernel_generation(&guard, session_id).ok_or_else(|| {
            String::from("Question kernel generation is missing after registration.")
        })
    })();
    let generation = match registration {
        Ok(generation) => generation,
        Err(error) => {
            crate::agent_stream::fail_undispatched_question_execution(
                &invocation.state,
                session_id,
                &execution,
                &error,
            )
            .await?;
            return Err(error);
        }
    };
    let preparation_turn_id = start.preparation_turn_id.clone();
    let dispatch = (|| {
        if !crate::commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            app,
            &invocation.state,
            workspace,
            session_id,
            SessionRunPhase::Preparing,
            generation,
            &preparation_turn_id,
        )? {
            return Err(String::from(
                "Question preparation owner changed before phase publication.",
            ));
        }
        crate::webui_prompt_lifecycle::spawn_webui_question_prepare_task(
            app.clone(),
            invocation.state.clone(),
            invocation.controller_runtime.clone(),
            agent_runtime,
            start,
            generation,
            execution.clone(),
        )
    })();
    if let Err(error) = dispatch {
        settle_registered_dispatch_failure(
            app,
            invocation,
            session_id,
            generation,
            &preparation_turn_id,
            &execution,
            &error,
        )
        .await?;
        return Err(error);
    }
    drop(admission);
    Ok(QuestionExecutionLaunchDisposition::Started)
}

/// Settles failure after registration using the same kernel generation and durable execution.
/// 使用相同内核代际与可靠执行结算登记之后的失败。
/// Returns an explicit repair error if the original owner cannot be safely finalized.
/// 原所有者无法安全终结时返回明确修复错误。
pub(super) async fn settle_registered_dispatch_failure(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    session_id: Uuid,
    generation: u64,
    preparation_turn_id: &str,
    execution: &CollaborationQuestionExecutionRecord,
    error: &str,
) -> Result<(), String> {
    if !crate::agent_stream::fail_question_execution_preparation(
        app,
        &invocation.state,
        session_id,
        generation,
        preparation_turn_id,
        execution,
        error,
    )
    .await?
    {
        return Err(String::from(
            "Question dispatch failure lost its exact kernel owner; recovery is required.",
        ));
    }
    Ok(())
}
