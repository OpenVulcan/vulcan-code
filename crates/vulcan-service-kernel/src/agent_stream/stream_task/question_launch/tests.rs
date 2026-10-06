//! Domain-backed launch guards; no provider or user data is used.
//! 基于领域的启动守卫；不使用供应商或用户数据。

use super::*;

/// Accepts immutable test publications without using a real user session directory.
/// 接纳不可变测试发布，不使用真实用户会话目录。
struct QuestionDispatchTestBackend;

impl crate::session_runtime_registry_v1::SessionPersistenceBackend for QuestionDispatchTestBackend {
    /// Returns the exact metadata from `candidate` as the published persistence boundary.
    /// 返回 `candidate` 中的精确元数据作为已发布持久化边界。
    fn flush(
        &self,
        candidate: &vulcan_session_core::v1::document::FlushCandidate,
    ) -> Result<
        vulcan_workspace_store::workspace_store::SessionV1FlushResult,
        vulcan_workspace_store::workspace_store::SessionV1StoreError,
    > {
        Ok(
            vulcan_workspace_store::workspace_store::SessionV1FlushResult {
                written_file_count: candidate.dirty.len(),
                published_metadata: Some(candidate.metadata.clone()),
            },
        )
    }
}

/// Pre-kernel dispatch failure closes only the admitted QA scope and releases its terminal marker.
/// 内核登记前的分发失败只关闭已准入答疑范围，并释放其终态标记。
#[tokio::test]
async fn undispatched_question_failure_preserves_business_scope() {
    for in_parent in [false, true] {
        // The production service registry owns the same domain execution used by the failure helper.
        // 生产服务注册表拥有与失败助手使用的同一领域执行。
        let (document, session_id, execution) = launch_fixture(in_parent);
        let expected_control = document.collaboration_run_control();
        let service_state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        let handle = crate::session_runtime_registry_v1::SessionHandle::with_backend(
            session_id,
            document,
            Arc::new(QuestionDispatchTestBackend),
        );
        service_state
            .session_runtime_registry
            .register(handle.clone())
            .unwrap();
        let state = Arc::new(Mutex::new(service_state));
        let admission =
            crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
                &state, session_id,
            )
            .unwrap()
            .unwrap();
        fail_undispatched_question_execution(
            &state,
            session_id,
            &execution,
            "Injected kernel registration failure.",
        )
        .await
        .unwrap();
        handle
            .read(|document| {
                assert_eq!(document.collaboration_run_control(), expected_control);
                assert_eq!(document.active_turn().unwrap().closed, !in_parent);
                assert_eq!(
                    document.collaboration_state().question_executions[&execution.execution_id]
                        .outcome,
                    Some(CollaborationQuestionExecutionOutcome::Failed)
                );
                assert!(document.terminal_settlement_record().is_none());
                assert_eq!(document.memory_revision(), document.durable_revision());
            })
            .unwrap();
        assert!(
            !state
                .lock()
                .unwrap()
                .session_terminal_persistence_owners
                .contains_key(&session_id)
        );
        drop(admission);
    }
}

/// Pre-kernel failure cannot terminalize a different live kernel or leave a spurious terminal owner.
/// 内核登记前失败不能终结另一个活动内核，也不能留下虚假终态所有者。
#[tokio::test]
async fn undispatched_question_failure_rejects_active_kernel() {
    // Deliberately replace the process owner while retaining the original durable QA fixture.
    // 保留原可靠答疑夹具，同时刻意设置另一个进程所有者。
    let (document, session_id, execution) = launch_fixture(true);
    let before_revision = document.memory_revision();
    let service_state =
        crate::tests::test_support::test_service_state_with_parent_session(session_id);
    let handle = crate::session_runtime_registry_v1::SessionHandle::with_backend(
        session_id,
        document,
        Arc::new(QuestionDispatchTestBackend),
    );
    service_state
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    let state = Arc::new(Mutex::new(service_state));
    let admission =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &state, session_id,
        )
        .unwrap()
        .unwrap();
    let authorization = admission
        .existing_submission_authorization(session_id)
        .unwrap();
    crate::start_reserved_webui_session_kernel_turn(
        &mut state.lock().unwrap(),
        session_id,
        "other-kernel",
        vulcan_session_core::ActiveTurnKind::UserPrompt,
        authorization.reservation_id,
    )
    .unwrap();
    assert!(
        fail_undispatched_question_execution(&state, session_id, &execution, "Stale cleanup.")
            .await
            .is_err()
    );
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        before_revision
    );
    assert!(
        !state
            .lock()
            .unwrap()
            .session_terminal_persistence_owners
            .contains_key(&session_id)
    );
}
use vulcan_session_core::v1::collaboration::*;
use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission};

/// Creates a real accepted question and running execution for either supported turn purpose.
/// 为两种支持的回合用途创建真实已接纳问题和运行中执行。
/// `in_parent` selects the retained business turn. Returns the exact document, session and execution.
/// `in_parent` 选择保留的业务回合。返回精确文档、会话和执行。
fn launch_fixture(
    in_parent: bool,
) -> (
    SessionDocument,
    SessionId,
    CollaborationQuestionExecutionRecord,
) {
    // session_id isolates each target from every other test.
    // session_id 将每个目标与其他测试隔离。
    let session_id = SessionId::new_v4();
    // document carries the real single-version domain contract.
    // document 承载真实单一版本领域契约。
    let mut document =
        SessionDocument::new(session_id.to_string(), "project", "Question launch", 1);
    // parent_turn exists only when the question attaches to existing business.
    // parent_turn 仅在问题附着既有业务时存在。
    let parent_turn = if in_parent {
        let turn = document
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root").unwrap(),
                    "Business task",
                    vec![],
                ),
                2,
            )
            .unwrap()
            .into_value();
        document
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("running").unwrap(),
                state: CollaborationRunControl::Running,
                changed_at_ms: 3,
            })
            .unwrap();
        Some(turn)
    } else {
        None
    };
    // purpose and target encode the exact same logical binding.
    // purpose 与 target 编码完全相同的逻辑绑定。
    let purpose = if in_parent {
        CollaborationExecutionPurpose::InTurnQuestion
    } else {
        CollaborationExecutionPurpose::NaturallyIdleQuestion
    };
    let target = CollaborationParticipant::new(
        session_id.to_string(),
        session_id.to_string(),
        "project",
        1,
        parent_turn,
        None,
    )
    .unwrap();
    // question_id links durable acceptance and execution.
    // question_id 关联可靠接纳与执行。
    let question_id = CollaborationQuestionId::try_new("question").unwrap();
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("accept").unwrap(),
            question_id: question_id.clone(),
            delivery_id: CollaborationDeliveryId::try_new("delivery").unwrap(),
            source: CollaborationParticipant::new("source", "source", "project", 1, None, None)
                .unwrap(),
            target: target.clone(),
            bound_turn_id: parent_turn,
            execution_purpose: purpose,
            accepted_at_ms: 4,
        })
        .unwrap();
    // execution is returned by the reducer rather than constructed as an unverified record.
    // execution 由归约器返回，并非手工构造的未验证记录。
    let execution = document
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin").unwrap(),
            execution_id: CollaborationExecutionId::try_new("execution").unwrap(),
            question_id,
            participant: target,
            purpose,
            parent_turn_id: parent_turn,
            started_at_ms: 5,
        })
        .unwrap()
        .into_value()
        .execution;
    (document, session_id, execution)
}

/// Both business-bound and idle QA launches retain their exact auxiliary terminal scope.
/// 业务绑定与空闲答疑启动均保留精确辅助终态范围。
#[test]
fn question_launch_preserves_scope_for_both_turn_purposes() {
    for in_parent in [false, true] {
        let (document, session_id, execution) = launch_fixture(in_parent);
        let scope = question_launch_restart_state(&document, session_id, &execution)
            .unwrap()
            .terminal_scope();
        assert_eq!(
            scope,
            TerminalSettlementScope::QuestionExecution {
                execution_id: execution.execution_id,
                api_call_id: None,
            }
        );
    }
}

/// A changed session or execution snapshot cannot reuse another launch owner's authority.
/// 改变后的会话或执行快照不能复用其他启动所有者的权限。
#[test]
fn question_launch_rejects_changed_identity_without_mutation() {
    let (document, session_id, mut execution) = launch_fixture(false);
    let before = (document.memory_revision(), document.events().to_vec());
    assert!(question_launch_restart_state(&document, SessionId::new_v4(), &execution).is_err());
    execution.started_at_ms += 1;
    assert!(question_launch_restart_state(&document, session_id, &execution).is_err());
    assert_eq!(
        before,
        (document.memory_revision(), document.events().to_vec())
    );
}

/// Explicit stop rejects launch even while the original execution and logical turn are retained.
/// 即使原执行和逻辑回合仍保留，显式停止也拒绝启动。
#[test]
fn question_launch_rejects_stopped_targets() {
    for control in [
        CollaborationRunControl::Stopped,
        CollaborationRunControl::MainAndSubagents,
    ] {
        let (mut document, session_id, execution) = launch_fixture(true);
        document
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("stop").unwrap(),
                state: control,
                changed_at_ms: 6,
            })
            .unwrap();
        assert!(question_launch_restart_state(&document, session_id, &execution).is_err());
    }
}

/// Completing an unanswered auxiliary execution prevents replay from creating another stream.
/// 完成未回答的辅助执行后，重放不能创建另一条流。
#[test]
fn question_launch_rejects_completed_execution() {
    let (mut document, session_id, execution) = launch_fixture(false);
    document
        .complete_question_execution(CompleteQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("complete").unwrap(),
            execution_id: execution.execution_id.clone(),
            outcome: CollaborationQuestionExecutionOutcome::Unanswered,
            completed_at_ms: 6,
        })
        .unwrap();
    assert!(question_launch_restart_state(&document, session_id, &execution).is_err());
}

/// MainOnly admits the retained business turn's question launch; both stop modes refuse it unchanged.
/// MainOnly 允许其保留业务回合的问题启动；两种停止模式都在不修改任何事实的前提下拒绝。
///
/// # Notes
/// 说明
/// This drives the same gate the production question stream task calls before it touches any child
/// configuration or provider service, so a MainOnly admission here is what lets a subagent question
/// resume the retained parent turn instead of merely being stored.
/// 本测试驱动生产问题流任务在接触任何子配置或供应商服务之前调用的同一闸门，因此此处的 MainOnly
/// 准入正是"子代理问题恢复其保留父回合"而非仅被存储的依据。
/// The receiving parent is stopped for main execution in exactly one of these states, so the accepted
/// delivery and its bound execution must survive every refusal byte-for-byte.
/// 三种状态中恰好有一种停止主执行，因此每次拒绝后已接纳投递及其绑定执行都必须逐字保留。
#[test]
fn question_launch_admits_main_only_resume_and_refuses_stop_states_unchanged() {
    for (control, admitted) in [
        (CollaborationRunControl::MainOnly, true),
        (CollaborationRunControl::Stopped, false),
        (CollaborationRunControl::MainAndSubagents, false),
    ] {
        let (mut document, session_id, execution) = launch_fixture(true);
        document
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new(format!("control-{control:?}"))
                    .unwrap(),
                state: control,
                changed_at_ms: 6,
            })
            .unwrap();
        // Every state retains the same open parent turn and the same admitted QA execution.
        // 三种状态都保留同一个开放父回合与同一条已准入答疑执行。
        let before = (
            document.active_turn().map(|turn| turn.turn_id),
            document
                .collaboration_state()
                .question_executions
                .get(&execution.execution_id)
                .cloned(),
            document
                .collaboration_state()
                .inbound_deliveries
                .values()
                .map(|delivery| (delivery.state, delivery.bound_turn_id))
                .collect::<Vec<_>>(),
            document.durable_revision(),
        );
        let result = question_launch_restart_state(&document, session_id, &execution);
        if admitted {
            // MainOnly resumes the retained parent turn rather than opening a separate business turn.
            // MainOnly 恢复其保留的父回合，而不是另开一个业务回合。
            assert!(
                result.is_ok(),
                "MainOnly must admit its retained turn's question launch: {result:?}"
            );
            let admission = crate::agent_collaboration_execution::admit_collaboration_execution(
                control,
                execution.purpose,
            )
            .unwrap();
            assert!(
                admission.wakes_retained_parent_turn,
                "MainOnly must declare that the retained parent turn is woken"
            );
            assert!(
                admission.suppresses_business_completion,
                "the resumed auxiliary execution must not create a second business completion"
            );
        } else {
            let error = result.expect_err("an explicit stop must refuse the question launch");
            assert!(
                !error.is_empty(),
                "a refused question launch must report a concrete reason"
            );
        }
        // The gate is a pure reader: neither admission nor refusal may change any durable fact.
        // 该闸门是纯读取方：准入与拒绝都不得改变任何可靠事实。
        let after = (
            document.active_turn().map(|turn| turn.turn_id),
            document
                .collaboration_state()
                .question_executions
                .get(&execution.execution_id)
                .cloned(),
            document
                .collaboration_state()
                .inbound_deliveries
                .values()
                .map(|delivery| (delivery.state, delivery.bound_turn_id))
                .collect::<Vec<_>>(),
            document.durable_revision(),
        );
        assert_eq!(
            before, after,
            "the question launch gate must not mutate the document in state {control:?}"
        );
    }
}
