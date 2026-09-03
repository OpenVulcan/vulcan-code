//! Production operation-policy regressions without live provider traffic.
//! 不产生真实供应商流量的生产操作策略回归。

use super::*;
use vulcan_ai_contract::ResourceKind;

/// Materializes exact host sources through the production engine without credentials, network, or workspace path inference.
/// 通过生产引擎物化精确宿主来源，不使用凭据、网络或工作区路径推断。
async fn materialize_host_sources(
    sources: &[crate::ProviderToolMediaSource],
    arguments: serde_json::Value,
    policy: crate::MediaDeliveryPolicy,
) -> Result<serde_json::Value, String> {
    materialize_operation_uploads(
        OperationUploadExecutionContext {
            media_sources: sources,
            media_delivery_policy: policy,
            http: &reqwest::Client::new(),
            entry: &dummy_entry(),
            secret: &dummy_secret(),
            credential_profile: &dummy_credential_profile(),
            channels: &[channel(
                UploadPrimitiveKind::InlineDataUrl,
                vec![String::from("image/png")],
                None,
                None,
            )],
            progress: None,
            cancellation: &MediaUploadCancellation::new(),
            model_id: None,
            workspace_root: None,
            fixture_loopback_origin: None,
        },
        &image_edit_operation(),
        arguments,
    )
    .await
}

/// Materializes one direct local image path through the production operation engine.
/// 通过生产操作引擎物化一条直接本地图片路径。
async fn materialize_direct_local_source(
    workspace_root: Option<&Path>,
    reference: &str,
) -> Result<serde_json::Value, String> {
    materialize_operation_uploads(
        OperationUploadExecutionContext {
            media_sources: &[],
            media_delivery_policy: crate::MediaDeliveryPolicy::default(),
            http: &reqwest::Client::new(),
            entry: &dummy_entry(),
            secret: &dummy_secret(),
            credential_profile: &dummy_credential_profile(),
            channels: &[channel(
                UploadPrimitiveKind::InlineDataUrl,
                vec![String::from("image/png")],
                None,
                None,
            )],
            progress: None,
            cancellation: &MediaUploadCancellation::new(),
            model_id: None,
            workspace_root,
            fixture_loopback_origin: None,
        },
        &image_edit_operation(),
        serde_json::json!({"source_paths": reference, "prompt": "describe"}),
    )
    .await
}

/// Builds a host proof for exact fixture `path`, `reference` and `bytes`, preserving the original image name.
/// 为精确夹具`path`、`reference`和`bytes`构建宿主证据，保留原始图片名称。
fn host_source(path: &Path, reference: &str, bytes: &[u8]) -> crate::ProviderToolMediaSource {
    crate::ProviderToolMediaSource {
        reference: reference.into(),
        local_path: path.into(),
        file_name: "original.png".into(),
        mime_type: "image/png".into(),
        source_identity: crate::PreparedMediaSourceIdentity {
            byte_len: bytes.len() as u64,
            content_sha256: hex::encode(Sha256::digest(bytes)),
        },
    }
}

/// Direct extension inputs accept global paths and replace them before provider dispatch.
/// 直接扩展输入接受全局路径，并在供应商分派前完成替换。
#[tokio::test]
async fn operation_direct_paths_accept_external_files_without_leaking_host_paths()
-> Result<(), Box<dyn std::error::Error>> {
    // The file is deliberately outside the active workspace directory.
    // 文件被有意放置在活动工作区目录之外。
    let directory = tempfile::tempdir()?;
    let workspace = directory.path().join("project");
    let image = directory.path().join("outside.png");
    tokio::fs::create_dir(&workspace).await?;
    tokio::fs::write(&image, b"abc").await?;
    let expected = serde_json::json!({
        "source_images": "data:image/png;base64,YWJj",
        "prompt": "describe"
    });

    // Relative parent traversal and a workspace-free absolute path must reach the same wire-safe result.
    // 相对父目录跳转与无需工作区的绝对路径必须得到相同的 Wire 安全结果。
    assert_eq!(
        materialize_direct_local_source(Some(&workspace), "../outside.png").await?,
        expected
    );
    assert_eq!(
        materialize_direct_local_source(None, image.to_string_lossy().as_ref()).await?,
        expected
    );
    Ok(())
}

/// Preserves MIME and exact source bytes for extensionless project entities while keeping private metadata off the wire.
/// 为无扩展名项目实体保留MIME及精确来源字节，同时避免私有元数据进入Wire。
#[tokio::test]
async fn operation_host_source_uses_catalog_mime_and_safe_locator()
-> Result<(), Box<dyn std::error::Error>> {
    // Only a temporary extensionless entity is read by this test.
    // 本测试只读取临时无扩展名实体。
    let directory = tempfile::tempdir()?;
    // The digest-shaped name intentionally cannot supply a MIME through extension inference.
    // 摘要形态名称故意无法通过扩展名推断提供MIME。
    let path = directory.path().join("0123456789abcdef");
    tokio::fs::write(&path, b"abc").await?;
    // An exact host-owned proof is supplied independently of the tool's argument JSON.
    // 精确宿主持有证据独立于工具参数JSON提供。
    let source = host_source(&path, "project-resource://1", b"abc");
    // The wire result must contain only native content, without private paths or the unresolved locator.
    // Wire结果只能包含原生内容，不得包含私有路径或未解析定位符。
    let result = materialize_host_sources(
        &[source],
        serde_json::json!({"source_paths": "project-resource://1", "prompt": "describe"}),
        crate::MediaDeliveryPolicy::default(),
    )
    .await?;
    assert_eq!(
        result,
        serde_json::json!({"source_images":"data:image/png;base64,YWJj", "prompt":"describe"})
    );
    Ok(())
}

/// Host metadata and Data URLs cannot bypass the binding's declared MIME mapping.
/// 宿主元数据及Data URL不能绕过绑定声明的MIME映射。
#[tokio::test]
async fn operation_authoritative_mime_respects_binding_formats()
-> Result<(), Box<dyn std::error::Error>> {
    // The nonexistent path proves this MIME guard runs before content reads.
    // 不存在的路径证明本MIME守卫先于内容读取。
    let directory = tempfile::tempdir()?;
    // This image binding declares only PNG/JPEG, even when a wider channel could accept audio.
    // 本图片绑定仅声明PNG/JPEG，即使更宽通道能接受音频。
    let mut binding = image_edit_operation().upload_bindings.remove(0);
    // Catalog MIME stays authoritative; a misleading filename cannot relabel the source.
    // 目录MIME保持权威；误导性文件名不能重新标记来源。
    let mut source = host_source(
        &directory.path().join("not-read"),
        "project-resource://1",
        b"abc",
    );
    source.mime_type = "audio/wav".into();
    assert!(
        prepare_operation_upload_resource(
            &binding,
            &source.reference,
            None,
            std::slice::from_ref(&source)
        )
        .await
        .err()
        .expect("host MIME must fail")
        .contains("does not match")
    );
    assert!(
        prepare_operation_upload_resource(&binding, "data:audio/wav;base64,YWJj", None, &[])
            .await
            .err()
            .expect("Data URL MIME must fail")
            .contains("does not match")
    );
    // A fixed MIME explicitly overrides the extension map, exactly as existing path inference does.
    // 固定MIME显式覆盖扩展名映射，与既有路径推断完全一致。
    binding.mime_type = Some("audio/wav".into());
    assert_eq!(
        prepare_operation_upload_resource(&binding, &source.reference, None, &[source.clone()])
            .await?
            .resource
            .mime_type,
        "audio/wav"
    );
    Ok(())
}

/// Verifies an HTTPS URL with a mixed-case scheme stays on the remote-source branch.
/// 验证 scheme 大小写混合的 HTTPS URL 仍进入远端来源分支。
#[tokio::test]
async fn operation_remote_https_scheme_is_case_insensitive()
-> Result<(), Box<dyn std::error::Error>> {
    // binding supplies the exact image MIME contract used to classify the URL path.
    // binding 提供用于分类 URL 路径的精确图片 MIME 契约。
    let binding = image_edit_operation().upload_bindings.remove(0);
    // reference deliberately uses a non-canonical scheme spelling accepted by URL syntax.
    // reference 故意使用 URL 语法允许的非规范 scheme 拼写。
    let reference = "HTTPS://media.example.com/source.png";
    // source must retain the URL and must never materialize a local path.
    // source 必须保留 URL，且绝不能物化本地路径。
    let source = prepare_operation_upload_resource(&binding, reference, None, &[]).await?;

    assert_eq!(source.resource.remote_url.as_deref(), Some(reference));
    assert!(source.resource.local_path.is_none());
    assert_eq!(source.resource.mime_type, "image/png");
    Ok(())
}

/// Rejects stale or mismatched host proofs before dispatch, and applies independent policy before reading the source.
/// 在分派前拒绝陈旧或不匹配宿主证据，并在读取来源前应用独立策略。
#[tokio::test]
async fn operation_host_source_rejects_corruption_and_unconsumed_proof()
-> Result<(), Box<dyn std::error::Error>> {
    // A temporary source is mutated only inside this isolated fixture.
    // 临时来源只在本隔离夹具内修改。
    let directory = tempfile::tempdir()?;
    // Preserve a filename without an extension to exercise authoritative metadata throughout.
    // 保留无扩展名文件名，以全程验证权威元数据。
    let path = directory.path().join("entity");
    tokio::fs::write(&path, b"abc").await?;
    // The first identity remains fixed when later bytes change.
    // 后续字节改变时第一份身份保持固定。
    let source = host_source(&path, "project-resource://1", b"abc");
    // The AI argument names exactly the one host-resolved locator.
    // AI参数精确指定一个宿主已解析定位符。
    let arguments = serde_json::json!({"source_paths":"project-resource://1"});
    tokio::fs::write(&path, b"xyz").await?;
    assert!(
        materialize_host_sources(
            std::slice::from_ref(&source),
            arguments.clone(),
            crate::MediaDeliveryPolicy::default()
        )
        .await
        .unwrap_err()
        .contains("changed after registration")
    );
    assert!(
        materialize_host_sources(
            &[],
            arguments.clone(),
            crate::MediaDeliveryPolicy::default()
        )
        .await
        .unwrap_err()
        .contains("no host-owned source")
    );
    assert!(
        materialize_host_sources(
            &[source.clone(), source.clone()],
            arguments.clone(),
            crate::MediaDeliveryPolicy::default()
        )
        .await
        .unwrap_err()
        .contains("ambiguous")
    );
    assert!(
        materialize_host_sources(
            std::slice::from_ref(&source),
            serde_json::json!({"source_paths":"data:image/png;base64,YWJj"}),
            crate::MediaDeliveryPolicy::default()
        )
        .await
        .unwrap_err()
        .contains("not consumed")
    );
    // A disabled extension inline branch must fail before attempting to open a missing entity.
    // 扩展内联分支禁用时，必须在尝试打开缺失实体之前失败。
    let mut policy = crate::MediaDeliveryPolicy::default();
    policy.independent_extension.image.without_upload_max_bytes = 0;
    tokio::fs::remove_file(&path).await?;
    assert!(
        materialize_host_sources(&[source], arguments, policy)
            .await
            .unwrap_err()
            .contains("Media extension cannot")
    );
    Ok(())
}

/// Builds a fresh independent planner from `policy`; fails the test on invalid fixture settings.
/// 根据 `policy` 构建新的独立规划器；夹具设置无效时令测试失败。
fn planner(policy: crate::MediaDeliveryPolicy) -> crate::MediaDeliveryPlanner {
    crate::MediaDeliveryPlanner::new(policy, crate::MediaDeliveryPurpose::IndependentExtension)
        .expect("valid independent policy")
}

/// Verifies inline declarations do not hide a real channel or permit evasion of its hard limit.
/// 验证内联声明不能隐藏真实通道，也不能规避其硬限制。
#[test]
fn operation_policy_selects_real_upload_before_inline_and_never_evades_its_limit() {
    // Both declarations share an exact input binding and therefore have declared consumer shapes.
    // 两条声明共用精确输入绑定，因此均具有已声明的消费形态。
    let inline = channel(
        UploadPrimitiveKind::InlineDataUrl,
        vec![String::from("video/mp4")],
        None,
        None,
    );
    let mut real = channel(
        UploadPrimitiveKind::MultipartFile,
        vec![String::from("video/mp4")],
        Some(8),
        None,
    );
    real.upload_channel_id = String::from("real");
    let channels = [inline, real];
    let references = [String::from("test.channel"), String::from("real")];
    let source = resource("video/mp4", 4);
    let selected = resolve_operation_delivery_channel(
        &channels,
        &references,
        &source,
        None,
        &mut planner(crate::MediaDeliveryPolicy::default()),
        &[ResourceKind::Video],
    )
    .expect("real default");
    assert_eq!(selected.upload_channel_id, "real");
    // Positive preference can select the declared inline representation only below the hard interface ceiling.
    // 正数偏好仅能在接口硬上限以内选择已声明的内联表示。
    let mut policy = crate::MediaDeliveryPolicy::default();
    policy.independent_extension.video.with_upload_max_bytes = 16;
    let selected = resolve_operation_delivery_channel(
        &channels,
        &references,
        &source,
        None,
        &mut planner(policy),
        &[ResourceKind::Video],
    )
    .expect("small inline preference");
    assert_eq!(selected.upload_channel_id, "test.channel");
    let error = resolve_operation_delivery_channel(
        &channels,
        &references,
        &resource("video/mp4", 9),
        None,
        &mut planner(policy),
        &[ResourceKind::Video],
    )
    .expect_err("selected real hard limit");
    assert!(error.contains("too large for the selected upload interface"));
    // A larger second real channel does not change the ordered selection's original hard limit.
    // 更大的第二真实通道不能改变按顺序选中接口的原始硬限制。
    let mut larger = channels[1].clone();
    larger.upload_channel_id = String::from("larger");
    larger.max_bytes = Some(1024);
    let more_channels = [channels[0].clone(), channels[1].clone(), larger];
    let more_references = [
        String::from("test.channel"),
        String::from("real"),
        String::from("larger"),
    ];
    assert!(
        resolve_operation_delivery_channel(
            &more_channels,
            &more_references,
            &resource("video/mp4", 9),
            None,
            &mut planner(policy),
            &[ResourceKind::Video]
        )
        .is_err()
    );
}

/// Verifies independent modality thresholds and aggregate reservations stay separate from conversation policy.
/// 验证独立模态阈值与累计预留量独立于主会话策略。
#[test]
fn operation_policy_uses_independent_kind_and_aggregate_limits() {
    // One inline-only declaration is no real upload regardless of its upload-channel name.
    // 无论上传通道如何命名，仅内联声明都不是真实上传。
    let channels = [channel(
        UploadPrimitiveKind::InlineBase64,
        Vec::new(),
        None,
        None,
    )];
    let references = [String::from("test.channel")];
    let mut policy = crate::MediaDeliveryPolicy::default();
    policy.conversation.image.without_upload_max_bytes = 0;
    policy.independent_extension.image.without_upload_max_bytes = 3;
    policy.independent_extension.video.without_upload_max_bytes = 1;
    policy.independent_extension.file.without_upload_max_bytes = 0;
    policy.independent_extension.max_total_encoded_bytes = 4;
    let mut request = planner(policy);
    assert!(
        resolve_operation_delivery_channel(
            &channels,
            &references,
            &resource("image/png", 3),
            None,
            &mut request,
            &[ResourceKind::Image]
        )
        .is_ok()
    );
    assert_eq!(request.encoded_bytes_reserved(), 4);
    assert!(
        resolve_operation_delivery_channel(
            &channels,
            &references,
            &resource("image/png", 1),
            None,
            &mut request,
            &[ResourceKind::Image]
        )
        .expect_err("combined budget")
        .contains("combined encoded files")
    );
    assert!(
        resolve_operation_delivery_channel(
            &channels,
            &references,
            &resource("video/mp4", 2),
            None,
            &mut planner(policy),
            &[ResourceKind::Video]
        )
        .expect_err("video independent threshold")
        .contains("inline size limit")
    );
    assert!(
        resolve_operation_delivery_channel(
            &channels,
            &references,
            &resource("application/pdf", 1),
            None,
            &mut planner(policy),
            &[ResourceKind::File]
        )
        .expect_err("file inline disabled")
        .contains("inline delivery is disabled")
    );
    assert!(
        resolve_operation_delivery_channel(
            &channels,
            &references,
            &resource("video/mp4", 1),
            None,
            &mut planner(policy),
            &[ResourceKind::Image]
        )
        .expect_err("operation modality contract")
        .contains("modality is not declared")
    );
}

/// Verifies a broken or unauthorized real upload cannot be reinterpreted as an inline-only operation.
/// 验证损坏或未授权真实上传不能被重新解释成仅内联操作。
#[test]
fn operation_policy_rejects_invalid_real_upload_before_inline() {
    let inline = channel(UploadPrimitiveKind::InlineBase64, Vec::new(), None, None);
    let mut real = channel(UploadPrimitiveKind::MultipartFile, Vec::new(), None, None);
    real.upload_channel_id = String::from("real");
    real.credential_role = Some(String::from("upload"));
    let references = [String::from("test.channel"), String::from("real")];
    assert!(
        resolve_operation_delivery_channel(
            &[inline.clone(), real.clone()],
            &references,
            &resource("image/png", 1),
            None,
            &mut planner(crate::MediaDeliveryPolicy::default()),
            &[ResourceKind::Image]
        )
        .expect_err("denied real upload")
        .contains("not authorized")
    );
    real.credential_role = None;
    real.primitive = Some(UploadPrimitiveKind::PresignedFormUpload);
    assert!(
        resolve_operation_delivery_channel(
            &[inline, real],
            &references,
            &resource("image/png", 1),
            None,
            &mut planner(crate::MediaDeliveryPolicy::default()),
            &[ResourceKind::Image]
        )
        .expect_err("unregistered real primitive")
        .contains("no registered executable primitive")
    );
}

/// Verifies deferred Base64 validation uses strict canonical decoding without retaining a full payload.
/// 验证延迟 Base64 校验使用严格规范解码，不保留完整负载。
#[test]
fn operation_base64_preflight_rejects_invalid_tail_and_padding() {
    assert!(validate_operation_base64_payload("YWJj", 3).is_ok());
    assert!(validate_operation_base64_payload("YR==", 1).is_err());
    assert!(validate_operation_base64_payload("YW=j", 2).is_err());
    assert!(validate_operation_base64_payload("====", 1).is_err());
    // Premature padding exactly at a chunk boundary cannot bypass whole-input canonical validation.
    // 精确落在分块边界的过早填充不能绕过完整输入规范校验。
    let padded_chunk = format!("{}YQ==YWJj", "YWJj".repeat(4095));
    assert!(validate_operation_base64_payload(&padded_chunk, 12 * 1024 + 3).is_err());
    let valid_large = "YWJj".repeat(4097);
    assert!(validate_operation_base64_payload(&valid_large, 4097 * 3).is_ok());
}

/// Verifies all deferred payloads are validated before the first real upload can start.
/// 验证所有延迟负载都在首个真实上传能够开始之前完成校验。
#[tokio::test]
async fn operation_policy_rejects_later_bad_base64_before_network() {
    let channel = channel(
        UploadPrimitiveKind::MultipartFile,
        vec![String::from("image/png")],
        Some(1024),
        Some(2),
    );
    let error = materialize_operation_uploads(
        OperationUploadExecutionContext {
            media_sources: &[],
            media_delivery_policy: crate::MediaDeliveryPolicy::default(),
            http: &reqwest::Client::new(),
            entry: &dummy_entry(), secret: &dummy_secret(), credential_profile: &dummy_credential_profile(),
            channels: &[channel], progress: None, cancellation: &MediaUploadCancellation::new(),
            model_id: None, workspace_root: None, fixture_loopback_origin: None,
        },
        &image_edit_operation(),
        serde_json::json!({"source_paths": ["data:image/png;base64,YWJj", "data:image/png;base64,!!!!"]}),
    ).await.expect_err("all inputs must validate before network");
    assert!(error.contains("base64 is invalid"), "{error}");
}

/// Verifies a small video is materialized by the declared inline primitive without upload progress.
/// 验证小视频由已声明内联原语物化，且不会发出上传进度。
#[tokio::test]
async fn operation_policy_materializes_small_video_without_fake_upload_progress() {
    let channel = channel(
        UploadPrimitiveKind::InlineDataUrl,
        vec![String::from("video/mp4")],
        Some(1024),
        Some(1),
    );
    let mut operation = image_edit_operation();
    operation.input_resources = vec![ResourceKind::Video];
    operation.upload_bindings[0].mime_type = Some(String::from("video/mp4"));
    let materialized = materialize_operation_uploads(
        OperationUploadExecutionContext {
            media_sources: &[],
            media_delivery_policy: crate::MediaDeliveryPolicy::default(),
            http: &reqwest::Client::new(),
            entry: &dummy_entry(),
            secret: &dummy_secret(),
            credential_profile: &dummy_credential_profile(),
            channels: &[channel],
            progress: Some(std::sync::Arc::new(|_, _| {
                panic!("inline is not upload progress")
            })),
            cancellation: &MediaUploadCancellation::new(),
            model_id: None,
            workspace_root: None,
            fixture_loopback_origin: None,
        },
        &operation,
        serde_json::json!({"source_paths": "data:video/mp4;base64,YWJj"}),
    )
    .await
    .expect("small inline video");
    assert_eq!(materialized["source_images"], "data:video/mp4;base64,YWJj");
}

/// Applies each interface's count to one physical upload while preflighting every binding before dispatch.
/// 将每个接口数量限制应用于一次物理上传，同时在分派前预检全部绑定。
#[tokio::test]
async fn operation_policy_preflights_all_bindings_with_per_upload_count() {
    // A one-file interface can be called independently for each resource; it is not an operation-wide count.
    // 单文件接口可以为每个资源独立调用；它不是整次操作数量限制。
    let channel = channel(
        UploadPrimitiveKind::MultipartFile,
        vec![String::from("image/png")],
        Some(1024),
        Some(1),
    );
    let mut operation = image_edit_operation();
    let mut second_binding = operation.upload_bindings[0].clone();
    second_binding.source_path = String::from("/second_source");
    second_binding.target_path = String::from("/second_image");
    operation.upload_bindings.push(second_binding);
    let error = materialize_operation_uploads(
        OperationUploadExecutionContext {
            media_sources: &[],
            media_delivery_policy: crate::MediaDeliveryPolicy::default(),
            http: &reqwest::Client::new(), entry: &dummy_entry(), secret: &dummy_secret(),
            credential_profile: &dummy_credential_profile(), channels: &[channel],
            progress: None, cancellation: &MediaUploadCancellation::new(),
            model_id: None, workspace_root: None, fixture_loopback_origin: None,
        },
        &operation,
        serde_json::json!({"source_paths":"data:image/png;base64,YWJj", "second_source":"data:image/png;base64,!!!!"}),
    ).await.expect_err("all sources still validate before any upload");
    assert!(error.contains("base64 is invalid"), "{error}");
    // Inline materialization demonstrates successful independent calls without contacting a provider.
    // 内联物化证明独立调用成功，不联系供应商。
    let inline = super::channel(
        UploadPrimitiveKind::InlineDataUrl,
        vec![String::from("image/png")],
        Some(1024),
        Some(1),
    );
    // Two values in one field and another binding are three independent single-file materializations.
    // 一个字段中的两个值和另一个绑定，是三次独立单文件物化。
    let result = materialize_operation_uploads(OperationUploadExecutionContext {
        media_sources: &[], media_delivery_policy: crate::MediaDeliveryPolicy::default(), http: &reqwest::Client::new(), entry: &dummy_entry(), secret: &dummy_secret(),
        credential_profile: &dummy_credential_profile(), channels: &[inline], progress: None, cancellation: &MediaUploadCancellation::new(), model_id: None, workspace_root: None, fixture_loopback_origin: None,
    }, &operation, serde_json::json!({"source_paths":["data:image/png;base64,YWJj", "data:image/png;base64,YWJj"], "second_source":"data:image/png;base64,YWJj"})).await.expect("per-upload item count");
    assert_eq!(
        result["source_images"]
            .as_array()
            .expect("original array shape")
            .len(),
        2
    );
    assert_eq!(result["second_image"], "data:image/png;base64,YWJj");
}
