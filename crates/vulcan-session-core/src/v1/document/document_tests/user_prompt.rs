//! User prompt parameter ownership, chronology, replay and failure-atomicity regressions.
//! 用户提示词参数归属、时序、重放及失败原子性回归。

use super::*;
use crate::v1::projection::{
    project_provider_items, project_provider_items_with_sources, provider_visible_fingerprint,
};
use crate::v1::{
    UserPromptMediaBinding, UserPromptMediaDelivery, UserPromptMediaOrigin, UserPromptMediaRoute,
    UserPromptParameters, UserPromptResourceDelivery, UserPromptResourceDeliveryKind,
};

mod partial_completion;
mod upload_selection;

/// Reads exact accepted inputs through queue promotion and freezing without borrowing mutable draft text or another action's proof.
/// 通过队列提升和冻结读取精确已接纳输入，不借用可变草稿正文或其他动作的证明。
#[test]
fn user_prompt_exact_client_lookup_follows_immutable_lifecycle() -> Result<(), DomainError> {
    // The source occurrence remains fixed across its declared lifecycle states.
    // 来源出现通过其已声明生命周期状态时保持固定。
    let resources = vec![ResourceId::try_new(1)?];
    // All accepted state belongs to one document, not a file-global latest-use cache.
    // 全部已接纳状态属于一个文档，而非文件全局最近使用缓存。
    let mut document = new_document();
    document.submit_root_input_fixture("root", vec![], T0)?;
    // The real active API allows queue-to-supplementary-input promotion.
    // 真实活动API允许队列提升为补充输入。
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    // Original preparation is already model A when the exact client action enters the queue.
    // 精确客户端动作进入队列时，原准备已经属于模型A。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("exact-source")?,
        "original queued text",
        resources.clone(),
    )
    .with_parameters(prepared_parameters(&resources))?;
    // The receipt carries the only valid identity for subsequent promotion.
    // 回执携带后续提升唯一有效的身份。
    let queued = document
        .enqueue_input(submission.clone(), T0 + 2)?
        .into_value();
    assert_eq!(
        document.client_input_for_action(&submission.client_action_id)?,
        Some(submission.clone())
    );
    document
        .metadata
        .input_state
        .queued_inputs
        .push(document.metadata.input_state.queued_inputs[0].clone());
    assert!(
        document
            .client_input_for_action(&submission.client_action_id)
            .is_err()
    );
    document.metadata.input_state.queued_inputs.pop();
    document.promote_queued_input_to_pending_turn(
        crate::v1::config::QueuedInputIdentity {
            client_action_id: queued.client_action_id,
            input_id: queued.input_id,
        },
        T0 + 3,
    )?;
    assert_eq!(
        document.client_input_for_action(&submission.client_action_id)?,
        Some(submission.clone())
    );
    complete_ok(&mut document, api, None, T0 + 4)?;
    // The new prompt's first request may be B, but the accepted draft proof must remain A.
    // 新提示词首次请求可以为B，但已接纳草稿证明必须仍为A。
    let mut final_origin = origin("model-B", &resources, false);
    final_origin.resources[0] = submission
        .parameters
        .as_ref()
        .expect("initial parameters")
        .prepared_resources[0]
        .clone()
        .expect("original proof");
    document.freeze_user_prompt_media(
        vec![UserPromptMediaBinding {
            prompt_seq: prompt_seq(&document, "original queued text"),
            origin: final_origin,
        }],
        T0 + 5,
    )?;
    // Lookup retains actual frozen request facts together with immutable original admission proof.
    // 查询同时保留真实冻结请求事实及不可变原准入证明。
    let accepted = reload(&document)?
        .client_input_for_action(&submission.client_action_id)?
        .expect("accepted source");
    assert_eq!(accepted.text, submission.text);
    assert!(matches!(
        accepted
            .parameters
            .as_ref()
            .expect("parameters")
            .media_delivery,
        UserPromptMediaDelivery::Frozen { .. }
    ));
    assert_eq!(
        accepted
            .parameters
            .as_ref()
            .expect("parameters")
            .prepared_resources,
        submission
            .parameters
            .as_ref()
            .expect("parameters")
            .prepared_resources
    );
    // A different queued input moved to the composer keeps its original accepted text in its terminal receipt.
    // 另一排队输入移入输入区后，仍在其终结回执保留原已接纳正文。
    let draft_input = ClientInputSubmission::new(
        ClientActionId::try_new("draft-source")?,
        "before editing",
        resources.clone(),
    )
    .with_parameters(prepared_parameters(&resources))?;
    // Only this exact queue identity may be moved to the draft.
    // 只有本精确队列身份可移入草稿。
    let queued = document
        .enqueue_input(draft_input.clone(), T0 + 6)?
        .into_value();
    document.move_queued_input_to_draft(
        crate::v1::config::QueuedInputIdentity {
            client_action_id: queued.client_action_id,
            input_id: queued.input_id,
        },
        T0 + 7,
    )?;
    document.set_draft("edited mutable draft", resources, T0 + 8)?;
    assert_eq!(
        reload(&document)?.client_input_for_action(&draft_input.client_action_id)?,
        Some(draft_input)
    );
    assert!(
        document
            .client_input_for_action(&ClientActionId::try_new("missing-action")?)?
            .is_none()
    );
    Ok(())
}

/// Keeps duplicate source positions distinct and propagates unknown history without authorizing fresh native delivery.
/// 保持重复来源位置独立，并传播未知历史而不授权新的原生投递。
#[test]
fn user_prompt_source_occurrences_preserve_proof_and_unknown_state() -> Result<(), DomainError> {
    use crate::v1::{UserPromptResourcePreparation, UserPromptResourceSource};
    // Same content appears twice, with different original models at the two positions.
    // 相同内容出现两次，两个位置具有不同原模型。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(1)?];
    // First occurrence was uploaded by A and second was inlined by B.
    // 第一次出现由A上传，第二次出现由B内联。
    let first = origin("model-A", &resources[..1], true).resources.remove(0);
    // Both claims remain scoped to their own positions despite equal resource ids.
    // 即使资源ID相等，两个证明仍限定于各自位置。
    let second = origin("model-B", &resources[..1], false)
        .resources
        .remove(0);
    // The constructor preserves exact preparation order and validates each proof's ResourceId.
    // 构造器保留精确准备顺序，并验证各证明的ResourceId。
    let parameters = UserPromptParameters::for_resource_preparations(
        &resources,
        vec![
            UserPromptResourcePreparation::Prepared(first.clone()),
            UserPromptResourcePreparation::Prepared(second.clone()),
        ],
    )?
    .expect("media");
    // One immutable accepted input owns both original occurrences.
    // 一个不可变已接纳输入持有两个原出现。
    let mut document = new_document();
    document.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("source-input")?,
            "source",
            resources.clone(),
        )
        .with_parameters(parameters)?,
        T0,
    )?;
    for (position, expected) in [first, second].into_iter().enumerate() {
        assert_eq!(
            document.user_prompt_resource_preparation(
                &UserPromptResourceSource {
                    client_action_id: ClientActionId::try_new("source-input")?,
                    resource_position: position
                },
                resources[position]
            )?,
            UserPromptResourcePreparation::Prepared(expected)
        );
    }
    assert!(
        document
            .user_prompt_resource_preparation(
                &UserPromptResourceSource {
                    client_action_id: ClientActionId::try_new("source-input")?,
                    resource_position: 2
                },
                resources[0]
            )
            .is_err()
    );
    assert!(
        document
            .user_prompt_resource_preparation(
                &UserPromptResourceSource {
                    client_action_id: ClientActionId::try_new("source-input")?,
                    resource_position: 0
                },
                ResourceId::try_new(2)?
            )
            .is_err()
    );
    assert_eq!(
        document.user_prompt_resource_preparation(
            &UserPromptResourceSource {
                client_action_id: ClientActionId::try_new("not-in-this-session")?,
                resource_position: 0
            },
            resources[0]
        )?,
        UserPromptResourcePreparation::Unproven
    );
    // A copied unknown occurrence remains explicitly unknown alongside a genuinely new occurrence.
    // 复制的未知出现与真正新出现并列时，仍保持显式未知。
    let unknown = UserPromptParameters::for_resource_preparations(
        &resources,
        vec![
            UserPromptResourcePreparation::Unproven,
            UserPromptResourcePreparation::Pending,
        ],
    )?
    .expect("media");
    assert_eq!(unknown.unproven_resource_positions, vec![0]);
    // Unknown provenance may freeze only an actual address projection, never a fabricated native original.
    // 未知来源只能冻结实际地址投影，绝不能伪造原生原身份。
    let mut projected = origin("projection-model", &resources, false);
    assert!(unknown.validate_preserved_preparation(&projected).is_err());
    projected.resources[0].delivery = UserPromptResourceDeliveryKind::Attachment;
    unknown.validate_preserved_preparation(&projected)?;
    // Explicit unknown positions are part of immutable admission identity.
    // 显式未知位置属于不可变准入身份。
    let mut cleared = unknown.clone();
    cleared.unproven_resource_positions.clear();
    assert!(!crate::v1::user_prompt::initial_preparation_matches(
        Some(&unknown),
        Some(&cleared)
    ));
    // The same source's stale pending flag cannot be treated as fresh after an API boundary.
    // 同一来源的陈旧待准备标记不能在API边界之后视为新意图。
    let mut stale = new_document();
    stale.submit_root_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("stale-source")?,
            "stale",
            resources.clone(),
        ),
        T0,
    )?;
    // Before first preparation this is a real unprepared source intention.
    // 首次准备之前，这是真实未准备来源意图。
    let source = UserPromptResourceSource {
        client_action_id: ClientActionId::try_new("stale-source")?,
        resource_position: 0,
    };
    assert_eq!(
        stale.user_prompt_resource_preparation(&source, resources[0])?,
        UserPromptResourcePreparation::Pending
    );
    stale.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    assert_eq!(
        stale.user_prompt_resource_preparation(&source, resources[0])?,
        UserPromptResourcePreparation::Unproven
    );
    Ok(())
}

/// Preserves source proof when a restored project draft reorders, removes or mixes equal-content occurrences with a new staged asset.
/// 恢复项目草稿重排、移除或将相同内容出现与新暂存资产混合时，保留来源证明。
#[test]
fn user_prompt_project_draft_reorder_keeps_exact_source_positions() -> Result<(), DomainError> {
    use crate::v1::{UserPromptResourcePreparation, UserPromptResourceSource};
    // Equal ResourceIds deliberately cannot serve as original occurrence identities.
    // 相等ResourceId有意不能充当原出现身份。
    let resource = ResourceId::try_new(1)?;
    // Each original position was prepared under a different model.
    // 每个原位置均在不同模型下准备。
    let parameters = UserPromptParameters::for_resource_preparations(
        &[resource, resource],
        vec![
            UserPromptResourcePreparation::Prepared(
                origin("A", &[resource], true).resources.remove(0),
            ),
            UserPromptResourcePreparation::Prepared(
                origin("B", &[resource], false).resources.remove(0),
            ),
        ],
    )?
    .expect("parameters");
    // Create a real queued input and move it to the durable composer.
    // 创建真实排队输入并移动到持久输入区。
    let mut document = new_document();
    document.submit_root_input_fixture("active", vec![], T0)?;
    // This client action remains the stable anchor after draft text and order change.
    // 草稿正文及顺序变化后，本客户端动作仍为稳定锚点。
    let action = ClientActionId::try_new("project-source")?;
    // Queue receipt selects the exact input transition.
    // 队列回执选择精确输入转换。
    let queued = document
        .enqueue_input(
            ClientInputSubmission::new(action.clone(), "queued", vec![resource, resource])
                .with_parameters(parameters)?,
            T0 + 1,
        )?
        .into_value();
    document.move_queued_input_to_draft(
        crate::v1::config::QueuedInputIdentity {
            client_action_id: queued.client_action_id,
            input_id: queued.input_id,
        },
        T0 + 2,
    )?;
    assert_eq!(
        document.metadata.input_state.draft.resource_sources,
        vec![
            Some(UserPromptResourceSource {
                client_action_id: action.clone(),
                resource_position: 0
            }),
            Some(UserPromptResourceSource {
                client_action_id: action.clone(),
                resource_position: 1
            })
        ]
    );
    // Reorder by explicit source anchors and insert a truly new staged occurrence between them.
    // 按显式来源锚点重排，并在其间插入真正新暂存出现。
    let sources = vec![
        Some(UserPromptResourceSource {
            client_action_id: action.clone(),
            resource_position: 1,
        }),
        None,
        Some(UserPromptResourceSource {
            client_action_id: action.clone(),
            resource_position: 0,
        }),
    ];
    document.set_draft_with_resource_sources(
        "edited",
        vec![resource; 3],
        vec![None, Some("new-asset".into()), None],
        sources.clone(),
        T0 + 3,
    )?;
    // Reload proves actual persisted source order and proof, not just a borrowed draft object.
    // 重载证明真实持久来源顺序及证明，而非仅借用草稿对象。
    let restored = reload(&document)?;
    assert_eq!(
        restored.metadata.input_state.draft.resource_sources,
        sources
    );
    // The new staged entry has no fabricated prior model, while the two known entries remain B then A.
    // 新暂存条目没有伪造先前模型，两个已知条目则保持B随后A。
    let proof = restored
        .metadata
        .input_state
        .draft
        .parameters
        .expect("mixed proof");
    assert_eq!(
        proof.prepared_resources[0]
            .as_ref()
            .expect("B")
            .original_route
            .model_id,
        "B"
    );
    assert!(proof.prepared_resources[1].is_none());
    assert_eq!(
        proof.prepared_resources[2]
            .as_ref()
            .expect("A")
            .original_route
            .model_id,
        "A"
    );
    document.set_draft_with_resource_sources(
        "only A remains",
        vec![resource],
        vec![],
        vec![Some(UserPromptResourceSource {
            client_action_id: action.clone(),
            resource_position: 0,
        })],
        T0 + 4,
    )?;
    assert_eq!(
        document
            .metadata
            .input_state
            .draft
            .parameters
            .as_ref()
            .expect("proof")
            .prepared_resources[0]
            .as_ref()
            .expect("A")
            .original_route
            .model_id,
        "A"
    );
    // Contradictory sources fail before replacing the previous durable draft.
    // 矛盾来源在替换先前可靠草稿之前失败。
    let before = document.metadata.clone();
    assert!(
        document
            .set_draft_with_resource_sources(
                "bad",
                vec![resource],
                vec![],
                vec![Some(UserPromptResourceSource {
                    client_action_id: action.clone(),
                    resource_position: 3
                })],
                T0 + 5
            )
            .is_err()
    );
    assert_eq!(document.metadata, before);
    assert!(
        document
            .set_draft_with_resource_sources(
                "bad",
                vec![resource],
                vec![Some("asset".into())],
                vec![Some(UserPromptResourceSource {
                    client_action_id: action,
                    resource_position: 0
                })],
                T0 + 6
            )
            .is_err()
    );
    assert_eq!(document.metadata, before);
    Ok(())
}

/// Builds a non-secret original route for exact `model` and ordered `resources`, using `uploaded` to select receipt identity.
/// 为精确`model`及有序`resources`构建非秘密原路线，通过`uploaded`选择回执身份。
fn origin(model: &str, resources: &[ResourceId], uploaded: bool) -> UserPromptMediaOrigin {
    // Ordinary fixtures prepare each occurrence on the same first-request route.
    // 普通夹具在相同首次请求路线上准备每次出现。
    let route = UserPromptMediaRoute {
        provider_id: String::from("openai"),
        service_entry_id: String::from("api"),
        connection_id: String::from("connection-1"),
        model_id: model.to_string(),
        protocol_profile_id: String::from("openai-responses.v1"),
    };
    UserPromptMediaOrigin {
        route: route.clone(),
        resources: resources
            .iter()
            .map(|resource_id| UserPromptResourceDelivery {
                resource_id: *resource_id,
                original_route: route.clone(),
                delivery: if uploaded {
                    UserPromptResourceDeliveryKind::Uploaded {
                        upload_channel_id: String::from("files"),
                        upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
                        reference_fingerprint: "b".repeat(64),
                    }
                } else {
                    UserPromptResourceDeliveryKind::Inline
                },
            })
            .collect(),
    }
}

/// Returns the exact prompt event named `text` in `document`; test fixture names are unique.
/// 返回`document`中正文为`text`的精确提示词事件；测试夹具名称唯一。
fn prompt_seq(document: &SessionDocument, text: &str) -> Seq {
    document
        .events()
        .iter()
        .find(|event| matches!(&event.event, EventBody::UserInput(input) if input.text == text))
        .expect("exact fixture prompt")
        .seq()
}

/// Reloads exact serialized event bytes and the existing metadata/runtime owners, returning validated derived state.
/// 重载精确序列化事件字节及既有元数据和运行时所有者，返回已校验派生状态。
fn reload(document: &SessionDocument) -> Result<SessionDocument, DomainError> {
    // JSON serialization proves new parameters survive actual serialization, not just a structure clone.
    // JSON序列化证明新参数通过真实序列化保留，而非仅结构克隆。
    let bytes = serde_json::to_vec(document.events()).expect("serialize prompt events");
    let events = serde_json::from_slice(&bytes).expect("deserialize exact prompt events");
    SessionDocument::from_parts(
        serde_json::from_slice(
            &serde_json::to_vec(&document.metadata).expect("serialize input metadata"),
        )
        .expect("deserialize input metadata"),
        document.runtime_lock.clone(),
        document.compactions.clone(),
        document.segment_catalog().to_vec(),
        events,
    )
}

/// Creates two newly promoted supplementary prompts before their first API, retaining their actual event order.
/// 在首次API前创建两条新提升的补充提示词，并保留实际事件顺序。
fn supplementary_fixture() -> Result<(SessionDocument, ResourceId, Seq, Seq), DomainError> {
    let mut document = new_document();
    let resource = ResourceId::try_new(1)?;
    document.submit_root_input_fixture("initial", vec![], T0)?;
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    document.add_pending_in_turn_input_fixture("first", vec![resource], T0 + 2)?;
    document.add_pending_in_turn_input_fixture("second", vec![resource], T0 + 3)?;
    complete_ok(&mut document, api, None, T0 + 4)?;
    let first = prompt_seq(&document, "first");
    let second = prompt_seq(&document, "second");
    Ok((document, resource, first, second))
}

/// Builds pending input parameters with one already prepared model-A upload and optional later unprepared resources.
/// 构建待准备输入参数，其中含一个已准备模型A上传及可选后续未准备资源。
fn prepared_parameters(resources: &[ResourceId]) -> UserPromptParameters {
    // The first resource's original receipt belongs to its earlier draft intention.
    // 第一个资源的原回执属于其更早草稿意图。
    let first = origin("model-A", &resources[..1], true).resources.remove(0);
    // Preserve exact occurrence positions, including still-unprepared later attachments.
    // 保留精确出现位置，包括仍未准备的后续附件。
    let mut prepared_resources = vec![None; resources.len()];
    prepared_resources[0] = Some(first);
    UserPromptParameters {
        unproven_resource_positions: Vec::new(),
        version: 1,
        prepared_resources,
        media_delivery: UserPromptMediaDelivery::Pending,
    }
}

/// Preserves exact staged occurrence identities across text edits and serialization without merging equal ResourceIds.
/// 跨正文编辑及序列化保留精确暂存出现身份，不合并相同ResourceId。
#[test]
fn user_prompt_draft_lineage_is_ordered_durable_and_validated() -> Result<(), DomainError> {
    // Two attachments can share content while representing separate explicit intentions.
    // 两个附件可共享内容，同时表示独立显式意图。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(1)?];
    // The draft owner keeps staged lineage independently of resource deduplication.
    // 草稿所有者独立于资源去重保存暂存来源。
    let mut document = new_document();
    // Stable host-issued asset ids retain exact positions through reload.
    // 稳定宿主签发资产ID通过重载保留精确位置。
    let lineage = vec![Some("asset-first".into()), Some("asset-second".into())];
    document.set_draft_with_staged_attachments("draft", resources.clone(), lineage.clone(), T0)?;
    document.set_draft("edited text", resources.clone(), T0 + 1)?;
    assert_eq!(
        reload(&document)?
            .metadata
            .input_state
            .draft
            .staged_attachment_ids,
        lineage
    );
    // Replacing an attachment with equal bytes is still a real draft revision because the asset identity differs.
    // 使用相同字节替换附件仍产生真实草稿修订，因为资产身份不同。
    let replacement = vec![Some("asset-third".into()), Some("asset-second".into())];
    assert!(
        document
            .set_draft_with_staged_attachments(
                "edited text",
                resources.clone(),
                replacement.clone(),
                T0 + 2
            )?
            .changed()
    );
    assert_eq!(
        reload(&document)?
            .metadata
            .input_state
            .draft
            .staged_attachment_ids,
        replacement
    );
    // Invalid later mutations cannot partially replace the existing draft.
    // 无效后续修改不能部分替换既有草稿。
    let before = document.metadata.clone();
    for invalid in [
        vec![Some("one".into())],
        vec![Some("attachment://forbidden".into()), None],
    ] {
        assert!(
            document
                .set_draft_with_staged_attachments("invalid", resources.clone(), invalid, T0 + 3)
                .is_err()
        );
        assert_eq!(document.metadata, before);
    }
    // Legacy absence stays absent and may never infer an asset from its content hash or ResourceId.
    // 旧记录缺失保持缺失，绝不能从内容摘要或ResourceId推断资产。
    document.set_draft_with_staged_attachments(
        "project only",
        resources,
        vec![None, None],
        T0 + 4,
    )?;
    assert!(
        reload(&document)?
            .metadata
            .input_state
            .draft
            .staged_attachment_ids
            .is_empty()
    );
    Ok(())
}

/// Validates pending prompt proof before API completion mutates either stream history or pending input metadata.
/// 在API完成修改入流历史或待处理输入元数据之前，校验待处理提示词证明。
#[test]
fn user_prompt_invalid_pending_proof_rejects_api_completion_atomically() -> Result<(), DomainError>
{
    // Prepare a real open API and a valid admitted supplementary input.
    // 准备真实开放API和有效已准入补充输入。
    let mut document = new_document();
    document.submit_root_input_fixture("root", vec![], T0)?;
    // Completion uses the existing exact request fixture route.
    // 完成使用既有精确请求夹具路线。
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    document.add_pending_in_turn_input(
        ClientInputSubmission::new(
            ClientActionId::try_new("invalid-later-proof")?,
            "pending",
            vec![ResourceId::try_new(1)?],
        )
        .with_parameters(prepared_parameters(&[ResourceId::try_new(1)?]))?,
        T0 + 2,
    )?;
    document.metadata.input_state.pending_in_turn_inputs[0]
        .parameters
        .as_mut()
        .expect("proof")
        .version = 99;
    // Capture every persistent owner after corruption and before attempted promotion.
    // 在损坏之后、尝试提升之前捕获每个持久所有者。
    let before = serde_json::to_value((
        &document.metadata,
        &document.runtime_lock,
        document.events(),
    ))
    .expect("before state");
    assert!(complete_ok(&mut document, api, None, T0 + 3).is_err());
    assert_eq!(
        serde_json::to_value((
            &document.metadata,
            &document.runtime_lock,
            document.events()
        ))
        .expect("after state"),
        before
    );
    Ok(())
}

/// Preserves draft origins when the first request uses B, rejects original replacement and keeps exact retries valid after freezing.
/// 首次请求使用B时保留草稿原身份，拒绝原身份替换，并在冻结后保持精确重试有效。
#[test]
fn user_prompt_prepared_origins_survive_admission_freezing_and_retry() -> Result<(), DomainError> {
    // One prompt combines an earlier upload and a newly attached resource.
    // 一条提示词组合更早上传及新附加资源。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(2)?];
    // Only this document owns the input and its subsequent immutable preparation.
    // 只有本文件持有输入及其后续不可变准备。
    let mut document = new_document();
    // Admission receives the same explicit host proof that a real draft handoff supplies.
    // 准入接收与真实草稿交接相同的明确宿主证明。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("prepared-root")?,
        "prepared-root",
        resources.clone(),
    )
    .with_parameters(prepared_parameters(&resources))?;
    document.submit_root_input(submission.clone(), T0)?;
    // Current B may choose a representation for the new resource, but cannot rewrite the first resource's A origin.
    // 当前B可为新资源选择表示，但不能改写第一个资源的A原身份。
    let mut binding = UserPromptMediaBinding {
        prompt_seq: prompt_seq(&document, "prepared-root"),
        origin: origin("model-B", &resources, false),
    };
    assert!(
        document
            .freeze_user_prompt_media(vec![binding.clone()], T0 + 1)
            .is_err()
    );
    binding.origin.resources[0] = submission
        .parameters
        .as_ref()
        .expect("host proof")
        .prepared_resources[0]
        .clone()
        .expect("first origin");
    document.freeze_user_prompt_media(vec![binding], T0 + 2)?;
    // Reload uses actual parameter serialization and then retries the same original client action.
    // 重载使用真实参数序列化，然后重试同一原客户端动作。
    let mut restored = reload(&document)?;
    assert!(
        !restored
            .submit_root_input(submission.clone(), T0 + 3)?
            .changed()
    );
    // A conflicting proof under the same idempotency key must fail even if text and resource ids match.
    // 即使正文及资源ID相同，同一幂等键下冲突证明仍必须失败。
    let mut conflict = submission;
    conflict
        .parameters
        .as_mut()
        .expect("host proof")
        .prepared_resources[0]
        .as_mut()
        .expect("first origin")
        .original_route
        .model_id = "model-B".into();
    assert!(restored.submit_root_input(conflict, T0 + 4).is_err());
    Ok(())
}

/// Retains admitted preparation through queue promotion, API completion, durable reload and restoration to a draft.
/// 通过队列提升、API完成、可靠重载及恢复到草稿保留准入准备。
#[test]
fn user_prompt_prepared_origins_follow_queue_and_supplementary_input() -> Result<(), DomainError> {
    // Resource identity remains fixed while its input changes lifecycle state.
    // 输入生命周期状态变化时资源身份保持固定。
    let resources = vec![ResourceId::try_new(1)?];
    // Open the real core API boundary before enqueueing the prepared user input.
    // 在排入已准备用户输入之前打开真实核心API边界。
    let mut document = new_document();
    document.submit_root_input_fixture("root", vec![], T0)?;
    // The active API gives the supplementary input an explicit future stream position.
    // 活动API使补充输入具有明确未来入流位置。
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 1,
    )?;
    // The exact prepared proof travels with the queued input instead of the current active model.
    // 精确准备证明随排队输入传递，而非取自当前活动模型。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("prepared-queued")?,
        "prepared-queued",
        resources.clone(),
    )
    .with_parameters(prepared_parameters(&resources))?;
    // Admission returns the stable identity used for all later queue transitions.
    // 准入返回全部后续队列转换使用的稳定身份。
    let queued = document
        .enqueue_input(submission.clone(), T0 + 2)?
        .into_value();
    document.promote_queued_input_to_pending_turn(
        crate::v1::config::QueuedInputIdentity {
            client_action_id: queued.client_action_id,
            input_id: queued.input_id,
        },
        T0 + 3,
    )?;
    assert_eq!(
        document.metadata.input_state.pending_in_turn_inputs[0].parameters,
        submission.parameters
    );
    complete_ok(&mut document, api, None, T0 + 4)?;
    // Provider projection receives the retained proof at the original promoted event position.
    // 供应商投影在原提升事件位置接收保留证明。
    let projected = project_provider_items_with_sources(document.events(), None, &[])?;
    assert_eq!(
        projected
            .last()
            .expect("promoted prompt")
            .user_prompt_parameters,
        submission.parameters
    );
    reload(&document)?;
    // A second queued input may be restored to the composer without losing its preparation.
    // 第二个排队输入可以恢复到输入区而不丢失准备。
    let second = ClientInputSubmission::new(
        ClientActionId::try_new("prepared-draft")?,
        "prepared-draft",
        resources.clone(),
    )
    .with_parameters(prepared_parameters(&resources))?;
    // The queue receipt remains the sole transition identity.
    // 队列回执仍是唯一转换身份。
    let queued = document.enqueue_input(second.clone(), T0 + 5)?.into_value();
    document.move_queued_input_to_draft(
        crate::v1::config::QueuedInputIdentity {
            client_action_id: queued.client_action_id,
            input_id: queued.input_id,
        },
        T0 + 6,
    )?;
    assert_eq!(
        document.metadata.input_state.draft.parameters,
        second.parameters
    );
    assert!(!document.enqueue_input(second, T0 + 7)?.changed());
    reload(&document)?;
    // Editing only text does not discard the restored attachment's original upload proof.
    // 仅编辑正文不丢弃已恢复附件的原上传证明。
    let original_parameters = document.metadata.input_state.draft.parameters.clone();
    document.set_draft("edited draft text", resources, T0 + 8)?;
    assert_eq!(
        reload(&document)?.metadata.input_state.draft.parameters,
        original_parameters
    );
    Ok(())
}

/// Verifies original parameters belong to the prompt, freeze once, survive reload and never leak into wire semantics.
/// 验证原参数属于提示词、仅冻结一次、重载后保留，并且绝不泄漏到线路语义。
#[test]
fn user_prompt_media_freezes_once_survives_reload_and_keeps_public_projection_clean()
-> Result<(), DomainError> {
    let mut document = new_document();
    let resources = vec![ResourceId::try_new(2)?, ResourceId::try_new(1)?];
    document.submit_root_input_fixture("original", resources.clone(), T0)?;
    let seq = prompt_seq(&document, "original");
    let fingerprint = provider_visible_fingerprint(document.events(), seq)?;
    let binding = UserPromptMediaBinding {
        prompt_seq: seq,
        origin: origin("model-A", &resources, true),
    };
    assert!(document.freeze_user_prompt_media(vec![binding.clone()], T0 + 1)?);
    let revision = document.memory_revision();
    let original = document.events().to_vec();
    assert!(!document.freeze_user_prompt_media(vec![binding.clone()], T0 + 2)?);
    assert_eq!(document.memory_revision(), revision);
    assert_eq!(document.events(), original);
    assert_eq!(
        provider_visible_fingerprint(document.events(), seq)?,
        fingerprint
    );
    let mut restored = reload(&document)?;
    assert_eq!(restored.events(), document.events());
    let mut replacement = binding.clone();
    replacement.origin.route.model_id = String::from("model-B");
    assert!(
        restored
            .freeze_user_prompt_media(vec![replacement], T0 + 3)
            .is_err()
    );
    assert_eq!(restored.events(), original);
    let sourced = project_provider_items_with_sources(restored.events(), None, &[])?;
    assert_eq!(sourced[0].source_from_seq, seq);
    assert_eq!(
        sourced[0].user_prompt_parameters,
        Some(UserPromptParameters {
            unproven_resource_positions: Vec::new(),
            version: 1,
            prepared_resources: Vec::new(),
            media_delivery: UserPromptMediaDelivery::Frozen {
                origin: binding.origin
            },
        })
    );
    let wire = serde_json::to_string(&project_provider_items(restored.events(), None, &[])?)
        .expect("public semantic JSON");
    assert!(!wire.contains("parameters"));
    assert!(!wire.contains("model-A"));
    assert!(!wire.contains("reference_fingerprint"));
    Ok(())
}

/// Retains distinct original models inside one prompt without changing resource order or allowing later route replacement.
/// 在一条提示词内保留不同原模型，不改变资源顺序，也不允许后续替换路线。
#[test]
fn user_prompt_media_occurrences_keep_independent_original_routes() -> Result<(), DomainError> {
    // The same logical resource appears twice in one prompt, matching two explicit attachment intentions.
    // 同一逻辑资源在一条提示词出现两次，对应两个显式附件意图。
    let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(1)?];
    // Only this isolated document owns the chronological prompt event.
    // 只有本隔离文档持有时序提示词事件。
    let mut document = new_document();
    document.submit_root_input_fixture("mixed-origin", resources.clone(), T0)?;
    // The first physical request belongs to B while the first attachment was prepared under A.
    // 首次物理请求属于B，而第一个附件在A下准备。
    let mut binding = UserPromptMediaBinding {
        prompt_seq: prompt_seq(&document, "mixed-origin"),
        origin: origin("model-B", &resources, true),
    };
    binding.origin.resources[0].original_route.model_id = String::from("model-A");
    document.freeze_user_prompt_media(vec![binding.clone()], T0 + 1)?;
    // Disk-shaped reload preserves exact per-occurrence metadata, not a prompt-wide inferred origin.
    // 磁盘形态重载保留精确逐出现元数据，而非整条提示词推断原身份。
    let mut restored = reload(&document)?;
    assert_eq!(restored.events(), document.events());
    // Attempting to rewrite just one resource's original model must still fail atomically.
    // 仅尝试改写一个资源原模型仍必须原子失败。
    let before = restored.events().to_vec();
    binding.origin.resources[0].original_route.model_id = String::from("model-B");
    assert!(
        restored
            .freeze_user_prompt_media(vec![binding], T0 + 2)
            .is_err()
    );
    assert_eq!(restored.events(), before);
    Ok(())
}

/// Verifies a shared file in two prompts retains two independent origins in actual event order.
/// 验证两条提示词共享一个文件时，仍按实际事件顺序保留两个独立原身份。
#[test]
fn user_prompt_media_shared_file_keeps_per_prompt_origin_and_rejects_reverse_order()
-> Result<(), DomainError> {
    let (mut document, resource, first, second) = supplementary_fixture()?;
    let first_binding = UserPromptMediaBinding {
        prompt_seq: first,
        origin: origin("model-A", &[resource], true),
    };
    let second_binding = UserPromptMediaBinding {
        prompt_seq: second,
        origin: origin("model-B", &[resource], false),
    };
    let before = document.events().to_vec();
    let revision = document.memory_revision();
    assert!(
        document
            .freeze_user_prompt_media(vec![second_binding.clone(), first_binding.clone()], T0 + 5)
            .is_err()
    );
    assert_eq!(document.events(), before);
    assert_eq!(document.memory_revision(), revision);
    assert!(
        document.freeze_user_prompt_media(
            vec![first_binding.clone(), second_binding.clone()],
            T0 + 5
        )?
    );
    let projected = project_provider_items_with_sources(document.events(), None, &[])?;
    let prompts = projected
        .iter()
        .filter(|item| item.user_prompt_parameters.is_some())
        .collect::<Vec<_>>();
    assert_eq!(prompts.len(), 2);
    for (item, binding) in prompts.iter().zip([first_binding, second_binding]) {
        assert_eq!(item.source_from_seq, binding.prompt_seq);
        assert_eq!(
            item.user_prompt_parameters
                .as_ref()
                .expect("own prompt parameters")
                .media_delivery,
            UserPromptMediaDelivery::Frozen {
                origin: binding.origin
            }
        );
    }
    assert_eq!(reload(&document)?.events(), document.events());
    Ok(())
}

/// Verifies later preparation cannot backfill an earlier unprepared prompt after an API boundary has passed.
/// 验证API边界过去之后，后来的准备不能回填较早尚未冻结的提示词。
#[test]
fn user_prompt_media_never_backfills_unknown_or_already_dispatched_history()
-> Result<(), DomainError> {
    for legacy in [false, true] {
        let mut document = new_document();
        let resource = ResourceId::try_new(1)?;
        document.submit_root_input_fixture("earlier", vec![resource], T0)?;
        let seq = prompt_seq(&document, "earlier");
        if legacy {
            // Missing parameters are an explicit old-record fixture, not a new pending prompt.
            // 参数缺失是明确旧记录夹具，不是新待准备提示词。
            let EventBody::UserInput(input) = &mut document.events[0].event else {
                unreachable!("fixture root")
            };
            input.parameters = None;
            document = reload(&document)?;
        } else {
            let api = document.start_api(
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                T0 + 1,
            )?;
            complete_ok(&mut document, api, None, T0 + 2)?;
        }
        let before = document.events().to_vec();
        assert!(document.preparing_user_prompt_media_seqs().is_empty());
        assert!(
            document
                .freeze_user_prompt_media(
                    vec![UserPromptMediaBinding {
                        prompt_seq: seq,
                        origin: origin("model-B", &[resource], false),
                    }],
                    T0 + 3
                )
                .is_err()
        );
        assert_eq!(document.events(), before);
    }
    Ok(())
}

/// Checks preparation eligibility follows exact prompt order and disappears as soon as its first API starts.
/// 检查准备资格遵循精确提示词顺序，并在首次API开始后立即消失。
#[test]
fn user_prompt_media_preflight_uses_core_chronology() -> Result<(), DomainError> {
    // Supplementary inputs become new prompts together only after the preceding API finishes.
    // 补充输入仅在前一API结束后一起成为新提示词。
    let (mut document, _, first, second) = supplementary_fixture()?;
    assert_eq!(
        document.preparing_user_prompt_media_seqs(),
        vec![first, second]
    );
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 5,
    )?;
    assert!(document.preparing_user_prompt_media_seqs().is_empty());
    complete_ok(&mut document, api, None, T0 + 6)?;
    assert!(document.preparing_user_prompt_media_seqs().is_empty());
    Ok(())
}

/// Verifies a malformed later resource or exhausted later event revision cannot partially freeze earlier prompts.
/// 验证后项资源无效或后项事件修订耗尽时，不能部分冻结前项提示词。
#[test]
fn user_prompt_media_batch_failure_is_atomic() -> Result<(), DomainError> {
    for exhausted_revision in [false, true] {
        let (mut document, resource, first, second) = supplementary_fixture()?;
        let first_binding = UserPromptMediaBinding {
            prompt_seq: first,
            origin: origin("A", &[resource], true),
        };
        let mut second_binding = UserPromptMediaBinding {
            prompt_seq: second,
            origin: origin("B", &[resource], false),
        };
        if exhausted_revision {
            document
                .events
                .iter_mut()
                .find(|event| event.seq() == second)
                .expect("second prompt")
                .event_revision = JS_SAFE_INTEGER_MAX;
        } else {
            second_binding.origin.resources[0].resource_id = ResourceId::try_new(9)?;
        }
        let before = document.events().to_vec();
        let revision = document.memory_revision();
        assert!(
            document
                .freeze_user_prompt_media(vec![first_binding, second_binding], T0 + 5)
                .is_err()
        );
        assert_eq!(document.events(), before);
        assert_eq!(document.memory_revision(), revision);
    }
    Ok(())
}

/// Verifies unknown versions, reordered resources and private URLs are rejected during strict reload.
/// 验证严格重载拒绝未知版本、资源乱序及私有URL。
#[test]
fn user_prompt_media_reload_validates_parameters_and_reference_identity() -> Result<(), DomainError>
{
    for invalid in 0..3 {
        let mut document = new_document();
        let resources = vec![ResourceId::try_new(1)?, ResourceId::try_new(2)?];
        document.submit_root_input_fixture("input", resources.clone(), T0)?;
        let seq = prompt_seq(&document, "input");
        document.freeze_user_prompt_media(
            vec![UserPromptMediaBinding {
                prompt_seq: seq,
                origin: origin("A", &resources, true),
            }],
            T0 + 1,
        )?;
        let EventBody::UserInput(input) = &mut document.events[0].event else {
            unreachable!("fixture root")
        };
        let parameters = input.parameters.as_mut().expect("frozen parameters");
        if invalid == 0 {
            parameters.version = 2;
        }
        let UserPromptMediaDelivery::Frozen { origin } = &mut parameters.media_delivery else {
            unreachable!("frozen fixture")
        };
        if invalid == 1 {
            origin.resources.swap(0, 1);
        }
        if invalid == 2 {
            let UserPromptResourceDeliveryKind::Uploaded {
                reference_fingerprint,
                ..
            } = &mut origin.resources[0].delivery
            else {
                unreachable!("uploaded fixture")
            };
            *reference_fingerprint = String::from("https://private.example/secret-file");
        }
        assert!(reload(&document).is_err());
        assert!(project_provider_items_with_sources(document.events(), None, &[]).is_err());
    }
    Ok(())
}

/// Verifies explicit prompt editing starts a new intent instead of inheriting the old upload origin.
/// 验证显式提示词编辑开启新意图，而不是继承旧上传身份。
#[test]
fn user_prompt_media_edit_resets_only_replaced_prompt() -> Result<(), DomainError> {
    let mut document = new_document();
    let resource = ResourceId::try_new(1)?;
    document.submit_root_input_fixture("first", vec![resource], T0)?;
    let first = prompt_seq(&document, "first");
    document.freeze_user_prompt_media(
        vec![UserPromptMediaBinding {
            prompt_seq: first,
            origin: origin("A", &[resource], true),
        }],
        T0 + 1,
    )?;
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 2,
    )?;
    document.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 3)?;
    complete_ok(&mut document, api, None, T0 + 4)?;
    document.close_turn(TurnOutcome::Completed, 4, T0 + 4)?;
    document.submit_root_input_fixture("second", vec![resource], T0 + 5)?;
    let second = prompt_seq(&document, "second");
    document.freeze_user_prompt_media(
        vec![UserPromptMediaBinding {
            prompt_seq: second,
            origin: origin("B", &[resource], false),
        }],
        T0 + 6,
    )?;
    // Generation edits require the existing safe closed-turn boundary, as all prior edit tests do.
    // 代际编辑要求既有安全闭合回合边界，与此前全部编辑测试一致。
    let api = document.start_api(
        ApiPurposeKind::MainGeneration,
        ApiRequestSnapshot::new("openai", "gpt-5"),
        T0 + 7,
    )?;
    document.append_assistant_text(api, "done", AssistantSemanticStatus::Completed, T0 + 8)?;
    complete_ok(&mut document, api, None, T0 + 9)?;
    document.close_turn(TurnOutcome::Completed, 4, T0 + 9)?;
    // Follow the real generation precondition: issue a flush candidate and acknowledge that exact revision.
    // 遵循真实代际前置条件：生成落盘候选并确认其精确修订。
    let candidate = document
        .prepare_flush_candidates()?
        .expect("generation flush candidate");
    document.advance_durable(candidate.revision)?;
    let edited = document.build_edited_generation(EditGenerationInput {
        target_seq: second,
        replacement: RootInputReplacement::new("edited", vec![resource]),
        now_ms: T0 + 10,
    })?;
    assert_eq!(edited.events[0], document.events()[0]);
    let EventBody::UserInput(input) = &edited
        .events
        .iter()
        .find(|event| event.seq == second)
        .expect("edited prompt keeps the target seq")
        .event
    else {
        unreachable!("edited root")
    };
    assert_eq!(
        input.parameters,
        UserPromptParameters::for_new_resources(&[resource])
    );
    Ok(())
}

/// Verifies the API transaction freezes prompt parameters atomically and rejects both proof and route drift.
/// 验证API事务原子冻结提示词参数，并拒绝证明及路线漂移。
#[test]
fn user_prompt_media_api_commit_is_atomic_and_bound_to_exact_request() -> Result<(), DomainError> {
    for invalid in 0..3 {
        let mut document = new_document();
        let resource = ResourceId::try_new(1)?;
        document.submit_root_input_fixture("input", vec![resource], T0)?;
        let seq = prompt_seq(&document, "input");
        let mut request = ApiRequestSnapshot::new("openai", "gpt-5");
        request.connection_id = Some(String::from("connection-1"));
        request.profile = Some(String::from("openai-responses.v1"));
        let mut projection = ProviderProjectionProof {
            segment_generation: document.segment_generation(),
            document_revision: document.memory_revision(),
            retained_from_seq: seq,
            through_seq: seq,
            fingerprint_sha256: "a".repeat(64),
        };
        if invalid == 1 {
            projection.fingerprint_sha256 = String::from("invalid");
        }
        let mut binding = UserPromptMediaBinding {
            prompt_seq: seq,
            origin: origin("gpt-5", &[resource], false),
        };
        if invalid == 2 {
            binding.origin.route.model_id = String::from("later-model");
        }
        let before = document.events().to_vec();
        let revision = document.memory_revision();
        let result = document.prepare_and_start_api_with_user_prompt_media(
            PrepareAndStartApiInput {
                purpose: ApiPurposeKind::MainGeneration,
                request,
                projection,
                request_budget_evidence: test_provider_request_budget_evidence(),
                locked_baseline: LockedBaselineProjection::project(vec![
                    LockedBaselineSegmentInput {
                        kind: LockedSourceKind::AgentsMd,
                        node_id: String::from("agents-project"),
                        target: LockedPromptTarget::Developer,
                        source_label: Some(String::from("AGENTS.md")),
                        source_id: None,
                        content: String::from("stable current-generation rules"),
                    },
                ])?,
                continuation_action_id: None,
                now_ms: T0 + 1,
            },
            vec![binding],
        );
        if invalid != 0 {
            assert!(result.is_err());
            assert_eq!(document.events(), before);
            assert_eq!(document.memory_revision(), revision);
        } else {
            let receipt = result?.into_value();
            assert_eq!(document.memory_revision(), revision + 1);
            let patch = document.memory_revision_receipt(revision + 1)?;
            assert_eq!(patch.changed_event_seqs, vec![seq, receipt.started_seq]);
            let EventBody::UserInput(input) = &document.events()[0].event else {
                unreachable!("root input")
            };
            assert!(matches!(
                input
                    .parameters
                    .as_ref()
                    .expect("prompt owner")
                    .media_delivery,
                UserPromptMediaDelivery::Frozen { .. }
            ));
            let api_json =
                serde_json::to_string(&document.events().last().expect("API event").event)
                    .expect("API JSON");
            assert!(!api_json.contains("media_delivery"));
            assert!(!api_json.contains("reference_fingerprint"));
            // The media transaction must retain only the caller-supplied prompt baseline in the runtime lock.
            // 媒体事务必须让运行时锁只保留调用方提供的提示词基线。
            assert_eq!(document.runtime_lock.locked_sources.len(), 1);
            assert_eq!(
                document.runtime_lock.locked_sources[0].content,
                "stable current-generation rules"
            );
            // Completing and starting a second physical request proves prompt-media parameters never alter the locked runtime baseline.
            // 完成并启动第二次物理请求，证明提示词媒体参数绝不会改变锁定运行时基线。
            let repeated_request = ApiRequestSnapshot {
                connection_id: Some(String::from("connection-1")),
                profile: Some(String::from("openai-responses.v1")),
                ..ApiRequestSnapshot::new("openai", "gpt-5")
            };
            document.complete_api_group(
                receipt.api_call_id,
                ApiCompletion {
                    request: repeated_request.clone(),
                    usage: usage(1, 1),
                    status: ApiCallStatus::Completed,
                    end_reason: Some(ApiEndReason::CompletedSuccessfully),
                    error: None,
                    continuation_eligibility: ContinuationEligibility::Forbidden,
                    timing: ApiTimings::default(),
                    provider_source: None,
                    completed_at_ms: T0 + 2,
                },
            )?;
            let second_api =
                document.start_api(ApiPurposeKind::MainGeneration, repeated_request, T0 + 3)?;
            assert_ne!(second_api, receipt.api_call_id);
        }
    }
    Ok(())
}
