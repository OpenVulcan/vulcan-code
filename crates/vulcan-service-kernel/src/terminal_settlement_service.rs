//! Durable terminal settlement recovery for one exact session.
//! 单个精确会话的可靠终态结算恢复。
//!
//! Once the settlement intent is durable in the authoritative V1 document,
//! a failed fallback write or process restart can rebuild the pending owner
//! and retry the exact turn idempotently. Failures before intent persistence
//! retain only a process-local repair owner and require their own recovery path.
//! 结算意图写入权威 V1 文档后，回退写入失败或进程重启都能重建待处理所有者，
//! 并幂等重试精确回合。意图持久化前的失败仅保留进程内修复所有者，须由独立恢复路径处理。

use std::sync::Arc;

use vulcan_session_core::v1::TerminalSettlementRecord;
use vulcan_session_core::v1::TerminalSettlementState;

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
};
use crate::state::{
    SessionTerminalPersistenceOwner, SessionTerminalPersistenceOwnerGuard,
    SharedServiceBridgeState, TerminalPreIntentRepair,
};
use crate::{SessionId, current_v1_unix_millis, webui_trace};

/// One durable settlement-intent transition committed through the exact session document.
/// 通过精确会话文档提交的一次可靠结算意图转换。
pub(crate) enum TerminalSettlementIntentTransition {
    /// Writes or re-confirms the recoverable intent before the dangerous terminal phase.
    /// 在危险的终态阶段前写入或再次确认可恢复意图。
    Begin {
        /// Exact intent record to persist.
        /// 要持久化的精确意图记录。
        record: TerminalSettlementRecord,
    },
    /// Marks one exact intent as requiring a controlled retry.
    /// 把一条精确意图标记为需要受控重试。
    MarkRepairRequired {
        /// Exact settlement identity.
        /// 精确结算身份。
        settlement_id: String,
    },
    /// Resolves one exact intent after its durable terminal fact is confirmed.
    /// 在可靠终态事实确认后结算一条精确意图。
    Resolve {
        /// Exact settlement identity.
        /// 精确结算身份。
        settlement_id: String,
    },
}

/// Builds the stable diagnostic settlement identity for one exact owner generation.
/// 为一个精确所有者代际构建稳定的诊断结算身份。
pub(crate) fn terminal_settlement_id(session_id: SessionId, owner_generation: &str) -> String {
    format!("terminal-settlement:{session_id}:{owner_generation}")
}

/// Retains the exact retry payload before any fallible intent preparation begins.
/// 在可能失败的意图准备开始前保留精确重试载荷。
pub(crate) fn retain_terminal_pre_intent_repair(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    repair: TerminalPreIntentRepair,
) -> Result<(), String> {
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock terminal owner state: {error}"))?;
    let owner = guard
        .session_terminal_persistence_owners
        .get(&session_id)
        .ok_or_else(|| format!("terminal owner is absent for session {session_id}"))?;
    if owner.owner_generation() != repair.owner_generation {
        return Err(format!(
            "terminal owner changed before pre-intent repair: session={session_id}"
        ));
    }
    guard.terminal_pre_intent_repairs.insert(session_id, repair);
    Ok(())
}

/// Clears a pre-intent retry only for the same owner generation after a safe boundary.
/// 仅在到达安全边界后清除同一所有者代际的意图前重试。
pub(crate) fn clear_terminal_pre_intent_repair(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    owner_generation: &str,
) {
    let Ok(mut guard) = state.lock() else {
        webui_trace(
            "terminal_pre_intent_repair_clear_state_unavailable",
            format!("session={session_id}"),
        );
        return;
    };
    if guard
        .terminal_pre_intent_repairs
        .get(&session_id)
        .is_some_and(|repair| repair.owner_generation == owner_generation)
    {
        guard.terminal_pre_intent_repairs.remove(&session_id);
    }
}

/// Commits one settlement-intent transition durably and idempotently.
/// 可靠且幂等地提交一次结算意图转换。
///
/// # Parameters
/// 参数
/// `handle` is the exact registered V1 session owner and `transition` selects the intent mutation.
/// `handle` 是精确已注册的 V1 会话 owner；`transition` 选择意图变更。
///
/// # Returns
/// 返回值
/// Success after the document accepted the transition, or an explicit storage/domain error that
/// leaves the previous intent unchanged.
/// 文档接纳该转换后返回成功；否则返回明确的存储或领域错误，并保持原意图不变。
pub(crate) async fn persist_terminal_settlement_intent(
    handle: &Arc<SessionHandle>,
    transition: TerminalSettlementIntentTransition,
) -> Result<(), String> {
    let settlement_id = match &transition {
        TerminalSettlementIntentTransition::Begin { record } => record.settlement_id.clone(),
        TerminalSettlementIntentTransition::MarkRepairRequired { settlement_id }
        | TerminalSettlementIntentTransition::Resolve { settlement_id } => settlement_id.clone(),
    };
    // transition_now_ms is the single settlement timestamp shared by every intent mutation in this request.
    // transition_now_ms 是本次请求全部意图变更共享的唯一结算时间戳。
    let transition_now_ms = current_v1_unix_millis()?;
    handle
        .clone()
        .mutate_durable_if_changed_async(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                let mutation = match &transition {
                    TerminalSettlementIntentTransition::Begin { record } => document
                        .begin_terminal_settlement(record.clone(), record.updated_at_ms)
                        .map(|mutation| match mutation {
                            vulcan_session_core::v1::IdempotentMutation::Committed(_) => {
                                vulcan_session_core::v1::IdempotentMutation::Committed(())
                            }
                            vulcan_session_core::v1::IdempotentMutation::Idempotent(_) => {
                                vulcan_session_core::v1::IdempotentMutation::Idempotent(())
                            }
                        }),
                    TerminalSettlementIntentTransition::MarkRepairRequired { settlement_id } => {
                        document.mark_terminal_settlement_repair_required(
                            settlement_id.as_str(),
                            transition_now_ms,
                        )
                    }
                    TerminalSettlementIntentTransition::Resolve { settlement_id } => document
                        .resolve_terminal_settlement(settlement_id.as_str(), transition_now_ms),
                }?;
                Ok(match mutation {
                    vulcan_session_core::v1::IdempotentMutation::Committed(()) => {
                        SessionMutationChange::Changed(())
                    }
                    vulcan_session_core::v1::IdempotentMutation::Idempotent(()) => {
                        SessionMutationChange::Unchanged(())
                    }
                })
            },
        )
        .await
        .map(|_| ())
        .map_err(|error| {
            format!(
                "failed to persist terminal settlement `{settlement_id}` intent transition: {error}"
            )
        })
}

/// Rebuilds the in-process terminal-persistence owner from the durable settlement intent.
/// 从可靠结算意图重建进程内终态持久化所有者。
///
/// Called whenever the exact session handle is loaded, so a restart re-arms the admission block
/// from the document instead of guessing that an unfinished settlement succeeded.
/// 在加载精确会话句柄时调用，使重启后从文档重新武装准入阻塞，而不是猜测未完成的结算已成功。
///
/// # Parameters
/// 参数
/// `state` owns the owner table, `session_id` selects the session, and `handle` reads the durable intent.
/// `state` 持有所有者表；`session_id` 选择会话；`handle` 读取可靠意图。
pub(crate) fn rebuild_pending_terminal_settlement_owner(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    handle: &SessionHandle,
) {
    let record = match handle.read(|document| document.terminal_settlement_record().cloned()) {
        Ok(record) => record,
        Err(error) => {
            // An unreadable durable boundary is never treated as settled; the session stays closed.
            // 无法读取的可靠边界绝不被当作已结算；会话保持关闭。
            install_unreadable_settlement_owner(state, session_id, &error.to_string());
            return;
        }
    };
    let Some(record) = record else {
        return;
    };
    let recovered = SessionTerminalPersistenceOwner::RecoveredSettlement {
        generation: record.owner_generation.clone(),
    };
    let mut guard = match state.lock() {
        Ok(guard) => guard,
        Err(error) => {
            webui_trace(
                "terminal_settlement_owner_rebuild_state_unavailable",
                format!("session={session_id} error={error}"),
            );
            return;
        }
    };
    if guard
        .terminal_pre_intent_repairs
        .get(&session_id)
        .is_some_and(|repair| repair.owner_generation == record.owner_generation)
    {
        guard.terminal_pre_intent_repairs.remove(&session_id);
    }
    match guard.session_terminal_persistence_owners.get(&session_id) {
        Some(existing) if existing.owner_generation() == record.owner_generation => {
            // The exact original owner is still live in this process; nothing is rebuilt.
            // 精确原始所有者在本进程内仍然存活；无需重建。
        }
        Some(existing) => {
            // A newer live owner legitimately keeps admission closed and is never replaced.
            // 更新的存活所有者合法保持准入关闭，且绝不被替换。
            webui_trace(
                "terminal_settlement_owner_rebuild_superseded",
                format!(
                    "session={session_id} settlement={} owner={existing:?}",
                    record.settlement_id
                ),
            );
        }
        None => {
            guard
                .session_terminal_persistence_owners
                .insert(session_id, recovered);
            webui_trace(
                "terminal_settlement_owner_rebuilt",
                format!(
                    "session={session_id} settlement={} state={}",
                    record.settlement_id,
                    terminal_settlement_state_label(record.state)
                ),
            );
        }
    }
}

/// Installs one fail-closed owner for a session whose durable boundary could not be read.
/// 为可靠边界无法读取的会话安装一个失败关闭的所有者。
fn install_unreadable_settlement_owner(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    error: &str,
) {
    webui_trace(
        "terminal_settlement_document_unreadable",
        format!("session={session_id} error={error}"),
    );
    let mut guard = match state.lock() {
        Ok(guard) => guard,
        Err(lock_error) => {
            webui_trace(
                "terminal_settlement_owner_rebuild_state_unavailable",
                format!("session={session_id} error={lock_error}"),
            );
            return;
        }
    };
    if !guard
        .session_terminal_persistence_owners
        .contains_key(&session_id)
    {
        guard.session_terminal_persistence_owners.insert(
            session_id,
            SessionTerminalPersistenceOwner::RecoveredSettlement {
                generation: format!("unreadable:{session_id}"),
            },
        );
    }
}

/// Result of one controlled terminal-settlement retry.
/// 一次受控终态结算重试的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalSettlementRetryOutcome {
    /// No durable settlement intent exists for the exact session.
    /// 该精确会话不存在可靠结算意图。
    NoPendingSettlement,
    /// The durable terminal fact already existed; the settlement resolved without a second write.
    /// 可靠终态事实已经存在；结算在无第二次写入的情况下完成。
    AlreadySettled,
    /// The retry persisted the terminal boundary idempotently and resolved the settlement.
    /// 重试幂等地持久化终态边界并完成结算。
    SettledByRetry,
}

/// Runs one controlled, idempotent terminal-settlement retry for the exact session.
/// 为精确会话执行一次受控幂等的终态结算重试。
///
/// The retry loads the exact handle (re-arming the in-process owner from the durable intent) and
/// settles the recorded turn exactly once.
/// 重试加载精确句柄（从可靠意图重新武装进程内所有者），并把记录的回合恰好结算一次。
///
/// # Parameters
/// 参数
/// `state` owns the session registry and owner table; `session_id` selects the exact session.
/// `state` 持有会话注册表与所有者表；`session_id` 选择精确会话。
///
/// # Returns
/// 返回值
/// The retry outcome, or an explicit storage error that leaves the session closed without guessing.
/// 返回重试结果；返回明确存储错误且不猜测，会话保持关闭。
pub(crate) async fn retry_session_terminal_settlement(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<TerminalSettlementRetryOutcome, String> {
    // Loading the handle also rebuilds the in-process owner from the durable intent.
    // 加载句柄的同时会根据可靠意图重建进程内所有者。
    let handle =
        crate::session_goal_lifecycle::session_v1_handle_from_state_with_settlement_rebuild(
            state, session_id,
        )
        .await?;
    let outcome = settle_pending_terminal_settlement(state, &handle, session_id).await?;
    if outcome != TerminalSettlementRetryOutcome::NoPendingSettlement {
        return Ok(outcome);
    }
    retry_pre_intent_terminal_settlement(state, session_id).await
}

/// Retries one retained pre-intent owner after the exact V1 handle is available again.
/// 在精确 V1 句柄重新可用后重试一个保留的意图前所有者。
///
/// # Parameters
/// 参数
/// `state` owns the exact process-local owner and `session_id` selects its session.
/// `state` 持有精确的进程内所有者，`session_id` 选择其会话。
///
/// # Returns
/// 返回值
/// The settled outcome, no pending work, or a failure that keeps the owner closed.
/// 返回已结算结果、无待处理工作，或保持所有者关闭的失败。
pub(crate) async fn retry_pre_intent_terminal_settlement(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<TerminalSettlementRetryOutcome, String> {
    // A stream can fail before its durable intent exists; only its still-current exact owner may retry.
    // 流可能在可靠意图产生前失败；只有仍属当前的精确所有者能够重试。
    let pending = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock terminal owner state: {error}"))?;
        guard
            .terminal_pre_intent_repairs
            .get(&session_id)
            .cloned()
            .and_then(|repair| {
                guard
                    .session_terminal_persistence_owners
                    .get(&session_id)
                    .filter(|owner| owner.owner_generation() == repair.owner_generation)
                    .cloned()
                    .map(|owner| (owner, repair))
            })
    };
    let Some((owner, repair)) = pending else {
        return Ok(TerminalSettlementRetryOutcome::NoPendingSettlement);
    };
    let owner_guard =
        SessionTerminalPersistenceOwnerGuard::adopt_registered(state.clone(), session_id, owner)?;
    if crate::settle_stream_terminal_authority(
        state,
        session_id,
        false,
        repair.reason_code,
        repair.redacted_detail,
        owner_guard,
    )
    .await
    {
        Ok(TerminalSettlementRetryOutcome::SettledByRetry)
    } else {
        Err(format!(
            "pre-intent terminal settlement retry failed for session {session_id}"
        ))
    }
}

/// Settles one exact durable terminal settlement against its authoritative document.
/// 针对其权威文档结算一条精确的可靠终态结算。
///
/// The retry reuses an already-durable terminal fact when the earlier write landed, persists the
/// same turn idempotently otherwise, resolves the intent, and releases only an owner whose
/// generation matches the recorded boundary.
/// 若早先写入已落场则复用既有可靠终态事实，否则按同一回合幂等落盘，随后结算意图，
/// 并且只释放代际与所记录边界匹配的所有者。
///
/// # Parameters
/// 参数
/// `state` owns the owner table, `handle` is the exact registered V1 session owner, and
/// `session_id` selects the session.
/// `state` 持有所有者表；`handle` 是精确已注册的 V1 会话 owner；`session_id` 选择会话。
///
/// # Returns
/// 返回值
/// The retry outcome, or an explicit storage/domain error that never guesses success.
/// 返回重试结果；返回明确的存储或领域错误，绝不猜测成功。
pub(crate) async fn settle_pending_terminal_settlement(
    state: &SharedServiceBridgeState,
    handle: &Arc<SessionHandle>,
    session_id: SessionId,
) -> Result<TerminalSettlementRetryOutcome, String> {
    let record = handle
        .read(|document| document.terminal_settlement_record().cloned())
        .map_err(|error| format!("failed to read durable terminal settlement: {error}"))?;
    let Some(record) = record else {
        return Ok(TerminalSettlementRetryOutcome::NoPendingSettlement);
    };
    // terminal_fact_durable is honest about an earlier write that failed at the store: the in-memory
    // document may already hold the terminal fact while its flush was rejected.
    // terminal_fact_durable 对早先在存储端失败的写入保持诚实：内存文档可能已持有终态事实，
    // 但其 flush 曾被拒绝。
    // active_turn_id is captured with the terminal fact so recovery can reject a different turn.
    // active_turn_id 与终态事实一并读取，使恢复能够拒绝另一条活动回合。
    // terminal_fact_in_memory permits flushing the same dirty fact after a failed store write.
    // terminal_fact_in_memory 允许在存储写入失败后重刷同一条内存脏终态事实。
    let (terminal_fact_durable, terminal_fact_in_memory, active_turn_id) = handle
        .read(|document| {
            let terminal_fact_in_memory = document.turn_has_terminal_fact(record.turn_id);
            (
                terminal_fact_in_memory
                    && document.memory_revision() == document.durable_revision(),
                terminal_fact_in_memory,
                document.active_turn().map(|turn| turn.turn_id),
            )
        })
        .map_err(|error| format!("failed to read durable terminal fact: {error}"))?;
    if !terminal_fact_durable {
        // The boundary writer closes the document's currently active turn. It is only safe when
        // that exact turn is the one named by the durable settlement intent.
        // 边界写入器闭合文档当前活动回合；只有它与可靠结算意图指定的回合完全一致才可安全调用。
        if active_turn_id != Some(record.turn_id)
            && !(active_turn_id.is_none() && terminal_fact_in_memory)
        {
            return Err(format!(
                "terminal settlement `{}` cannot retry: recorded turn {} differs from active turn {:?}",
                record.settlement_id,
                record.turn_id.value(),
                active_turn_id.map(|turn_id| turn_id.value())
            ));
        }
        // The same turn id and reason code make the retry settle exactly once without a second fact.
        // 同一回合 ID 与原因码使重试恰好结算一次，不产生第二条事实。
        crate::agent_stream::persist_v1_execution_failure_boundary_for_turn(
            state,
            session_id,
            record.reason_code.as_str(),
            None,
            Some(record.turn_id),
        )
        .await?;
    }
    persist_terminal_settlement_intent(
        handle,
        TerminalSettlementIntentTransition::Resolve {
            settlement_id: record.settlement_id.clone(),
        },
    )
    .await?;
    release_recorded_settlement_owner(state, session_id, &record);
    Ok(if terminal_fact_durable {
        TerminalSettlementRetryOutcome::AlreadySettled
    } else {
        TerminalSettlementRetryOutcome::SettledByRetry
    })
}

/// Releases one in-process owner only when its generation matches the recorded boundary.
/// 仅当进程内所有者的代际与所记录边界匹配时才释放它。
///
/// A newer owner legitimately keeps the session closed; an older owner may never release it.
/// 更新的所有者合法保持会话关闭；旧所有者不得释放新所有者。
fn release_recorded_settlement_owner(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    record: &TerminalSettlementRecord,
) {
    let mut guard = match state.lock() {
        Ok(guard) => guard,
        Err(error) => {
            webui_trace(
                "terminal_settlement_owner_release_state_unavailable",
                format!("session={session_id} error={error}"),
            );
            return;
        }
    };
    if guard
        .terminal_pre_intent_repairs
        .get(&session_id)
        .is_some_and(|repair| repair.owner_generation == record.owner_generation)
    {
        guard.terminal_pre_intent_repairs.remove(&session_id);
    }
    match guard.session_terminal_persistence_owners.get(&session_id) {
        Some(owner) if owner.owner_generation() == record.owner_generation => {
            guard
                .session_terminal_persistence_owners
                .remove(&session_id);
            webui_trace(
                "terminal_settlement_owner_released",
                format!("session={session_id} settlement={}", record.settlement_id),
            );
        }
        Some(owner) => {
            webui_trace(
                "terminal_settlement_owner_release_superseded",
                format!(
                    "session={session_id} settlement={} owner={owner:?}",
                    record.settlement_id
                ),
            );
        }
        None => {
            webui_trace(
                "terminal_settlement_owner_release_absent",
                format!("session={session_id} settlement={}", record.settlement_id),
            );
        }
    }
}

/// Ensures no durable terminal settlement still blocks one exact session before admission.
/// 在准入前确保没有可靠终态结算仍然阻塞该精确会话。
///
/// One controlled idempotent retry runs first, so a recovered store can complete a pending
/// settlement; an unsettled boundary stays explicitly blocked with a stable diagnostic identity.
/// 先执行一次受控幂等重试，使恢复后的存储能够完成待结算项；未结算边界保持显式阻塞，
/// 并附带稳定的诊断身份。
pub(crate) async fn ensure_session_terminal_settlement_settled(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<(), String> {
    match retry_session_terminal_settlement(state, session_id).await {
        Ok(outcome) => {
            webui_trace(
                "terminal_settlement_admission_check",
                format!("session={session_id} outcome={outcome:?}"),
            );
            Ok(())
        }
        Err(error) => Err(format!(
            "ERR_TERMINAL_SETTLEMENT_REPAIR_REQUIRED: session={session_id} error={error}"
        )),
    }
}

/// Reports the stable settlement state label used by diagnostics and admission errors.
/// 返回诊断与准入错误使用的稳定结算状态标签。
const fn terminal_settlement_state_label(state: TerminalSettlementState) -> &'static str {
    match state {
        TerminalSettlementState::Pending => "settlement_pending",
        TerminalSettlementState::RepairRequired => "repair_required",
    }
}
