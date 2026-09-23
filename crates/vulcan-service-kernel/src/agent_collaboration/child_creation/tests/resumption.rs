//! Real-file publication and recovery of a successor whose parent intent is already durable.
//! 父侧意图已经可靠保存时的后继真实文件发布及恢复。

use super::*;
use vulcan_session_core::v1::child_execution::{ChildExecutionPreparation, ChildExecutionProfile};
use vulcan_session_core::v1::collaboration::CollaborationChildTaskRecord;
use vulcan_session_core::v1::context::LockedBaselineProjection;
use vulcan_session_core::v1::document::PrepareAndStartApiInput;
use vulcan_session_core::v1::event::{
    ApiCallStatus, ApiPurposeKind, ApiRequestSnapshot, EventBody, ProviderProjectionProof,
};
use vulcan_session_core::v1::projection::provider_visible_fingerprint;

/// Creates an existing child plus a durable parent-owned successor that has not yet modified the child.
/// 创建已有子任务及尚未修改子历史的可靠父侧后继。
/// Returns temporary storage, parent, live registry, original child, successor reservation and profile.
/// 返回临时存储、父任务、活动注册表、原子任务、后继预留及配置。
fn reserved_resumption_source() -> (
    tempfile::TempDir,
    Arc<SessionHandle>,
    Arc<SessionRuntimeRegistry>,
    Arc<SessionHandle>,
    CollaborationChildTaskRecord,
    ChildExecutionProfile,
) {
    let (temporary, parent, _, original, profile) =
        startup_recovery::native_execution::tests::reserved_creation_source();
    let registry = Arc::new(SessionRuntimeRegistry::default());
    publication::publish(&registry, &parent, &parent, &original, |_| Ok(())).unwrap();
    let child = registry
        .get(Uuid::parse_str(&original.child.session_id).unwrap())
        .unwrap();
    parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.terminal_child_task(TerminalChildTaskInput {
                    operation_id: CollaborationOperationId::try_new("prior-execution-terminal")
                        .unwrap(),
                    execution_id: original.execution_id,
                    task_id: original.task_id.clone(),
                    outcome: CollaborationChildTaskTerminalOutcome::Failed,
                    terminal_at_ms: 20,
                })
            },
        )
        .unwrap();
    // Leave the superseded physical API durable and open, as in a crash before successor publication.
    // 保留被替代物理 API 的可靠开放状态，模拟后继发布前崩溃。
    let api_preparation = child
        .read(|document| {
            let through_seq = document.events().last().unwrap().seq;
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request: ApiRequestSnapshot::new("provider", "model"),
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
                now_ms: 20,
            }
        })
        .unwrap();
    child
        .prepare_and_start_api_durable(api_preparation)
        .unwrap();
    let mut invocation_config = profile.invocation_config;
    invocation_config.prompt = "Resume the exact review".into();
    let preparation = ChildExecutionPreparation {
        task_id: child.session_id(),
        root_session_id: parent.session_id(),
        parent_session_id: parent.session_id(),
        execution_id: "resumed-review-execution".into(),
        runtime_identity: profile.runtime_identity,
        invocation_config,
        resolved_config: profile.resolved_config,
        effective_config: profile.effective_config,
        runtime_policy: profile.runtime_policy,
        execution_agent_id: profile.execution_agent_id,
        connection_id: profile.connection_id,
        temperature: profile.temperature,
        admitted_tools: profile.admitted_tools,
        request_overrides: profile.request_overrides,
        fixed_agent_prompt: profile.fixed_agent_prompt,
        prompt_context: profile.prompt_context,
    };
    let (_, intent) = child
        .read_full_durable(|document| document.prepare_child_resumption(preparation, 21))
        .unwrap()
        .unwrap();
    let profile = intent.profile.clone();
    let record = parent
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.reserve_child_creation(
                    ReserveChildTaskInput {
                        source_tool_name: "send_message".into(),
                        operation_id: CollaborationOperationId::try_new(
                            "resumed-review-reservation",
                        )
                        .unwrap(),
                        execution_id: CollaborationExecutionId::try_new(
                            &profile.business_execution_id,
                        )
                        .unwrap(),
                        task_id: original.task_id,
                        parent_turn_id: original.parent_turn_id,
                        child: original.child,
                        reserved_at_ms: 21,
                    },
                    intent,
                )
            },
        )
        .unwrap()
        .value
        .into_value()
        .task;
    (temporary, parent, registry, child, record, profile)
}

/// Real successor publication preserves previous root history and cleans only the new turn on listener failure.
/// 真实后继发布保留前序根历史，监听失败时只清理新回合。
/// Returns nothing; exact replay preserves revision and both listener outcomes retain original identity.
/// 不返回值；精确重放保留修订，两种监听结果均保留原身份。
#[test]
fn child_resumption_publication_preserves_history_and_cleans_its_own_turn() {
    for reject in [false, true] {
        let (_temporary, parent, registry, child, record, profile) = reserved_resumption_source();
        let (before_revision, before_events) = child
            .read_full_durable(|document| (document.durable_revision(), document.events().to_vec()))
            .unwrap();
        let result = publication::publish(&registry, &parent, &parent, &record, |admitted| {
            assert!(Arc::ptr_eq(&admitted, &child));
            let disk = load_session_v1_for_runtime(&admitted.storage_locator().unwrap())
                .unwrap()
                .document;
            assert!(disk.events().starts_with(&before_events));
            assert_eq!(disk.durable_revision(), before_revision + 1);
            assert_eq!(
                disk.metadata.child_execution_profile.as_ref(),
                Some(&profile)
            );
            if reject {
                Err("Successor listener rejected".into())
            } else {
                Ok(())
            }
        });
        if reject {
            assert_eq!(result.unwrap_err(), "Successor listener rejected");
            child
                .read_full_durable(|document| {
                    assert!(document.active_turn().is_none_or(|turn| turn.closed));
                    assert!(document.events().iter().any(|event| {
                        event.turn_id == Some(profile.business_turn_id)
                            && matches!(
                                &event.event,
                                vulcan_session_core::v1::event::EventBody::TurnBoundary(_)
                            )
                    }));
                    assert!(document.events().starts_with(&before_events));
                })
                .unwrap();
        } else {
            result.unwrap();
            publication::publish(&registry, &parent, &parent, &record, |_| Ok(())).unwrap();
            assert_eq!(
                child.read(|document| document.durable_revision()).unwrap(),
                before_revision + 1
            );
        }
    }
}

/// Cold loading applies the persisted successor before generic interrupted-process projection.
/// 冷加载在通用进程中断投影之前应用持久后继。
/// Returns nothing; repeated loading retains one canonical handle and never replays the successor root.
/// 不返回值；重复加载保留唯一句柄，绝不重放后继根输入。
#[test]
fn child_resumption_cold_publication_reuses_original_intent() {
    let (_temporary, parent, _old_registry, child, record, profile) = reserved_resumption_source();
    let locator = child.storage_locator().unwrap();
    let before = child
        .read_full_durable(|document| document.events().to_vec())
        .unwrap();
    let generation = parent
        .read(|document| document.segment_generation())
        .unwrap();
    let registry = Arc::new(SessionRuntimeRegistry::default());
    assert!(
        startup_recovery::tree::resumption::load(
            &registry,
            &parent,
            generation + 1,
            &parent,
            locator.clone(),
            std::slice::from_ref(&record),
        )
        .is_err()
    );
    assert!(registry.get(child.session_id()).is_err());
    let recovered = startup_recovery::tree::resumption::load(
        &registry,
        &parent,
        generation,
        &parent,
        locator.clone(),
        std::slice::from_ref(&record),
    )
    .unwrap();
    let revision = recovered
        .read_full_durable(|document| {
            assert!(document.events().starts_with(&before));
            assert!(document.active_api().is_none());
            assert!(document.events().iter().any(|event| {
                matches!(&event.event, EventBody::ApiStats(stats) if stats.status == ApiCallStatus::Interrupted)
            }));
            assert_eq!(
                document.metadata.child_execution_profile.as_ref(),
                Some(&profile)
            );
            assert!(
                document
                    .child_business_recovery_profile(&record.execution_id)
                    .is_ok()
            );
            document.durable_revision()
        })
        .unwrap();
    let repeated = startup_recovery::tree::resumption::load(
        &registry,
        &parent,
        generation,
        &parent,
        locator,
        &[record],
    )
    .unwrap();
    assert!(Arc::ptr_eq(&repeated, &recovered));
    assert_eq!(
        repeated
            .read(|document| document.durable_revision())
            .unwrap(),
        revision
    );
}

/// A parent stop before successor publication cancels only the reserved successor.
/// 后继发布前父会话停止时，只取消已预留的后继。
/// Real disk assertions retain predecessor history and prove idempotent parent settlement.
/// 真实磁盘断言保留前序历史，并证明父侧结算幂等。
#[tokio::test]
async fn stopped_unpublished_child_resumption_releases_parent_without_rewriting_child() {
    // This successor is durably reserved while its predecessor still owns the physical history.
    // 此后继已可靠预留，而前序仍拥有物理历史。
    let (_temporary, parent, registry, child, record, _) = reserved_resumption_source();
    registry.register(parent.clone()).unwrap();
    parent.set_collaboration_run_control_durable(
        vulcan_session_core::v1::collaboration::SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("stop-before-successor").unwrap(),
            state: CollaborationRunControl::MainAndSubagents,
            changed_at_ms: 22,
        },
    ).unwrap();
    // Capture physical source events before recovery; no successor root is present.
    // 恢复前捕获物理来源事件；此时没有后继根输入。
    let before = child.read_full_durable(|document| (
        document.durable_revision(), document.events().to_vec(),
    )).unwrap();
    // Use a service owner matching the canonical registry without starting an inference runtime.
    // 使用与规范注册表匹配的服务所有者，不启动推理运行时。
    let mut service = crate::tests::test_support::test_service_state_with_parent_session(parent.session_id());
    service.session_runtime_registry = registry.clone();
    // The normal recovery entry takes ownership from this exact service state.
    // 正常恢复入口从此精确服务状态取得所有权。
    let state = Arc::new(Mutex::new(service));
    // The root and direct parent coincide for this concrete successor.
    // 此具体后继的根与直接父会话相同。
    let generation = parent.read(|document| document.segment_generation()).unwrap();
    assert_eq!(startup_recovery::native_results::recover_terminal_returns(
        &state, &registry, parent.clone(), generation, parent.clone(), child.clone(), vec![record.clone()],
    ).await.unwrap(), 1);
    // Parent terminal evidence has no fabricated child response or activation.
    // 父终态凭据没有虚构子回复或激活。
    let restored = load_session_v1_for_runtime(&parent.storage_locator().unwrap()).unwrap().document;
    // Stable execution identity selects the exact successor obligation.
    // 稳定执行身份选择精确后继义务。
    let terminal = &restored.collaboration_state().child_tasks[&record.execution_id];
    assert_eq!(terminal.terminal_outcome, Some(CollaborationChildTaskTerminalOutcome::Stopped));
    assert!(terminal.active_at_ms.is_none());
    assert!(terminal.non_success_result.is_none());
    assert!(restored.completion_obligations(record.parent_turn_id).open_child_task_ids.is_empty());
    assert_eq!(startup_recovery::native_results::recover_terminal_returns(
        &state, &registry, parent.clone(), generation, parent.clone(), child.clone(), vec![record],
    ).await.unwrap(), 0);
    // Source bytes remain unchanged after both recovery attempts.
    // 两次恢复尝试后来源字节保持不变。
    let after = load_session_v1_for_runtime(&child.storage_locator().unwrap()).unwrap().document;
    assert_eq!(after.durable_revision(), before.0);
    assert_eq!(after.events(), before.1.as_slice());
}

/// A changed source is rejected in live and cold publication without adopting the predecessor for cleanup.
/// 来源变化时实时和冷发布均拒绝，且不取得清理前序的权限。
/// Returns nothing; stopped predecessor history remains byte-for-byte equivalent after both attempts.
/// 不返回值；两次尝试之后停止的前序历史保持完全一致。
#[test]
fn child_resumption_publication_rejects_changed_history() {
    let (_temporary, parent, registry, child, record, _) = reserved_resumption_source();
    child
        .mutate_durable(
            SessionMutationAdmission::ControlPlane,
            SessionFlushReason::MetadataChanged,
            |document| {
                document.set_collaboration_run_control(
                    vulcan_session_core::v1::collaboration::SetRunControlInput {
                        operation_id: CollaborationOperationId::try_new("later-child-stop")
                            .unwrap(),
                        state: CollaborationRunControl::Stopped,
                        changed_at_ms: 22,
                    },
                )
            },
        )
        .unwrap();
    let before = child
        .read_full_durable(|document| {
            (
                serde_json::to_value(&document.metadata).unwrap(),
                document.events().to_vec(),
            )
        })
        .unwrap();
    assert!(
        publication::publish(&registry, &parent, &parent, &record, |_| panic!(
            "changed source"
        ))
        .is_err()
    );
    let cold = Arc::new(SessionRuntimeRegistry::default());
    let generation = parent
        .read(|document| document.segment_generation())
        .unwrap();
    assert!(
        startup_recovery::tree::resumption::load(
            &cold,
            &parent,
            generation,
            &parent,
            child.storage_locator().unwrap(),
            &[record],
        )
        .is_err()
    );
    let disk = load_session_v1_for_runtime(&child.storage_locator().unwrap())
        .unwrap()
        .document;
    assert_eq!(serde_json::to_value(&disk.metadata).unwrap(), before.0);
    assert_eq!(disk.events(), before.1.as_slice());
}
