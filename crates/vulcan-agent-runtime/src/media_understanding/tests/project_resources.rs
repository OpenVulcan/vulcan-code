//! Host-resource bridge tests preserving safe locators and immutable metadata.
//! 保留安全定位符及不可变元数据的宿主资源桥测试。

use super::*;
use crate::media_resource_runtime::MediaResourceRuntime;
use std::sync::atomic::{AtomicUsize, Ordering};
use vulcan_core::types::ConversationAttachment;
use vulcan_session_core::v1::ResourceId;

/// A strict metadata-only fixture that verifies the invoking owner and resource occurrence.
/// 验证发起所有者及资源出现的严格仅元数据夹具。
#[derive(Debug)]
struct ProjectResolver {
    /// Count of actual host lookups, excluding malformed or ownerless invocations.
    /// 真实宿主查询次数，不包括损坏或缺失所有者的调用。
    calls: AtomicUsize,
    /// Exact fixture entity, deliberately without an extension.
    /// 精确夹具实体，故意不带扩展名。
    path: PathBuf,
}

#[async_trait::async_trait]
impl MediaResourceRuntime for ProjectResolver {
    /// Returns registered image metadata only for the exact fixture owner and id.
    /// 仅为精确夹具所有者和ID返回已注册图片元数据。
    async fn resolve_project_resource(
        &self,
        session_id: &str,
        resource_id: ResourceId,
    ) -> Result<ConversationAttachment, String> {
        assert_eq!(session_id, "invoking-session");
        assert_eq!(resource_id.value(), 7);
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ConversationAttachment {
            kind: ConversationAttachmentKind::Image,
            file_name: "original.png".into(),
            local_path: self.path.to_string_lossy().into_owned(),
            resource_path: Some("project-resource://7".into()),
            source_path: None,
            asset_id: None,
            byte_len: Some(3),
            source_sha256: Some("a".repeat(64)),
            delivery_mode: None,
            preview_local_path: None,
            mime_type: "image/png".into(),
        })
    }
}

/// Keeps only the disclosed locator in argument JSON while passing all original source facts on the host-only channel.
/// 参数JSON中只保留已披露定位符，同时通过仅宿主通道传递全部原始来源事实。
#[tokio::test]
async fn project_media_arguments_keep_private_metadata_outside_json()
-> Result<(), Box<dyn std::error::Error>> {
    // This temporary directory supplies an absolute fixture path without reading or writing media bytes.
    // 本临时目录提供绝对夹具路径，不读取或写入媒体字节。
    let directory = tempfile::tempdir()?;
    // The host lookup owns MIME and filename, even though the entity basename has neither.
    // 尽管实体基础名不含相关信息，宿主查询仍持有MIME和文件名。
    let resolver = ProjectResolver {
        calls: AtomicUsize::new(0),
        path: directory.path().join("opaque-digest"),
    };
    // Input whitespace is normalized exactly once without changing the media prompt.
    // 输入空白只规范化一次，不改变媒体提示词。
    let (arguments, sources) = resolve_media_understanding_resource_arguments(
        None,
        Some("invoking-session"),
        ProviderToolCapabilityKind::ImageUnderstand,
        json!({"image_path":" project-resource://7 ","prompt":"describe"}),
        Some(&resolver),
    )
    .await?;
    assert_eq!(
        arguments,
        json!({"image_path":"project-resource://7","prompt":"describe"})
    );
    // source is the only host proof corresponding to the single fixed-facade locator.
    // source 是与单个固定外壳定位符对应的唯一宿主证据。
    let source = sources
        .first()
        .expect("project locator should produce one host source");
    assert_eq!(sources.len(), 1);
    assert_eq!(source.reference, "project-resource://7");
    assert_eq!(source.local_path, resolver.path);
    assert_eq!(source.file_name, "original.png");
    assert_eq!(source.mime_type, "image/png");
    assert_eq!(source.source_identity.byte_len, 3);
    assert_eq!(source.source_identity.content_sha256, "a".repeat(64));
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    Ok(())
}

/// Direct paths use filesystem identity without borrowing attachment ownership from the conversation.
/// 直接路径使用文件系统身份，不借用会话附件所有权。
#[tokio::test]
async fn direct_media_paths_accept_absolute_and_project_external_files()
-> Result<(), Box<dyn std::error::Error>> {
    // The workspace and media file are siblings so one fixture covers absolute and project-external relative paths.
    // 工作区与媒体文件互为同级，因此一个夹具即可覆盖绝对路径与项目外相对路径。
    let directory = tempfile::tempdir()?;
    let workspace = directory.path().join("project");
    let video = directory.path().join("outside.mp4");
    tokio::fs::create_dir(&workspace).await?;
    tokio::fs::write(&video, b"abc").await?;
    let canonical_video = tokio::fs::canonicalize(&video).await?;
    let expected_host_path = PathBuf::from(vulcan_runtime_assets::native_host_path_text_from_path(
        &canonical_video,
    ));

    // An explicit local path is not an attachment locator and therefore needs no conversation owner.
    // 显式本地路径不是附件定位符，因此不需要会话所有者。
    for reference in [
        video.to_string_lossy().into_owned(),
        Path::new("..")
            .join("outside.mp4")
            .to_string_lossy()
            .into_owned(),
    ] {
        let (arguments, sources) = resolve_media_understanding_resource_arguments(
            Some(&workspace),
            None,
            ProviderToolCapabilityKind::VideoUnderstand,
            json!({"video_path": reference.clone(), "prompt": "describe"}),
            None,
        )
        .await?;
        // source is the only local proof corresponding to the fixed media path.
        // source 是与固定媒体路径对应的唯一本地证据。
        let source = sources
            .first()
            .expect("direct path should produce one host source");
        assert_eq!(sources.len(), 1);
        assert_eq!(arguments["video_path"], reference);
        assert_eq!(source.reference, reference);
        assert_eq!(source.local_path, expected_host_path);
        assert_eq!(source.file_name, "outside.mp4");
        assert_eq!(source.mime_type, "video/mp4");
        assert_eq!(source.source_identity.byte_len, 3);
        assert_eq!(
            source.source_identity.content_sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    // The local-path branch still closes on the exact media modality instead of trusting the requested tool.
    // 本地路径分支仍按精确媒体模态关闭失败，不会盲信请求的工具。
    let image = directory.path().join("outside.jpg");
    tokio::fs::write(&image, b"abc").await?;
    assert!(
        resolve_media_understanding_attachment(
            Some(&workspace),
            None,
            ProviderToolCapabilityKind::VideoUnderstand,
            image.to_string_lossy().as_ref(),
            None,
        )
        .await
        .expect_err("an image path must not enter video understanding")
        .contains("does not match")
    );
    Ok(())
}

/// HTTPS media references bypass local path resolution and remain subject to the frozen operation upload route.
/// HTTPS 媒体引用绕过本地路径解析，并继续受冻结操作上传路线约束。
#[tokio::test]
async fn direct_https_media_reference_is_deferred_to_manifest_upload_policy()
-> Result<(), Box<dyn std::error::Error>> {
    // reference deliberately points to no local file; successful preparation proves no canonicalization occurred.
    // reference 刻意不指向本地文件；准备成功证明没有发生路径规范化。
    let reference = "https://media.example.com/source/video.mp4";
    // arguments retain the exact URL for operation-level passthrough validation and projection.
    // arguments 为操作级直通校验与投影保留精确 URL。
    let (arguments, sources) = resolve_media_understanding_resource_arguments(
        None,
        None,
        ProviderToolCapabilityKind::VideoUnderstand,
        json!({"video_path": reference, "prompt": "describe"}),
        None,
    )
    .await?;

    assert_eq!(arguments["video_path"], reference);
    assert!(
        sources.is_empty(),
        "remote passthrough must not invent a local host source"
    );
    Ok(())
}

/// Plain HTTP media references fail explicitly instead of entering local canonicalization.
/// 普通 HTTP 媒体引用会明确失败，而不是进入本地路径规范化。
#[tokio::test]
async fn direct_http_media_reference_fails_before_local_path_resolution() {
    // error must identify the registered HTTPS-only primitive contract.
    // error 必须指出已注册原语仅支持 HTTPS 的契约。
    let error = resolve_media_understanding_resource_arguments(
        None,
        None,
        ProviderToolCapabilityKind::VideoUnderstand,
        json!({
            "video_path": "http://media.example.com/source/video.mp4",
            "prompt": "describe"
        }),
        None,
    )
    .await
    .expect_err("plain HTTP must not be treated as a local file path");

    assert!(
        error.contains("manifest-declared HTTPS URL passthrough"),
        "{error}"
    );
}

/// Rejects absent owner/port and noncanonical resource ids before any project lookup or workspace fallback.
/// 在任何项目查询或工作区回退之前拒绝缺失所有者/端口及非规范资源ID。
#[tokio::test]
async fn project_media_resolution_requires_exact_host_authority()
-> Result<(), Box<dyn std::error::Error>> {
    // No media bytes or network calls are needed for the authority boundary.
    // 权威边界不需要媒体字节或网络调用。
    let directory = tempfile::tempdir()?;
    // Count lookups to prove invalid requests never reach the resource owner.
    // 统计查询，以证明无效请求绝不抵达资源所有者。
    let resolver = ProjectResolver {
        calls: AtomicUsize::new(0),
        path: directory.path().join("opaque-digest"),
    };
    assert!(
        resolve_media_understanding_attachment(
            None,
            None,
            ProviderToolCapabilityKind::ImageUnderstand,
            "project-resource://7",
            Some(&resolver)
        )
        .await
        .unwrap_err()
        .contains("owning conversation")
    );
    assert!(
        resolve_media_understanding_attachment(
            None,
            Some("invoking-session"),
            ProviderToolCapabilityKind::ImageUnderstand,
            "project-resource://7",
            None
        )
        .await
        .unwrap_err()
        .contains("unavailable")
    );
    assert!(
        resolve_media_understanding_attachment(
            Some(directory.path()),
            Some("invoking-session"),
            ProviderToolCapabilityKind::ImageUnderstand,
            "project-resource://07",
            Some(&resolver)
        )
        .await
        .unwrap_err()
        .contains("Invalid project resource locator")
    );
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    Ok(())
}

/// Dynamic bindings retain the same invoking owner and canonical metadata without assuming a fixed tool modality.
/// 动态绑定不假设固定工具模态，保留相同发起所有者及规范元数据。
#[tokio::test]
async fn dynamic_project_source_adapter_preserves_owner_and_catalog_identity()
-> Result<(), Box<dyn std::error::Error>> {
    use vulcan_provider_adapters::ProviderToolMediaSourceResolver;
    // The metadata fixture never reads the entity or contacts a provider.
    // 元数据夹具绝不读取实体或联系供应商。
    let directory = tempfile::tempdir()?;
    // The original filename and MIME remain independent of the stored entity basename.
    // 原文件名和MIME独立于存储实体基础名。
    let resolver = ProjectResolver {
        calls: AtomicUsize::new(0),
        path: directory.path().join("opaque-digest"),
    };
    // Host context, not JSON, fixes the owner for the whole invocation.
    // 宿主上下文而非JSON为整个调用固定所有者。
    let adapter = crate::media_resource_runtime::SessionProviderToolMediaSourceResolver {
        runtime: Some(&resolver),
        workspace_root: None,
        session_id: "invoking-session",
    };
    // Invalid aliases never reach the project owner or try a local-path fallback.
    // 无效别名绝不抵达项目所有者，也不尝试本地路径回退。
    for reference in [
        "project-resource://07",
        "project-resource://7?model=b",
        "project-resource://7/path",
        "attachment://7",
    ] {
        assert!(adapter.resolve_source(reference).await.is_err());
    }
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 0);
    // The exact locator gets one host proof while retaining the original image modality.
    // 精确定位符取得一份宿主证明，同时保留原图片模态。
    let source = adapter.resolve_source("project-resource://7").await?;
    assert_eq!(source.reference, "project-resource://7");
    assert_eq!(source.file_name, "original.png");
    assert_eq!(source.mime_type, "image/png");
    assert_eq!(source.local_path, resolver.path);
    assert_eq!(source.source_identity.byte_len, 3);
    assert_eq!(source.source_identity.content_sha256, "a".repeat(64));
    assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    // Fixed facades still reject a different requested modality through the shared port.
    // 固定外壳仍通过共享端口拒绝不同请求模态。
    assert!(
        resolver
            .resolve_project_media(
                "invoking-session",
                ResourceId::try_new(7)?,
                ConversationAttachmentKind::Audio
            )
            .await
            .unwrap_err()
            .contains("does not match")
    );
    Ok(())
}

/// Managed attachment locators use the invoking session and index without requiring a V1 project port.
/// 托管附件定位符使用发起会话和索引，不要求V1项目端口。
#[tokio::test]
async fn dynamic_attachment_source_adapter_requires_exact_managed_owner()
-> Result<(), Box<dyn std::error::Error>> {
    use vulcan_provider_adapters::ProviderToolMediaSourceResolver;
    // Only an isolated staged fixture is created; user workspaces remain untouched.
    // 只创建隔离暂存夹具；不触碰用户工作区。
    let directory = tempfile::tempdir()?;
    // The index and managed source share the same host workspace authority.
    // 索引及托管来源共享相同宿主工作区权威。
    let workspace = directory.path().to_string_lossy();
    // Generic files retain their original MIME instead of being forced into an image tool category.
    // 普通文件保留原MIME，不强制归入图片工具类别。
    let attachment = crate::attachment_assets::stage_attachment_bytes(
        &workspace,
        &crate::attachment_assets::generic_attachment_descriptor(),
        "original.bin",
        None,
        b"abc",
    )?;
    // The disclosed locator selects one opaque id, never a caller-supplied physical path.
    // 披露定位符选择一个不透明ID，绝不选择调用方提供的物理路径。
    let reference = format!(
        "attachment://{}",
        attachment.asset_id.as_deref().ok_or("fixture asset")?
    );
    // Project and managed schemes have separate explicit authorities in the same invocation.
    // 相同调用中的项目和托管方案具有分离的显式权威。
    let adapter = crate::media_resource_runtime::SessionProviderToolMediaSourceResolver {
        runtime: None,
        workspace_root: Some(directory.path()),
        session_id: "invoking-session",
    };
    assert!(
        adapter
            .resolve_source(&reference)
            .await
            .unwrap_err()
            .contains("not owned")
    );
    crate::attachment_asset_index::promote_staged_attachment_to_message_ref(
        &workspace,
        "invoking-session",
        "message-1",
        std::slice::from_ref(&attachment),
        &[],
    )?;
    // A published message owner admits metadata while leaving content verification to the bounded materializer.
    // 已发布消息所有者准入元数据，同时将内容验证留给有界物化器。
    let source = adapter.resolve_source(&reference).await?;
    assert_eq!(source.reference, reference);
    assert_eq!(source.file_name, "original.bin");
    assert_eq!(source.mime_type, attachment.mime_type);
    assert_eq!(source.source_identity.byte_len, 3);
    assert_eq!(
        Some(source.source_identity.content_sha256),
        attachment.source_sha256
    );
    // A different session cannot borrow the same source just because its workspace matches.
    // 不同会话不能仅因工作区相同就借用同一来源。
    let other = crate::media_resource_runtime::SessionProviderToolMediaSourceResolver {
        runtime: None,
        workspace_root: Some(directory.path()),
        session_id: "other-session",
    };
    assert!(
        other
            .resolve_source(&reference)
            .await
            .unwrap_err()
            .contains("not owned")
    );
    assert!(
        adapter
            .resolve_source("project-resource://1")
            .await
            .unwrap_err()
            .contains("unavailable")
    );
    Ok(())
}
