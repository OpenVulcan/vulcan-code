//! Durable context compaction owned by an exact running child execution.
//! 由精确运行中子执行持有的可靠上下文压缩。

use super::*;
use futures_util::StreamExt;
use vulcan_agent_runtime::ExecutionStreamEvent;
use vulcan_agent_runtime::agent_runtime::events::RuntimeEventKind;
use vulcan_agent_runtime::agent_runtime::prompt_compaction::{
    ChildTaskCompactionPhase, ChildTaskCompactionRequest, PromptCompactionFoundationJobRequest,
    PromptCompactionLaunchContext, PromptCompactionStreamEvent,
    stream_prompt_compaction_foundation_job,
};
use vulcan_agent_runtime::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use vulcan_agent_runtime::agent_runtime::task_runtime::{TaskExecutionRequest, TaskRuntime};
use vulcan_session_core::v1::DomainError;
use vulcan_session_core::v1::context_maintenance::ContextMaintenanceExecution;
use vulcan_session_core::v1::document::SessionDocument;

/// Immutable physical ownership captured before planning and checked again during node publication.
/// 规划前捕获并在节点发布时重新检查的不可变物理归属。
#[derive(Clone, Copy, PartialEq, Eq)]
struct ChildCompactionOwner {
    /// Exact physical history generation owning the child profile.
    /// 拥有子档案的精确物理历史代次。
    generation: u64,
    /// Exact open business or maintenance turn explicitly selected at admission.
    /// 准入时显式选择的精确开放业务或维护回合。
    turn_id: TurnId,
}

/// Validates `invocation` against the runtime and durable document, returning its exact open owner.
/// 根据运行时与可靠文档校验 `invocation`，返回其精确开放归属。
/// Called while canonical lifecycle ownership is held; no document changes are made.
/// 在持有规范生命周期归属时调用；不修改文档。
fn validate_owner(
    document: &SessionDocument,
    runtime: &TaskRuntime,
    invocation: &TaskExecutionRequest,
    maintenance: Option<&ContextMaintenanceExecution>,
) -> Result<ChildCompactionOwner, DomainError> {
    // Business uses its active lease; maintenance must instead prove the independent record below.
    // 业务使用其活动租约；维护则必须证明下方独立记录。
    let facts = if maintenance.is_none() {
        Some(
            runtime
                .active_task_v1_execution_facts(invocation.task_id, &invocation.execution_id)
                .map_err(|_| DomainError::CollaborationStateViolation {
                    field: "child_compaction.runtime_owner",
                })?,
        )
    } else {
        None
    };
    // A persisted profile supplies business ownership independently of transient UI state.
    // 持久化档案独立于瞬态界面状态提供业务归属。
    let profile = document.metadata.child_execution_profile.as_ref().ok_or(
        DomainError::CollaborationStateViolation {
            field: "child_compaction.profile",
        },
    )?;
    // Compaction requires an explicitly admitted open turn; completed business and question-only turns cannot substitute.
    // 压缩要求显式准入的开放回合；已完成业务或仅答疑回合不能替代。
    let turn = document
        .active_turn()
        .filter(|turn| !turn.closed)
        .ok_or(DomainError::NoActiveTurn)?;
    if facts.as_ref().is_some_and(|facts| {
        facts.root_session_id != invocation.root_session_id
            || facts.parent_session_id != invocation.current_session_id
            || facts.prompt != invocation.prompt
    }) || profile.root_session_id != invocation.root_session_id.to_string()
        || profile.parent_session_id != invocation.current_session_id.to_string()
        || profile.task_id.as_str() != invocation.task_id.to_string()
        || profile.business_execution_id != invocation.execution_id
        || maintenance.is_none() && profile.business_turn_id != turn.turn_id
        || profile.segment_generation != document.segment_generation()
    {
        return Err(DomainError::CollaborationStateViolation {
            field: "child_compaction.business_owner",
        });
    }
    if let Some(maintenance) = maintenance
        && (document.metadata.context_maintenance.as_ref() != Some(maintenance)
            || maintenance.terminal.is_some()
            || maintenance.turn_id != turn.turn_id
            || turn.origin != TurnOrigin::ContextMaintenance
            || maintenance.business_turn_id != profile.business_turn_id
            || maintenance
                .business_execution_id
                .as_ref()
                .map(|id| id.as_str())
                != Some(profile.business_execution_id.as_str())
            || maintenance.segment_generation != document.segment_generation())
    {
        return Err(DomainError::CollaborationStateViolation {
            field: "child_compaction.maintenance_owner",
        });
    }
    if document.active_api().is_some() {
        return Err(DomainError::CollaborationStateViolation {
            field: "child_compaction.api_in_flight",
        });
    }
    Ok(ChildCompactionOwner {
        generation: document.segment_generation(),
        turn_id: turn.turn_id,
    })
}

/// Plans, executes and commits a running child's summary against its unique durable owner.
/// 针对唯一可靠所有者规划、执行并提交运行中子任务的摘要。
/// `app` publishes durable changes, `state` owns registries, and `request` binds the original execution.
/// `app` 发布可靠变更，`state` 持有注册表，`request` 绑定原执行。
/// Returns the newly projected canonical snapshot; any ownership, persistence or summary failure forbids continuation.
/// 返回新投影的规范快照；归属、持久化或摘要失败均禁止继续。
pub(super) async fn compact(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    request: ChildTaskCompactionRequest,
) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
    if request.phase == ChildTaskCompactionPhase::CompletedBusinessMaintenance {
        return child_maintenance::run(app, state, request).await;
    }
    compact_admitted(app, state, request, None).await
}

/// Executes compaction using either its original business lease or an explicitly admitted maintenance record.
/// 使用原业务租约或显式准入维护记录执行压缩。
/// `maintenance` is supplied only by the service-owned worker; returns the newly committed canonical snapshot.
/// `maintenance` 仅由服务自有工作任务提供；返回新提交的规范快照。
pub(super) async fn compact_admitted(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    request: ChildTaskCompactionRequest,
    maintenance: Option<ContextMaintenanceExecution>,
) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
    if (request.phase == ChildTaskCompactionPhase::CompletedBusinessMaintenance)
        != maintenance.is_some()
    {
        return Err(String::from(
            "Child compaction phase does not match its admitted execution.",
        ));
    }
    if request.preparation.session_id != request.invocation.task_id {
        return Err(String::from(
            "Child compaction preparation belongs to a different session.",
        ));
    }
    // Release the service mutex before lifecycle gates, disk reads or model execution.
    // 在生命周期门、磁盘读取或模型执行之前释放服务互斥锁。
    let (registry, runtime, resources) = {
        let guard = state.lock().map_err(|error| error.to_string())?;
        let runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| String::from("Child compaction requires an initialized runtime."))?;
        (
            Arc::clone(&guard.session_runtime_registry),
            Arc::clone(&runtime.task_runtime),
            Arc::clone(&guard.project_resource_catalog_registry),
        )
    };
    // Keep this canonical handle for the entire attempt; no session-name or root-slot lookup substitutes for it.
    // 整次尝试保留此规范句柄；不以会话名称或根槽位查找替代。
    let handle = registry
        .get(request.invocation.task_id)
        .map_err(|error| error.to_string())?;
    // Move original identity into the canonical read callback, without duplicating the task's resident history.
    // 将原身份交给规范读取回调，不复制任务驻留历史。
    let invocation = Arc::new(request.invocation);
    // Capture the exact admission while the same gate used by stop/replacement excludes concurrent transition.
    // 通过停止及替换使用的同一门排除并发转换，捕获精确准入。
    let owner = {
        let handle = Arc::clone(&handle);
        let runtime = Arc::clone(&runtime);
        let invocation = Arc::clone(&invocation);
        // Only the exact transferred service owner may dispatch a maintenance request.
        // 只有精确转交的服务所有者可以分发维护请求。
        let state = state.clone();
        // Keep maintenance identity immutable across asynchronous canonical admission.
        // 跨异步规范准入保持维护身份不可变。
        let maintenance = maintenance.clone();
        lifecycle_write::with_canonical_handles(&registry, vec![Arc::clone(&handle)], move || {
            if let Some(execution) = &maintenance {
                child_maintenance::require_owner(&state, handle.session_id(), execution)?;
            }
            handle
                .read(|document| {
                    validate_owner(document, &runtime, &invocation, maintenance.as_ref())
                })
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())
        })
        .await?
    };
    // Child policies are frozen by the actual preparation; root memory ownership cannot be borrowed here.
    // 子策略由实际准备冻结；此处不能借用根记忆归属。
    if request.preparation.runtime_policy
        != vulcan_session_core::ExecutionRuntimePolicy::task_session()
    {
        return Err(String::from(
            "Child compaction requires the admitted isolated task runtime policy.",
        ));
    }
    // Planning reads the real durable V1 transcript, including current dynamic-message expiration.
    // 规划读取真实可靠 V1 转录，包括当前动态消息失效状态。
    let planned = crate::context_compaction::webui_context_compaction_plan_from_v1(
        state,
        invocation.task_id,
        request.model_target.context_compaction_policy,
        "Child compaction has no durable source window.",
    )?;
    // This sink alone owns summary API preparation, output normalization and physical completion.
    // 此接收器独占摘要 API 准备、输出规范化与物理完成。
    let mut sink = crate::agent_stream::ProviderV1StreamSink::new(Arc::clone(&handle), resources);
    sink.bind_compaction_source(&planned.full_context_state)?;
    sink.require_child_compaction_source_owner(owner.turn_id, &invocation.execution_id)?;
    // Preserve the exact task prompt controls while replacing only its source media authority with the durable plan.
    // 保留精确任务提示控制，仅将来源媒体权威替换为可靠规划。
    let mut launch_context =
        PromptCompactionLaunchContext::from_execution_request(&request.preparation);
    launch_context.media_projection = Some(planned.media_projection);
    // The declared model window uses the same reserve rule as root summary execution.
    // 声明的模型窗口使用与根摘要执行相同的余量规则。
    let input_budget = crate::context_compaction::context_compaction_input_budget_from_limit(
        request.model_target.context_limit,
    );
    // Original route diagnostics are frozen before moving the model target into its execution request.
    // 将模型目标交给执行请求之前冻结原路由诊断。
    let model_label = Some(request.model_target.label());
    // Retain the exact durable tail boundary for the final compaction node.
    // 为最终压缩节点保留精确可靠尾段边界。
    let retained_from_seq = planned.retained_from_seq;
    // Heap-pin the large runtime stream, as the root compactor does, so nested maintenance futures cannot multiply its stack footprint.
    // 与根压缩器相同，将大型运行时流固定在堆上，避免嵌套维护 Future 倍增其线程栈占用。
    let mut stream = Box::pin(stream_prompt_compaction_foundation_job(
        PromptCompactionFoundationJobRequest {
            ownership: vulcan_agent_runtime::agent_runtime::prompt_compaction::PromptCompactionOwnership::SessionV1,
            session_id: invocation.task_id,
            session_title: invocation.display_title.clone(),
            workspace_path: request.preparation.workspace_path.clone(),
            plan: planned.plan,
            full_context_state: planned.full_context_state,
            input_budget,
            observed_input_tokens: None,
            estimated_input_tokens: None,
            provider_configuration: request.provider_configuration,
            model_target: Some(request.model_target),
            execution_agent_id: request.preparation.execution_agent_id.clone(),
            launch_context,
            model_label,
            reasoning_effort: request.preparation.reasoning_effort,
            foundation: request.foundation,
            luaskills_runtime: request.luaskills_runtime,
        },
    ));
    // Cancellation accounting is independent of optional root display.
    // 取消记账独立于可选根展示。
    let mut telemetry =
        crate::ProviderTelemetryHookGuard::new_with_persistence(Arc::clone(state), None);
    // No root presentation identity is created for an embedded child.
    // 不为嵌入子代理创建根展示身份。
    let mut output_id = None;
    // A retry replaces only its own temporary output buffer.
    // 重试仅替换自己的临时输出缓冲。
    let mut raw = String::new();
    // Missing terminal events are explicit failures, never a successful resident-only rewrite.
    // 缺失终态事件明确失败，绝不视为成功的仅驻留改写。
    let mut finished = None;
    while let Some(event) = stream.next().await {
        match event {
            PromptCompactionStreamEvent::PreparedSource { source, mode } => {
                sink.select_compaction_source(&source, mode)?;
                raw.clear();
            }
            PromptCompactionStreamEvent::Stream(event) => {
                if let Some(deferred) =
                    crate::context_compaction::apply_context_compaction_stream_event(
                        app,
                        state,
                        None,
                        &mut output_id,
                        &mut raw,
                        &mut sink,
                        &mut telemetry,
                        event,
                    )
                    .await?
                {
                    let ExecutionStreamEvent::RuntimeEvent { session_id, event } = deferred else {
                        return Err(String::from(
                            "Child compaction deferred an unsupported host event.",
                        ));
                    };
                    if session_id != invocation.task_id {
                        return Err(String::from(
                            "Child compaction host event belongs to another session.",
                        ));
                    }
                    if let Some(execution) = &maintenance {
                        child_maintenance::require_owner(state, invocation.task_id, execution)?;
                        // Bootstrap and discovery emit metadata-free ToolStarted/ToolFinished display events; actual typed tools were already handled by the sink.
                        // 启动和发现会发出无元数据的 ToolStarted/ToolFinished 展示事件；实际强类型工具已由接收器处理。
                        // Reject untyped execution metadata and child lifecycle events without borrowing the released business listener.
                        // 拒绝无类型的执行元数据及子生命周期事件，不借用已释放的业务监听器。
                        if matches!(
                            event.kind,
                            RuntimeEventKind::DelegateStarted
                                | RuntimeEventKind::DelegateFinished
                                | RuntimeEventKind::ToolBatchPrepared
                        ) || (matches!(
                            event.kind,
                            RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
                        ) && event.metadata.is_some())
                        {
                            let error = "Child maintenance received an unowned lifecycle event.";
                            vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::reject_runtime_event_projection_barrier_from_event_with_reason(&event, error)?;
                            return Err(error.to_owned());
                        }
                        crate::append_runtime_event_projection_and_release_barrier(
                            app,
                            state,
                            invocation.task_id,
                            &event,
                        )?;
                    } else {
                        runtime.publish_child_compaction_host_event(
                            invocation.task_id,
                            &invocation.execution_id,
                            event,
                        )?;
                    }
                }
            }
            PromptCompactionStreamEvent::Finished(result) => {
                finished = Some(result?);
                break;
            }
            PromptCompactionStreamEvent::Progress(_) => {}
        }
    }
    // A valid model result and exact durable receipts must both exist before any context pointer advances.
    // 推进任何上下文指针之前，有效模型结果和精确可靠回执必须同时存在。
    let execution = finished
        .ok_or_else(|| String::from("Child compaction stream ended without a terminal result."))?;
    if execution.fallback_used {
        return Err(String::from(
            "Child compaction cannot commit a fallback summary.",
        ));
    }
    // Consume each physical receipt once, leaving business completion with the original child execution.
    // 每个物理回执只消费一次，业务完成仍由原子执行持有。
    let durability = sink.take_finished_durability();
    // Both child modes use the same exact local/native receipt contract as the root session.
    // 两种子执行模式与主会话使用同一精确本地/原生回执契约。
    let commit_input = crate::context_compaction::verified_compaction_commit_input(
        &execution,
        &durability,
        retained_from_seq,
    )?;
    // Publish under canonical lifecycle ownership with a fresh runtime/profile check inside the document mutation.
    // 在规范生命周期归属下发布，并在文档修改内部重新检查运行时及档案。
    let committed = {
        let handle = Arc::clone(&handle);
        let runtime = Arc::clone(&runtime);
        let invocation = Arc::clone(&invocation);
        let timestamp = now_ms()?;
        // Commit requires the same service owner and durable maintenance start used for planning.
        // 提交要求与规划时相同的服务所有者及可靠维护起始。
        let state = state.clone();
        // The immutable optional record distinguishes maintenance from the active business lease.
        // 不可变可选记录区分维护与活动业务租约。
        let maintenance = maintenance.clone();
        lifecycle_write::with_canonical_handles(&registry, vec![Arc::clone(&handle)], move || {
            if let Some(execution) = &maintenance {
                child_maintenance::require_owner(&state, handle.session_id(), execution)?;
            }
            handle
                .mutate_durable_if_changed(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::ContextChanged,
                    move |document| {
                        if validate_owner(document, &runtime, &invocation, maintenance.as_ref())?
                            != owner
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "child_compaction.commit_owner",
                            });
                        }
                        document
                            .record_compaction(commit_input, timestamp)
                            .map(session_change)
                    },
                )
                .map_err(|error| error.to_string())
        })
        .await?
    };
    // Read the ordinary canonical projection only after the committed compaction pointer is visible.
    // 仅在已提交压缩指针可见之后读取普通规范投影。
    let snapshot = handle
        .capture_controller_projection_snapshot()
        .map_err(|error| error.to_string())?;
    if snapshot.current_compaction_id != Some(committed.value)
        || snapshot.segment_generation != owner.generation
        || snapshot.open_turn_id != Some(owner.turn_id)
        || snapshot.child_business_execution_id.as_deref() != Some(invocation.execution_id.as_str())
    {
        return Err(String::from(
            "Child compaction ownership changed before canonical snapshot handoff.",
        ));
    }
    if let Some(execution) = &maintenance {
        child_maintenance::require_owner(state, invocation.task_id, execution)?;
    } else {
        runtime.active_task_v1_execution_facts(invocation.task_id, &invocation.execution_id)?;
    }
    // The same production conversion used by physical preparation preserves tools, media and host envelopes.
    // 与物理准备相同的生产转换保留工具、媒体及宿主信封。
    let conversation = crate::agent_stream::provider_projection_controller_state(&snapshot)?;
    Ok(Some(build_core_conversation_snapshot(&conversation)?))
}
