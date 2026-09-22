//! Stops service-owned child QA without replacing the saved business result.
//! 停止服务持有的子答疑，不替换已保存业务结果。

use super::*;

/// Stops one already-authorized child auxiliary execution, including its pre-API preparation.
/// 停止一个已授权子辅助执行，包括 API 启动前的准备阶段。
/// `session_id` identifies a retained child owner; `app` and `state` supply host lifecycle services.
/// `session_id` 标识保留的子所有者；`app` 与 `state` 提供宿主生命周期服务。
/// Returns false when no auxiliary owner exists; errors preserve terminal repair and execution exclusion.
/// 没有辅助所有者时返回 false；错误保留终态修复及执行互斥。
pub(crate) async fn stop_owned_child_question_execution(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<bool, String> {
    // Ordinary child business stops must not require an auxiliary document or recovery owner.
    // 普通子业务停止不应要求辅助文档或恢复所有者。
    if !state
        .lock()
        .map_err(|error| error.to_string())?
        .child_question_execution_owners
        .contains_key(&session_id)
    {
        return Ok(false);
    }
    // Recover through the registered child; indexed-session loading cannot locate embedded tasks.
    // 通过已登记子会话恢复；索引会话加载无法定位嵌入任务。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    crate::terminal_settlement_service::rebuild_pending_terminal_settlement_owner(
        state,
        session_id,
        handle.as_ref(),
    );
    if crate::terminal_settlement_service::settle_pending_terminal_settlement(
        state, &handle, session_id,
    )
    .await?
        == crate::terminal_settlement_service::TerminalSettlementRetryOutcome::NoPendingSettlement
    {
        crate::terminal_settlement_service::retry_pre_intent_terminal_settlement(state, session_id)
            .await?;
    }
    // Only the service's retained task guard identifies an embedded auxiliary execution.
    // 只有服务保留的任务守卫可以标识嵌入辅助执行。
    let execution_id = {
        let service = state.lock().map_err(|error| error.to_string())?;
        match service.child_question_execution_owners.get(&session_id) {
            Some(owner) => owner.execution_id().clone(),
            None => return Ok(false),
        }
    };
    // Stop shares submission exclusion with the dedicated child launcher.
    // 停止与专用子启动器共享提交互斥。
    let _stop_admission = begin_session_run_stop_exclusion(state, session_id)?;
    handle
        .read(|document| question_stop::question_stop_scope(document, &execution_id))
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    {
        let service = state.lock().map_err(|error| error.to_string())?;
        validate_child_question_stop_kernel(&service, session_id, &execution_id)?;
    }
    // Persist rejection of later messages before aborting any process-owned work.
    // 在中止任何进程工作之前，持久化拒绝后续消息的状态。
    let stopped_at_ms = current_v1_goal_unix_millis()?;
    handle
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "child-question-stop:{}:{}",
                execution_id.as_str(),
                Uuid::new_v4()
            ))
            .map_err(|error| error.to_string())?,
            state: CollaborationRunControl::Stopped,
            changed_at_ms: stopped_at_ms,
        })
        .map_err(|error| error.to_string())?;
    // Install terminal ownership before removing the stream, so a registration failure is non-destructive.
    // 先安装终态所有权再移除流，保证登记失败不产生破坏。
    let terminal_owner = SessionTerminalPersistenceOwner::RunStop(Uuid::new_v4());
    let (active_task, preparation_task) = {
        let mut service = state.lock().map_err(|error| error.to_string())?;
        validate_child_question_stop_kernel(&service, session_id, &execution_id)?;
        register_session_terminal_persistence_owner(
            &mut service,
            session_id,
            terminal_owner.clone(),
        )?;
        let active_task = service.active_stream_tasks.remove(&session_id);
        let preparation_task = service.question_preparation_tasks.remove(&session_id);
        stop_webui_session_kernel_generation(&mut service, session_id);
        (active_task, preparation_task)
    };
    // This marker and the independent child guard remain retained on every cleanup error.
    // 任意清理错误都会保留此标记及独立子守卫。
    let terminal_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        terminal_owner,
    )?;
    // Abort both owners before joining either, including the brief preparation-to-stream overlap.
    // 在等待任何一方之前中止两个所有者，包括准备到流交接的短暂重叠。
    if let Some(task) = &preparation_task {
        task.task.abort();
    }
    if let Some(task) = &active_task {
        task.abort();
    }
    if let Some(task) = preparation_task {
        if let Err(error) = task.task.abort_and_wait().await {
            eprintln!("[webui-service] child question preparation aborted task join: {error}");
        }
    }
    if let Some(active_task) = active_task {
        // An aborted task may report cancellation; its completed join is the quiescence evidence.
        // 被中止任务可能报告取消；完成等待本身就是静止证据。
        if let Err(error) = active_task.abort_and_wait().await {
            eprintln!("[webui-service] child question aborted task join: {error}");
        }
    }
    // Drain any admitted result publication before freezing the exact API stop boundary.
    // 冻结精确 API 停止边界前，排空已经准入的结果发布。
    let preparation_settlement = handle.acquire_request_preparation_settlement().await;
    let scope = question_stop::persist_question_stop_intent(
        state,
        &handle,
        session_id,
        &execution_id,
        terminal_guard.owner_generation(),
    )
    .await?;
    let browser_manager = state
        .lock()
        .map_err(|error| error.to_string())?
        .browser_manager
        .clone();
    browser_manager
        .retire_ai_connection_after_turn(session_id)
        .await
        .map_err(|error| error.to_string())?;
    let interruption_at_ms = current_v1_goal_unix_millis()?;
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                if question_stop::question_stop_scope(document, &execution_id)? != scope {
                    return Err(
                        vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                            field: "child_question_stop.scope_changed",
                        },
                    );
                }
                match document.interrupt_active_api(interruption_at_ms) {
                    Ok(_) => Ok(SessionMutationChange::Changed(())),
                    Err(vulcan_session_core::v1::DomainError::NoActiveApi) => {
                        Ok(SessionMutationChange::Unchanged(()))
                    }
                    Err(error) => Err(error),
                }
            },
        )
        .map_err(|error| error.to_string())?;
    // The scoped terminal writer reacquires this gate, so release it before settlement.
    // 范围终态写入器会重新获取此关卡，因此在结算前释放。
    drop(preparation_settlement);
    if !crate::agent_stream::settle_stream_terminal_authority_with_scope(
        state,
        session_id,
        false,
        question_stop::QUESTION_STOP_REASON,
        Some(question_stop::QUESTION_STOP_DETAIL.to_owned()),
        terminal_guard,
        scope,
    )
    .await
    {
        return Err(String::from(
            "Child question stop requires terminal repair; execution admission remains retained.",
        ));
    }
    {
        let mut service = state.lock().map_err(|error| error.to_string())?;
        if let Some(runtime) = service
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
        {
            runtime.active_precheck_turn_id = None;
        }
    }
    crate::agent_stream::apply_session_phase(app, state, session_id, SessionRunPhase::Stopped)?;
    Ok(true)
}

/// Validates retained child ownership and the optional kernel before any stop mutation.
/// 在任何停止修改之前，校验保留的子所有权及可选内核。
/// An absent kernel is permitted during pre-registration cleanup, but an ownerless stream is rejected.
/// 登记前清理允许没有内核，但拒绝没有所有者的流。
fn validate_child_question_stop_kernel(
    service: &ServiceBridgeState,
    session_id: SessionId,
    execution_id: &vulcan_session_core::v1::collaboration::CollaborationExecutionId,
) -> Result<(), String> {
    crate::agent_collaboration_execution::preparation_tasks::validate_question_preparation_task(
        service, session_id,
    )?;
    if !service
        .child_question_execution_owners
        .get(&session_id)
        .is_some_and(|owner| owner.execution_id() == execution_id)
    {
        return Err(String::from("Child question stop owner has changed."));
    }
    if let Some(turn) = service
        .session_runtime_kernels
        .get(&session_id)
        .and_then(|kernel| kernel.active_turn())
    {
        if !matches!(&turn.kind, ActiveTurnKind::QuestionExecution { execution_id: active } if active == execution_id)
        {
            return Err(String::from(
                "Child question stop cannot interrupt a different kernel execution.",
            ));
        }
    } else if service.active_stream_tasks.contains_key(&session_id) {
        return Err(String::from(
            "Child question stop found a stream without its kernel owner.",
        ));
    }
    Ok(())
}
