//! Production auxiliary report settlement with real V1 receipts and injected storage failures.
//! 使用真实 V1 回执与存储故障注入的生产辅助报告结算。

use super::*;
use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationReceipt,
};
use crate::state::{
    SessionTerminalPersistenceOwner, SessionTerminalPersistenceOwnerGuard,
    register_session_terminal_persistence_owner,
};
use vulcan_session_core::v1::collaboration::*;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::PrepareAndStartApiInput;
use vulcan_session_core::v1::event::{ApiRequestSnapshot, ProviderProjectionProof};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;
use vulcan_session_core::v1::*;

/// Retains the real session owner and its finished physical request for one auxiliary test.
/// 为单个辅助测试保留真实会话所有者及已完成物理请求。
struct QuestionFinishFixture {
    /// Service state containing the prepared adapter and registered document.
    /// 包含已准备适配器及已登记文档的服务状态。
    state: SharedServiceBridgeState,
    /// Unique target session identity.
    /// 唯一目标会话身份。
    session_id: SessionId,
    /// Canonical document used by both finish and recovery.
    /// 结束与恢复共同使用的规范文档。
    handle: Arc<SessionHandle>,
    /// Captured stream execution scope.
    /// 捕获的流执行范围。
    scope: TerminalSettlementScope,
    /// Actual durable physical terminal receipt.
    /// 实际可靠物理终态回执。
    receipt: SessionMutationReceipt<ApiGroupCompletionReceipt>,
    /// Exact retained terminal admission guard.
    /// 精确保留的终态准入守卫。
    owner: SessionTerminalPersistenceOwnerGuard,
    /// Original business adapter records, which auxiliary finish must preserve.
    /// 辅助结束必须保留的原业务适配器记录。
    business_records: String,
}

/// Builds a real QA request permit input from the exact current document frontier.
/// 从精确当前文档前沿构建真实答疑请求许可输入。
/// `document` owns the projection; `now_ms` dates preparation. Returns an input for the production API gate.
/// `document` 拥有投影；`now_ms` 记录准备时间。返回生产 API 关卡使用的输入。
fn question_api_preparation(document: &SessionDocument, now_ms: i64) -> PrepareAndStartApiInput {
    // Bind the last authoritative event, including the execution start.
    // 绑定最后一条权威事件，包括执行启动。
    let through_seq = document.events().last().unwrap().seq;
    PrepareAndStartApiInput {
        purpose: ApiPurposeKind::QuestionExecution,
        request: ApiRequestSnapshot::new("openai", "model"),
        request_budget_evidence:
            crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
        projection: ProviderProjectionProof {
            segment_generation: document.segment_generation(),
            document_revision: document.memory_revision(),
            retained_from_seq: Seq::try_new(1).unwrap(),
            through_seq,
            fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq)
                .unwrap(),
        },
        locked_baseline: LockedBaselineProjection::empty(),
        continuation_action_id: None,
        now_ms,
    }
}

/// Creates a valid question attempt and completes its API through the canonical writer.
/// 创建有效问题尝试，并通过规范写入器完成其 API。
/// `control` selects idle or retained-parent admission; `failed_writes` identifies subsequent injected failures.
/// `control` 选择空闲或保留父回合准入；`failed_writes` 指定后续注入的失败写入。
/// Returns the exact receipt and owner required by the production report finisher.
/// 返回生产报告收尾器所需的精确回执与所有者。
fn question_finish_fixture(
    control: CollaborationRunControl,
    failed_writes: BTreeSet<usize>,
) -> QuestionFinishFixture {
    // Each fixture uses isolated in-memory ownership and a scripted, non-user persistence backend.
    // 每个夹具使用隔离内存所有权及脚本化非用户持久化后端。
    let session_id = SessionId::new_v4();
    // Only an idle target owns a new auxiliary turn.
    // 只有空闲目标拥有新的辅助回合。
    let in_parent = control != CollaborationRunControl::NaturallyIdle;
    // Real typed domain transitions establish all required identities.
    // 真实类型化领域转换建立全部所需身份。
    let mut document =
        SessionDocument::new(session_id.to_string(), "project-a", "Question finish", 1);
    // Retained parents own their original user root, not a fabricated QA root input.
    // 保留父会话拥有原用户根输入，而非伪造答疑根输入。
    let parent_turn = in_parent.then(|| {
        document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("parent-root").unwrap(),
                    "Original task",
                    vec![],
                ),
                2,
            )
            .unwrap()
            .into_value()
    });
    if in_parent {
        document
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("control").unwrap(),
                state: control,
                changed_at_ms: 3,
            })
            .unwrap();
    }
    // The question purpose follows the exact real parent binding.
    // 问题用途遵循精确真实父绑定。
    let purpose = if in_parent {
        CollaborationExecutionPurpose::InTurnQuestion
    } else {
        CollaborationExecutionPurpose::NaturallyIdleQuestion
    };
    // Source identity is accepted as a delivered source fact by the target reducer.
    // 目标归约器将来源身份作为已投递来源事实接纳。
    let source = CollaborationParticipant::new(
        "source",
        "source",
        "project-a",
        1,
        Some(TurnId::try_new(1).unwrap()),
        None,
    )
    .unwrap();
    // The target binds its actual session and optional parent turn.
    // 目标绑定实际会话及可选父回合。
    let target = CollaborationParticipant::new(
        session_id.to_string(),
        session_id.to_string(),
        "project-a",
        1,
        parent_turn,
        None,
    )
    .unwrap();
    // Stable identities are shared by delivery, execution and the final stream scope.
    // 投递、执行及最终流范围共享稳定身份。
    let question_id = CollaborationQuestionId::try_new("question").unwrap();
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("accept").unwrap(),
            question_id: question_id.clone(),
            delivery_id: CollaborationDeliveryId::try_new("delivery").unwrap(),
            source,
            target: target.clone(),
            bound_turn_id: parent_turn,
            execution_purpose: purpose,
            accepted_at_ms: 4,
        })
        .unwrap();
    // The execution exists before any physical API, matching production scheduling order.
    // 执行先于任何物理 API 存在，与生产调度顺序一致。
    let execution_id = CollaborationExecutionId::try_new("execution").unwrap();
    document
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin").unwrap(),
            execution_id: execution_id.clone(),
            question_id,
            participant: target,
            purpose,
            parent_turn_id: parent_turn,
            started_at_ms: 5,
        })
        .unwrap();
    // This real API produces final text but does not close any logical business task.
    // 此真实 API 产生最终文本，但不关闭任何逻辑业务任务。
    let preparation = question_api_preparation(&document, 6);
    // Capture the permitted physical API identity for the terminal receipt.
    // 捕获已许可的物理 API 身份，供终态回执使用。
    let api_id = document
        .prepare_and_start_api(preparation)
        .unwrap()
        .into_value()
        .api_call_id;
    document
        .append_assistant_text(
            api_id,
            "Optional question execution ended",
            AssistantSemanticStatus::Completed,
            7,
        )
        .unwrap();
    // The existing completed business adapter makes duplicate summaries and statistics observable.
    // 既有已完成业务适配器使重复摘要及统计可被观察。
    let mut adapter = SessionRuntimeStateEntry::default();
    vulcan_agent_runtime::session_event_runtime::apply_terminal_execution_report_to_state(
        &mut adapter,
        session_id,
        "1000",
        &question_finish_report(session_id, "Original task result"),
    )
    .unwrap();
    adapter.active_precheck_turn_id = Some("retained-business-precheck".into());
    // Capture the exact preexisting records before the service receives a QA final.
    // 服务收到答疑 final 前捕获精确既有记录。
    let business_records = format!("{:?}", adapter.conversation_state.records);
    // The service starts in a physical streaming phase while preserving the adapter's business facts.
    // 服务从物理流阶段开始，同时保留适配器中的业务事实。
    let mut service = test_service_state_with_parent_session(session_id);
    service
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .session_runtime_state
        .insert(session_id, adapter);
    service
        .workspace_snapshot
        .as_mut()
        .unwrap()
        .session_run_phases
        .insert(session_id, SessionRunPhase::Thinking);
    // Writing API completion first creates the real durable receipt supplied to the finisher.
    // 先写入 API 完成，产生传给收尾器的真实可靠回执。
    let handle = register_test_session_v1_document_with_scripted_flush_failures(
        &mut service,
        session_id,
        document,
        failed_writes,
    );
    // The first flush is the physical terminal; later flush indices target auxiliary settlement.
    // 首次刷写是物理终态；后续刷写序号用于辅助结算故障注入。
    let receipt = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api_id,
                    ApiCompletion {
                        request: ApiRequestSnapshot::new("openai", "model"),
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings {
                            completed_at_ms: Some(8),
                            ..ApiTimings::default()
                        },
                        provider_source: None,
                        completed_at_ms: 8,
                    },
                )
            },
        )
        .unwrap();
    // Only this exact stream terminal owner can reopen admission.
    // 只有此精确流终态所有者可重新开放准入。
    let owner_id = SessionTerminalPersistenceOwner::StreamTask(Uuid::new_v4());
    register_session_terminal_persistence_owner(&mut service, session_id, owner_id.clone())
        .unwrap();
    // Shared state is reused directly by the finisher and any subsequent recovery.
    // 收尾器及后续恢复直接复用共享状态。
    let state = Arc::new(Mutex::new(service));
    // The guard is adopted after registration, matching the finished-event ownership handoff.
    // 登记后接管守卫，与结束事件的所有权交接一致。
    let owner =
        SessionTerminalPersistenceOwnerGuard::adopt_registered(state.clone(), session_id, owner_id)
            .unwrap();
    QuestionFinishFixture {
        state,
        session_id,
        handle,
        scope: TerminalSettlementScope::QuestionExecution {
            execution_id,
            api_call_id: Some(api_id),
        },
        receipt,
        owner,
        business_records,
    }
}

/// Builds a physical report with no fabricated checkpoint or tool answer.
/// 构建不含伪造检查点或工具答案的物理报告。
/// `session_id` and `text` identify the actual request; the returned report is successful by default.
/// `session_id` 和 `text` 标识实际请求；返回报告默认成功。
fn question_finish_report(session_id: SessionId, text: &str) -> ExecutionRunReport {
    ExecutionRunReport {
        session_id,
        preparation: None,
        assistant_output: Some(text.into()),
        thinking_output: None,
        conversation_state: None,
        provider_request_count: 1,
        last_successful_provider_usage: None,
        cumulative_successful_provider_usage: None,
        embedded_turn_usage_segments: vec![],
        mcp_discovery_rows: vec![],
        mcp_discovery_errors: vec![],
        warnings: vec![],
        error: None,
        controller_events: vec![],
    }
}

/// QA final settles as unanswered without closing a parent or replacing the original completed business records.
/// 答疑 final 以未回答结算，不关闭父回合或替换原已完成业务记录。
#[tokio::test]
async fn question_finished_preserves_business_state_and_restores_original_control() {
    for (control, phase) in [
        (
            CollaborationRunControl::NaturallyIdle,
            SessionRunPhase::Ready,
        ),
        (CollaborationRunControl::Running, SessionRunPhase::Preparing),
        (CollaborationRunControl::MainOnly, SessionRunPhase::Stopped),
    ] {
        // Run each control state through the real report finisher and canonical persistence backend.
        // 让每个控制状态经过真实报告收尾器及规范持久化后端。
        let fixture = question_finish_fixture(control, BTreeSet::new());
        // Preserve the original mode to detect accidental Goal/Plan lifecycle changes.
        // 保留原模式，以检测意外 Goal/Plan 生命周期变化。
        let mode = fixture
            .handle
            .read(|document| document.metadata.mode.clone())
            .unwrap();
        assert!(
            crate::handle_question_finished_report(
                &ServiceEventEmitter::new(vec![]),
                &fixture.state,
                question_finish_report(fixture.session_id, "Question final is not an answer"),
                fixture.scope,
                Some(fixture.receipt),
                false,
                fixture.owner
            )
            .await
        );
        fixture
            .handle
            .read(|document| {
                assert_eq!(document.collaboration_run_control(), control);
                assert_eq!(document.metadata.mode, mode);
                assert_eq!(
                    document.active_turn().unwrap().closed,
                    control == CollaborationRunControl::NaturallyIdle
                );
                assert_eq!(
                    document
                        .collaboration_state()
                        .question_executions
                        .values()
                        .next()
                        .unwrap()
                        .outcome,
                    Some(CollaborationQuestionExecutionOutcome::Unanswered)
                );
                assert_eq!(document.memory_revision(), document.durable_revision());
            })
            .unwrap();
        // No pending owner and no second business summary remain after durable completion.
        // 可靠完成后无待处理所有者，也无第二份业务摘要。
        let guard = fixture.state.lock().unwrap();
        assert!(
            !guard
                .session_terminal_persistence_owners
                .contains_key(&fixture.session_id)
        );
        // The original business adapter retains its precheck identity and all prior displayed records.
        // 原业务适配器保留预检查身份及全部先前展示记录。
        let workspace = guard.workspace_snapshot.as_ref().unwrap();
        assert_eq!(workspace.session_run_phases[&fixture.session_id], phase);
        assert_eq!(
            format!(
                "{:?}",
                workspace.session_runtime_state[&fixture.session_id]
                    .conversation_state
                    .records
            ),
            fixture.business_records
        );
        assert_eq!(
            workspace.session_runtime_state[&fixture.session_id]
                .active_precheck_turn_id
                .as_deref(),
            Some("retained-business-precheck")
        );
    }
}

/// A wrong physical receipt fails the auxiliary execution while retaining the original parent task.
/// 错误物理回执使辅助执行失败，同时保留原父任务。
#[tokio::test]
async fn question_finished_rejects_wrong_api_receipt_without_completing_parent() {
    // Keep the real captured scope but replace only the supplied terminal receipt's API identity.
    // 保留真实捕获范围，只替换传入终态回执的 API 身份。
    let mut fixture = question_finish_fixture(CollaborationRunControl::MainOnly, BTreeSet::new());
    fixture.receipt.value.api_call_id =
        ApiCallId::try_new(fixture.receipt.value.api_call_id.value() + 1).unwrap();
    assert!(
        crate::handle_question_finished_report(
            &ServiceEventEmitter::new(vec![]),
            &fixture.state,
            question_finish_report(fixture.session_id, "Invalid receipt"),
            fixture.scope,
            Some(fixture.receipt),
            false,
            fixture.owner
        )
        .await
    );
    fixture
        .handle
        .read(|document| {
            assert!(!document.active_turn().unwrap().closed);
            assert_eq!(
                document.collaboration_run_control(),
                CollaborationRunControl::MainOnly
            );
            assert_eq!(
                document
                    .collaboration_state()
                    .question_executions
                    .values()
                    .next()
                    .unwrap()
                    .outcome,
                Some(CollaborationQuestionExecutionOutcome::Failed)
            );
        })
        .unwrap();
}

/// Failed success/intent writes recover the original auxiliary outcome without duplicating business completion.
/// 成功及意图写入失败后恢复原辅助结果，不重复业务完成。
#[tokio::test]
async fn question_finished_storage_failure_recovers_committed_outcome() {
    // Fail the execution-completion flush and the following intent flush after the API was durable.
    // API 已可靠落盘后，让执行完成刷写及随后意图刷写失败。
    let fixture = question_finish_fixture(
        CollaborationRunControl::NaturallyIdle,
        [2, 3].into_iter().collect(),
    );
    assert!(
        !crate::handle_question_finished_report(
            &ServiceEventEmitter::new(vec![]),
            &fixture.state,
            question_finish_report(fixture.session_id, "Question ended"),
            fixture.scope,
            Some(fixture.receipt),
            false,
            fixture.owner
        )
        .await
    );
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .session_terminal_persistence_owners
            .contains_key(&fixture.session_id)
    );
    assert_eq!(
        crate::terminal_settlement_service::retry_pre_intent_terminal_settlement(
            &fixture.state,
            fixture.session_id
        )
        .await
        .unwrap(),
        crate::terminal_settlement_service::TerminalSettlementRetryOutcome::SettledByRetry
    );
    fixture.handle.read(|document| {
        assert!(document.active_turn().unwrap().closed);
        assert!(document.terminal_settlement_record().is_none());
        assert_eq!(document.collaboration_state().question_executions.values().next().unwrap().outcome, Some(CollaborationQuestionExecutionOutcome::Unanswered));
        assert_eq!(document.events().iter().filter(|event| matches!(&event.event, EventBody::Collaboration(value) if value.event_kind == CollaborationEventKind::QuestionExecutionCompleted)).count(), 1);
        assert_eq!(document.memory_revision(), document.durable_revision());
    }).unwrap();
}

/// An earlier successful request cannot finish or interrupt a newer request in the same execution.
/// 同一执行中的先前成功请求不能结束或中断更新请求。
#[tokio::test]
async fn question_finished_rejects_stale_terminal_after_new_request() {
    // Keep the completed first request and start a distinct second request under the same parent.
    // 保留已完成的首个请求，并在同一父回合下启动不同的第二个请求。
    let fixture = question_finish_fixture(CollaborationRunControl::Running, BTreeSet::new());
    // The new request crosses the real durable preparation gate before the stale report arrives.
    // 旧报告到达前，新请求经过真实可靠准备关卡。
    let preparation = fixture
        .handle
        .read(|document| question_api_preparation(document, 9))
        .unwrap();
    // Capture the new physical owner to verify it is never terminalized by the stale report.
    // 捕获新物理所有者，以验证旧报告绝不会终结它。
    let new_api = fixture
        .handle
        .prepare_and_start_api_durable(preparation)
        .unwrap()
        .value
        .api_call_id;
    assert!(
        !crate::handle_question_finished_report(
            &ServiceEventEmitter::new(vec![]),
            &fixture.state,
            question_finish_report(fixture.session_id, "Stale final"),
            fixture.scope,
            Some(fixture.receipt),
            false,
            fixture.owner,
        )
        .await
    );
    fixture
        .handle
        .read(|document| {
            assert_eq!(document.active_api().unwrap().api_call_id, new_api);
            assert!(!document.active_turn().unwrap().closed);
            assert_eq!(
                document
                    .collaboration_state()
                    .question_executions
                    .values()
                    .next()
                    .unwrap()
                    .state,
                CollaborationQuestionExecutionState::Running
            );
            assert!(document.terminal_settlement_record().is_some());
            assert_eq!(
                document
                    .events()
                    .iter()
                    .filter(|event| matches!(event.event, EventBody::ApiStats(_)))
                    .count(),
                1
            );
        })
        .unwrap();
    assert!(
        fixture
            .state
            .lock()
            .unwrap()
            .session_terminal_persistence_owners
            .contains_key(&fixture.session_id)
    );
}

/// A queued user task survives auxiliary completion and failed successor publication as one exact new business turn.
/// 排队用户任务跨辅助完成及后继发布失败保留为一个精确新业务回合。
#[tokio::test]
async fn question_finished_persists_queued_successor_and_recovers_failed_publication() {
    for fail_publication in [false, true] {
        // API completion is flush one, queue admission two, QA completion three, and successor selection four.
        // API 完成为第一次刷写，队列准入为第二次，答疑完成为第三次，后继选择为第四次。
        let fixture = question_finish_fixture(
            CollaborationRunControl::NaturallyIdle,
            if fail_publication {
                [4, 5].into_iter().collect()
            } else {
                BTreeSet::new()
            },
        );
        // Keep the accepted input identity so projection cannot silently replace the queued task.
        // 保留已接纳输入身份，避免投影静默替换排队任务。
        let action = ClientActionId::try_new("business-after-qa").unwrap();
        fixture
            .handle
            .enqueue_input(
                ClientInputSubmission::new(action.clone(), "Do the next task", vec![]),
                9,
            )
            .unwrap();
        // A successor write failure also fails the first repair-intent write to exercise retained ownership.
        // 后继写入失败同时使首次修复意图写入失败，以验证所有权保留。
        let settled = crate::handle_question_finished_report(
            &ServiceEventEmitter::new(vec![]),
            &fixture.state,
            question_finish_report(fixture.session_id, "No answer needed"),
            fixture.scope,
            Some(fixture.receipt),
            false,
            fixture.owner,
        )
        .await;
        assert_eq!(settled, !fail_publication);
        if fail_publication {
            assert!(
                fixture
                    .state
                    .lock()
                    .unwrap()
                    .session_terminal_persistence_owners
                    .contains_key(&fixture.session_id)
            );
            assert_eq!(
                crate::terminal_settlement_service::retry_pre_intent_terminal_settlement(
                    &fixture.state,
                    fixture.session_id
                )
                .await
                .unwrap(),
                crate::terminal_settlement_service::TerminalSettlementRetryOutcome::SettledByRetry
            );
        }
        fixture
            .handle
            .read(|document| {
                let execution = document
                    .collaboration_state()
                    .question_executions
                    .values()
                    .next()
                    .unwrap();
                let successor = execution.queued_successor.as_ref().unwrap();
                assert_eq!(successor.client_action_id, action);
                assert_ne!(execution.execution_turn_id, Some(successor.turn_id));
                assert_eq!(document.active_turn().unwrap().turn_id, successor.turn_id);
                assert!(!document.active_turn().unwrap().closed);
                assert_eq!(
                    execution.outcome,
                    Some(CollaborationQuestionExecutionOutcome::Unanswered)
                );
                assert!(document.metadata.input_state.queued_inputs.is_empty());
                assert!(document.terminal_settlement_record().is_none());
                assert_eq!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Running
                );
                assert_eq!(document.memory_revision(), document.durable_revision());
                assert_eq!(
                    document
                        .events()
                        .iter()
                        .filter(|event| matches!(event.event, EventBody::TurnStats(_)))
                        .count(),
                    1
                );
            })
            .unwrap();
        // The authoritative read runs again after the real dispatcher acquires its admission reservation.
        // 真实分发器取得准入预留后会再次执行权威读取。
        let successor = fixture
            .handle
            .read(|document| {
                document
                    .collaboration_state()
                    .question_executions
                    .values()
                    .next()
                    .unwrap()
                    .queued_successor
                    .clone()
                    .unwrap()
            })
            .unwrap();
        assert!(
            crate::webui_prompt_lifecycle::queued_dispatch_authority_from_v1_handle(
                fixture.handle.as_ref(),
                successor.turn_id,
                successor.input_id,
                &successor.client_action_id,
                successor.input_seq
            )
            .is_ok()
        );
        fixture
            .handle
            .set_collaboration_run_control_durable(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("stop-queued-dispatch").unwrap(),
                state: CollaborationRunControl::MainAndSubagents,
                changed_at_ms: 20,
            })
            .unwrap();
        assert!(
            crate::webui_prompt_lifecycle::queued_dispatch_authority_from_v1_handle(
                fixture.handle.as_ref(),
                successor.turn_id,
                successor.input_id,
                &successor.client_action_id,
                successor.input_seq
            )
            .is_err()
        );
        assert!(
            !fixture
                .state
                .lock()
                .unwrap()
                .session_terminal_persistence_owners
                .contains_key(&fixture.session_id)
        );
    }
}
