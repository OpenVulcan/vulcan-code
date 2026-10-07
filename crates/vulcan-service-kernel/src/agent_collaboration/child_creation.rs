//! Parent-first durable child execution publication with ownership until listener admission.
//! 父先子后的可靠子执行发布，并持有监听准入前的所有权。

use super::*;
use std::collections::BTreeSet;
use vulcan_agent_runtime::agent_runtime::task_runtime::collaboration_lifecycle::ChildExecutionProfileCaptureRequest;
use vulcan_session_core::v1::{DomainError, SessionDocument};
use vulcan_workspace_store::workspace_store::{
    CreateEmbeddedTaskSessionV1Request, create_embedded_task_session_v1,
};

mod publication;

#[cfg(test)]
mod tests;

/// Settles a failed initial publication using its retained parent authority, even without a child file.
/// 即使尚无子文件，也使用保留的父权威结算失败的初始发布。
/// `generation` and `reservation` freeze admission identity; returns after durable failure or a conflict.
/// `generation` 和 `reservation` 冻结准入身份；可靠失败落盘后返回，归属冲突则报错。
fn fail_reserved_creation(
    parent: &SessionHandle,
    generation: u64,
    reservation: &ChildTaskReservationRequest,
) -> Result<(), String> {
    let execution_id = CollaborationExecutionId::try_new(&reservation.execution.execution_id)
        .map_err(|error| error.to_string())?;
    let failed_operation = CollaborationOperationId::try_new(format!(
        "{}:terminal:failed",
        reservation.operation_id.as_str()
    ))
    .map_err(|error| error.to_string())?;
    let failed_at = now_ms()?;
    parent
        .mutate_durable_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                if document.segment_generation() != generation {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "child_creation.failed_generation",
                    });
                }
                let Some(record) = document
                    .collaboration_state()
                    .child_tasks
                    .get(&execution_id)
                else {
                    return Ok(SessionMutationChange::Unchanged(()));
                };
                if record.reservation_operation_id != reservation.operation_id
                    || record.task_id != reservation.execution.task_id
                    || record.child.session_id != reservation.execution.task_session_id.to_string()
                    || !document
                        .metadata
                        .child_creation_intents
                        .contains_key(&execution_id)
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "child_creation.failed_owner",
                    });
                }
                if record.state == vulcan_session_core::v1::CollaborationChildTaskState::Terminal {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                if record.state != vulcan_session_core::v1::CollaborationChildTaskState::Reserved {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "child_creation.failed_phase",
                    });
                }
                document
                    .terminal_child_task(TerminalChildTaskInput {
                        operation_id: failed_operation,
                        execution_id,
                        task_id: reservation.execution.task_id.clone(),
                        outcome: CollaborationChildTaskTerminalOutcome::Failed,
                        terminal_at_ms: failed_at,
                    })
                    .map(|mutation| {
                        if mutation.changed() {
                            SessionMutationChange::Changed(())
                        } else {
                            SessionMutationChange::Unchanged(())
                        }
                    })
            },
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Publishes an initial or resumed child from trusted preparation before DelegateStarted is exposed.
/// 在暴露 DelegateStarted 之前，从可信准备发布初始或续跑子执行。
/// The host owns canonical validation, ordered file publication and failure cleanup until subscription succeeds.
/// 宿主持有规范校验、有序文件发布及失败清理责任，直到订阅成功。
/// Returns the original reservation receipt; no Provider request is started by this method.
/// 返回原预留回执；本方法不启动 Provider 请求。
pub(super) async fn create(
    host: &SessionV1NativeAgentCollaborationRuntime,
    reservation: ChildTaskReservationRequest,
    preparation: ChildExecutionProfileCaptureRequest,
) -> Result<ChildTaskLifecycleReceipt, String> {
    let invocation = validate_host_invocation(
        &host.state,
        &reservation.invocation,
        &reservation.operation_id,
        None,
        None,
    )?;
    let runtime = invocation
        .state
        .lock()
        .map_err(|error| error.to_string())?
        .agent_runtime
        .clone()
        .ok_or("Child execution creation requires an agent runtime.")?;
    let facts = runtime.task_runtime.active_task_v1_execution_facts(
        reservation.execution.task_session_id,
        &reservation.execution.execution_id,
    )?;
    if facts.parent_session_id != invocation.source_session_id
        || preparation.task_id != facts.task_id
        || preparation.execution_id != facts.execution_id
        || preparation.parent_session_id != facts.parent_session_id
        || preparation.root_session_id != facts.root_session_id
        || preparation.invocation_config.prompt != facts.prompt
        || preparation.runtime_identity.display_title != facts.display_title
        || reservation.execution.task_id.as_str() != facts.task_id.to_string()
    {
        return Err(String::from(
            "Child execution creation does not match its runtime admission.",
        ));
    }
    let root = crate::session_goal_lifecycle::session_v1_handle_from_state(
        &invocation.state,
        facts.root_session_id,
    )
    .await?;
    let root_locator = root.storage_locator().map_err(|error| error.to_string())?;
    let parent = invocation.source_handle.clone();
    let parent_turn =
        TurnId::try_new(invocation.source_turn_id).map_err(|error| error.to_string())?;
    let parent_generation = parent
        .read(|document| document.segment_generation())
        .map_err(|error| error.to_string())?;
    let root_generation = root
        .read(|document| document.segment_generation())
        .map_err(|error| error.to_string())?;
    let usage_root = parent.lifetime_usage_snapshot()?.root_session_id;
    if usage_root != facts.root_session_id.to_string()
        || root_locator.project_id() != invocation.source_locator.project_id()
    {
        return Err(String::from(
            "Child execution creation conflicts with its parent's root or project.",
        ));
    }
    let created_at = i64::try_from(facts.admitted_at_ms)
        .map_err(|_| String::from("Initial child admission time exceeds the durable range."))?;
    let registry = invocation.registry.clone();
    // The unborn child's UUID joins the same global lock order as existing canonical participants.
    // 尚未创建子任务的 UUID 与已有规范参与者采用同一全局锁顺序。
    let mut gates = Vec::new();
    for identity in BTreeSet::from([root.session_id(), parent.session_id(), facts.task_id]) {
        gates.push(
            registry
                .acquire_lifecycle_transition(identity)
                .await
                .map_err(|error| error.to_string())?,
        );
    }
    let app = host.app.clone();
    crate::spawn_service_blocking_task(move || {
        // IO and listener admission retain the gates even when the requesting future is cancelled.
        // 即使请求 Future 取消，IO 与监听准入仍保留生命周期锁。
        let _gates = gates;
        startup_recovery::worker::require_active_owner(&invocation.state, &registry)?;
        if invocation
            .state
            .lock()
            .map_err(|error| error.to_string())?
            .agent_runtime
            .as_ref()
            .is_none_or(|current| !Arc::ptr_eq(&current.task_runtime, &runtime.task_runtime))
        {
            return Err(String::from(
                "Child execution creation runtime was replaced.",
            ));
        }
        for handle in [&root, &parent] {
            if !Arc::ptr_eq(
                &registry
                    .get(handle.session_id())
                    .map_err(|error| error.to_string())?,
                handle,
            ) {
                return Err(String::from(
                    "Child execution creation has a replaced ancestor.",
                ));
            }
        }
        root.read(|document| {
            if document.segment_generation() != root_generation
                || document.metadata.archived_at_ms.is_some()
                || matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents
                )
            {
                return Err(String::from(
                    "Child execution creation root is stopped, archived or replaced.",
                ));
            }
            Ok(())
        })
        .map_err(|error| error.to_string())??;
        // Before publication, invalid or cancelled callers leave no parent obligation.
        // 发布前，无效或取消的调用方不会留下父义务。
        if runtime
            .task_runtime
            .active_task_v1_execution_facts(facts.task_id, &facts.execution_id)?
            != facts
        {
            return Err(String::from(
                "Child execution creation admission facts were replaced.",
            ));
        }
        // Resume derives coordinates from the canonical existing history while all participant gates are retained.
        // 续跑在持有全部参与者锁时，从规范已有历史取得坐标。
        let execution_id = CollaborationExecutionId::try_new(&facts.execution_id)
            .map_err(|error| error.to_string())?;
        let existing_intent = parent
            .read_full_durable(|document| {
                document
                    .metadata
                    .child_creation_intents
                    .get(&execution_id)
                    .cloned()
            })
            .map_err(|error| error.to_string())?;
        let intent = if let Some(intent) = existing_intent {
            // A repeated original tool call reuses its committed allocator result instead of allocating again.
            // 重复原工具调用复用已提交分配结果，不再次分配。
            let is_resumption = matches!(
                intent.origin,
                vulcan_session_core::v1::child_execution::ChildCreationOrigin::Resumption { .. }
            );
            if is_resumption != facts.is_resumed
                || intent.created_at_ms != created_at
                || intent.project_tag != root_locator.project_id()
                || preparation
                    .bind_child_turn(
                        intent.profile.segment_generation,
                        intent.profile.business_turn_id,
                    )
                    .map_err(|error| error.to_string())?
                    != intent.profile
            {
                return Err(String::from(
                    "Child execution retry conflicts with its durable creation source.",
                ));
            }
            intent
        } else if facts.is_resumed {
            let child = registry
                .get(facts.task_id)
                .map_err(|error| error.to_string())?;
            let expected_locator =
                locate_embedded_task_session_v1(&root_locator, &facts.task_id.to_string())
                    .map_err(|error| error.to_string())?;
            if child.storage_locator().map_err(|error| error.to_string())? != expected_locator {
                return Err(String::from(
                    "Child resumption has a different canonical storage owner.",
                ));
            }
            child
                .read_full_durable(|document| {
                    document
                        .prepare_child_resumption(preparation, created_at)
                        .map(|(_, intent)| intent)
                })
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?
        } else {
            SessionDocument::prepare_initial_child_creation(
                preparation,
                root_locator.project_id(),
                created_at,
            )
            .map_err(|error| error.to_string())?
            .1
        };
        let child_identity = CollaborationParticipant::new(
            facts.task_id.to_string(),
            facts.root_session_id.to_string(),
            root_locator.project_id(),
            intent.profile.segment_generation,
            None,
            Some(reservation.execution.task_id.clone()),
        )
        .map_err(|error| error.to_string())?;
        // Cleanup authority starts only after this exact intent was accepted, including uncertain flush failures.
        // 仅在精确意图被接纳后取得清理权威，也覆盖刷写结果不确定的失败。
        let mut owns_reservation = false;
        let result = (|| {
            let receipt = parent
                .mutate_durable_if_changed(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        if document.segment_generation() != parent_generation
                            || document.metadata.archived_at_ms.is_some()
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "child_creation.parent_owner",
                            });
                        }
                        require_open_parent_turn(document, parent_turn)?;
                        // Gate waits cannot turn a completed or replaced physical tool API into new creation authority.
                        // 等待锁不能让已完成或被替换的物理工具 API 取得新的创建权威。
                        if document.active_api().is_none_or(|api| {
                            api.api_call_id != reservation.invocation.api_call_id
                                || !api.tool_seqs.contains(&reservation.invocation.tool_seq)
                        }) {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "child_creation.source_api",
                            });
                        }
                        if runtime
                            .task_runtime
                            .active_task_v1_execution_facts(facts.task_id, &facts.execution_id)
                            .map_err(|_| DomainError::CollaborationStateViolation {
                                field: "child_creation.runtime_owner",
                            })?
                            != facts
                        {
                            return Err(DomainError::CollaborationStateViolation {
                                field: "child_creation.replaced_runtime_facts",
                            });
                        }
                        let mutation = document.reserve_child_creation(
                            ReserveChildTaskInput {
                                source_tool_name: invocation.source_tool_id.clone(),
                                operation_id: reservation.operation_id.clone(),
                                execution_id: CollaborationExecutionId::try_new(
                                    &facts.execution_id,
                                )?,
                                task_id: reservation.execution.task_id.clone(),
                                parent_turn_id: parent_turn,
                                child: child_identity,
                                reserved_at_ms: reservation.reserved_at_ms,
                            },
                            intent,
                        )?;
                        owns_reservation = match &mutation {
                            IdempotentMutation::Committed(receipt)
                            | IdempotentMutation::Idempotent(receipt) => receipt.task.state
                                == vulcan_session_core::v1::CollaborationChildTaskState::Reserved,
                        };
                        Ok(session_change(mutation))
                    },
                )
                .map_err(|error| error.to_string())?;
            if receipt.value.task.state
                != vulcan_session_core::v1::CollaborationChildTaskState::Reserved
            {
                return Err(String::from(
                    "Child execution creation reservation already left startup.",
                ));
            }
            // Registry admission, file publication and listener rejection share one owned publication stage.
            // 注册表准入、文件发布和监听拒绝共用一个有归属的发布阶段。
            publication::publish(&registry, &root, &parent, &receipt.value.task, |child| {
                crate::agent_stream::start_admitted_detached_subagent_listener(
                    app,
                    invocation.state.clone(),
                    invocation.controller_runtime.clone(),
                    runtime.clone(),
                    parent.session_id(),
                    child,
                    facts.execution_id.clone(),
                )
            })?;
            Ok(child_task_lifecycle_receipt(
                reservation.operation_id.clone(),
                receipt,
            ))
        })();
        match result {
            Ok(receipt) => Ok(receipt),
            Err(primary) => {
                let mut failures = Vec::new();
                if owns_reservation
                    && let Err(error) =
                        fail_reserved_creation(&parent, parent_generation, &reservation)
                {
                    failures.push(format!("Child parent failure settlement failed: {error}"));
                }
                if failures.is_empty() {
                    Err(primary)
                } else {
                    Err(format!("{primary} {}", failures.join(" ")))
                }
            }
        }
    })
    .await
    .map_err(|error| error.to_string())?
}
