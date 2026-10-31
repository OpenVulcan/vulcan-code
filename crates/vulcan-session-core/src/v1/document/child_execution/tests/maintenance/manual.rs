//! Manual root compaction must have an independent, recoverable owner and durable error feedback.
//! 手动主会话压缩必须拥有独立可恢复的所有者和可靠错误反馈。

use super::*;
use crate::v1::context_maintenance::ContextMaintenanceExecution;

/// Builds and publishes an idle root with a failed business turn; returns its canonical document.
/// 构造并发布包含失败业务回合的空闲主会话，返回规范文档。
fn idle_root() -> SessionDocument {
    // The existing business history remains the source throughout all auxiliary tests.
    // 既有业务历史在所有辅助测试中始终作为来源。
    let mut document = SessionDocument::new("manual-root", "project", "Manual", 1);
    document
        .submit_root_input_fixture("Preserve the task", Vec::new(), 2)
        .unwrap();
    document
        .fail_execution_boundary_and_close_turn(
            "fixture_business_failed",
            Some("Fixture failure"),
            3,
        )
        .unwrap();
    publish(&mut document);
    document
}

/// A manual request after a failed turn can dispatch a compaction API and commit an independent summary.
/// 失败回合后的手动请求可以分发压缩 API，并提交独立摘要。
/// Returns nothing; checks source preservation, exact ownership, closure and cold reload.
/// 不返回值；检查来源保留、精确归属、闭合与冷重载。
#[test]
fn manual_compaction_opens_real_api_owner_and_preserves_business() {
    // Start from the same idle failure boundary that previously produced NoActiveTurn.
    // 从此前触发 NoActiveTurn 的同类空闲失败边界开始。
    let mut document = idle_root();
    // Original events are immutable evidence, located by their stable sequence identities.
    // 原事件是不可变证据，按稳定序号身份定位。
    let original = document.events().to_vec();
    // This identity owns only the explicit maintenance attempt.
    // 此身份仅拥有显式维护尝试。
    let execution_id = CollaborationExecutionId::try_new("manual-success").unwrap();
    // The real admission receipt binds the new turn to the exact closed business turn.
    // 真实准入回执将新回合绑定到精确已关闭业务回合。
    let started = document
        .begin_manual_context_maintenance(
            &document
                .capture_manual_context_maintenance_source()
                .unwrap(),
            execution_id.clone(),
            10,
        )
        .unwrap()
        .into_value();
    assert!(started.business_execution_id.is_none());
    assert_ne!(started.turn_id, started.business_turn_id);
    publish(&mut document);
    document = reload(&document);
    assert_eq!(
        document.active_turn().unwrap().origin,
        TurnOrigin::ContextMaintenance
    );
    // The API is prepared through Core's real turn admission rather than an invented user prompt.
    // API 经 Core 真实回合准入准备，不构造用户提示词。
    let request = ApiRequestSnapshot::new("provider", "model");
    // The physical API belongs exclusively to the new maintenance turn.
    // 物理 API 仅属于新维护回合。
    let api = document
        .start_api(ApiPurposeKind::Compaction, request.clone(), 11)
        .unwrap();
    // Only a completed semantic summary may become a compaction checkpoint.
    // 只有完成的语义摘要可以成为压缩检查点。
    let summary = document
        .append_assistant_text(
            api,
            "Retained task state",
            AssistantSemanticStatus::Completed,
            12,
        )
        .unwrap()
        .into_value();
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
    document
        .record_compaction(
            CompactionCommitInput::new(
                api,
                summary,
                original.first().unwrap().seq,
                "Retained task state",
                LockedBaselineProjection::empty(),
            ),
            14,
        )
        .unwrap();
    document
        .finish_context_maintenance(&execution_id, TurnClosureReason::Completed, 15)
        .unwrap();
    for event in &original {
        assert_eq!(
            document.events().iter().find(|row| row.seq == event.seq),
            Some(event)
        );
    }
    assert_eq!(
        document
            .events()
            .iter()
            .filter(|row| matches!(row.event, EventBody::UserInput(_)))
            .count(),
        1
    );
    publish(&mut document);
    document = reload(&document);
    assert!(document.active_turn().is_none());
    assert!(
        document
            .metadata
            .context_maintenance
            .unwrap()
            .terminal
            .unwrap()
            .compaction_id
            .is_some()
    );
}

/// Pre-dispatch failures remain visible and retryable after any number of previous manual failures.
/// 分发前失败保持可见，且任意多次先前手动失败后仍可重试。
/// Returns nothing; checks UI-only diagnostics, idempotent settlement, unchanged history and reload.
/// 不返回值；检查仅界面诊断、幂等结算、历史不变和重载。
#[test]
fn manual_compaction_failure_is_durable_and_retryable_without_api() {
    // Each attempt reuses the closed business source without reusing an auxiliary execution identity.
    // 每次尝试复用已关闭业务来源，不复用辅助执行身份。
    let mut document = idle_root();
    for attempt in 0..4 {
        // The generated execution identity is local to this regression fixture.
        // 生成的执行身份仅用于本回归夹具。
        let execution_id =
            CollaborationExecutionId::try_new(format!("manual-retry-{attempt}")).unwrap();
        // Monotonic fixture timestamps keep each independent retry distinguishable.
        // 单调夹具时间戳使每次独立重试可区分。
        let stamp = 10 + attempt * 3;
        document
            .begin_manual_context_maintenance(
                &document
                    .capture_manual_context_maintenance_source()
                    .unwrap(),
                execution_id,
                stamp,
            )
            .unwrap();
        assert!(
            document
                .fail_execution_boundary_and_close_turn(
                    "context_compaction_failed",
                    Some("Fixture preparation failure"),
                    stamp + 1
                )
                .unwrap()
        );
        assert!(
            !document
                .fail_execution_boundary_and_close_turn(
                    "context_compaction_failed",
                    Some("Fixture preparation failure"),
                    stamp + 1
                )
                .unwrap()
        );
        publish(&mut document);
        document = reload(&document);
        assert!(document.active_turn().is_none());
    }
    assert_eq!(document.events().iter().filter(|event| matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "context_compaction_failed") && event.visibility == EventVisibility::UiOnly).count(), 4);
    assert!(
        !document
            .events()
            .iter()
            .any(|event| matches!(event.event, EventBody::ApiStarted(_)))
    );
}

/// A stale plan cannot revive a stopped or newly edited source, and restart settles admitted maintenance once.
/// 过期计划不能复活已停止或新编辑的来源，重启只结算已准入维护一次。
/// Returns nothing; verifies freshness rejection without mutation and cancellation/recovery closure.
/// 不返回值；验证新鲜度拒绝没有修改，以及取消和恢复闭合。
#[test]
fn manual_compaction_rejects_stale_admission_and_recovers_interruption() {
    // Capture the exact source before an independent durable change.
    // 在独立可靠变化前捕获精确来源。
    let mut document = idle_root();
    // An explicit edit invalidates the earlier compaction admission proof.
    // 显式编辑使较早压缩准入证明失效。
    let stale = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    document.rename_session("Changed", 4).unwrap();
    publish(&mut document);
    // Rejected admission must preserve every previously published event.
    // 被拒绝准入必须保留每条先前已发布事件。
    let before = document.events().to_vec();
    assert!(
        document
            .begin_manual_context_maintenance(
                &stale,
                CollaborationExecutionId::try_new("stale").unwrap(),
                5
            )
            .is_err()
    );
    assert_eq!(document.events(), before);
    document
        .begin_manual_context_maintenance(
            &document
                .capture_manual_context_maintenance_source()
                .unwrap(),
            CollaborationExecutionId::try_new("restart").unwrap(),
            6,
        )
        .unwrap();
    publish(&mut document);
    document = reload(&document);
    assert_eq!(document.project_persisted_interruptions().unwrap(), 1);
    assert_eq!(document.project_persisted_interruptions().unwrap(), 0);
    publish(&mut document);
    document = reload(&document);
    assert!(document.active_turn().is_none());
    assert!(document.events().iter().any(|event| matches!(&event.event, EventBody::DisplayNote(note) if note.reason_code == "context_maintenance_process_interrupted")));
    document
        .begin_manual_context_maintenance(
            &document
                .capture_manual_context_maintenance_source()
                .unwrap(),
            CollaborationExecutionId::try_new("after-restart").unwrap(),
            10,
        )
        .unwrap();
    document.close_turn(TurnOutcome::Stopped, 1, 11).unwrap();
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
        TurnClosureReason::Stopped
    );
}

/// Clearing the slash-command draft must not invalidate an unchanged compaction source.
/// 清空斜杠命令草稿不得使未变化的压缩来源失效。
/// Returns nothing; reproduces draft writes between planning and actual maintenance admission.
/// 不返回值；复现规划与实际维护准入之间的草稿写入。
#[test]
fn manual_compaction_admits_after_slash_draft_is_saved_and_cleared() {
    // The original source is fully published before the composer changes its local draft.
    // 输入框改变本地草稿前，原始来源已完整发布。
    let mut document = idle_root();
    // The source proof retains exact history while draft persistence changes only bookkeeping.
    // 来源证明保留精确历史，而草稿持久化仅改变记账。
    let source = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    document.set_draft("/compact", Vec::new(), 4).unwrap();
    publish(&mut document);
    document.clear_draft(5).unwrap();
    publish(&mut document);
    document
        .begin_manual_context_maintenance(
            &source,
            CollaborationExecutionId::try_new("slash-draft-race").unwrap(),
            6,
        )
        .unwrap();
    assert_eq!(
        document
            .metadata
            .context_maintenance
            .as_ref()
            .unwrap()
            .source_revision,
        source.metadata.document_revision
    );
    assert!(document.metadata.input_state.draft.text.is_empty());
    assert_eq!(
        document.active_turn().unwrap().origin,
        TurnOrigin::ContextMaintenance
    );
}

/// Unsaved text remains a draft when manual maintenance starts, including a pending metadata flush.
/// 手动维护开始时未发送文本保持为草稿，包括尚未刷新的元数据。
/// Returns nothing; checks both persisted and pending draft writes without submitting user input.
/// 不返回值；检查已持久化和待刷新草稿写入，且不提交用户输入。
#[test]
fn manual_compaction_preserves_draft_without_consuming_it() {
    for flush_draft in [false, true] {
        // Each case starts with one published business history and an independent source proof.
        // 每种情况从一份已发布业务历史和独立来源证明开始。
        let mut document = idle_root();
        // Capture before the composer edits text for its next business request.
        // 在输入框为下一业务请求编辑文本之前捕获。
        let source = document
            .capture_manual_context_maintenance_source()
            .unwrap();
        document
            .set_draft("Next task still being edited", Vec::new(), 4)
            .unwrap();
        if flush_draft {
            publish(&mut document);
        }
        // Admission may publish the pending draft but must never clear or promote it.
        // 准入可以发布待刷新草稿，但绝不能清空或提升它。
        let draft = document.metadata.input_state.draft.clone();
        document
            .begin_manual_context_maintenance(
                &source,
                CollaborationExecutionId::try_new("retained-draft").unwrap(),
                5,
            )
            .unwrap();
        publish(&mut document);
        document = reload(&document);
        assert_eq!(document.metadata.input_state.draft, draft);
        assert_eq!(
            document
                .events()
                .iter()
                .filter(|event| matches!(event.event, EventBody::UserInput(_)))
                .count(),
            1
        );
    }
}

/// Planning cannot certify a source before its pending writes have been published.
/// 在待处理写入发布前，规划不能认证来源。
/// Returns nothing; verifies the registry must flush before capturing admission evidence.
/// 不返回值；验证注册表必须先刷新再捕获准入证据。
#[test]
fn manual_compaction_source_requires_durable_capture() {
    // Keep a real metadata mutation unpublished to exercise the source-capture boundary.
    // 保留一个尚未发布的真实元数据修改，以覆盖来源捕获边界。
    let mut document = idle_root();
    document.set_draft("/compact", Vec::new(), 4).unwrap();
    assert!(matches!(
        document.capture_manual_context_maintenance_source(),
        Err(DomainError::CollaborationStateViolation {
            field: "manual_maintenance.source_unpublished"
        })
    ));
    publish(&mut document);
    assert!(document.capture_manual_context_maintenance_source().is_ok());
}

/// A draft edit cannot hide a model selection change from the compaction admission guard.
/// 草稿编辑不能向压缩准入检查隐藏模型选择变化。
/// Returns nothing; verifies a metadata-only model change rejects without starting maintenance.
/// 不返回值；验证仅元数据模型变化会拒绝请求且不启动维护。
#[test]
fn manual_compaction_rejects_model_change_alongside_draft() {
    use crate::v1::config::PendingRequestConfig;
    // Freeze the original model selection with its already published history.
    // 将原模型选择与其已发布历史一同冻结。
    let mut document = idle_root();
    // The proof precedes both independent UI writes.
    // 此证明早于两个独立界面写入。
    let source = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    document.set_draft("/compact", Vec::new(), 4).unwrap();
    document
        .set_pending_request(
            PendingRequestConfig {
                provider: Some("changed-provider".to_owned()),
                connection_id: None,
                model: Some("changed-model".to_owned()),
                thinking_level: None,
                temperature: None,
                set_at_ms: 4,
            },
            4,
        )
        .unwrap();
    publish(&mut document);
    // Rejection must preserve the exact new configuration and all published history.
    // 拒绝必须保留精确的新配置与全部已发布历史。
    let before = document.clone();
    assert!(matches!(
        document.begin_manual_context_maintenance(
            &source,
            CollaborationExecutionId::try_new("changed-model").unwrap(),
            5
        ),
        Err(DomainError::CollaborationStateViolation {
            field: "manual_maintenance.source_changed"
        })
    ));
    assert_eq!(document.metadata, before.metadata);
    assert_eq!(document.events(), before.events());
    assert!(document.metadata.context_maintenance.is_none());
}

/// Stop wins a race with stale manual preparation even when a draft save happened concurrently.
/// 即使同时发生草稿保存，停止仍优先于过期的手动准备。
/// Returns nothing; checks run-control preservation and absence of an auxiliary owner after rejection.
/// 不返回值；检查拒绝后运行控制保持不变且没有辅助所有者。
#[test]
fn manual_compaction_rejects_stop_alongside_draft() {
    use crate::v1::collaboration::{
        CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
    };
    // Start from a genuine idle source, before the explicit stop signal arrives.
    // 从真实空闲来源开始，此时显式停止信号尚未到达。
    let mut document = idle_root();
    // Admission must remain tied to the pre-stop control state.
    // 准入必须仍绑定停止之前的控制状态。
    let source = document
        .capture_manual_context_maintenance_source()
        .unwrap();
    document.set_draft("/compact", Vec::new(), 4).unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("manual-stop-race").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: 4,
        })
        .unwrap();
    publish(&mut document);
    // Capture the post-stop revision so rejected admission cannot silently reopen execution.
    // 捕获停止后的修订，使被拒绝准入不能静默重开执行。
    let revision = document.memory_revision();
    assert!(matches!(
        document.begin_manual_context_maintenance(
            &source,
            CollaborationExecutionId::try_new("stopped-plan").unwrap(),
            5
        ),
        Err(DomainError::CollaborationStateViolation {
            field: "manual_maintenance.source_changed"
        })
    ));
    assert_eq!(document.memory_revision(), revision);
    assert_eq!(
        document.collaboration_run_control(),
        CollaborationRunControl::Stopped
    );
    assert!(document.metadata.context_maintenance.is_none());
}

/// Input arriving during manual maintenance stays queued and cannot be consumed by a summary request.
/// 手动维护期间到达的输入保持排队，不能被摘要请求消费。
/// Returns nothing; verifies both insertion entry points reject without changing input or business ownership.
/// 不返回值；验证两个插队入口都拒绝且不改变输入或业务归属。
#[test]
fn manual_compaction_keeps_arriving_user_input_queued() {
    use crate::v1::{ClientActionId, ClientInputSubmission, config::QueuedInputIdentity};
    // The auxiliary owner is distinct from the retained failed business.
    // 辅助所有者与保留的失败业务不同。
    let mut document = idle_root();
    document
        .begin_manual_context_maintenance(
            &document
                .capture_manual_context_maintenance_source()
                .unwrap(),
            CollaborationExecutionId::try_new("input-guard").unwrap(),
            10,
        )
        .unwrap();
    // Capture business observations before enqueueing during the auxiliary turn.
    // 在辅助回合期间排队前捕获业务观察状态。
    let observed = document.metadata.business_hook.observed_inputs.clone();
    // This action remains the user's real queued input after rejected insertion.
    // 插队被拒绝后，此动作仍是用户真实排队输入。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("queued-during-compaction").unwrap(),
        "Run after compaction",
        Vec::new(),
    );
    // Use the real admission receipt to preserve exact queue identity.
    // 使用真实准入回执保留精确队列身份。
    let queued = document
        .enqueue_input(submission.clone(), 11)
        .unwrap()
        .into_value();
    // Rejection must not change the queue, revision or history.
    // 拒绝不得改变队列、修订或历史。
    let revision = document.memory_revision();
    assert!(matches!(
        document.promote_queued_input_to_pending_turn(
            QueuedInputIdentity {
                client_action_id: queued.client_action_id,
                input_id: queued.input_id
            },
            12
        ),
        Err(DomainError::CollaborationStateViolation {
            field: "context_maintenance.business_inputs"
        })
    ));
    assert!(matches!(
        document.add_pending_in_turn_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("direct-insertion").unwrap(),
                "Do not consume this",
                Vec::new()
            ),
            12
        ),
        Err(DomainError::CollaborationStateViolation {
            field: "context_maintenance.business_inputs"
        })
    ));
    assert_eq!(document.memory_revision(), revision);
    assert!(
        document
            .metadata
            .input_state
            .pending_in_turn_inputs
            .is_empty()
    );
    assert_eq!(document.metadata.input_state.queued_inputs.len(), 1);
    assert_eq!(
        document
            .metadata
            .input_state
            .queued_inputs
            .first()
            .unwrap()
            .client_action_id,
        submission.client_action_id
    );
    assert_eq!(document.metadata.business_hook.observed_inputs, observed);
    document.close_turn(TurnOutcome::Stopped, 3, 13).unwrap();
    publish(&mut document);
    document = reload(&document);
    assert_eq!(document.metadata.input_state.queued_inputs.len(), 1);
    assert!(document.active_turn().is_none());
}

/// Root maintenance explicitly encodes no child identity, while omission still violates the durable schema.
/// 主会话维护显式编码无子身份，而字段缺失仍违反持久模式。
/// Returns nothing; verifies nullable-field strictness without locking prose or catalog contents.
/// 不返回值；验证可空字段的严格性，不锁定文案或目录内容。
#[test]
fn manual_compaction_requires_explicit_nullable_child_identity() {
    // Serialize a real admitted record, then remove only the required identity field.
    // 序列化真实准入记录，随后仅删除必填身份字段。
    let mut document = idle_root();
    // Root admission emits the exact record shape accepted by the new schema.
    // 主会话准入产生新模式接纳的精确记录形状。
    let record = document
        .begin_manual_context_maintenance(
            &document
                .capture_manual_context_maintenance_source()
                .unwrap(),
            CollaborationExecutionId::try_new("schema").unwrap(),
            10,
        )
        .unwrap()
        .into_value();
    // Remove only the identity field to distinguish absent from explicitly null.
    // 仅移除身份字段，以区分缺失与显式空值。
    let mut value = serde_json::to_value(record).unwrap();
    assert!(
        serde_json::from_value::<ContextMaintenanceExecution>(value.clone())
            .unwrap()
            .business_execution_id
            .is_none()
    );
    value
        .as_object_mut()
        .unwrap()
        .remove("business_execution_id");
    assert!(serde_json::from_value::<ContextMaintenanceExecution>(value).is_err());
}
