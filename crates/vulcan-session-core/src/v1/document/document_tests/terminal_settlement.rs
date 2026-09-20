//! Regression tests for the durable terminal settlement intent lifecycle.
//! 可靠终态结算意图生命周期的回归测试。

use super::*;
use crate::v1::config::{TerminalSettlementRecord, TerminalSettlementState};

/// Builds one settlement intent for the fixture session.
/// 为夹具会话构建一条结算意图。
fn settlement_record(settlement_id: &str, turn_id: TurnId) -> TerminalSettlementRecord {
    TerminalSettlementRecord {
        settlement_id: settlement_id.to_string(),
        session_id: String::from("sess-1"),
        turn_id,
        owner_generation: String::from("stream-task:owner-a"),
        state: TerminalSettlementState::Pending,
        reason_code: String::from("stream_finished_without_v1_api_terminal"),
        updated_at_ms: T0,
    }
}

/// Verifies one exact intent is idempotent while a different intent cannot supersede it.
/// 验证完全相同的意图保持幂等，而不同的意图不能取代未完成结算。
#[test]
fn terminal_settlement_begin_is_idempotent_and_rejects_conflicts() {
    let mut document = new_document();
    let turn_id = document
        .submit_root_input_fixture("prompt", Vec::new(), T0)
        .expect("turn should open");
    let record = settlement_record("settle-a", turn_id);

    let first = document
        .begin_terminal_settlement(record.clone(), T0 + 1)
        .expect("first intent should write");
    assert!(first.changed());
    let repeated = document
        .begin_terminal_settlement(record, T0 + 2)
        .expect("exact intent replay should stay idempotent");
    assert!(!repeated.changed());
    assert_eq!(
        repeated.into_value(),
        TerminalSettlementState::Pending,
        "replayed intent keeps its durable state"
    );

    let conflict =
        document.begin_terminal_settlement(settlement_record("settle-b", turn_id), T0 + 3);
    assert!(matches!(
        conflict,
        Err(DomainError::TerminalSettlementConflict { .. })
    ));

    let mut other_session = TerminalSettlementRecord {
        session_id: String::from("sess-2"),
        ..settlement_record("settle-c", turn_id)
    };
    other_session.session_id = String::from("sess-2");
    assert!(matches!(
        document.begin_terminal_settlement(other_session, T0 + 4),
        Err(DomainError::TerminalSettlementSessionMismatch { .. })
    ));
    assert_eq!(
        document
            .terminal_settlement_record()
            .map(|record| record.settlement_id.as_str()),
        Some("settle-a"),
        "a rejected intent never replaces the durable one"
    );
}

/// Verifies repair marking and resolution are both exact-mutation idempotent.
/// 验证修复标记与结算都是精确变更幂等的。
#[test]
fn terminal_settlement_mark_and_resolve_are_idempotent() {
    let mut document = new_document();
    let turn_id = document
        .submit_root_input_fixture("prompt", Vec::new(), T0)
        .expect("turn should open");
    document
        .begin_terminal_settlement(settlement_record("settle-a", turn_id), T0 + 1)
        .expect("intent should write");

    let marked = document
        .mark_terminal_settlement_repair_required("settle-a", T0 + 2)
        .expect("repair mark should commit");
    assert!(marked.changed());
    assert_eq!(
        document
            .terminal_settlement_record()
            .expect("record retained")
            .state,
        TerminalSettlementState::RepairRequired
    );
    let remarked = document
        .mark_terminal_settlement_repair_required("settle-a", T0 + 3)
        .expect("repair replay should stay idempotent");
    assert!(!remarkded_changed(&remarked));
    assert!(matches!(
        document.mark_terminal_settlement_repair_required("settle-missing", T0 + 4),
        Err(DomainError::TerminalSettlementMissing { .. })
    ));

    let resolved = document
        .resolve_terminal_settlement("settle-a", T0 + 5)
        .expect("resolution should commit");
    assert!(resolved.changed());
    assert!(document.terminal_settlement_record().is_none());
    let replayed = document
        .resolve_terminal_settlement("settle-a", T0 + 6)
        .expect("resolution replay should stay idempotent");
    assert!(!remarkded_changed(&replayed));
    // An absent record means nothing is left to settle; resolution never guesses a success it cannot prove.
    // 记录缺失表示没有待结算项；结算绝不猜测一个无法证明的成功。
    let absent = document
        .resolve_terminal_settlement("settle-missing", T0 + 7)
        .expect("resolving an absent settlement is an explicit no-op");
    assert!(!remarkded_changed(&absent));
}

/// Verifies the terminal statistic event is the only closure proof for one exact turn.
/// 验证终态统计事件是一条精确回合唯一的闭合证明。
#[test]
fn turn_has_terminal_fact_tracks_turn_statistics() {
    let mut document = new_document();
    let turn_id = document
        .submit_root_input_fixture("prompt", Vec::new(), T0)
        .expect("turn should open");

    assert!(!document.turn_has_terminal_fact(turn_id));

    document
        .close_turn(TurnOutcome::Stopped, 8, T0 + 1)
        .expect("turn should close");
    assert!(document.turn_has_terminal_fact(turn_id));
    assert!(
        !document.turn_has_terminal_fact(
            TurnId::try_new(turn_id.value() + 1).expect("successor turn id")
        )
    );
}

/// Verifies the settlement intent survives one metadata serialization round trip.
/// 验证结算意图在元数据序列化往返后保持不变。
#[test]
fn terminal_settlement_survives_metadata_round_trip() {
    let mut document = new_document();
    let turn_id = document
        .submit_root_input_fixture("prompt", Vec::new(), T0)
        .expect("turn should open");
    document
        .begin_terminal_settlement(settlement_record("settle-a", turn_id), T0 + 1)
        .expect("intent should write");

    let json = serde_json::to_string(&document.metadata).expect("metadata should serialize");
    let decoded: MetadataDocument = serde_json::from_str(&json).expect("metadata should decode");

    assert_eq!(
        decoded.input_state.terminal_settlement,
        document.terminal_settlement_record().cloned()
    );
}

/// Returns whether one idempotent settlement mutation committed a new revision.
/// 返回一次幂等结算变更是否提交了新修订。
fn remarkded_changed<T>(mutation: &IdempotentMutation<T>) -> bool {
    mutation.changed()
}
