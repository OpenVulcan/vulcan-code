//! Archived collaboration settlement and deletion use actual SQLite and published session files.
//! 归档协作结算与删除使用实际 SQLite 和已发布会话文件。

use super::*;
use crate::ServiceEventEmitter;
use crate::agent_collaboration::reconcile_session_v1_collaboration_outbox;
use crate::session_v1_lifecycle::{
    SessionV1LifecycleError, archive_session_lifecycle_v1, delete_session_lifecycle_v1,
    guards::admit_edit_generation, publish_guarded_edit_generation,
};
use vulcan_agent_runtime::tooling::collaboration::{REPLY_QUESTION_TOOL_ID, ReplyQuestionArgs};
use vulcan_session_core::v1::collaboration::CollaborationQuestionRecord;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::{
    EditGenerationInput, PrepareAndStartApiInput, RootInputReplacement, SessionDocument,
};
use vulcan_session_core::v1::event::{
    ApiRequestSnapshot, ToolPayload, TurnOutcome, UserInputSource,
};
use vulcan_session_core::v1::ids::RequestStreamId;
use vulcan_session_core::v1::projection::provider_visible_fingerprint;

/// Deleting the target first must preserve missing source acknowledgement and allow settled cleanup.
/// 先删目标必须保留缺失的来源确认，并允许已结算清理。
#[tokio::test]
async fn archived_target_deletion_waits_for_source_settlement_acknowledgement() {
    run_archived_settlement_deletion(false).await;
}

/// A source deleted after acknowledgement must not permanently prevent deletion of its settled target.
/// 确认后删除的来源不能永久阻止删除已结算目标。
#[tokio::test]
async fn archived_target_deletion_accepts_already_deleted_settled_source() {
    run_archived_settlement_deletion(true).await;
}

/// Publishes a source cancellation and target settlement, withholding only the source acknowledgement.
/// 发布来源撤销和目标结算，仅保留来源确认尚未写入。
/// `settle` commits the cancellation and target settlement; otherwise the question stays pending.
/// `settle` 提交撤销与目标结算；否则问题保持待处理。
/// Returns isolated storage, invocation, target, the source question record and the cancellation operation.
/// 返回隔离存储、调用、目标、来源问题记录及撤销操作。
async fn unacknowledged_settlement_fixture(
    settle: bool,
) -> (
    TestHostRuntimeRoot,
    ValidatedInvocation,
    Arc<SessionHandle>,
    CollaborationQuestionRecord,
    String,
) {
    // Both indexed roots and every physical write live under the actual isolated host fixture.
    // 双方索引根及全部物理写入均位于实际隔离宿主夹具中。
    let (storage, invocation, target) = fixture(false).await;
    // Retain the immutable source and project authority from the production locator.
    // 从生产定位器保留不可变来源及项目权威。
    let source = &invocation.source_handle;
    // The source owns a real business turn before issuing its question.
    // 来源在发出问题前持有真实业务回合。
    let source_turn = source
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("archived-source").unwrap(),
                "Ask for direction",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value;
    // The destination independently owns the receiving business turn.
    // 目标独立持有接收业务回合。
    let target_turn = target
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("archived-target").unwrap(),
                "Review the question",
                vec![],
            ),
            2,
        )
        .unwrap()
        .value;
    // Participant coordinates are captured from the actual durable documents.
    // 参与者坐标从实际可靠文档捕获。
    let source_participant = source
        .read(|document| {
            CollaborationParticipant::new(
                source.session_id().to_string(),
                source.session_id().to_string(),
                invocation.source_locator.project_id(),
                document.segment_generation(),
                Some(source_turn),
                None,
            )
            .unwrap()
        })
        .unwrap();
    // Destination coordinates preserve their own independent generation and turn.
    // 目标坐标保留其独立代次与回合。
    let target_participant = target
        .read(|document| {
            CollaborationParticipant::new(
                target.session_id().to_string(),
                target.session_id().to_string(),
                invocation.source_locator.project_id(),
                document.segment_generation(),
                Some(target_turn),
                None,
            )
            .unwrap()
        })
        .unwrap();
    // The service-issued namespace supports exact source lookup without scanning.
    // 服务签发命名空间支持不扫描的精确来源定位。
    let question_id = CollaborationQuestionId::try_new(format!(
        "cq1/v1:{}:archive-settlement",
        source.session_id()
    ))
    .unwrap();
    // Stable delivery identity binds both durable records.
    // 稳定投递身份绑定双方可靠记录。
    let delivery_id = CollaborationDeliveryId::try_new("archive-settlement-delivery").unwrap();
    source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.create_question(CreateQuestionInput {
                    operation_id: CollaborationOperationId::try_new("archive-question-create")
                        .unwrap(),
                    question_id: question_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant.clone(),
                    body: "Which direction?".into(),
                    execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                    created_at_ms: 3,
                })
            },
        )
        .unwrap();
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.accept_delivery(AcceptDeliveryInput {
                    operation_id: CollaborationOperationId::try_new("archive-question-accept")
                        .unwrap(),
                    question_id: question_id.clone(),
                    delivery_id: delivery_id.clone(),
                    source: source_participant.clone(),
                    target: target_participant,
                    bound_turn_id: Some(target_turn),
                    execution_purpose: CollaborationExecutionPurpose::InTurnQuestion,
                    accepted_at_ms: 4,
                })
            },
        )
        .unwrap();
    // The exact durable question record retained for the caller's answer or settlement assertions.
    // 为调用方的回答或结算断言保留精确可靠问题记录。
    let question = source
        .read(|document| document.question_snapshot(&question_id).cloned())
        .unwrap()
        .unwrap();
    if !settle {
        // A pending question leaves both sides holding a live obligation under their running businesses,
        // so both turns stay open for a real in-turn question execution.
        // 待处理问题使双方在各自运行中的业务下持有活动义务，因此双方回合保持开放以承载真实
        // 回合内答疑执行。
        assert!(
            source
                .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
                .unwrap()
        );
        assert!(
            target
                .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
                .unwrap()
        );
        return (storage, invocation, target, question, String::new());
    }
    for handle in [source, &target] {
        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::TurnCompleted,
                |document| document.close_turn(TurnOutcome::Completed, 1, 5),
            )
            .unwrap();
    }
    // Commit exactly the first two cancellation writes, simulating process loss before the source acknowledgement.
    // 精确提交撤销的前两次写入，模拟来源确认前进程丢失。
    let operation = format!("user-question-cancel:v1:{}", Uuid::new_v4());
    // This is the actual source settlement receipt, not an invented event sequence.
    // 这是实际来源结算回执，不是人工构造的事件序号。
    let cancelled = source
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.cancel_question(CancelQuestionInput {
                    operation_id: CollaborationOperationId::try_new(&operation).unwrap(),
                    question_id: question_id.clone(),
                    requester: source_participant,
                    reason: "No longer required".into(),
                    cancelled_at_ms: 6,
                })
            },
        )
        .unwrap()
        .value
        .into_value();
    target
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.settle_inbound_delivery(SettleInboundDeliveryInput {
                    operation_id: CollaborationOperationId::try_new(format!(
                        "{operation}:target_settlement"
                    ))
                    .unwrap(),
                    delivery_id: delivery_id.clone(),
                    question_id: question_id.clone(),
                    source_operation_id: CollaborationOperationId::try_new(&operation).unwrap(),
                    settlement: CollaborationTargetSettlementState::Cancelled,
                    source_event_seq: cancelled.event_seq,
                    acknowledged_at_ms: 7,
                })
            },
        )
        .unwrap();
    assert!(
        source
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    assert!(
        !target
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    (storage, invocation, target, question, operation)
}

/// Replays trusted cancellation after cold archive and deletes both roots in `source_first` order.
/// 冷态归档后重放受信任撤销，并按 `source_first` 顺序删除双方根。
/// Returns after asserting both legal deletion orders and rejection of late operations.
/// 断言两个合法删除顺序及迟到操作拒绝后返回。
async fn run_archived_settlement_deletion(source_first: bool) {
    // Reuse the exact durable gap exercised by generation replacement tests.
    // 复用代次替换测试验证的精确可靠间隙。
    let (_storage, invocation, target, question, operation) =
        unacknowledged_settlement_fixture(true).await;
    let question_id = question.question_id.clone();
    // The earlier owner is retained only for immutable identity and locator assertions.
    // 先前所有者仅用于不可变身份及定位断言。
    let source = &invocation.source_handle;
    for handle in [source, &target] {
        archive_session_lifecycle_v1(
            &invocation.controller_runtime,
            &invocation.registry,
            handle.session_id(),
            8,
        )
        .await
        .unwrap();
    }
    // Replace the registry to exclude resident ownership as evidence of a durable acknowledgement.
    // 替换注册表，排除以驻留归属冒充可靠确认的可能。
    let cold = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = cold.clone();
    // Deletion must retain the only target receipt needed to complete source-side recovery.
    // 删除必须保留来源恢复所需的唯一目标回执。
    let rejected =
        delete_session_lifecycle_v1(&invocation.controller_runtime, &cold, target.session_id())
            .await;
    assert!(
        matches!(rejected, Err(SessionV1LifecycleError::UnsettledCollaboration(id)) if id == target.session_id()),
        "Deletion removed an unacknowledged peer receipt: {rejected:?}"
    );
    service_cancel_collaboration_question(
        invocation.state.clone(),
        question_id.as_str().to_owned(),
        source.session_id().to_string(),
        operation.clone(),
        "No longer required".into(),
    )
    .await
    .unwrap();
    for handle in [source, &target] {
        // Read the current cold owner, since archive deliberately retired the earlier handle.
        // 读取当前冷态所有者，因为归档已明确退出先前句柄。
        let current = cold.get(handle.session_id()).unwrap();
        assert!(
            !current
                .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
                .unwrap()
        );
        assert!(
            current
                .read(|document| document.metadata.archived_at_ms.is_some())
                .unwrap()
        );
    }
    // Exercise both legal deletion orders, including an absent source SQLite row during target admission.
    // 验证两个合法删除顺序，包括目标准入时来源 SQLite 行已经缺失的情况。
    let deletion_order = if source_first {
        [source.session_id(), target.session_id()]
    } else {
        [target.session_id(), source.session_id()]
    };
    for id in deletion_order {
        delete_session_lifecycle_v1(&invocation.controller_runtime, &cold, id)
            .await
            .unwrap();
    }
    // A late trusted cancellation returns the exact missing-root state and never recreates deleted files.
    // 迟到的受信任撤销返回精确根缺失状态，绝不重建已删除文件。
    let late = service_cancel_collaboration_question(
        invocation.state.clone(),
        question_id.as_str().to_owned(),
        source.session_id().to_string(),
        operation,
        "No longer required".into(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        late,
        "user cancellation source root is not an indexed session"
    );
}

/// Captures the target's real generation, durable revision and first root seq without guessing values.
/// 捕获目标真实代次、可靠修订与首个根 seq，不假定分配器值。
fn target_edit_frontier(
    target: &Arc<SessionHandle>,
) -> (u64, u64, vulcan_session_core::v1::ids::Seq) {
    target
        .read(|document| {
            (
                document.segment_generation(),
                document.durable_revision(),
                document
                    .events()
                    .iter()
                    .find_map(|event| match &event.event {
                        vulcan_session_core::v1::event::EventBody::UserInput(input)
                            if input.input_source == UserInputSource::Root =>
                        {
                            Some(event.seq)
                        }
                        _ => None,
                    })
                    .unwrap(),
            )
        })
        .unwrap()
}

/// Replacing target history must not discard the terminal receipt still needed by its source.
/// 替换目标历史不得丢弃来源仍需要的终态回执。
/// Uses actual session files and the same protected admission the production edit service now calls.
/// 使用实际会话文件，以及生产编辑服务当前调用的同一受保护准入。
#[tokio::test]
async fn history_edit_preserves_unacknowledged_target_settlement() {
    // Both canonical documents contain the committed settlement gap, with no active business turn.
    // 双方规范文档包含已提交结算间隙，且没有活动业务回合。
    let (_storage, invocation, target, _question, _operation) =
        unacknowledged_settlement_fixture(true).await;
    let (generation, durable_revision, target_seq) = target_edit_frontier(&target);
    // The production edit service now takes this exact protected admission before publishing.
    // 生产编辑服务当前在发布前取得此精确受保护准入。
    let result = publish_guarded_edit_generation(
        &invocation.controller_runtime,
        &invocation.registry,
        &target,
        generation,
        edit_input(target_seq),
    )
    .await;
    // The missing durable source acknowledgement must reject the edit, not merely its generation CAS.
    // 缺失的可靠来源确认必须拒绝编辑，而不只是其 generation CAS。
    assert!(
        matches!(
            result,
            Err(SessionV1LifecycleError::UnsettledCollaboration(id))
                if id == target.session_id()
        ),
        "History edit discarded the peer settlement receipt: {result:?}"
    );
    assert_eq!(
        target.read(SessionDocument::segment_generation).unwrap(),
        generation
    );
    assert_eq!(
        target.read(|document| document.durable_revision()).unwrap(),
        durable_revision
    );
    // The only receipt the source still needs must survive the rejected replacement.
    // 来源仍需要的唯一回执必须在被拒的替换后保留。
    let retained = target
        .read(|document| {
            document
                .collaboration_state()
                .inbound_deliveries
                .values()
                .filter(|delivery| delivery.settlement.is_some())
                .count()
        })
        .unwrap();
    assert_eq!(retained, 1);
    // The local target condition alone is false here; only the peer acknowledgement guard rejects.
    // 此处仅本地目标条件为假；只有对端确认守卫会拒绝。
    assert!(
        !target
            .read(SessionDocument::has_unsettled_collaboration_for_edit_or_delete)
            .unwrap()
    );
    // A warm target still owns its peer set, so the guard must not depend on a cold registry.
    // 驻留目标仍持有其对端集合，因此守卫不得依赖冷态注册表。
    assert!(Arc::ptr_eq(
        &invocation.registry.get(target.session_id()).unwrap(),
        &target
    ));
    let admission = admit_edit_generation(
        &invocation.controller_runtime,
        &invocation.registry,
        &target,
    )
    .await;
    assert!(
        matches!(
            admission,
            Err(SessionV1LifecycleError::UnsettledCollaboration(id))
                if id == target.session_id()
        ),
        "the receipt admission independently admitted a lost receipt: {admission:?}"
    );
    assert_eq!(
        target.read(SessionDocument::segment_generation).unwrap(),
        generation
    );
}

/// A settled source acknowledgement must still allow the same legitimate history edit.
/// 已结算的来源确认之后，同一合法历史编辑仍必须成功。
/// Uses the production outbox repair as the only writer of that acknowledgement.
/// 使用生产出站箱修复作为该确认的唯一写入者。
#[tokio::test]
async fn history_edit_succeeds_after_source_settlement_acknowledgement() {
    // The fixture withholds exactly the durable source acknowledgement.
    // 夹具精确保留来源确认尚未写入。
    let (_storage, invocation, target, _question, _operation) =
        unacknowledged_settlement_fixture(true).await;
    let (generation, _, target_seq) = target_edit_frontier(&target);
    // The real production repair entry point writes the missing acknowledgement from durable facts.
    // 实际生产修复入口依据可靠事实写入缺失确认。
    let repaired = reconcile_session_v1_collaboration_outbox(
        &ServiceEventEmitter::new(Vec::new()),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .unwrap();
    assert!(repaired > 0, "the outbox repair wrote nothing durable");
    let source_ack = invocation
        .source_handle
        .read(|document| {
            document
                .collaboration_state()
                .outbound_deliveries
                .values()
                .filter(|delivery| delivery.settlement_ack.is_some())
                .count()
        })
        .unwrap();
    assert_eq!(source_ack, 1);
    // The same protected admission now owns a fully acknowledged receipt and must permit the edit.
    // 同一受保护准入当前持有完整确认的回执，必须允许该编辑。
    let receipt = publish_guarded_edit_generation(
        &invocation.controller_runtime,
        &invocation.registry,
        &target,
        generation,
        edit_input(target_seq),
    )
    .await
    .unwrap();
    assert_eq!(receipt.target_seq, target_seq);
    assert_eq!(receipt.segment_generation, generation + 1);
    assert_eq!(
        target.read(SessionDocument::segment_generation).unwrap(),
        generation + 1
    );
    // The edited durable document must physically retain the replaced root text.
    let edited_text = target
        .read(|document| {
            document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    vulcan_session_core::v1::event::EventBody::UserInput(input)
                        if event.seq == target_seq =>
                    {
                        Some(input.text.clone())
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    assert_eq!(edited_text, "Edited target");
}

/// Builds one deterministic root replacement for the fixture's exact root seq.
/// 为夹具的精确根 seq 构建一份确定性的根替换。
fn edit_input(target_seq: vulcan_session_core::v1::ids::Seq) -> EditGenerationInput {
    EditGenerationInput {
        target_seq,
        replacement: RootInputReplacement::new("Edited target", vec![]),
        now_ms: 9,
    }
}

/// Converts one literal into the strict V1 seq identity used by request projection proofs.
/// 将一个字面量转换为请求投影凭据使用的严格 V1 seq 身份。
fn vl_seq(value: u64) -> vulcan_session_core::v1::ids::Seq {
    vulcan_session_core::v1::ids::Seq::try_new(value).unwrap()
}

/// Starts one real physical question-execution API and returns its exact model ToolCall context.
/// 启动一次真实物理答疑 API，并返回其精确模型工具调用上下文。
/// `purpose` selects the auxiliary or parent purpose that the caller already proved admissible.
/// `purpose` 选择调用方已经证明可准入的辅助或父级用途。
/// `stream_label` gives this physical attempt its own request stream and controller tool identity.
/// `stream_label` 为本次物理尝试提供独立的请求流与控制器工具身份。
fn start_question_execution_tool_call(
    target: &Arc<SessionHandle>,
    purpose: ApiPurposeKind,
    now_ms: i64,
    stream_label: &str,
) -> VulcanToolInvocationContext {
    let input = target
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose,
                request: ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: vulcan_session_core::v1::event::ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: vl_seq(1),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms,
            }
        })
        .unwrap();
    let api_call_id = target
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    prepare_reply_question_tool_call(target, api_call_id, now_ms, stream_label)
}

/// Prepares one additional reply tool call on an already admitted physical API.
/// 在已准入物理 API 上准备一条额外的回答工具调用。
/// Returns the exact context a later answer attempt is validated against.
/// 返回后续回答尝试进行校验时使用的精确上下文。
fn prepare_reply_question_tool_call(
    target: &Arc<SessionHandle>,
    api_call_id: vulcan_session_core::v1::ApiCallId,
    now_ms: i64,
    stream_label: &str,
) -> VulcanToolInvocationContext {
    let request_stream_id = RequestStreamId::try_new(format!("{stream_label}-stream")).unwrap();
    let tool_call_id = target
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    request_stream_id.clone(),
                    REPLY_QUESTION_TOOL_ID,
                    ToolPayload::empty(),
                    None,
                    now_ms,
                )
            },
        )
        .unwrap()
        .value;
    let tool_seq = target
        .read(|document| {
            document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    vulcan_session_core::v1::event::EventBody::ToolCall(tool)
                        if tool.tool_call_id == tool_call_id =>
                    {
                        Some(event.seq)
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    VulcanToolInvocationContext {
        session_id: target.session_id().to_string(),
        request_stream_id,
        api_call_id,
        tool_call_id,
        tool_seq,
        controller_tool_call_id: format!("{stream_label}-tool"),
        provider_tool_call_id: None,
    }
}

/// An archived target must still deliver a real model tool answer through the host entry.
/// 已归档目标仍必须通过宿主入口交付真实模型工具回答。
/// Covers the controlled answer path retained after archive, not only query and cancellation.
/// 覆盖归档后保留的受控回答路径，而不只是查询与撤销。
#[tokio::test]
async fn archived_target_model_answer_completes_durable_settlement() {
    // The question is pending and unanswered; the target still owns its open business turn.
    // 问题待处理且未回答；目标仍持有其开放业务回合。
    let (_storage, invocation, target_handle, question, _operation) =
        unacknowledged_settlement_fixture(false).await;
    let target_locator = target_handle.storage_locator().unwrap();
    let source_locator = invocation.source_handle.storage_locator().unwrap();
    // The running target owns a real in-turn question execution before it is archived.
    // 运行中的目标在被归档之前持有真实回合内答疑执行。
    target_handle
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| document.prepare_question_execution_before_api(question.clone(), 8),
        )
        .unwrap();
    let context = start_question_execution_tool_call(
        &target_handle,
        ApiPurposeKind::QuestionExecution,
        9,
        "archived-answer",
    );
    let execution_id = target_handle
        .read(|document| {
            document
                .collaboration_state()
                .question_executions
                .keys()
                .next()
                .cloned()
                .unwrap()
        })
        .unwrap();
    // Archive keeps the open business turn, its auxiliary execution and the accepted delivery.
    // 归档保留开放业务回合、其辅助执行以及已接纳投递。
    for handle in [&invocation.source_handle, &target_handle] {
        archive_session_lifecycle_v1(
            &invocation.controller_runtime,
            &invocation.registry,
            handle.session_id(),
            10,
        )
        .await
        .unwrap();
    }
    // A cold registry excludes every retired resident owner from the answer path.
    // 冷注册表把全部已退出驻留所有者排除在回答路径之外。
    let cold = Arc::new(SessionRuntimeRegistry::default());
    invocation.state.lock().unwrap().session_runtime_registry = cold.clone();
    // The archived target is loaded from its exact durable locator, not a warm business handle.
    // 已归档目标从其精确可靠定位器加载，而不是复用驻留业务句柄。
    let archived_target = SessionHandle::new(
        target_handle.session_id(),
        target_locator.clone(),
        load_session_v1_for_runtime(&target_locator)
            .unwrap()
            .document,
    );
    cold.register(Arc::clone(&archived_target)).unwrap();
    assert!(
        archived_target
            .read(|document| document.metadata.archived_at_ms.is_some())
            .unwrap()
    );
    assert!(
        archived_target
            .read(|document| document
                .inbound_delivery_snapshot(&question.delivery_id)
                .is_some())
            .unwrap()
    );
    // Archive must retain the running auxiliary execution and its exact physical API.
    assert!(
        archived_target
            .read(|document| {
                document.collaboration_state().question_executions[&execution_id].state
                    == CollaborationQuestionExecutionState::Running
            })
            .unwrap()
    );
    assert!(
        archived_target
            .read(|document| document.active_api().is_some())
            .unwrap()
    );
    assert!(cold.get(invocation.source_session_id).is_err());
    // The archived source stays legible for the exact question lookup without a resident owner.
    // 已归档来源在无驻留所有者时仍可被精确问题定位读取。
    let archived_source = SessionHandle::new(
        invocation.source_handle.session_id(),
        source_locator.clone(),
        load_session_v1_for_runtime(&source_locator)
            .unwrap()
            .document,
    );
    assert!(
        archived_source
            .read(|document| document.metadata.archived_at_ms.is_some())
            .unwrap()
    );
    let runtime = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        ServiceEventEmitter::new(Vec::new()),
    );
    let reply = runtime
        .invoke(
            context,
            AgentCollaborationCommand::ReplyQuestion(ReplyQuestionArgs {
                question_id: question.question_id.as_str().to_owned(),
                answer: "Use the archived settlement path.".into(),
                unable_to_answer: false,
            }),
        )
        .await;
    // The archived answer must settle durably through the real target settlement and source ack.
    // 归档后的回答必须通过真实目标结算与来源确认可靠结算。
    let reply = reply.unwrap_or_else(|error| panic!("archived model answer was rejected: {error}"));
    assert_eq!(reply["status"], "ok");
    assert_eq!(
        reply["question_state"].as_str(),
        Some("answered"),
        "the archived answer must settle the source question: {reply}"
    );
    // The source settlement acknowledgement is the durable fact a later edit or delete relies on.
    // 来源结算确认是后续编辑或删除所依赖的可靠事实。
    let source_ack = load_session_v1_for_runtime(&source_locator)
        .unwrap()
        .document;
    assert!(
        source_ack
            .collaboration_state()
            .outbound_deliveries
            .values()
            .any(|delivery| delivery.settlement_ack.is_some())
    );
    // The archived target holds the terminal settlement receipt needed by that acknowledgement.
    // 已归档目标持有该确认所需的终态结算回执。
    assert!(
        archived_target
            .read(|document| document
                .collaboration_state()
                .inbound_deliveries
                .values()
                .any(|delivery| delivery.settlement.is_some()))
            .unwrap()
    );
}

/// An answer that arrives after the asker's business already completed must settle without reviving it.
/// 在提问方业务已经完成之后到达的答案必须完成结算，且不得复活该业务。
///
/// # Notes
/// 说明
/// The existing late-answer evidence covers a *second* answer while both business turns are still open.
/// This test covers the reviewer-requested first-answer case: the asker closed its turn before the
/// answer existed, which is reachable because questions never block business completion.
/// 既有的迟到回答证据覆盖的是双方业务回合仍开放时的**第二次**回答。本测试覆盖审核要求的
/// 首次答案场景：提问方在答案存在之前就关闭了自己的回合，而这是可达的，因为问题从不阻塞业务完成。
#[tokio::test]
async fn answer_after_asker_completion_settles_without_reviving_the_asker_turn() {
    // The question is pending and unanswered; both sides still hold their open business turns.
    // 问题待处理且未回答；双方仍持有各自开放的业务回合。
    let (_storage, invocation, target_handle, question, _operation) =
        unacknowledged_settlement_fixture(false).await;
    let source = invocation.source_handle.clone();
    let source_turn = source
        .read(|document| document.active_turn().unwrap().turn_id)
        .unwrap();
    // The asker finishes its business before any answer exists; questions do not block completion.
    // 提问方在任何答案存在之前结束业务；问题不阻塞完成。
    source
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::TurnCompleted,
            |document| document.close_turn(TurnOutcome::Completed, 1, 9),
        )
        .unwrap();
    // The running target owns a real in-turn question execution before it answers.
    // 运行中的目标在回答之前持有真实回合内答疑执行。
    target_handle
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| document.prepare_question_execution_before_api(question.clone(), 10),
        )
        .unwrap();
    let context = start_question_execution_tool_call(
        &target_handle,
        ApiPurposeKind::QuestionExecution,
        11,
        "late-answer-after-completion",
    );
    // The target answers through the real native port, exactly as a model would after the asker stopped.
    // 目标通过真实原生端口回答，与提问方停止后模型的真实行为一致。
    let reply = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        ServiceEventEmitter::new(Vec::new()),
    )
    .invoke(
        context,
        AgentCollaborationCommand::ReplyQuestion(ReplyQuestionArgs {
            question_id: question.question_id.as_str().to_owned(),
            answer: "Answered after the asker had already finished.".into(),
            unable_to_answer: false,
        }),
    )
    .await
    .unwrap_or_else(|error| panic!("late answer was rejected: {error}"));
    assert_eq!(reply["status"], "ok");
    assert_eq!(
        reply["question_state"].as_str(),
        Some("answered"),
        "a late answer must still settle the question: {reply}"
    );
    // The asker's closed turn must stay closed and no new business turn may appear.
    // 提问方已关闭的回合必须保持关闭，且不得出现新的业务回合。
    source
        .read(|document| {
            assert_eq!(
                document.active_turn().map(|turn| turn.turn_id),
                Some(source_turn),
                "the closed turn must remain the session's only turn"
            );
            assert!(
                document.active_turn().unwrap().closed,
                "a late answer must not reopen the asker's completed turn"
            );
            assert!(
                document
                    .collaboration_state()
                    .question_executions
                    .is_empty(),
                "the asker owes no question execution for the late answer"
            );
        })
        .unwrap();
    // The durable settlement is complete on both sides: the source acknowledged the settlement.
    // 双方可靠结算均已完成：来源已确认该结算。
    let settled = source
        .read(|document| {
            let delivery = document
                .collaboration_state()
                .outbound_deliveries
                .values()
                .next()
                .cloned()
                .expect("the asker must retain its outbound delivery");
            let answered = document
                .collaboration_state()
                .questions
                .values()
                .next()
                .cloned()
                .expect("the asker must retain its question");
            (delivery.settlement_ack.is_some(), answered.answer)
        })
        .unwrap();
    assert!(
        settled.0,
        "a late answer must still produce the durable source settlement acknowledgement"
    );
    assert_eq!(
        settled.1.map(|answer| answer.body),
        Some(String::from(
            "Answered after the asker had already finished."
        )),
        "the late answer body must remain bound to the original question"
    );
    // The cold reader observes the same settlement without any resident asker turn.
    // 冷读取方在没有任何驻留提问方回合的情况下观察到同一结算。
    let reloaded = load_session_v1_for_runtime(&source.storage_locator().unwrap())
        .unwrap()
        .document;
    assert!(
        reloaded
            .collaboration_state()
            .questions
            .values()
            .any(|record| record.answer.is_some()),
        "the late answer must survive a cold reload"
    );
}

/// Admits one real `wait_question` tool call on the asker's open business turn.
/// 在提问方的开放业务回合上准入一条真实的 `wait_question` 工具调用。
///
/// # Parameters
/// 参数
/// `handle` is the asker's canonical session and `now_ms` its semantic boundary time.
/// `handle` 是提问方的规范会话，`now_ms` 是其语义边界时间。
///
/// # Returns
/// 返回值
/// Returns the exact context whose identity the native wait command validates against.
/// 返回原生命令在等待校验时比对的精确上下文。
fn prepare_wait_question_tool_call(
    handle: &Arc<SessionHandle>,
    now_ms: i64,
) -> VulcanToolInvocationContext {
    let input = handle
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot::new("openai", "model"),
                request_budget_evidence:
                    crate::session_runtime_registry_v1::test_provider_request_budget_evidence(),
                projection: vulcan_session_core::v1::event::ProviderProjectionProof {
                    segment_generation: document.segment_generation(),
                    document_revision: document.memory_revision(),
                    retained_from_seq: vulcan_session_core::v1::ids::Seq::try_new(1).unwrap(),
                    through_seq,
                    fingerprint_sha256: provider_visible_fingerprint(
                        document.events(),
                        through_seq,
                    )
                    .unwrap(),
                },
                locked_baseline: LockedBaselineProjection::empty(),
                continuation_action_id: None,
                now_ms,
            }
        })
        .unwrap();
    let api_call_id = handle
        .prepare_and_start_api_durable(input)
        .unwrap()
        .value
        .api_call_id;
    let request_stream_id = RequestStreamId::try_new("wait-question-stream").unwrap();
    let tool_call_id = handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::ToolBoundary,
            |document| {
                document.prepare_tool(
                    api_call_id,
                    request_stream_id.clone(),
                    WAIT_QUESTION_TOOL_ID,
                    ToolPayload::empty(),
                    None,
                    now_ms,
                )
            },
        )
        .unwrap()
        .value;
    let tool_seq = handle
        .read(|document| {
            document
                .events()
                .iter()
                .find_map(|event| match &event.event {
                    vulcan_session_core::v1::event::EventBody::ToolCall(tool)
                        if tool.tool_call_id == tool_call_id =>
                    {
                        Some(event.seq)
                    }
                    _ => None,
                })
                .unwrap()
        })
        .unwrap();
    VulcanToolInvocationContext {
        session_id: handle.session_id().to_string(),
        request_stream_id,
        api_call_id,
        tool_call_id,
        tool_seq,
        controller_tool_call_id: String::from("wait-question-tool"),
        provider_tool_call_id: None,
    }
}

/// An answer that already exists must be observed at once instead of waiting out the timeout.
/// 已经存在的答案必须立即被观察到，而不是等满超时。
///
/// # Notes
/// 说明
/// This is the "answer arrives first" combination: the responder settles the question before the asker
/// ever waits. A wait implementation that only watched for *new* collaboration changes would block here
/// and report a timeout while the answer was already durable.
/// 这是"答案先到"组合：回答方在提问方开始等待之前就结算了问题。若等待实现只观察**新增**协作变化，
/// 就会在这里阻塞并报告超时，而答案其实已经可靠存在。
#[tokio::test]
async fn answer_arriving_before_the_wait_is_observed_immediately() {
    let (_storage, invocation, target_handle, question, _operation) =
        unacknowledged_settlement_fixture(false).await;
    // The responder answers through the real native port before the asker waits at all.
    // 回答方在提问方开始等待之前就通过真实原生端口作答。
    target_handle
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| document.prepare_question_execution_before_api(question.clone(), 10),
        )
        .unwrap();
    let context = start_question_execution_tool_call(
        &target_handle,
        ApiPurposeKind::QuestionExecution,
        11,
        "answer-before-wait",
    );
    let reply = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        ServiceEventEmitter::new(Vec::new()),
    )
    .invoke(
        context,
        AgentCollaborationCommand::ReplyQuestion(ReplyQuestionArgs {
            question_id: question.question_id.as_str().to_owned(),
            answer: "Answer produced before the asker waited.".into(),
            unable_to_answer: false,
        }),
    )
    .await
    .unwrap_or_else(|error| panic!("the pre-wait answer was rejected: {error}"));
    assert_eq!(reply["question_state"].as_str(), Some("answered"));
    // Now the asker waits with a generous timeout; a settled question must end the wait at once.
    // 现在提问方以充裕超时等待；已结算问题必须立即结束等待。
    let source = invocation.source_handle.clone();
    let started = std::time::Instant::now();
    let wait_context = prepare_wait_question_tool_call(&source, 12);
    let observed = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        ServiceEventEmitter::new(Vec::new()),
    )
    .invoke(
        wait_context,
        AgentCollaborationCommand::WaitQuestion(
            vulcan_agent_runtime::tooling::collaboration::WaitQuestionArgs {
                question_id: question.question_id.as_str().to_owned(),
                after_revision: None,
                timeout_ms: 30_000,
            },
        ),
    )
    .await
    .unwrap_or_else(|error| panic!("waiting on a settled question failed: {error}"));
    // The wait must report the settled snapshot rather than burning the whole timeout budget.
    // 等待必须报告已结算快照，而不是耗尽整个超时预算。
    assert_eq!(
        observed["status"], "ok",
        "an already answered question must not be waited out: {observed}"
    );
    assert_eq!(observed["question_state"].as_str(), Some("answered"));
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "a settled question must be observed without consuming the wait budget"
    );
    assert_eq!(
        observed["answer"]["body"].as_str(),
        Some("Answer produced before the asker waited."),
        "the observed answer must be the exact durable body"
    );
    // Observing the answer must not fabricate a second settlement on the asker.
    // 观察答案不得在提问方伪造第二次结算。
    source
        .read(|document| {
            let answered = document
                .collaboration_state()
                .questions
                .values()
                .filter(|record| record.answer.is_some())
                .count();
            assert_eq!(answered, 1, "exactly one answer may exist");
        })
        .unwrap();
}

/// An edit admission must hold its own target gate, not only the settlement peers.
/// 编辑准入必须持有其自身目标门，而不只是结算对端。
/// While the target gate is held elsewhere, the admission must not complete.
/// 当目标门已被他处持有时，准入不得完成。
#[tokio::test]
async fn edit_admission_holds_target_lifecycle_gate() {
    // The positive fixture carries the settled receipt; the acknowledgement is repaired first.
    // 正向夹具携带已结算回执；先修复来源确认。
    let (_storage, invocation, target, _question, _operation) =
        unacknowledged_settlement_fixture(true).await;
    reconcile_session_v1_collaboration_outbox(
        &ServiceEventEmitter::new(Vec::new()),
        invocation.state.clone(),
        invocation.source_session_id,
    )
    .await
    .unwrap();
    let registry = invocation.registry.clone();
    let session_id = target.session_id();
    let controller = invocation.controller_runtime.clone();
    // Hold the exact gate the edit admission must also own for its target.
    // 持有编辑准入同样必须为其目标拥有的精确门。
    let held = crate::session_v1_lifecycle::guards::hold_lifecycle_gate(&registry, session_id)
        .await
        .unwrap();
    // A pending admission observes a receipt whose source acknowledgement is already durable.
    // 挂起的准入观察到来源确认已经可靠的延续回执。
    let admission_registry = registry.clone();
    // The admission borrows the canonical owner, so this test intentionally leaks one handle clone.
    // 准入借用规范所有者，因此本测试有意泄漏一份句柄克隆。
    let admission_target: &'static Arc<SessionHandle> = Box::leak(Box::new(Arc::clone(&target)));
    let admission = tokio::spawn(async move {
        admit_edit_generation(&controller, &admission_registry, admission_target).await
    });
    // The admission must still be waiting on the held target gate.
    // 准入必须仍在等待已持有的目标门。
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert!(
        !admission.is_finished(),
        "the edit admission completed without holding its own target gate"
    );
    // Releasing the gate lets the admission finish and resolve the acknowledged peer set.
    // 释放该门后准入完成，并解析已确认的对端集合。
    drop(held);
    let admitted = tokio::time::timeout(std::time::Duration::from_secs(5), admission)
        .await
        .expect("the edit admission never resumed after its gate was released")
        .unwrap();
    assert!(
        admitted.is_ok(),
        "the fully acknowledged target must admit an edit: {admitted:?}"
    );
    drop(admitted);
    let generation = target.read(SessionDocument::segment_generation).unwrap();
    let receipt = publish_guarded_edit_generation(
        &invocation.controller_runtime,
        &invocation.registry,
        &target,
        generation,
        edit_input(target_edit_frontier(&target).2),
    )
    .await
    .unwrap();
    assert_eq!(receipt.segment_generation, generation + 1);
}

/// A late second answer is refused after settlement and revives no target business task.
/// 结算后的迟到第二次回答被拒绝，且不复活任何目标业务任务。
/// Uses the real native reply port, the canonical question reducer and both real session files.
/// 使用真实原生回答端口、规范问题归约器以及双方真实会话文件。
#[tokio::test]
async fn late_answer_after_settlement_is_rejected_without_reviving_target_task() {
    // The question is pending and unanswered, and both business turns remain open.
    // 问题待处理且未回答，双方业务回合保持开放。
    let (_storage, invocation, target_handle, question, _operation) =
        unacknowledged_settlement_fixture(false).await;
    // The running target owns a real in-turn question execution before its first answer.
    // 运行中的目标在首次回答之前持有真实回合内答疑执行。
    target_handle
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| document.prepare_question_execution_before_api(question.clone(), 8),
        )
        .unwrap();
    let runtime = session_v1_native_agent_collaboration_runtime(
        invocation.state.clone(),
        ServiceEventEmitter::new(Vec::new()),
    );
    let settled = runtime
        .invoke(
            start_question_execution_tool_call(
                &target_handle,
                ApiPurposeKind::QuestionExecution,
                9,
                "settling-answer",
            ),
            AgentCollaborationCommand::ReplyQuestion(ReplyQuestionArgs {
                question_id: question.question_id.as_str().to_owned(),
                answer: "First answer".into(),
                unable_to_answer: false,
            }),
        )
        .await
        .unwrap_or_else(|error| panic!("the first answer must settle the question: {error}"));
    assert_eq!(settled["question_state"].as_str(), Some("answered"));
    // Capture the settled durable facts, including the source acknowledgement.
    // 捕获已结算的可靠事实，包括来源确认。
    let source_locator = invocation.source_handle.storage_locator().unwrap();
    let target_locator = target_handle.storage_locator().unwrap();
    let (_, source_question, source_acknowledged) = invocation
        .source_handle
        .read(|document| {
            (
                document.durable_revision(),
                document
                    .collaboration_state()
                    .questions
                    .get(&question.question_id)
                    .cloned()
                    .unwrap(),
                document
                    .collaboration_state()
                    .outbound_deliveries
                    .values()
                    .any(|delivery| delivery.settlement_ack.is_some()),
            )
        })
        .unwrap();
    assert_eq!(source_question.state, CollaborationQuestionState::Answered);
    assert_eq!(
        source_question.answer.as_ref().unwrap().body,
        "First answer"
    );
    assert!(source_acknowledged);
    let active_api = target_handle
        .read(|document| document.active_api().unwrap().api_call_id)
        .unwrap();
    // The late attempt owns its own real prepared ToolCall before the answer is validated.
    // 迟到尝试在回答被校验之前持有其自身的真实已准备工具调用。
    let late_context =
        prepare_reply_question_tool_call(&target_handle, active_api, 10, "late-answer");
    // Capture both durable frontiers after that legitimate preparation, so the rejected answer
    // itself must leave every durable fact untouched.
    // 在该合法准备之后捕获双方可靠前沿，使被拒回答本身必须不改变任何可靠事实。
    let source_revision = invocation
        .source_handle
        .read(|document| document.durable_revision())
        .unwrap();
    let (target_revision, target_state) = target_handle
        .read(|document| {
            (
                document.durable_revision(),
                document.collaboration_state().clone(),
            )
        })
        .unwrap();
    // A second independently admitted tool call is a late answer for the settled question.
    // 独立准入的第二次工具调用构成对该已结算问题的迟到回答。
    let rejected = runtime
        .invoke(
            late_context,
            AgentCollaborationCommand::ReplyQuestion(ReplyQuestionArgs {
                question_id: question.question_id.as_str().to_owned(),
                answer: "Late answer".into(),
                unable_to_answer: false,
            }),
        )
        .await
        .unwrap_err();
    assert!(
        rejected.contains("already settled as answered"),
        "a settled question must refuse a second answer: {rejected}"
    );
    // The source keeps exactly one answer and neither side gains a new business turn.
    // 来源只保留一个回答，双方都不新增业务回合。
    invocation
        .source_handle
        .read(|document| {
            assert_eq!(document.durable_revision(), source_revision);
            let current = &document.collaboration_state().questions[&question.question_id];
            assert_eq!(current.state, CollaborationQuestionState::Answered);
            assert_eq!(current.answer.as_ref().unwrap().body, "First answer");
            assert_eq!(
                document
                    .collaboration_state()
                    .questions
                    .values()
                    .filter(|question| question.answer.is_some())
                    .count(),
                1
            );
        })
        .unwrap();
    target_handle
        .read(|document| {
            assert_eq!(document.durable_revision(), target_revision);
            assert_eq!(document.collaboration_state(), &target_state);
            assert!(document.active_turn().is_some_and(|turn| !turn.closed));
            assert!(document.active_api().is_some());
        })
        .unwrap();
    // The real files must still carry the single original answer after the rejected attempt.
    // 拒绝之后真实文件仍必须只携带原回答。
    let loaded_source = load_session_v1_for_runtime(&source_locator)
        .unwrap()
        .document;
    let loaded_question = &loaded_source.collaboration_state().questions[&question.question_id];
    assert_eq!(loaded_question.state, CollaborationQuestionState::Answered);
    assert_eq!(
        loaded_question.answer.as_ref().unwrap().body,
        "First answer"
    );
    let loaded_target = load_session_v1_for_runtime(&target_locator)
        .unwrap()
        .document;
    assert_eq!(loaded_target.durable_revision(), target_revision);
}
