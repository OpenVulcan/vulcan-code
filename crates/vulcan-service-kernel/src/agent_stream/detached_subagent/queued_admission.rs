//! Durable ownership for inbox-driven child executions before their start barrier is released.
//! 在释放启动屏障前，为收件箱驱动的子执行建立可靠归属。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskV1ExecutionFacts;
use vulcan_session_core::v1::DomainError;
use vulcan_session_core::v1::collaboration::{
    ActivateChildTaskInput, CollaborationChildTaskState, CollaborationExecutionId,
    CollaborationOperationId, CollaborationParticipant, CollaborationRunControl,
    ReserveChildTaskInput,
};
use vulcan_session_core::v1::document::{ChildTaskNonSuccessInput, SubagentReturnInput};
use vulcan_session_core::v1::event::UserInputSource;
use vulcan_session_core::v1::ids::TurnId;

/// A predecessor's exact durable result, built before the atomic parent handoff.
/// 在原子父侧交接前构造的前驱精确可靠结果。
enum PredecessorResult {
    /// Completed content uses the existing opaque source proof.
    /// 完成正文使用既有不透明来源凭据。
    Completed(SubagentReturnInput),
    /// Failure and stop retain their distinct original source proof.
    /// 失败与停止保留各自不同的原始来源凭据。
    NonSuccess(ChildTaskNonSuccessInput),
}

/// Identifies only the host-generated inbox reservation; accepts a durable record and returns exact identity membership.
/// 仅识别宿主生成的收件箱预留；输入可靠记录，返回精确身份归属。
pub(crate) fn is_queued_reservation(
    record: &vulcan_session_core::v1::collaboration::CollaborationChildTaskRecord,
) -> bool {
    record.source_tool_name == vulcan_subagent_core::SEND_MESSAGE_TOOL
        && record.reservation_operation_id.as_str()
            == format!("queued-child-reserve:{}", record.execution_id.as_str())
}

/// Closes a process-lost queued start that never reached recoverable profile capture.
/// 关闭进程丢失且尚未到达可恢复配置捕获阶段的排队启动。
/// Canonical gated participants and the exact reservation prove ownership; returns whether a failure was recorded.
/// 持锁的规范参与者及精确预留证明归属；返回是否已记录失败。
pub(crate) fn recover_unprepared(
    state: &SharedServiceBridgeState,
    parent: &SessionHandle,
    child: &SessionHandle,
    record: &vulcan_session_core::v1::collaboration::CollaborationChildTaskRecord,
) -> Result<bool, String> {
    if record.state != CollaborationChildTaskState::Reserved || !is_queued_reservation(record) {
        return Ok(false);
    }
    // A still-running preparation must never be mistaken for a process exit.
    // 仍在运行的准备绝不能被误判为进程退出。
    let runtime = state
        .lock()
        .map_err(|error| error.to_string())?
        .agent_runtime
        .as_ref()
        .ok_or("Queued child recovery requires a runtime.")?
        .task_runtime
        .clone();
    if runtime.is_task_execution_running(child.session_id(), record.execution_id.as_str())? {
        return Ok(false);
    }
    if child
        .read(|document| {
            document
                .metadata
                .child_execution_profile
                .as_ref()
                .is_some_and(|profile| {
                    profile.business_execution_id == record.execution_id.as_str()
                })
        })
        .map_err(|error| error.to_string())?
    {
        return Ok(false);
    }
    // The missing exact profile proves Provider launch was never admitted; preserve that host failure explicitly.
    // 缺少精确配置证明供应商启动从未准入；明确保留该宿主失败。
    let now = current_v1_unix_millis()?;
    non_success::publish_non_success(
        parent,
        child,
        record.execution_id.as_str(),
        &vulcan_session_boundary::SubagentMailboxEvent {
            parent_thread_id: parent.session_id().into(),
            parent_turn_id: None,
            child_thread_id: child.session_id().into(),
            kind: vulcan_session_boundary::SubagentMailboxEventKind::Failed,
            timestamp_ms: u64::try_from(now).map_err(|error| error.to_string())?,
            payload: serde_json::json!({"status":"failed", "execution_id":record.execution_id.as_str(), "error_text":"The process exited before queued child preparation completed."}),
        },
        now,
    )?;
    Ok(true)
}

/// Activates a queued reservation only after its exact recoverable profile was committed.
/// 仅在精确可恢复配置提交后激活排队预留。
/// The captured parent, child and execution identities fence the write; unrelated admission paths are unchanged.
/// 捕获的父、子及执行身份约束写入；无关准入路径保持原样。
pub(crate) async fn activate(
    state: &SharedServiceBridgeState,
    parent_id: SessionId,
    child_id: SessionId,
    execution_id: &str,
) -> Result<(), String> {
    // Resolve canonical participants without holding the service lock across document work.
    // 解析规范参与者，不让服务锁跨越文档工作。
    let (registry, runtime) = {
        // Snapshot service-owned handles without retaining its mutex during publication.
        // 快照服务持有的句柄，发布期间不保留服务互斥锁。
        let guard = state.lock().map_err(|error| error.to_string())?;
        (
            guard.session_runtime_registry.clone(),
            guard
                .agent_runtime
                .as_ref()
                .ok_or("Queued child activation requires a runtime.")?
                .task_runtime
                .clone(),
        )
    };
    // Retain the canonical parent that owns the completion obligation.
    // 保留拥有完成义务的规范父会话。
    let parent = registry.get(parent_id).map_err(|error| error.to_string())?;
    // Retain the exact canonical child through publication.
    // 在发布期间保留精确规范子会话。
    let child = registry.get(child_id).map_err(|error| error.to_string())?;
    // Parse the immutable execution key before publication.
    // 在发布之前解析不可变执行键。
    let execution =
        CollaborationExecutionId::try_new(execution_id).map_err(|error| error.to_string())?;
    // Native explicit spawn owns its independent activation protocol.
    // 原生显式派生拥有独立激活协议。
    let queued = parent
        .read(|document| {
            document
                .collaboration_state()
                .child_tasks
                .get(&execution)
                .is_some_and(is_queued_reservation)
        })
        .map_err(|error| error.to_string())?;
    if !queued {
        return Ok(());
    }
    crate::agent_collaboration::lifecycle_write::with_canonical_handles(&registry, vec![parent.clone(), child.clone()], move || {
        // The runtime lease must still be current after waiting for participant gates.
        // 等待参与者锁后，运行时租约仍须为当前租约。
        let facts = runtime.active_task_v1_execution_facts(child_id, execution.as_str())?;
        // Only the captured profile for this exact open business execution can authorize activation.
        // 只有该精确开放业务执行的已捕获配置可以授权激活。
        let profile = child.read_full_durable(|document| document.child_business_recovery_profile(&execution).cloned()).map_err(|error| error.to_string())?.map_err(|error| error.to_string())?;
        if facts.parent_session_id != parent_id || profile.parent_session_id != parent_id.to_string() || profile.root_session_id != facts.root_session_id.to_string() {
            return Err(String::from("Queued child activation profile has different owners."));
        }
        // Retain one observation timestamp for this publication.
        // 为此次发布保留同一观测时间戳。
        let now = current_v1_unix_millis()?;
        parent.mutate_durable_if_changed(SessionMutationAdmission::ControlPlane, SessionFlushReason::MetadataChanged, |document| {
            // Recheck the authoritative reservation inside the parent mutation.
            // 在父侧修改内部重新检查权威预留。
            let record = document.collaboration_state().child_tasks.get(&execution).ok_or(DomainError::CollaborationStateViolation { field: "queued_child.activation_missing" })?;
            if !is_queued_reservation(record) || record.child.session_id != child_id.to_string() || record.child.segment_generation != profile.segment_generation || record.child.turn_id != Some(profile.business_turn_id) || runtime.active_task_v1_execution_facts(child_id, execution.as_str()).ok().as_ref() != Some(&facts) {
                return Err(DomainError::CollaborationStateViolation { field: "queued_child.activation_owner" });
            }
            document.activate_child_task(ActivateChildTaskInput {
                operation_id: CollaborationOperationId::try_new(format!("queued-child-activate:{}", execution.as_str()))?,
                execution_id: execution, task_id: profile.task_id, active_at_ms: now,
            }).map(|mutation| match mutation { vulcan_session_core::v1::document::IdempotentMutation::Committed(_) => SessionMutationChange::Changed(()), vulcan_session_core::v1::document::IdempotentMutation::Idempotent(_) => SessionMutationChange::Unchanged(()) })
        }).map_err(|error| error.to_string())?;
        Ok(())
    }).await
}

/// Admits a resumed execution against its live runtime owner and canonical participant gates.
/// 根据实时运行时所有者及规范参与者锁准入续跑执行。
/// `facts` and `child` come from DelegateStarted; success permits acknowledgement of its start barrier.
/// `facts` 和 `child` 来自 DelegateStarted；成功才允许确认其启动屏障。
pub(super) async fn admit(
    state: &SharedServiceBridgeState,
    runtime: &AgentRuntimeBridge,
    facts: &TaskV1ExecutionFacts,
    child: &Arc<SessionHandle>,
) -> Result<(), String> {
    if !facts.is_resumed {
        return Ok(());
    }
    // Freeze the exact parent turn before waiting; a replacement turn cannot inherit this start.
    // 等待前冻结精确父回合；替代回合不能继承此次启动。
    let registry = state
        .lock()
        .map_err(|error| error.to_string())?
        .session_runtime_registry
        .clone();
    // Retain the canonical parent that owns the completion obligation.
    // 保留拥有完成义务的规范父会话。
    let parent = registry
        .get(facts.parent_session_id)
        .map_err(|error| error.to_string())?;
    // Retain the root controlling this physical child tree.
    // 保留控制该物理子树的根会话。
    let root = registry
        .get(facts.root_session_id)
        .map_err(|error| error.to_string())?;
    // Parse the immutable execution key before publication.
    // 在发布之前解析不可变执行键。
    let execution = CollaborationExecutionId::try_new(&facts.execution_id)
        .map_err(|error| error.to_string())?;
    // Explicit spawn resumptions already own admission through child_creation.
    // 显式派生续跑已通过 child_creation 拥有准入。
    if parent
        .read(|document| {
            // An existing reservation must still identify this nonterminal child execution.
            // 已有预留仍须标识本次非终态子执行。
            let Some(record) = document.collaboration_state().child_tasks.get(&execution) else {
                return Ok(false);
            };
            if record.child.session_id != facts.task_id.to_string()
                || record.child.root_session_id != facts.root_session_id.to_string()
                || record.parent_segment_generation != document.segment_generation()
                || record.state == CollaborationChildTaskState::Terminal
            {
                return Err(String::from(
                    "Queued child start has a different or terminal reservation.",
                ));
            }
            Ok(true)
        })
        .map_err(|error| error.to_string())??
    {
        return Ok(());
    }
    // Capture generation and turn together to reject history replacement.
    // 同时捕获代次及回合，以拒绝历史替换。
    let (generation, turn) = parent
        .read(|document| {
            document
                .active_turn()
                .filter(|turn| !turn.closed)
                .map(|turn| (document.segment_generation(), turn.turn_id))
                .ok_or_else(|| String::from("Queued child admission requires an open parent turn."))
        })
        .map_err(|error| error.to_string())??;
    // The existing child must belong to this exact root directory before a new parent obligation is accepted.
    // 接纳新的父义务前，已有子会话必须属于这个精确根目录。
    let root_locator = root.storage_locator().map_err(|error| error.to_string())?;
    if child.storage_locator().map_err(|error| error.to_string())?
        != vulcan_workspace_store::workspace_store::locate_embedded_task_session_v1(
            &root_locator,
            &facts.task_id.to_string(),
        )
        .map_err(|error| error.to_string())?
    {
        return Err(String::from(
            "Queued child admission has a different storage root.",
        ));
    }
    // Retain the exact canonical child through publication.
    // 在发布期间保留精确规范子会话。
    let child = child.clone();
    // Keep the admitted facts immutable across the asynchronous wait.
    // 在异步等待期间保持准入事实不可变。
    let facts = facts.clone();
    // Keep the same task runtime for lease validation.
    // 保留同一任务运行时以校验租约。
    let runtime = runtime.task_runtime.clone();
    // Retain the service owner for post-wait validation.
    // 保留服务所有者以便等待后校验。
    let state = state.clone();
    // Retain registry identity independently from the lock helper.
    // 独立于锁辅助函数保留注册表身份。
    let owner_registry = registry.clone();
    crate::agent_collaboration::lifecycle_write::with_canonical_handles(
        &registry,
        vec![root.clone(), parent.clone(), child.clone()],
        move || {
            // The start lease, service instance and root control must still authorize this execution after waiting.
            // 等待后，启动租约、服务实例及根控制仍须授权本执行。
            {
                // Snapshot service-owned handles without retaining its mutex during publication.
                // 快照服务持有的句柄，发布期间不保留服务互斥锁。
                let guard = state.lock().map_err(|error| error.to_string())?;
                if !Arc::ptr_eq(&guard.session_runtime_registry, &owner_registry)
                    || guard
                        .agent_runtime
                        .as_ref()
                        .is_none_or(|current| !Arc::ptr_eq(&current.task_runtime, &runtime))
                {
                    return Err(String::from(
                        "Queued child admission service owner was replaced.",
                    ));
                }
            }
            // Revalidate the exact execution lease immediately before mutation.
            // 在修改之前立即重新校验精确执行租约。
            let validate_owner = || {
                if runtime
                    .active_task_v1_execution_facts(facts.task_id, &facts.execution_id)
                    .ok()
                    .as_ref()
                    != Some(&facts)
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "queued_child.runtime_owner",
                    });
                }
                Ok(())
            };
            validate_owner().map_err(|error| error.to_string())?;
            root.read(|document| {
                if document.metadata.archived_at_ms.is_some()
                    || matches!(
                        document.collaboration_run_control(),
                        CollaborationRunControl::Stopped
                            | CollaborationRunControl::MainAndSubagents
                    )
                {
                    return Err(String::from(
                        "Queued child admission root is stopped or archived.",
                    ));
                }
                Ok(())
            })
            .map_err(|error| error.to_string())??;
            publish(&parent, &child, &facts, generation, turn, validate_owner)
        },
    )
    .await
}

/// Serializes startup-abort settlement with a queued admission already publishing in a blocking worker.
/// 将启动中止结算与阻塞工作线程中正在发布的排队准入串行处理。
/// Captured listener owners and event are unchanged; returns the existing exact abort result after admission drains.
/// 捕获的监听所有者及事件保持不变；准入退出后返回既有精确中止结果。
#[allow(
    clippy::too_many_arguments,
    reason = "Mirrors the existing captured startup-abort boundary without inventing another owner."
)]
pub(crate) async fn settle_startup_abort(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &AgentRuntimeBridge,
    parent_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
    child: &Arc<SessionHandle>,
    owner: startup_rejection::DetachedStartupOwner,
    event: &RuntimeEvent,
) -> Result<(), String> {
    // The same canonical parent/child gates outlive cancellation of the admission waiter.
    // 相同规范父子锁的生命周期长于准入等待器取消。
    let registry = state
        .lock()
        .map_err(|error| error.to_string())?
        .session_runtime_registry
        .clone();
    // Retain the canonical parent that owns the completion obligation.
    // 保留拥有完成义务的规范父会话。
    let parent = registry.get(parent_id).map_err(|error| error.to_string())?;
    // Own every captured callback input until the gated blocking settlement completes.
    // 持有全部已捕获回调输入，直到持锁阻塞结算完成。
    let (app, state, runtime, child, event, execution_id) = (
        app.clone(),
        state.clone(),
        runtime.clone(),
        child.clone(),
        event.clone(),
        execution_id.to_owned(),
    );
    crate::agent_collaboration::lifecycle_write::with_canonical_handles(
        &registry,
        vec![parent, child.clone()],
        move || {
            startup_abort::handle_startup_abort(
                &app,
                &state,
                &runtime,
                parent_id,
                task_id,
                &execution_id,
                &child,
                owner,
                &event,
            )
        },
    )
    .await
}

/// Atomically publishes predecessor results and the successor reservation in the captured parent turn.
/// 在捕获的父回合中原子发布前驱结果及后继预留。
/// Canonical gated handles supply durable sources; `validate_owner` rechecks the start lease inside the write.
/// 持锁的规范句柄提供可靠来源；`validate_owner` 在写入内部重查启动租约。
/// Returns after durable publication; any validation conflict leaves the entire handoff unchanged.
/// 可靠发布后返回；任何校验冲突均保持整个交接不变。
pub(crate) fn publish(
    parent: &SessionHandle,
    child: &SessionHandle,
    facts: &TaskV1ExecutionFacts,
    parent_generation: u64,
    parent_turn: TurnId,
    validate_owner: impl Fn() -> Result<(), DomainError>,
) -> Result<(), String> {
    if !facts.is_resumed
        || facts.parent_session_id != parent.session_id()
        || facts.task_id != child.session_id()
    {
        return Err(String::from(
            "Queued child admission has different canonical owners.",
        ));
    }
    // Capture the exact source root and business turn, never the previous task's terminal metadata.
    // 捕获精确来源根及业务回合，绝不使用上一任务的终态元数据。
    let (child_generation, child_turn, root_seq) = child.read_full_durable(|document| {
        // Only a unique exact root can establish the successor's source identity.
        // 只有唯一精确根才能建立后继来源身份。
        let roots = document.events().iter().filter(|entry| matches!(&entry.event,
            EventBody::UserInput(input) if input.input_source == UserInputSource::Root && input.client_action_id.as_str() == facts.execution_id)).collect::<Vec<_>>();
        // Destructure the singleton instead of assuming an indexed event has a turn.
        // 解构唯一成员，而非假定某个下标事件拥有回合。
        let [root] = roots.as_slice() else {
            return Err(String::from("Queued child admission has no exact source root."));
        };
        // A source without explicit turn ownership is rejected before publication.
        // 缺少显式回合归属的来源在发布前拒绝。
        let turn_id = root.turn_id.ok_or("Queued child source root has no turn.")?;
        // Read the same typed root selected above, preserving its exact prompt and timestamp.
        // 读取上方选中的同一个强类型根，保留精确提示词与时间戳。
        let EventBody::UserInput(input) = &root.event else {
            return Err(String::from("Queued child source is not a root input."));
        };
        if root.created_at_ms != i64::try_from(facts.admitted_at_ms).map_err(|error| error.to_string())?
            || input.text != facts.prompt
            || document.active_turn().is_none_or(|turn| turn.closed || turn.turn_id != turn_id)
        {
            return Err(String::from("Queued child admission differs from its open source root."));
        }
        Ok((document.segment_generation(), turn_id, root.seq))
    }).map_err(|error| error.to_string())??;
    // Parse the immutable execution key before publication.
    // 在发布之前解析不可变执行键。
    let execution = CollaborationExecutionId::try_new(&facts.execution_id)
        .map_err(|error| error.to_string())?;
    // Read only nonterminal owners of this same physical child.
    // 只读取该同一物理子会话的非终态所有者。
    let records = parent
        .read_full_durable(|document| {
            document
                .collaboration_state()
                .child_tasks
                .values()
                .filter(|record| {
                    record.child.session_id == facts.task_id.to_string()
                        && record.execution_id != execution
                        && record.state != CollaborationChildTaskState::Terminal
                })
                .cloned()
                .collect::<Vec<_>>()
        })
        .map_err(|error| error.to_string())?;
    // Build only exact earlier source results; unfinished or replaced executions cannot be silently retired.
    // 只构造精确更早来源结果；未完成或被替换的执行不能被静默退休。
    let mut predecessors = Vec::with_capacity(records.len());
    for record in records {
        if record.parent_segment_generation != parent_generation
            || record.parent_turn_id != parent_turn
            || record.child.segment_generation != child_generation
            || record.child.root_session_id != facts.root_session_id.to_string()
        {
            return Err(String::from("Queued child predecessor ownership changed."));
        }
        // Resolve the predecessor through its exact earlier root.
        // 通过精确更早根解析前驱。
        let request = child.read_full_durable(|document| {
            if !document.events().iter().any(|entry| entry.seq < root_seq && matches!(&entry.event,
                EventBody::UserInput(input) if input.input_source == UserInputSource::Root && input.client_action_id.as_str() == record.execution_id.as_str())) {
                return Err(String::from("Queued child predecessor has no earlier source root."));
            }
            document.recover_completed_child_return_request(&record.execution_id, record.task_id.clone()).map_err(|error| error.to_string())
        }).map_err(|error| error.to_string())??;
        // Preserve success and non-success as different opaque proof types.
        // 将成功与非成功保留为不同的不透明凭据类型。
        let result = if let Some(request) = request {
            // Use the same immutable source frontier as normal return publication.
            // 使用与正常回流发布相同的不可变来源前沿。
            let frontier = parent
                .read_full_durable(|document| {
                    document.subagent_return_frontier_before(
                        &record.child.session_id,
                        child_generation,
                        request.source_assistant_text_seq,
                    )
                })
                .map_err(|error| error.to_string())?;
            PredecessorResult::Completed(
                child
                    .read_full_durable(|document| {
                        document.build_subagent_return_input_since(request, frontier)
                    })
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?,
            )
        } else {
            PredecessorResult::NonSuccess(
                child
                    .read_full_durable(|document| {
                        document.recover_child_task_non_success_input(record.execution_id.clone())
                    })
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?
                    .ok_or("Queued child predecessor has no durable terminal proof.")?,
            )
        };
        predecessors.push((record, result));
    }
    // Retain one observation timestamp for this publication.
    // 为此次发布保留同一观测时间戳。
    let now = i64::try_from(facts.admitted_at_ms).map_err(|error| error.to_string())?;
    // Use the physical child identity as its durable task key.
    // 使用物理子身份作为其可靠任务键。
    let task =
        SubagentTaskId::try_new(facts.task_id.to_string()).map_err(|error| error.to_string())?;
    // The storage locator supplies the authoritative project identity.
    // 存储定位器提供权威项目身份。
    let locator = child.storage_locator().map_err(|error| error.to_string())?;
    parent
        .mutate_durable_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                validate_owner()?;
                if document.segment_generation() != parent_generation
                    || document.metadata.archived_at_ms.is_some()
                    || matches!(
                        document.collaboration_run_control(),
                        CollaborationRunControl::Stopped
                            | CollaborationRunControl::MainAndSubagents
                    )
                    || document
                        .active_turn()
                        .is_none_or(|turn| turn.closed || turn.turn_id != parent_turn)
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "queued_child.parent_owner",
                    });
                }
                document.apply_atomic_transition(|document| {
                    for (record, result) in predecessors {
                        // Concurrent terminal publication may advance state, but must retain exact reservation identity.
                        // 并发终态发布可以推进状态，但必须保留精确预留身份。
                        let current = document
                            .collaboration_state()
                            .child_tasks
                            .get(&record.execution_id);
                        if current.is_none_or(|current| {
                            current.child != record.child
                                || current.task_id != record.task_id
                                || current.parent_turn_id != record.parent_turn_id
                                || current.parent_segment_generation
                                    != record.parent_segment_generation
                                || current.reservation_operation_id
                                    != record.reservation_operation_id
                                || current.source_tool_name != record.source_tool_name
                        }) {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "queued_child.predecessor_changed",
                            });
                        }
                        match result {
                            PredecessorResult::Completed(input) => {
                                document.complete_child_task_with_return(
                                    record.execution_id,
                                    input,
                                    now,
                                )?;
                            }
                            PredecessorResult::NonSuccess(input) => {
                                document.record_child_task_non_success(input, now)?;
                            }
                        }
                    }
                    // A reservation keeps the parent open before recoverable preparation completes.
                    // 预留在可恢复准备完成前保持父回合开放。
                    let reserved = document.reserve_child_task(ReserveChildTaskInput {
                        operation_id: CollaborationOperationId::try_new(format!(
                            "queued-child-reserve:{}",
                            facts.execution_id
                        ))?,
                        source_tool_name: vulcan_subagent_core::SEND_MESSAGE_TOOL.into(),
                        execution_id: execution.clone(),
                        task_id: task.clone(),
                        parent_turn_id: parent_turn,
                        child: CollaborationParticipant::new(
                            facts.task_id.to_string(),
                            facts.root_session_id.to_string(),
                            locator.project_id(),
                            child_generation,
                            Some(child_turn),
                            Some(task.clone()),
                        )?,
                        reserved_at_ms: now,
                    })?;
                    Ok(
                        if matches!(
                            reserved,
                            vulcan_session_core::v1::document::IdempotentMutation::Committed(_)
                        ) {
                            SessionMutationChange::Changed(())
                        } else {
                            SessionMutationChange::Unchanged(())
                        },
                    )
                })
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
