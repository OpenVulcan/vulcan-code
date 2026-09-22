//! Durable terminal settlement recovery regressions.
//! 可靠终态结算恢复回归测试。

use super::test_support::TestHostRuntimeRoot;
use super::*;
use crate::session_runtime_registry_v1::SessionHandle;
use crate::state::{
    SessionTerminalPersistenceOwner, SessionTerminalPersistenceOwnerGuard,
    register_session_terminal_persistence_owner,
};
use crate::terminal_settlement_service::{
    TerminalSettlementIntentTransition, TerminalSettlementRetryOutcome,
    persist_terminal_settlement_intent, rebuild_pending_terminal_settlement_owner,
    retry_pre_intent_terminal_settlement, settle_pending_terminal_settlement,
};
use vulcan_session_core::v1::{
    ClientActionId, ClientInputSubmission, EventBody, SessionDocument, TerminalSettlementRecord,
    TerminalSettlementState, TurnId,
};
use vulcan_workspace_store::workspace_store::load_session_v1_full;

/// Failed auxiliary writes recover through the existing owner gate without closing the parent turn.
/// 辅助写入失败通过既有所有者门禁恢复，不关闭父回合。
#[tokio::test]
async fn question_terminal_settlement_retries_exact_scope_without_business_completion() {
    use vulcan_session_core::v1::TerminalSettlementScope;
    use vulcan_session_core::v1::collaboration::*;
    for in_parent in [false, true] {
        // Each target has its own canonical UUID and scripted persistence owner.
        // 每个目标拥有独立规范 UUID 及脚本化持久化所有者。
        let session_id = SessionId::new_v4();
        // Assemble the accepted durable execution before installing failure injection.
        // 在安装失败注入前装配已接纳可靠执行。
        let mut document = SessionDocument::new(
            session_id.to_string(),
            "project-a",
            "Auxiliary recovery",
            1_700_000_000_000,
        );
        // Only the retained-parent case admits a business root input.
        // 只有保留父回合场景才准入业务根输入。
        let parent_turn_id = in_parent.then(|| {
            document
                .submit_root_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("qa-parent-root").unwrap(),
                        "Original task.",
                        vec![],
                    ),
                    1_700_000_000_001,
                )
                .unwrap()
                .into_value()
        });
        if in_parent {
            document
                .set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("qa-main-only").unwrap(),
                    state: CollaborationRunControl::MainOnly,
                    changed_at_ms: 1_700_000_000_002,
                })
                .unwrap();
        }
        // Question scope comes from target acceptance and preserves the original parent identity.
        // 问题范围来自目标接纳，并保留原父身份。
        let purpose = if in_parent {
            CollaborationExecutionPurpose::InTurnQuestion
        } else {
            CollaborationExecutionPurpose::NaturallyIdleQuestion
        };
        // A delivered source fact is independent of the target's physical execution lifecycle.
        // 已投递来源事实独立于目标物理执行生命周期。
        let source = CollaborationParticipant::new(
            "source-session",
            "source-session",
            "project-a",
            1,
            Some(TurnId::try_new(1).unwrap()),
            None,
        )
        .unwrap();
        // The target is bound to its real session and optional retained turn.
        // 目标绑定真实会话及可选保留回合。
        let target = CollaborationParticipant::new(
            session_id.to_string(),
            session_id.to_string(),
            "project-a",
            1,
            parent_turn_id,
            None,
        )
        .unwrap();
        // Exact question identity is shared by acceptance and execution start.
        // 精确问题身份由接纳与执行启动共享。
        let question_id = CollaborationQuestionId::try_new("qa-settlement-question").unwrap();
        document
            .accept_delivery(AcceptDeliveryInput {
                operation_id: CollaborationOperationId::try_new("qa-accept").unwrap(),
                delivery_id: CollaborationDeliveryId::try_new("qa-delivery").unwrap(),
                question_id: question_id.clone(),
                source,
                target: target.clone(),
                bound_turn_id: parent_turn_id,
                execution_purpose: purpose,
                accepted_at_ms: 1_700_000_000_003,
            })
            .unwrap();
        // The execution identity remains authoritative after the original in-memory owner is removed.
        // 原内存所有者被移除后，执行身份仍是权威。
        let execution_id = CollaborationExecutionId::try_new("qa-settlement-execution").unwrap();
        document
            .begin_question_execution(BeginQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new("qa-begin").unwrap(),
                execution_id: execution_id.clone(),
                question_id,
                participant: target,
                purpose,
                parent_turn_id,
                started_at_ms: 1_700_000_000_004,
            })
            .unwrap();
        // Scope is persisted before both fallback attempts and the repair-mark write fail.
        // 两次回退尝试及修复标记写入失败前先持久化范围。
        let scope = TerminalSettlementScope::QuestionExecution {
            execution_id: execution_id.clone(),
            api_call_id: None,
        };
        // Preserve business controls to prove recovery cannot resume MainOnly or change Goal state.
        // 保留业务控制，证明恢复不能解除 MainOnly 或改变 Goal 状态。
        let expected_control = document.collaboration_run_control();
        // Registry state owns the same canonical handle used by production terminal repair.
        // 注册表状态持有与生产终态修复相同的规范句柄。
        let mut service_state = test_service_state_with_parent_session(session_id);
        // Fail only the dangerous boundary and repair writes; the later controlled retry can succeed.
        // 只让危险边界及修复写入失败，后续受控重试可以成功。
        let handle = register_test_session_v1_document_with_scripted_flush_failures(
            &mut service_state,
            session_id,
            document,
            [2, 3, 4].into_iter().collect(),
        );
        // Exact terminal ownership must survive the failed storage attempts.
        // 精确终态所有权必须跨越存储失败尝试保留。
        let owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
        register_session_terminal_persistence_owner(&mut service_state, session_id, owner.clone())
            .unwrap();
        // The shared owner table is deliberately cleared later to exercise reconstruction.
        // 后续刻意清空共享所有者表以验证重建。
        let state = Arc::new(Mutex::new(service_state));
        // Guard is consumed by the real terminal orchestration, not bypassed by direct Core calls.
        // 守卫由真实终态编排消费，不通过直接 Core 调用绕过。
        let guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
            state.clone(),
            session_id,
            owner.clone(),
        )
        .unwrap();
        assert!(
            !crate::settle_stream_terminal_authority_with_scope(
                &state,
                session_id,
                false,
                "qa_stream_failed",
                Some("Controlled failure.".into()),
                guard,
                scope.clone()
            )
            .await
        );
        handle
            .read(|document| {
                assert_eq!(document.terminal_settlement_record().unwrap().scope, scope);
                assert!(document.memory_revision() > document.durable_revision());
                assert_eq!(document.active_turn().unwrap().closed, !in_parent);
            })
            .unwrap();
        state
            .lock()
            .unwrap()
            .session_terminal_persistence_owners
            .clear();
        rebuild_pending_terminal_settlement_owner(&state, session_id, &handle);
        assert_eq!(
            settle_pending_terminal_settlement(&state, &handle, session_id)
                .await
                .unwrap(),
            TerminalSettlementRetryOutcome::SettledByRetry
        );
        assert!(
            !state
                .lock()
                .unwrap()
                .session_terminal_persistence_owners
                .contains_key(&session_id)
        );
        handle.read(|document| {
            assert!(document.terminal_settlement_record().is_none());
            assert_eq!(document.memory_revision(), document.durable_revision());
            assert_eq!(document.collaboration_run_control(), expected_control);
            assert_eq!(document.active_turn().unwrap().closed, !in_parent);
            assert_eq!(document.collaboration_state().question_executions[&execution_id].outcome, Some(CollaborationQuestionExecutionOutcome::Failed));
            assert_eq!(document.events().iter().filter(|event| matches!(&event.event, EventBody::Collaboration(value) if value.event_kind == CollaborationEventKind::QuestionExecutionCompleted)).count(), 1);
        }).unwrap();
    }
}

/// Builds one service state whose exact session owns one open V1 turn with a scripted store.
/// 构建一个服务状态，其精确会话持有一条开放 V1 回合，并挂接脚本化存储。
///
/// Flush attempt 1 is the settlement intent; attempts 2 and 3 are the dangerous fallback terminal
/// write and its retry. Callers extend the failed set to model a store that also rejects the repair
/// mark (4) or the later controlled retry (5 and 6).
/// 刷新尝试 1 是结算意图；尝试 2 与 3 是危险的回退终态写入及其重试。调用方可扩展失败集合，
/// 建模同样拒绝修复标记（4）或后续受控重试（5 与 6）的存储。
fn scripted_settlement_fixture(
    session_id: SessionId,
    extra_failed_attempts: impl IntoIterator<Item = usize>,
) -> (ServiceBridgeState, Arc<SessionHandle>) {
    let mut document = SessionDocument::new(
        session_id.to_string(),
        "project-a",
        "Terminal settlement",
        1_700_000_000_000,
    );
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("settlement-root").expect("valid client action id"),
                "run provider request",
                Vec::new(),
            ),
            1_700_000_000_001,
        )
        .expect("root input should open one V1 turn");
    let mut service_state = test_service_state_with_parent_session(session_id);
    let failed_attempts = [2_usize, 3]
        .into_iter()
        .chain(extra_failed_attempts)
        .collect();
    let handle = register_test_session_v1_document_with_scripted_flush_failures(
        &mut service_state,
        session_id,
        document,
        failed_attempts,
    );
    (service_state, handle)
}

/// Runs one failed terminal settlement for the exact fixture session.
/// 为精确夹具会话执行一次失败的终态结算。
async fn settle_failed_terminal(
    session_id: SessionId,
    service_state: ServiceBridgeState,
    owner: SessionTerminalPersistenceOwner,
) -> (SharedServiceBridgeState, SessionTerminalPersistenceOwner) {
    let mut service_state = service_state;
    register_session_terminal_persistence_owner(&mut service_state, session_id, owner.clone())
        .expect("terminal owner should register");
    let state = Arc::new(Mutex::new(service_state));
    let guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        owner.clone(),
    )
    .expect("terminal owner should be adoptable");
    assert!(
        !crate::settle_stream_terminal_authority(
            &state,
            session_id,
            false,
            "stream_finished_without_v1_api_terminal",
            None,
            guard,
        )
        .await
    );
    (state, owner)
}

/// Verifies a failed fallback terminal write keeps its owner and records a repair-required intent.
/// 验证失败的回退终态写入会保留其 owner，并记录“需要修复”的可靠意图。
#[tokio::test]
async fn terminal_settlement_failure_marks_repair_and_keeps_owner() {
    // session_id identifies the exact stream whose fallback terminal write fails.
    // session_id 标识回退终态写入失败的精确流。
    let session_id = SessionId::new_v4();
    let (service_state, handle) = scripted_settlement_fixture(session_id, []);
    let owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    let (state, owner) = settle_failed_terminal(session_id, service_state, owner).await;

    {
        let mut guard = state.lock().expect("inspect retained owner");
        assert_eq!(
            guard.session_terminal_persistence_owners.get(&session_id),
            Some(&owner),
            "an unsettled terminal boundary keeps its exact owner"
        );
        assert!(
            start_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                "blocked-replacement-turn",
            )
            .is_err(),
            "new input must be explicitly blocked with a terminal diagnostic"
        );
    }

    // The durable document itself now carries the recoverable repair-required intent.
    // 可靠文档自身现在携带可恢复的“需要修复”意图。
    let record = handle
        .read(|document| document.terminal_settlement_record().cloned())
        .expect("settlement intent should remain readable")
        .expect("settlement intent should exist");
    assert_eq!(record.state, TerminalSettlementState::RepairRequired);
    assert_eq!(
        record.reason_code,
        "stream_finished_without_v1_api_terminal"
    );
    assert!(record.settlement_id.contains("terminal-settlement:"));
    assert_eq!(record.turn_id, {
        handle
            .read(|document| {
                document
                    .active_turn()
                    .map(|turn| turn.turn_id)
                    .ok_or_else(|| String::from("open turn should remain readable"))
            })
            .expect("turn lookup should remain readable")
            .expect("open turn should remain open")
    });
}

/// Verifies a controlled retry while the store is still broken fails explicitly and keeps the owner.
/// 验证存储仍损坏时的受控重试显式失败并保留 owner。
#[tokio::test]
async fn unrecovered_storage_keeps_settlement_and_reports_explicit_error() {
    let session_id = SessionId::new_v4();
    // Flush attempts 4, 5, and 6 fail too, so neither the repair mark nor the retry can persist.
    // 刷新尝试 4、5 与 6 同样失败，因此修复标记与重试都无法落盘。
    let (service_state, handle) = scripted_settlement_fixture(session_id, [4, 5, 6]);
    let owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    let (state, owner) = settle_failed_terminal(session_id, service_state, owner).await;

    // The intent remains present in the document; its repair mark could not flush, so a restart
    // would still rebuild the pending settlement from the durable Pending intent.
    // 意图仍存在于文档中；其修复标记无法 flush，因此重启后仍会从可靠的 Pending 意图重建待结算项。
    let record = handle
        .read(|document| document.terminal_settlement_record().cloned())
        .expect("settlement intent should remain readable")
        .expect("settlement intent should exist");
    assert!(record.settlement_id.contains("terminal-settlement:"));

    let retry_error = settle_pending_terminal_settlement(&state, &handle, session_id)
        .await
        .expect_err("storage failure must stay explicit");
    assert!(retry_error.contains("injected storage failure"));
    assert_eq!(
        state
            .lock()
            .expect("inspect owner after failed retry")
            .session_terminal_persistence_owners
            .get(&session_id),
        Some(&owner),
        "a failed retry must not release the exact owner"
    );
    let mut guard = state.lock().expect("inspect blocked admission");
    assert!(
        start_webui_session_kernel_user_turn(&mut guard, session_id, "blocked-replacement-turn")
            .is_err()
    );
}

/// Verifies a restart rebuilds the admission owner from the durable intent and one retry settles it.
/// 验证重启会从可靠意图重建准入 owner，并由一次重试完成结算。
#[tokio::test]
async fn recovered_settlement_blocks_admission_until_idempotent_retry() {
    let session_id = SessionId::new_v4();
    // The repair mark also fails, so the terminal fact stays non-durable until the retry publishes it.
    // 修复标记同样失败，因此终态事实在重试发布前保持非可靠状态。
    let (service_state, handle) = scripted_settlement_fixture(session_id, [4]);
    let owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    let owner_generation = owner.owner_generation();
    let (state, _owner) = settle_failed_terminal(session_id, service_state, owner).await;
    let settlement_turn_id = handle
        .read(|document| {
            document
                .terminal_settlement_record()
                .map(|record| record.turn_id)
        })
        .expect("settlement record should remain readable")
        .expect("settlement record should exist");

    let (fact_in_memory, active_turn_id, turn_closed, memory_revision, durable_revision) = handle
        .read(|document| {
            (
                document.turn_has_terminal_fact(settlement_turn_id),
                document.active_turn().map(|turn| turn.turn_id),
                document.active_turn().is_some_and(|turn| turn.closed),
                document.memory_revision(),
                document.durable_revision(),
            )
        })
        .expect("read failed-flush V1 state");
    assert!(
        fact_in_memory,
        "failed flush keeps the exact fact in memory"
    );
    assert_eq!(active_turn_id, Some(settlement_turn_id));
    assert!(turn_closed);
    assert!(memory_revision > durable_revision);

    // Simulate the process-local owner loss while retaining this handle's dirty document; the
    // durable intent must re-arm admission and the next retry must flush the same terminal fact.
    // 模拟进程内 owner 丢失但保留当前句柄的脏文档；可靠意图必须重新武装准入，
    // 下一次重试须刷写同一条终态事实。
    {
        let mut guard = state.lock().expect("clear owners for restart");
        guard.session_terminal_persistence_owners.clear();
        guard.session_runtime_kernels.clear();
    }
    rebuild_pending_terminal_settlement_owner(&state, session_id, handle.as_ref());
    {
        let mut guard = state.lock().expect("inspect rebuilt owner");
        assert_eq!(
            guard.session_terminal_persistence_owners.get(&session_id),
            Some(&SessionTerminalPersistenceOwner::RecoveredSettlement {
                generation: owner_generation.clone(),
            }),
            "the exact original generation must be rebuilt, not a fresh owner"
        );
        assert!(
            start_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                "blocked-replacement-turn"
            )
            .is_err()
        );
    }

    // One controlled retry settles the exact turn once and releases only the rebuilt owner.
    // 一次受控重试恰好结算该回合一次，并且只释放重建的 owner。
    let outcome = settle_pending_terminal_settlement(&state, &handle, session_id)
        .await
        .expect("recovered storage should settle");
    assert_eq!(outcome, TerminalSettlementRetryOutcome::SettledByRetry);
    {
        let guard = state.lock().expect("inspect released owner");
        assert!(guard.session_terminal_persistence_owners.is_empty());
    }
    // The settlement is absent, the turn owns exactly one terminal fact, and repeat retries no-op.
    // 结算已消失，回合恰好拥有一条终态事实，重复重试为无操作。
    let repeated = settle_pending_terminal_settlement(&state, &handle, session_id)
        .await
        .expect("repeat retry should stay idempotent");
    assert_eq!(
        repeated,
        TerminalSettlementRetryOutcome::NoPendingSettlement
    );
    assert!(
        handle
            .read(|document| document.terminal_settlement_record().is_none())
            .expect("settlement record should remain readable")
    );
    let terminal_fact_count = handle
        .read(|document| {
            document
                .events()
                .iter()
                .filter(|event| {
                    event.turn_id == Some(settlement_turn_id)
                        && matches!(event.event, EventBody::TurnStats(_))
                })
                .count()
        })
        .expect("terminal facts should remain readable");
    assert_eq!(terminal_fact_count, 1);
    let mut guard = state.lock().expect("start replacement turn");
    start_webui_session_kernel_user_turn(&mut guard, session_id, "replacement-turn")
        .expect("replacement turn should start after durable settlement");
}

/// A corrupt or stale intent cannot close a different active turn during recovery.
/// 损坏或过期的结算意图在恢复时不能闭合另一条活动回合。
#[tokio::test]
async fn settlement_retry_rejects_different_active_turn() {
    let session_id = SessionId::new_v4();
    let (service_state, handle) = scripted_settlement_fixture(session_id, []);
    let active_turn_id = handle
        .read(|document| document.active_turn().map(|turn| turn.turn_id))
        .expect("read open turn")
        .expect("fixture has one active turn");
    let owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    let record = TerminalSettlementRecord {
        settlement_id: format!(
            "terminal-settlement:{session_id}:{}",
            owner.owner_generation()
        ),
        session_id: session_id.to_string(),
        turn_id: TurnId::try_new(active_turn_id.value() + 1).expect("different valid turn id"),
        scope: vulcan_session_core::v1::TerminalSettlementScope::BusinessTurn,
        owner_generation: owner.owner_generation(),
        state: TerminalSettlementState::Pending,
        reason_code: String::from("stream_finished_without_v1_api_terminal"),
        updated_at_ms: 1_700_000_000_002,
    };
    persist_terminal_settlement_intent(
        &handle,
        TerminalSettlementIntentTransition::Begin {
            record: record.clone(),
        },
    )
    .await
    .expect("persist stale exact intent");
    let state = Arc::new(Mutex::new(service_state));
    rebuild_pending_terminal_settlement_owner(&state, session_id, handle.as_ref());

    let error = settle_pending_terminal_settlement(&state, &handle, session_id)
        .await
        .expect_err("a different active turn must fail closed");
    assert!(error.contains("differs from active turn"));
    let mutation_error = crate::agent_stream::persist_v1_execution_failure_boundary_for_turn(
        &state,
        session_id,
        "stream_finished_without_v1_api_terminal",
        None,
        Some(record.turn_id),
    )
    .await
    .expect_err("the durable mutation must reject the same stale turn");
    assert!(mutation_error.contains("turn-scoped mutation expected open turn"));
    assert_eq!(
        handle
            .read(|document| document.active_turn().map(|turn| turn.turn_id))
            .expect("read protected turn"),
        Some(active_turn_id),
    );
    assert_eq!(
        handle
            .read(|document| document.terminal_settlement_record().cloned())
            .expect("read retained intent"),
        Some(record),
    );
    assert!(
        state
            .lock()
            .expect("read retained owner")
            .session_terminal_persistence_owners
            .contains_key(&session_id)
    );
}

/// Verifies an older settlement owner can never release a newer live owner.
/// 验证旧的结算 owner 绝不可能释放更新的存活 owner。
#[tokio::test]
async fn recorded_settlement_release_never_removes_newer_owner() {
    let session_id = SessionId::new_v4();
    let (service_state, handle) = scripted_settlement_fixture(session_id, [4]);
    let original_owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    let (state, _owner) = settle_failed_terminal(session_id, service_state, original_owner).await;

    // A newer boundary takes over admission before the retry resolves the durable intent.
    // 重试结算可靠意图之前，更新的边界接管准入。
    let newer_owner = SessionTerminalPersistenceOwner::RunStop(Uuid::new_v4());
    {
        let mut guard = state.lock().expect("release original before newer owner");
        guard.session_terminal_persistence_owners.clear();
        register_session_terminal_persistence_owner(&mut guard, session_id, newer_owner.clone())
            .expect("newer terminal owner should register");
    }

    let outcome = settle_pending_terminal_settlement(&state, &handle, session_id)
        .await
        .expect("durable settlement should resolve against the newer boundary");
    assert_eq!(outcome, TerminalSettlementRetryOutcome::SettledByRetry);

    let guard = state.lock().expect("inspect newer owner after release");
    assert_eq!(
        guard.session_terminal_persistence_owners.get(&session_id),
        Some(&newer_owner),
        "the older settlement must never release the newer owner"
    );
}

/// Verifies an owner retained before intent creation can settle after its V1 handle returns.
/// 验证意图创建前保留的所有者在 V1 句柄恢复后能够结算。
#[tokio::test]
async fn pre_intent_handle_failure_retries_exact_terminal_owner() {
    // The stream finishes while its exact V1 handle is temporarily absent from the registry.
    // 流结束时，其精确 V1 句柄暂时不在注册表中。
    let session_id = SessionId::new_v4();
    let mut service_state = test_service_state_with_parent_session(session_id);
    let owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    register_session_terminal_persistence_owner(&mut service_state, session_id, owner.clone())
        .expect("owner should register");
    let state = Arc::new(Mutex::new(service_state));
    let owner_guard = SessionTerminalPersistenceOwnerGuard::adopt_registered(
        state.clone(),
        session_id,
        owner.clone(),
    )
    .expect("owner guard should adopt");
    assert!(
        !crate::settle_stream_terminal_authority(
            &state,
            session_id,
            false,
            "stream_exhausted",
            None,
            owner_guard,
        )
        .await
    );
    {
        let guard = state.lock().expect("inspect retained pre-intent repair");
        assert_eq!(
            guard.session_terminal_persistence_owners.get(&session_id),
            Some(&owner)
        );
        assert_eq!(
            guard
                .terminal_pre_intent_repairs
                .get(&session_id)
                .expect("pre-intent repair must be retained")
                .reason_code,
            "stream_exhausted"
        );
    }

    // Restoring the same V1 session authority lets the controlled retry persist intent first.
    // 恢复同一 V1 会话权威后，受控重试先持久化意图。
    let mut document = SessionDocument::new(
        session_id.to_string(),
        "project-a",
        "Pre-intent repair",
        1_700_000_000_000,
    );
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("pre-intent-root").expect("valid action"),
                "run provider request",
                Vec::new(),
            ),
            1_700_000_000_001,
        )
        .expect("root input should open a turn");
    let handle = {
        let mut guard = state.lock().expect("register recovered V1 handle");
        register_test_session_v1_document_with_scripted_flush_failures(
            &mut guard,
            session_id,
            document,
            std::collections::BTreeSet::new(),
        )
    };
    let outcome = retry_pre_intent_terminal_settlement(&state, session_id)
        .await
        .expect("recovered V1 handle should settle the retained exact owner");
    assert_eq!(outcome, TerminalSettlementRetryOutcome::SettledByRetry);
    assert!(
        state
            .lock()
            .expect("inspect resolved owner")
            .session_terminal_persistence_owners
            .is_empty()
    );
    assert!(
        state
            .lock()
            .expect("inspect resolved repair")
            .terminal_pre_intent_repairs
            .is_empty()
    );
    assert!(
        handle
            .read(|document| document.terminal_settlement_record().is_none())
            .expect("read resolved intent")
    );
}

/// A physical V1 reload rebuilds one pending settlement and persists exactly one terminal fact.
/// 物理重载 V1 后重建一条待结算意图，并恰好持久化一条终态事实。
#[tokio::test]
async fn durable_settlement_survives_physical_restart_and_retries_once() {
    let session_id = SessionId::new_v4();
    let fixture = TestHostRuntimeRoot::new(session_id);
    let locator = fixture.v1_storage_locator();
    let document = load_session_v1_full(&locator)
        .expect("load real V1 fixture")
        .document;
    let handle = SessionHandle::new(session_id, locator.clone(), document);
    let turn_id = handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("physical-restart-root").expect("valid action"),
                "run provider request",
                Vec::new(),
            ),
            1_700_000_000_001,
        )
        .expect("open a durable turn")
        .value;
    let owner = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    let record = TerminalSettlementRecord {
        settlement_id: format!(
            "terminal-settlement:{session_id}:{}",
            owner.owner_generation()
        ),
        session_id: session_id.to_string(),
        turn_id,
        scope: vulcan_session_core::v1::TerminalSettlementScope::BusinessTurn,
        owner_generation: owner.owner_generation(),
        state: TerminalSettlementState::Pending,
        reason_code: String::from("stream_finished_without_v1_api_terminal"),
        updated_at_ms: 1_700_000_000_002,
    };
    persist_terminal_settlement_intent(
        &handle,
        TerminalSettlementIntentTransition::Begin {
            record: record.clone(),
        },
    )
    .await
    .expect("persist exact intent before process restart");
    drop(handle);

    let reloaded = load_session_v1_full(&locator)
        .expect("physically reload V1 after restart")
        .document;
    assert_eq!(reloaded.terminal_settlement_record(), Some(&record));
    let restarted_handle = SessionHandle::new(session_id, locator.clone(), reloaded);
    let restarted_service = test_service_state_with_parent_session(session_id);
    restarted_service
        .session_runtime_registry
        .register(restarted_handle.clone())
        .expect("register restarted exact V1 handle");
    let restarted_state = Arc::new(Mutex::new(restarted_service));
    rebuild_pending_terminal_settlement_owner(
        &restarted_state,
        session_id,
        restarted_handle.as_ref(),
    );
    {
        let mut guard = restarted_state.lock().expect("inspect rebuilt owner");
        assert!(
            start_webui_session_kernel_user_turn(
                &mut guard,
                session_id,
                "replacement before retry",
            )
            .is_err()
        );
    }
    assert_eq!(
        settle_pending_terminal_settlement(&restarted_state, &restarted_handle, session_id)
            .await
            .expect("retry exact physical V1 turn"),
        TerminalSettlementRetryOutcome::SettledByRetry
    );
    drop(restarted_handle);
    let settled = load_session_v1_full(&locator)
        .expect("physically reload settled V1 turn")
        .document;
    assert!(settled.terminal_settlement_record().is_none());
    assert_eq!(
        settled
            .events()
            .iter()
            .filter(|event| {
                event.turn_id == Some(turn_id) && matches!(event.event, EventBody::TurnStats(_))
            })
            .count(),
        1,
    );
    let repeated_handle = {
        restarted_state
            .lock()
            .expect("read registered handle")
            .session_runtime_registry
            .get(session_id)
            .expect("registered handle remains available")
    };
    assert_eq!(
        settle_pending_terminal_settlement(&restarted_state, &repeated_handle, session_id)
            .await
            .expect("repeat retry must be idempotent"),
        TerminalSettlementRetryOutcome::NoPendingSettlement
    );
}
