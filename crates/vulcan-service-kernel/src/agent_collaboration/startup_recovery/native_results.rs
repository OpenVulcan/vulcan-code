//! Reconstructs owned terminal results from exact durable native child executions.
//! 从精确可靠原生子执行重建所属终态结果。

use super::*;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskRecord, CollaborationChildTaskState,
};

/// Repairs closed source executions while leaving unfinished work open.
/// 修复已闭合来源执行，同时保持未完成任务开放。
/// `state` identifies live owners; `records` are hints revalidated with canonical parents and children under lifecycle gates.
/// `state` 识别活动所有者；`records` 是在线生命周期锁内与规范父子文档重新校验的线索。
/// `root` and `root_generation` retain discovered storage and tree-stop authority through those same gates.
/// `root` 与 `root_generation` 通过相同锁保留发现时的存储及整树停止权威。
/// Returns the number of newly published terminal results; never starts a provider or fabricates text.
/// 返回新发布终态结果数量；绝不启动供应商或制造正文。
pub(in crate::agent_collaboration) async fn recover_terminal_returns(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    root: Arc<SessionHandle>,
    root_generation: u64,
    parent: Arc<SessionHandle>,
    child: Arc<SessionHandle>,
    records: Vec<CollaborationChildTaskRecord>,
) -> Result<usize, String> {
    // Retain the same service owner throughout the guarded blocking publication.
    // 在受保护的阻塞发布期间保留相同服务所有者。
    let state = state.clone();
    // Reject a replaced startup owner after waiting for lifecycle admission.
    // 等待生命周期准入后拒绝已替换的启动所有者。
    let owned_registry = registry.clone();
    lifecycle_write::with_canonical_handles(
        registry,
        vec![root.clone(), parent.clone(), child.clone()],
        move || {
            worker::require_active_owner(&state, &owned_registry)?;
            if root
                .read(|document| document.segment_generation())
                .map_err(|error| error.to_string())?
                != root_generation
            {
                return Err(String::from("native result root generation changed"));
            }
            // A physical child can retain several historical executions, each with separate source proof.
            // 一个物理子会话可保留多次历史执行，每次都有独立来源凭据。
            let mut repaired = 0;
            for original in records {
                // Discovery never authorizes publication; read the current durable canonical reservation.
                // 发现绝不授权发布；读取当前可靠规范预留。
                let current = parent
                    .read_full_durable(|document| {
                        if document.segment_generation() != original.parent_segment_generation {
                            return Err(String::from("native result parent generation changed"));
                        }
                        document
                            .collaboration_state()
                            .child_tasks
                            .get(&original.execution_id)
                            .cloned()
                            .ok_or_else(|| String::from("native result reservation is missing"))
                    })
                    .map_err(|error| error.to_string())??;
                if current.child != original.child
                    || current.child.root_session_id != root.session_id().to_string()
                    || current.task_id != original.task_id
                    || current.parent_turn_id != original.parent_turn_id
                    || current.reservation_operation_id != original.reservation_operation_id
                    || current.source_tool_name != original.source_tool_name
                {
                    return Err(String::from(
                        "native result reservation changed after discovery",
                    ));
                }
                if current.state == CollaborationChildTaskState::Terminal {
                    continue;
                }
                // A whole-tree stop may have persisted before its descendant control writes.
                // 整树停止可能在后代控制写入前已经落盘。
                let stopped_without_owner =
                    stopped_child_has_no_live_owner(&state, &root, &parent, &child)?;
                if stopped_without_owner
                    && settle_unpublished_resumption_stop(&parent, &child, &current)?
                {
                    repaired += 1;
                    continue;
                }
                // Resolve the original root action and business terminal from full durable source history.
                // 从完整可靠来源历史解析原根动作与业务终态。
                let request = child
                    .read_full_durable(|document| {
                        if document.segment_generation() != current.child.segment_generation
                            || document.metadata.session_id != current.child.session_id
                        {
                            return Err(String::from("native result child scope changed"));
                        }
                        document
                            .recover_completed_child_return_request(
                                &current.execution_id,
                                current.task_id.clone(),
                            )
                            .map_err(|error| error.to_string())
                    })
                    .map_err(|error| error.to_string())??;
                let Some(request) = request else {
                    // A persisted host reason supplies failure versus stop; ordinary diagnostics never classify it.
                    // 持久宿主原因提供失败与停止分类；普通诊断绝不负责分类。
                    let non_success = child
                        .read_full_durable(|document| {
                            document
                                .recover_child_task_non_success_input(current.execution_id.clone())
                        })
                        .map_err(|error| error.to_string())?
                        .map_err(|error| error.to_string())?;
                    let Some(input) = non_success else {
                        if stopped_without_owner {
                            repaired +=
                                usize::from(recover_explicit_stop(&parent, &child, &current)?);
                        }
                        continue;
                    };
                    // Commit the result and parent-owned terminal in the same existing durable transition.
                    // 在同一次既有可靠转换中提交结果与父侧所属终态。
                    let observed_at_ms = now_ms()?;
                    let receipt = parent
                        .mutate_durable_if_changed(
                            SessionMutationAdmission::ControlPlane,
                            SessionFlushReason::SubagentReturned,
                            |document| {
                                document
                                    .record_child_task_non_success(input, observed_at_ms)
                                    .map(session_change)
                            },
                        )
                        .map_err(|error| error.to_string())?;
                    repaired += usize::from(receipt.changed);
                    continue;
                };
                // Coverage belongs to this generation, even if an older generation reused the same sequence numbers.
                // 覆盖归属于此代次，即使旧代次复用了相同序号。
                let frontier = parent
                    .read_full_durable(|document| {
                        document.subagent_return_frontier_before(
                            &current.child.session_id,
                            current.child.segment_generation,
                            request.source_assistant_text_seq,
                        )
                    })
                    .map_err(|error| error.to_string())?;
                // Use the same opaque builder and atomic parent transition as live completion.
                // 使用与实时完成相同的不透明构造器及原子父转换。
                let input = child
                    .read_full_durable(|document| {
                        document.build_subagent_return_input_since(request, frontier)
                    })
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())?;
                // Timestamp only the parent observation; original source timestamps remain immutable.
                // 只记录父侧观测时间；原来源时间保持不可变。
                let observed_at_ms = now_ms()?;
                let receipt = parent.mutate_durable_if_changed(
                    SessionMutationAdmission::ControlPlane,
                    SessionFlushReason::SubagentReturned,
                    |document| {
                        document
                            .complete_child_task_with_return(
                                current.execution_id,
                                input,
                                observed_at_ms,
                            )
                            .map(session_change)
                    },
                );
                repaired += usize::from(receipt.map_err(|error| error.to_string())?.changed);
            }
            Ok(repaired)
        },
    )
    .await
}

/// Checks explicit tree/parent/child stop and absence of process-local execution ownership.
/// 检查整树、父或子明确停止，以及进程内执行所有权缺失。
/// Callers hold root, parent and child lifecycle gates; returns whether stop repair is authorized.
/// 调用方持有根、父、子生命周期锁；返回是否授权停止修复。
fn stopped_child_has_no_live_owner(
    state: &SharedServiceBridgeState,
    root: &SessionHandle,
    parent: &SessionHandle,
    child: &SessionHandle,
) -> Result<bool, String> {
    // Only explicit stop controls authorize recovery; archive and missing workers do not.
    // 只有明确停止控制授权恢复；归档和工作者缺失均不授权。
    let mut explicitly_stopped = false;
    for handle in [root, parent, child] {
        explicitly_stopped |= handle
            .read_full_durable(|document| {
                matches!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped | CollaborationRunControl::MainAndSubagents,
                )
            })
            .map_err(|error| error.to_string())?;
    }
    if !explicitly_stopped {
        return Ok(false);
    }
    // A process-local owner is still responsible for its true terminal outcome.
    // 进程内所有者仍负责其真实终态结果。
    let runtime = {
        // Do not retain the service lock while consulting the task runtime.
        // 查询任务运行时期间不保留服务锁。
        let guard = state.lock().map_err(|error| error.to_string())?;
        if guard
            .detached_subagent_task_listeners
            .contains_key(&child.session_id())
        {
            return Ok(false);
        }
        guard.agent_runtime.clone()
    };
    if let Some(runtime) = runtime
        && runtime.task_runtime.is_task_running(child.session_id())?
    {
        return Ok(false);
    }
    Ok(true)
}

/// Cancels an unpublished successor without closing or rewriting its predecessor.
/// 取消尚未发布的后继，不闭合或改写其前序。
/// `record` and canonical handles are guarded; callers proved explicit stop and no live child owner.
/// `record` 和规范句柄均受锁保护；调用方已证明明确停止且子任务无活动所有者。
/// Returns true only for a reserved resumption whose exact root input was never published.
/// 仅对尚未发布精确根输入的已预留后继返回 true。
fn settle_unpublished_resumption_stop(
    parent: &SessionHandle,
    child: &SessionHandle,
    record: &CollaborationChildTaskRecord,
) -> Result<bool, String> {
    if record.state != CollaborationChildTaskState::Reserved {
        return Ok(false);
    }
    // The parent-owned captured intent proves this is a successor, not damaged initial storage.
    // 父侧捕获意图证明这是后继，而非损坏的初始存储。
    let reservation_seq = parent
        .read_full_durable(|document| {
            if !document.metadata.child_creation_intents.get(&record.execution_id)
            .is_some_and(|intent| matches!(intent.origin,
                vulcan_session_core::v1::child_execution::ChildCreationOrigin::Resumption { .. }))
        {
            return Ok(None);
        }
            document
                .collaboration_state()
                .operations
                .get(&record.reservation_operation_id)
                .map(|operation| Some(operation.event_seq))
                .ok_or_else(|| String::from("stopped successor reservation operation is missing"))
        })
        .map_err(|error| error.to_string())??;
    let Some(reservation_seq) = reservation_seq else {
        return Ok(false);
    };
    // Original root identity is the publication boundary; profile age alone is insufficient.
    // 原根身份是发布边界；仅凭配置的新旧程度不足以判断。
    let has_root = child
        .read_full_durable(|document| {
            if document.segment_generation() != record.child.segment_generation
                || document.metadata.session_id != record.child.session_id
            {
                return Err(String::from("stopped successor child scope changed"));
            }
            Ok(document.events().iter().any(|event| {
                matches!(&event.event,
            EventBody::UserInput(input)
                if input.input_source == vulcan_session_core::v1::event::UserInputSource::Root
                    && input.client_action_id.as_str() == record.execution_id.as_str())
            }))
        })
        .map_err(|error| error.to_string())??;
    if has_root {
        return Ok(false);
    }
    // The immutable reservation key preserves retry identity without manufacturing a child result.
    // 不可变预留键保留重试身份，不制造子结果。
    let operation_id = CollaborationOperationId::try_new(format!(
        "child-resumption-stop:{}:{}",
        record.parent_segment_generation,
        reservation_seq.value(),
    ))
    .map_err(|error| error.to_string())?;
    // Observe cancellation now while keeping all original creation timestamps unchanged.
    // 当前观测取消，全部原创建时间保持不变。
    let stopped_at_ms = now_ms()?;
    parent.mutate_durable_if_changed(
        SessionMutationAdmission::ControlPlane,
        SessionFlushReason::SubagentReturned,
        |document| {
            if document.collaboration_state().child_tasks.get(&record.execution_id) != Some(record) {
                return Err(vulcan_session_core::v1::DomainError::CollaborationStateViolation {
                    field: "stopped_successor.reservation_changed",
                });
            }
            document.terminal_child_task(vulcan_session_core::v1::collaboration::TerminalChildTaskInput {
                operation_id,
                execution_id: record.execution_id.clone(),
                task_id: record.task_id.clone(),
                outcome: vulcan_session_core::v1::collaboration::CollaborationChildTaskTerminalOutcome::Stopped,
                terminal_at_ms: stopped_at_ms,
            }).map(session_change)
        },
    ).map_err(|error| error.to_string())?;
    Ok(true)
}

/// Closes the original open source after explicit stop and absence of live ownership were proved.
/// 在已证明明确停止且没有活动所有者后闭合原开放来源。
/// `record` is the exact parent reservation; returns false for a different or already closed turn.
/// `record` 是精确父预留；回合不同或已闭合时返回 false。
fn recover_explicit_stop(
    parent: &SessionHandle,
    child: &SessionHandle,
    record: &CollaborationChildTaskRecord,
) -> Result<bool, String> {
    // Retain source identity even when stop intent was recorded only on an ancestor.
    // 即使停止意图只记录于祖先，也保持来源身份。
    let stopped_original = child
        .read_full_durable(|document| {
            // Only the original open root can be closed; a later question or task is unrelated.
            // 只能闭合原开放根回合；后续问题或任务与此无关。
            let Some(turn) = document.active_turn().filter(|turn| !turn.closed) else {
                return false;
            };
            document.events().iter().any(|event| {
                event.turn_id == Some(turn.turn_id)
                    && matches!(&event.event, EventBody::UserInput(input)
                    if input.input_source == vulcan_session_core::v1::event::UserInputSource::Root
                        && input.client_action_id.as_str() == record.execution_id.as_str())
            })
        })
        .map_err(|error| error.to_string())?;
    if !stopped_original {
        return Ok(false);
    }
    // Reuse the verified live publisher for source closure and atomic parent result recording.
    // 复用已验证实时发布器，闭合来源并原子记录父结果。
    let observed_at_ms = now_ms()?;
    crate::agent_stream::non_success::publish_non_success(
        parent,
        child,
        record.execution_id.as_str(),
        &vulcan_session_boundary::SubagentMailboxEvent {
            parent_thread_id: parent.session_id().into(),
            parent_turn_id: None,
            child_thread_id: child.session_id().into(),
            kind: vulcan_session_boundary::SubagentMailboxEventKind::Stopped,
            timestamp_ms: observed_at_ms.max(0) as u64,
            payload: serde_json::json!({}),
        },
        observed_at_ms,
    )?;
    Ok(true)
}
