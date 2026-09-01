//! Real session-owner resource resolution regressions without network or provider configuration.
//! 不使用网络或供应商配置的真实会话所有者资源解析回归。

use super::*;
use crate::session_runtime_registry_v1::SessionHandle;
use vulcan_workspace_store::workspace_store::{
    ProjectResourceCatalogRegistry, ProjectResourceRegistration, create_session_v1_storage_fixture,
    load_session_v1_full,
};

/// Registers one session and `name`/`mime` source in isolated `directory`, returning its exact owner and resource id.
/// 在隔离`directory`注册一个会话及`name`/`mime`来源，返回精确所有者和资源ID。
fn register_source(
    registry: &SessionRuntimeRegistry,
    directory: &std::path::Path,
    name: &str,
    mime: &str,
    bytes: &[u8],
) -> Result<(SessionId, ResourceId), String> {
    // A real storage locator, not the caller's workspace, selects the project partition.
    // 真实存储定位器而非调用者工作区选择项目分区。
    let session_id = SessionId::new_v4();
    // Each directory receives an independent project catalog even when logical ids coincide.
    // 即使逻辑ID重合，每个目录仍获得独立项目目录。
    let locator = create_session_v1_storage_fixture(directory, &session_id.to_string(), 1)?;
    // Load the production document representation before registering its unique handle.
    // 注册唯一句柄前加载生产文档表示。
    let loaded = load_session_v1_full(&locator).map_err(|error| error.to_string())?;
    // Resource registration preserves display MIME and filename independently of the hashed entity.
    // 资源注册独立于哈希实体保留显示MIME及文件名。
    let ids = ProjectResourceCatalogRegistry::default()
        .resolve_or_register_project_resources(
            &locator.project_directory(),
            &[ProjectResourceRegistration {
                original_name: name.into(),
                mime: Some(mime.into()),
                content: bytes.to_vec(),
                provider_bindings: Vec::new(),
            }],
        )
        .map_err(|error| error.to_string())?;
    registry
        .register(SessionHandle::new(session_id, locator, loaded.document))
        .map_err(|error| error.to_string())?;
    Ok((session_id, ids[0]))
}

/// Resolves equal logical ids against the invoking session's project and preserves complete source metadata.
/// 按调用会话项目解析相同逻辑ID，并保留完整来源元数据。
#[tokio::test]
async fn project_media_resolution_is_owner_scoped() -> Result<(), Box<dyn std::error::Error>> {
    // Independent temporary roots cannot share physical resource entities.
    // 独立临时根目录不能共享物理资源实体。
    let first = tempfile::tempdir()?;
    // This second root intentionally also starts with resource id one.
    // 第二个根目录故意也从资源ID一开始。
    let second = tempfile::tempdir()?;
    // One registry owns both sessions, exercising actual lookup rather than a mock branch.
    // 一个注册表持有两个会话，验证真实查询而非模拟分支。
    let registry = Arc::new(SessionRuntimeRegistry::default());
    // Different names and bytes must remain isolated despite equal logical ids.
    // 尽管逻辑ID相等，不同名称和字节仍必须隔离。
    let (first_session, first_id) =
        register_source(&registry, first.path(), "first.png", "image/png", b"first")?;
    // Capture the second session's independent ownership proof.
    // 捕获第二个会话的独立所有权证据。
    let (second_session, second_id) = register_source(
        &registry,
        second.path(),
        "second.png",
        "image/png",
        b"second",
    )?;
    assert_eq!(first_id, second_id);
    // The production port receives only host session identity, never a project path from model arguments.
    // 生产端口只接收宿主会话身份，不接收模型参数中的项目路径。
    let port = SessionMediaResources { registry };
    // Resolve through each exact registered owner.
    // 通过各个精确已注册所有者解析。
    let first_media = port
        .resolve_project_media(
            &first_session.to_string(),
            first_id,
            ConversationAttachmentKind::Image,
        )
        .await?;
    // The same locator text resolves to the second owner's independent entry.
    // 相同定位符文本解析至第二个所有者的独立条目。
    let second_media = port
        .resolve_project_media(
            &second_session.to_string(),
            second_id,
            ConversationAttachmentKind::Image,
        )
        .await?;
    assert_eq!(first_media.file_name, "first.png");
    assert_eq!(first_media.mime_type, "image/png");
    assert_eq!(first_media.byte_len, Some(5));
    assert_eq!(
        first_media.resource_path.as_deref(),
        Some("project-resource://1")
    );
    assert_eq!(second_media.file_name, "second.png");
    assert_ne!(first_media.local_path, second_media.local_path);
    assert_ne!(first_media.source_sha256, second_media.source_sha256);
    assert!(
        std::path::Path::new(&first_media.local_path)
            .extension()
            .is_none()
    );
    assert!(
        port.resolve_project_media(
            &first_session.to_string(),
            first_id,
            ConversationAttachmentKind::Audio
        )
        .await
        .is_err()
    );
    assert!(
        port.resolve_project_media(
            &SessionId::new_v4().to_string(),
            first_id,
            ConversationAttachmentKind::Image
        )
        .await
        .is_err()
    );
    assert!(
        port.resolve_project_media(
            "invalid-session",
            first_id,
            ConversationAttachmentKind::Image
        )
        .await
        .is_err()
    );
    Ok(())
}

/// Dynamic operation resolution preserves every catalog modality, including PDF and generic files.
/// 动态操作解析保留全部目录模态，包括PDF及普通文件。
#[tokio::test]
async fn dynamic_project_resolution_keeps_catalog_modality()
-> Result<(), Box<dyn std::error::Error>> {
    for (name, mime, kind) in [
        ("image.png", "image/png", ConversationAttachmentKind::Image),
        ("audio.wav", "audio/wav", ConversationAttachmentKind::Audio),
        ("video.mp4", "video/mp4", ConversationAttachmentKind::Video),
        (
            "document.pdf",
            "application/pdf",
            ConversationAttachmentKind::Pdf,
        ),
        (
            "file.bin",
            "application/octet-stream",
            ConversationAttachmentKind::File,
        ),
    ] {
        // Every fixture owns an independent registered project and a tiny immutable source.
        // 每个夹具持有独立已注册项目及一个微小不可变来源。
        let directory = tempfile::tempdir()?;
        // The same production port serves fixed facades and arbitrary declared operation bindings.
        // 相同生产端口服务固定外壳及任意声明操作绑定。
        let registry = Arc::new(SessionRuntimeRegistry::default());
        // Original MIME and filename come only from registration, not the extensionless entity path.
        // 原MIME和文件名仅来自注册，不来自无扩展名实体路径。
        let (session, id) = register_source(&registry, directory.path(), name, mime, b"abc")?;
        // The invocation selects only the exact registered session and logical id.
        // 调用只选择精确已注册会话及逻辑ID。
        let port = SessionMediaResources { registry };
        let source = port
            .resolve_project_resource(&session.to_string(), id)
            .await?;
        assert_eq!(source.kind, kind);
        assert_eq!(source.file_name, name);
        assert_eq!(source.mime_type, mime);
        assert_eq!(source.byte_len, Some(3));
        assert!(
            std::path::Path::new(&source.local_path)
                .extension()
                .is_none()
        );
    }
    Ok(())
}

/// Treats missing entities and post-registration size drift as safe explicit failures without path disclosure.
/// 将实体缺失和注册后大小漂移视为安全明确失败，不泄露路径。
#[tokio::test]
async fn project_media_missing_or_changed_source_has_safe_error()
-> Result<(), Box<dyn std::error::Error>> {
    // Only this temporary root is mutated by the regression.
    // 回归测试只修改这个临时根目录。
    let directory = tempfile::tempdir()?;
    // Register the real session owner and a three-byte source.
    // 注册真实会话所有者及三字节来源。
    let registry = Arc::new(SessionRuntimeRegistry::default());
    // Preserve exact identities for each metadata lookup.
    // 为每次元数据查询保留精确身份。
    let (session, id) = register_source(
        &registry,
        directory.path(),
        "source.png",
        "image/png",
        b"png",
    )?;
    // Use the production resolver to obtain the fixture-only host path.
    // 使用生产解析器取得仅供夹具使用的宿主路径。
    let port = SessionMediaResources { registry };
    // The original attachment proves all later failures refer to this exact entity.
    // 原附件证明后续全部失败均指向这个精确实体。
    let original = port
        .resolve_project_media(&session.to_string(), id, ConversationAttachmentKind::Image)
        .await?;
    std::fs::write(&original.local_path, b"grown")?;
    assert_eq!(
        port.resolve_project_media(&session.to_string(), id, ConversationAttachmentKind::Image)
            .await
            .unwrap_err(),
        "Project resource source is unavailable or invalid."
    );
    std::fs::remove_file(&original.local_path)?;
    assert_eq!(
        port.resolve_project_media(&session.to_string(), id, ConversationAttachmentKind::Image)
            .await
            .unwrap_err(),
        "Project resource source is missing."
    );
    Ok(())
}
