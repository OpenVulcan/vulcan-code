//! Durable terminal reports for failed and stopped owned child executions.
//! 失败和停止的所属子执行可靠终态报告。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionMutationAdmission, SessionMutationChange,
};
use vulcan_session_boundary::{SubagentMailboxEvent, SubagentMailboxEventKind};
use vulcan_session_core::v1::DomainError;
use vulcan_session_core::v1::collaboration::{
    CollaborationChildTaskTerminalOutcome, CollaborationExecutionId,
};
use vulcan_session_core::v1::event::{TurnOutcome, UserInputSource};

/// Reads the exact host-produced mailbox and rejects another execution or nonterminal event.
/// 读取精确宿主邮箱，拒绝其他执行或非终态事件。
/// The listener supplies both session identities and execution; returns validated terminal feedback.
/// 监听器提供双方会话及执行身份；返回已校验终态反馈。
pub(super) fn terminal_mailbox(
    event: &RuntimeEvent,
    parent_id: SessionId,
    child_id: SessionId,
    execution_id: &str,
) -> Result<SubagentMailboxEvent, String> {
    // Delegate metadata is produced by build_task_host_payload, never by model text.
    // 委派元数据由 build_task_host_payload 产生，绝不来自模型文本。
    let metadata = event
        .metadata
        .as_ref()
        .ok_or("Child terminal metadata is missing.")?;
    if subagent_feedback_execution_id_from_tool_metadata(metadata)? != execution_id {
        return Err(String::from(
            "Child terminal execution does not match its listener.",
        ));
    }
    // The mailbox is the typed terminal discriminator in the existing runtime contract.
    // 邮箱是既有运行时契约中的强类型终态判别器。
    let mailbox: SubagentMailboxEvent =
        serde_json::from_value(metadata["payload"]["subagent_mailbox_event"].clone())
            .map_err(|error| error.to_string())?;
    if mailbox.parent_thread_id != parent_id.into()
        || mailbox.child_thread_id != child_id.into()
        || !matches!(
            mailbox.kind,
            SubagentMailboxEventKind::Completed
                | SubagentMailboxEventKind::Failed
                | SubagentMailboxEventKind::Stopped
        )
    {
        return Err(String::from(
            "Child terminal mailbox does not match its listener ownership.",
        ));
    }
    Ok(mailbox)
}

/// Closes the exact failed/stopped source and publishes its opaque proof before parent wake-up.
/// 在唤醒父会话之前关闭精确失败或停止来源，并发布不透明凭据。
/// Canonical handles and execution identify the reserved relationship; mailbox supplies host outcome.
/// 规范句柄及执行标识预留关系；邮箱提供宿主结果。
/// Returns only after both source closure and atomic parent result publication are durable.
/// 仅在来源闭合与原子父结果发布均可靠落盘后返回。
pub(crate) fn publish_non_success(
    parent: &SessionHandle,
    child: &SessionHandle,
    execution_id: &str,
    mailbox: &SubagentMailboxEvent,
    now_ms: i64,
) -> Result<(), String> {
    // Classify only explicit host terminal variants; successful content has a separate builder.
    // 仅按明确宿主终态分支分类；成功正文使用独立构造器。
    let outcome = match mailbox.kind {
        SubagentMailboxEventKind::Failed => CollaborationChildTaskTerminalOutcome::Failed,
        SubagentMailboxEventKind::Stopped => CollaborationChildTaskTerminalOutcome::Stopped,
        _ => return Err(String::from("Expected a failed or stopped child terminal.")),
    };
    if mailbox.parent_thread_id != parent.session_id().into()
        || mailbox.child_thread_id != child.session_id().into()
    {
        return Err(String::from(
            "Child report mailbox has different canonical owners.",
        ));
    }
    // Require the original parent reservation before touching any child state.
    // 修改任何子状态之前，要求存在原父预留。
    let execution =
        CollaborationExecutionId::try_new(execution_id).map_err(|error| error.to_string())?;
    // Retain the original child generation, including when a newer turn has already been created.
    // 保留原子代次，包括已经创建更新回合的情况。
    let reservation = parent
        .read_full_durable(|document| {
            document
                .collaboration_state()
                .child_tasks
                .get(&execution)
                .cloned()
        })
        .map_err(|error| error.to_string())?
        .ok_or("Child report has no owned execution reservation.")?;
    if reservation.child.session_id != child.session_id().to_string() {
        return Err(String::from(
            "Child report does not match the reserved child.",
        ));
    }
    // Host errors are mandatory for failure, while normal stop never invents an error note.
    // 失败必须携带宿主错误，而正常停止绝不制造错误提示。
    let failure_detail = if outcome == CollaborationChildTaskTerminalOutcome::Failed {
        Some(
            mailbox
                .payload
                .get("error_text")
                .and_then(serde_json::Value::as_str)
                .filter(|text| !text.trim().is_empty())
                .ok_or("Failed child feedback is missing its original error.")?
                .to_owned(),
        )
    } else {
        None
    };
    // Resolve the original turn only through its unique root action, never through the current turn.
    // 仅通过唯一根动作解析原回合，绝不通过当前回合推断。
    let (turn_id, started_at_ms) = child
        .read_full_durable(|document| {
            if document.segment_generation() != reservation.child.segment_generation {
                return Err(String::from("Child report source generation was replaced."));
            }
            // Full durable history includes the exact root even after segment sealing.
            // 完整可靠历史在分段封口后仍包含精确根输入。
            let roots = document
                .events()
                .iter()
                .filter(|event| {
                    matches!(&event.event,
            EventBody::UserInput(input) if input.input_source == UserInputSource::Root
                && input.client_action_id.as_str() == execution_id)
                })
                .collect::<Vec<_>>();
            if roots.len() != 1 {
                return Err(String::from(
                    "Child report requires one exact execution root.",
                ));
            }
            Ok((
                roots[0]
                    .turn_id
                    .ok_or("Child execution root has no turn.")?,
                roots[0].created_at_ms,
            ))
        })
        .map_err(|error| error.to_string())??;
    child
        .mutate_durable_if_changed(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::DisplayBoundary,
            |document| {
                if document.segment_generation() != reservation.child.segment_generation {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "child_report.source_generation",
                    });
                }
                // An existing exact terminal is authoritative; never close an unrelated replacement turn.
                // 已有精确终态是权威；绝不关闭无关替代回合。
                if document.events().iter().any(|event| {
                    event.turn_id == Some(turn_id)
                        && matches!(&event.event, EventBody::TurnStats(_))
                }) {
                    return Ok(SessionMutationChange::Unchanged(()));
                }
                if document
                    .active_turn()
                    .is_none_or(|turn| turn.closed || turn.turn_id != turn_id)
                {
                    return Err(DomainError::CollaborationStateViolation {
                        field: "child_report.source_turn",
                    });
                }
                document.apply_atomic_transition(|document| {
                    match outcome {
                        CollaborationChildTaskTerminalOutcome::Failed => {
                            document.fail_execution_boundary_and_close_turn(
                                "detached_execution_failed",
                                failure_detail.as_deref(),
                                now_ms,
                            )?;
                        }
                        CollaborationChildTaskTerminalOutcome::Stopped => {
                            if document.active_api().is_some() {
                                document.interrupt_active_api(now_ms)?;
                            }
                            document.close_turn(
                                TurnOutcome::Stopped,
                                now_ms.saturating_sub(started_at_ms).max(0),
                                now_ms,
                            )?;
                        }
                        _ => unreachable!("non-success classification is checked before mutation"),
                    }
                    Ok(SessionMutationChange::Changed(()))
                })
            },
        )
        .map_err(|error| error.to_string())?;
    // Build only after source publication; transport text cannot manufacture this input.
    // 仅在来源发布后构造；传输正文无法伪造该输入。
    let input = child
        .read_full_durable(|document| {
            document.build_child_task_non_success_input(execution, turn_id, outcome)
        })
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())?;
    parent
        .mutate_durable_if_changed(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::SubagentReturned,
            |document| {
                document
                    .record_child_task_non_success(input, now_ms)
                    .map(|result| match result {
                        vulcan_session_core::v1::document::IdempotentMutation::Committed(value) => {
                            SessionMutationChange::Changed(value)
                        }
                        vulcan_session_core::v1::document::IdempotentMutation::Idempotent(
                            value,
                        ) => SessionMutationChange::Unchanged(value),
                    })
            },
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}
