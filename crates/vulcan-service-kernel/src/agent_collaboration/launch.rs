//! Dispatches accepted questions into an explicitly owned auxiliary execution.
//! 将已接纳问题分发到具有显式所有者的辅助执行。

use super::*;
use crate::agent_collaboration_execution::QuestionExecutionLaunchDisposition;
use vulcan_agent_runtime::workbench_prompt_turn::{
    SharedPromptTurnBeginInput, begin_shared_prompt_turn,
};
use vulcan_session_core::{
    ActiveTurnKind,
    v1::collaboration::{CollaborationQuestionExecutionRecord, FailDeliveryInput},
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
) -> Result<(), String> {
    let question = read_question_source(&invocation.source_handle, &question_id)?.0;
    let target_id =
        Uuid::parse_str(&question.target.session_id).map_err(|error| error.to_string())?;
    let owner = dispatch_owner::DispatchOwner::acquire(&invocation.state, target_id)?;
    // The accepted record survives the original API; wakeup never borrows its expired tool authority.
    // 已接纳记录独立于原 API 存续；唤醒不能借用过期工具权限。
    let app = app.clone();
    let invocation = invocation.clone();
    crate::spawn_service_task(async move {
        let _owner = owner;
        if let Err(error) = wake_accepted_question(&app, &invocation, &question_id).await {
            tracing::warn!(
                "[webui-service] question wakeup failed for {}: {error}",
                question_id.as_str()
            );
            if let Err(diagnostic_error) = crate::push_webui_service_diagnostic(
                &invocation.state,
                format!("代理问题 {} 启动失败：{error}", question_id.as_str()),
            ) {
                tracing::warn!(
                    "[webui-service] question wakeup diagnostic failed: {diagnostic_error}"
                );
            }
        }
    });
    Ok(())
}

/// Settles a revoked, accepted question under its source and target lifecycle gates.
/// 在来源和目标生命周期锁内结算已撤权且已接纳的问题。
///
/// `source` and `target` are canonical handles already locked by the caller; `question_id` selects
/// their exact durable exchange and `stamp` timestamps any new failure receipt.
/// `source` 和 `target` 是调用方已锁定的规范句柄；`question_id` 选择双方精确可靠交换，`stamp`
/// 标记新失败回执的时间。
/// Returns true after a target failure and source progress are both durable, including recovery
/// of an earlier target failure; an accepted authorized question returns false.
/// 目标失败及来源进度都可靠落盘后返回真，包括修复此前已有的目标失败；仍获授权的已接纳问题返回假。
fn settle_revoked_accepted_question_locked(
    source: &Arc<SessionHandle>,
    target: &Arc<SessionHandle>,
    question_id: &CollaborationQuestionId,
    stamp: i64,
) -> Result<bool, String> {
    // The source record identifies the only question and outbound delivery allowed to settle.
    // 来源记录确定唯一允许结算的问题与出站投递。
    let (question, delivery, _) = read_question_source(source, question_id)?;
    if question.state != CollaborationQuestionState::Pending {
        return Ok(false);
    }
    if question.source.session_id != source.session_id().to_string()
        || question.target.session_id != target.session_id().to_string()
    {
        return Err(String::from(
            "question wakeup participants do not match their canonical owners",
        ));
    }
    // The target receipt must match the source before grant revocation can change its state.
    // 授权撤销改变目标状态前，目标回执必须与来源精确匹配。
    let inbound = target
        .read(|document| {
            validate_question_target_delivery(document, &question, &delivery)
                .map_err(|error| error.to_string())?;
            document
                .inbound_delivery_snapshot(&delivery.delivery_id)
                .cloned()
                .ok_or_else(|| String::from("accepted question has no target inbound receipt"))
        })
        .map_err(|error| format!("failed to read question wakeup target receipt: {error}"))??;
    // An existing failure is copied even if authorization was subsequently granted again.
    // 已有失败即使随后重新授权也要复制，不能让终态倒退。
    let (target_revision, target_event_seq, failure) = match inbound.state {
        CollaborationDeliveryState::Failed => (
            inbound.target_revision,
            target_inbound_event_seq(target, &inbound)?,
            inbound
                .failure
                .ok_or_else(|| String::from("failed target question has no failure reason"))?,
        ),
        CollaborationDeliveryState::Accepted => {
            if question.source.project_id == question.target.project_id
                || source_handle_grant_allows(
                    source,
                    source.session_id(),
                    target.session_id(),
                    CollaborationGrantAction::Ask,
                )?
            {
                return Ok(false);
            }
            // Reuse the provider-boundary operation identity so concurrent wake and projection agree.
            // 复用供应商边界的操作身份，使并发唤醒与投影收敛到同一结果。
            let operation_id = CollaborationOperationId::try_new(format!(
                "{}:fail",
                delivery.delivery_id.as_str()
            ))
            .map_err(|error| error.to_string())?;
            let delivery_id = delivery.delivery_id.clone();
            let question_id = question.question_id.clone();
            let receipt = target
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        document
                            .fail_delivery(FailDeliveryInput {
                                operation_id,
                                delivery_id,
                                question_id,
                                reason: CollaborationDeliveryFailureReason::AuthorizationRevoked,
                                failed_at_ms: stamp,
                            })
                            .map(session_change)
                    },
                )
                .map_err(|error| format!("failed to settle revoked target question: {error}"))?;
            (
                receipt.value.revision,
                receipt.value.event_seq,
                CollaborationDeliveryFailureReason::AuthorizationRevoked,
            )
        }
        CollaborationDeliveryState::Projected => return Ok(false),
        CollaborationDeliveryState::Recorded => {
            return Err(String::from(
                "target question has no accepted delivery state",
            ));
        }
    };
    // The source progress follows the target's exact published revision and event sequence.
    // 来源进度跟随目标精确已发布的修订与事件序列。
    let progress = RecordDeliveryProgressInput {
        operation_id: CollaborationOperationId::try_new(format!(
            "{}:source-failed",
            delivery.delivery_id.as_str()
        ))
        .map_err(|error| error.to_string())?,
        delivery_id: delivery.delivery_id,
        question_id: question.question_id,
        state: CollaborationDeliveryState::Failed,
        failure: Some(failure),
        target_revision: Some(target_revision),
        target_event_seq: Some(target_event_seq),
        updated_at_ms: stamp,
    };
    source
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                document
                    .record_delivery_progress(progress)
                    .map(session_change)
            },
        )
        .map_err(|error| format!("failed to acknowledge revoked question: {error}"))?;
    Ok(true)
}

/// Repairs an accepted question whose Ask grant was revoked before any provider boundary.
/// 修复在任何供应商边界之前提问授权已撤销的已接纳问题。
///
/// `invocation` owns the durable source and `target` is its exact resolved recipient.
/// `invocation` 持有可靠来源，`target` 是精确解析的接收者。
/// Returns whether the question reached a durable delivery failure.
/// 返回问题是否已达到可靠的投递失败状态。
async fn settle_revoked_accepted_question(
    invocation: &ValidatedInvocation,
    target: &ResolvedSession,
    question_id: &CollaborationQuestionId,
) -> Result<bool, String> {
    // Both owners are retained until the target and source publications complete.
    // 双方所有者在目标与来源发布完成前持续保留。
    let source = Arc::clone(&invocation.source_handle);
    let receiver = Arc::clone(&target.handle);
    let question_id = question_id.clone();
    lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![Arc::clone(&source), Arc::clone(&receiver)],
        move || {
            settle_revoked_accepted_question_locked(&source, &receiver, &question_id, now_ms()?)
        },
    )
    .await
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
    let _dispatch_owner = dispatch_owner::DispatchOwner::acquire(
        &invocation.state,
        Uuid::parse_str(&question.target.session_id).map_err(|error| error.to_string())?,
    )?;
    let (session_id, target) = resolve_question_target(invocation, &question).await?;
    // An idle target has no provider boundary at which revocation could otherwise be settled.
    // 空闲目标没有其他供应商边界来结算撤权。
    if settle_revoked_accepted_question(invocation, &target, question_id).await? {
        return Ok(QuestionExecutionLaunchDisposition::AlreadyHandled);
    }
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
    // Shared QA setup creates no business Hook or human input; run it before durable execution admission.
    // 共享答疑设置不创建业务 Hook 或人类输入；在可靠执行准入之前完成。
    let start = {
        let _state_guard = invocation.state.lock().map_err(|error| error.to_string())?;
        let mut hook_runtime = agent_runtime.lock_conversation_hooks()?;
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
    // The final authorization decision and actual question projection share both lifecycle gates.
    // 最终授权判断与实际问题投影共享双方生命周期锁。
    let source_for_write = Arc::clone(&invocation.source_handle);
    let target_for_write = Arc::clone(&target.handle);
    let question_id_for_write = question_id.clone();
    let execution = lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![Arc::clone(&source_for_write), Arc::clone(&target_for_write)],
        move || {
            if settle_revoked_accepted_question_locked(
                &source_for_write,
                &target_for_write,
                &question_id_for_write,
                started_at_ms,
            )? {
                return Ok(None);
            }
            let (current, _, _) = read_question_source(&source_for_write, &question_id_for_write)?;
            if current.state != CollaborationQuestionState::Pending {
                return Ok(None);
            }
            let target_id = target_for_write.session_id();
            if current.target.session_id != target_id.to_string() {
                return Err(String::from(
                    "question target changed before execution admission",
                ));
            }
            // A prior projection already owns the question; never start another execution.
            // 先前投影已经拥有该问题，绝不能再启动一次执行。
            let inbound_state = target_for_write
                .read(|document| {
                    document
                        .inbound_delivery_snapshot(&current.delivery_id)
                        .map(|inbound| inbound.state)
                })
                .map_err(|error| error.to_string())?
                .ok_or_else(|| String::from("question target lost its accepted delivery"))?;
            if inbound_state != CollaborationDeliveryState::Accepted {
                return Ok(None);
            }
            let receipt = target_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    move |document| {
                        participants::require_document_message_admission(
                            document,
                            &current.source,
                            &current.target,
                        )?;
                        document
                            .prepare_question_execution_before_api(current, started_at_ms)
                            .map(session_change)
                    },
                )
                .map_err(|error| error.to_string())?;
            Ok(Some(receipt.value.execution))
        },
    )
    .await?;
    let Some(execution) = execution else {
        return Ok(QuestionExecutionLaunchDisposition::AlreadyHandled);
    };
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
