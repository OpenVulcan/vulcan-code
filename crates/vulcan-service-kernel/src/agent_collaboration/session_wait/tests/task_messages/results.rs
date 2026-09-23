//! Real-storage result return and cross-project receipt-repair integration tests.
//! 真实存储结果回传与跨项目回执修复集成测试。

use super::*;

/// Commits a real completed API and final assistant through the canonical session handle.
/// 通过规范会话句柄提交真实完成 API 和最终助手正文。
/// `handle` keeps its logical business turn open for the separate completion decision.
/// `handle` 保持逻辑业务回合开放，供独立完成判断使用。
pub(super) fn finish_business_response(handle: &Arc<SessionHandle>) {
    use vulcan_session_core::v1::UsageSnapshot;
    use vulcan_session_core::v1::document::ApiCompletion;
    use vulcan_session_core::v1::event::{
        ApiCallStatus, ApiEndReason, ApiTimings, AssistantSemanticStatus, ContinuationEligibility,
    };
    let api = start_api(handle);
    handle
        .update_assistant_text(
            api,
            "Business response",
            AssistantSemanticStatus::Completed,
            8,
        )
        .unwrap();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ApiCompleted,
            |document| {
                document.complete_api_group(
                    api,
                    ApiCompletion {
                        request: ApiRequestSnapshot::new("openai", "model"),
                        usage: UsageSnapshot::default(),
                        status: ApiCallStatus::Completed,
                        end_reason: Some(ApiEndReason::CompletedSuccessfully),
                        error: None,
                        continuation_eligibility: ContinuationEligibility::Forbidden,
                        timing: ApiTimings::default(),
                        provider_source: None,
                        completed_at_ms: 9,
                    },
                )
            },
        )
        .unwrap();
}

/// Late steering cannot close with an old answer; its next actual business request permits completion.
/// 迟到方向不能用旧回答结束；下一次实际业务请求允许完成。
#[tokio::test]
async fn session_collaboration_continuation_gates_late_steering() {
    use crate::session_runtime_registry_v1::SuccessfulTurnSettlement;
    let (_root, invocation, target, context) = message_fixture(false).await;
    finish_business_response(&target);
    send(
        &invocation,
        &target,
        context,
        SessionMessageDeliveryMode::Interrupt,
        "Review the new direction",
    )
    .await
    .unwrap();
    let original_turn = target
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    assert!(matches!(
        target.complete_successful_turn(10).await.unwrap().value,
        SuccessfulTurnSettlement::Collaboration
    ));
    let authority = target
        .read(|document| document.pending_collaboration_continuation().unwrap())
        .unwrap();
    assert_eq!(authority.turn_id, original_turn);
    project_collaboration_before_api(&invocation.state, &target, 11)
        .await
        .unwrap();
    assert!(matches!(
        target.complete_successful_turn(12).await.unwrap().value,
        SuccessfulTurnSettlement::Collaboration
    ));
    finish_business_response(&target);
    assert!(matches!(
        target.complete_successful_turn(13).await.unwrap().value,
        SuccessfulTurnSettlement::Completed(_)
    ));
    let loaded = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(loaded.pending_collaboration_continuation().is_none());
    assert_eq!(loaded.collaboration_state().task_results.len(), 1);
}

/// Runs the production continuation preparation on a worker with the production stack budget.
/// 在使用生产栈预算的工作线程运行生产续跑准备。
#[test]
fn session_collaboration_continuation_dispatches_and_settles_preparation_failure() {
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    executor
        .block_on(executor.spawn(collaboration_continuation_dispatch_scenario()))
        .unwrap();
}

/// A late result reaches real preparation without a new business turn; unavailable Provider failure settles it.
/// 迟到结果进入真实准备且不新建业务回合；供应商不可用失败会结算该回合。
async fn collaboration_continuation_dispatch_scenario() {
    use crate::session_runtime_registry_v1::SuccessfulTurnSettlement;
    use vulcan_agent_runtime::workbench_prompt_turn::{
        SharedPromptTurnRuntimeBeginInput, begin_shared_prompt_turn_with_runtime_state,
    };
    use vulcan_workbench_model::{PendingSessionPromptSource, SessionRuntimeStateEntry};
    let (root, invocation, target, context) = message_fixture(false).await;
    let delivery = finish_steered_target(&invocation, &target, context).await;
    close_requester_api(&invocation);
    finish_business_response(&invocation.source_handle);
    reconcile_task_results_for_session(&invocation.state, target.session_id())
        .await
        .unwrap();
    assert!(matches!(
        invocation
            .source_handle
            .complete_successful_turn(10)
            .await
            .unwrap()
            .value,
        SuccessfulTurnSettlement::Collaboration
    ));
    let before_inputs = invocation
        .source_handle
        .read(|document| {
            document
                .events()
                .iter()
                .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
                .count()
        })
        .unwrap();
    vulcan_workspace_store::data_plane::prepare_application_schema_for_test(
        &invocation.controller_runtime,
    )
    .await
    .unwrap();
    let directory = root.data_root().join("continuation-project");
    std::fs::create_dir_all(&directory).unwrap();
    let project = vulcan_workbench_model::TaskProjectSummary::new(
        invocation.source_locator.project_id(),
        "Continuation",
        directory.display().to_string(),
        false,
        0,
        None,
        None,
        Vec::new(),
    );
    vulcan_workspace_store::workspace_store::persist_new_project(
        invocation.controller_runtime.clone(),
        &project,
    )
    .await
    .unwrap();
    let runtime = crate::tests::test_support::test_agent_runtime_bridge();
    // Seed the original business Hook through its real entry, without altering the source's V1 ledger.
    // 通过真实入口建立原业务 Hook，不改变来源 V1 账本。
    let mut hook_state = SessionRuntimeStateEntry::default();
    begin_shared_prompt_turn_with_runtime_state(
        &mut hook_state,
        &mut runtime.conversation_hook_runtime.lock().unwrap(),
        SharedPromptTurnRuntimeBeginInput {
            session_id: invocation.source_session_id,
            session_title: "父会话",
            workspace_path: directory.to_str().unwrap(),
            prompt_entry_id: "original-business".into(),
            continuation_action_id: None,
            prompt: "Delegate work",
            hidden_model_guidance: None,
            provider_visible_agent_messages: Vec::new(),
            attachments: Vec::new(),
            provider_attachments: None,
            existing_canonical_user_message_id: None,
            append_canonical_user_message: false,
            append_visible_user_prompt_record: false,
            source: PendingSessionPromptSource::UserFollowup,
            vmm_runtime_ready: false,
            vmm_precheck_ready: false,
            execution_mode: "build".into(),
            reasoning_effort: None,
            owning_project_id: Some(invocation.source_locator.project_id().to_owned()),
        },
    )
    .unwrap();
    {
        let mut state = invocation.state.lock().unwrap();
        state.agent_runtime = Some(runtime.clone());
        state
            .workspace_snapshot
            .as_mut()
            .unwrap()
            .task_projects
            .push(project);
    }
    let started = crate::webui_turn_chaining::start_exact_v1_collaboration_continuation(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        &invocation.controller_runtime,
        &runtime,
        invocation.source_session_id,
    )
    .await
    .unwrap();
    assert!(started);
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if invocation
                .source_handle
                .read(|document| document.active_turn().is_none_or(|turn| turn.closed))
                .unwrap()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("unavailable provider must settle the original continuation task");
    let loaded = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        loaded
            .events()
            .iter()
            .filter(|event| matches!(&event.event, EventBody::UserInput(_)))
            .count(),
        before_inputs
    );
    assert!(
        loaded
            .collaboration_state()
            .received_task_results
            .contains_key(&delivery)
    );
    assert!(!crate::webui_session_kernel_has_active_turn(
        &invocation.state.lock().unwrap(),
        invocation.source_session_id
    ));
    assert!(
        !crate::webui_turn_chaining::start_exact_v1_collaboration_continuation(
            &crate::ServiceEventEmitter::new(vec![]),
            &invocation.state,
            &invocation.controller_runtime,
            &runtime,
            invocation.source_session_id,
        )
        .await
        .unwrap()
    );
}

/// Closes the fixture's real physical API while preserving its open source business turn.
/// 闭合夹具的真实物理 API，同时保留开放来源业务回合。
/// Returns after durable tool interruption and API statistics permit another request boundary.
/// 工具中断及 API 统计可靠提交并允许下一请求边界后返回。
fn close_requester_api(invocation: &ValidatedInvocation) {
    invocation
        .source_handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.apply_atomic_transition(|document| {
                    document.interrupt_active_api(8)?;
                    document.set_collaboration_run_control(
                        vulcan_session_core::v1::collaboration::SetRunControlInput {
                            operation_id: CollaborationOperationId::try_new("requester-running")?,
                            state: CollaborationRunControl::Running,
                            changed_at_ms: 8,
                        },
                    )?;
                    Ok(())
                })
            },
        )
        .unwrap();
}

/// Actual request-boundary projection survives disk reload and cannot leak an old result into new work.
/// 实际请求边界投影经磁盘重载保留，且不能把旧结果泄露到新任务。
#[tokio::test]
async fn session_task_result_projection_uses_only_original_requester_turn() {
    for replace_turn in [false, true] {
        let (_root, invocation, target, context) = message_fixture(false).await;
        let delivery = finish_steered_target(&invocation, &target, context).await;
        reconcile_task_results_for_session(&invocation.state, target.session_id())
            .await
            .unwrap();
        close_requester_api(&invocation);
        if replace_turn {
            invocation
                .source_handle
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::TurnCompleted,
                    |document| {
                        document.close_turn(
                            vulcan_session_core::v1::event::TurnOutcome::Stopped,
                            1,
                            9,
                        )
                    },
                )
                .unwrap();
            invocation
                .source_handle
                .submit_root_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new("replacement-result-source").unwrap(),
                        "Different business",
                        vec![],
                    ),
                    10,
                )
                .unwrap();
        }
        let before = invocation
            .source_handle
            .read(|document| document.memory_revision())
            .unwrap();
        project_collaboration_before_api(&invocation.state, &invocation.source_handle, 11)
            .await
            .unwrap();
        let source =
            load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
                .unwrap()
                .document;
        if replace_turn {
            assert_eq!(source.memory_revision(), before);
            assert!(
                source
                    .collaboration_state()
                    .task_result_projections
                    .is_empty()
            );
            assert!(
                source
                    .collaboration_state()
                    .received_task_results
                    .contains_key(&delivery)
            );
        } else {
            let proof = &source.collaboration_state().task_result_projections[&delivery];
            assert_eq!(proof.turn_id.value(), invocation.source_turn_id);
            assert_eq!(
                proof.inputs.len(),
                1,
                "stopped result still carries an explicit outcome"
            );
            assert_eq!(proof.receiver_revision, source.durable_revision());
            assert!(source.events().iter().any(|event| event.seq == proof.inputs[0].input_seq
                && matches!(&event.event, EventBody::UserInput(input) if input.text.contains("<vulcan_agent_message") && input.text.contains("stopped"))));
        }
        let revision = source.memory_revision();
        project_collaboration_before_api(&invocation.state, &invocation.source_handle, 12)
            .await
            .unwrap();
        assert_eq!(
            invocation
                .source_handle
                .read(|document| document.memory_revision())
                .unwrap(),
            revision
        );
    }
}

/// A revoked result grant projects one redacted failure and never releases the original body later.
/// 结果授权撤销后只投影一次脱敏失败，后续也不释放原正文。
#[tokio::test]
async fn session_task_result_projection_rechecks_original_grant() {
    let (_root, invocation, target, context) = message_fixture(true).await;
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Steer].into_iter().collect(),
    )
    .await
    .unwrap();
    let delivery = finish_steered_target(&invocation, &target, context).await;
    reconcile_task_results_for_session(&invocation.state, target.session_id())
        .await
        .unwrap();
    close_requester_api(&invocation);
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
    project_collaboration_before_api(&invocation.state, &invocation.source_handle, 11)
        .await
        .unwrap();
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    let proof = source.collaboration_state().task_result_projections[&delivery].clone();
    assert_eq!(
        proof.disclosure,
        vulcan_session_core::v1::collaboration::CollaborationTaskResultDisclosure::AuthorizationRevoked
    );
    assert_eq!(proof.inputs.len(), 1);
    let input_seq = proof.inputs[0].input_seq;
    assert!(source.events().iter().any(|event| event.seq == input_seq
        && matches!(&event.event, EventBody::UserInput(input)
            if input.text.contains("task_result_unavailable")
                && input.text.contains("authorization_revoked")
                && !input.text.contains("Business response"))));
    crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Steer].into_iter().collect(),
    )
    .await
    .unwrap();
    let revision = invocation
        .source_handle
        .read(|document| document.memory_revision())
        .unwrap();
    project_collaboration_before_api(&invocation.state, &invocation.source_handle, 12)
        .await
        .unwrap();
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(source.memory_revision(), revision);
    assert_eq!(
        source.collaboration_state().task_result_projections[&delivery],
        proof
    );
}

/// Delivers one native steering message and closes its exact target turn without inventing a final response.
/// 投递一条原生方向消息并闭合精确目标回合，不伪造最终回复。
/// Returns the original result identity for durable receipt assertions.
/// 返回原结果身份，用于可靠回执断言。
async fn finish_steered_target(
    invocation: &ValidatedInvocation,
    target: &Arc<SessionHandle>,
    context: VulcanToolInvocationContext,
) -> CollaborationDeliveryId {
    let sent = send(
        invocation,
        target,
        context,
        SessionMessageDeliveryMode::Interrupt,
        "Check the original task",
    )
    .await
    .unwrap();
    let delivery_id =
        CollaborationDeliveryId::try_new(sent["delivery_id"].as_str().unwrap()).unwrap();
    project_collaboration_before_api(&invocation.state, target, 5)
        .await
        .unwrap();
    target
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::TurnCompleted,
            |document| {
                document.close_turn(vulcan_session_core::v1::event::TurnOutcome::Stopped, 1, 6)
            },
        )
        .unwrap();
    delivery_id
}

/// Result transfer persists both receipts, preserves active requester execution and becomes a no-op on retry.
/// 结果投递持久化双方回执，保留请求者活动执行，重试不产生修改。
#[tokio::test]
async fn session_task_result_return_is_durable_and_idempotent() {
    let (_root, invocation, target, context) = message_fixture(false).await;
    let delivery_id = finish_steered_target(&invocation, &target, context).await;
    let source_api = invocation
        .source_handle
        .read(|document| document.active_api().cloned())
        .unwrap();
    assert_eq!(
        reconcile_task_results_for_session(&invocation.state, target.session_id())
            .await
            .unwrap(),
        1
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    let target_document = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    let receipt = &source.collaboration_state().received_task_results[&delivery_id];
    let acknowledgement = &target_document
        .collaboration_state()
        .task_result_acknowledgements[&delivery_id];
    assert_eq!(acknowledgement.acceptance, *receipt);
    assert_eq!(
        receipt.result,
        target_document.collaboration_state().task_results[&delivery_id]
    );
    assert_eq!(
        invocation
            .source_handle
            .read(|document| document.active_api().cloned())
            .unwrap(),
        source_api
    );
    assert!(
        receipt
            .result
            .terminal
            .as_ref()
            .unwrap()
            .final_assistant
            .is_none()
    );
    let source_revision = invocation
        .source_handle
        .read(|document| document.memory_revision())
        .unwrap();
    let target_revision = target.read(|document| document.memory_revision()).unwrap();
    assert_eq!(
        reconcile_task_results_for_session(&invocation.state, target.session_id())
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        invocation
            .source_handle
            .read(|document| document.memory_revision())
            .unwrap(),
        source_revision
    );
    assert_eq!(
        target.read(|document| document.memory_revision()).unwrap(),
        target_revision
    );
}

/// Revoked grants freeze a redacted first receipt; earlier full receipts still repair confirmation.
/// 撤权后首次回执冻结为脱敏形式；较早的完整回执仍可修复确认。
#[tokio::test]
async fn session_task_result_return_rechecks_grants_and_repairs_acknowledgement() {
    for already_received in [false, true] {
        let (_root, invocation, target, context) = message_fixture(true).await;
        let grant = crate::project_conversation_services::service_add_collaboration_grant(
            invocation.state.clone(),
            invocation.source_session_id.to_string(),
            target.session_id().to_string(),
            format!("user-grant-add:v1:{}", Uuid::new_v4()),
            [CollaborationGrantAction::Steer].into_iter().collect(),
        )
        .await
        .unwrap();
        let delivery_id = finish_steered_target(&invocation, &target, context).await;
        let original_receipt = if already_received {
            let result = target
                .read(|document| document.collaboration_state().task_results[&delivery_id].clone())
                .unwrap();
            Some(
                invocation
                    .source_handle
                    .mutate_durable_if_changed(
                        SessionMutationAdmission::MetadataOnly,
                        SessionFlushReason::MetadataChanged,
                        |document| {
                            document
                                .accept_task_result(
                                    CollaborationOperationId::try_new("already-received").unwrap(),
                                    result,
                                    vulcan_session_core::v1::collaboration::CollaborationTaskResultDisclosure::Full,
                                    7,
                                )
                                .map(session_change)
                        },
                    )
                    .unwrap()
                    .value,
            )
        } else {
            None
        };
        crate::project_conversation_services::service_remove_collaboration_grant(
            invocation.state.clone(),
            invocation.source_session_id.to_string(),
            grant["grant"]["grant_id"].as_str().unwrap().to_string(),
            format!("user-grant-remove:v1:{}", Uuid::new_v4()),
        )
        .await
        .unwrap();
        let returned =
            reconcile_task_results_for_session(&invocation.state, target.session_id()).await;
        if let Some(receipt) = original_receipt {
            assert_eq!(returned.unwrap(), 1);
            assert_eq!(
                target
                    .read(
                        |document| document.collaboration_state().task_result_acknowledgements
                            [&delivery_id]
                            .acceptance
                            .clone()
                    )
                    .unwrap(),
                receipt
            );
        } else {
            assert_eq!(returned.unwrap(), 1);
            let receipt = invocation
                .source_handle
                .read(|document| {
                    document.collaboration_state().received_task_results[&delivery_id].clone()
                })
                .unwrap();
            assert_eq!(
                receipt.disclosure,
                vulcan_session_core::v1::collaboration::CollaborationTaskResultDisclosure::AuthorizationRevoked
            );
            assert_eq!(
                target
                    .read(
                        |document| document.collaboration_state().task_result_acknowledgements
                            [&delivery_id]
                            .acceptance
                            .clone()
                    )
                    .unwrap(),
                receipt
            );
            assert!(
                !invocation
                    .source_handle
                    .read(|document| document
                        .collaboration_state()
                        .requires_edit_or_delete_settlement())
                    .unwrap()
            );
            assert!(
                !target
                    .read(|document| document
                        .collaboration_state()
                        .requires_edit_or_delete_settlement())
                    .unwrap()
            );
            crate::project_conversation_services::service_add_collaboration_grant(
                invocation.state.clone(),
                invocation.source_session_id.to_string(),
                target.session_id().to_string(),
                format!("user-grant-add:v1:{}", Uuid::new_v4()),
                [CollaborationGrantAction::Steer].into_iter().collect(),
            )
            .await
            .unwrap();
            assert_eq!(
                reconcile_task_results_for_session(&invocation.state, target.session_id())
                    .await
                    .unwrap(),
                0
            );
        }
    }
}

/// Revocation before receipt stores only a derived result without final text and settles both sides.
/// 接收前撤权只保存派生且无最终正文的结果，并结算双方。
#[tokio::test]
async fn session_completed_task_result_revoked_before_receipt_is_redacted() {
    let (_root, invocation, target, context) = message_fixture(true).await;
    let grant = crate::project_conversation_services::service_add_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        target.session_id().to_string(),
        format!("user-grant-add:v1:{}", Uuid::new_v4()),
        [CollaborationGrantAction::Steer].into_iter().collect(),
    )
    .await
    .unwrap();
    let sent = send(
        &invocation,
        &target,
        context,
        SessionMessageDeliveryMode::Interrupt,
        "Check the original task",
    )
    .await
    .unwrap();
    let delivery_id =
        CollaborationDeliveryId::try_new(sent["delivery_id"].as_str().unwrap()).unwrap();
    project_collaboration_before_api(&invocation.state, &target, 5)
        .await
        .unwrap();
    finish_business_response(&target);
    target
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::TurnCompleted,
            |document| {
                document.close_turn(
                    vulcan_session_core::v1::event::TurnOutcome::Completed,
                    1,
                    10,
                )
            },
        )
        .unwrap();
    let full_result = target
        .read(|document| document.collaboration_state().task_results[&delivery_id].clone())
        .unwrap();
    assert_eq!(
        full_result
            .terminal
            .as_ref()
            .unwrap()
            .final_assistant
            .as_ref()
            .unwrap()
            .source_text
            .text,
        "Business response"
    );
    crate::project_conversation_services::service_remove_collaboration_grant(
        invocation.state.clone(),
        invocation.source_session_id.to_string(),
        grant["grant"]["grant_id"].as_str().unwrap().to_owned(),
        format!("user-grant-remove:v1:{}", Uuid::new_v4()),
    )
    .await
    .unwrap();
    assert_eq!(
        reconcile_task_results_for_session(&invocation.state, target.session_id())
            .await
            .unwrap(),
        1
    );
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    let receipt = &source.collaboration_state().received_task_results[&delivery_id];
    assert_eq!(
        receipt.disclosure,
        vulcan_session_core::v1::collaboration::CollaborationTaskResultDisclosure::AuthorizationRevoked
    );
    assert_eq!(
        receipt
            .result
            .terminal
            .as_ref()
            .unwrap()
            .final_assistant
            .as_ref()
            .unwrap()
            .source_text
            .text,
        ""
    );
    let target_document = load_session_v1_for_runtime(&target.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(
        target_document
            .collaboration_state()
            .task_result_acknowledgements[&delivery_id]
            .acceptance,
        *receipt
    );
    assert!(
        !source
            .collaboration_state()
            .requires_edit_or_delete_settlement()
    );
    assert!(
        !target_document
            .collaboration_state()
            .requires_edit_or_delete_settlement()
    );
    close_requester_api(&invocation);
    project_collaboration_before_api(&invocation.state, &invocation.source_handle, 11)
        .await
        .unwrap();
    let source = load_session_v1_for_runtime(&invocation.source_handle.storage_locator().unwrap())
        .unwrap()
        .document;
    let proof = &source.collaboration_state().task_result_projections[&delivery_id];
    assert_eq!(
        proof.disclosure,
        vulcan_session_core::v1::collaboration::CollaborationTaskResultDisclosure::AuthorizationRevoked
    );
    assert!(
        source
            .events()
            .iter()
            .any(|event| event.seq == proof.inputs[0].input_seq
                && matches!(&event.event, EventBody::UserInput(input)
            if input.text.contains("task_result_unavailable")
                && !input.text.contains("Business response")))
    );
}
