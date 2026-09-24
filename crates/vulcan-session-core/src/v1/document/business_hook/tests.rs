//! Durable Hook ownership regression tests using actual business transitions.
//! 使用实际业务转换的可靠 Hook 归属回归测试。

use super::*;
use crate::v1::collaboration::{
    CollaborationOperationId, CollaborationRunControl, SetRunControlInput,
};

/// First preparation keeps the original human start after steering and refuses prior business or Hook ownership.
/// 首次准备在方向调整后保留原人类起点，并拒绝此前业务或 Hook 归属。
/// Uses actual publication, auxiliary interruption and business transitions without modifying recovery facts.
/// 使用真实发布、辅助中断及业务转换，不修改恢复凭据。
#[test]
fn initial_human_business_proves_original_unexecuted_input() {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
        ApiPurposeKind::MainGeneration,
    ] {
        // The canonical root must be published before it can grant any startup authority.
        // 规范根必须先发布，才能授予启动权威。
        let (mut document, binding) = fixture();
        assert!(document.main_business_initial_human_input().is_err());
        document
            .add_pending_in_turn_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("later-direction").unwrap(),
                    "Later direction",
                    vec![],
                ),
                4,
            )
            .unwrap();
        document
            .start_api(purpose, ApiRequestSnapshot::new("provider", "model"), 5)
            .unwrap();
        document.interrupt_active_api_for_process(6).unwrap();
        // Flush through the real revision owner, rather than assigning durability fields.
        // 通过真实修订所有者刷写，不直接赋值可靠字段。
        let revision = document
            .prepare_flush_candidates()
            .unwrap()
            .unwrap()
            .revision;
        document.advance_durable(revision).unwrap();
        let before = document.clone();
        if purpose == ApiPurposeKind::MainGeneration {
            assert!(matches!(
                document.main_business_initial_human_input(),
                Err(DomainError::CollaborationStateViolation {
                    field: "initial_business.prior_execution"
                })
            ));
        } else {
            // Later direction and completed auxiliary history cannot replace the original root source.
            // 后续方向及已结束辅助历史不能替换原根来源。
            let source = document.main_business_initial_human_input().unwrap();
            assert_eq!(source.preparation.turn_id, binding.turn_id);
            assert_eq!(source.preparation.start_seq, binding.start_seq);
            assert_eq!(source.input.text, "original work");
            assert_eq!(source.input.client_action_id.as_str(), "root");
            assert_eq!(source.preparation.revision, revision);
        }
        assert_eq!(document, before);
        document.capture_business_hook_binding(binding, 7).unwrap();
        let revision = document
            .prepare_flush_candidates()
            .unwrap()
            .unwrap()
            .revision;
        document.advance_durable(revision).unwrap();
        assert!(document.main_business_initial_human_input().is_err());
    }
}

/// A scheduled successor can initialize its own Hook while retaining a previous turn's historical binding.
/// 已调度后继可以初始化自己的 Hook，同时保留前一回合的历史绑定。
/// The source proof remains exact and refuses explicitly stopped ownership.
/// 来源凭据保持精确，并拒绝明确停止的归属。
#[test]
fn initial_human_business_accepts_exact_queued_successor() {
    // Keep a real historical Hook to prove that initialization is scoped to the current turn.
    // 保留真实历史 Hook，证明初始化按当前回合划定范围。
    let (mut document, binding) = fixture();
    document
        .capture_business_hook_binding(binding.clone(), 4)
        .unwrap();
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    document.create_goal("Goal", None, "Continue", 6).unwrap();
    let queued = document
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("queued-initial").unwrap(),
                "Queued original",
                vec![],
            ),
            7,
        )
        .unwrap()
        .into_value();
    let EnqueueInputDisposition::SupersededGoalAndScheduled {
        scheduled_turn_id,
        input_seq,
        ..
    } = queued.disposition
    else {
        panic!("expected queued successor");
    };
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    let source = document.main_business_initial_human_input().unwrap();
    assert_eq!(source.preparation.turn_id, scheduled_turn_id);
    assert_eq!(source.preparation.start_seq, input_seq);
    assert_eq!(source.input.input_source, UserInputSource::QueuedScheduled);
    assert_eq!(document.metadata.business_hook.binding, Some(binding));
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("stop-initial").unwrap(),
            state: CollaborationRunControl::Stopped,
            changed_at_ms: 8,
        })
        .unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(document.main_business_initial_human_input().is_err());
}

/// Automatic main recovery preserves its original Hook and refuses uncertain external tool effects.
/// 自动主恢复保留原 Hook，并拒绝不确定的外部工具效果。
/// Uses actual API interruption and publication; returns no value.
/// 使用真实 API 中断及发布；无返回值。
#[test]
fn main_business_recovery_requires_confirmed_tool_boundary() {
    for has_tool in [false, true] {
        let (mut document, binding) = fixture();
        document
            .capture_business_hook_binding(binding.clone(), 4)
            .unwrap();
        let api = document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("provider", "model"),
                5,
            )
            .unwrap();
        if has_tool {
            document
                .start_tool(
                    api,
                    "shell_command",
                    crate::v1::ToolPayload::empty(),
                    None,
                    6,
                )
                .unwrap();
        }
        document.interrupt_active_api_for_process(7).unwrap();
        let revision = document
            .prepare_flush_candidates()
            .unwrap()
            .unwrap()
            .revision;
        document.advance_durable(revision).unwrap();
        let before = document.clone();
        if has_tool {
            assert!(matches!(
                document.main_business_recovery_state(),
                Err(DomainError::CollaborationStateViolation {
                    field: "main_business_recovery.unconfirmed_tool_result"
                })
            ));
        } else {
            assert_eq!(
                document.main_business_recovery_state().unwrap().binding,
                Some(binding)
            );
        }
        assert_eq!(document, before);
    }
}

/// Archived main business cannot be resumed by its retained Hook seed.
/// 归档主业务不能凭保留的 Hook 种子恢复。
/// The read-only recovery check must preserve the complete durable document.
/// 只读恢复检查必须保留完整可靠文档。
#[test]
fn main_business_recovery_rejects_archived_owner() {
    let (mut document, binding) = fixture();
    document.capture_business_hook_binding(binding, 4).unwrap();
    document.metadata.archived_at_ms = Some(5);
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    let before = document.clone();
    assert!(matches!(
        document.main_business_recovery_state(),
        Err(DomainError::CollaborationStateViolation {
            field: "main_business_recovery.owner"
        })
    ));
    assert_eq!(document, before);
}

/// Continue persists optional human input once, while a pure Continue creates no observation.
/// Continue 仅持久化一次可选人类输入，纯继续不创建观察。
/// Uses real interruption eligibility, exact retry and strict document reconstruction.
/// 使用真实中断资格、精确重试和严格文档重建。
#[test]
fn business_hook_explicit_continue_preserves_optional_input() {
    for text in ["", "@inject 2 rule\nSupplement"] {
        // The original Hook remains bound to the same business turn through an explicit continuation.
        // 显式继续期间原 Hook 始终绑定同一业务回合。
        let (mut document, binding) = fixture();
        document
            .capture_business_hook_binding(binding.clone(), 4)
            .unwrap();
        document
            .start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                5,
            )
            .unwrap();
        document.interrupt_active_api_for_process(6).unwrap();
        let request = ContinuationRequest::new(
            ClientActionId::try_new("explicit-continue").unwrap(),
            text,
            vec![],
        );
        let receipt = document
            .continue_open_turn(request.clone(), 7)
            .unwrap()
            .into_value();
        let before_retry = document.clone();
        assert_eq!(
            document
                .continue_open_turn(request, 8)
                .unwrap()
                .into_value(),
            receipt
        );
        assert_eq!(document, before_retry);
        let restored = SessionDocument::from_parts(
            document.metadata.clone(),
            document.runtime_lock.clone(),
            document.compactions.clone(),
            document.segment_catalog().to_vec(),
            document.events().to_vec(),
        )
        .unwrap();
        assert_eq!(
            restored.metadata.business_hook.binding.as_ref(),
            Some(&binding)
        );
        assert_eq!(
            restored.metadata.business_hook.observed_inputs.len(),
            usize::from(!text.is_empty())
        );
        if !text.is_empty() {
            let observed = &restored.metadata.business_hook.observed_inputs[0];
            assert_eq!(Some(observed.input_id), receipt.input_id);
            assert_eq!(observed.content, text);
            assert_eq!(
                observed.kind,
                crate::v1::business_hook::BusinessHookInputKind::Interrupt
            );
        }
    }
}

/// One composite publication preserves the order of several human admissions sharing a revision.
/// 一次组合发布保留共享修订的多条人类准入顺序。
/// Validates the real reserved-revision mechanism and strict reload rather than inventing larger revisions.
/// 验证真实预留修订机制和严格重载，不臆造更大的修订。
#[test]
fn business_hook_composite_admission_preserves_shared_revision_order() {
    // Every nested operation uses the outer transaction's one publication revision.
    // 每个嵌套操作使用外层事务的唯一发布修订。
    let (mut document, binding) = fixture();
    let revision = document.memory_revision() + 1;
    document
        .apply_atomic_transition(|staged| {
            staged.enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("queue-composite")?,
                    "Queued",
                    vec![],
                ),
                4,
            )?;
            staged.add_pending_in_turn_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("direct-composite")?,
                    "Direct",
                    vec![],
                ),
                4,
            )?;
            Ok(())
        })
        .unwrap();
    assert_eq!(document.memory_revision(), revision);
    assert_eq!(
        document
            .metadata
            .business_hook
            .observed_inputs
            .iter()
            .map(|entry| entry.observed_revision)
            .collect::<Vec<_>>(),
        vec![revision, revision]
    );
    document.capture_business_hook_binding(binding, 5).unwrap();
    let restored = SessionDocument::from_parts(
        document.metadata.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    assert_eq!(
        restored
            .metadata
            .business_hook
            .observed_inputs
            .iter()
            .map(|entry| entry.client_action_id.as_str())
            .collect::<Vec<_>>(),
        vec!["queue-composite", "direct-composite"]
    );
}

/// Promotion observes an older queue identity at its current business revision rather than sorting by input ID.
/// 提升在当前业务修订观察旧队列身份，而不是按输入 ID 排序。
/// Uses a real predecessor and successor turn, then verifies strict durable recovery and replay.
/// 使用真实前序与后继回合，再验证严格可靠恢复及重放。
#[test]
fn business_hook_observes_old_queue_at_new_turn_promotion() {
    use crate::v1::business_hook::BusinessHookInputKind;
    // The queued identity is allocated before the successor business turn exists.
    // 在后继业务回合存在之前分配排队身份。
    let (mut document, mut binding) = fixture();
    let queued = document
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("old-queue").unwrap(),
                "@inject 2 old\nOld queue",
                vec![],
            ),
            4,
        )
        .unwrap()
        .into_value();
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("next-root").unwrap(),
                "New work",
                vec![],
            ),
            6,
        )
        .unwrap();
    let turn = document.active_turn().unwrap().clone();
    assert!(queued.input_id.value() < turn.turn_id.value());
    document
        .add_pending_in_turn_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("current-direct").unwrap(),
                "Current direction",
                vec![],
            ),
            7,
        )
        .unwrap();
    // First observation of the old queue follows the newly admitted direct input despite its smaller input ID.
    // 尽管输入 ID 更小，旧队列的首次观察仍晚于新准入的直接输入。
    let identity = crate::v1::config::QueuedInputIdentity {
        client_action_id: queued.client_action_id,
        input_id: queued.input_id,
    };
    let before_revision = document.memory_revision();
    document
        .promote_queued_input_to_pending_turn(identity.clone(), 8)
        .unwrap();
    assert_eq!(document.memory_revision(), before_revision + 1);
    let before_retry = document.clone();
    document
        .promote_queued_input_to_pending_turn(identity, 9)
        .unwrap();
    assert_eq!(document, before_retry);
    binding.turn_id = turn.turn_id;
    binding.start_seq = turn.start_seq;
    binding.hook_turn_id = "next-hook".into();
    binding.user_content = "New work".into();
    document.capture_business_hook_binding(binding, 10).unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    let restored = SessionDocument::from_parts(
        document.metadata.clone(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    let journal = restored
        .business_hook_recovery_state()
        .unwrap()
        .observed_inputs;
    assert_eq!(journal.len(), 2);
    assert_eq!(journal[0].client_action_id.as_str(), "current-direct");
    assert_eq!(journal[1].client_action_id.as_str(), "old-queue");
    assert!(
        journal
            .iter()
            .all(|entry| entry.kind == BusinessHookInputKind::Interrupt)
    );
    assert!(journal[0].input_id > journal[1].input_id);
    assert!(journal[0].observed_revision < journal[1].observed_revision);
}

/// Mixed queue and direct input admissions publish one ordered, immutable journal that survives cold load.
/// 混合队列与直接输入准入发布一份有序不可变日志，并在冷加载后保留。
/// Uses real input allocation and binding capture; no extra revision may be introduced by exact retry.
/// 使用真实输入分配和绑定捕获；精确重试不能增加修订。
#[test]
fn business_hook_mixed_admission_is_atomic_and_restorable() {
    use crate::v1::business_hook::BusinessHookInputKind;
    // Interleave real admission kinds before the initial Hook is captured.
    // 在初始 Hook 捕获之前交错真实准入种类。
    let (mut document, binding) = fixture();
    for (action, kind) in [
        ("queue-first", BusinessHookInputKind::Queued),
        ("direct", BusinessHookInputKind::Interrupt),
        ("queue-last", BusinessHookInputKind::Queued),
    ] {
        let submission = ClientInputSubmission::new(
            ClientActionId::try_new(action).unwrap(),
            format!("@inject 2 rule\n{action}"),
            vec![],
        );
        let revision = document.memory_revision();
        match kind {
            BusinessHookInputKind::Queued => {
                document.enqueue_input(submission.clone(), 4).unwrap();
            }
            BusinessHookInputKind::Interrupt => {
                document
                    .add_pending_in_turn_input(submission.clone(), 4)
                    .unwrap();
            }
        }
        assert_eq!(document.memory_revision(), revision + 1);
        let original = document.clone();
        match kind {
            BusinessHookInputKind::Queued => {
                document.enqueue_input(submission, 5).unwrap();
            }
            BusinessHookInputKind::Interrupt => {
                document.add_pending_in_turn_input(submission, 5).unwrap();
            }
        }
        assert_eq!(document, original);
    }
    document.capture_business_hook_binding(binding, 6).unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    let restored = SessionDocument::from_parts(
        serde_json::from_value(serde_json::to_value(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    let journal = restored
        .business_hook_recovery_state()
        .unwrap()
        .observed_inputs;
    assert_eq!(
        journal
            .iter()
            .map(|item| (item.client_action_id.as_str(), item.kind))
            .collect::<Vec<_>>(),
        vec![
            ("queue-first", BusinessHookInputKind::Queued),
            ("direct", BusinessHookInputKind::Interrupt),
            ("queue-last", BusinessHookInputKind::Queued)
        ]
    );
    assert!(
        journal
            .windows(2)
            .all(|pair| pair[0].input_id < pair[1].input_id)
    );
    assert_eq!(journal[1].content, "@inject 2 rule\ndirect");
    // The new unreleased contract requires both its journal and every explicit kind.
    // 未发布的新契约要求日志及每条明确种类完整存在。
    let mut malformed = serde_json::to_value(&restored.metadata.business_hook).unwrap();
    malformed["observed_inputs"][1]
        .as_object_mut()
        .unwrap()
        .remove("kind");
    assert!(serde_json::from_value::<BusinessHookMetadata>(malformed).is_err());
}

/// Creates an admitted running business root and its matching immutable Hook seed.
/// 创建已准入的运行业务根及其匹配的不可变 Hook 种子。
/// Returns the unpublished document and binding for persistence and conflict checks.
/// 返回尚未发布的文档与绑定，用于持久化及冲突检查。
fn fixture() -> (SessionDocument, BusinessHookBinding) {
    // Allocate ownership through the real root transition rather than invented coordinates.
    // 通过真实根转换分配归属，不臆造坐标。
    let mut document = SessionDocument::new("main", "project", "Main", 1);
    document
        .submit_root_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("root").unwrap(),
                "original work",
                vec![],
            ),
            2,
        )
        .unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("running").unwrap(),
            state: CollaborationRunControl::Running,
            changed_at_ms: 3,
        })
        .unwrap();
    // The original host-sanitized content may differ from the canonical raw user input.
    // 宿主清理后的原正文可以不同于规范原始用户输入。
    let turn = document.active_turn().unwrap();
    let binding = BusinessHookBinding {
        session_id: "main".into(),
        segment_generation: document.segment_generation(),
        turn_id: turn.turn_id,
        start_seq: turn.start_seq,
        hook_turn_id: "original-hook".into(),
        user_content: "sanitized original work".into(),
        vmm_runtime_ready: false,
        observes_user: true,
    };
    (document, binding)
}

/// Queue admission persists ordered observations atomically, including before the first Hook binding exists.
/// 排队准入原子持久化有序观察，包括首个 Hook 绑定尚不存在时。
#[test]
fn business_hook_queue_observation_survives_retry_removal_and_reload() {
    // Queue facts belong to the actual open turn even if preparation has not reached Hook capture.
    // 即使准备尚未到达 Hook 捕获，队列事实仍属于实际开放回合。
    let (mut document, binding) = fixture();
    let queued = ClientInputSubmission::new(
        ClientActionId::try_new("queued-1").unwrap(),
        "  later work  ",
        vec![],
    );
    let revision = document.memory_revision();
    let receipt = document
        .enqueue_input(queued.clone(), 4)
        .unwrap()
        .into_value();
    assert_eq!(document.memory_revision(), revision + 1);
    assert!(document.metadata.business_hook.binding.is_none());
    assert_eq!(document.metadata.business_hook.observed_inputs.len(), 1);
    assert_eq!(
        document.metadata.business_hook.observed_inputs[0].turn_id,
        binding.turn_id
    );
    assert_eq!(
        document.metadata.business_hook.observed_inputs[0].input_id,
        receipt.input_id
    );
    assert_eq!(
        document.metadata.business_hook.observed_inputs[0].content,
        "  later work  "
    );
    // Exact browser retries and later Hook capture cannot append or discard the original observation.
    // 浏览器精确重试及稍后的 Hook 捕获不能追加或丢弃原观察。
    let before = document.clone();
    document.enqueue_input(queued, 5).unwrap();
    assert_eq!(document, before);
    document.capture_business_hook_binding(binding, 6).unwrap();
    assert_eq!(document.metadata.business_hook.observed_inputs.len(), 1);
    document
        .remove_queued_input(
            crate::v1::config::QueuedInputIdentity {
                client_action_id: receipt.client_action_id,
                input_id: receipt.input_id,
            },
            7,
        )
        .unwrap();
    assert!(document.metadata.input_state.queued_inputs.is_empty());
    // Queue removal resolves future execution, without erasing the already observed historical input.
    // 删除队列项解决未来执行，不抹去已观察历史输入。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    let reloaded = SessionDocument::from_parts(
        serde_json::from_value(serde_json::to_value(&document.metadata).unwrap()).unwrap(),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        document.events().to_vec(),
    )
    .unwrap();
    let recovered = reloaded.business_hook_recovery_state().unwrap();
    assert_eq!(
        recovered.observed_inputs,
        document.metadata.business_hook.observed_inputs
    );
}

/// Idle queueing has no business observer and must not invent a Hook owner or inherit a completed turn.
/// 空闲排队没有业务观察者，不能伪造 Hook 所有者或继承已完成回合。
#[test]
fn business_hook_queue_observation_does_not_claim_idle_or_closed_turn() {
    let (mut document, binding) = fixture();
    document.capture_business_hook_binding(binding, 4).unwrap();
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    document
        .enqueue_input(
            ClientInputSubmission::new(
                ClientActionId::try_new("idle-queue").unwrap(),
                "future task",
                vec![],
            ),
            6,
        )
        .unwrap();
    assert!(document.metadata.business_hook.observed_inputs.is_empty());
}

/// Proves exact replay, conflict atomicity and durable recovery without model-visible input changes.
/// 证明精确重放、冲突原子性和可靠恢复，且模型可见输入不变。
#[test]
fn business_hook_capture_is_immutable_and_requires_durable_recovery() {
    // Real history must remain identical when only Hook metadata is captured.
    // 仅捕获 Hook 元数据时真实历史必须完全一致。
    let (mut document, binding) = fixture();
    let events = document.events().to_vec();
    assert!(
        document
            .capture_business_hook_binding(binding.clone(), 4)
            .unwrap()
    );
    assert_eq!(document.events(), events);
    assert!(document.business_hook_recovery_binding().is_err());
    // Conflicting identities must not change revisions or partially replace the captured seed.
    // 冲突身份不得改变修订或部分替换已捕获种子。
    let before = document.clone();
    for changed in [
        BusinessHookBinding {
            observes_user: false,
            ..binding.clone()
        },
        BusinessHookBinding {
            hook_turn_id: "replacement".into(),
            ..binding.clone()
        },
        BusinessHookBinding {
            user_content: "injected guidance".into(),
            ..binding.clone()
        },
        BusinessHookBinding {
            session_id: "other".into(),
            ..binding.clone()
        },
        BusinessHookBinding {
            segment_generation: 2,
            ..binding.clone()
        },
    ] {
        assert!(document.capture_business_hook_binding(changed, 5).is_err());
        assert_eq!(document, before);
    }
    assert!(
        !document
            .capture_business_hook_binding(binding.clone(), 5)
            .unwrap()
    );
    assert_eq!(document, before);
    // Publish the exact revision before allowing a cold owner to use the binding.
    // 允许冷所有者使用绑定之前发布精确修订。
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert_eq!(document.business_hook_recovery_binding().unwrap(), &binding);
    // Missing original history cannot be repaired by substituting a later projected user message.
    // 缺失原历史时不能替换为稍后的投影用户消息来修复。
    document
        .events
        .retain(|event| event.seq != binding.start_seq);
    assert!(document.business_hook_recovery_binding().is_err());
}

/// Prevents a captured seed from authorizing stopped or physically executing business work.
/// 防止已捕获种子授权已停止或正在物理执行的业务。
#[test]
fn business_hook_recovery_rejects_stopped_or_active_api() {
    let (mut document, binding) = fixture();
    document.capture_business_hook_binding(binding, 4).unwrap();
    document
        .start_api(
            ApiPurposeKind::MainGeneration,
            crate::v1::event::ApiRequestSnapshot::new("provider", "model"),
            0,
        )
        .unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(document.business_hook_recovery_binding().is_err());
    document.interrupt_active_api_for_process(5).unwrap();
    document
        .set_collaboration_run_control(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new("stop").unwrap(),
            state: CollaborationRunControl::MainOnly,
            changed_at_ms: 6,
        })
        .unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    assert!(document.business_hook_recovery_binding().is_err());
}

/// New event-tree generations clear executable Hook ownership while preserving source evidence.
/// 新事件树代次清除可执行 Hook 归属，同时保留来源证据。
#[test]
fn business_hook_does_not_cross_fork_generation() {
    let (mut document, binding) = fixture();
    document.capture_business_hook_binding(binding, 4).unwrap();
    document.close_turn(TurnOutcome::Completed, 1, 5).unwrap();
    let revision = document
        .prepare_flush_candidates()
        .unwrap()
        .unwrap()
        .revision;
    document.advance_durable(revision).unwrap();
    let fork = document
        .build_fork_generation(super::super::generation::ForkGenerationInput {
            child_session_id: "fork".into(),
            title: "Fork".into(),
            source_turn_end_seq: document.events().last().unwrap().seq,
            now_ms: 6,
        })
        .unwrap();
    assert!(fork.metadata.business_hook.binding.is_none());
    assert!(document.metadata.business_hook.binding.is_some());
    // Unreleased storage requires the new container explicitly, without a legacy default.
    // 未发布存储明确要求新容器，不提供历史默认值。
    let mut encoded = serde_json::to_value(&document.metadata).unwrap();
    encoded.as_object_mut().unwrap().remove("business_hook");
    assert!(serde_json::from_value::<crate::v1::config::MetadataDocument>(encoded).is_err());
}
