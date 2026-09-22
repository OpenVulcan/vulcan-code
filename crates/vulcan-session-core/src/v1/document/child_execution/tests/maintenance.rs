//! Durable success and freshness requirements for post-business context maintenance.
//! 业务完成后上下文维护的可靠成功与新鲜度要求。

use super::*;

/// Restart closes unfinished maintenance with or without a physical API and never reopens business.
/// 重启闭合有或无物理 API 的未完成维护，绝不重开业务。
/// Returns nothing; reloads actual serialized records before and after interruption publication.
/// 不返回值；在中断发布前后重载实际序列化记录。
#[test]
fn maintenance_restart_settles_auxiliary_execution_once() {
    for with_api in [false, true] {
        // Build a successful business independently for each crash boundary.
        // 为每个崩溃边界独立构造成功业务。
        let (mut document, execution) = completed_child(ApiPurposeKind::MainGeneration);
        publish(&mut document);
        // Keep the immutable business proof across auxiliary crash cleanup.
        // 在辅助崩溃清理前后保留不可变业务证明。
        let source = document
            .capture_child_context_maintenance_source(&execution)
            .unwrap()
            .unwrap();
        document
            .begin_child_context_maintenance(
                &source,
                CollaborationExecutionId::try_new("maintenance-restart").unwrap(),
                10,
            )
            .unwrap();
        if with_api {
            // Persist an unfinished semantic under the actual maintenance API.
            // 在实际维护 API 下持久化未完成语义。
            let api = document
                .start_api(
                    ApiPurposeKind::Compaction,
                    ApiRequestSnapshot::new("provider", "model"),
                    11,
                )
                .unwrap();
            document
                .append_assistant_text(api, "Partial", AssistantSemanticStatus::Streaming, 12)
                .unwrap();
        }
        publish(&mut document);
        document = reload(&document);
        // Cold projection must be atomic and idempotent even before a Provider request started.
        // 即使供应商请求尚未启动，冷投影也必须原子且幂等。
        let revision = document.memory_revision();
        assert_eq!(document.project_persisted_interruptions().unwrap(), 1);
        assert_eq!(document.memory_revision(), revision + 1);
        assert!(document.active_api().is_none());
        assert!(document.active_turn().unwrap().closed);
        assert_eq!(document.project_persisted_interruptions().unwrap(), 0);
        publish(&mut document);
        document = reload(&document);
        assert_eq!(document.project_persisted_interruptions().unwrap(), 0);
        assert_eq!(
            document
                .capture_child_context_maintenance_source(&execution)
                .unwrap()
                .unwrap()
                .completed_business(),
            source.completed_business()
        );
    }
}

/// An active maintenance failure atomically seals its API, diagnostic and auxiliary lifetime.
/// 活动维护失败原子封口其 API、诊断和辅助生命周期。
/// Returns nothing; verifies exact correlation, replay and original business isolation.
/// 不返回值；验证精确关联、重放以及原业务隔离。
#[test]
fn maintenance_active_api_failure_is_atomic_and_correlated() {
    // Preserve the published business facts before auxiliary admission.
    // 在辅助准入之前保留已发布业务事实。
    let (mut document, execution) = completed_child(ApiPurposeKind::MainGeneration);
    publish(&mut document);
    // The source identifies the original completed business, not the auxiliary result.
    // 来源标识原已完成业务，而非辅助结果。
    let source = document
        .capture_child_context_maintenance_source(&execution)
        .unwrap()
        .unwrap();
    // This independent identity owns the failing physical API.
    // 此独立身份拥有失败的物理 API。
    let maintenance_id = CollaborationExecutionId::try_new("maintenance-active-failure").unwrap();
    // Capture the exact auxiliary turn for diagnostic correlation.
    // 捕获精确辅助回合以关联诊断。
    let started = document
        .begin_child_context_maintenance(&source, maintenance_id, 10)
        .unwrap()
        .into_value();
    // Open a real compaction API and leave its assistant semantic unfinished.
    // 开启真实压缩 API，并保留未完成的 assistant 语义。
    let api = document
        .start_api(
            ApiPurposeKind::Compaction,
            ApiRequestSnapshot::new("provider", "model"),
            11,
        )
        .unwrap();
    document
        .append_assistant_text(
            api,
            "Partial summary",
            AssistantSemanticStatus::Streaming,
            12,
        )
        .unwrap();
    // Failure must advance a single publication revision despite multiple terminal facts.
    // 尽管存在多个终态事实，失败只能推进一次发布修订。
    let revision = document.memory_revision();
    assert!(
        document
            .fail_execution_boundary_and_close_turn(
                "maintenance-provider-failed",
                Some("Connection lost"),
                13
            )
            .unwrap()
    );
    assert_eq!(document.memory_revision(), revision + 1);
    assert!(document.active_api().is_none());
    assert!(document.active_turn().unwrap().closed);
    assert!(document.events().iter().any(|event| event.turn_id == Some(started.turn_id)
        && matches!(&event.event, EventBody::DisplayNote(note) if note.notice_kind == DisplayNoticeKind::SystemError
            && note.reason_code == "maintenance-provider-failed" && note.api_call_id == Some(api)
            && note.redacted_detail.as_deref() == Some("Connection lost"))));
    assert!(
        !document
            .fail_execution_boundary_and_close_turn(
                "maintenance-provider-failed",
                Some("Connection lost"),
                14
            )
            .unwrap()
    );
    assert_eq!(document.memory_revision(), revision + 1);
    publish(&mut document);
    document = reload(&document);
    assert!(document.active_turn().is_none());
    // Replay after cold loading must not create a new session-scoped failure notice.
    // 冷加载后重放不能创建新的会话级失败通知。
    let reloaded = document.clone();
    assert!(
        !document
            .fail_execution_boundary_and_close_turn(
                "maintenance-provider-failed",
                Some("Connection lost"),
                15
            )
            .unwrap()
    );
    assert_eq!(document, reloaded);
    assert_eq!(
        document
            .metadata
            .context_maintenance
            .as_ref()
            .unwrap()
            .terminal
            .as_ref()
            .unwrap()
            .reason,
        TurnClosureReason::Failed
    );
    assert_eq!(
        document
            .capture_child_context_maintenance_source(&execution)
            .unwrap()
            .unwrap()
            .completed_business(),
        source.completed_business()
    );
}

/// Reconstructs a fully serialized maintenance-bearing document; returns the actual loaded authority.
/// 重建完整序列化的带维护文档；返回实际加载权威。
fn reload(document: &SessionDocument) -> SessionDocument {
    SessionDocument::from_parts(
        serde_json::from_value(serde_json::to_value(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        serde_json::from_value(serde_json::to_value(document.events()).unwrap()).unwrap(),
    )
    .unwrap()
}

/// Maintenance failure closes its independent owner and retains the original business proof and TTL.
/// 维护失败闭合独立所有者，并保留原业务证明及 TTL。
/// Returns nothing; exercises start replay, cold recovery, purpose admission and failure routing.
/// 不返回值；执行起始重放、冷恢复、用途准入及失败路由。
#[test]
fn maintenance_lifecycle_failure_is_independent_and_recoverable() {
    // Original business has actually completed; the one-turn memory belongs to the next business lifetime.
    // 原业务实际已完成；单回合记忆属于下一业务生命周期。
    let (mut document, execution) = completed_child(ApiPurposeKind::MainGeneration);
    document
        .append_system_context(
            None,
            "maintenance-ttl",
            SystemContextOperation::Upsert,
            SystemContextScope::Session,
            Some(1),
            Some(SystemRoleHint::Developer),
            "Retain for next business",
            9,
        )
        .unwrap();
    publish(&mut document);
    // Freeze the actual validated business source and compare it after maintenance failure.
    // 冻结实际校验的业务来源，并在维护失败后比较。
    let source = document
        .capture_child_context_maintenance_source(&execution)
        .unwrap()
        .unwrap();
    // This ID is independent from the original root action.
    // 此 ID 独立于原根动作。
    let maintenance_id = CollaborationExecutionId::try_new("maintenance-failure").unwrap();
    // A new auxiliary turn must not reuse the completed business turn.
    // 新辅助回合不能复用已完成业务回合。
    let started = document
        .begin_child_context_maintenance(&source, maintenance_id.clone(), 10)
        .unwrap()
        .into_value();
    assert_ne!(started.turn_id, source.profile().business_turn_id);
    assert_eq!(
        document.active_turn().unwrap().origin,
        TurnOrigin::ContextMaintenance
    );
    // Replayed admission preserves its original timestamp, anchor and revision.
    // 重放准入保留原时间戳、锚点和修订。
    let revision = document.memory_revision();
    assert!(
        !document
            .begin_child_context_maintenance(&source, maintenance_id.clone(), 11)
            .unwrap()
            .changed()
    );
    assert_eq!(document.memory_revision(), revision);
    publish(&mut document);
    document = reload(&document);
    assert_eq!(document.active_turn().unwrap().turn_id, started.turn_id);
    // Auxiliary maintenance cannot dispatch normal work or select a business/Goal successor.
    // 辅助维护不能分发普通工作或选择业务、目标后继。
    let before = document.clone();
    assert!(
        document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("provider", "model"),
                12
            )
            .is_err()
    );
    assert!(
        document
            .complete_turn_and_select_successor(started.turn_id, 12)
            .is_err()
    );
    assert!(
        document
            .finish_child_context_maintenance(&maintenance_id, TurnClosureReason::Completed, 12)
            .is_err()
    );
    assert_eq!(document, before);
    assert!(
        document
            .fail_turn_without_active_api("maintenance-failed", Some("Summary unavailable"), 13)
            .unwrap()
    );
    assert_eq!(
        document
            .metadata
            .context_maintenance
            .as_ref()
            .unwrap()
            .terminal
            .as_ref()
            .unwrap()
            .reason,
        TurnClosureReason::Failed
    );
    publish(&mut document);
    document = reload(&document);
    assert!(document.active_turn().is_none());
    assert_eq!(
        document.collaboration_run_control(),
        CollaborationRunControl::NaturallyIdle
    );
    assert_eq!(
        document
            .capture_child_context_maintenance_source(&execution)
            .unwrap()
            .unwrap()
            .completed_business(),
        source.completed_business()
    );
    // Auxiliary boundaries consume no business TTL and create no expiration marker.
    // 辅助边界不消耗业务 TTL，也不创建失效标记。
    let active = crate::v1::projection::derive_active_system_state(
        document.events(),
        document.events().last().unwrap().seq,
    )
    .unwrap();
    assert_eq!(
        active
            .iter()
            .find(|state| state.context_key == "maintenance-ttl")
            .unwrap()
            .remaining_turns,
        Some(1)
    );
    assert_eq!(document.events().iter().filter(|event| matches!(&event.event, EventBody::TurnBoundary(boundary) if boundary.purpose == TurnBoundaryPurpose::Business)).count(), 1);
    // Conflicting terminal replay fails instead of upgrading maintenance failure to success.
    // 冲突终态重放失败，而不是把维护失败提升为成功。
    assert!(
        !document
            .finish_child_context_maintenance(&maintenance_id, TurnClosureReason::Failed, 14)
            .unwrap()
            .changed()
    );
    assert!(
        document
            .finish_child_context_maintenance(&maintenance_id, TurnClosureReason::Completed, 14)
            .is_err()
    );
}

/// Maintenance success requires a real owned compaction and survives serialized closure validation.
/// 维护成功要求真实所属压缩，并经序列化闭合校验保留。
/// Returns nothing; malformed journal/metadata closure is rejected rather than repaired.
/// 不返回值；畸形日志或元数据闭合被拒绝，不自动修复。
#[test]
fn maintenance_lifecycle_success_requires_owned_compaction() {
    // Start from a published business result and retain its immutable transcript prefix.
    // 从已发布业务结果开始，保留其不可变转录前缀。
    let (mut document, execution) = completed_child(ApiPurposeKind::MainGeneration);
    publish(&mut document);
    // The original history must survive the independent auxiliary API unchanged.
    // 原历史必须在独立辅助 API 之后保持不变。
    let original_events = document.events().to_vec();
    // Capture original authority before allocating the independent execution.
    // 在分配独立执行之前捕获原权威。
    let source = document
        .capture_child_context_maintenance_source(&execution)
        .unwrap()
        .unwrap();
    // Exact auxiliary identity is preserved through API, node and terminal publication.
    // 精确辅助身份贯穿 API、节点及终态发布。
    let maintenance_id = CollaborationExecutionId::try_new("maintenance-success").unwrap();
    document
        .begin_child_context_maintenance(&source, maintenance_id.clone(), 10)
        .unwrap();
    // Compaction uses the same physical routing admitted by the original fixture.
    // 压缩使用原夹具准入的同一物理路由。
    let request = ApiRequestSnapshot::new("provider", "model");
    // The real API owns the exact summary semantic receipt.
    // 真实 API 拥有精确摘要语义回执。
    let api = document
        .start_api(ApiPurposeKind::Compaction, request.clone(), 11)
        .unwrap();
    // Node commitment must use the actually allocated summary event.
    // 节点提交必须使用实际分配的摘要事件。
    let summary_seq = document
        .append_assistant_text(
            api,
            "Maintenance summary",
            AssistantSemanticStatus::Completed,
            12,
        )
        .unwrap()
        .into_value();
    assert!(
        document
            .finish_child_context_maintenance(&maintenance_id, TurnClosureReason::Failed, 13)
            .is_err()
    );
    document
        .complete_api_group(
            api,
            ApiCompletion {
                request,
                usage: crate::v1::stats::UsageSnapshot::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: 13,
            },
        )
        .unwrap();
    assert!(
        document
            .finish_child_context_maintenance(&maintenance_id, TurnClosureReason::Completed, 14)
            .is_err()
    );
    document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                summary_seq,
                Seq::try_new(1).unwrap(),
                "Maintenance summary",
                LockedBaselineProjection::empty(),
            ),
            14,
        )
        .unwrap();
    document
        .finish_child_context_maintenance(&maintenance_id, TurnClosureReason::Completed, 15)
        .unwrap();
    assert_eq!(
        &document.events()[..original_events.len()],
        original_events.as_slice()
    );
    publish(&mut document);
    document = reload(&document);
    assert!(document.active_turn().is_none());
    // The strict loader rejects terminal metadata that disagrees with the host-only journal.
    // 严格加载器拒绝与仅宿主日志不一致的终态元数据。
    let mut invalid_metadata = document.metadata.clone();
    invalid_metadata
        .context_maintenance
        .as_mut()
        .unwrap()
        .terminal
        .as_mut()
        .unwrap()
        .reason = TurnClosureReason::Failed;
    assert!(
        SessionDocument::from_parts(
            invalid_metadata,
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            document.events().to_vec()
        )
        .is_err()
    );
    // Removing both the node and mutable pointer cannot erase the successful maintenance receipt.
    // 同时移除节点与可变指针，不能抹除成功维护回执。
    let mut invalid_runtime = document.runtime_lock.clone();
    invalid_runtime.current_compaction_id = None;
    assert!(
        SessionDocument::from_parts(
            document.metadata.clone(),
            invalid_runtime,
            vec![],
            document.segment_catalog().to_vec(),
            document.events().to_vec()
        )
        .is_err()
    );
}
use crate::v1::collaboration::{
    CollaborationExecutionId, CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};

/// Publishes the fixture's actual flush candidate; returns nothing and never forges a revision.
/// 发布夹具实际刷盘候选；不返回值，也不伪造修订。
fn publish(document: &mut SessionDocument) {
    // The domain-issued revision is the only accepted publication acknowledgement.
    // 领域签发的修订是唯一被接受的发布确认。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
}

/// Builds an original completed child using real API, assistant, terminal and turn transitions.
/// 使用真实 API、助手、终态及回合转换构建原已完成子任务。
/// `purpose` selects the physical request; returns an unpublished document and its original execution ID.
/// `purpose` 选择物理请求；返回未发布文档及其原执行 ID。
fn completed_child(purpose: ApiPurposeKind) -> (SessionDocument, CollaborationExecutionId) {
    // The existing profile fixture supplies the exact real root and resolved child settings.
    // 既有档案夹具提供精确真实根输入及已解析子设置。
    let (mut document, profile) = fixture();
    // Root action identity remains the maintenance source throughout all later turns.
    // 根动作身份在所有后续回合中始终是维护来源。
    let execution = CollaborationExecutionId::try_new(&profile.business_execution_id).unwrap();
    document
        .capture_child_execution_profile(profile, 3)
        .unwrap();
    // Physical routing and terminal evidence use the same exact declared request.
    // 物理路由及终态证据使用同一精确声明请求。
    let request = ApiRequestSnapshot::new("provider", "model");
    // The actual allocated API owns the final assistant body.
    // 实际分配的 API 拥有最终助手正文。
    let api = document.start_api(purpose, request.clone(), 4).unwrap();
    document
        .append_assistant_text(
            api,
            "Original completed result",
            AssistantSemanticStatus::Completed,
            5,
        )
        .unwrap();
    document
        .complete_api_group(
            api,
            ApiCompletion {
                request,
                usage: crate::v1::stats::UsageSnapshot::default(),
                status: ApiCallStatus::Completed,
                end_reason: Some(ApiEndReason::CompletedSuccessfully),
                error: None,
                continuation_eligibility: ContinuationEligibility::Forbidden,
                timing: ApiTimings::default(),
                provider_source: None,
                completed_at_ms: 6,
            },
        )
        .unwrap();
    document.close_turn(TurnOutcome::Completed, 5, 7).unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("completed-idle").unwrap(),
            state: CollaborationRunControl::NaturallyIdle,
            changed_at_ms: 8,
        })
        .unwrap();
    (document, execution)
}

/// Source capture is read-only, requires actual publication, and survives strict serialized metadata reload.
/// 来源捕获只读、要求实际发布，并经元数据严格序列化重载保留。
/// Returns nothing; confirms exact original success without reopening its business turn.
/// 不返回值；确认精确原成功，不重开其业务回合。
#[test]
fn maintenance_source_requires_durable_success_and_survives_reload() {
    // A local completion is not a durable maintenance permission.
    // 本地完成不构成可靠维护许可。
    let (mut document, execution) = completed_child(ApiPurposeKind::MainGeneration);
    assert!(matches!(
        document.capture_child_context_maintenance_source(&execution),
        Err(DomainError::SubagentSourceNotDurable { .. })
    ));
    publish(&mut document);
    // Capture must not alter the completed business state or append an event.
    // 捕获不能改变已完成业务状态或追加事件。
    let before = document.clone();
    // The unforgeable source carries the actual successful original execution.
    // 不可伪造来源携带实际成功的原执行。
    let source = document
        .capture_child_context_maintenance_source(&execution)
        .unwrap()
        .unwrap();
    document
        .validate_child_context_maintenance_source(&source)
        .unwrap();
    assert_eq!(source.profile().business_execution_id, execution.as_str());
    assert_eq!(&document, &before);
    // Reconstruct through the same strict document loader used by stored state.
    // 通过存储状态使用的同一严格文档加载器重建。
    let loaded = SessionDocument::from_parts(
        serde_json::from_str(&serde_json::to_string(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    assert!(loaded.active_turn().is_none());
    loaded
        .validate_child_context_maintenance_source(&source)
        .unwrap();
    assert_eq!(
        loaded
            .capture_child_context_maintenance_source(&execution)
            .unwrap()
            .unwrap(),
        source
    );
}

/// Auxiliary text, stopped controls and a later business execution never become maintenance success authority.
/// 辅助正文、停止控制及后继业务执行绝不能成为维护成功权威。
/// Returns nothing; each rejection starts from independent published state.
/// 不返回值；每次拒绝从独立已发布状态开始。
#[test]
fn maintenance_source_rejects_auxiliary_stop_and_successor() {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
    ] {
        // Even a completed turn cannot promote an auxiliary final response into business success.
        // 即使回合已完成，也不能将辅助最终回复提升为业务成功。
        let (mut document, execution) = completed_child(purpose);
        publish(&mut document);
        assert!(
            document
                .capture_child_context_maintenance_source(&execution)
                .is_err()
        );
    }
    for control in [
        CollaborationRunControl::Stopped,
        CollaborationRunControl::MainOnly,
        CollaborationRunControl::MainAndSubagents,
    ] {
        // A valid old source cannot override later explicit stop, even when its result stays unchanged.
        // 有效旧来源不能覆盖后续明确停止，即使其结果未改变。
        let (mut document, execution) = completed_child(ApiPurposeKind::MainGeneration);
        publish(&mut document);
        // Capture before stopping proves stale tokens do not retain dispatch rights.
        // 停止前捕获，证明过期凭据不能保留分发权限。
        let source = document
            .capture_child_context_maintenance_source(&execution)
            .unwrap()
            .unwrap();
        document
            .set_collaboration_run_control(SetRunControlInput {
                operation_id: CollaborationOperationId::try_new("later-stop").unwrap(),
                state: control,
                changed_at_ms: 9,
            })
            .unwrap();
        publish(&mut document);
        assert!(
            document
                .validate_child_context_maintenance_source(&source)
                .is_err()
        );
        assert!(
            document
                .capture_child_context_maintenance_source(&execution)
                .is_err()
        );
    }
    // A later task supersedes old maintenance even before it has a newly captured profile.
    // 后继任务即使尚未捕获新档案，也会取代旧维护。
    let (mut document, execution) = completed_child(ApiPurposeKind::MainGeneration);
    publish(&mut document);
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("successor").unwrap(),
                "New business",
                vec![],
            ),
            9,
        )
        .unwrap();
    document.close_turn(TurnOutcome::Stopped, 1, 10).unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("successor-idle").unwrap(),
            state: CollaborationRunControl::NaturallyIdle,
            changed_at_ms: 11,
        })
        .unwrap();
    publish(&mut document);
    assert!(matches!(
        document.capture_child_context_maintenance_source(&execution),
        Err(DomainError::CollaborationStateViolation {
            field: "child_maintenance.business_superseded"
        })
    ));
}
