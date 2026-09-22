//! Embedded auxiliary admission and dispatch without business task resumption.
//! 不恢复业务任务的嵌入辅助准入与分发。

use super::*;
use crate::agent_collaboration_execution::QuestionExecutionLaunchDisposition;
use vulcan_session_core::{
    ActiveTurnKind, v1::collaboration::CollaborationQuestionExecutionRecord,
};
use vulcan_workbench_model::{SessionRunPhase, SessionRuntimeStateEntry};

/// Starts an accepted embedded question under both submission and task execution reservations.
/// 在提交预留与任务执行预留共同保护下启动已接纳嵌入问题。
/// The independent wakeup task retains `reservation_id`; `target` is canonically resolved and authorized.
/// 独立唤醒任务持有 `reservation_id`；`target` 已经过规范定位与授权。
/// Returns after scheduling dedicated preparation, or a durable scoped failure.
/// 调度专用准备后返回，或返回经过可靠范围结算的失败。
pub(super) async fn wake_embedded_question(
    app: &crate::ServiceEventEmitter,
    invocation: &ValidatedInvocation,
    question_id: &CollaborationQuestionId,
    session_id: Uuid,
    target: &ResolvedSession,
    reservation_id: Uuid,
) -> Result<QuestionExecutionLaunchDisposition, String> {
    // Capture only current host services; no ordinary-session summary is required for an embedded target.
    // 仅捕获当前宿主服务；嵌入目标不要求普通会话摘要。
    let agent_runtime = {
        // Fail before durable admission if the service cannot maintain runtime projection state.
        // 服务无法维护运行态投影时，在可靠准入之前失败。
        let state = invocation.state.lock().map_err(|error| error.to_string())?;
        if state.workspace_snapshot.is_none() {
            return Err(String::from(
                "Child question dispatch requires initialized workspace runtime state.",
            ));
        }
        state.agent_runtime.clone().ok_or_else(|| {
            String::from("Child question dispatch requires an initialized agent runtime.")
        })?
    };
    // This excludes business preparation even while the child's active API is absent.
    // 即使子活动 API 为空，该预留仍排除业务准备。
    let Some(child_admission) = agent_runtime
        .task_runtime
        .try_reserve_child_question_execution(session_id)?
    else {
        return Ok(QuestionExecutionLaunchDisposition::DeferredWhileBusy);
    };
    // Durable source state is read again after target resolution and both admission gates.
    // 目标定位及两个准入关卡之后再次读取可靠来源状态。
    let (question, _, _) = read_question_source(&invocation.source_handle, question_id)?;
    if question.state != CollaborationQuestionState::Pending {
        return Ok(QuestionExecutionLaunchDisposition::AlreadyHandled);
    }
    if question.target.session_id != session_id.to_string() || question.target.task_id.is_none() {
        return Err(String::from(
            "Embedded question dispatch participant no longer matches its target.",
        ));
    }
    if question.source.project_id != question.target.project_id
        && !source_handle_grant_allows(
            &invocation.source_handle,
            invocation.source_session_id,
            session_id,
            CollaborationGrantAction::Ask,
        )?
    {
        return Err(String::from(
            "Child question wakeup authorization was revoked.",
        ));
    }
    // Reject missing or stale child configuration before opening an auxiliary turn.
    // 在打开辅助回合之前拒绝缺少或过期的子配置。
    target
        .handle
        .read(|document| {
            let profile = document
                .metadata
                .child_execution_profile
                .as_ref()
                .ok_or_else(|| {
                    String::from("Child question execution has no captured business profile.")
                })?;
            profile
                .validate_owner(&session_id.to_string(), document.segment_generation())
                .map_err(|error| error.to_string())?;
            if profile.root_session_id != question.target.root_session_id
                || Some(&profile.task_id) != question.target.task_id.as_ref()
            {
                return Err(String::from(
                    "Child question profile does not match the admitted participant.",
                ));
            }
            Ok(())
        })
        .map_err(|error| error.to_string())??;
    // The target mutation atomically rechecks control, projects the message and creates its exact QA owner.
    // 目标修改原子重查控制、投影消息并创建精确答疑所有者。
    let started_at_ms = now_ms()?;
    let execution = super::child_question_admission::prepare_child_question_execution(
        &invocation.state,
        &invocation.registry,
        &target.handle,
        child_admission,
        question,
        started_at_ms,
    )
    .await?;
    // The kernel correlation identifies preparation only, never a new human prompt or business execution.
    // 内核关联只标识准备，不标识新的人类提示词或业务执行。
    let preparation_turn_id = format!("question-execution:{}", execution.execution_id.as_str());
    // Keep registration separate from projection, because failures require different terminal owners.
    // 将登记与投影分开，因为失败时需要不同终态所有者。
    let registration = (|| {
        let mut state = invocation.state.lock().map_err(|error| error.to_string())?;
        crate::start_reserved_webui_session_kernel_turn(
            &mut state,
            session_id,
            &preparation_turn_id,
            ActiveTurnKind::QuestionExecution {
                execution_id: execution.execution_id.clone(),
            },
            reservation_id,
        )?;
        crate::webui_session_kernel_generation(&state, session_id).ok_or_else(|| {
            String::from("Child question kernel generation is missing after registration.")
        })
    })();
    // Once registered, only this exact kernel may settle preparation failure.
    // 登记后，只有该精确内核可以结算准备失败。
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
    if let Err(error) = publish_child_question_preparing(
        app,
        &invocation.state,
        session_id,
        &preparation_turn_id,
        generation,
        &execution,
    ) {
        launch::settle_registered_dispatch_failure(
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
    // Detached preparation allows Stop to acquire submission admission during browser/service I/O.
    // 分离准备允许停止操作在浏览器或服务 I/O 期间取得提交准入。
    let task_app = app.clone();
    let task_invocation = invocation.clone();
    let task_target = target.clone();
    let task_execution = execution.clone();
    let task_turn_id = preparation_turn_id.clone();
    // Register before browser/service work so Stop can cancel and join the actual preparation.
    // 在浏览器或服务工作之前登记，使 Stop 可取消并等待实际准备退出。
    let registration =
        crate::agent_collaboration_execution::preparation_tasks::spawn_question_preparation_task(
            &invocation.state,
            session_id,
            generation,
            &preparation_turn_id,
            &execution.execution_id,
            async move {
                // These inputs belong to the registered preparation, while the caller retains failure-settlement facts.
                // 这些输入属于已登记准备，调用方同时保留失败结算事实。
                let app = task_app;
                let invocation = task_invocation;
                let target = task_target;
                let execution = task_execution;
                let preparation_turn_id = task_turn_id;
                let preparation = async {
                    let request = child_question_request::prepare_child_question_request(
                        &app,
                        &invocation,
                        session_id,
                        &target,
                    )
                    .await?;
                    crate::agent_stream::spawn_question_execution_stream_task(
                        app.clone(),
                        invocation.state.clone(),
                        invocation.controller_runtime.clone(),
                        agent_runtime,
                        request,
                        generation,
                        preparation_turn_id.clone(),
                        &execution,
                    )
                    .await
                }
                .await;
                if let Err(error) = preparation {
                    if let Err(cleanup_error) = launch::settle_registered_dispatch_failure(
                        &app,
                        &invocation,
                        session_id,
                        generation,
                        &preparation_turn_id,
                        &execution,
                        &error,
                    )
                    .await
                    {
                        eprintln!(
                            "[webui-service] child question cleanup failed: {cleanup_error}; original error: {error}"
                        );
                    }
                    if let Err(diagnostic_error) = crate::push_webui_service_diagnostic(
                        &invocation.state,
                        format!("子代理答疑请求准备失败：{error}"),
                    ) {
                        eprintln!(
                            "[webui-service] child question diagnostic failed: {diagnostic_error}"
                        );
                    }
                }
            },
        );
    if let Err(error) = registration {
        launch::settle_registered_dispatch_failure(
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
    Ok(QuestionExecutionLaunchDisposition::Started)
}

/// Publishes auxiliary preparation into runtime-only state under the exact kernel generation.
/// 在精确内核代际下，将辅助准备发布到仅运行态记录。
/// Does not create an ordinary session index or modify a task's saved business outcome.
/// 不创建普通会话索引，也不修改任务已保存的业务结果。
fn publish_child_question_preparing(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: Uuid,
    preparation_turn_id: &str,
    generation: u64,
    execution: &CollaborationQuestionExecutionRecord,
) -> Result<(), String> {
    if crate::capture_webui_question_preparation_generation(
        state,
        session_id,
        preparation_turn_id,
        &execution.execution_id,
    )? != Some(generation)
    {
        return Err(String::from(
            "Child question preparation owner changed before phase publication.",
        ));
    }
    // Mutation and phase generation share the same service lock; the caller still holds dispatch admission.
    // 修改与阶段代际共用服务锁；调用方仍持有分发准入。
    let event = {
        let mut service = state.lock().map_err(|error| error.to_string())?;
        let workspace = service.workspace_snapshot.as_mut().ok_or_else(|| {
            String::from("Child question workspace runtime state is unavailable.")
        })?;
        let runtime = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(SessionRuntimeStateEntry::default);
        runtime.active_precheck_turn_id = Some(preparation_turn_id.to_owned());
        runtime.updated_at = crate::current_unix_millis_text();
        crate::agent_stream::apply_session_phase_to_state(
            &mut service,
            session_id,
            SessionRunPhase::Preparing,
        )?
    };
    crate::emit_service_event_with_trace(
        app,
        "child_question_preparing",
        crate::ServiceEvent::WorkbenchDelta(Box::new(event)),
    );
    Ok(())
}

#[cfg(test)]
mod tests;
