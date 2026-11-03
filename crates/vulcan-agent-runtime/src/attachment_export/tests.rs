//! Attachment export tests: ownership-checked copies that ordinary tools can open.
//! 附件导出测试：经过归属校验、普通工具可打开的副本。

use super::*;
use crate::media_resource_runtime::MediaResourceRuntime;
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};
use vulcan_session_core::v1::ResourceId;

/// Fixture resolving resource 9 of `invoking-session` to an extensionless digest-named entity.
/// 将 `invoking-session` 的资源 9 解析为无扩展名摘要命名实体的夹具。
#[derive(Debug)]
struct ProjectResolver {
    /// Entity path, named like the host catalog stores it.
    /// 实体路径，按宿主目录的存储方式命名。
    path: PathBuf,
    /// Registered size of the entity.
    /// 实体登记大小。
    byte_len: u64,
    /// Registered SHA-256 of the entity.
    /// 实体登记 SHA-256。
    sha256: String,
}

#[async_trait::async_trait]
impl MediaResourceRuntime for ProjectResolver {
    /// Returns registered DOCX metadata only for the fixture owner and id.
    /// 仅为夹具所有者及 ID 返回已登记 DOCX 元数据。
    async fn resolve_project_resource(
        &self,
        session_id: &str,
        resource_id: ResourceId,
    ) -> Result<ConversationAttachment, String> {
        if session_id != "invoking-session" || resource_id.value() != 9 {
            return Err("Project resource is not owned by this session.".into());
        }
        Ok(ConversationAttachment {
            kind: ConversationAttachmentKind::File,
            file_name: "report: draft.docx".into(),
            local_path: self.path.to_string_lossy().into_owned(),
            resource_path: Some("project-resource://9".into()),
            source_path: None,
            asset_id: None,
            byte_len: Some(self.byte_len),
            source_sha256: Some(self.sha256.clone()),
            delivery_mode: None,
            preview_local_path: None,
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .into(),
        })
    }
}

/// Builds a project with one stored entity holding `content`, returning the workspace, storage and resolver.
/// 构建一个存有 `content` 实体的项目，返回工作区、存储及解析器。
fn fixture(
    content: &[u8],
) -> Result<(tempfile::TempDir, tempfile::TempDir, Arc<ProjectResolver>), Box<dyn std::error::Error>>
{
    let workspace = tempfile::tempdir()?;
    let storage = tempfile::tempdir()?;
    let sha256 = hex::encode(Sha256::digest(content));
    let path = storage.path().join(&sha256);
    std::fs::write(&path, content)?;
    let resolver = Arc::new(ProjectResolver {
        path,
        byte_len: content.len() as u64,
        sha256,
    });
    Ok((workspace, storage, resolver))
}

/// Exports `locator` for `invoking-session` with `resolver`.
/// 使用 `resolver` 为 `invoking-session` 导出 `locator`。
async fn export(
    workspace: &Path,
    resolver: &Arc<ProjectResolver>,
    locator: &str,
) -> Result<Value, String> {
    let resources: Arc<dyn MediaResourceRuntime> = resolver.clone();
    export_attachment(
        workspace,
        Some(resources),
        "invoking-session",
        json!({ "resource_path": locator }),
    )
    .await
    .map(|result| result.model_output)
}

/// A project resource becomes a workspace file with its original extension and exact bytes.
/// 项目资源成为保留原扩展名与精确字节的工作区文件。
#[tokio::test]
async fn project_resource_is_copied_into_the_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let (workspace, _storage, resolver) = fixture(b"docx bytes")?;
    let output = export(workspace.path(), &resolver, "project-resource://9").await?;
    let relative = output["path"].as_str().ok_or("path")?;
    assert!(relative.starts_with(".vulcan/temp/attachments/"));
    assert!(relative.ends_with(".docx"));
    assert_eq!(std::fs::read(workspace.path().join(relative))?, b"docx bytes");
    assert_eq!(output["byte_len"], json!(10));
    Ok(())
}

/// Exporting the same resource twice yields the same path and keeps the copy intact.
/// 重复导出同一资源得到相同路径且副本保持完整。
#[tokio::test]
async fn repeated_export_reuses_the_same_copy() -> Result<(), Box<dyn std::error::Error>> {
    let (workspace, _storage, resolver) = fixture(b"same")?;
    let first = export(workspace.path(), &resolver, "project-resource://9").await?;
    let second = export(workspace.path(), &resolver, "project-resource://9").await?;
    assert_eq!(first["path"], second["path"]);
    let relative = second["path"].as_str().ok_or("path")?;
    assert_eq!(std::fs::read(workspace.path().join(relative))?, b"same");
    Ok(())
}

/// A source whose bytes no longer match the registered digest leaves no export behind.
/// 字节不再匹配登记摘要的来源不留下任何导出文件。
#[tokio::test]
async fn changed_source_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let (workspace, _storage, resolver) = fixture(b"original")?;
    std::fs::write(&resolver.path, b"tampered")?;
    assert!(
        export(workspace.path(), &resolver, "project-resource://9")
            .await
            .is_err()
    );
    let exports = crate::project_temp_maintenance::resolve_project_temp_dir(workspace.path())
        .join(EXPORT_DIRECTORY_NAME);
    let leftover = std::fs::read_dir(&exports)
        .into_iter()
        .flatten()
        .flatten()
        .flat_map(|directory| std::fs::read_dir(directory.path()).into_iter().flatten())
        .count();
    assert_eq!(leftover, 0);
    Ok(())
}

/// Plain paths and resources owned by another session are refused.
/// 普通路径与其他会话所属资源被拒绝。
#[tokio::test]
async fn non_locators_and_foreign_resources_are_refused() -> Result<(), Box<dyn std::error::Error>>
{
    let (workspace, _storage, resolver) = fixture(b"x")?;
    assert!(export(workspace.path(), &resolver, "docs/a.pdf").await.is_err());
    assert!(
        export(workspace.path(), &resolver, "project-resource://8")
            .await
            .is_err()
    );
    Ok(())
}

/// Sanitized names stay one portable component and keep their extension.
/// 清理后的名称保持为单个可移植分量并保留扩展名。
#[test]
fn file_names_become_one_portable_component() {
    assert_eq!(sanitize_file_name("a/b\\c.pdf"), "a_b_c.pdf");
    assert_eq!(sanitize_file_name("report: draft.docx"), "report_ draft.docx");
    assert_eq!(sanitize_file_name("CON.txt"), "_CON.txt");
    assert_eq!(sanitize_file_name("COM1.pptx"), "_COM1.pptx");
    assert_eq!(sanitize_file_name(".."), FALLBACK_FILE_NAME);
    assert_eq!(sanitize_file_name("name. "), "name");
}
