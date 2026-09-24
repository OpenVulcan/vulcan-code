//! Restarts captured native business work without creating another delegation or root input.
//! 恢复已捕获原生业务，不创建另一次委派或根输入。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskRecord, CollaborationChildTaskState,
};
use vulcan_session_core::v1::document::SessionDocument;

#[cfg(test)]
pub(in crate::agent_collaboration) mod tests;

/// Resumes active original child work after terminal-return reconciliation.
/// 终态回流核对后恢复活动中的原子业务。
/// Discovery records are hints; parent reservation and full child ownership are checked again at dispatch.
/// 发现记录仅为线索；分发时再次检查父预留与完整子归属。
/// Returns after listener and runtime handoff, or an explicit retryable preparation error.
/// 监听及运行时交接后返回，或返回明确可重试准备错误。
pub(super) async fn recover(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    parent: Arc<SessionHandle>,
    child: Arc<SessionHandle>,
    records: Vec<CollaborationChildTaskRecord>,
) -> Result<(), String> {
    for record in records {
        // Keep recovery preparation visible to waiters until its guarded dispatch or failure.
        // 在受锁保护的分发或失败之前，让等待方可见恢复准备所有权。
        let _dispatch_owner =
            super::super::dispatch_owner::DispatchOwner::acquire(state, child.session_id())?;
        worker::require_active_owner(state, registry)?;
        // An in-process owner may already have an active API; it must not be cold-validated.
        // 进程内所有者可能已有活动 API；不能再对它执行冷恢复校验。
        let available_runtime = state
            .lock()
            .map_err(|error| error.to_string())?
            .agent_runtime
            .clone();
        if let Some(runtime) = &available_runtime
            && runtime.task_runtime.is_task_running(child.session_id())?
        {
            continue;
        }
        let Some(document) = source_for_dispatch(&parent, &child, &record)? else {
            continue;
        };
        let (agent_runtime, controller_runtime) = {
            let guard = state.lock().map_err(|error| error.to_string())?;
            (
                guard.agent_runtime.clone().ok_or_else(|| {
                    String::from("Child business recovery requires an agent runtime.")
                })?,
                guard.controller_runtime.clone().ok_or_else(|| {
                    String::from("Child business recovery requires a controller runtime.")
                })?,
            )
        };
        if agent_runtime
            .task_runtime
            .is_task_running(child.session_id())?
        {
            continue;
        }
        let profile = document
            .child_business_recovery_profile(&record.execution_id)
            .map_err(|error| error.to_string())?;
        let target = ResolvedSession {
            locator: child.storage_locator().map_err(|error| error.to_string())?,
            handle: child.clone(),
        };
        let invocation = recovery_invocation(
            state.clone(),
            controller_runtime.clone(),
            registry.clone(),
            child.session_id(),
            child.clone(),
        )?;
        let mut request = super::super::child_question_request::prepare_child_business_request(
            app,
            &invocation,
            child.session_id(),
            &target,
            &record.execution_id,
        )
        .await?;
        request.knowledge_access = Some(
            crate::agent_stream::resolve_child_question_knowledge(
                state,
                target.locator.project_id(),
                profile.request_overrides.knowledge_access.as_ref(),
            )
            .await?,
        );
        // Preparation resolves current secrets and authorized tools, but does not start a Provider request.
        // 准备解析当前秘密及授权工具，但不启动 Provider 请求。
        // Provider preparation carries large typed state; keep it out of the scanner future.
        // Provider 准备携带较大强类型状态；不将其内联到扫描 Future 中。
        let launch = Box::pin(vulcan_agent_runtime::agent_runtime::runtime_stream_launch::prepare_runtime_stream_launch_with_plugins_and_public_ai(
            &agent_runtime.provider_configuration,
            &agent_runtime.agent_profile_catalog,
            agent_runtime.luaskills_runtime.clone(),
            Some(agent_runtime.vulcan_plugin_runtime.clone()),
            agent_runtime.extension_generation.clone(),
            agent_runtime.task_runtime.clone(),
            agent_runtime.question_runtime.clone(),
            agent_runtime.command_session_runtime.clone(),
            Some(agent_runtime.public_ai_runtime.clone()),
            request,
        )).await.map_err(|report| report.error.unwrap_or_else(|| String::from("Recovered child preparation failed without a diagnostic.")))?;
        let dispatch_app = app.clone();
        let dispatch_state = state.clone();
        let dispatch_registry = registry.clone();
        let dispatch_parent = parent.clone();
        let dispatch_child = child.clone();
        let dispatch_runtime = agent_runtime.clone();
        let dispatch_controller = controller_runtime.clone();
        let captured_profile = profile.clone();
        // Lifecycle gates cover revalidation, event subscription and the synchronous provider handoff.
        // 生命周期锁覆盖重查、事件订阅及同步 Provider 交接。
        lifecycle_write::with_canonical_handles(
            registry,
            vec![parent.clone(), child.clone()],
            move || {
                worker::require_active_owner(&dispatch_state, &dispatch_registry)?;
                if dispatch_runtime
                    .task_runtime
                    .is_task_running(dispatch_child.session_id())?
                {
                    return Ok(());
                }
                let Some(current) = admit_prepared_source(
                    &dispatch_parent,
                    &dispatch_child,
                    &record,
                    &captured_profile,
                    now_ms()?,
                )?
                else {
                    return Ok(());
                };
                dispatch_runtime
                    .task_runtime
                    .start_recovered_child_business(
                        launch,
                        &current,
                        &record.execution_id,
                        |_| {
                            crate::agent_stream::start_admitted_detached_subagent_listener(
                                dispatch_app,
                                dispatch_state,
                                dispatch_controller,
                                dispatch_runtime.clone(),
                                dispatch_parent.session_id(),
                                dispatch_child,
                                record.execution_id.as_str().to_owned(),
                            )
                        },
                    )?;
                Ok(())
            },
        )
        .await?;
    }
    Ok(())
}

/// Proves the exact parent obligation and current open child before any recovery may dispatch.
/// 在任何恢复分发之前证明精确父义务与当前开放子回合。
/// Stopped, archived or terminal work returns None; contradictory ownership returns an error.
/// 停止、归档或终态任务返回 None；矛盾归属返回错误。
fn source_for_dispatch(
    parent: &SessionHandle,
    child: &SessionHandle,
    discovered: &CollaborationChildTaskRecord,
) -> Result<Option<SessionDocument>, String> {
    let owned = parent
        .read_full_durable(|document| {
            let current = document
                .collaboration_state()
                .child_tasks
                .get(&discovered.execution_id)
                .ok_or_else(|| String::from("Recovered child parent reservation is missing."))?;
            if current.state == CollaborationChildTaskState::Terminal
                || document.metadata.archived_at_ms.is_some()
                || matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
                )
            {
                return Ok(false);
            }
            if current != discovered
                || document.segment_generation() != current.parent_segment_generation
                || current.child.session_id != child.session_id().to_string()
                || current.task_id.as_str() != child.session_id().to_string()
                || document
                    .active_turn()
                    .is_none_or(|turn| turn.closed || turn.turn_id != current.parent_turn_id)
            {
                return Err(String::from(
                    "Recovered child parent reservation no longer owns this execution.",
                ));
            }
            Ok(true)
        })
        .map_err(|error| error.to_string())??;
    if !owned {
        return Ok(None);
    }
    child
        .read_full_durable(|document| {
            if document.metadata.archived_at_ms.is_some()
                || document.collaboration_run_control() != CollaborationRunControl::Running
                || document.active_turn().is_none_or(|turn| turn.closed)
            {
                return Ok(None);
            }
            // Reserved children may have crashed before capture, or still retain a previous business profile.
            // 已预留子任务可能在捕获前崩溃，或者仍保留上一次业务配置。
            // Only a profile for this exact execution proves that preparation crossed its durable boundary.
            // 只有此精确执行的配置才能证明准备已经越过可靠边界。
            if discovered.state == CollaborationChildTaskState::Reserved
                && document
                    .metadata
                    .child_execution_profile
                    .as_ref()
                    .is_none_or(|profile| {
                        profile.business_execution_id != discovered.execution_id.as_str()
                    })
            {
                return Ok(None);
            }
            let profile = document
                .child_business_recovery_profile(&discovered.execution_id)
                .map_err(|error| error.to_string())?;
            if profile.parent_session_id != parent.session_id().to_string()
                || profile.root_session_id != discovered.child.root_session_id
                || profile.segment_generation != discovered.child.segment_generation
                || profile.task_id != discovered.task_id
            {
                return Err(String::from(
                    "Recovered child profile has a different parent or generation.",
                ));
            }
            Ok(Some(document.clone()))
        })
        .map_err(|error| error.to_string())?
}

/// Revalidates a prepared cold source and durably activates its original reservation before handoff.
/// 重查已准备冷来源，并在交接之前可靠激活原预留。
/// The caller must hold parent and child lifecycle gates; `prepared_profile` is the completed preparation source.
/// 调用方必须持有父子生命周期锁；`prepared_profile` 是已完成准备所使用的来源。
/// Returns the unchanged child document, None for stopped work, or an explicit ownership/publication error.
/// 返回未改变的子文档；停止工作返回 None；归属或发布失败返回明确错误。
fn admit_prepared_source(
    parent: &SessionHandle,
    child: &SessionHandle,
    record: &CollaborationChildTaskRecord,
    prepared_profile: &vulcan_session_core::v1::child_execution::ChildExecutionProfile,
    active_at_ms: i64,
) -> Result<Option<SessionDocument>, String> {
    let Some(document) = source_for_dispatch(parent, child, record)? else {
        return Ok(None);
    };
    if document.metadata.child_execution_profile.as_ref() != Some(prepared_profile) {
        return Err(String::from(
            "Child business profile changed during recovery preparation.",
        ));
    }
    if record.state == CollaborationChildTaskState::Reserved {
        // Use the original lifecycle operation identity, not a second recovery-owned delegation.
        // 使用原生命周期操作身份，不增加第二次恢复自有委派。
        let operation_id = CollaborationOperationId::try_new(format!(
            "{}:activate",
            record.reservation_operation_id.as_str(),
        ))
        .map_err(|error| error.to_string())?;
        parent
            .mutate_durable_if_changed(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |parent_document| {
                    if parent_document
                        .collaboration_state()
                        .child_tasks
                        .get(&record.execution_id)
                        != Some(record)
                    {
                        return Err(
                            vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                                field: "child_business_recovery.reservation_changed",
                            },
                        );
                    }
                    require_open_parent_turn(parent_document, record.parent_turn_id)?;
                    Ok(session_change(parent_document.activate_child_task(
                        ActivateChildTaskInput {
                            operation_id,
                            execution_id: record.execution_id.clone(),
                            task_id: record.task_id.clone(),
                            active_at_ms,
                        },
                    )?))
                },
            )
            .map_err(|error| format!("Failed to activate prepared child recovery: {error}"))?;
    }
    Ok(Some(document))
}
