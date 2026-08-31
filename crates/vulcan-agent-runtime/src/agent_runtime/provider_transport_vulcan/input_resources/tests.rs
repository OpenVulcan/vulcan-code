//! Regression tests for the input_resources boundary.
//! input_resources边界的回归测试。

use super::*;

/// Verifies frozen manifest constraints produce Provider-neutral degradation reasons.
/// 验证冻结清单约束会生成供应商无关的降级原因。
#[test]
fn frozen_resource_constraints_are_provider_neutral() {
    // localOnly is a generic route rule and contains no provider identity.
    // localOnly 是不含供应商身份的通用路由规则。
    let local_only = InputResourceConstraint {
        resource_kind: ResourceKind::Audio,
        accepted_mime_types: Vec::new(),
        max_bytes: None,
        max_count: None,
        reference_policy: ResourceReferencePolicy::LocalOnly,
        upload_channel_refs: Vec::new(),
    };
    // remoteAudio intentionally violates the generic local-only policy.
    // remoteAudio 刻意违反通用的仅本地策略。
    let remote_audio = ResourceReference {
        id: String::from("audio-1"),
        kind: ResourceKind::Audio,
        media_type: Some(String::from("audio/mpeg")),
        source_kind: vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
        file_name: None,
        image_detail: None,
        resource_path: None,
        local_path: None,
        url: Some(String::from("https://media.example/audio.mp3")),
    };
    assert!(frozen_resource_constraint_unavailability_reason(&local_only, &remote_audio).is_some());
    // preparedLocalAudio retains local provenance while carrying immutable bytes prepared by the host.
    // preparedLocalAudio 保留本地来源，同时携带由宿主准备的不可变字节。
    let prepared_local_audio = ResourceReference {
        id: String::from("audio-prepared"),
        kind: ResourceKind::Audio,
        media_type: Some(String::from("audio/mpeg")),
        source_kind: ResourceSourceKind::HostResolved,
        file_name: Some(String::from("audio.mp3")),
        image_detail: None,
        resource_path: None,
        local_path: Some(String::from("C:/workspace/audio.mp3")),
        url: Some(String::from("data:audio/mpeg;base64,AA==")),
    };
    assert!(
        frozen_resource_constraint_unavailability_reason(&local_only, &prepared_local_audio,)
            .is_none(),
        "host-prepared local media should retain local-only provenance"
    );

    // pdfOnly proves MIME admission comes entirely from frozen manifest data.
    // pdfOnly 证明 MIME 准入完全来自冻结清单数据。
    let pdf_only = InputResourceConstraint {
        resource_kind: ResourceKind::File,
        accepted_mime_types: vec![String::from("application/pdf")],
        max_bytes: None,
        max_count: None,
        reference_policy: ResourceReferencePolicy::Any,
        upload_channel_refs: Vec::new(),
    };
    // textFile uses an otherwise valid remote representation with the wrong MIME type.
    // textFile 使用其他方面有效但 MIME 类型错误的远程表示。
    let text_file = ResourceReference {
        id: String::from("file-1"),
        kind: ResourceKind::File,
        media_type: Some(String::from("text/plain")),
        source_kind: vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
        file_name: None,
        image_detail: None,
        resource_path: None,
        local_path: None,
        url: Some(String::from("https://media.example/file.txt")),
    };
    assert!(frozen_resource_constraint_unavailability_reason(&pdf_only, &text_file).is_some());

    // remoteOnlyUpload proves local canonical resources remain eligible only with an exact upload channel.
    // remoteOnlyUpload 证明本地规范资源只有在存在精确上传通道时才可进入仅远程投影。
    let remote_only_upload = InputResourceConstraint {
        resource_kind: ResourceKind::Audio,
        accepted_mime_types: vec![String::from("audio/mpeg")],
        max_bytes: None,
        max_count: None,
        reference_policy: ResourceReferencePolicy::RemoteOnly,
        upload_channel_refs: vec![String::from("audio-upload")],
    };
    let local_audio = ResourceReference {
        id: String::from("audio-local"),
        kind: ResourceKind::Audio,
        media_type: Some(String::from("audio/mpeg")),
        source_kind: ResourceSourceKind::HostResolved,
        file_name: Some(String::from("audio.mp3")),
        image_detail: None,
        resource_path: None,
        local_path: Some(String::from("C:/workspace/audio.mp3")),
        url: None,
    };
    assert!(
        frozen_resource_constraint_unavailability_reason(&remote_only_upload, &local_audio)
            .is_none()
    );
}

/// Verifies metadata-only projection exposes only the safe locator and never the managed host path.
/// 验证纯元数据投影只暴露安全定位符，绝不暴露宿主管理路径。
#[test]
fn resource_projection_fallback_uses_the_safe_resource_path() {
    let resource = ResourceReference {
        id: String::from("asset-1"),
        kind: ResourceKind::Image,
        media_type: Some(String::from("image/png")),
        source_kind: ResourceSourceKind::HostResolved,
        file_name: Some(String::from("screen\"\nINJECTED=1.png")),
        image_detail: None,
        resource_path: Some(String::from(
            ".vulcan/code/attachments/images/screen\"\nINJECTED=1.png",
        )),
        local_path: Some(String::from("C:/private/screen.png")),
        url: None,
    };

    let text = resource_projection_budget_fallback_text(
        &resource,
        "the request-wide host inline-media budget",
        Some(128),
    );

    assert!(text.contains(".vulcan/code/attachments/images/screen\\\"\\nINJECTED=1.png"));
    assert!(!text.contains("\nINJECTED=1"));
    assert!(!text.contains("C:/private"));
    assert!(!text.contains("file_id"));
}

/// Verifies provider file ids use the dedicated source channel instead of masquerading as URLs.
/// 验证供应商文件标识使用专用来源通道，而不会伪装成 URL。
#[test]
fn prepared_provider_file_id_preserves_reference_encoding() {
    let mut resource = ResourceReference {
        id: String::from("asset-1"),
        kind: ResourceKind::Image,
        media_type: Some(String::from("image/png")),
        source_kind: ResourceSourceKind::HostResolved,
        file_name: Some(String::from("screen.png")),
        image_detail: None,
        resource_path: Some(String::from(".vulcan/code/attachments/screen.png")),
        local_path: Some(String::from("C:/private/screen.png")),
        url: None,
    };

    apply_prepared_media_reference(
        &mut resource,
        vulcan_provider_adapters::PreparedMediaReferenceSnapshot {
            provider_reference: String::from("file-provider-1"),
            reference_encoding: UploadReferenceEncoding::ProviderFileId,
            created_at_unix_millis: 1,
            expires_at_unix_millis: 2,
        },
    );

    assert_eq!(resource.id, "file-provider-1");
    assert_eq!(resource.source_kind, ResourceSourceKind::ProviderFileId);
    assert!(resource.url.is_none());
    assert_eq!(
        resource.resource_path.as_deref(),
        Some(".vulcan/code/attachments/screen.png")
    );
    let fallback = resource_projection_budget_fallback_text(
        &resource,
        "the request-wide host inline-media budget",
        Some(128),
    );
    assert!(!fallback.contains("file-provider-1"));
    assert!(fallback.contains(".vulcan/code/attachments/screen.png"));
}

/// Verifies a Provider file id cannot leak through the fallback locator when canonical metadata is absent.
/// 验证缺失规范元数据时，供应商文件标识不能通过降级定位符泄漏。
#[test]
fn provider_file_id_fallback_without_resource_path_is_redacted() {
    let resource = ResourceReference {
        id: String::from("secret-provider-file-id"),
        kind: ResourceKind::Image,
        media_type: Some(String::from("image/png")),
        source_kind: ResourceSourceKind::ProviderFileId,
        file_name: Some(String::from("screen.png")),
        image_detail: None,
        resource_path: None,
        local_path: None,
        url: None,
    };

    let fallback = resource_projection_budget_fallback_text(
        &resource,
        "the request-wide host inline-media budget",
        Some(128),
    );

    assert!(!fallback.contains("secret-provider-file-id"));
    assert!(fallback.contains("attachment://unavailable"));
    assert!(fallback.contains("id=\"unavailable\""));
}

/// Verifies URL-shaped upload references retain the canonical attachment identity.
/// 验证 URL 形上传引用会保留规范附件身份。
#[test]
fn prepared_response_url_preserves_canonical_resource_identity() {
    let mut resource = ResourceReference {
        id: String::from("asset-1"),
        kind: ResourceKind::Video,
        media_type: Some(String::from("video/mp4")),
        source_kind: ResourceSourceKind::HostResolved,
        file_name: Some(String::from("clip.mp4")),
        image_detail: None,
        resource_path: Some(String::from(".vulcan/code/attachments/clip.mp4")),
        local_path: Some(String::from("C:/private/clip.mp4")),
        url: None,
    };

    apply_prepared_media_reference(
        &mut resource,
        vulcan_provider_adapters::PreparedMediaReferenceSnapshot {
            provider_reference: String::from("https://media.example/clip.mp4"),
            reference_encoding: UploadReferenceEncoding::ResponseUrl,
            created_at_unix_millis: 1,
            expires_at_unix_millis: 2,
        },
    );

    assert_eq!(resource.id, "asset-1");
    assert_eq!(resource.source_kind, ResourceSourceKind::RemoteUrl);
    assert_eq!(
        resource.url.as_deref(),
        Some("https://media.example/clip.mp4")
    );
}

/// Verifies aggregate Base64 accounting uses exact expansion and fails closed at the host limit.
/// 验证 Base64 聚合计费使用精确膨胀值，并在宿主上限处封闭失败。
#[test]
fn inline_resource_budget_accounts_for_exact_base64_expansion() {
    let mut budget = InlineResourceProjectionBudget::default();
    assert_eq!(budget.reserve(ResourceKind::Image, None, Some(1)), Ok(()));
    assert_eq!(budget.used_base64_bytes, 4);
    assert_eq!(
        budget.reserve(
            ResourceKind::Image,
            None,
            Some((HOST_INLINE_RESOURCE_BASE64_BUDGET_BYTES - 4) / 4 * 3),
        ),
        Ok(())
    );
    assert_eq!(
        budget.reserve(ResourceKind::Image, None, Some(1)),
        Err("the request-wide host inline-media budget")
    );
    assert_eq!(
        budget.retained_resource_counts.get(&ResourceKind::Image),
        Some(&2)
    );
}

/// Verifies native resource counts are shared across the whole request instead of reset per message.
/// 验证原生资源数量在整个请求中共享，而不是按消息重置。
#[test]
fn inline_resource_budget_enforces_request_wide_resource_count() {
    let mut budget = InlineResourceProjectionBudget::default();
    assert_eq!(budget.reserve(ResourceKind::Image, Some(1), None), Ok(()));
    assert!(budget.count_limit_reached(ResourceKind::Image, Some(1)));
    assert_eq!(
        budget.reserve(ResourceKind::Image, Some(1), None),
        Err("the frozen route request-wide resource-count limit")
    );
    assert_eq!(budget.reserve(ResourceKind::Audio, Some(1), None), Ok(()));
}

/// Verifies pre-materialized inline data is decoded for the same aggregate budget as local files.
/// 验证预先物化的内联数据会被解码，并与本地文件使用同一聚合预算。
#[test]
fn inline_data_resource_reports_exact_raw_byte_length() {
    let resource = ResourceReference {
        id: String::from("inline-image"),
        kind: ResourceKind::Image,
        media_type: Some(String::from("image/png")),
        source_kind: ResourceSourceKind::InlineData,
        file_name: Some(String::from("image.png")),
        image_detail: None,
        resource_path: None,
        local_path: None,
        url: Some(String::from("data:image/png;base64,AQIDBA==")),
    };

    assert_eq!(inline_data_resource_raw_byte_len(&resource), Ok(Some(4)));
}

/// Verifies generic files never enter media projectors merely because a profile supports media.
/// 验证通用文件不会仅因协议 Profile 支持媒体而进入媒体投影器。
#[test]
fn protocol_resource_kind_gate_keeps_generic_files_metadata_only() {
    for protocol in [
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
        vulcan_ai_protocols::GEMINI_GENERATE_CONTENT_PROFILE_ID,
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID,
    ] {
        assert!(!protocol_profile_supports_resource_kind(
            protocol,
            ResourceKind::File,
        ));
    }
    assert!(protocol_profile_supports_resource_kind(
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID,
        ResourceKind::Image,
    ));
}
