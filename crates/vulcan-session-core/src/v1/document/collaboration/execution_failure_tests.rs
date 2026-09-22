//! Exact auxiliary failure ownership and atomic recovery regressions.
//! 精确辅助失败所有权与原子恢复回归。

use super::*;
use crate::v1::config::{
    TerminalSettlementRecord, TerminalSettlementScope, TerminalSettlementState,
};
use crate::v1::document::{ClientInputSubmission, TurnOrigin};
use crate::v1::event::{
    ApiCallStatus, ApiPurposeKind, ApiRequestSnapshot, ToolCallStatus, ToolPayload,
};
use crate::v1::ids::ClientActionId;

/// Creates a real accepted question execution, with an optional active QA API and tool.
/// 创建真实已接纳问题执行，并可附带活动答疑 API 与工具。
/// The returned intent identifies the exact target turn and execution, before persistence.
/// 返回意图在持久化前标识精确目标回合与执行。
fn fixture(in_parent: bool, with_api: bool) -> (SessionDocument, TerminalSettlementRecord) {
    // Both documents own concrete participants in the same project.
    // 两个文档分别拥有同项目中的具体参与者。
    let mut source = SessionDocument::new("failure-source", "project", "source", 1);
    // The target either retains a parent turn or begins naturally idle.
    // 目标要么保留父回合，要么从自然空闲开始。
    let mut target = SessionDocument::new("failure-target", "project", "target", 1);
    // Source ownership comes from a real user admission.
    // 来源所有权来自真实用户准入。
    let source_turn = source
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("source-root").unwrap(),
                "work",
                vec![],
            ),
            2,
        )
        .unwrap()
        .into_value();
    // A parent turn exists only for an in-turn question.
    // 只有回合内问题存在父回合。
    let parent_turn = in_parent.then(|| {
        target
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("target-root").unwrap(),
                    "work",
                    vec![],
                ),
                2,
            )
            .unwrap()
            .into_value()
    });
    if in_parent {
        target
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("running").unwrap(),
                state: CollaborationRunControl::Running,
                changed_at_ms: 2,
            })
            .unwrap();
    }
    // Purpose and identities are shared verbatim by creation, acceptance and execution.
    // 用途和身份由创建、接纳及执行原样共享。
    let purpose = if in_parent {
        CollaborationExecutionPurpose::InTurnQuestion
    } else {
        CollaborationExecutionPurpose::NaturallyIdleQuestion
    };
    // Source participant binds its real user turn.
    // 来源参与者绑定真实用户回合。
    let source_identity = CollaborationParticipant::new(
        "failure-source",
        "failure-source",
        "project",
        1,
        Some(source_turn),
        None,
    )
    .unwrap();
    // Target participant binds the retained parent, when one exists.
    // 存在保留父回合时，目标参与者绑定该回合。
    let target_identity = CollaborationParticipant::new(
        "failure-target",
        "failure-target",
        "project",
        1,
        parent_turn,
        None,
    )
    .unwrap();
    // Stable question and delivery identities remain queryable after failed execution.
    // 稳定问题及投递身份在执行失败后仍可查询。
    let question_id = CollaborationQuestionId::try_new("failure-question").unwrap();
    // Delivery is independent of whether the target chooses to answer.
    // 投递独立于目标是否选择回答。
    let delivery_id = CollaborationDeliveryId::try_new("failure-delivery").unwrap();
    source
        .create_question(CreateQuestionInput {
            operation_id: CollaborationOperationId::try_new("create").unwrap(),
            question_id: question_id.clone(),
            delivery_id: delivery_id.clone(),
            source: source_identity.clone(),
            target: target_identity.clone(),
            body: "Optional question.".into(),
            execution_purpose: purpose,
            created_at_ms: 3,
        })
        .unwrap();
    target
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("accept").unwrap(),
            question_id: question_id.clone(),
            delivery_id,
            source: source_identity,
            target: target_identity.clone(),
            bound_turn_id: parent_turn,
            execution_purpose: purpose,
            accepted_at_ms: 4,
        })
        .unwrap();
    // Execution is created before its optional physical request starts.
    // 执行先于可选物理请求创建。
    let execution_id = CollaborationExecutionId::try_new("failure-execution").unwrap();
    target
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin").unwrap(),
            execution_id: execution_id.clone(),
            question_id,
            participant: target_identity,
            purpose,
            parent_turn_id: parent_turn,
            started_at_ms: 5,
        })
        .unwrap();
    // Capture only the actual active API; pre-request failure has no fabricated API identity.
    // 仅捕获实际活动 API；请求前失败不伪造 API 身份。
    let api_call_id = with_api.then(|| {
        // Native tool state must be terminalized together with the failed request.
        // 原生工具状态必须与失败请求一起终结。
        let api_id = target
            .start_api(
                ApiPurposeKind::QuestionExecution,
                ApiRequestSnapshot::new("openai", "model"),
                6,
            )
            .unwrap();
        target
            .start_tool(
                api_id,
                "read_file",
                ToolPayload::try_new(serde_json::json!({"path":"file"})).unwrap(),
                None,
                7,
            )
            .unwrap();
        api_id
    });
    // The intent names the real auxiliary or parent turn, never a guessed successor.
    // 意图指定真实辅助或父回合，绝不猜测后继。
    let record = TerminalSettlementRecord {
        settlement_id: "failure-settlement".into(),
        session_id: "failure-target".into(),
        turn_id: target.active_turn().unwrap().turn_id,
        scope: TerminalSettlementScope::QuestionExecution {
            execution_id,
            api_call_id,
        },
        owner_generation: "stream-task:failure-owner".into(),
        state: TerminalSettlementState::Pending,
        reason_code: "qa_failed".into(),
        updated_at_ms: 8,
    };
    (target, record)
}

/// QA failures settle only their execution, including native tools, with one publication and restart-safe replay.
/// 答疑失败仅结算自身执行及原生工具，通过一次发布完成并支持重启安全重放。
#[test]
fn question_failure_preserves_parent_and_replays_after_reload() {
    for in_parent in [false, true] {
        for with_api in [false, true] {
            // Every case begins with real ownership and a durable-intent transition.
            // 每个场景从真实所有权及可靠意图转换开始。
            let (mut document, record) = fixture(in_parent, with_api);
            document
                .begin_terminal_settlement(record.clone(), 8)
                .unwrap();
            // Preserve the entire business mode and publication revision for comparison.
            // 保留完整业务模式及发布修订以供比较。
            let mode = document.metadata.mode.clone();
            // Composite API, note, execution and optional auxiliary closure publish together.
            // API、提示、执行及可选辅助闭合组合在一起发布。
            let revision = document.memory_revision();
            assert!(
                document
                    .fail_question_execution_for_terminal_settlement(
                        &record,
                        Some("Controlled failure."),
                        9
                    )
                    .unwrap()
            );
            assert_eq!(document.memory_revision(), revision + 1);
            assert_eq!(document.metadata.mode, mode);
            assert!(document.active_api().is_none());
            assert_eq!(document.active_turn().unwrap().closed, !in_parent);
            assert_eq!(
                document.active_turn().unwrap().origin == TurnOrigin::QuestionExecution,
                !in_parent
            );
            assert_eq!(document.turn_has_terminal_fact(record.turn_id), !in_parent);
            assert!(
                document
                    .terminal_settlement_has_terminal_fact(&record)
                    .unwrap()
            );
            if with_api {
                assert!(document.events().iter().any(|event| matches!(&event.event, EventBody::ToolCall(tool) if tool.status == ToolCallStatus::Interrupted && tool.output.is_some())));
                assert!(document.events().iter().any(|event| matches!(&event.event, EventBody::ApiStats(api) if api.purpose == ApiPurposeKind::QuestionExecution && api.status == ApiCallStatus::Failed)));
            }
            // Reload from serialized authority, without using any process-local execution cache.
            // 从序列化权威重载，不使用任何进程内执行缓存。
            let mut reloaded = SessionDocument::from_parts(
                serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
                document.runtime_lock.clone(),
                document.compactions.clone(),
                document.segment_catalog().to_vec(),
                document.events().to_vec(),
            )
            .unwrap();
            // Failed-store retry must not duplicate the error, tool result or execution completion.
            // 存储失败重试不得重复错误、工具结果或执行完成。
            let before = reloaded.clone();
            assert!(
                !reloaded
                    .fail_question_execution_for_terminal_settlement(
                        &record,
                        Some("Controlled failure."),
                        10
                    )
                    .unwrap()
            );
            assert_eq!(reloaded, before);
            reloaded
                .resolve_terminal_settlement(&record.settlement_id, 11)
                .unwrap();
            assert!(reloaded.terminal_settlement_record().is_none());
        }
    }
}

/// A wrong physical API and an unproved resolution leave every domain fact unchanged.
/// 错误物理 API 及无凭据的结算保持全部领域事实不变。
#[test]
fn question_failure_rejects_replacement_api_and_unproved_resolution() {
    // Retain a live business parent to catch accidental fallback into its ordinary failure path.
    // 保留活动业务父回合，以捕获意外回退到其普通失败路径。
    let (mut document, mut record) = fixture(true, true);
    if let TerminalSettlementScope::QuestionExecution { api_call_id, .. } = &mut record.scope {
        *api_call_id =
            Some(crate::v1::ids::ApiCallId::try_new(api_call_id.unwrap().value() + 1).unwrap());
    }
    document
        .begin_terminal_settlement(record.clone(), 8)
        .unwrap();
    // A stale captured API cannot interrupt whichever request happens to be current.
    // 过期捕获 API 不能中断恰好处于当前状态的其他请求。
    let before = document.clone();
    assert!(
        document
            .fail_question_execution_for_terminal_settlement(&record, None, 9)
            .is_err()
    );
    assert!(
        document
            .resolve_terminal_settlement(&record.settlement_id, 9)
            .is_err()
    );
    assert_eq!(document, before);
}

/// An already terminal physical request remains immutable while its exact auxiliary execution settles.
/// 已终态的物理请求保持不可变，同时结算其精确辅助执行。
#[test]
fn question_failure_recovers_after_physical_api_interruption() {
    for in_parent in [false, true] {
        // Process interruption can persist before the stream failure intent is committed.
        // 进程中断可能先于流失败意图持久化。
        let (mut document, record) = fixture(in_parent, true);
        document.interrupt_active_api_for_process(8).unwrap();
        document
            .begin_terminal_settlement(record.clone(), 9)
            .unwrap();
        // Reload proves that recovery uses persisted evidence rather than the old stream object.
        // 重载证明恢复使用持久证据而非旧流对象。
        let mut reloaded = SessionDocument::from_parts(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            document.events().to_vec(),
        )
        .unwrap();
        // Existing API and tool outcomes must not be rewritten by auxiliary settlement.
        // 辅助结算不得改写既有 API 和工具结果。
        let previous_events = reloaded.events().to_vec();
        // The composite failure publishes one revision after all physical work already ended.
        // 全部物理工作已结束后，组合失败仍只发布一个修订。
        let revision = reloaded.memory_revision();
        assert!(
            reloaded
                .fail_question_execution_for_terminal_settlement(&record, None, 10)
                .unwrap()
        );
        assert_eq!(reloaded.memory_revision(), revision + 1);
        assert_eq!(
            &reloaded.events()[..previous_events.len()],
            previous_events.as_slice()
        );
        assert_eq!(reloaded.active_turn().unwrap().closed, !in_parent);
        assert!(
            reloaded
                .terminal_settlement_has_terminal_fact(&record)
                .unwrap()
        );
        reloaded
            .resolve_terminal_settlement(&record.settlement_id, 11)
            .unwrap();
    }
}

/// Even a terminal captured API cannot authorize recovery after a replacement API has run.
/// 捕获的 API 即使已终态，也不能授权在替代 API 运行后进行恢复。
#[test]
fn question_failure_rejects_older_terminal_api() {
    // Both APIs belong to the same turn and purpose; their exact identities still matter.
    // 两个 API 属于同一回合及用途，但精确身份仍然重要。
    let (mut document, record) = fixture(true, true);
    document.interrupt_active_api_for_process(8).unwrap();
    document
        .start_api(
            ApiPurposeKind::QuestionExecution,
            ApiRequestSnapshot::new("openai", "model"),
            9,
        )
        .unwrap();
    document.interrupt_active_api_for_process(10).unwrap();
    document
        .begin_terminal_settlement(record.clone(), 11)
        .unwrap();
    // Failure must leave the replacement's facts and every document revision untouched.
    // 失败必须保持替代请求事实及文档全部修订不变。
    let before = document.clone();
    assert!(
        document
            .fail_question_execution_for_terminal_settlement(&record, None, 12)
            .is_err()
    );
    assert_eq!(document, before);
}

/// Human inputs stay queued throughout an auxiliary turn, including before its first physical API.
/// 人类输入在整个辅助回合中保持排队，包括首次物理 API 之前。
#[test]
fn question_execution_keeps_business_inputs_queued() {
    for with_api in [false, true] {
        // The target owns a real auxiliary execution, not an ordinary business turn.
        // 目标拥有真实辅助执行，而非普通业务回合。
        let (mut document, record) = fixture(false, with_api);
        // A real user's future task is durably accepted without changing auxiliary ownership.
        // 真实用户的未来任务被可靠接纳，不改变辅助所有权。
        let action = ClientActionId::try_new("queued-business").unwrap();
        document
            .enqueue_input(
                ClientInputSubmission::new(action.clone(), "New business task", vec![]),
                8,
            )
            .unwrap();
        // Capture both queue identity and the full document before the attempted promotion.
        // 尝试晋升前捕获队列身份及完整文档。
        let identity = document
            .queued_input_identity_for_action(&action)
            .unwrap()
            .unwrap();
        // Rejected insertion must leave events, metadata, ids and revisions unchanged.
        // 被拒绝的插入必须保持事件、元数据、身份及修订不变。
        let before = document.clone();
        assert!(matches!(
            document.promote_queued_input_to_pending_turn(identity, 9),
            Err(DomainError::CollaborationStateViolation {
                field: "question_execution.business_inputs"
            })
        ));
        assert_eq!(document, before);
        assert!(matches!(
            document.add_pending_in_turn_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("direct-business").unwrap(),
                    "Direct input",
                    vec![]
                ),
                9
            ),
            Err(DomainError::CollaborationStateViolation {
                field: "question_execution.business_inputs"
            })
        ));
        assert_eq!(document, before);
        if with_api {
            document.interrupt_active_api_for_process(10).unwrap();
        }
        // Host collaboration still has a legal stream lane; only human business promotion is prohibited.
        // 宿主协作仍拥有合法入流通道；仅禁止人类业务输入晋升。
        document
            .append_in_turn_guidance_before_api(
                ClientActionId::try_new("host-message").unwrap(),
                "Typed agent envelope".into(),
                11,
            )
            .unwrap();
        // Completing the auxiliary execution preserves the exact future business input.
        // 完成辅助执行时保留精确未来业务输入。
        let TerminalSettlementScope::QuestionExecution { execution_id, .. } = record.scope else {
            unreachable!()
        };
        document
            .complete_question_execution(CompleteQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new("complete-auxiliary").unwrap(),
                execution_id,
                outcome: CollaborationQuestionExecutionOutcome::Unanswered,
                completed_at_ms: 12,
            })
            .unwrap();
        assert!(document.active_turn().unwrap().closed);
        assert!(
            document
                .metadata
                .input_state
                .pending_in_turn_inputs
                .is_empty()
        );
        assert_eq!(
            document.metadata.input_state.queued_inputs,
            before.metadata.input_state.queued_inputs
        );
    }
}

/// Scheduling after auxiliary completion preserves FIFO, original receipts and single-revision publication.
/// 辅助完成后的调度保留 FIFO、原始回执及单修订发布。
#[test]
fn question_successor_is_atomic_and_replayable() {
    // Start with a real auxiliary owner and two distinct future user tasks.
    // 从真实辅助所有者和两个不同未来用户任务开始。
    let (mut document, record) = fixture(false, false);
    for action in ["first-task", "second-task"] {
        document
            .enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new(action).unwrap(),
                    action,
                    vec![],
                ),
                8,
            )
            .unwrap();
    }
    // The retained scope provides the exact predecessor identity.
    // 保留范围提供精确前驱身份。
    let TerminalSettlementScope::QuestionExecution { execution_id, .. } = record.scope else {
        unreachable!()
    };
    // Completion and successor scheduling have separate immutable receipts.
    // 完成与后继调度拥有独立不可变回执。
    let completion = CompleteQuestionExecutionInput {
        operation_id: CollaborationOperationId::try_new("complete-before-schedule").unwrap(),
        execution_id: execution_id.clone(),
        outcome: CollaborationQuestionExecutionOutcome::Unanswered,
        completed_at_ms: 9,
    };
    // Capture the original completion snapshot before the later scheduling fact exists.
    // 后续调度事实存在前捕获原完成快照。
    let completed = document
        .complete_question_execution(completion.clone())
        .unwrap()
        .into_value();
    // Model mode, historical prefix and queued identities must remain stable across scheduling.
    // 模型模式、历史前缀及排队身份必须跨调度保持稳定。
    let before = document.clone();
    // The same operation is reused after stop and serialized reload.
    // 停止和序列化重载后复用同一操作。
    let operation = CollaborationOperationId::try_new("schedule-successor").unwrap();
    // One outer atomic publication owns input, execution receipt and running control.
    // 一次外层原子发布拥有输入、执行回执及运行控制。
    let scheduled = document
        .schedule_question_queued_successor(execution_id.clone(), operation.clone(), 10)
        .unwrap()
        .unwrap()
        .into_value();
    assert_eq!(document.memory_revision(), before.memory_revision() + 1);
    assert_eq!(&document.events()[..before.events().len()], before.events());
    assert_eq!(document.metadata.mode, before.metadata.mode);
    assert_eq!(
        document.collaboration_run_control(),
        CollaborationRunControl::Running
    );
    // The first accepted input becomes a new business turn without reallocating its user identity.
    // 首条已接纳输入成为新业务回合，不重新分配用户身份。
    let successor = scheduled.execution.queued_successor.as_ref().unwrap();
    assert_eq!(
        successor.input_id,
        before.metadata.input_state.queued_inputs[0].input_id
    );
    assert_eq!(successor.client_action_id.as_str(), "first-task");
    assert_eq!(
        document.metadata.input_state.queued_inputs,
        before.metadata.input_state.queued_inputs[1..]
    );
    assert_eq!(document.active_turn().unwrap().turn_id, successor.turn_id);
    assert_eq!(document.active_turn().unwrap().origin, TurnOrigin::UserRoot);
    assert!(!document.active_turn().unwrap().closed);
    assert_eq!(
        document
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::TurnStats(_)))
            .count(),
        1
    );
    assert_eq!(
        document
            .complete_question_execution(completion)
            .unwrap()
            .into_value(),
        completed
    );
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("stop-successor").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: 11,
        })
        .unwrap();
    // Reconstruct from serialized authority rather than carrying the process-local receipt.
    // 从序列化权威重建，而非携带进程内回执。
    let mut reloaded = SessionDocument::from_parts(
        serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    // Exact retry returns the old scheduling receipt without resuming stopped work or consuming task two.
    // 精确重试返回旧调度回执，不恢复已停止工作或消费第二个任务。
    let stopped = reloaded.clone();
    assert_eq!(
        reloaded
            .schedule_question_queued_successor(execution_id, operation, 12)
            .unwrap()
            .unwrap()
            .into_value(),
        scheduled
    );
    assert_eq!(reloaded, stopped);
}

/// Auxiliary scheduling never substitutes for an unfinished, stopped or unsettled predecessor.
/// 辅助调度绝不替代未完成、已停止或尚待结算的前驱。
#[test]
fn question_successor_rejects_invalid_admission_and_empty_queue_is_noop() {
    for in_parent in [false, true] {
        // Parent-bound attempts are never permitted to open a new business task.
        // 绑定父回合的尝试绝不允许开启新业务任务。
        let (mut document, record) = fixture(in_parent, false);
        // Read the exact durable execution rather than inferring one from the active UI phase.
        // 读取精确可靠执行，不从活动 UI 阶段推断执行。
        let TerminalSettlementScope::QuestionExecution { execution_id, .. } = record.scope.clone()
        else {
            unreachable!()
        };
        // Scheduling before completion must not change even an empty queue.
        // 完成前的调度即使面对空队列也不得改变文档。
        let before = document.clone();
        assert!(
            document
                .schedule_question_queued_successor(
                    execution_id.clone(),
                    CollaborationOperationId::try_new("schedule").unwrap(),
                    8
                )
                .is_err()
        );
        assert_eq!(document, before);
        document
            .complete_question_execution(CompleteQuestionExecutionInput {
                operation_id: CollaborationOperationId::try_new("complete").unwrap(),
                execution_id: execution_id.clone(),
                outcome: CollaborationQuestionExecutionOutcome::Unanswered,
                completed_at_ms: 9,
            })
            .unwrap();
        // A completed standalone execution with an empty queue is a genuine no-op.
        // 已完成独立执行面对空队列时是真正的无操作。
        let completed = document.clone();
        let empty = document.schedule_question_queued_successor(
            execution_id.clone(),
            CollaborationOperationId::try_new("schedule").unwrap(),
            10,
        );
        if in_parent {
            assert!(empty.is_err());
        } else {
            assert!(empty.unwrap().is_none());
        }
        assert_eq!(document, completed);
        if !in_parent {
            document
                .enqueue_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("next-task").unwrap(),
                        "next",
                        vec![],
                    ),
                    11,
                )
                .unwrap();
            document.begin_terminal_settlement(record, 12).unwrap();
            // Pending storage repair keeps the new task queued and the same predecessor intact.
            // 待处理存储修复使新任务保持排队，同时维持相同前驱。
            let repairing = document.clone();
            assert!(
                document
                    .schedule_question_queued_successor(
                        execution_id.clone(),
                        CollaborationOperationId::try_new("schedule").unwrap(),
                        13
                    )
                    .is_err()
            );
            assert_eq!(document, repairing);
            document
                .resolve_terminal_settlement("failure-settlement", 14)
                .unwrap();
            document
                .set_collaboration_run_control(SetRunControlInput {
                    operation_id: CollaborationOperationId::try_new("stop-before-schedule")
                        .unwrap(),
                    state: CollaborationRunControl::Stopped,
                    changed_at_ms: 15,
                })
                .unwrap();
            // User stop wins over automatic post-QA scheduling.
            // 用户停止优先于自动答疑后调度。
            let stopped = document.clone();
            assert!(
                document
                    .schedule_question_queued_successor(
                        execution_id,
                        CollaborationOperationId::try_new("schedule").unwrap(),
                        16
                    )
                    .is_err()
            );
            assert_eq!(document, stopped);
        }
    }
}
