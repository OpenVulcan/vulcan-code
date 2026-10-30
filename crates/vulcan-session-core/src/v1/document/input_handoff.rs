//! Initial scheduled input batches and exact completed-Turn ownership handoff.
//! 首次调度输入批次与精确的已完成回合所有权交接。

use super::SessionDocument;
use crate::v1::errors::DomainError;
use crate::v1::event::{
    ConversationEvent, DisplayNoticeKind, EventBody, QueuedInputResolutionKind, TurnOutcome,
    UserInputEvent, UserInputSource,
};
use crate::v1::ids::{Seq, TurnId};

/// Reads the contiguous initial `turn_id` batch at `start_seq` from canonical `events`.
/// 从规范 `events` 读取位于 `start_seq` 的 `turn_id` 连续初始批次。
/// Returns the original event/input pairs up to the next source boundary, rejecting an invalid root.
/// 返回下一个来源边界之前的原事件与输入对，根输入无效时拒绝。
fn scheduled_input_batch(
    events: &[ConversationEvent],
    turn_id: TurnId,
    start_seq: Seq,
) -> Result<Vec<(&ConversationEvent, &UserInputEvent)>, DomainError> {
    // Resolve the exact first user event of this Turn, not a neighboring source position.
    // 解析本回合精确的首个用户事件，不使用相邻来源位置。
    let start = events
        .iter()
        .position(|event| {
            event.turn_id == Some(turn_id) && matches!(&event.event, EventBody::UserInput(_))
        })
        .ok_or_else(|| DomainError::reference("scheduled_input_batch", start_seq.value()))?;
    if events[start].seq != start_seq {
        return Err(DomainError::InvalidQueuedInputOrder {
            field: "scheduled_batch_start",
        });
    }
    // A batch contains its scheduled root and only the immediately following supplementary inputs.
    // 批次包含调度根输入，以及仅紧随其后的补充输入。
    let mut batch = Vec::new();
    for event in &events[start..] {
        let EventBody::UserInput(input) = &event.event else {
            break;
        };
        if event.turn_id != Some(turn_id) {
            break;
        }
        if batch.is_empty() {
            if input.input_source != UserInputSource::QueuedScheduled {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "scheduled_batch_source",
                });
            }
        } else if input.input_source != UserInputSource::PendingInTurn {
            // A later AgentMessage is independent guidance, not a malformed member of the frozen batch.
            // 后续 AgentMessage 是独立引导，不是冻结批次中格式错误的成员。
            break;
        }
        batch.push((event, input));
    }
    Ok(batch)
}

/// Proves that `candidate` moved from `previous_turn` to its immediately following scheduled batch.
/// 证明 `candidate` 从 `previous_turn` 转入紧随其后的调度批次。
/// Returns false unless canonical closure, root, membership and message order prove the handoff.
/// 仅在规范闭合、根输入、成员和消息顺序均证明交接时返回真。
pub(super) fn input_was_handed_off(
    events: &[ConversationEvent],
    candidate: &ConversationEvent,
    previous_turn: TurnId,
) -> bool {
    // A same-Turn projection is not a handoff and must use the original strict validation.
    // 同回合投影不是交接，必须使用原严格校验。
    let Some(turn_id) = candidate.turn_id.filter(|turn| *turn != previous_turn) else {
        return false;
    };
    // The first user event owns the new Turn's exact scheduled batch boundary.
    // 首个用户事件持有新回合精确的调度批次边界。
    let Some(start) = events.iter().position(|event| {
        event.turn_id == Some(turn_id) && matches!(&event.event, EventBody::UserInput(_))
    }) else {
        return false;
    };
    // Terminal Goal notices and the new Turn's route change may follow predecessor statistics.
    // 终态目标提示与新回合路由变化可以位于前驱统计之后。
    let Some(predecessor) = events[..start].iter().rev().find(|event| {
        !matches!(&event.event, EventBody::RequestConfigChanged(_) if event.turn_id == Some(turn_id))
            && !matches!(&event.event, EventBody::DisplayNote(note)
                if event.turn_id.is_none() && note.notice_kind == DisplayNoticeKind::GoalStatus)
    }) else { return false; };
    if predecessor.turn_id != Some(previous_turn)
        || !matches!(&predecessor.event, EventBody::TurnStats(stats) if stats.outcome == TurnOutcome::Completed)
    {
        return false;
    }
    // An intervening event closes initial batch membership, so later inputs cannot claim this handoff.
    // 中间事件会关闭初始批次成员范围，因此迟到输入不能声称属于本次交接。
    match scheduled_input_batch(events, turn_id, events[start].seq) {
        Ok(batch) => batch.iter().any(|(event, _)| event.seq == candidate.seq),
        Err(_) => false,
    }
}

impl SessionDocument {
    /// Retires proven handoffs whose input events fall beyond `fork_target_seq` in the fork's `retained` prefix.
    /// 在分叉的 `retained` 前缀内退休输入事件位于 `fork_target_seq` 之后的已证实交接。
    /// Uses `now_ms` for child-only audit revisions; returns an error for invalid source history or revision exhaustion.
    /// 使用 `now_ms` 修订仅属于子分支的审计；来源历史无效或修订耗尽时返回错误。
    pub(super) fn retire_input_handoffs_outside_fork(
        &self,
        retained: &mut [ConversationEvent],
        fork_target_seq: Seq,
        now_ms: i64,
    ) -> Result<(), DomainError> {
        // Prove source identities and payloads before allowing any branch-local disposition change.
        // 允许任何分支内处置变更之前，先证明来源身份与载荷。
        super::input::validate_queued_input_resolution_history(&self.metadata, &self.events)?;
        for event in retained {
            let EventBody::QueuedInputResolution(resolution) = &event.event else {
                continue;
            };
            if resolution.resolution != QueuedInputResolutionKind::PromotedToPendingInTurn {
                continue;
            }
            // A resolution can still own pending metadata; only an exact later streamed input proves a clipped handoff.
            // 处置仍可能持有待处理元数据；只有精确的更晚入流输入才能证明被截断的交接。
            let omitted_input = self.events.iter().find(|candidate| {
                candidate.seq > fork_target_seq
                    && matches!(&candidate.event, EventBody::UserInput(input)
                        if input.client_action_id == resolution.client_action_id
                            && input.input_id == resolution.input_id)
            });
            let Some(omitted_input) = omitted_input else {
                continue;
            };
            // The original completed Turn and the actual successor batch must jointly prove transfer.
            // 原已完成回合与实际后继批次必须共同证明转交。
            let previous_turn = event.turn_id.ok_or(DomainError::InvalidQueuedInputOrder {
                field: "promoted_resolution_turn_id",
            })?;
            if !input_was_handed_off(&self.events, omitted_input, previous_turn) {
                return Err(DomainError::InvalidQueuedInputOrder {
                    field: "fork_omitted_input_handoff",
                });
            }
            // Forks exclude future work while retaining each audit identity and payload.
            // 分叉排除未来工作，同时保留每条审计的身份与载荷。
            // Clear its pending owner only in the child; normal history validation remains strict.
            // 仅在子分支清除其待处理所有者，普通历史校验保持严格。
            event.mutate_in_place(now_ms.max(event.updated_at_ms), |body| {
                let EventBody::QueuedInputResolution(resolution) = body else {
                    unreachable!("the fork resolution body was just validated");
                };
                resolution.resolution = QueuedInputResolutionKind::Removed;
            })?;
            event.turn_id = None;
        }
        Ok(())
    }

    /// Reads the complete initial queued batch for `turn_id` and its exact `start_seq`.
    /// 读取 `turn_id` 及其精确 `start_seq` 对应的完整初始排队批次。
    /// Returns original message identities, text and attachments in persisted order, or a domain error.
    /// 按持久顺序返回原消息身份、正文和附件，或返回领域错误。
    pub fn initial_queued_input_batch(
        &self,
        turn_id: TurnId,
        start_seq: Seq,
    ) -> Result<Vec<(&ConversationEvent, &UserInputEvent)>, DomainError> {
        scheduled_input_batch(&self.events, turn_id, start_seq)
    }
}
