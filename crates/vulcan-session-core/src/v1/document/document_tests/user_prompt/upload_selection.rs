//! Original upload selection precedes execution and may only gain its exact completion receipt.
//! 原上传选择先于执行，且只能补齐其精确完成回执。

use super::*;
use crate::v1::{
    UserPromptResourceBinding, UserPromptResourcePreparation, UserPromptResourceSource,
};

/// Keeps an imported draft link immutable and rejects malformed linkage instead of upgrading it through a new V1 upload producer.
/// 保持导入草稿关联不可变，并拒绝错误关联，不经新V1上传生产者升级它。
#[test]
fn user_prompt_draft_upload_link_is_validated_and_not_rewritten() -> Result<(), DomainError> {
    // Exact fixture facts provide the source, channel and route for both variants.
    // 精确夹具事实为两种状态提供来源、通道及路线。
    let resource = ResourceId::try_new(1)?;
    let completed = origin("A", &[resource], true).resources.remove(0);
    let mut pending = selected_upload(&completed);
    let UserPromptResourceDeliveryKind::UploadPending {
        draft_intent_id, ..
    } = &mut pending.delivery
    else {
        unreachable!()
    };
    *draft_intent_id = Some(uuid::Uuid::new_v4().to_string().into_boxed_str());
    assert!(!completed.completes_upload(&pending));
    let parameters = UserPromptParameters::for_resource_preparations(
        &[resource],
        vec![UserPromptResourcePreparation::Prepared(pending.clone())],
    )?
    .expect("resource parameters");
    assert_eq!(
        serde_json::from_str::<UserPromptParameters>(
            &serde_json::to_string(&parameters).expect("serialize parameters")
        )
        .expect("deserialize parameters"),
        parameters
    );
    let UserPromptResourceDeliveryKind::UploadPending {
        draft_intent_id, ..
    } = &mut pending.delivery
    else {
        unreachable!()
    };
    *draft_intent_id = Some("bad-intent".into());
    assert!(
        UserPromptParameters::for_resource_preparations(
            &[resource],
            vec![UserPromptResourcePreparation::Prepared(pending)]
        )
        .is_err()
    );
    Ok(())
}

/// Removes only the completed receipt from the fixture's `completed` origin, returning its exact pre-upload selection.
/// 仅移除夹具`completed`原身份中的已完成回执，返回其精确上传前选择。
fn selected_upload(completed: &UserPromptResourceDelivery) -> UserPromptResourceDelivery {
    // The fixture already owns a validated completed upload; retain its exact route and channel.
    // 夹具已经持有已验证完成上传；保留其精确路线及通道。
    let mut selected = completed.clone();
    // The original reference is deliberately absent rather than an empty or guessed fingerprint.
    // 原引用有意缺失，而非空值或猜测指纹。
    let UserPromptResourceDeliveryKind::Uploaded {
        upload_channel_id,
        upload_contract_fingerprint,
        ..
    } = &completed.delivery
    else {
        panic!("completed upload fixture")
    };
    selected.delivery = UserPromptResourceDeliveryKind::UploadPending {
        draft_intent_id: None,
        upload_channel_id: upload_channel_id.clone(),
        upload_contract_fingerprint: upload_contract_fingerprint.clone(),
    };
    selected
}

/// Keeps selected upload identity durable and permits only receipt completion on its unchanged original route and channel.
/// 保持所选上传身份可靠，且仅允许原路线及通道不变的回执完成。
#[test]
fn user_prompt_upload_selection_only_allows_exact_forward_completion() -> Result<(), DomainError> {
    // Resource identity belongs to this prompt occurrence, independent of all later cache entries.
    // 资源身份属于本提示词出现，独立于全部后续缓存条目。
    let resource = ResourceId::try_new(1)?;
    // One fixed successful result defines the expected exact selected contract.
    // 一个固定成功结果定义预期精确所选契约。
    let completed = origin("A", &[resource], true).resources.remove(0);
    // No reference has completed when this original choice enters the prompt.
    // 本原选择进入提示词时，尚无完成引用。
    let selected = selected_upload(&completed);
    // The real owner validates serialization, mutation and API boundaries.
    // 真实所有者校验序列化、修改及API边界。
    let mut document = new_document();
    document.submit_root_input_fixture("selected", vec![resource], T0)?;
    // Keep the exact original event sequence throughout both states.
    // 在两个状态中保持精确原事件序号。
    let seq = prompt_seq(&document, "selected");
    document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        UserPromptResourceBinding {
            prompt_seq: seq,
            resource_position: 0,
            resource: selected.clone(),
        },
        T0 + 1,
    )?;
    document = reload(&document)?;
    // Independent invalid mutations may not consume the selected choice or partially update the document.
    // 独立无效修改不能消耗所选选择或部分更新文档。
    let before = document.events().to_vec();
    // Every changed dimension is an explicit rejected claim, never a fallback candidate.
    // 每个变化维度均为明确拒绝声明，绝非回退候选。
    let mut wrong_model = completed.clone();
    wrong_model.original_route.model_id = "B".into();
    // Changing only the channel still attempts to replace the original upload operation.
    // 仅改变通道仍是在尝试替换原上传操作。
    let mut wrong_channel = completed.clone();
    if let UserPromptResourceDeliveryKind::Uploaded {
        upload_channel_id, ..
    } = &mut wrong_channel.delivery
    {
        *upload_channel_id = "other-channel".into();
    }
    // The same channel name cannot hide a different frozen upload contract.
    // 相同通道名不能隐藏不同冻结上传契约。
    let mut wrong_contract = completed.clone();
    if let UserPromptResourceDeliveryKind::Uploaded {
        upload_contract_fingerprint,
        ..
    } = &mut wrong_contract.delivery
    {
        *upload_contract_fingerprint = format!("sha256:{}", "c".repeat(64));
    }
    // Metadata projection must not overwrite selected upload identity.
    // 元数据投影不能覆盖所选上传身份。
    let mut downgraded = completed.clone();
    downgraded.delivery = UserPromptResourceDeliveryKind::Attachment;
    // Completing a selected upload must validate the opaque receipt format before mutating the live prompt.
    // 补齐所选上传时，必须在修改实时提示词之前校验不透明回执格式。
    let mut malformed_receipt = completed.clone();
    if let UserPromptResourceDeliveryKind::Uploaded {
        reference_fingerprint,
        ..
    } = &mut malformed_receipt.delivery
    {
        *reference_fingerprint = "not-a-sha256-receipt".into();
    }
    for rejected in [
        wrong_model,
        wrong_channel,
        wrong_contract,
        downgraded,
        malformed_receipt,
        origin("A", &[resource], false).resources.remove(0),
    ] {
        assert!(
            document
                .record_user_prompt_resource_delivery(
                    document.segment_generation(),
                    UserPromptResourceBinding {
                        prompt_seq: seq,
                        resource_position: 0,
                        resource: rejected
                    },
                    T0 + 2
                )
                .is_err()
        );
        assert_eq!(document.events(), before);
    }
    assert!(document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        UserPromptResourceBinding {
            prompt_seq: seq,
            resource_position: 0,
            resource: completed.clone()
        },
        T0 + 3
    )?);
    assert!(!document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        UserPromptResourceBinding {
            prompt_seq: seq,
            resource_position: 0,
            resource: completed.clone()
        },
        T0 + 4
    )?);
    assert!(
        document
            .record_user_prompt_resource_delivery(
                document.segment_generation(),
                UserPromptResourceBinding {
                    prompt_seq: seq,
                    resource_position: 0,
                    resource: selected
                },
                T0 + 5
            )
            .is_err()
    );
    // The completed first reference remains immutable; a later valid-looking reference cannot replace it.
    // 已完成首次引用保持不可变；后续看似有效引用不能替换它。
    let mut replaced = completed.clone();
    if let UserPromptResourceDeliveryKind::Uploaded {
        reference_fingerprint,
        ..
    } = &mut replaced.delivery
    {
        *reference_fingerprint = "d".repeat(64);
    }
    assert!(
        document
            .record_user_prompt_resource_delivery(
                document.segment_generation(),
                UserPromptResourceBinding {
                    prompt_seq: seq,
                    resource_position: 0,
                    resource: replaced
                },
                T0 + 6
            )
            .is_err()
    );
    // B may own the first API while the resource continues to belong to A.
    // B可以持有首次API，而资源继续属于A。
    let mut final_origin = origin("B", &[resource], false);
    final_origin.resources[0] = completed;
    document.freeze_user_prompt_media(
        vec![UserPromptMediaBinding {
            prompt_seq: seq,
            origin: final_origin,
        }],
        T0 + 7,
    )?;
    reload(&document)?;
    Ok(())
}

/// Retains selected-but-uncompleted upload identity after cancellation without exposing a receipt or granting fresh source intent.
/// 取消后保留已选但未完成的上传身份，不暴露回执，也不授予新来源意图。
#[test]
fn user_prompt_upload_selection_survives_cancel_as_known_unavailable_origin()
-> Result<(), DomainError> {
    // The exact action is used for historical source lookup after real turn closure.
    // 精确动作用于真实回合闭合后的历史来源查询。
    let action = ClientActionId::try_new("selected-cancel")?;
    // A selected upload has a valid contract but no completed reference.
    // 已选上传具有有效契约，但没有完成引用。
    let resource = ResourceId::try_new(1)?;
    // This pending representation is not interchangeable with new unprepared input.
    // 本待回执表示不能与新的未准备输入互换。
    let selected = selected_upload(&origin("A", &[resource], true).resources.remove(0));
    // Only one original chronological prompt owns this choice.
    // 仅一个原时序提示词持有本选择。
    let mut document = new_document();
    document.submit_root_input(
        ClientInputSubmission::new(action.clone(), "selected-cancel", vec![resource]),
        T0,
    )?;
    document.record_user_prompt_resource_delivery(
        document.segment_generation(),
        UserPromptResourceBinding {
            prompt_seq: prompt_seq(&document, "selected-cancel"),
            resource_position: 0,
            resource: selected.clone(),
        },
        T0 + 1,
    )?;
    document.close_turn(TurnOutcome::Stopped, 0, T0 + 2)?;
    // Physical-shaped hydration preserves selection even with no open preparation window.
    // 即使没有开放准备窗口，物理形态补水也保留选择。
    let restored = reload(&document)?;
    assert_eq!(
        restored.user_prompt_resource_preparation(
            &UserPromptResourceSource {
                client_action_id: action,
                resource_position: 0
            },
            resource
        )?,
        UserPromptResourcePreparation::Prepared(selected)
    );
    // Host metadata must not leak into provider-visible semantic messages.
    // 宿主元数据不能泄漏到供应商可见语义消息。
    let visible = serde_json::to_string(&project_provider_items(restored.events(), None, &[])?)
        .expect("visible messages");
    assert!(!visible.contains("upload_pending"));
    assert!(!visible.contains("upload_contract_fingerprint"));
    Ok(())
}

/// Rejects late completion after first-API freeze and refuses to overwrite immutable admission proof imported from another occurrence.
/// 拒绝首次API冻结后的迟到完成，并拒绝覆盖从另一出现导入的不可变准入证明。
#[test]
fn user_prompt_upload_selection_cannot_backfill_frozen_or_imported_history()
-> Result<(), DomainError> {
    // The same valid completed result is used to exercise both independent authority boundaries.
    // 使用相同有效完成结果验证两个独立权限边界。
    let resource = ResourceId::try_new(1)?;
    // The original route and contract stay equal so only lifecycle authority can decide the outcome.
    // 原路线及契约保持相等，使结果仅由生命周期权限决定。
    let completed = origin("A", &[resource], true).resources.remove(0);
    // Imported proof must not be confused with the current producer's own in-flight choice.
    // 导入证明不能与当前生产者自身在途选择混淆。
    let selected = selected_upload(&completed);
    for imported in [false, true] {
        // Each branch begins with an independent authoritative document.
        // 每个分支从独立权威文档开始。
        let mut document = new_document();
        // Initial input remains immutable throughout later media preparation.
        // 初始输入在后续媒体准备期间保持不可变。
        let mut submission = ClientInputSubmission::new(
            ClientActionId::try_new("selected-boundary")?,
            "selected-boundary",
            vec![resource],
        );
        if imported {
            submission = submission.with_parameters(
                UserPromptParameters::for_resource_preparations(
                    &[resource],
                    vec![UserPromptResourcePreparation::Prepared(selected.clone())],
                )?
                .expect("media"),
            )?;
        }
        document.submit_root_input(submission, T0)?;
        // The exact sequence remains the completion destination, never a latest-message guess.
        // 精确序号始终为完成目标，绝非最近消息猜测。
        let seq = prompt_seq(&document, "selected-boundary");
        if !imported {
            document.record_user_prompt_resource_delivery(
                document.segment_generation(),
                UserPromptResourceBinding {
                    prompt_seq: seq,
                    resource_position: 0,
                    resource: selected.clone(),
                },
                T0 + 1,
            )?;
            document.freeze_user_prompt_media(
                vec![UserPromptMediaBinding {
                    prompt_seq: seq,
                    origin: UserPromptMediaOrigin {
                        route: selected.original_route.clone(),
                        resources: vec![selected.clone()],
                    },
                }],
                T0 + 2,
            )?;
        }
        // Neither a frozen occurrence nor imported initial proof can be completed by this new producer.
        // 冻结出现及导入初始证明均不能由本新生产者补齐。
        let before = document.events().to_vec();
        assert!(
            document
                .record_user_prompt_resource_delivery(
                    document.segment_generation(),
                    UserPromptResourceBinding {
                        prompt_seq: seq,
                        resource_position: 0,
                        resource: completed.clone()
                    },
                    T0 + 3
                )
                .is_err()
        );
        assert_eq!(document.events(), before);
        reload(&document)?;
    }
    Ok(())
}
