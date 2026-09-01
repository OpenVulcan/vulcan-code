//! Partial prompt media completion, durable identity and chronological boundary regressions.
//! 提示词媒体部分完成、可靠身份和时序边界回归。

use super::*;
use crate::v1::{
    UserPromptResourceBinding, UserPromptResourcePreparation, UserPromptResourceSource,
};

/// Builds a completed `resource` result for exact prompt `seq` and occurrence `position` without changing its original route.
/// 为精确提示词`seq`及出现`position`构建已完成`resource`结果，不改变其原路线。
fn completion(
    seq: Seq,
    position: usize,
    resource: UserPromptResourceDelivery,
) -> UserPromptResourceBinding {
    UserPromptResourceBinding {
        prompt_seq: seq,
        resource_position: position,
        resource,
    }
}

/// Persists partial results independently of API admission and preserves distinct equal-file positions through reload and final freeze.
/// 独立于API准入保存部分结果，并跨重载及最终冻结保留相同文件的独立位置。
#[test]
fn user_prompt_partial_completion_retains_original_occurrences_and_retry_identity()
-> Result<(), DomainError> {
    // Equal resources intentionally exercise occurrence identity instead of global file identity.
    // 相同资源有意验证出现身份而非全局文件身份。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(1)?];
    // This exact original submission remains the only retry authority.
    // 本精确原提交仍是唯一重试权威。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("partial-root")?,
        "partial",
        resources.clone(),
    );
    // A owns the completed upload even if B later owns the first physical request.
    // 即使B稍后持有首次物理请求，A仍持有已完成上传。
    let original = origin("A", &resources[..1], true).resources.remove(0);
    // The owner is isolated and uses the real event and serialization paths.
    // 所有者独立隔离，并使用真实事件及序列化路径。
    let mut document = new_document();
    document.submit_root_input(submission.clone(), T0)?;
    // Preserve both chronological identity and public semantic fingerprint.
    // 同时保留时序身份及公开语义指纹。
    let seq = prompt_seq(&document, "partial");
    // Media provenance is host-only, not an extra model-visible prompt.
    // 媒体来源为仅宿主信息，不是额外模型可见提示词。
    let fingerprint = provider_visible_fingerprint(document.events(), seq)?;
    // Exact admission proof must not be replaced by a later runtime result.
    // 精确准入证明不能被后续运行时结果替换。
    let admission = document
        .client_input_for_action(&submission.client_action_id)?
        .expect("accepted")
        .parameters;
    assert!(document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        completion(seq, 0, original.clone()),
        T0 + 1
    )?);
    // Reload through actual serde and document validation, not a shallow clone.
    // 通过真实serde和文档校验重载，而非浅克隆。
    let mut restored = reload(&document)?;
    assert_eq!(restored.preparing_user_prompt_media_seqs(), vec![seq]);
    assert_eq!(restored.events().len(), 1);
    assert_eq!(
        provider_visible_fingerprint(restored.events(), seq)?,
        fingerprint
    );
    assert_eq!(
        restored.user_prompt_resource_preparation(
            &UserPromptResourceSource {
                client_action_id: submission.client_action_id.clone(),
                resource_position: 0
            },
            resources[0]
        )?,
        UserPromptResourcePreparation::Prepared(original.clone())
    );
    assert_eq!(
        restored.user_prompt_resource_preparation(
            &UserPromptResourceSource {
                client_action_id: submission.client_action_id.clone(),
                resource_position: 1
            },
            resources[1]
        )?,
        UserPromptResourcePreparation::Pending
    );
    // Repeating the original submission neither duplicates the prompt nor rewrites its original proof.
    // 重复原提交既不复制提示词，也不改写其原证明。
    assert!(
        !restored
            .submit_root_input(submission.clone(), T0 + 2)?
            .changed()
    );
    // Identical completion is idempotent down to revision and event timestamp.
    // 相同完成幂等，精确到修订号及事件时间戳。
    let revision = restored.memory_revision();
    assert!(!restored.record_user_prompt_resource_delivery(
        restored.segment_generation(),
        completion(seq, 0, original.clone()),
        T0 + 3
    )?);
    assert_eq!(restored.memory_revision(), revision);
    assert_eq!(restored.events(), document.events());
    // Final request B keeps A's earlier upload and only owns the still-unprepared second occurrence.
    // 最终请求B保留A更早上传，仅持有尚未准备的第二次出现。
    let mut final_origin = origin("B", &resources, false);
    final_origin.resources[0] = original;
    restored.freeze_user_prompt_media(
        vec![UserPromptMediaBinding {
            prompt_seq: seq,
            origin: final_origin.clone(),
        }],
        T0 + 4,
    )?;
    // The unchanged admission proof coexists with the actual first request route and per-resource origins.
    // 未变的准入证明与真实首次请求路线及逐资源原身份共存。
    let accepted = reload(&restored)?
        .client_input_for_action(&submission.client_action_id)?
        .expect("frozen input")
        .parameters
        .expect("media parameters");
    assert_eq!(
        accepted.prepared_resources,
        admission.expect("admission").prepared_resources
    );
    assert_eq!(
        accepted.media_delivery,
        UserPromptMediaDelivery::Frozen {
            origin: final_origin
        }
    );
    Ok(())
}

/// Rejects replacement of completed or admitted proof, invalid occurrence identities and wrong generations without partial mutation.
/// 拒绝替换已完成或准入证明、无效出现身份及错误代际，不发生部分修改。
#[test]
fn user_prompt_partial_completion_rejects_replacement_atomically() -> Result<(), DomainError> {
    // First occurrence already has an immutable admitted upload; the second is new intent.
    // 第一次出现已具有不可变准入上传；第二次为新意图。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(2)?];
    // Runtime writes must keep this original admission proof intact.
    // 运行时写入必须保持本原准入证明不变。
    let parameters = prepared_parameters(&resources);
    // Each invalid case starts from the same exact document state.
    // 每个无效用例从同一精确文档状态开始。
    let mut document = new_document();
    document.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("partial-errors")?,
            "partial-errors",
            resources.clone(),
        )
        .with_parameters(parameters.clone())?,
        T0,
    )?;
    // The valid second occurrence is then immutable as well.
    // 有效第二次出现此后同样不可变。
    let seq = prompt_seq(&document, "partial-errors");
    document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        completion(
            seq,
            1,
            origin("B", &resources[1..], false).resources.remove(0),
        ),
        T0 + 1,
    )?;
    // Snapshot all mutation-bearing facts for failure atomicity.
    // 快照全部携带修改的事实以验证失败原子性。
    let before = document.events().to_vec();
    // A failed attempt must not consume a document revision.
    // 失败尝试不能消耗文档修订。
    let revision = document.memory_revision();
    // Wrong model, wrong id, out-of-range position and wrong generation are independent invalid claims.
    // 错误模型、错误ID、越界位置及错误代际是独立无效声明。
    let claims = [
        (
            document.segment_generation(),
            completion(
                seq,
                0,
                origin("C", &resources[..1], true).resources.remove(0),
            ),
        ),
        (
            document.segment_generation(),
            completion(
                seq,
                1,
                origin("C", &resources[1..], false).resources.remove(0),
            ),
        ),
        (
            document.segment_generation(),
            completion(
                seq,
                1,
                origin("B", &resources[..1], false).resources.remove(0),
            ),
        ),
        (
            document.segment_generation(),
            completion(
                seq,
                2,
                origin("B", &resources[1..], false).resources.remove(0),
            ),
        ),
        (
            document.segment_generation() + 1,
            completion(
                seq,
                1,
                origin("B", &resources[1..], false).resources.remove(0),
            ),
        ),
    ];
    for (generation, binding) in claims {
        assert!(
            document
                .record_user_prompt_resource_delivery(generation, binding, T0 + 2)
                .is_err()
        );
        assert_eq!(document.events(), before);
        assert_eq!(document.memory_revision(), revision);
    }
    assert!(
        document
            .freeze_user_prompt_media(
                vec![UserPromptMediaBinding {
                    prompt_seq: seq,
                    origin: origin("C", &resources, false)
                }],
                T0 + 3
            )
            .is_err()
    );
    assert_eq!(document.events(), before);
    // Reload retains both admitted and runtime proof at their original positions.
    // 重载在原位置同时保留准入及运行时证明。
    let restored = reload(&document)?;
    // Inspect the exact user event rather than a queue receipt or latest-resource cache.
    // 检查精确用户事件，而非队列回执或最近资源缓存。
    let EventBody::UserInput(input) = &restored.events()[0].event else {
        unreachable!("root")
    };
    assert_eq!(
        input
            .parameters
            .as_ref()
            .expect("parameters")
            .prepared_resources,
        parameters.prepared_resources
    );
    Ok(())
}

/// Keeps legacy unknown and already dispatched unfinished positions ineligible for late backfilling.
/// 保持旧未知及已分派的未完成位置不具备迟到回填资格。
#[test]
fn user_prompt_partial_completion_cannot_backfill_legacy_or_dispatched_history()
-> Result<(), DomainError> {
    for legacy in [false, true] {
        // One new resource occurrence becomes either old-format history or a passed first-API boundary.
        // 一次新资源出现将变为旧格式历史或已过首次API边界。
        let resource = ResourceId::try_new(1)?;
        // Each rejected history scenario owns an isolated document.
        // 每个被拒历史场景持有隔离文档。
        let mut document = new_document();
        document.submit_root_input_fixture("boundary", vec![resource], T0)?;
        // Capture the exact original event before introducing the historical boundary.
        // 在引入历史边界之前捕获精确原事件。
        let seq = prompt_seq(&document, "boundary");
        if legacy {
            // Missing parameters intentionally represent the old persisted format.
            // 缺失参数有意代表旧持久化格式。
            let EventBody::UserInput(input) = &mut document.events[0].event else {
                unreachable!("root")
            };
            input.parameters = None;
        } else {
            document.start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                T0 + 1,
            )?;
        }
        // A late result must not manufacture historical provenance.
        // 迟到结果不能制造历史来源证明。
        let before = document.events().to_vec();
        assert!(
            document
                .record_user_prompt_resource_delivery(
                    document.segment_generation(),
                    completion(seq, 0, origin("A", &[resource], true).resources.remove(0)),
                    T0 + 2
                )
                .is_err()
        );
        assert_eq!(document.events(), before);
    }
    Ok(())
}

/// Validates partial-state shape on reload and rejects native results for explicitly unproven source positions.
/// 重载校验部分状态结构，并拒绝显式未经证明来源位置的原生结果。
#[test]
fn user_prompt_partial_completion_validates_shape_and_unproven_positions() -> Result<(), DomainError>
{
    // Unknown inherited history is distinct from a fresh unprepared occurrence.
    // 继承的未知历史区别于新的未准备出现。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(2)?];
    // Only the first occurrence is explicitly inherited without original proof.
    // 仅第一次出现被显式声明为缺少原证明的继承来源。
    let mut parameters = UserPromptParameters::for_resource_preparations(
        &resources,
        vec![
            UserPromptResourcePreparation::Unproven,
            UserPromptResourcePreparation::Pending,
        ],
    )?
    .expect("media");
    // The real owner validates both mutation and hydration paths.
    // 真实所有者校验修改及补水两条路径。
    let mut document = new_document();
    document.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("unknown-partial")?,
            "unknown",
            resources.clone(),
        )
        .with_parameters(parameters.clone())?,
        T0,
    )?;
    // Bind attempted completion to this exact accepted prompt.
    // 将尝试完成绑定到本精确已接纳提示词。
    let seq = prompt_seq(&document, "unknown");
    assert!(
        document
            .record_user_prompt_resource_delivery(
                document.segment_generation(),
                completion(
                    seq,
                    0,
                    origin("A", &resources[..1], true).resources.remove(0)
                ),
                T0 + 1
            )
            .is_err()
    );
    // An explicit address result is allowed without claiming a native original route.
    // 显式地址结果可被允许，但不声称原生原路线。
    let mut attachment = origin("B", &resources[..1], false).resources.remove(0);
    attachment.delivery = UserPromptResourceDeliveryKind::Attachment;
    document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        completion(seq, 0, attachment.clone()),
        T0 + 2,
    )?;
    reload(&document)?;
    // Invalid length, empty completion, mismatched ids and native unknown claims remain strict errors.
    // 无效长度、空完成、ID不符及未知原生声明仍为严格错误。
    for partial in [
        vec![],
        vec![None, None],
        vec![Some(attachment.clone())],
        vec![
            Some(origin("A", &resources[..1], true).resources.remove(0)),
            None,
        ],
        vec![None, Some(attachment.clone())],
    ] {
        parameters.media_delivery = UserPromptMediaDelivery::Preparing { resources: partial };
        assert!(parameters.validate(&resources).is_err());
        // The same invalid state must also fail the actual serialized-event hydration path.
        // 相同无效状态也必须在真实序列化事件补水路径失败。
        let EventBody::UserInput(input) = &mut document.events[0].event else {
            unreachable!("root")
        };
        input.parameters = Some(parameters.clone());
        assert!(reload(&document).is_err());
    }
    parameters.media_delivery = UserPromptMediaDelivery::Preparing {
        resources: vec![Some(attachment), None],
    };
    assert!(parameters.validate(&resources).is_ok());
    assert!(
        parameters.validate_pending(&resources).is_err(),
        "clients cannot submit runtime preparation state"
    );
    // A partial snapshot cannot silently drop an immutable admitted occurrence either.
    // 部分快照同样不能静默丢弃不可变准入出现。
    let mut admitted = prepared_parameters(&resources);
    admitted.media_delivery = UserPromptMediaDelivery::Preparing {
        resources: vec![
            None,
            Some(origin("B", &resources[1..], false).resources.remove(0)),
        ],
    };
    assert!(admitted.validate(&resources).is_err());
    Ok(())
}

/// Prefers the real partial user event over its older queue receipt while retaining immutable submit identity.
/// 优先选择真实部分完成用户事件而非更早队列回执，同时保留不可变提交身份。
#[test]
fn user_prompt_partial_completion_lookup_survives_queue_promotion() -> Result<(), DomainError> {
    // The input is admitted during a real active API and becomes a chronological supplementary prompt afterwards.
    // 输入在真实活动API期间准入，随后成为时序补充提示词。
    let resource = ResourceId::try_new(1)?;
    // Only this session owns the queued input and its subsequent user event.
    // 仅本会话持有排队输入及其后续用户事件。
    let mut document = new_document();
    document.submit_root_input_fixture("root", vec![], T0)?;
    // The preceding API establishes the actual queue-to-supplementary transition boundary.
    // 前一API建立真实队列到补充输入的转换边界。
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    // Preserve the same client action across the receipt and promoted prompt.
    // 在回执及提升提示词间保留相同客户端动作。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("partial-queued")?,
        "queued-partial",
        vec![resource],
    );
    // Promotion accepts only the exact identity returned by this enqueue call.
    // 提升仅接受本次排队调用返回的精确身份。
    let queued = document
        .enqueue_input(submission.clone(), T0 + 2)?
        .into_value();
    document.promote_queued_input_to_pending_turn(
        crate::v1::QueuedInputIdentity {
            client_action_id: queued.client_action_id,
            input_id: queued.input_id,
        },
        T0 + 3,
    )?;
    complete_ok(&mut document, api, None, T0 + 4)?;
    // The later user event owns the actual completed result.
    // 较晚用户事件持有实际已完成结果。
    let original = origin("A", &[resource], true).resources.remove(0);
    document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        completion(prompt_seq(&document, "queued-partial"), 0, original.clone()),
        T0 + 5,
    )?;
    // Exact source lookup must not regress to the older Pending promotion receipt after hydration.
    // 精确来源查询不能在补水后退回更早Pending提升回执。
    let restored = reload(&document)?;
    assert_eq!(
        restored.user_prompt_resource_preparation(
            &UserPromptResourceSource {
                client_action_id: submission.client_action_id,
                resource_position: 0
            },
            resource
        )?,
        UserPromptResourcePreparation::Prepared(original)
    );
    Ok(())
}

/// Retains completed original identity after cancellation while unfinished positions become unknown history, not new intent.
/// 取消后保留已完成原身份，未完成位置变为未知历史而非新意图。
#[test]
fn user_prompt_partial_completion_closed_turn_keeps_only_known_originals() -> Result<(), DomainError>
{
    // Cancellation may arrive after the first occurrence finishes and before the second is prepared.
    // 取消可能在第一次出现完成后、第二次准备前到达。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(2)?];
    // This immutable action locates both occurrences after turn closure.
    // 本不可变动作在回合闭合后定位两次出现。
    let action = ClientActionId::try_new("cancel-partial")?;
    // Cancellation modifies the real document rather than a test-only preparation flag.
    // 取消修改真实文档，而非仅测试准备标志。
    let mut document = new_document();
    document.submit_root_input(
        ClientInputSubmission::new(action.clone(), "cancel-partial", resources.clone()),
        T0,
    )?;
    // Only the first occurrence completes before the turn is stopped.
    // 仅第一次出现在回合停止前完成。
    let original = origin("A", &resources[..1], true).resources.remove(0);
    document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        completion(prompt_seq(&document, "cancel-partial"), 0, original.clone()),
        T0 + 1,
    )?;
    document.close_turn(TurnOutcome::Stopped, 0, T0 + 2)?;
    // Reload must not require a live open turn to recover an already completed result.
    // 恢复已经完成结果时，重载不能要求实时开放回合。
    let restored = reload(&document)?;
    assert!(restored.preparing_user_prompt_media_seqs().is_empty());
    assert_eq!(
        restored.user_prompt_resource_preparation(
            &UserPromptResourceSource {
                client_action_id: action.clone(),
                resource_position: 0
            },
            resources[0]
        )?,
        UserPromptResourcePreparation::Prepared(original)
    );
    assert_eq!(
        restored.user_prompt_resource_preparation(
            &UserPromptResourceSource {
                client_action_id: action,
                resource_position: 1
            },
            resources[1]
        )?,
        UserPromptResourcePreparation::Unproven
    );
    Ok(())
}
