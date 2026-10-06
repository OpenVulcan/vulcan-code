//! Service-owned cancellation from result publication waiting to maintenance settlement.
//! 从等待结果发布到维护结算的服务自有取消。

use super::*;
use futures_util::FutureExt;
use vulcan_agent_runtime::agent_runtime::prompt_compaction::ChildTaskCompactionRequest;
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_session_core::v1::child_execution::ChildExecutionProfile;
use vulcan_session_core::v1::event::TurnClosureReason;

/// One registered worker whose cancellation never skips durable cleanup.
/// 一个已登记工作任务，其取消绝不跳过可靠清理。
pub(crate) struct ChildMaintenanceTask {
    /// Exact root and project storage owned from registration through final cleanup.
    /// 从登记到最终清理持有的精确根及项目存储。
    pub(super) storage_scope: MaintenanceStorageScope,
    /// Exact process identity used by completion cleanup.
    /// 完成清理使用的精确进程身份。
    task_id: Uuid,
    /// Original completed business whose maintenance is owned by this worker.
    /// 本工作任务维护的原已完成业务。
    business_execution_id: String,
    /// Signals cancellation of work, while leaving its settlement supervisor alive.
    /// 通知工作取消，同时保持其结算监督流程存活。
    cancel: tokio::sync::watch::Sender<bool>,
    /// Reports cleanup only after physical work and admitted writers have drained.
    /// 仅在物理工作及已准入写入器排空后报告清理结果。
    completion: tokio::sync::watch::Receiver<Option<Result<(), String>>>,
    /// Retains the real task until its own completion removes this exact entry.
    /// 保留真实任务，直到其自身完成移除本精确条目。
    _task: crate::state::ServiceTaskHandle<()>,
}

/// Waits for exact host publication, admits maintenance, and runs its canonical compactor.
/// 等待精确宿主发布、准入维护，并运行其规范压缩器。
/// `request` preserves original ownership; returns the committed snapshot after reliable cleanup.
/// `request` 保留原归属；可靠清理后返回已提交快照。
pub(crate) async fn run(
    app: &crate::ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    request: ChildTaskCompactionRequest,
) -> Result<Option<vulcan_core::types::ConversationSnapshot>, String> {
    // Snapshot dependencies before taking canonical lifecycle ownership.
    // 获取规范生命周期所有权前捕获依赖。
    let (registry, runtime) = {
        let service = state.lock().map_err(|error| error.to_string())?;
        (
            service.session_runtime_registry.clone(),
            service
                .agent_runtime
                .as_ref()
                .ok_or_else(|| String::from("Child maintenance requires an initialized runtime."))?
                .task_runtime
                .clone(),
        )
    };
    // The registered child is the only document allowed to authorize this request.
    // 已登记子任务是唯一允许授权此请求的文档。
    let handle = registry
        .get(request.invocation.task_id)
        .map_err(|error| error.to_string())?;
    // Capture immutable provenance before registering any asynchronous worker.
    // 登记任何异步工作任务前捕获不可变来源。
    let profile = handle
        .read(|document| {
            let profile = document
                .metadata
                .child_execution_profile
                .as_ref()
                .ok_or_else(|| String::from("Child maintenance has no execution profile."))?;
            if profile.business_execution_id != request.invocation.execution_id
                || profile.root_session_id != request.invocation.root_session_id.to_string()
                || profile.parent_session_id != request.invocation.current_session_id.to_string()
                || profile.task_id.as_str() != request.invocation.task_id.to_string()
            {
                return Err(String::from(
                    "Child maintenance invocation does not own its profile.",
                ));
            }
            Ok(profile.clone())
        })
        .map_err(|error| error.to_string())??;
    // Retain ordinary service dependencies inside the registered worker, not the runtime's detached waiter.
    // 在已登记工作任务内保留普通服务依赖，而非运行时分离的等待者。
    let work_state = state.clone();
    // Publication and settlement use the same canonical registry.
    // 发布及结算使用同一规范注册表。
    let work_registry = registry.clone();
    // This handle remains exact across result waiting and domain admission.
    // 此句柄在等待结果及领域准入期间保持精确。
    let work_handle = handle.clone();
    // Events keep the real service emitter without manufacturing a root run.
    // 事件保留真实服务发送器，不制造根运行。
    let app = app.clone();
    // The completion receiver can be dropped without cancelling the owned worker or its cleanup.
    // 完成接收端可以被丢弃，不会取消所属工作任务或清理。
    let result = spawn(state, &registry, &handle, profile, async move {
        // Acquire shared task exclusion before waiting, so an idle QA cannot replace the maintenance source.
        // 等待前获取共享任务互斥，防止空闲答疑替换维护来源。
        let Some(admission) = runtime.try_reserve_child_context_maintenance(
            request.invocation.task_id,
            &request.invocation.execution_id,
        )?
        else {
            return Ok(None);
        };
        // This cache is published only after the parent listener commits the actual business result.
        // 此缓存仅在父监听器提交实际业务结果之后发布。
        let publication = runtime
            .wait_for_durable_task_return(
                request.invocation.task_id,
                &request.invocation.execution_id,
            )
            .await?;
        if publication.kind != vulcan_session_boundary::SubagentMailboxEventKind::Completed {
            return Ok(None);
        }
        // Core re-proves the exact durable source after waiting, independently of resident status.
        // 等待后由 Core 重新证明精确可靠来源，独立于常驻状态。
        let source = work_handle
            .read(|document| {
                document.capture_child_context_maintenance_source(
                    &CollaborationExecutionId::try_new(&request.invocation.execution_id)?,
                )
            })
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string())?
            .ok_or_else(|| String::from("Child maintenance has no completed durable source."))?;
        // Admission transfers exclusion to service state even if storage subsequently fails.
        // 即使随后存储失败，准入也把互斥转交服务状态。
        let execution = prepare(
            &work_state,
            &work_registry,
            &work_handle,
            admission,
            source,
            now_ms()?,
        )
        .await?;
        child_compaction::compact_admitted(&app, &work_state, request, Some(execution)).await
    })
    .await?;
    result
        .await
        .map_err(|_| String::from("Child maintenance worker exited without a result."))?
}

/// Registers `work` before polling it, using the exact captured `profile` and canonical `handle`.
/// 在轮询 `work` 前，使用精确捕获的 `profile` 和规范 `handle` 登记。
/// Returns a result receiver; stopping work always runs cleanup on the service-owned worker.
/// 返回结果接收端；停止工作始终在服务自有工作任务上执行清理。
async fn spawn<F>(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    handle: &Arc<SessionHandle>,
    profile: ChildExecutionProfile,
    work: F,
) -> Result<
    tokio::sync::oneshot::Receiver<
        Result<Option<vulcan_core::types::ConversationSnapshot>, String>,
    >,
    String,
>
where
    F: std::future::Future<
            Output = Result<Option<vulcan_core::types::ConversationSnapshot>, String>,
        > + Send
        + 'static,
{
    // Only this registered task may remove its entry after cleanup.
    // 只有此已登记任务可以在清理后移除自己的条目。
    let task_id = Uuid::new_v4();
    // Clone canonical owners into cancellation-resistant registration.
    // 将规范所有者克隆到抵抗取消的登记流程。
    let state = state.clone();
    // The writer validates and retains this exact handle.
    // 写入器校验并保留本精确句柄。
    let handle = handle.clone();
    // The worker needs this same registry for final settlement.
    // 工作任务需要同一注册表进行最终结算。
    let worker_registry = registry.clone();
    // Root archival owns the embedded directory, so the root must be canonical under the same admission gates.
    // 根归档拥有嵌入目录，因此根必须在同一组准入门内保持规范归属。
    let root_id = profile
        .root_session_id
        .parse::<Uuid>()
        .map_err(|error| error.to_string())?;
    // An unloaded or replaced root cannot authorize a late background request.
    // 已卸载或替换的根不能授权迟到后台请求。
    let root = registry.get(root_id).map_err(|error| error.to_string())?;
    lifecycle_write::with_canonical_handles(registry, vec![root.clone(), handle.clone()], move || {
        handle.read(|document| {
            if document.metadata.child_execution_profile.as_ref() != Some(&profile)
                || document.segment_generation() != profile.segment_generation
                || document.metadata.archived_at_ms.is_some()
                || !matches!(document.collaboration_run_control(), CollaborationRunControl::Running | CollaborationRunControl::NaturallyIdle)
            { return Err(String::from("Child maintenance registration is stale or stopped.")); }
            Ok(())
        }).map_err(|error| error.to_string())??;
        // Root locator identity and actual root metadata are checked under the same canonical gates.
        // 在同一组规范门内检查根定位器身份及实际根元数据。
        let storage_scope = MaintenanceStorageScope::capture(&handle, &worker_registry)?;
        // Registration and stop admission share one service mutex, closing the first-poll race.
        // 登记和停止准入共享同一服务互斥锁，封闭首次轮询竞态。
        let mut service = state.lock().map_err(|error| error.to_string())?;
        let session_id = handle.session_id();
        if service.service_lifecycle_state.shutdown_drain_active
            || service.session_turn_admission_reservations.contains_key(&session_id)
            || service.session_turn_admission_reservations.get(&root_id).is_some_and(|reservation| reservation.kind == crate::state::SessionTurnAdmissionReservationKind::LifecycleTransition)
            || service.project_admission_reservations.get(&crate::state::ProjectAdmissionScope::Exact(storage_scope.project_id.clone())).is_some_and(|reservation| reservation.kind == crate::state::ProjectAdmissionReservationKind::LifecycleTransition)
            || service.child_maintenance_tasks.contains_key(&session_id)
            || service.child_maintenance_owners.contains_key(&session_id)
            || service.child_maintenance_stops.get(&session_id) == Some(&profile.business_execution_id)
        { return Err(String::from("Child maintenance registration is excluded by a current or stopped owner.")); }
        // No external effect occurs until the entry is visible to Stop.
        // 条目对停止操作可见之前不发生任何外部效果。
        let (start_sender, start_receiver) = tokio::sync::oneshot::channel();
        // Cancellation stops only work, never the settlement supervisor.
        // 取消仅停止工作，绝不停止结算监督流程。
        let (cancel, mut cancelled) = tokio::sync::watch::channel(false);
        // Stop waits for cleanup, while the ordinary caller receives the model result separately.
        // 停止等待清理，普通调用者单独接收模型结果。
        let (completed, completion) = tokio::sync::watch::channel(None);
        // The runtime caller is allowed to stop waiting without losing service ownership.
        // 运行时调用者可以停止等待，不丢失服务所有权。
        let (reply, result) = tokio::sync::oneshot::channel();
        // Weak cleanup lookup avoids keeping the service alive solely through its task table.
        // 清理使用弱引用查找，避免仅因任务表而保持服务存活。
        let worker_state = Arc::downgrade(&state);
        // Immutable business identity fences cleanup and retained repair against successors.
        // 不可变业务身份阻止清理及保留修复越过后继边界。
        let business_execution_id = profile.business_execution_id.clone();
        // The worker owns all execution and cleanup effects after its start gate.
        // 启动门之后，工作任务拥有全部执行与清理效果。
        let worker = crate::spawn_service_task(async move {
            if start_receiver.await.is_err() { return; }
            let (outcome, reason) = tokio::select! {
                biased;
                _ = cancelled.changed() => (Err(String::from("Child maintenance was stopped.")), TurnClosureReason::Stopped),
                outcome = std::panic::AssertUnwindSafe(work).catch_unwind() => {
                    let outcome = outcome.unwrap_or_else(|_| Err(String::from("Child maintenance work panicked.")));
                    let reason = if matches!(&outcome, Ok(Some(_))) { TurnClosureReason::Completed } else { TurnClosureReason::Failed };
                    (outcome, reason)
                }
            };
            // The selected work future has been dropped; drain admitted writers before inspecting retained ownership.
            // 所选择的工作 Future 已丢弃；检查保留归属之前先排空已准入写入器。
            let cleanup = match worker_state.upgrade() {
                Some(state) => {
                    let cleanup = settle_owned(&state, &worker_registry, &handle, &business_execution_id, reason).await;
                    match state.lock() {
                        Ok(mut service) => {
                            if service.child_maintenance_tasks.get(&session_id).is_some_and(|task| task.task_id == task_id) {
                                service.child_maintenance_tasks.remove(&session_id);
                            }
                        }
                        Err(error) => { let _ = completed.send(Some(Err(error.to_string()))); return; }
                    }
                    cleanup
                }
                None => Err(String::from("Child maintenance lost its service owner during cleanup.")),
            };
            let response = match &cleanup { Ok(()) => outcome, Err(error) => Err(format!("Child maintenance cleanup requires repair: {error}")) };
            let _ = reply.send(response);
            completed.send_replace(Some(cleanup));
        });
        service.child_maintenance_tasks.insert(session_id, ChildMaintenanceTask {
            storage_scope, task_id, business_execution_id: profile.business_execution_id.clone(), cancel, completion, _task: worker,
        });
        if start_sender.send(()).is_err() {
            // No work could pass this gate; remove the unstarted entry while registration still owns the mutex.
            // 没有工作能够通过此门；登记仍持有互斥锁时移除未启动条目。
            service.child_maintenance_tasks.remove(&session_id);
            return Err(String::from("Child maintenance start gate closed."));
        }
        Ok(result)
    }).await
}

/// Drains admitted writers, then settles the retained maintenance for exactly `business_execution_id`.
/// 排空已准入写入器，再结算精确 `business_execution_id` 对应保留维护。
/// No domain admission is a successful no-op; a different owner is an explicit error.
/// 没有领域准入时成功且不修改；不同所有者明确报错。
async fn settle_owned(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    handle: &Arc<SessionHandle>,
    business_execution_id: &str,
    reason: TurnClosureReason,
) -> Result<(), String> {
    // Reacquiring lifecycle ownership observes a cancelled prepare's completed blocking transfer.
    // 重新获取生命周期所有权，观察被取消准备的阻塞转交已完成。
    let read_state = state.clone();
    // The exact business identity must survive beyond this caller's borrowed arguments.
    // 精确业务身份必须在调用者借用参数之外存活。
    let business_execution_id = business_execution_id.to_owned();
    // The session key is authoritative in both service tables.
    // 会话键在两个服务表中均为权威。
    let session_id = handle.session_id();
    let execution =
        lifecycle_write::with_canonical_handles(registry, vec![handle.clone()], move || {
            let service = read_state.lock().map_err(|error| error.to_string())?;
            match service.child_maintenance_owners.get(&session_id) {
                Some(owner)
                    if owner.execution.business_execution_id.as_str() == business_execution_id =>
                {
                    Ok(Some(owner.execution.execution_id.clone()))
                }
                Some(_) => Err(String::from(
                    "Child maintenance cleanup belongs to a different business.",
                )),
                None => Ok(None),
            }
        })
        .await?;
    if let Some(execution_id) = execution {
        settlement::settle(state, registry, handle, &execution_id, reason, now_ms()?).await?;
    }
    Ok(())
}

/// Stops the current child's maintenance and rejects late scheduling for its exact business identity.
/// 停止当前子任务维护，并拒绝其精确业务身份的迟到调度。
/// `state` and `session_id` select canonical ownership; returns after cleanup or retains explicit repair failure.
/// `state` 和 `session_id` 选择规范归属；清理后返回，或保留明确修复失败。
async fn stop(state: &SharedServiceBridgeState, session_id: Uuid) -> Result<(), String> {
    // Signal an already registered worker before waiting on any blocking writer's lifecycle gate.
    // 等待任何阻塞写入器的生命周期门之前，先通知已登记工作任务。
    let pending = {
        let mut service = state.lock().map_err(|error| error.to_string())?;
        let pending = service
            .child_maintenance_tasks
            .get(&session_id)
            .map(|task| {
                task.cancel.send_replace(true);
                (task.business_execution_id.clone(), task.completion.clone())
            });
        if let Some((business, _)) = &pending {
            service
                .child_maintenance_stops
                .insert(session_id, business.clone());
        }
        pending
    };
    if let Some((_, completion)) = pending {
        return wait_completion(completion).await;
    }
    // Unloaded children cannot have newly admitted native work; do not load them merely to stop a background job.
    // 未加载子任务不可能具有新准入原生工作；不为停止后台任务而加载它们。
    let registry = state
        .lock()
        .map_err(|error| error.to_string())?
        .session_runtime_registry
        .clone();
    let handle = match registry.get(session_id) {
        Ok(handle) => handle,
        Err(SessionRuntimeError::SessionNotRegistered(_)) => {
            if state
                .lock()
                .map_err(|error| error.to_string())?
                .child_maintenance_owners
                .contains_key(&session_id)
            {
                return Err(String::from(
                    "Child maintenance repair lost its canonical handle.",
                ));
            }
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };
    // Serialize Stop with registration without holding a lifecycle gate while waiting for the worker.
    // 将停止与登记串行化，等待工作任务时不持有生命周期门。
    let stop_state = state.clone();
    // Read and mark the exact original profile under the canonical gate.
    // 在规范门内读取并标记精确原档案。
    let stop_handle = handle.clone();
    let stopping =
        lifecycle_write::with_canonical_handles(&registry, vec![handle.clone()], move || {
            let business = stop_handle
                .read(|document| {
                    document
                        .metadata
                        .child_execution_profile
                        .as_ref()
                        .map(|profile| profile.business_execution_id.clone())
                })
                .map_err(|error| error.to_string())?;
            let mut service = stop_state.lock().map_err(|error| error.to_string())?;
            let Some(business) = business else {
                if service.child_maintenance_owners.contains_key(&session_id) {
                    return Err(String::from(
                        "Child maintenance stop lost its original business profile.",
                    ));
                }
                return Ok(None);
            };
            if service
                .child_maintenance_tasks
                .get(&session_id)
                .is_some_and(|task| task.business_execution_id != business)
            {
                return Err(String::from(
                    "Child maintenance stop found a replaced worker.",
                ));
            }
            service
                .child_maintenance_stops
                .insert(session_id, business.clone());
            let completion = service
                .child_maintenance_tasks
                .get(&session_id)
                .map(|task| {
                    task.cancel.send_replace(true);
                    task.completion.clone()
                });
            Ok(Some((business, completion)))
        })
        .await?;
    let Some((business, completion)) = stopping else {
        return Ok(());
    };
    if let Some(completion) = completion {
        return wait_completion(completion).await;
    }
    settle_owned(
        state,
        &registry,
        &handle,
        &business,
        TurnClosureReason::Stopped,
    )
    .await
}

/// Waits for the worker's actual cleanup receipt; cancellation of this waiter does not cancel cleanup.
/// 等待工作任务实际清理回执；取消本等待者不会取消清理。
/// `completion` belongs to the exact registered task; returns its cleanup result or an explicit lost-owner error.
/// `completion` 属于精确已登记任务；返回清理结果或明确所有者丢失错误。
async fn wait_completion(
    mut completion: tokio::sync::watch::Receiver<Option<Result<(), String>>>,
) -> Result<(), String> {
    loop {
        if let Some(result) = completion.borrow_and_update().clone() {
            return result;
        }
        completion.changed().await.map_err(|_| {
            String::from("Child maintenance worker exited without cleanup confirmation.")
        })?;
    }
}

/// Stops registered and late maintenance across the runtime's exact descendant tree, including `session_id`.
/// 在运行时精确后代树中停止已登记及迟到维护，包括 `session_id`。
/// `runtime` supplies actual parent relationships; unrelated sessions are never selected.
/// `runtime` 提供实际父子关系；绝不选择无关会话。
pub(crate) async fn stop_tree(
    state: &SharedServiceBridgeState,
    runtime: &TaskRuntime,
    session_id: Uuid,
) -> Result<(), String> {
    // Registry parent edges include completed children, unlike a running-business-only panel cache.
    // 注册表父边包含已完成子任务，不同于仅运行中业务的面板缓存。
    let registry = runtime.subagent_registry_snapshot()?;
    // Explicit visited identities prevent duplicate cancellation of nested descendants.
    // 显式已访问身份防止重复取消嵌套后代。
    let mut visited = std::collections::BTreeSet::new();
    // Start with the requested root, which can be an ordinary main session or an embedded task.
    // 从请求根开始，它可以是普通主会话或嵌入任务。
    let mut pending = vec![session_id];
    while let Some(current) = pending.pop() {
        if !visited.insert(current) {
            continue;
        }
        pending.extend(
            registry
                .list_for_parent(current.into())
                .into_iter()
                .map(|entry| entry.session_id.as_uuid()),
        );
    }
    stop_selected(state, visited).await
}

/// Signals every selected registered worker before waiting for any single child's storage cleanup.
/// 等待任一子任务存储清理之前，先通知全部已选登记工作任务。
/// `ids` is the authoritative target set; returns the first cleanup failure after cancellation has reached all workers.
/// `ids` 是权威目标集合；取消已到达全部工作任务后返回首个清理失败。
async fn stop_selected(
    state: &SharedServiceBridgeState,
    ids: std::collections::BTreeSet<Uuid>,
) -> Result<(), String> {
    // One locked observation prevents a blocked first child from delaying cancellation of its siblings.
    // 单次加锁观察防止第一个子任务阻塞而延迟取消其同级。
    {
        let mut service = state.lock().map_err(|error| error.to_string())?;
        for session_id in &ids {
            if let Some(task) = service.child_maintenance_tasks.get(session_id) {
                task.cancel.send_replace(true);
                let business = task.business_execution_id.clone();
                service
                    .child_maintenance_stops
                    .insert(*session_id, business);
            }
        }
    }
    // Continue draining other targets even if one child needs storage repair.
    // 即使某个子任务需要存储修复，也继续排空其他目标。
    let mut failures = Vec::new();
    for session_id in ids {
        if let Err(error) = stop(state, session_id).await {
            failures.push(format!("{session_id}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Child maintenance stop requires repair: {}",
            failures.join("; ")
        ))
    }
}

/// Drains every registered maintenance worker and retained repair before application shutdown flushes writers.
/// 在应用关闭刷写写入器之前，排空全部已登记维护工作任务及保留修复。
/// Shutdown admission must already be closed in `state`; returns explicit cleanup failure.
/// `state` 中必须已关闭关闭阶段准入；明确返回清理失败。
pub(crate) async fn stop_all(state: &SharedServiceBridgeState) -> Result<(), String> {
    // Freeze identities after shutdown rejects new registrations.
    // 在关闭拒绝新登记后冻结身份。
    let ids = {
        let service = state.lock().map_err(|error| error.to_string())?;
        if !service.service_lifecycle_state.shutdown_drain_active {
            return Err(String::from(
                "Maintenance shutdown requires closed service admission.",
            ));
        }
        service
            .child_maintenance_tasks
            .keys()
            .chain(service.child_maintenance_owners.keys())
            .copied()
            .collect::<std::collections::BTreeSet<_>>()
    };
    stop_selected(state, ids).await
}

#[cfg(test)]
mod tests;
