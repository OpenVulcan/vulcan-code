//! Prompt-media origin persistence across physical segment rollover boundaries.
//! 提示词媒体原身份跨物理分片滚动边界的持久化。

use super::*;
use vulcan_session_core::v1::{
    ResourceId, UserPromptMediaBinding, UserPromptMediaDelivery, UserPromptMediaOrigin,
    UserPromptMediaRoute, UserPromptResourceBinding, UserPromptResourceDelivery,
    UserPromptResourceDeliveryKind,
};

/// Persists partial resource completion in the real tail, retains writability until API freeze, and keeps original admission parameters unchanged.
/// 将部分资源完成写入真实尾段，在API冻结前保留可写性，并保持原准入参数不变。
#[test]
fn user_prompt_media_partial_completion_survives_flush_reload_and_rollover() {
    // The tiny segment limit exercises the real writer's rollover decision on every flush.
    // 极小分片限制在每次刷新时验证真实写入器滚动决策。
    let (_root, layout) = fresh_layout("partial-media-rollover");
    // Only this isolated session owner can publish its chronological prompt.
    // 仅本隔离会话所有者可发布其时序提示词。
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    // Repeated resource ids retain independent positions throughout partial persistence.
    // 重复资源ID在整个部分持久化过程中保持独立位置。
    let resource_id = ResourceId::try_new(7).expect("resource");
    // The exact submission keeps no earlier draft proof.
    // 精确提交不携带更早草稿证明。
    let mut submission = test_submission("partial-media", "partial prompt");
    submission.resource_ids = vec![resource_id, resource_id];
    document.submit_root_input(submission, 2).expect("root");
    // First resource completion belongs to model A, without yet claiming the first request route.
    // 第一次资源完成属于模型A，尚不声称首次请求路线。
    let route = UserPromptMediaRoute {
        provider_id: "openai".into(),
        service_entry_id: "api".into(),
        connection_id: "connection-1".into(),
        model_id: "A".into(),
        protocol_profile_id: "openai-responses.v1".into(),
    };
    // Opaque receipt proof contains no provider URL or media bytes.
    // 不透明回执证明不含供应商URL或媒体字节。
    let first = UserPromptResourceDelivery {
        resource_id,
        original_route: route.clone(),
        delivery: UserPromptResourceDeliveryKind::Uploaded {
            upload_channel_id: "files".into(),
            upload_contract_fingerprint: format!("sha256:{}", "a".repeat(64)),
            reference_fingerprint: "b".repeat(64),
        },
    };
    // Original sequence remains fixed while only parameters change.
    // 仅参数变化时，原序号保持固定。
    let prompt_seq = document.events()[0].seq();
    document
        .record_user_prompt_resource_delivery(
            document.segment_generation(),
            UserPromptResourceBinding {
                prompt_seq,
                resource_position: 0,
                resource: first.clone(),
            },
            3,
        )
        .expect("partial result");
    // Flush under real segment constraints, without using an in-memory-only persistence substitute.
    // 在真实分片约束下刷新，不使用仅内存持久化替代物。
    let limits = FlushLimits {
        max_lines: 1,
        max_bytes: 8 * 1024 * 1024,
    };
    flush_with_limits(&layout, &mut document, &limits, 3);
    assert_eq!(document.metadata.split_list.len(), 1);
    // A fresh disk reload must retain partial results and the unfinished second position.
    // 全新磁盘重载必须保留部分结果及未完成的第二个位置。
    let mut restored = load_session(&layout).expect("partial reload").document;
    // Read the exact event reloaded from disk, not a cloned in-memory parameter.
    // 读取从磁盘重载的精确事件，而非克隆的内存参数。
    let vulcan_session_core::v1::EventBody::UserInput(input) = &restored.events()[0].event else {
        unreachable!("root")
    };
    assert_eq!(
        input
            .parameters
            .as_ref()
            .expect("parameters")
            .media_delivery,
        UserPromptMediaDelivery::Preparing {
            resources: vec![Some(first.clone()), None]
        }
    );
    assert!(
        input
            .parameters
            .as_ref()
            .expect("parameters")
            .prepared_resources
            .is_empty()
    );
    // Model B later owns the first request but may not replace A's already completed occurrence.
    // 模型B稍后持有首次请求，但不能替换A已完成出现。
    let mut request_route = route;
    request_route.model_id = "B".into();
    // The second occurrence has its own later inline choice.
    // 第二次出现具有其独立的较晚内联选择。
    let second = UserPromptResourceDelivery {
        resource_id,
        original_route: request_route.clone(),
        delivery: UserPromptResourceDeliveryKind::Inline,
    };
    restored
        .freeze_user_prompt_media(
            vec![UserPromptMediaBinding {
                prompt_seq,
                origin: UserPromptMediaOrigin {
                    route: request_route,
                    resources: vec![first, second],
                },
            }],
            4,
        )
        .expect("first request freeze");
    flush_with_limits(&layout, &mut restored, &limits, 4);
    assert_eq!(restored.metadata.split_list.len(), 2);
    assert_eq!(
        load_session(&layout)
            .expect("sealed reload")
            .document
            .events(),
        restored.events()
    );
}

/// Verifies a pending media prompt stays writable, then permits sealing after its original parameters are frozen.
/// 验证待准备媒体提示词保持可写，原参数冻结后才允许封口。
#[test]
fn user_prompt_media_pending_defers_rollover_until_origin_is_frozen() {
    let (_root, layout) = fresh_layout("pending-media-rollover");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
    document.metadata = publish_initial(&layout, &document);
    let resource_id = ResourceId::try_new(7).expect("project resource");
    let mut submission = test_submission("pending-media", "prompt");
    submission.resource_ids = vec![resource_id];
    document
        .submit_root_input(submission, 2)
        .expect("root with media");
    let prompt_seq = document.events()[0].seq();
    let limits = FlushLimits {
        max_lines: 1,
        max_bytes: 8 * 1024 * 1024,
    };
    flush_with_limits(&layout, &mut document, &limits, 2);
    assert_eq!(
        document.metadata.split_list.len(),
        1,
        "pending origin must not be sealed"
    );
    let mut restored = load_session(&layout)
        .expect("pending prompt reload")
        .document;
    restored
        .freeze_user_prompt_media(
            vec![UserPromptMediaBinding {
                prompt_seq,
                origin: UserPromptMediaOrigin {
                    route: UserPromptMediaRoute {
                        provider_id: String::from("openai"),
                        service_entry_id: String::from("api"),
                        connection_id: String::from("connection-1"),
                        model_id: String::from("gpt-5"),
                        protocol_profile_id: String::from("openai-responses.v1"),
                    },
                    resources: vec![UserPromptResourceDelivery {
                        resource_id,
                        original_route: UserPromptMediaRoute {
                            provider_id: String::from("openai"),
                            service_entry_id: String::from("api"),
                            connection_id: String::from("connection-1"),
                            model_id: String::from("gpt-5"),
                            protocol_profile_id: String::from("openai-responses.v1"),
                        },
                        delivery: UserPromptResourceDeliveryKind::Inline,
                    }],
                },
            }],
            3,
        )
        .expect("freeze original pending prompt");
    flush_with_limits(&layout, &mut restored, &limits, 3);
    assert_eq!(
        restored.metadata.split_list.len(),
        2,
        "frozen origin permits normal sealing"
    );
    let reloaded = load_session(&layout).expect("sealed prompt reload");
    assert_eq!(reloaded.document.events()[0], restored.events()[0]);
}

/// Verifies a passed request boundary or closed turn cannot keep unresolved media history pinning the active tail forever.
/// 验证已过请求边界或已闭合回合不会让未解析媒体历史永久占住活动尾段。
#[test]
fn user_prompt_media_finished_boundaries_release_rollover_pin() {
    for close_turn in [false, true] {
        let (_root, layout) = fresh_layout(if close_turn {
            "closed-prompt-media"
        } else {
            "past-prompt-media"
        });
        let mut document = SessionDocument::new(&layout.session_id, "project-1", "title", 1);
        document.metadata = publish_initial(&layout, &document);
        let mut submission = test_submission("media-boundary", "prompt");
        submission.resource_ids = vec![ResourceId::try_new(1).expect("project resource")];
        document
            .submit_root_input(submission, 2)
            .expect("root with media");
        if close_turn {
            document
                .close_turn(TurnOutcome::Stopped, 0, 3)
                .expect("cancel before API");
        } else {
            // A legacy producer may not have frozen an origin; it becomes unknown history, not a new upload intent.
            // 旧生产者可能未冻结原身份；它会成为未知历史，而不是新的上传意图。
            let api = prepare_api(
                &mut document,
                ApiPurposeKind::MainGeneration,
                ApiRequestSnapshot::new("openai", "gpt-5"),
                3,
            );
            complete_api(&mut document, api, 4, None);
        }
        flush_with_limits(
            &layout,
            &mut document,
            &FlushLimits {
                max_lines: 1,
                max_bytes: 8 * 1024 * 1024,
            },
            5,
        );
        assert_eq!(document.metadata.split_list.len(), 2);
        load_session(&layout).expect("finished media history remains loadable");
    }
}
