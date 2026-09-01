use super::*;
use sha2::{Digest, Sha256};
use std::io::Read;

/// Resolves whether one attachment can replay from the managed asset copy, source fallback, or neither.
/// 判断一个附件可从托管资产副本、来源回退路径还是都不可用来回放。
pub fn resolve_attachment_replay_status(
    attachment: &ConversationAttachment,
) -> AttachmentReplayStatus {
    let asset_record = load_attachment_asset_record_for_attachment(attachment);
    if asset_record
        .as_ref()
        .is_some_and(|record| record.deletion_pending || record.purged_at_unix_millis.is_some())
    {
        return AttachmentReplayStatus::Purged;
    }

    let managed_path = attachment.local_path.trim();
    if !managed_path.is_empty()
        && asset_record
            .as_ref()
            .is_none_or(|record| record.managed_path == managed_path)
        && attachment_replay_path_matches_integrity(Path::new(managed_path), asset_record.as_ref())
    {
        return AttachmentReplayStatus::Available;
    }

    let source_path = attachment
        .source_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    if source_path.as_ref().is_some_and(|path| {
        asset_record
            .as_ref()
            .is_none_or(|record| record.source_path.as_deref().map(str::trim) == path.to_str())
            && attachment_replay_path_matches_integrity(path, asset_record.as_ref())
    }) {
        return AttachmentReplayStatus::FallbackSourceOnly;
    }

    AttachmentReplayStatus::Missing
}

/// Verifies one replay path against the immutable byte facts retained by its managed asset record.
/// 根据托管资产记录保留的不可变字节事实校验一条回放路径。
///
/// # Parameters
/// 参数
/// `path` is the exact managed or fallback source path and `asset_record` is the optional indexed
/// identity that owns the canonical byte length and SHA-256 digest.
/// `path` 是精确的托管路径或来源回退路径，`asset_record` 是可选索引身份，持有规范字节长度与
/// SHA-256 摘要。
///
/// # Returns
/// 返回值
/// `true` only when the path is a file and every available indexed integrity fact still matches.
/// 仅当路径是文件且全部可用索引完整性事实仍匹配时返回 `true`。
fn attachment_replay_path_matches_integrity(
    path: &Path,
    asset_record: Option<&AttachmentAssetRecord>,
) -> bool {
    if !path.is_file() {
        return false;
    }
    let Some(asset_record) = asset_record else {
        return true;
    };
    let Some(expected_byte_len) = asset_record.byte_len else {
        return false;
    };
    let Some(expected_sha256) = asset_record
        .source_sha256
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };
    std::fs::metadata(path).is_ok_and(|metadata| metadata.len() == expected_byte_len)
        && attachment_record_sha256(path).is_ok_and(|actual| actual == expected_sha256)
}

/// Loads the asset-index record associated with the provided conversation attachment when one exists.
/// 在存在对应记录时，载入与给定会话附件关联的资产索引记录。
pub fn load_attachment_asset_record_for_attachment(
    attachment: &ConversationAttachment,
) -> Option<AttachmentAssetRecord> {
    let asset_root = resolve_asset_root_from_attachment(attachment)?;
    let document = load_attachment_asset_index_document(&asset_root).ok()?;
    if let Some(asset_id) = attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        && let Some(record) = document
            .assets
            .iter()
            .find(|record| record.asset_id == asset_id)
    {
        return Some(record.clone());
    }
    let managed_path = attachment.local_path.trim();
    document
        .assets
        .iter()
        .find(|record| record.managed_path == managed_path)
        .cloned()
}

/// Resolves one AI-visible attachment locator into an owned, integrity-checked managed resource.
/// 将一个 AI 可见附件定位符解析为已校验所有权与完整性的托管资源。
///
/// # Parameters
/// 参数
/// `workspace_path` owns the managed asset index, `session_id` is the invoking conversation,
/// `reference` is either `attachment://<id>` or the canonical project-relative resource path, and
/// `expected_kind` prevents cross-modality substitution.
/// `workspace_path` 持有托管资产索引，`session_id` 是发起调用的会话，`reference` 是
/// `attachment://<id>` 或规范项目相对资源路径，`expected_kind` 防止跨模态替换。
///
/// # Returns
/// 返回值
/// The authoritative attachment only when ownership, path containment, byte length, and SHA-256 all match.
/// 仅当所有权、路径边界、字节长度与 SHA-256 全部匹配时返回权威附件。
pub fn resolve_owned_attachment_resource(
    workspace_path: &str,
    session_id: &str,
    reference: &str,
    expected_kind: ConversationAttachmentKind,
) -> Result<ConversationAttachment, String> {
    let session_id = session_id.trim();
    if session_id.is_empty() {
        return Err(String::from(
            "Attachment resource resolution requires an owning conversation id.",
        ));
    }
    resolve_indexed_attachment_resource_internal(
        workspace_path,
        Some(session_id),
        reference,
        expected_kind,
    )
}

/// Resolves metadata for canonical `reference` under exact `session_id` and `workspace_path` without reading resource bytes.
/// 在精确`session_id`及`workspace_path`下解析规范`reference`元数据，不读取资源字节。
/// Returns original managed identity after owner, containment and size checks; the caller must validate content after admission.
/// 在所有者、边界和大小检查后返回原托管身份；调用方必须在准入后验证内容。
pub(crate) fn resolve_owned_attachment_resource_metadata(
    workspace_path: &str,
    session_id: &str,
    reference: &str,
) -> Result<ConversationAttachment, String> {
    if session_id.trim().is_empty() {
        return Err(String::from(
            "Attachment resource resolution requires an owning conversation id.",
        ));
    }
    // Dynamic bindings accept the disclosed opaque id, never an alias that normalizes into another source.
    // 动态绑定接受披露的不透明ID，绝不接受规范化成另一来源的别名。
    let asset_id = reference
        .strip_prefix("attachment://")
        .filter(|id| {
            !id.is_empty()
                && id.trim() == *id
                && !id.chars().any(|character| {
                    character.is_control() || character == '/' || character == '\\'
                })
        })
        .ok_or_else(|| String::from("Invalid canonical attachment resource locator."))?;
    // Keep the validated exact spelling when selecting the existing owner-scoped record.
    // 选择既有所有者范围记录时保持已校验精确拼写。
    let reference = format!("attachment://{asset_id}");
    resolve_indexed_attachment_resource_metadata_internal(
        workspace_path,
        Some(session_id),
        &reference,
        None,
    )
}

/// Resolves declared fixed-tool `reference` under `session_id` in `workspace_path`, enforcing `expected_kind` without reading bytes.
/// 在`workspace_path`中按`session_id`解析固定工具声明的`reference`，执行`expected_kind`约束但不读取字节。
/// Returns metadata for either legacy disclosed locator form; the media planner must admit it before content verification.
/// 为两种旧披露定位符形态返回元数据；媒体规划器必须先准入，然后才能验证内容。
pub(crate) fn resolve_owned_attachment_resource_metadata_for_kind(
    workspace_path: &str,
    session_id: &str,
    reference: &str,
    expected_kind: ConversationAttachmentKind,
) -> Result<ConversationAttachment, String> {
    if session_id.trim().is_empty() {
        return Err(String::from(
            "Attachment resource resolution requires an owning conversation id.",
        ));
    }
    resolve_indexed_attachment_resource_metadata_internal(
        workspace_path,
        Some(session_id),
        reference,
        Some(expected_kind),
    )
}

/// Resolves one immutable managed attachment for a host-owned draft mutation before it has canonical message ownership.
/// 在托管附件拥有规范消息所有权之前，为宿主持有的草稿变更解析一条不可变托管附件。
///
/// # Parameters
/// 参数
/// `workspace_path` identifies the project-scoped managed asset index, `reference` is an `attachment://<id>` locator, and `expected_kind` prevents cross-modality mutation.
/// `workspace_path` 标识项目范围的托管资产索引，`reference` 是 `attachment://<id>` 定位符，`expected_kind` 防止跨模态变更。
///
/// # Returns
/// 返回值
/// The authoritative attachment only when the asset id, kind, path containment, byte length, and SHA-256 all match.
/// 仅当资产标识、类型、路径边界、字节长度与 SHA-256 全部匹配时返回权威附件。
pub fn resolve_indexed_attachment_resource(
    workspace_path: &str,
    reference: &str,
    expected_kind: ConversationAttachmentKind,
) -> Result<ConversationAttachment, String> {
    if !reference.trim().starts_with("attachment://") {
        return Err(String::from(
            "Host draft attachment mutation requires an attachment asset id locator.",
        ));
    }
    resolve_indexed_attachment_resource_internal(workspace_path, None, reference, expected_kind)
}

/// Resolves and verifies one indexed attachment with optional canonical conversation ownership enforcement.
/// 解析并校验一条索引附件，并可选强制校验规范会话所有权。
///
/// # Parameters
/// 参数
/// `workspace_path` identifies the managed index, `owner_session_id` optionally requires canonical ownership, `reference` locates the asset, and `expected_kind` freezes the modality.
/// `workspace_path` 标识托管索引，`owner_session_id` 可选要求规范所有权，`reference` 定位资产，`expected_kind` 冻结模态。
///
/// # Returns
/// 返回值
/// The integrity-verified canonical attachment or one explicit validation error.
/// 返回通过完整性校验的规范附件，或一个明确的校验错误。
fn resolve_indexed_attachment_resource_internal(
    workspace_path: &str,
    owner_session_id: Option<&str>,
    reference: &str,
    expected_kind: ConversationAttachmentKind,
) -> Result<ConversationAttachment, String> {
    // Existing full-verification callers retain their strict content check after common metadata admission.
    // 既有完整校验调用者在共同元数据准入后保留严格内容检查。
    let attachment = resolve_indexed_attachment_resource_metadata_internal(
        workspace_path,
        owner_session_id,
        reference,
        Some(expected_kind),
    )?;
    // Only the immutable registered digest can authorize the already selected managed entity.
    // 只有不可变注册摘要可以授权已选定托管实体。
    let actual_sha256 = attachment_record_sha256(Path::new(&attachment.local_path))?;
    if attachment.source_sha256.as_deref() != Some(actual_sha256.as_str()) {
        return Err(String::from(
            "Managed attachment resource content no longer matches its canonical digest.",
        ));
    }
    Ok(attachment)
}

/// Resolves exact indexed metadata for `reference` in `workspace_path`, optionally enforcing the declared owner and modality.
/// 在`workspace_path`中解析`reference`精确索引元数据，可选执行声明的所有者及模态约束。
/// Returns containment- and size-checked original facts without reading content; callers must verify bytes after policy admission.
/// 不读取内容即返回通过边界及大小检查的原始事实；调用者必须在策略准入后验证字节。
fn resolve_indexed_attachment_resource_metadata_internal(
    workspace_path: &str,
    owner_session_id: Option<&str>,
    reference: &str,
    expected_kind: Option<ConversationAttachmentKind>,
) -> Result<ConversationAttachment, String> {
    let reference = reference.trim().replace('\\', "/");
    if reference.is_empty() {
        return Err(String::from("Attachment resource locator is empty."));
    }
    if Path::new(reference.as_str()).is_absolute()
        || Path::new(reference.as_str())
            .components()
            .any(|component| component == Component::ParentDir)
    {
        return Err(String::from(
            "Attachment resource locators must be project-relative and cannot traverse parent directories.",
        ));
    }

    let asset_root = resolve_managed_attachment_root(workspace_path);
    let document = load_attachment_asset_index_document(&asset_root)?;
    let record = if let Some(asset_id) = reference.strip_prefix("attachment://") {
        let asset_id = asset_id.trim();
        if asset_id.is_empty() || asset_id.contains('/') {
            return Err(String::from("Attachment resource id is invalid."));
        }
        document
            .assets
            .iter()
            .find(|record| record.asset_id == asset_id)
    } else {
        document.assets.iter().find(|record| {
            crate::attachment_assets::managed_attachment_resource_path(
                workspace_path,
                Path::new(record.managed_path.as_str()),
            )
            .is_ok_and(|resource_path| resource_path == reference)
        })
    }
    .ok_or_else(|| {
        String::from("Attachment resource is not present in the managed asset index.")
    })?;

    if owner_session_id
        .is_some_and(|session_id| !record.session_ids.iter().any(|owner| owner == session_id))
    {
        return Err(String::from(
            "Attachment resource is not owned by the invoking conversation.",
        ));
    }
    if record.deletion_pending || record.purged_at_unix_millis.is_some() {
        return Err(String::from(
            "Attachment resource is no longer available for understanding.",
        ));
    }
    if expected_kind.is_some_and(|expected| record.kind != expected) {
        return Err(String::from(
            "Attachment resource modality does not match the invoked understanding tool.",
        ));
    }

    let managed_path = PathBuf::from(record.managed_path.as_str());
    let canonical_root = fs::canonicalize(&asset_root)
        .map_err(|error| format!("Managed attachment root cannot be resolved: {error}"))?;
    let canonical_path = fs::canonicalize(&managed_path)
        .map_err(|error| format!("Managed attachment resource cannot be resolved: {error}"))?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(String::from(
            "Managed attachment resource escaped its authoritative root.",
        ));
    }
    let metadata = fs::metadata(&canonical_path)
        .map_err(|error| format!("Managed attachment resource cannot be inspected: {error}"))?;
    if !metadata.is_file() || record.byte_len != Some(metadata.len()) {
        return Err(String::from(
            "Managed attachment resource byte length no longer matches its canonical record.",
        ));
    }
    let expected_sha256 = record
        .source_sha256
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            String::from("Managed attachment resource has no canonical SHA-256 digest.")
        })?;
    let resource_path = crate::attachment_assets::managed_attachment_resource_path(
        workspace_path,
        &canonical_path,
    )?;
    Ok(ConversationAttachment {
        kind: record.kind.clone(),
        file_name: record.original_file_name.clone(),
        local_path: vulcan_runtime_assets::native_host_path_text_from_path(&canonical_path),
        resource_path: Some(resource_path),
        source_path: None,
        asset_id: Some(record.asset_id.clone()),
        byte_len: record.byte_len,
        source_sha256: Some(expected_sha256.to_string()),
        delivery_mode: None,
        preview_local_path: None,
        mime_type: record.mime_type.clone(),
    })
}

/// Computes the lowercase SHA-256 digest for one managed attachment record.
/// 计算单个托管附件记录的小写 SHA-256 摘要。
///
/// # Parameters
/// 参数
/// `path` is the already contained canonical managed file.
/// `path` 是已经通过边界校验的规范托管文件。
///
/// # Returns
/// 返回值
/// The exact lowercase digest or one explicit read error.
/// 返回精确的小写摘要，或一个明确读取错误。
fn attachment_record_sha256(path: &Path) -> Result<String, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("Managed attachment resource cannot be opened: {error}"))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("Managed attachment resource cannot be hashed: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(test)]
mod tests {
    //! Owned attachment-resource resolution regression tests.
    //! 附件资源所有权解析回归测试。

    use super::*;
    use crate::attachment_assets::{generic_attachment_descriptor, stage_attachment_bytes};
    use tempfile::tempdir;

    /// Stages and assigns one immutable fixture to a canonical conversation message.
    /// 暂存一个不可变夹具并将其分配给一条规范会话消息。
    fn owned_fixture() -> (tempfile::TempDir, ConversationAttachment) {
        let workspace = tempdir().expect("temporary workspace should be created");
        let workspace_path = workspace.path().to_string_lossy();
        let attachment = stage_attachment_bytes(
            workspace_path.as_ref(),
            &generic_attachment_descriptor(),
            "fixture.bin",
            None,
            b"owned immutable bytes",
        )
        .expect("attachment should be staged");
        promote_staged_attachment_to_message_ref(
            workspace_path.as_ref(),
            "session-owner",
            "message-1",
            std::slice::from_ref(&attachment),
            &[],
        )
        .expect("attachment should be assigned to the conversation");
        (workspace, attachment)
    }

    /// Metadata admission retains original digest and ownership without reading content before policy checks.
    /// 元数据准入保留原始摘要和所有权，不在策略检查之前读取内容。
    #[test]
    fn owned_resource_metadata_defers_content_verification_without_losing_identity() {
        // One temporary, already promoted file supplies the canonical session owner.
        // 一个临时、已提升文件提供规范会话所有者。
        let (workspace, attachment) = owned_fixture();
        // The fixture's exact asset id is the only lookup authority.
        // 夹具精确资产ID是唯一查询权威。
        let reference = format!(
            "attachment://{}",
            attachment.asset_id.as_deref().expect("fixture asset")
        );
        // Read the original registered proof before fault injection.
        // 故障注入之前读取原注册证明。
        let original = resolve_owned_attachment_resource_metadata(
            &workspace.path().to_string_lossy(),
            "session-owner",
            &reference,
        )
        .expect("owned metadata");
        assert_eq!(original.source_sha256, attachment.source_sha256);
        assert_eq!(original.mime_type, attachment.mime_type);
        // Same-length corruption distinguishes metadata-only admission from the legacy full integrity check.
        // 同长度损坏区分仅元数据准入和旧完整校验。
        fs::write(
            &original.local_path,
            vec![b'x'; original.byte_len.expect("fixture size") as usize],
        )
        .expect("fixture mutation");
        assert_eq!(
            resolve_owned_attachment_resource_metadata(
                &workspace.path().to_string_lossy(),
                "session-owner",
                &reference
            )
            .expect("metadata does not read bytes")
            .source_sha256,
            original.source_sha256
        );
        // Fixed media facades retain their disclosed project-relative form with the same metadata-first ordering.
        // 固定媒体外壳以相同元数据优先顺序保留其披露的项目相对形态。
        assert_eq!(
            resolve_owned_attachment_resource_metadata_for_kind(
                &workspace.path().to_string_lossy(),
                "session-owner",
                original
                    .resource_path
                    .as_deref()
                    .expect("canonical resource path"),
                ConversationAttachmentKind::File
            )
            .expect("fixed tool metadata")
            .source_sha256,
            original.source_sha256
        );
        assert!(
            resolve_owned_attachment_resource_metadata_for_kind(
                &workspace.path().to_string_lossy(),
                "session-owner",
                &reference,
                ConversationAttachmentKind::Image
            )
            .is_err()
        );
        assert!(
            resolve_owned_attachment_resource(
                &workspace.path().to_string_lossy(),
                "session-owner",
                &reference,
                attachment.kind
            )
            .expect_err("full verification still rejects corruption")
            .contains("canonical digest")
        );
        assert!(
            resolve_owned_attachment_resource_metadata(
                &workspace.path().to_string_lossy(),
                "session-other",
                &reference
            )
            .expect_err("other owner")
            .contains("not owned")
        );
        for invalid in [
            format!(" {reference}"),
            format!("{reference}/other"),
            "attachment:\\alias".to_string(),
        ] {
            assert!(
                resolve_owned_attachment_resource_metadata(
                    &workspace.path().to_string_lossy(),
                    "session-owner",
                    &invalid
                )
                .is_err()
            );
        }
    }

    /// Verifies both stable locator forms resolve only for the owning session and exact modality.
    /// 验证两种稳定定位符只会为所属会话和精确模态解析。
    #[test]
    fn owned_resource_accepts_safe_id_and_project_relative_path() {
        let (workspace, attachment) = owned_fixture();
        let workspace_path = workspace.path().to_string_lossy();
        let resource_path = attachment
            .resource_path
            .as_deref()
            .expect("staged attachment should have a resource path");
        let asset_locator = format!(
            "attachment://{}",
            attachment
                .asset_id
                .as_deref()
                .expect("staged attachment should have an asset id")
        );
        let indexed = load_attachment_asset_record_for_attachment(&attachment)
            .expect("staged attachment should remain indexed");
        let recomputed_resource_path = crate::attachment_assets::managed_attachment_resource_path(
            workspace_path.as_ref(),
            Path::new(indexed.managed_path.as_str()),
        )
        .expect("indexed path should produce a safe locator");
        assert_eq!(recomputed_resource_path, resource_path);
        let asset_root = resolve_managed_attachment_root(workspace_path.as_ref());
        let document = load_attachment_asset_index_document(&asset_root)
            .expect("workspace attachment index should load");
        assert!(document.assets.iter().any(|record| {
            crate::attachment_assets::managed_attachment_resource_path(
                workspace_path.as_ref(),
                Path::new(record.managed_path.as_str()),
            )
            .is_ok_and(|candidate| candidate == resource_path)
        }));

        let by_path = resolve_owned_attachment_resource(
            workspace_path.as_ref(),
            "session-owner",
            resource_path,
            ConversationAttachmentKind::File,
        )
        .expect("project-relative path should resolve");
        let by_id = resolve_owned_attachment_resource(
            workspace_path.as_ref(),
            "session-owner",
            asset_locator.as_str(),
            ConversationAttachmentKind::File,
        )
        .expect("asset locator should resolve");
        assert_eq!(by_path.asset_id, attachment.asset_id);
        assert_eq!(by_id.asset_id, attachment.asset_id);

        assert!(
            resolve_owned_attachment_resource(
                workspace_path.as_ref(),
                "session-other",
                resource_path,
                ConversationAttachmentKind::File,
            )
            .is_err()
        );
        assert!(
            resolve_owned_attachment_resource(
                workspace_path.as_ref(),
                "session-owner",
                resource_path,
                ConversationAttachmentKind::Image,
            )
            .is_err()
        );
    }

    /// Verifies a host draft can resolve an unpromoted staged asset only through its exact opaque asset locator.
    /// 验证宿主草稿只能通过精确不透明资产定位符解析尚未提升的暂存资产。
    #[test]
    fn indexed_resource_accepts_unpromoted_asset_id_only() {
        let workspace = tempdir().expect("temporary workspace should be created");
        let workspace_path = workspace.path().to_string_lossy();
        let attachment = stage_attachment_bytes(
            workspace_path.as_ref(),
            &crate::attachment_assets::attachment_descriptor_from_mime_type("image/png")
                .expect("png descriptor should resolve"),
            "draft.png",
            None,
            b"draft immutable bytes",
        )
        .expect("draft attachment should stage");
        let asset_locator = format!(
            "attachment://{}",
            attachment
                .asset_id
                .as_deref()
                .expect("staged attachment should have an asset id")
        );

        let resolved = resolve_indexed_attachment_resource(
            workspace_path.as_ref(),
            asset_locator.as_str(),
            ConversationAttachmentKind::Image,
        )
        .expect("unpromoted asset id should resolve for host draft mutation");
        assert_eq!(resolved.asset_id, attachment.asset_id);
        assert!(
            resolve_indexed_attachment_resource(
                workspace_path.as_ref(),
                attachment
                    .resource_path
                    .as_deref()
                    .expect("staged attachment should have a resource path"),
                ConversationAttachmentKind::Image,
            )
            .is_err()
        );
    }

    /// Verifies absolute, traversing, and content-mutated resources fail closed.
    /// 验证绝对路径、上级遍历与内容被替换的资源都会封闭失败。
    #[test]
    fn owned_resource_rejects_unsafe_or_mutated_inputs() {
        let (workspace, attachment) = owned_fixture();
        let workspace_path = workspace.path().to_string_lossy();
        assert!(
            resolve_owned_attachment_resource(
                workspace_path.as_ref(),
                "session-owner",
                attachment.local_path.as_str(),
                ConversationAttachmentKind::File,
            )
            .is_err()
        );
        assert!(
            resolve_owned_attachment_resource(
                workspace_path.as_ref(),
                "session-owner",
                "../fixture.bin",
                ConversationAttachmentKind::File,
            )
            .is_err()
        );

        fs::write(attachment.local_path.as_str(), b"owned immutable bytez")
            .expect("managed fixture should be overwritten for integrity testing");
        assert!(
            resolve_owned_attachment_resource(
                workspace_path.as_ref(),
                "session-owner",
                attachment
                    .resource_path
                    .as_deref()
                    .expect("staged attachment should have a resource path"),
                ConversationAttachmentKind::File,
            )
            .is_err()
        );
        assert_eq!(
            resolve_attachment_replay_status(&attachment),
            AttachmentReplayStatus::Missing,
            "ordinary Provider replay must reject the same-length mutated managed copy"
        );
    }
}
