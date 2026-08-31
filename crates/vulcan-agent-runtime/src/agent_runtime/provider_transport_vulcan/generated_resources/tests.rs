//! Regression tests for the generated_resources boundary.
//! generated_resources边界的回归测试。

use super::*;
use std::path::PathBuf;
use vulcan_ai_contract::ResourceReference;

/// Verifies generated image bytes enter managed attachment storage and the Data URL is discarded.
/// 验证生成图片字节会进入托管附件存储，且 Data URL 会被丢弃。
#[tokio::test]
async fn generated_image_event_materializes_inside_workspace() {
    let workspace = std::env::temp_dir().join(format!(
        "vulcan-generated-resource-test-{}",
        uuid::Uuid::new_v4()
    ));
    tokio::fs::create_dir_all(&workspace)
        .await
        .expect("test workspace should be created");
    let event = AiStreamEvent::ResponseItemStarted {
        item_id: String::from("item"),
        item: ContentItem::Resource {
            resource: ResourceReference {
                id: String::from("image:item"),
                kind: ResourceKind::Image,
                media_type: Some(String::from("image/png")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::InlineData,
                file_name: None,
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: Some(String::from("data:image/png;base64,aW1hZ2U=")),
            },
        },
    };
    let materialized =
        materialize_provider_resource_event("openai", workspace.to_string_lossy().as_ref(), event)
            .await
            .expect("generated image should materialize");
    let AiStreamEvent::ResponseItemStarted {
        item: ContentItem::Resource { resource },
        ..
    } = materialized
    else {
        panic!("materialized event should remain a resource item");
    };
    assert!(resource.url.is_none());
    let local_path = PathBuf::from(
        resource
            .local_path
            .expect("materialized resource should have a local path"),
    );
    assert!(local_path.starts_with(&workspace));
    assert_eq!(
        tokio::fs::read(&local_path)
            .await
            .expect("materialized image should be readable"),
        b"image"
    );
    tokio::fs::remove_dir_all(&workspace)
        .await
        .expect("test workspace should be removed");
}

/// Verifies a user-safe remote resource URL is preserved instead of being decoded as Base64.
/// 验证用户安全的远程资源 URL 会被保留，而不会被当作 Base64 解码。
#[tokio::test]
async fn remote_generated_image_url_remains_remote() {
    let event = AiStreamEvent::ResponseItemStarted {
        item_id: String::from("item"),
        item: ContentItem::Resource {
            resource: ResourceReference {
                id: String::from("remote-image"),
                kind: ResourceKind::Image,
                media_type: Some(String::from("image/png")),
                source_kind: vulcan_ai_contract::ResourceSourceKind::RemoteUrl,
                file_name: None,
                image_detail: None,
                resource_path: None,
                local_path: None,
                url: Some(String::from("https://example.invalid/image.png")),
            },
        },
    };
    let preserved = materialize_provider_resource_event("openai", "C:\\workspace", event)
        .await
        .expect("remote resource should remain valid");
    assert!(matches!(
        preserved,
        AiStreamEvent::ResponseItemStarted {
            item: ContentItem::Resource { resource },
            ..
        } if resource.url.as_deref() == Some("https://example.invalid/image.png")
            && resource.local_path.is_none()
    ));
}

/// Verifies generated resource file stems cannot escape the host-owned output directory.
/// 验证生成资源文件主名无法逃逸宿主自有输出目录。
#[test]
fn generated_resource_stem_removes_path_syntax() {
    assert_eq!(
        safe_generated_resource_stem("../../image:item"),
        "------image-item"
    );
}
