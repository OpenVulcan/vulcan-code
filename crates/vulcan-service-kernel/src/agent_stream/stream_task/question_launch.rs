//! Exact question execution ownership at preparation and stream launch boundaries.
//! 准备与流启动边界的精确问题执行归属。

use super::*;
use vulcan_agent_runtime::ExecutionPromptPurpose;
use vulcan_session_core::v1::collaboration::CollaborationQuestionExecutionRecord;
use vulcan_session_core::v1::document::SessionDocument;

/// Settles an admitted QA execution whose process-local kernel could not be registered.
/// 结算已准入但未能登记进程内内核的答疑执行。
/// The caller retains submission admission; `execution` identifies the exact durable owner.
/// 调用方保留提交准入；`execution` 标识精确可靠所有者。
/// Returns only after scoped terminal persistence, retaining repair ownership on failure.
/// 带范围终态可靠落盘后才返回；失败时保留修复所有权。
pub(crate) async fn fail_undispatched_question_execution(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    execution: &CollaborationQuestionExecutionRecord,
    error: &str,
) -> Result<(), String> {
    // Prove the original no-API owner before installing a terminal persistence marker.
    // 安装终态持久化标记之前证明原始无 API 所有者。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    let scope = handle
        .read(|document| {
            question_launch_restart_state(document, session_id, execution)
                .map(|restart| restart.terminal_scope())
        })
        .map_err(|error| error.to_string())??;
    let owner = SessionTerminalPersistenceOwner::PromptPreparation(Uuid::new_v4());
    {
        let mut guard = state.lock().map_err(|error| error.to_string())?;
        if guard.active_stream_tasks.contains_key(&session_id)
            || crate::webui_session_kernel_has_active_turn(&guard, session_id)
        {
            return Err(String::from(
                "Undispatched question cleanup cannot own an active kernel or stream.",
            ));
        }
        crate::state::register_session_terminal_persistence_owner(
            &mut guard,
            session_id,
            owner.clone(),
        )?;
    }
    let terminal_owner =
        SessionTerminalPersistenceOwnerGuard::adopt_registered(state.clone(), session_id, owner)?;
    if !settle_stream_terminal_authority_with_scope(
        state,
        session_id,
        false,
        "question_dispatch_failed",
        Some(error.to_owned()),
        terminal_owner,
        scope,
    )
    .await
    {
        return Err(String::from(
            "Undispatched question settlement requires repair; terminal owner retained.",
        ));
    }
    Ok(())
}

/// Validates a captured execution against the current target document before any provider request.
/// 在任何供应商请求之前，对照当前目标文档校验捕获的执行。
/// `document` owns current facts; `session_id` and `execution` identify the intended owner. Returns its exact stream scope.
/// `document` 持有当前事实；`session_id` 与 `execution` 标识预期所有者。返回精确流范围。
fn question_launch_restart_state(
    document: &SessionDocument,
    session_id: SessionId,
    execution: &CollaborationQuestionExecutionRecord,
) -> Result<StreamRestartState, String> {
    // restart_state validates exclusive business-parent versus auxiliary-turn ownership.
    // restart_state 校验业务父回合与辅助回合的互斥归属。
    let restart_state = StreamRestartState::for_question_execution(execution)?;
    if execution.participant.session_id != session_id.to_string()
        || execution.participant.segment_generation != document.segment_generation()
        || document
            .collaboration_state()
            .question_executions
            .get(&execution.execution_id)
            != Some(execution)
    {
        return Err(String::from(
            "Question execution launch no longer owns the target document.",
        ));
    }
    // turn_id is already proven exclusive by the typed stream constructor above.
    // turn_id 已由前面的类型化流构造器证明具有互斥归属。
    let turn_id = execution
        .execution_turn_id
        .or(execution.parent_turn_id)
        .ok_or_else(|| String::from("Question execution launch has no bound turn."))?;
    if !document
        .active_turn()
        .is_some_and(|turn| !turn.closed && turn.turn_id == turn_id)
        || document.active_api().is_some()
    {
        return Err(String::from(
            "Question execution launch requires its open turn without an active API.",
        ));
    }
    crate::agent_collaboration_execution::admit_collaboration_execution(
        document.collaboration_run_control(),
        execution.purpose,
    )?;
    Ok(restart_state)
}

/// Starts a prepared QA request with its captured durable execution and kernel generation.
/// 使用捕获的可靠执行与内核代际启动已准备的答疑请求。
/// Runtime arguments provide dispatch services; `request`, `execution`, and kernel fields must identify the same owner.
/// 运行时参数提供分发服务；`request`、`execution` 及内核字段必须指向同一所有者。
/// Returns after scheduling or an explicit ownership error; never converts a QA request into business execution.
/// 调度后返回，或返回明确的归属错误；绝不把答疑请求转为业务执行。
// Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary.
// 事件边界显式保留流所有权、代际守卫和独立借用的遥测状态。
#[expect(
    clippy::too_many_arguments,
    reason = "Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary."
)]
pub(crate) async fn spawn_question_execution_stream_task(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    controller_runtime: HostRuntimeHandle,
    agent_runtime: AgentRuntimeBridge,
    mut request: ExecutionPreparationRequest,
    expected_kernel_generation: u64,
    expected_kernel_turn_id: String,
    execution: &CollaborationQuestionExecutionRecord,
) -> Result<(), String> {
    if request.prompt_purpose != ExecutionPromptPurpose::QuestionExecution
        || request.session_goal_context.is_some()
        || request.native_goal_runtime.is_some()
    {
        return Err(String::from(
            "Question stream requires dedicated purpose without business Goal execution.",
        ));
    }
    // handle is the canonical document, not a workspace or mailbox mirror.
    // handle 是规范文档，而非工作区或邮箱镜像。
    let handle = crate::session_goal_lifecycle::registered_session_v1_handle_from_state(
        &state,
        request.session_id,
    )?;
    // Reject invalid ownership before accessing any child configuration or authorization service.
    // 访问子配置或授权服务之前拒绝无效归属。
    handle
        .read(|document| question_launch_restart_state(document, request.session_id, execution))
        .map_err(|error| format!("Failed to read question launch ownership: {error}"))??;
    if let Some(task_id) = &execution.participant.task_id {
        crate::agent_collaboration::child_execution_ownership::require_child_question_execution_owner(
            &state,
            request.session_id,
            &execution.execution_id,
        )?;
        // Only the canonical target document may supply executable child settings.
        // 只有规范目标文档可以提供可执行子设置。
        let (profile, generation) = handle
            .read(|document| {
                (
                    document.metadata.child_execution_profile.clone(),
                    document.segment_generation(),
                )
            })
            .map_err(|error| format!("Failed to read child question profile: {error}"))?;
        let profile = profile.ok_or_else(|| {
            String::from("Child question execution has no captured business profile.")
        })?;
        if &profile.task_id != task_id
            || profile.root_session_id != execution.participant.root_session_id
        {
            return Err(String::from(
                "Child question profile does not match the admitted participant.",
            ));
        }
        vulcan_agent_runtime::agent_runtime::task_runtime::restore_child_question_request(
            &mut request,
            &profile,
            generation,
        )?;
        // Resolve against the canonical locator, because embedded tasks have no ordinary workspace index.
        // 按规范定位器解析，因为嵌入任务没有普通工作区索引。
        let locator = handle
            .storage_locator()
            .map_err(|error| error.to_string())?;
        if request.project_id.as_deref() != Some(locator.project_id()) {
            return Err(String::from(
                "Child question request belongs to a different project.",
            ));
        }
        request.knowledge_access = Some(
            super::question_knowledge::resolve_child_question_knowledge(
                &state,
                locator.project_id(),
                profile.request_overrides.knowledge_access.as_ref(),
            )
            .await?,
        );
    }
    // Authorization may yield to stop, completion or history replacement; rebuild scope from current facts.
    // 授权期间可能发生停止、完成或历史替换；从当前事实重新构建范围。
    let restart_state = handle
        .read(|document| question_launch_restart_state(document, request.session_id, execution))
        .map_err(|error| format!("Failed to revalidate question launch ownership: {error}"))??;
    if crate::capture_webui_question_preparation_generation(
        &state,
        request.session_id,
        &expected_kernel_turn_id,
        &execution.execution_id,
    )? != Some(expected_kernel_generation)
    {
        return Err(String::from(
            "Question stream preparation owner has changed.",
        ));
    }
    spawn_agent_stream_task_with_restart_state(
        app,
        state,
        controller_runtime,
        agent_runtime,
        request,
        expected_kernel_generation,
        expected_kernel_turn_id,
        restart_state,
    );
    Ok(())
}

/// Settles only the captured QA preparation after its exact kernel owner fails before stream registration.
/// 精确内核所有者在流登记前失败时，仅结算捕获的答疑准备。
/// `execution` supplies durable scope; kernel identity prevents stale cleanup from stopping a replacement.
/// `execution` 提供可靠范围；内核身份防止过期清理停止替代执行。
/// Returns false when ownership was replaced; persistence failure retains the terminal owner and returns an error.
/// 所有权已被替代时返回 false；持久化失败保留终态所有者并返回错误。
pub(crate) async fn fail_question_execution_preparation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    expected_kernel_generation: u64,
    expected_kernel_turn_id: &str,
    execution: &CollaborationQuestionExecutionRecord,
    error: &str,
) -> Result<bool, String> {
    // scope retains the original execution even when target state changes during asynchronous cleanup.
    // scope 即使在异步清理期间目标状态改变，也保留原始执行。
    let scope = StreamRestartState::for_question_execution(execution)?.terminal_scope();
    if execution.participant.session_id != session_id.to_string() {
        return Err(String::from(
            "Question preparation failure has a different target session.",
        ));
    }
    // handle checks durable ownership before the process-local kernel can be terminalized.
    // handle 在进程内内核可被终结之前校验可靠所有权。
    let handle =
        crate::session_goal_lifecycle::registered_session_v1_handle_from_state(state, session_id)?;
    handle
        .read(|document| question_launch_restart_state(document, session_id, execution))
        .map_err(|error| format!("Failed to read question preparation failure owner: {error}"))??;
    if crate::capture_webui_question_preparation_generation(
        state,
        session_id,
        expected_kernel_turn_id,
        &execution.execution_id,
    )? != Some(expected_kernel_generation)
    {
        return Ok(false);
    }
    if handle_pre_registration_execution_stream_task_launch_failure(
        app,
        state,
        session_id,
        Some(expected_kernel_generation),
    )? != StreamTaskLaunchFailureOutcome::FailedOwnerFinalized
    {
        return Ok(false);
    }
    // terminal_owner closes new admission until the scoped intent and domain settlement are durable.
    // terminal_owner 在带范围意图及领域结算可靠落盘之前关闭新准入。
    let terminal_owner = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        SessionTerminalPersistenceOwner::KernelGeneration(expected_kernel_generation),
    )?;
    {
        // guard clears only the exact QA preparation marker, preserving the parent business Hook.
        // guard 仅清除精确答疑准备标记，保留父业务 Hook。
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock question cleanup state: {error}"))?;
        if let Some(runtime) = guard
            .workspace_snapshot
            .as_mut()
            .and_then(|workspace| workspace.session_runtime_state.get_mut(&session_id))
            && runtime.active_precheck_turn_id.as_deref() == Some(expected_kernel_turn_id)
        {
            runtime.active_precheck_turn_id = None;
        }
    }
    if !settle_stream_terminal_authority_with_scope(
        state,
        session_id,
        false,
        "question_preparation_failed",
        Some(error.to_owned()),
        terminal_owner,
        scope,
    )
    .await
    {
        return Err(String::from(
            "Question preparation settlement requires repair; terminal owner retained.",
        ));
    }
    Ok(true)
}

#[cfg(test)]
mod tests;
