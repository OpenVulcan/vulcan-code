//! Persistence and concurrency tests for the actual session waiter, without a Provider.
//! 实际会话等待器的持久化与并发测试，不调用供应商。

use super::*;
use crate::tests::test_support::{TestHostRuntimeRoot, test_service_state_with_parent_session};
use vulcan_session_core::v1::collaboration::{
    AddCollaborationGrantInput, CollaborationGrantAuthority, CollaborationGrantId,
    RemoveCollaborationGrantInput,
};
use vulcan_session_core::v1::config::MetadataDocument;
use vulcan_session_core::v1::context::{RuntimeInitializationState, RuntimeLock};
use vulcan_session_core::v1::document::ClientInputSubmission;
use vulcan_session_core::v1::ids::ClientActionId;
use vulcan_workspace_store::workspace_store::{CreateSessionV1Request, create_indexed_session_v1};

mod lifecycle;
mod task_messages;

/// Cancelling a publication waiter must not let the real user revocation overtake its writer.
/// 取消发布等待者不能让真实用户撤销越过其写入器。
#[tokio::test]
async fn collaboration_lifecycle_write_cancelled_waiter_retains_grant_authority() {
    let (_root, invocation, target) = fixture(true).await;
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Notify].into_iter().collect(),
    )
    .await
    .unwrap();
    let source = invocation.source_handle.clone();
    let target_for_write = target.clone();
    let registry = invocation.registry.clone();
    let handles = vec![source.clone(), target.clone()];
    // Rendezvous inside the actual blocking owner; no scheduling sleep is used to infer entry.
    // 在实际阻塞所有者内部会合；不靠调度延迟推断已经进入。
    let (entered_tx, entered_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let writer = tokio::spawn(async move {
        lifecycle_write::with_canonical_handles(&registry, handles, move || {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let allowed = source_handle_grant_allows(
                &source,
                source.session_id(),
                target_for_write.session_id(),
                CollaborationGrantAction::Notify,
            )?;
            if !allowed {
                return Err(String::from("grant was revoked before protected write"));
            }
            let receipt = target_for_write
                .mutate_durable_if_changed(
                    SessionMutationAdmission::MetadataOnly,
                    SessionFlushReason::MetadataChanged,
                    |document| {
                        document
                            .set_draft("Protected publication", vec![], 3)
                            .map(session_change)
                    },
                )
                .map_err(|error| error.to_string())?;
            finished_tx.send(receipt.revision).unwrap();
            Ok(())
        })
        .await
    });
    entered_rx.await.unwrap();
    writer.abort();
    assert!(writer.await.unwrap_err().is_cancelled());
    let revoke = crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_string(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    );
    tokio::pin!(revoke);
    let premature = tokio::time::timeout(std::time::Duration::from_millis(40), &mut revoke).await;
    // Always release the blocked writer before asserting, including a regressed early revocation.
    // 即使撤销错误地提前返回，也在断言之前释放阻塞写入器。
    release_tx.send(()).unwrap();
    assert!(
        premature.is_err(),
        "revocation passed a live blocking writer: {premature:?}"
    );
    let revision = tokio::time::timeout(std::time::Duration::from_secs(5), finished_rx)
        .await
        .unwrap()
        .unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(5), &mut revoke)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        target.read(|document| document.durable_revision()).unwrap(),
        revision
    );
    assert!(
        !source_handle_grant_allows(
            &invocation.source_handle,
            invocation.source_session_id,
            target.session_id(),
            CollaborationGrantAction::Notify,
        )
        .unwrap()
    );
}

/// Opposite participant directions share one lock order, while stale duplicate handles are rejected.
/// 相反参与者方向共享一个锁顺序，同时拒绝过期重复句柄。
#[tokio::test]
async fn collaboration_lifecycle_write_orders_pairs_and_rejects_stale_owner() {
    let (_root, invocation, target) = fixture(false).await;
    let forward = lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![invocation.source_handle.clone(), target.clone()],
        || Ok(1),
    );
    let reverse = lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![target.clone(), invocation.source_handle.clone()],
        || Ok(2),
    );
    let (forward, reverse) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        tokio::join!(forward, reverse)
    })
    .await
    .unwrap();
    assert_eq!(forward.unwrap(), 1);
    assert_eq!(reverse.unwrap(), 2);
    let locator = target.storage_locator().unwrap();
    let stale = SessionHandle::new(
        target.session_id(),
        locator.clone(),
        load_session_v1_for_runtime(&locator).unwrap().document,
    );
    let error = lifecycle_write::with_canonical_handles(
        &invocation.registry,
        vec![target, stale],
        || -> Result<(), String> { panic!("stale owner ran its callback") },
    )
    .await
    .unwrap_err();
    assert!(error.contains("stale"));
}

/// Creates two indexed sessions in one isolated database; `cross_project` selects target scope.
/// 在隔离数据库创建两个索引会话；`cross_project` 决定目标项目范围。
/// Returns the storage owner, a service invocation fixture, and the canonical target handle.
/// 返回存储所有者、服务调用夹具和规范目标句柄。
async fn fixture(
    cross_project: bool,
) -> (TestHostRuntimeRoot, ValidatedInvocation, Arc<SessionHandle>) {
    // All persistent files are owned by this fixture, outside real application roots.
    // 所有持久文件归本夹具持有，位于真实应用数据根之外。
    let source_id = Uuid::new_v4();
    let root = TestHostRuntimeRoot::new(source_id);
    let controller = root.attached_host_runtime_handle().await;
    let source_locator = root.v1_storage_locator();
    let source = SessionHandle::new(
        source_id,
        source_locator.clone(),
        load_session_v1_for_runtime(&source_locator)
            .unwrap()
            .document,
    );
    let target_id = Uuid::new_v4();
    let project_id = if cross_project {
        "other-project"
    } else {
        source_locator.project_id()
    };
    let metadata = MetadataDocument::new(target_id.to_string(), project_id, "Wait target", 1);
    let runtime_lock = RuntimeLock {
        initialization_state: RuntimeInitializationState::FreshUninitialized,
        current_compaction_id: None,
        locked_sources: vec![],
    };
    create_indexed_session_v1(
        &controller,
        &CreateSessionV1Request {
            cleaned_history: None,
            project_id,
            storage_partition:
                vulcan_workspace_store::workspace_store::SESSION_V1_STORAGE_PARTITION,
            date_bucket: "1970-01-01",
            metadata: &metadata,
            runtime_lock: &runtime_lock,
            compactions: &[],
            events: &[],
        },
    )
    .await
    .unwrap();
    let locator = locate_optional_session_v1_storage(&controller, &target_id.to_string())
        .await
        .unwrap()
        .unwrap();
    let target = SessionHandle::new(
        target_id,
        locator.clone(),
        load_session_v1_for_runtime(&locator).unwrap().document,
    );
    let mut service = test_service_state_with_parent_session(source_id);
    service.controller_runtime = Some(controller.clone());
    let registry = service.session_runtime_registry.clone();
    registry.register(source.clone()).unwrap();
    registry.register(target.clone()).unwrap();
    let invocation = ValidatedInvocation {
        state: Arc::new(Mutex::new(service)),
        controller_runtime: controller,
        registry,
        source_session_id: source_id,
        source_handle: source,
        source_locator,
        source_turn_id: 1,
        source_api_call_id: None,
        source_tool_id: WAIT_SESSION_TOOL_ID.into(),
        operation_id: "wait-operation".into(),
    };
    (root, invocation, target)
}

/// Builds a finite observation request for `target`, preserving the caller's exact cursor.
/// 为 `target` 构建有限观察请求，保留调用者的精确游标。
fn args(target: &SessionHandle, after_revision: Option<u64>, timeout_ms: u64) -> WaitSessionArgs {
    WaitSessionArgs {
        session_id: target.session_id().to_string(),
        after_revision,
        timeout_ms,
    }
}

/// An unflushed edit cannot wake a durable waiter, and timeout does not mutate either session.
/// 未刷写修改不能唤醒可靠等待；超时不修改任何会话。
#[tokio::test]
async fn session_wait_ignores_unpublished_revision_and_timeout_has_no_side_effects() {
    let (_root, invocation, target) = fixture(false).await;
    let before = target
        .storage_locator()
        .unwrap()
        .event_window_state()
        .unwrap();
    target
        .mutate_in_memory(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .set_draft("Unpublished", vec![], 2)
                    .map(IdempotentMutation::into_value)
            },
        )
        .unwrap();
    let memory_revision = target.read(|document| document.memory_revision()).unwrap();
    assert!(memory_revision > before.document_revision);
    let source_revision = invocation
        .source_handle
        .read(|document| document.memory_revision())
        .unwrap();
    let result = wait_session(&invocation, args(&target, None, 30))
        .await
        .unwrap();
    assert_eq!(result["status"], "timeout");
    assert_eq!(result["after_revision"], before.document_revision);
    assert_eq!(result["document_revision"], before.document_revision);
    assert_eq!(
        target.read(|document| document.memory_revision()).unwrap(),
        memory_revision
    );
    assert_eq!(
        invocation
            .source_handle
            .read(|document| document.memory_revision())
            .unwrap(),
        source_revision
    );
    target.flush_all().unwrap();
    let result = wait_session(
        &invocation,
        args(&target, Some(before.document_revision), 30),
    )
    .await
    .unwrap();
    assert_eq!(result["wait_status"], "changed");
    assert_eq!(result["document_revision"], memory_revision);
    assert_eq!(
        result["next_seq"],
        target
            .storage_locator()
            .unwrap()
            .event_window_state()
            .unwrap()
            .next_seq
    );
}

/// A pending real waiter observes a later publication without starting any business task.
/// 真实等待器观察后续发布，不启动业务任务。
#[tokio::test]
async fn session_wait_returns_later_durable_publication() {
    let (_root, invocation, target) = fixture(false).await;
    let baseline = target.read(|document| document.durable_revision()).unwrap();
    let request = args(&target, Some(baseline), 5000);
    let waiting = wait_session(&invocation, request);
    tokio::pin!(waiting);
    tokio::select! {
        result = &mut waiting => panic!("wait ended before publication: {result:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(80)) => {}
    }
    target
        .set_draft("Changed while waiting".into(), vec![], vec![], vec![], 2)
        .unwrap();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(result["wait_status"], "changed");
    assert_eq!(result["after_revision"], baseline);
    assert!(result["document_revision"].as_u64().unwrap() > baseline);
    assert!(
        target
            .read(|document| document.active_turn().is_none())
            .unwrap()
    );
}

/// Revocation during an explicit wait rejects further cross-project observations.
/// 显式等待期间撤销授权后拒绝后续跨项目观察。
#[tokio::test]
async fn session_wait_rechecks_read_grant_after_revocation() {
    let (_root, invocation, target) = fixture(true).await;
    assert!(
        wait_session(&invocation, args(&target, None, 30))
            .await
            .unwrap_err()
            .contains("grant")
    );
    let grant_id = CollaborationGrantId::try_new("read-grant").unwrap();
    invocation
        .source_handle
        .mutate_in_memory_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .add_collaboration_grant(AddCollaborationGrantInput {
                        operation_id: CollaborationOperationId::try_new("grant-add").unwrap(),
                        grant_id: grant_id.clone(),
                        authority: CollaborationGrantAuthority::LocalUser,
                        source_session_id: invocation.source_session_id.to_string(),
                        target_session_id: target.session_id().to_string(),
                        actions: [CollaborationGrantAction::Read].into_iter().collect(),
                        created_at_ms: 2,
                    })
                    .map(session_change)
            },
        )
        .unwrap();
    // The shared resolver now rejects unpublished authority before the wait-specific permission check.
    // 共享定位器现在会在等待专用权限检查之前拒绝尚未发布的授权。
    assert!(
        wait_session(&invocation, args(&target, None, 30))
            .await
            .unwrap_err()
            .contains("cross-project collaboration requires an active source-owned grant")
    );
    assert!(
        !source_handle_grant_allows(
            &invocation.source_handle,
            invocation.source_session_id,
            target.session_id(),
            CollaborationGrantAction::Read,
        )
        .unwrap()
    );
    invocation.source_handle.flush_all().unwrap();
    assert!(
        source_handle_grant_allows(
            &invocation.source_handle,
            invocation.source_session_id,
            target.session_id(),
            CollaborationGrantAction::Read,
        )
        .unwrap()
    );
    let waiting = wait_session(&invocation, args(&target, None, 5000));
    tokio::pin!(waiting);
    tokio::select! {
        result = &mut waiting => panic!("authorized wait ended before revocation: {result:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(80)) => {}
    }
    invocation
        .source_handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .remove_collaboration_grant(RemoveCollaborationGrantInput {
                        operation_id: CollaborationOperationId::try_new("grant-remove").unwrap(),
                        grant_id,
                        revoked_at_ms: 3,
                    })
                    .map(session_change)
            },
        )
        .unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap_err()
            .contains("Read grant")
    );
}

/// A real persisted ToolCall reaches the production native port; spoofed tool identities fail.
/// 真实持久 ToolCall 通过生产原生端口执行；伪造工具身份被拒绝。
#[tokio::test]
async fn session_wait_native_dispatch_requires_exact_tool_identity() {
    use vulcan_session_core::v1::context::LockedBaselineProjection;
    use vulcan_session_core::v1::document::PrepareAndStartApiInput;
    use vulcan_session_core::v1::event::{
        ApiRequestSnapshot, ProviderProjectionProof, ToolPayload,
    };
    use vulcan_session_core::v1::ids::RequestStreamId;
    use vulcan_session_core::v1::projection::provider_visible_fingerprint;

    // Admit the source business/API/tool using the same Core operations as production dispatch.
    // 使用生产分发同样的 Core 操作准入来源业务、API 与工具。
    let (_root, invocation, target) = fixture(false).await;
    invocation
        .source_handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("native-wait-root").unwrap(),
                "Observe another session",
                vec![],
            ),
            2,
        )
        .unwrap();
    let input = invocation
        .source_handle
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms: 3,
            }
        })
        .unwrap();
    let api_call_id = invocation
        .source_handle
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    let request_stream_id = RequestStreamId::try_new("native-wait-stream").unwrap();
    let tool_call_id = invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    request_stream_id.clone(),
                    WAIT_SESSION_TOOL_ID,
                    ToolPayload::empty(),
                    None,
                    4,
                )
            },
        )
        .unwrap()
        .value;
    let tool_seq = invocation
        .source_handle
        .read(|document| {
            document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    EventBody::ToolCall(tool) if tool.tool_call_id == tool_call_id => {
                        Some(event.seq)
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    let context = VulcanToolInvocationContext {
        turn_id: vulcan_session_core::v1::ids::TurnId::try_new(1).expect("synthetic turn"),
        session_id: invocation.source_session_id.to_string(),
        request_stream_id,
        api_call_id,
        tool_call_id,
        tool_seq,
        controller_tool_call_id: "native-wait-tool".into(),
        provider_tool_call_id: None,
    };
    let runtime = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        crate::ServiceEventEmitter::new(vec![]),
    );
    let result = runtime
        .invoke(
            context.clone(),
            AgentCollaborationCommand::WaitSession(args(&target, None, 1)),
        )
        .await
        .unwrap();
    assert_eq!(result["status"], "timeout");
    assert_eq!(result["session_id"], target.session_id().to_string());
    assert_eq!(
        result["operation_id"],
        stable_operation_id(&context, WAIT_SESSION_TOOL_ID)
    );
    let mut spoofed = context.clone();
    spoofed.provider_tool_call_id = Some("another-tool".into());
    assert!(
        runtime
            .invoke(
                spoofed,
                AgentCollaborationCommand::WaitSession(args(&target, None, 1))
            )
            .await
            .unwrap_err()
            .contains("provider_tool_call_id")
    );
    // Removing the canonical owner invalidates a previously validated long-wait fixture.
    // 移除规范所有者后，之前已校验的长等待夹具也失效。
    invocation
        .registry
        .unregister_lifecycle_session(invocation.source_session_id)
        .unwrap();
    assert!(
        wait_session(&invocation, args(&target, None, 1))
            .await
            .unwrap_err()
            .contains("source owner")
    );
}

/// An incoming question interrupts observation while retaining the original target and cursor.
/// 入站问题中断观察，同时保留原始目标和游标。
#[tokio::test]
async fn session_wait_returns_incoming_question_without_consuming_it() {
    let (_root, mut invocation, target) = fixture(false).await;
    // Use real business turns and domain question/admission records on both sides.
    // 双方使用真实业务回合及领域问题和接纳记录。
    let waiting_turn = invocation
        .source_handle
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("waiter-root").unwrap(),
                "Wait for another session",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value;
    let asking_turn = target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("asker-root").unwrap(),
                "Ask for direction",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value;
    invocation.source_turn_id = waiting_turn.value();
    let asking = CollaborationParticipant::new(
        target.session_id().to_string(),
        target.session_id().to_string(),
        invocation.source_locator.project_id(),
        1,
        Some(asking_turn),
        None,
    )
    .unwrap();
    let waiting = CollaborationParticipant::new(
        invocation.source_session_id.to_string(),
        invocation.source_session_id.to_string(),
        invocation.source_locator.project_id(),
        1,
        Some(waiting_turn),
        None,
    )
    .unwrap();
    let question_id =
        CollaborationQuestionId::try_new(format!("cq1/v1:{}:reverse", target.session_id()))
            .unwrap();
    let delivery_id = CollaborationDeliveryId::try_new("reverse-delivery").unwrap();
    target
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .create_question(CreateQuestionInput {
                        operation_id: CollaborationOperationId::try_new("reverse-question")
                            .unwrap(),
                        question_id: question_id.clone(),
                        delivery_id: delivery_id.clone(),
                        source: asking.clone(),
                        target: waiting.clone(),
                        body: "Which direction?".into(),
                        execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                        created_at_ms: 3,
                    })
                    .map(session_change)
            },
        )
        .unwrap();
    invocation
        .source_handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            |document| {
                document
                    .accept_delivery(AcceptDeliveryInput {
                        operation_id: CollaborationOperationId::try_new("reverse-accept").unwrap(),
                        question_id: question_id.clone(),
                        delivery_id: delivery_id.clone(),
                        source: asking,
                        target: waiting,
                        bound_turn_id: Some(waiting_turn),
                        execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                        accepted_at_ms: 4,
                    })
                    .map(session_change)
            },
        )
        .unwrap();
    let baseline = target.read(|document| document.durable_revision()).unwrap();
    let source_revision = invocation
        .source_handle
        .read(|document| document.memory_revision())
        .unwrap();
    let result = wait_session(&invocation, args(&target, Some(baseline), 5000))
        .await
        .unwrap();
    assert_eq!(result["wait_status"], "incoming");
    assert_eq!(result["waited_session_id"], target.session_id().to_string());
    assert_eq!(result["after_revision"], baseline);
    assert_eq!(result["question"], "Which direction?");
    assert_eq!(result["question_id"], question_id.as_str());
    invocation
        .source_handle
        .read(|document| {
            assert_eq!(document.memory_revision(), source_revision);
            assert_eq!(
                document.collaboration_state().inbound_deliveries[&delivery_id].state,
                CollaborationDeliveryState::Accepted
            );
        })
        .unwrap();
}
