use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_session_core::v1::collaboration::*;
use vulcan_session_core::v1::document::SessionDocument;

/// Publishes immutable test candidates without accessing real user storage.
/// 发布不可变测试候选，不访问真实用户存储。
struct OwnershipTestBackend;

impl crate::session_runtime_registry_v1::SessionPersistenceBackend for OwnershipTestBackend {
    /// Returns the metadata supplied by `candidate` as the exact published result.
    /// 将 `candidate` 提供的元数据作为精确发布结果返回。
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

/// Creates and publishes a real idle-child auxiliary execution; returns its service, document, and record.
/// 创建并发布真实空闲子辅助执行；返回服务、文档及执行记录。
pub(crate) fn child_ownership_fixture() -> (
    SharedServiceBridgeState,
    Arc<SessionHandle>,
    CollaborationQuestionExecutionRecord,
) {
    // Each fixture uses its own canonical child identity.
    // 每个夹具使用独立规范子身份。
    let child_id = Uuid::new_v4();
    // The child document has no unfinished business turn.
    // 子文档没有未完成的业务回合。
    let mut document = SessionDocument::new(child_id.to_string(), "project", "Child", 1);
    // Target binds the child task ID independently of its root session.
    // 目标将子任务标识与根会话分别绑定。
    let target = CollaborationParticipant::new(
        child_id.to_string(),
        Uuid::new_v4().to_string(),
        "project",
        1,
        None,
        Some(SubagentTaskId::try_new(child_id.to_string()).unwrap()),
    )
    .unwrap();
    // One durable question supplies acceptance and auxiliary ownership identity.
    // 一个可靠问题提供接纳及辅助所有权身份。
    let question_id = CollaborationQuestionId::try_new("child-question").unwrap();
    document
        .accept_delivery(AcceptDeliveryInput {
            operation_id: CollaborationOperationId::try_new("accept").unwrap(),
            question_id: question_id.clone(),
            delivery_id: CollaborationDeliveryId::try_new("delivery").unwrap(),
            source: CollaborationParticipant::new("source", "source", "project", 1, None, None)
                .unwrap(),
            target: target.clone(),
            bound_turn_id: None,
            execution_purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            accepted_at_ms: 2,
        })
        .unwrap();
    // The Core reducer supplies a validated execution instead of constructing a fake record.
    // Core 归约器提供已校验执行，不手工构造伪记录。
    let execution = document
        .begin_question_execution(BeginQuestionExecutionInput {
            operation_id: CollaborationOperationId::try_new("begin").unwrap(),
            execution_id: CollaborationExecutionId::try_new("child-execution").unwrap(),
            question_id,
            participant: target,
            purpose: CollaborationExecutionPurpose::NaturallyIdleQuestion,
            parent_turn_id: None,
            started_at_ms: 3,
        })
        .unwrap()
        .into_value()
        .execution;
    // Canonical handle provides the same durability checks as production.
    // 规范句柄提供与生产相同的可靠性检查。
    let handle = SessionHandle::with_backend(child_id, document, Arc::new(OwnershipTestBackend));
    handle.flush_all().unwrap();
    // Service state is isolated from real sessions and Provider execution.
    // 服务状态与真实会话及 Provider 执行隔离。
    let service = crate::tests::test_support::test_service_state_with_parent_session(child_id);
    service
        .session_runtime_registry
        .register(handle.clone())
        .unwrap();
    (Arc::new(Mutex::new(service)), handle, execution)
}

/// Completes `execution` in memory only and returns after the real mutation publication.
/// 仅在内存中完成 `execution`，并在真实修改发布后返回。
fn complete_without_flush(
    handle: &SessionHandle,
    execution: &CollaborationQuestionExecutionRecord,
) {
    handle
        .mutate_in_memory(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.complete_question_execution(CompleteQuestionExecutionInput {
                    operation_id: CollaborationOperationId::try_new("complete").unwrap(),
                    execution_id: execution.execution_id.clone(),
                    outcome: CollaborationQuestionExecutionOutcome::Unanswered,
                    completed_at_ms: 4,
                })
            },
        )
        .unwrap();
}

/// Service admission survives unflushed completion and releases only after publication.
/// 服务准入跨未落盘完成保留，仅在发布后释放。
#[test]
fn child_question_owner_requires_durable_terminal_before_release() {
    // Production document and service own one admitted auxiliary execution.
    // 生产文档及服务持有一个已准入辅助执行。
    let (state, handle, execution) = child_ownership_fixture();
    // Cold TaskRuntime has no business state to replace.
    // 冷 TaskRuntime 没有可被替换的业务状态。
    let runtime = Arc::new(TaskRuntime::new());
    // Exact task admission moves into service ownership.
    // 精确任务准入移入服务所有权。
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    retain_child_question_execution_owner(&state, &execution, admission).unwrap();
    require_child_question_execution_owner(&state, handle.session_id(), &execution.execution_id)
        .unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    assert!(!runtime.is_task_running(handle.session_id()).unwrap());
    // Exact auxiliary scope is required even if a caller claims terminal persistence succeeded.
    // 即使调用者声称终态落盘成功，也必须具备精确辅助范围。
    let scope = TerminalSettlementScope::QuestionExecution {
        execution_id: execution.execution_id.clone(),
        api_call_id: None,
    };
    assert!(release_child_question_execution_owner(&state, handle.session_id(), &scope).is_err());
    complete_without_flush(&handle, &execution);
    assert!(release_child_question_execution_owner(&state, handle.session_id(), &scope).is_err());
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    handle.flush_all().unwrap();
    release_child_question_execution_owner(&state, handle.session_id(), &scope).unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_some()
    );
}

/// A failed publication check keeps exclusion while disabling dispatch until reliable cleanup.
/// 发布校验失败时保留互斥并禁用分发，直到可靠清理完成。
#[tokio::test]
async fn child_question_owner_transfer_failure_retains_cleanup_exclusion() {
    // Metadata may advance between durable question admission and ownership transfer.
    // 可靠问题准入与所有权转交之间，元数据可能继续推进。
    let (state, handle, execution) = child_ownership_fixture();
    handle
        .mutate_in_memory(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| document.rename_session("Unpublished title", 4),
        )
        .unwrap();
    // The task guard must outlive the rejected transfer and remain outside business running state.
    // 任务守卫必须跨被拒转交存续，并保持在业务运行状态之外。
    let runtime = Arc::new(TaskRuntime::new());
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    assert!(retain_child_question_execution_owner(&state, &execution, admission).is_err());
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
    assert!(!runtime.is_task_running(handle.session_id()).unwrap());
    assert!(
        require_child_question_execution_owner(
            &state,
            handle.session_id(),
            &execution.execution_id
        )
        .is_err()
    );
    // The production failure path persists the terminal and releases even a non-dispatchable owner.
    // 生产失败路径持久化终态，并释放不可分发的所有者。
    crate::agent_stream::fail_undispatched_question_execution(
        &state,
        handle.session_id(),
        &execution,
        "Injected transfer publication failure.",
    )
    .await
    .unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_some()
    );
    assert!(
        state
            .lock()
            .unwrap()
            .child_question_execution_owners
            .is_empty()
    );
    assert_eq!(
        handle
            .read(
                |document| document.collaboration_state().question_executions
                    [&execution.execution_id]
                    .outcome
            )
            .unwrap(),
        Some(CollaborationQuestionExecutionOutcome::Failed)
    );
}

/// The dedicated stop settles child QA during preparation without touching business running state.
/// 专用停止在准备阶段结算子答疑，不改变业务运行状态。
#[tokio::test]
async fn child_question_owner_stop_settles_preparation() {
    for (registered_kernel, started_api) in [(false, false), (true, false), (true, true)] {
        // Both pre-registration and admitted-kernel states retain the same real domain scope.
        // 登记前与已准入内核状态都保留相同真实领域范围。
        let (state, handle, execution) = child_ownership_fixture();
        let runtime = Arc::new(TaskRuntime::new());
        let admission = runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .unwrap();
        retain_child_question_execution_owner(&state, &execution, admission).unwrap();
        if registered_kernel {
            // Release submission admission before Stop requests its own exclusion.
            // Stop 请求自己的互斥前释放提交准入。
            let prompt_admission =
                crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
                    &state,
                    handle.session_id(),
                )
                .unwrap()
                .unwrap();
            let reservation_id = prompt_admission
                .existing_submission_authorization(handle.session_id())
                .unwrap()
                .reservation_id;
            crate::start_reserved_webui_session_kernel_turn(
                &mut state.lock().unwrap(),
                handle.session_id(),
                "child-qa-prepare",
                vulcan_session_core::ActiveTurnKind::QuestionExecution {
                    execution_id: execution.execution_id.clone(),
                },
                reservation_id,
            )
            .unwrap();
        }
        // An actual request permit exercises the same durable API interruption as a running provider.
        // 实际请求许可覆盖与运行中供应商相同的可靠 API 中断路径。
        let api_id = if started_api {
            use vulcan_session_core::v1::context::LockedBaselineProjection;
            use vulcan_session_core::v1::document::PrepareAndStartApiInput;
            use vulcan_session_core::v1::event::{ApiRequestSnapshot, ProviderProjectionProof};
            use vulcan_session_core::v1::projection::provider_visible_fingerprint;
            use vulcan_session_core::v1::{ApiPurposeKind, Seq};
            // Freeze the current projection frontier before creating the physical request.
            // 创建物理请求前冻结当前投影前沿。
            let input = handle
                .read(|document| {
                    // The final event includes the authoritative auxiliary execution start.
                    // 最后一条事件包含权威辅助执行启动。
                    let through_seq = document.events().last().unwrap().seq;
                    PrepareAndStartApiInput {
                        purpose: ApiPurposeKind::QuestionExecution,
                        request: ApiRequestSnapshot::new("openai", "model"),
                        request_budget_evidence: crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                        projection: ProviderProjectionProof {
                            segment_generation: document.segment_generation(),
                            document_revision: document.memory_revision(),
                            retained_from_seq: Seq::try_new(1).unwrap(),
                            through_seq,
                            fingerprint_sha256: provider_visible_fingerprint(document.events(), through_seq).unwrap(),
                        },
                        locked_baseline: LockedBaselineProjection::empty(),
                        continuation_action_id: None,
                        now_ms: 4,
                    }
                })
                .unwrap();
            Some(
                handle
                    .prepare_and_start_api_durable(input)
                    .unwrap()
                    .value
                    .api_call_id,
            )
        } else {
            None
        };
        // Keep a live registered future so the test also proves that Stop joins cancellation.
        // 保持已登记 Future 存活，使测试同时证明 Stop 等待取消完成。
        let mut stream_exit = if started_api {
            // Startup acknowledgement prevents an unpolled future from passing the cancellation check.
            // 启动确认避免尚未轮询的 Future 通过取消检查。
            let (started_sender, started_receiver) = tokio::sync::oneshot::channel();
            // This sender is released only when the live future is destroyed or reaches its unreachable end.
            // 只有存活 Future 被销毁或到达不可达结尾时才释放此发送端。
            let (exit_sender, exit_receiver) = tokio::sync::oneshot::channel::<()>();
            crate::state::spawn_registered_active_stream_task_with_mode(
                &state,
                handle.session_id(),
                Uuid::new_v4(),
                None,
                crate::state::ActiveStreamTaskRegistrationMode::RejectIfOccupied,
                async move {
                    started_sender.send(()).unwrap();
                    std::future::pending::<()>().await;
                    drop(exit_sender);
                },
            )
            .unwrap();
            started_receiver.await.unwrap();
            Some(exit_receiver)
        } else {
            None
        };
        if let Some(receiver) = stream_exit.as_mut() {
            assert_eq!(
                receiver.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Empty)
            );
        }
        assert!(
            crate::conversation_run_services::stop_owned_child_question_execution(
                &crate::ServiceEventEmitter::new(Vec::new()),
                &state,
                handle.session_id()
            )
            .await
            .unwrap()
        );
        if let Some(receiver) = stream_exit.as_mut() {
            assert_eq!(
                receiver.try_recv(),
                Err(tokio::sync::oneshot::error::TryRecvError::Closed)
            );
        }
        assert!(
            !state
                .lock()
                .unwrap()
                .active_stream_tasks
                .contains_key(&handle.session_id())
        );
        handle
            .read(|document| {
                assert_eq!(
                    document.collaboration_run_control(),
                    CollaborationRunControl::Stopped
                );
                assert_eq!(
                    document.collaboration_state().question_executions[&execution.execution_id]
                        .state,
                    CollaborationQuestionExecutionState::Completed
                );
                assert_eq!(document.memory_revision(), document.durable_revision());
                assert!(document.terminal_settlement_record().is_none());
                assert!(document.active_api().is_none());
                if let Some(api_id) = api_id {
                    // Stop must publish the exact request's user-interruption fact once.
                    // 停止必须且只能发布一次精确请求的用户中断事实。
                    let interruptions = document.events().iter().filter(|event| {
                        matches!(&event.event, vulcan_session_core::v1::event::EventBody::ApiStats(stats)
                            if stats.api_call_id == api_id
                                && stats.status == vulcan_session_core::v1::ApiCallStatus::Interrupted
                                && stats.end_reason == Some(vulcan_session_core::v1::ApiEndReason::UserInterrupted))
                    }).count();
                    assert_eq!(interruptions, 1);
                }
            })
            .unwrap();
        assert!(
            state
                .lock()
                .unwrap()
                .child_question_execution_owners
                .is_empty()
        );
        assert!(
            runtime
                .try_reserve_child_question_execution(handle.session_id())
                .unwrap()
                .is_some()
        );
        assert!(!runtime.is_task_running(handle.session_id()).unwrap());
    }
}

/// A conflicting business kernel cannot be stopped by a stale auxiliary owner.
/// 过期辅助所有者不能停止存在冲突的业务内核。
#[tokio::test]
async fn child_question_owner_stop_rejects_other_kernel_before_mutation() {
    // Keep exact QA document facts while deliberately replacing only the process kernel kind.
    // 保留精确答疑文档事实，同时刻意只替换进程内核类型。
    let (state, handle, execution) = child_ownership_fixture();
    let runtime = Arc::new(TaskRuntime::new());
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    retain_child_question_execution_owner(&state, &execution, admission).unwrap();
    {
        let prompt_admission =
            crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
                &state,
                handle.session_id(),
            )
            .unwrap()
            .unwrap();
        let reservation_id = prompt_admission
            .existing_submission_authorization(handle.session_id())
            .unwrap()
            .reservation_id;
        crate::start_reserved_webui_session_kernel_turn(
            &mut state.lock().unwrap(),
            handle.session_id(),
            "other-business",
            vulcan_session_core::ActiveTurnKind::UserPrompt,
            reservation_id,
        )
        .unwrap();
    }
    let revision = handle.read(|document| document.memory_revision()).unwrap();
    assert!(
        crate::conversation_run_services::stop_owned_child_question_execution(
            &crate::ServiceEventEmitter::new(Vec::new()),
            &state,
            handle.session_id()
        )
        .await
        .is_err()
    );
    assert_eq!(
        handle.read(|document| document.memory_revision()).unwrap(),
        revision
    );
    assert_eq!(
        handle
            .read(|document| document.collaboration_run_control())
            .unwrap(),
        CollaborationRunControl::NaturallyIdle
    );
    assert!(
        state
            .lock()
            .unwrap()
            .session_terminal_persistence_owners
            .is_empty()
    );
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
}

/// Business and stale auxiliary cleanup preserve the current child admission owner.
/// 业务及过期辅助清理保留当前子准入所有者。
#[test]
fn child_question_owner_rejects_wrong_identity_and_preserves_unrelated_cleanup() {
    // One live child owner remains while unrelated cleanup attempts run.
    // 无关清理执行期间，一个活动子所有者持续保留。
    let (state, handle, execution) = child_ownership_fixture();
    // Separate task identity cannot be substituted for the canonical child.
    // 独立任务身份不能替换规范子任务。
    let runtime = Arc::new(TaskRuntime::new());
    // Wrong guard is rejected without creating a service entry.
    // 错误守卫被拒绝，不创建服务条目。
    let wrong = runtime
        .try_reserve_child_question_execution(Uuid::new_v4())
        .unwrap()
        .unwrap();
    assert!(retain_child_question_execution_owner(&state, &execution, wrong).is_err());
    // Correct guard retains the exact child without enabling business completion waiting.
    // 正确守卫保留精确子任务，不启用业务完成等待。
    let admission = runtime
        .try_reserve_child_question_execution(handle.session_id())
        .unwrap()
        .unwrap();
    retain_child_question_execution_owner(&state, &execution, admission).unwrap();
    release_child_question_execution_owner(
        &state,
        handle.session_id(),
        &TerminalSettlementScope::BusinessTurn,
    )
    .unwrap();
    // A different auxiliary generation must neither validate dispatch nor remove the current owner.
    // 不同辅助代次既不能验证分发，也不能移除当前所有者。
    let stale_id = CollaborationExecutionId::try_new("stale-execution").unwrap();
    assert!(
        require_child_question_execution_owner(&state, handle.session_id(), &stale_id).is_err()
    );
    release_child_question_execution_owner(
        &state,
        handle.session_id(),
        &TerminalSettlementScope::QuestionExecution {
            execution_id: stale_id,
            api_call_id: None,
        },
    )
    .unwrap();
    require_child_question_execution_owner(&state, handle.session_id(), &execution.execution_id)
        .unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .is_none()
    );
}

/// Real success and failure settlement release service and TaskRuntime ownership together.
/// 真实成功与失败结算一并释放服务及 TaskRuntime 所有权。
#[tokio::test]
async fn child_question_owner_follows_production_terminal_settlement() {
    use crate::state::{SessionTerminalPersistenceOwner, SessionTerminalPersistenceOwnerGuard};
    for already_persisted in [false, true] {
        // Each production branch starts with its own durable auxiliary execution.
        // 每个生产分支从独立可靠辅助执行开始。
        let (state, handle, execution) = child_ownership_fixture();
        // Runtime guard must remain held until the production finalizer releases it.
        // 运行时守卫必须保持持有，直到生产终结器释放。
        let runtime = Arc::new(TaskRuntime::new());
        // Exact reservation is transferred before finalization begins.
        // 精确预留在终结开始前转交。
        let admission = runtime
            .try_reserve_child_question_execution(handle.session_id())
            .unwrap()
            .unwrap();
        retain_child_question_execution_owner(&state, &execution, admission).unwrap();
        // The terminal marker reproduces the real process admission boundary.
        // 终态标记复现真实进程准入边界。
        let marker = SessionTerminalPersistenceOwner::PromptPreparation(Uuid::new_v4());
        crate::state::register_session_terminal_persistence_owner(
            &mut state.lock().unwrap(),
            handle.session_id(),
            marker.clone(),
        )
        .unwrap();
        // Scope binds finalization to this auxiliary execution only.
        // 范围仅将终结绑定到本辅助执行。
        let scope = TerminalSettlementScope::QuestionExecution {
            execution_id: execution.execution_id.clone(),
            api_call_id: None,
        };
        if already_persisted {
            // A caller's success flag cannot substitute for actual durable completion.
            // 调用者的成功标志不能替代实际可靠完成。
            let premature_owner = SessionTerminalPersistenceOwnerGuard::adopt_registered(
                state.clone(),
                handle.session_id(),
                marker.clone(),
            )
            .unwrap();
            assert!(
                !crate::agent_stream::settle_stream_terminal_authority_with_scope(
                    &state,
                    handle.session_id(),
                    true,
                    "test_child_finish",
                    None,
                    premature_owner,
                    scope.clone(),
                )
                .await
            );
            require_child_question_execution_owner(
                &state,
                handle.session_id(),
                &execution.execution_id,
            )
            .unwrap();
            complete_without_flush(&handle, &execution);
            handle.flush_all().unwrap();
        }
        // The same registered marker is retained after rejected early cleanup.
        // 提前清理被拒绝后，原登记标记仍然保留。
        let owner = SessionTerminalPersistenceOwnerGuard::adopt_registered(
            state.clone(),
            handle.session_id(),
            marker,
        )
        .unwrap();
        assert!(
            crate::agent_stream::settle_stream_terminal_authority_with_scope(
                &state,
                handle.session_id(),
                already_persisted,
                "test_child_finish",
                None,
                owner,
                scope,
            )
            .await
        );
        assert!(
            state
                .lock()
                .unwrap()
                .session_terminal_persistence_owners
                .is_empty()
        );
        assert!(
            state
                .lock()
                .unwrap()
                .child_question_execution_owners
                .is_empty()
        );
        assert!(
            runtime
                .try_reserve_child_question_execution(handle.session_id())
                .unwrap()
                .is_some()
        );
        handle
            .read(|document| {
                assert_eq!(document.memory_revision(), document.durable_revision());
                assert_eq!(
                    document.collaboration_state().question_executions[&execution.execution_id]
                        .state,
                    CollaborationQuestionExecutionState::Completed
                );
            })
            .unwrap();
    }
}
