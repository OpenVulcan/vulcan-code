//! Media admission, scalar cache, and cancelled-upload progress regressions.
//! 媒体准入、标量缓存与上传取消进度回归测试。

use super::*;
use std::sync::Mutex as StdMutex;

/// Builds a frozen all-kind extension target for the exact policy, profile and optional upload.
/// 为精确策略、Profile 与可选上传构建冻结的全类型扩展目标。
/// Returns metadata only; this fixture performs no source reads or network operations.
/// 仅返回元数据；此夹具不读取来源，也不执行网络操作。
fn model_replacement_target<'a>(
    policy: crate::MediaDeliveryPolicy,
    profile_id: &'a str,
    upload: Option<&'a crate::ConversationMediaUploadContract>,
) -> ModelReplacementMediaTarget<'a> {
    ModelReplacementMediaTarget {
        policy,
        profile_id,
        model_inputs: &[
            ResourceKind::Image,
            ResourceKind::Audio,
            ResourceKind::Video,
            ResourceKind::File,
        ],
        constraint: None,
        upload,
    }
}

/// Builds a declared URI-producing upload with a fixed hard byte limit.
/// 构建返回 URI 且具有固定字节硬限制的已声明上传。
/// Returns an isolated non-secret contract used by admission tests.
/// 返回准入测试使用的隔离非秘密契约。
fn model_replacement_upload() -> crate::ConversationMediaUploadContract {
    crate::ConversationMediaUploadContract {
        timeout_ms: 60_000,
        upload_channel_id: String::from("test-real-upload"),
        contract_fingerprint: String::from("contract"),
        reference_scope: crate::UploadReferenceScope::Session,
        cache_duration_ms: Some(60_000),
        reference_encoding: crate::UploadReferenceEncoding::ResponseUrl,
        max_bytes: Some(12 * 1024 * 1024),
        max_count: Some(1),
    }
}

/// Verifies every modality uses the independent 10 MiB default and an inclusive raw-byte boundary.
/// 验证所有模态使用独立的 10 MiB 默认值，并包含原始字节边界值。
#[test]
fn media_understanding_upload_policy_is_modality_and_threshold_exact() {
    // Cases retain video coverage while adding files and independent extension size semantics.
    // 用例保留视频覆盖，同时加入文件与独立扩展大小语义。
    let cases = [
        (ResourceKind::Image, "image/png"),
        (ResourceKind::Audio, "audio/wav"),
        (ResourceKind::Video, "video/mp4"),
        (ResourceKind::File, "application/pdf"),
    ];
    for (kind, mime) in cases {
        // The extension default is separate from the conversation's 512 KiB policy.
        // 扩展默认值与主会话的 512 KiB 策略分离。
        let boundary = 10 * 1024 * 1024;
        assert_eq!(
            model_replacement_target(
                crate::MediaDeliveryPolicy::default(),
                vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
                None
            )
            .plan(kind, mime, boundary)
            .expect("inclusive independent boundary"),
            crate::LocalMediaMaterialization::Inline {
                source_byte_len: boundary
            },
        );
        assert!(
            model_replacement_target(
                crate::MediaDeliveryPolicy::default(),
                vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
                None
            )
            .plan(kind, mime, boundary + 1)
            .expect_err("oversized extension must fail")
            .contains("inline size limit")
        );
    }
}

/// Verifies a real interface limit wins before configurable inline preference or cached-reference reuse.
/// 验证真实接口限制优先于可配置内联偏好或缓存引用复用。
#[test]
fn media_understanding_policy_honors_upload_preference_and_hard_limit() {
    // A real upload is the default even for a single-byte video.
    // 即使视频只有一字节，默认仍使用真实上传。
    let mut upload = model_replacement_upload();
    assert_eq!(
        model_replacement_target(
            crate::MediaDeliveryPolicy::default(),
            vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            Some(&upload)
        )
        .plan(ResourceKind::Video, "video/mp4", 1)
        .expect("default upload"),
        crate::LocalMediaMaterialization::ProviderUpload { source_byte_len: 1 },
    );
    // A positive with-upload threshold admits its exact boundary and no byte beyond it.
    // 正数有上传阈值允许其精确边界，超出一字节则不允许。
    let mut policy = crate::MediaDeliveryPolicy::default();
    policy.independent_extension.video.with_upload_max_bytes = 8;
    assert_eq!(
        model_replacement_target(
            policy,
            vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            Some(&upload)
        )
        .plan(ResourceKind::Video, "video/mp4", 8)
        .expect("small upload-capable inline"),
        crate::LocalMediaMaterialization::Inline { source_byte_len: 8 },
    );
    assert_eq!(
        model_replacement_target(
            policy,
            vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            Some(&upload)
        )
        .plan(ResourceKind::Video, "video/mp4", 9)
        .expect("above preference uploads"),
        crate::LocalMediaMaterialization::ProviderUpload { source_byte_len: 9 },
    );
    upload.max_bytes = Some(4);
    assert!(
        model_replacement_target(
            policy,
            vulcan_llm_core::GEMINI_GENERATE_CONTENT_PROFILE_ID,
            Some(&upload)
        )
        .plan(ResourceKind::Video, "video/mp4", 8)
        .expect_err("hard interface ceiling")
        .contains("too large for the selected upload interface")
    );
}

/// Verifies exact reference forms, route restrictions and budgets are checked before materialization.
/// 验证物化之前检查精确引用形态、路由限制与预算。
#[test]
fn media_understanding_policy_rejects_invalid_consumer_and_route_before_upload() {
    // Chat video accepts URLs but cannot consume a bare provider file identifier.
    // Chat 视频接受 URL，但不能消费裸供应商文件标识。
    let mut upload = model_replacement_upload();
    upload.reference_encoding = crate::UploadReferenceEncoding::ProviderFileId;
    assert!(
        model_replacement_target(
            crate::MediaDeliveryPolicy::default(),
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            Some(&upload)
        )
        .plan(ResourceKind::Video, "video/mp4", 1)
        .expect_err("invalid reference contract")
        .contains("reference cannot be consumed")
    );
    assert!(
        model_replacement_target(
            crate::MediaDeliveryPolicy::default(),
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            None
        )
        .plan(ResourceKind::Video, "video/mp4", 1)
        .expect_err("Responses has no video projector")
        .contains("cannot consume this media inline")
    );
    assert!(
        model_replacement_target(
            crate::MediaDeliveryPolicy::default(),
            vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,
            None
        )
        .plan(ResourceKind::File, "application/pdf", 1)
        .is_ok()
    );
    // A request's encoded budget is independent of individual raw-byte thresholds.
    // 请求编码预算独立于各原始字节阈值。
    let mut policy = crate::MediaDeliveryPolicy::default();
    policy.independent_extension.max_total_encoded_bytes = 4;
    assert!(
        model_replacement_target(policy, vulcan_llm_core::OPENAI_CHAT_PROFILE_ID, None)
            .plan(ResourceKind::Audio, "audio/wav", 4)
            .expect_err("encoded aggregate is too large")
            .contains("combined encoded files")
    );
    // Model-specific MIME and count constraints remain authoritative.
    // 模型特定的 MIME 与数量限制始终保持权威。
    let mut constraint = crate::InputResourceConstraint {
        resource_kind: ResourceKind::Video,
        accepted_mime_types: vec![String::from("video/mp4")],
        max_bytes: Some(4),
        max_count: Some(1),
        reference_policy: crate::ResourceReferencePolicy::Any,
        upload_channel_refs: Vec::new(),
    };
    for mode in 0..4 {
        // Each mutation tests one independent route rejection without changing policy.
        // 每次变更测试一个独立路由拒绝条件，不改变策略。
        match mode {
            0 => constraint.max_bytes = Some(0),
            1 => {
                constraint.max_bytes = Some(4);
                constraint.max_count = Some(0);
            }
            2 => {
                constraint.max_count = Some(1);
                constraint.reference_policy = crate::ResourceReferencePolicy::RemoteOnly;
            }
            _ => {
                constraint.reference_policy = crate::ResourceReferencePolicy::Any;
                constraint.accepted_mime_types = vec![String::from("video/webm")];
            }
        }
        // The borrowed constraint is frozen for each individual invocation.
        // 借用约束在每次独立调用期间保持冻结。
        let mut target = model_replacement_target(
            crate::MediaDeliveryPolicy::default(),
            vulcan_llm_core::OPENAI_CHAT_PROFILE_ID,
            None,
        );
        target.constraint = Some(&constraint);
        assert!(target.plan(ResourceKind::Video, "video/mp4", 1).is_err());
    }
}

/// Verifies collection media bindings bypass only the scalar cache and remain valid for generic upload materialization.
/// 验证集合媒体绑定仅绕过标量缓存，且仍可由通用上传物化器正常处理。
#[test]
fn media_understanding_collection_upload_binding_bypasses_scalar_cache() {
    let scalar = crate::OperationUploadBinding {
        source_path: String::from("/video_path"),
        target_path: String::from("/video"),
        item_source_path: None,
        item_target_path: None,
        item_match_path: None,
        item_match_value: None,
        upload_channel_refs: vec![String::from("provider-files")],
        mime_type: Some(String::from("video/mp4")),
        mime_types_by_extension: Default::default(),
        required: true,
        remove_source: true,
        target_encoding: Default::default(),
    };
    let collection = crate::OperationUploadBinding {
        source_path: String::from("/references"),
        target_path: String::from("/references"),
        item_source_path: Some(String::from("/path")),
        item_target_path: Some(String::from("/resource")),
        ..scalar.clone()
    };

    assert!(provider_tool_media_upload_binding_is_cacheable(&scalar));
    assert!(!provider_tool_media_upload_binding_is_cacheable(
        &collection
    ));
}

/// Verifies dropping an active upload emits cancellation with the latest byte count.
/// 验证丢弃进行中的上传会使用最近字节数发出取消终态。
#[test]
fn media_upload_terminal_guard_emits_cancelled_on_drop() {
    // observed owns the complete ordered event sequence emitted by the test context.
    // observed 持有测试上下文发出的完整有序事件序列。
    let observed = Arc::new(StdMutex::new(Vec::new()));
    let observed_for_callback = Arc::clone(&observed);
    let context = RuntimeMediaUploadProgressContext {
        observer: Arc::new(move |event| {
            observed_for_callback
                .lock()
                .expect("progress event lock")
                .push(event);
        }),
        base: crate::RuntimeMediaUploadProgressEvent {
            transfer_id: String::from("transfer-one"),
            owner_session_id: String::from("session-one"),
            provider_id: String::from("provider-one"),
            service_entry_id: String::from("entry-one"),
            connection_id: String::from("connection-one"),
            model_id: String::from("model-one"),
            protocol_profile_id: String::from("provider-tool"),
            manifest_revision: 1,
            manifest_generation: 2,
            route_revision: String::from("route-one"),
            operation_id: String::from("video-understanding"),
            upload_channel_id: String::from("video-file"),
            file_name: String::from("clip.mp4"),
            media_kind: String::from("video"),
            phase: crate::RuntimeMediaUploadPhase::Preparing,
            bytes_sent: 0,
            total_bytes: Some(10),
            error: None,
        },
        last_bytes_sent: Arc::new(AtomicU64::new(0)),
        dispatch_state: Arc::default(),
    };
    {
        // guard remains armed while one uploading event advances the retained byte counter.
        // guard 在一条上传事件推进保留字节计数时保持启用。
        let _guard = RuntimeMediaUploadTerminalGuard::new(Some(&context));
        context.emit(crate::RuntimeMediaUploadPhase::Uploading, 7, Some(10), None);
    }

    let events = observed.lock().expect("progress event lock");
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].phase, crate::RuntimeMediaUploadPhase::Uploading);
    assert_eq!(events[1].phase, crate::RuntimeMediaUploadPhase::Cancelled);
    assert_eq!(events[1].bytes_sent, 7);
}
