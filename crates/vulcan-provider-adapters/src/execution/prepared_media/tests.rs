//! Regression coverage for frozen upload contracts, stable source bytes, and cache retention.
//! 冻结上传契约、稳定源字节和缓存保留策略的回归覆盖。

use super::super::test_support::bundled_manifest_route;
use super::*;
use vulcan_ai_protocols::{
    ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID, OPENAI_CHAT_PROFILE_ID, OPENAI_RESPONSES_PROFILE_ID,
};

/// Verifies conversation media upload selection is frozen entirely from bundled manifests.
/// 验证对话媒体上传选择完全由内置 Manifest 冻结。
#[test]
fn bundled_media_upload_routes_are_manifest_driven() {
    // kimi_routes cover every Open Platform and Coding Plan protocol route that accepts video.
    // kimi_routes 覆盖所有接受视频的开放平台与 Coding Plan 协议路由。
    let kimi_routes = [
        (
            "platform-cn",
            OPENAI_CHAT_PROFILE_ID,
            "platform-cn.video-file",
            "api.moonshot.cn",
            "/v1/files",
        ),
        (
            "platform-global",
            OPENAI_CHAT_PROFILE_ID,
            "platform-global.video-file",
            "api.moonshot.ai",
            "/v1/files",
        ),
        (
            "coding-plan",
            OPENAI_CHAT_PROFILE_ID,
            "coding-plan.video-file",
            "api.kimi.com",
            "/coding/v1/files",
        ),
        (
            "coding-plan",
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            "coding-plan.video-file",
            "api.kimi.com",
            "/coding/v1/files",
        ),
    ];
    for (entry_id, protocol_id, channel_id, endpoint_host, endpoint_path) in kimi_routes {
        // route is the exact provider entry and protocol selected by the model binding.
        // route 是模型绑定所选择的精确供应商入口与协议。
        let route = bundled_manifest_route("kimi", entry_id, protocol_id);
        // video_constraint binds local video preparation to one declared upload channel.
        // video_constraint 将本地视频准备绑定到一个已声明上传通道。
        let video_constraint = route
            .protocol_route
            .input_resource_constraints
            .iter()
            .find(|constraint| constraint.resource_kind == ResourceKind::Video)
            .expect("Kimi video route must declare its upload binding");
        assert_eq!(video_constraint.max_bytes, Some(100_000_000));
        assert_eq!(
            video_constraint.upload_channel_refs,
            [String::from(channel_id)]
        );
        // channel freezes the complete multipart upload and provider-reference contract.
        // channel 冻结完整的 Multipart 上传与供应商引用契约。
        let channel = route
            .upload_channels
            .iter()
            .find(|channel| channel.upload_channel_id == channel_id)
            .expect("Kimi upload channel must resolve");
        assert_eq!(
            channel.primitive,
            Some(crate::UploadPrimitiveKind::MultipartFile)
        );
        assert_eq!(channel.endpoint_host.as_deref(), Some(endpoint_host));
        assert_eq!(channel.endpoint_path.as_deref(), Some(endpoint_path));
        assert_eq!(channel.file_field.as_deref(), Some("file"));
        assert_eq!(channel.max_bytes, Some(100_000_000));
        assert_eq!(channel.response_id_pointer.as_deref(), Some("/id"));
        assert_eq!(channel.reference_template.as_deref(), Some("ms://{id}"));
        assert_eq!(
            channel.fixed_fields.get("purpose").map(String::as_str),
            Some("video")
        );
        assert!(channel.progress_supported && channel.cancellation_supported);
    }

    let alibaba = bundled_manifest_route("alibaba", "model-studio-cn", OPENAI_RESPONSES_PROFILE_ID);
    let alibaba_image = alibaba
        .protocol_route
        .input_resource_constraints
        .iter()
        .find(|constraint| constraint.resource_kind == ResourceKind::Image)
        .expect("Alibaba image route must declare its upload binding");
    assert_eq!(
        alibaba_image.upload_channel_refs,
        [String::from("cn.temporary-oss")]
    );
    assert_eq!(
        alibaba
            .protocol_route
            .request_headers
            .get("x-dashscope-ossresource-resolve")
            .map(String::as_str),
        Some("enable")
    );
    assert!(alibaba.upload_channels.iter().any(|channel| {
        channel.upload_channel_id == "cn.temporary-oss"
            && channel.primitive == Some(crate::UploadPrimitiveKind::SignedObjectUpload)
            && channel.signing_primitive
                == Some(crate::UploadSigningPrimitiveKind::TemporaryOssPolicy)
    }));
}

/// Verifies prepared media keys follow provider, service, and upload-contract boundaries.
/// 验证媒体准备缓存键遵循供应商、服务与上传契约边界。
#[test]
fn prepared_media_key_isolates_frozen_route_identity() {
    // route is a cacheable Kimi video route with an explicit frozen generation identity.
    // route 是带显式冻结代际身份且允许缓存的 Kimi 视频路由。
    let mut route =
        bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
    route.manifest_revision = 10;
    route.manifest_generation = 20;
    route.route_revision = String::from("sha256:route-a");
    let source_fingerprint = PreparedMediaSourceFingerprint {
        byte_len: 42,
        modified_at: None,
        content_sha256: [7; 32],
    };
    // first is the baseline key for one provider-service-contract and source identity.
    // first 是一个供应商-服务-契约与源身份组合的基准缓存键。
    let first = prepared_media_key(
        &route,
        &source_fingerprint,
        ResourceKind::Video,
        "video/mp4",
        None,
    )
    .expect("cacheable upload route should produce a key");
    // replaced_manifest keeps the provider upload contract stable despite chat-route metadata changes.
    // replaced_manifest 在聊天路由元数据变化时保持供应商上传契约稳定。
    let mut replaced_manifest = route.clone();
    replaced_manifest.manifest_revision = 11;
    replaced_manifest.manifest_generation = 21;
    replaced_manifest.route_revision = String::from("sha256:route-b");
    // replacement_key reuses the prior reference only because the provider-service upload contract is unchanged.
    // replacement_key 仅在供应商服务上传契约未变化时复用原引用。
    let replacement_key = prepared_media_key(
        &replaced_manifest,
        &source_fingerprint,
        ResourceKind::Video,
        "video/mp4",
        None,
    )
    .expect("replacement route should produce a key");
    assert_eq!(first, replacement_key);
    // other_account proves the default provider-protocol scope ignores concrete accounts.
    // other_account 证明默认供应商协议作用域忽略具体账号。
    let mut other_account = route.clone();
    other_account.connection.id = String::from("secondary-account");
    let account_key = prepared_media_key(
        &other_account,
        &source_fingerprint,
        ResourceKind::Video,
        "video/mp4",
        None,
    )
    .expect("second account route should produce a key");
    assert_eq!(first, account_key);
}

/// Verifies explicit credential and session upload scopes narrow reference reuse.
/// 验证显式凭据与会话上传作用域会收窄引用复用边界。
#[test]
fn prepared_media_key_honors_explicit_upload_reference_scopes() {
    // route is the exact Kimi video route whose channel can be narrowed for this test.
    // route 是当前测试可收窄上传通道作用域的精确 Kimi 视频路由。
    let mut route =
        bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
    let source_fingerprint = PreparedMediaSourceFingerprint {
        byte_len: 42,
        modified_at: None,
        content_sha256: [8; 32],
    };
    // credential_scope isolates otherwise identical media references by exact connection.
    // credential_scope 按精确连接隔离其他条件完全相同的媒体引用。
    let upload_channel = route
        .upload_channels
        .iter_mut()
        .find(|channel| channel.upload_channel_id == "coding-plan.video-file")
        .expect("Kimi coding-plan video upload channel must exist");
    upload_channel.reference_scope = crate::UploadReferenceScope::Credential;
    let credential_key = prepared_media_key(
        &route,
        &source_fingerprint,
        ResourceKind::Video,
        "video/mp4",
        None,
    )
    .expect("credential-scoped upload should produce a key");
    let mut other_connection = route.clone();
    other_connection.connection.id = String::from("secondary-account");
    let other_credential_key = prepared_media_key(
        &other_connection,
        &source_fingerprint,
        ResourceKind::Video,
        "video/mp4",
        None,
    )
    .expect("second credential-scoped route should produce a key");
    assert_ne!(credential_key, other_credential_key);

    // session_scope requires an exact session identity and never falls back to a global key.
    // session_scope 要求精确会话身份，绝不回退到全局键。
    let upload_channel = route
        .upload_channels
        .iter_mut()
        .find(|channel| channel.upload_channel_id == "coding-plan.video-file")
        .expect("Kimi coding-plan video upload channel must still exist");
    upload_channel.reference_scope = crate::UploadReferenceScope::Session;
    assert!(
        prepared_media_key(
            &route,
            &source_fingerprint,
            ResourceKind::Video,
            "video/mp4",
            None,
        )
        .is_none()
    );
    let session_a = prepared_media_key(
        &route,
        &source_fingerprint,
        ResourceKind::Video,
        "video/mp4",
        Some("session-a"),
    )
    .expect("session-scoped upload should produce a key");
    let session_b = prepared_media_key(
        &route,
        &source_fingerprint,
        ResourceKind::Video,
        "video/mp4",
        Some("session-b"),
    )
    .expect("second session-scoped upload should produce a key");
    assert_ne!(session_a, session_b);
}

/// Verifies prepared media cache pruning expires entries and enforces global capacity.
/// 验证媒体准备缓存裁剪会清除过期项并强制全局容量上限。
#[test]
fn prepared_media_cache_pruning_is_bounded() {
    // now is the deterministic pruning boundary shared by every fixture entry.
    // now 是全部测试条目共享的确定性裁剪边界。
    let now = Instant::now();
    // cache intentionally exceeds capacity and includes one already expired entry.
    // cache 故意超过容量，并包含一条已经过期的记录。
    let mut cache = BTreeMap::new();
    for index in 0..(PREPARED_MEDIA_CACHE_MAX_ENTRIES + 8) {
        // created_at gives every entry a strict oldest-first order.
        // created_at 为每条记录提供严格的最旧优先顺序。
        let created_at = now + Duration::from_millis(index as u64 + 1);
        cache.insert(
            format!("key-{index:04}"),
            PreparedMediaCacheEntry {
                provider_reference: format!("https://example.invalid/{index}"),
                reference_encoding: crate::UploadReferenceEncoding::ResponseUrl,
                expires_at: Some(now + Duration::from_secs(60)),
                created_at,
                created_at_unix_millis: 1,
                expires_at_unix_millis: 60_001,
                source_fingerprint: PreparedMediaSourceFingerprint {
                    byte_len: 1,
                    modified_at: None,
                    content_sha256: [0; 32],
                },
            },
        );
    }
    cache.insert(
        String::from("expired"),
        PreparedMediaCacheEntry {
            provider_reference: String::from("https://example.invalid/expired"),
            reference_encoding: crate::UploadReferenceEncoding::ResponseUrl,
            expires_at: Some(now),
            created_at: now,
            created_at_unix_millis: 1,
            expires_at_unix_millis: 1,
            source_fingerprint: PreparedMediaSourceFingerprint {
                byte_len: 1,
                modified_at: None,
                content_sha256: [u8::MAX; 32],
            },
        },
    );
    prune_prepared_media_cache(&mut cache, now);
    assert_eq!(cache.len(), PREPARED_MEDIA_CACHE_MAX_ENTRIES);
    assert!(!cache.contains_key("expired"));
    assert!(!cache.contains_key("key-0000"));
    assert!(cache.contains_key(&format!("key-{:04}", PREPARED_MEDIA_CACHE_MAX_ENTRIES + 7)));
}

/// Verifies a durable conversation reference survives monotonic cache expiry checks.
/// 验证持久会话引用可以穿过单调时钟缓存过期检查继续保留。
#[test]
fn prepared_media_cache_keeps_durable_conversation_reference() {
    let now = Instant::now();
    let mut cache = BTreeMap::new();
    cache.insert(
        String::from("durable"),
        PreparedMediaCacheEntry {
            provider_reference: String::from("mm_file://durable"),
            reference_encoding: crate::UploadReferenceEncoding::Template,
            expires_at: None,
            created_at: now,
            created_at_unix_millis: 1,
            expires_at_unix_millis: 2,
            source_fingerprint: PreparedMediaSourceFingerprint {
                byte_len: 1,
                modified_at: None,
                content_sha256: [0; 32],
            },
        },
    );
    prune_prepared_media_cache(&mut cache, now + Duration::from_secs(86_400));
    assert!(cache.contains_key("durable"));
}

/// Verifies model-extension references expire from the first use after one day or provider TTL.
/// 验证模型扩展引用从首次使用起一天后或供应商 TTL 到期后失效。
#[test]
fn provider_tool_reference_reuse_uses_first_use_and_provider_expiry() {
    let reference = PreparedMediaReferenceSnapshot {
        provider_reference: String::from("mm_file://video-1"),
        reference_encoding: crate::UploadReferenceEncoding::Template,
        created_at_unix_millis: 1_000,
        expires_at_unix_millis: 1_000 + 10 * PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS,
    };
    assert!(provider_tool_reference_is_reusable(
        &reference,
        1_000 + PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS - 1,
        PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS,
    ));
    assert!(!provider_tool_reference_is_reusable(
        &reference,
        1_000 + PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS,
        PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS,
    ));

    let provider_expired = PreparedMediaReferenceSnapshot {
        expires_at_unix_millis: 1_000 + 500,
        ..reference
    };
    assert!(!provider_tool_reference_is_reusable(
        &provider_expired,
        1_500,
        PROVIDER_TOOL_MEDIA_CACHE_DURATION_MS,
    ));
}

/// Verifies equal-length media replacements receive distinct content-addressed fingerprints.
/// 验证同长度媒体替换会获得不同的内容寻址指纹。
#[tokio::test]
async fn prepared_media_fingerprint_detects_equal_length_content_replacement() {
    // path is unique per test process and never overlaps a user-owned media source.
    // path 在每个测试进程中唯一，绝不会与用户媒体源重叠。
    let path = std::env::temp_dir().join(format!(
        "vulcan-prepared-media-fingerprint-{}.bin",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::write(path.as_path(), b"source-a")
        .await
        .expect("first media fixture should be written");
    let first = prepared_media_source_fingerprint(path.as_path(), None, None)
        .await
        .expect("first media fingerprint should be readable");

    tokio::fs::write(path.as_path(), b"source-b")
        .await
        .expect("replacement media fixture should be written");
    let second = prepared_media_source_fingerprint(path.as_path(), None, None)
        .await
        .expect("replacement media fingerprint should be readable");

    assert_eq!(first.byte_len, second.byte_len);
    assert_ne!(first.content_sha256, second.content_sha256);
    tokio::fs::remove_file(path.as_path())
        .await
        .expect("media fixture should be removed");
}

/// Verifies an inline preparation read retains exactly the bytes covered by its fingerprint.
/// 验证内联准备读取精确保留其指纹覆盖的字节。
#[tokio::test]
async fn prepared_inline_media_read_binds_retained_bytes_to_fingerprint() {
    // path is unique per test process and never overlaps a user-owned media source.
    // path 在每个测试进程中唯一，绝不会与用户媒体源重叠。
    let path = std::env::temp_dir().join(format!(
        "vulcan-prepared-inline-media-{}.bin",
        uuid::Uuid::new_v4()
    ));
    // expectedBytes is the exact payload that must survive the stable streamed read.
    // expectedBytes 是必须通过稳定流式读取保留的精确载荷。
    let expected_bytes = b"request-owned-inline-media";
    tokio::fs::write(path.as_path(), expected_bytes)
        .await
        .expect("inline media fixture should be written");

    // prepared contains both the retained request bytes and their content-addressed identity.
    // prepared 同时包含请求保留字节及其内容寻址身份。
    let prepared = read_prepared_media_source(path.as_path(), None, None, true)
        .await
        .expect("inline media should be read once into request ownership");

    assert_eq!(
        prepared.retained_bytes.as_deref(),
        Some(expected_bytes.as_slice())
    );
    assert_eq!(
        prepared.fingerprint.content_sha256,
        <[u8; 32]>::from(Sha256::digest(expected_bytes))
    );
    tokio::fs::remove_file(path.as_path())
        .await
        .expect("inline media fixture should be removed");
}
