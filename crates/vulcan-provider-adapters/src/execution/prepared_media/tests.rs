//! Regression coverage for frozen upload contracts, stable source bytes, and cache retention.
//! 冻结上传契约、稳定源字节和缓存保留策略的回归覆盖。

use super::super::test_support::bundled_manifest_route;
use super::*;
use std::sync::Arc;
use vulcan_llm_core::{
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
    let mut other_account = route;
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

/// Verifies concurrent callers receive one physical preparation owner and one shared success.
/// 验证并发调用方只产生一个真实准备所有者，并共享同一个成功结果。
#[tokio::test]
async fn prepared_media_flight_has_one_owner_and_shared_terminal_result() {
    // client owns the process-local registry shared by all of its execution clones.
    // client 持有全部执行副本共享的进程内注册表。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(vulcan_llm_core::ProtocolRegistry::first_batch().expect("protocol registry")),
    );
    let route = bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
    let identity = PreparedMediaSourceIdentity {
        byte_len: 42,
        content_sha256: hex::encode([9_u8; 32]),
    };
    // first owns the physical upload; later same-key callers can only follow it.
    // first 持有真实上传；后续同键调用方只能跟随该上传。
    let owner = match client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("first registration")
    {
        PreparedMediaFlightRegistration::Owner(owner) => owner,
        PreparedMediaFlightRegistration::Follower(_) => panic!("first caller must own the flight"),
    };
    let first_follower = match client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("second registration")
    {
        PreparedMediaFlightRegistration::Follower(flight) => flight,
        PreparedMediaFlightRegistration::Owner(_) => panic!("second caller must follow"),
    };
    let second_follower = match client
        .clone()
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("third registration")
    {
        PreparedMediaFlightRegistration::Follower(flight) => flight,
        PreparedMediaFlightRegistration::Owner(_) => panic!("third caller must follow"),
    };
    let expected = PreparedMediaFlightOutcome {
        provider_reference: String::from("ms://shared-file"),
        reference_encoding: crate::UploadReferenceEncoding::Template,
    };
    owner.complete(Ok(expected.clone()));
    let cancellation = MediaUploadCancellation::new();
    assert_eq!(
        first_follower.wait(&cancellation).await,
        Ok(expected.clone())
    );
    assert_eq!(second_follower.wait(&cancellation).await, Ok(expected));
    assert!(matches!(
        client
            .register_prepared_media_flight(
                &route,
                ResourceKind::Video,
                "video/mp4",
                &identity,
                None,
            )
            .expect("terminal flight must release ownership"),
        PreparedMediaFlightRegistration::Owner(_)
    ));
}

/// Verifies an abandoned owner fails current followers and permits a later retry owner.
/// 验证异常退出的所有者会终止当前跟随者，并允许后续重试重新取得所有权。
#[tokio::test]
async fn prepared_media_flight_owner_drop_releases_retry() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(vulcan_llm_core::ProtocolRegistry::first_batch().expect("protocol registry")),
    );
    let route = bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
    let identity = PreparedMediaSourceIdentity {
        byte_len: 24,
        content_sha256: hex::encode([10_u8; 32]),
    };
    let owner = match client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("owner registration")
    {
        PreparedMediaFlightRegistration::Owner(owner) => owner,
        PreparedMediaFlightRegistration::Follower(_) => panic!("first caller must own the flight"),
    };
    let follower = match client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("follower registration")
    {
        PreparedMediaFlightRegistration::Follower(flight) => flight,
        PreparedMediaFlightRegistration::Owner(_) => panic!("second caller must follow"),
    };
    drop(owner);
    let error = follower
        .wait(&MediaUploadCancellation::new())
        .await
        .expect_err("abandoned owner must terminate followers");
    assert_eq!(
        error,
        "Media preparation owner ended before publishing a terminal result."
    );
    assert!(matches!(
        client
            .register_prepared_media_flight(
                &route,
                ResourceKind::Video,
                "video/mp4",
                &identity,
                None,
            )
            .expect("retry registration"),
        PreparedMediaFlightRegistration::Owner(_)
    ));
}

/// Verifies cancelling one follower never cancels or replaces the physical preparation owner.
/// 验证取消一个跟随者绝不会取消或替换真实准备所有者。
#[tokio::test]
async fn prepared_media_flight_follower_cancellation_is_local() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(vulcan_llm_core::ProtocolRegistry::first_batch().expect("protocol registry")),
    );
    let route = bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
    let identity = PreparedMediaSourceIdentity {
        byte_len: 12,
        content_sha256: hex::encode([11_u8; 32]),
    };
    let owner = match client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("owner registration")
    {
        PreparedMediaFlightRegistration::Owner(owner) => owner,
        PreparedMediaFlightRegistration::Follower(_) => panic!("first caller must own the flight"),
    };
    let cancelled_follower = match client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("cancelled follower registration")
    {
        PreparedMediaFlightRegistration::Follower(flight) => flight,
        PreparedMediaFlightRegistration::Owner(_) => panic!("second caller must follow"),
    };
    let cancellation = MediaUploadCancellation::new();
    cancellation.cancel();
    assert_eq!(
        cancelled_follower.wait(&cancellation).await,
        Err(String::from("media upload cancelled"))
    );
    let active_follower = match client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("active follower registration")
    {
        PreparedMediaFlightRegistration::Follower(flight) => flight,
        PreparedMediaFlightRegistration::Owner(_) => {
            panic!("follower cancellation must not replace the owner")
        }
    };
    let expected = PreparedMediaFlightOutcome {
        provider_reference: String::from("ms://still-shared"),
        reference_encoding: crate::UploadReferenceEncoding::Template,
    };
    owner.complete(Ok(expected.clone()));
    assert_eq!(
        active_follower.wait(&MediaUploadCancellation::new()).await,
        Ok(expected)
    );
}

/// Verifies in-flight failures never cross credential connections even when successful references may.
/// 验证即使成功引用允许跨凭据复用，进行中失败也绝不会跨凭据连接传播。
#[test]
fn prepared_media_flight_isolates_exact_credential_connections() {
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(vulcan_llm_core::ProtocolRegistry::first_batch().expect("protocol registry")),
    );
    let route = bundled_manifest_route("kimi", "coding-plan", ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID);
    let mut other_connection = route.clone();
    other_connection.connection.id = String::from("independent-credential");
    let identity = PreparedMediaSourceIdentity {
        byte_len: 8,
        content_sha256: hex::encode([12_u8; 32]),
    };
    let first = client
        .register_prepared_media_flight(&route, ResourceKind::Video, "video/mp4", &identity, None)
        .expect("first credential registration");
    let second = client
        .register_prepared_media_flight(
            &other_connection,
            ResourceKind::Video,
            "video/mp4",
            &identity,
            None,
        )
        .expect("second credential registration");
    assert!(matches!(first, PreparedMediaFlightRegistration::Owner(_)));
    assert!(matches!(second, PreparedMediaFlightRegistration::Owner(_)));
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
    let prepared = read_prepared_media_source(
        path.as_path(),
        None,
        None,
        true,
        Some(expected_bytes.len() as u64),
    )
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

/// Verifies changed preflight size is rejected before any bytes are retained or reported as read.
/// 验证预检大小变化时，在保留或报告读取任何字节之前拒绝处理。
#[tokio::test]
async fn prepared_inline_media_read_rejects_growth_after_planning() {
    // The isolated directory owns all fixture bytes and removes them on drop.
    // 隔离目录持有全部夹具字节，并在释放时移除它们。
    let directory = tempfile::tempdir().expect("isolated media directory");
    // A larger physical source must never replace the previously admitted three-byte snapshot.
    // 更大的物理来源绝不能替换此前准入的三字节快照。
    let path = directory.path().join("grown.mp4");
    tokio::fs::write(&path, b"four")
        .await
        .expect("grown source");
    // No progress callback may be invoked before the changed-size failure.
    // 大小变化失败之前不得调用任何进度回调。
    let progress: MediaUploadProgressCallback =
        Arc::new(|_, _| panic!("read started before size check"));
    let error = read_prepared_media_source(&path, Some(progress), None, true, Some(3))
        .await
        .err()
        .expect("changed preflight size must fail");
    assert!(error.contains("size changed after delivery planning"));
}

/// Verifies exact inline video materialization and a missing planned upload never silently interconvert.
/// 验证精确内联视频物化与缺失的已规划上传绝不会静默互相转换。
#[tokio::test]
async fn prepared_local_media_obeys_planned_video_action_without_upload_fallback() {
    // All dependencies are isolated and this route has no real upload channel.
    // 全部依赖均隔离，此路由没有真实上传通道。
    let directory = tempfile::tempdir().expect("isolated video directory");
    let path = directory.path().join("clip.mp4");
    tokio::fs::write(&path, b"abc")
        .await
        .expect("tiny video bytes");
    let route = bundled_manifest_route("openrouter", "public-api", OPENAI_CHAT_PROFILE_ID);
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(vulcan_llm_core::ProtocolRegistry::first_batch().expect("registered profiles")),
    );
    let secret = crate::CredentialSecretEnvelope::new_api_key(String::from("offline-test-secret"))
        .expect("test credential")
        .execution_secret()
        .expect("execution secret");
    let cancellation = MediaUploadCancellation::new();
    // Capture the authoritative fixture identity before any policy-selected preparation.
    // 在任何策略选择的准备之前捕获权威夹具身份。
    let identity = client
        .read_admitted_media_source_identity(&path, 3, Some(&cancellation))
        .await
        .expect("fixture identity");
    let inline = client
        .prepare_local_media(
            &route,
            "test-model",
            &secret,
            AdmittedLocalMediaSource {
                path: &path,
                file_name: "clip.mp4",
                mime_type: "video/mp4",
                identity: &identity,
            },
            ResourceKind::Video,
            true,
            LocalMediaMaterialization::Inline { source_byte_len: 3 },
            None,
            &cancellation,
        )
        .await
        .expect("planned small inline video");
    assert_eq!(
        inline.prepared_reference(),
        Some("data:video/mp4;base64,YWJj")
    );
    let error = client
        .prepare_local_media(
            &route,
            "test-model",
            &secret,
            AdmittedLocalMediaSource {
                path: &path,
                file_name: "clip.mp4",
                mime_type: "video/mp4",
                identity: &identity,
            },
            ResourceKind::Video,
            true,
            LocalMediaMaterialization::ProviderUpload { source_byte_len: 3 },
            None,
            &cancellation,
        )
        .await
        .expect_err("missing upload cannot fall back");
    assert!(error.contains("inline fallback is forbidden"));
    // Replacing bytes without changing length must not redefine this attachment's original identity.
    // 替换字节但不改变长度，不能重新定义本附件的原始身份。
    tokio::fs::write(&path, b"xyz")
        .await
        .expect("replace exact fixture bytes");
    // Admission retains the previous hash even when new bytes would otherwise fit all soft limits.
    // 即使新字节仍满足全部软限制，准入仍保留原摘要。
    let changed = client
        .prepare_local_media_typed(
            &route,
            "test-model",
            &secret,
            AdmittedLocalMediaSource {
                path: &path,
                file_name: "clip.mp4",
                mime_type: "video/mp4",
                identity: &identity,
            },
            ResourceKind::Video,
            true,
            LocalMediaMaterialization::Inline { source_byte_len: 3 },
            None,
            &cancellation,
        )
        .await
        .expect_err("same-size content drift");
    assert_eq!(changed, MediaUploadFailure::SourceChanged);
}

/// Verifies the public admitted-identity reader rejects size drift and cancellation instead of hashing new source bytes.
/// 验证公开已准入身份读取器拒绝大小漂移和取消，而不是哈希新的来源字节。
#[tokio::test]
async fn admitted_source_identity_keeps_policy_size_and_cancellation() {
    // This test owns its source and does not contact any provider.
    // 本测试持有自己的来源，不联系任何供应商。
    let directory = tempfile::tempdir().expect("isolated admitted source directory");
    let path = directory.path().join("source.bin");
    tokio::fs::write(&path, b"abc").await.expect("write source");
    // The ordinary execution client exercises the public production identity entry point.
    // 普通执行客户端用于检验公开生产身份入口。
    let client = ProviderExecutionClient::new(
        reqwest::Client::new(),
        Arc::new(vulcan_llm_core::ProtocolRegistry::first_batch().expect("protocol registry")),
    );
    let identity = client
        .read_admitted_media_source_identity(&path, 3, None)
        .await
        .expect("admitted bytes");
    assert_eq!(identity.byte_len, 3);
    assert_eq!(identity.content_sha256, hex::encode(Sha256::digest(b"abc")));
    tokio::fs::write(&path, b"four").await.expect("grow source");
    assert!(
        client
            .read_admitted_media_source_identity(&path, 3, None)
            .await
            .expect_err("size drift")
            .contains("size changed after delivery planning")
    );
    // Cancellation remains effective even if a new caller supplies the current physical size.
    // 即使新调用方提供当前物理大小，取消仍然生效。
    let cancellation = MediaUploadCancellation::new();
    cancellation.cancel();
    assert!(
        client
            .read_admitted_media_source_identity(&path, 4, Some(&cancellation))
            .await
            .is_err()
    );
}

/// Pre-cancelled preparation never polls a source operation, even when that operation is immediately ready.
/// 预先取消的准备绝不轮询来源操作，即使该操作立即就绪。
#[tokio::test]
async fn prepared_source_io_precancelled_does_not_poll() {
    // The flag observes actual operation polling, not a synthetic progress callback.
    // 标志观察实际操作轮询，而非合成进度回调。
    let polled = std::sync::atomic::AtomicBool::new(false);
    // Cancellation belongs to the same source operation.
    // 取消属于同一来源操作。
    let cancellation = MediaUploadCancellation::new();
    cancellation.cancel();
    // This ready operation must never run after pre-cancellation.
    // 预先取消后，此就绪操作绝不能运行。
    let result = await_prepared_source_io(
        Some(&cancellation),
        async {
            polled.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(7_u8)
        },
        "Source read failed",
    )
    .await;
    assert_eq!(result, Err(String::from("media upload cancelled")));
    assert!(!polled.load(std::sync::atomic::Ordering::SeqCst));
}

/// Cancellation interrupts an already pending source operation without waiting for the filesystem future to finish.
/// 取消中断已经等待中的来源操作，不等待文件系统Future完成。
#[tokio::test]
async fn prepared_source_io_cancels_while_operation_is_pending() {
    // The start signal makes cancellation deterministic without sleeping.
    // 启动信号使取消确定可测，不使用休眠。
    let (started, ready) = tokio::sync::oneshot::channel();
    // The parent owns cancellation while the task borrows its owned clone for the exact operation.
    // 父级持有取消，任务为精确操作借用自身拥有的克隆。
    let cancellation = MediaUploadCancellation::new();
    // Preserve the exact signal across task ownership transfer.
    // 跨任务所有权移交保留精确信号。
    let owned = cancellation.clone();
    // A permanently pending operation models an unresponsive source read without touching external storage.
    // 永久等待操作模拟无响应来源读取，不触碰外部存储。
    let task = tokio::spawn(async move {
        await_prepared_source_io(
            Some(&owned),
            async {
                let _ = started.send(());
                std::future::pending::<std::io::Result<()>>().await
            },
            "Source read failed",
        )
        .await
    });
    ready.await.expect("operation started");
    cancellation.cancel();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), task)
            .await
            .expect("prompt cancellation")
            .expect("source task"),
        Err(String::from("media upload cancelled"))
    );
}

/// Same-poll cancellation beats a successful source result and prevents any returned bytes from being used.
/// 同次轮询取消优先于成功来源结果，防止任何返回字节被使用。
#[tokio::test]
async fn prepared_source_io_same_poll_cancellation_wins() {
    // Simulate a completion callback cancelling the parent before its operation returns.
    // 模拟完成回调在操作返回前取消父级。
    let cancellation = MediaUploadCancellation::new();
    // The operation is ready, but its successful value has no authority after cancellation.
    // 操作已经就绪，但取消后其成功值不再具有使用权限。
    let result = await_prepared_source_io(
        Some(&cancellation),
        async {
            cancellation.cancel();
            Ok(7_u8)
        },
        "Source read failed",
    )
    .await;
    assert_eq!(result, Err(String::from("media upload cancelled")));
}

/// The declared tokenless cache branch remains usable and source diagnostics never echo private I/O error strings.
/// 声明的无令牌缓存分支保持可用，来源诊断绝不回显私有IO错误字符串。
#[tokio::test]
async fn prepared_source_io_preserves_optional_reads_and_safe_errors() {
    assert_eq!(
        await_prepared_source_io(None, async { Ok(7_u8) }, "Source read failed").await,
        Ok(7)
    );
    // This artificial OS diagnostic must stay outside the public error boundary.
    // 此人为操作系统诊断必须保持在公开错误边界之外。
    let result = await_prepared_source_io::<()>(
        None,
        async { Err(std::io::Error::other("private-source-path-and-secret")) },
        "Source read failed",
    )
    .await;
    assert_eq!(result, Err(String::from("Source read failed.")));
}
