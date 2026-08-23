use super::*;

/// Persists one provider-internal reference under the immutable managed attachment record.
/// 在不可变托管附件记录下持久化一条供应商内部引用。
///
/// # Parameters
/// 参数
/// `attachment` identifies the exact managed source and `reference` carries one frozen route scope.
/// `attachment` 标识精确托管来源，`reference` 携带一个冻结路线作用域。
///
/// # Returns
/// 返回值
/// Success after an atomic asset-index update, or an explicit identity error.
/// 原子更新资产索引后返回成功，否则返回明确身份错误。
pub fn record_attachment_media_reference(
    attachment: &ConversationAttachment,
    reference: AttachmentMediaReferenceRecord,
) -> Result<(), String> {
    validate_attachment_media_reference(attachment, &reference)?;
    if reference.provider_reference.trim().is_empty()
        || reference.provider_reference != reference.provider_reference.trim()
        || reference.provider_reference.chars().any(char::is_control)
    {
        return Err(String::from(
            "Provider media reference value is empty or unsafe for durable storage.",
        ));
    }
    let asset_id = attachment_asset_id(attachment)?;
    let asset_root = resolve_asset_root_from_attachment(attachment).ok_or_else(|| {
        String::from("Managed attachment media reference has no authoritative asset root.")
    })?;
    let transaction_lock = attachment_index_transaction_lock(&asset_root)?;
    let _transaction_guard = transaction_lock.lock().map_err(|error| {
        format!(
            "failed to lock attachment index transaction `{}`: {error}",
            asset_root.display()
        )
    })?;
    let mut document = load_attachment_asset_index_document(&asset_root)?;
    let record = document
        .assets
        .iter_mut()
        .find(|record| record.asset_id == asset_id)
        .ok_or_else(|| format!("Managed attachment asset `{asset_id}` is not indexed."))?;
    if record.source_sha256.as_deref() != Some(reference.source_sha256.as_str())
        || record.byte_len != Some(reference.source_byte_len)
    {
        return Err(String::from(
            "Provider media reference source identity does not match the authoritative asset record.",
        ));
    }
    record
        .media_references
        .retain(|existing| !existing.matches_route_identity(&reference));
    record.media_references.push(reference);
    save_attachment_asset_index_document(&asset_root, &document)
}

/// Finds one unexpired provider-internal reference for an exact managed source and route scope.
/// 为精确托管来源与路线作用域查找一条未过期的供应商内部引用。
///
/// # Parameters
/// 参数
/// `attachment` supplies the authoritative asset identity, `expected` supplies the exact route key,
/// and `now_unix_millis` closes TTL validation.
/// `attachment` 提供权威资产身份，`expected` 提供精确路线键，`now_unix_millis` 封闭 TTL 校验。
///
/// # Returns
/// 返回值
/// A cloned internal reference when every identity field matches and TTL remains valid.
/// 当每个身份字段匹配且 TTL 仍有效时返回克隆的内部引用。
pub fn find_attachment_media_reference(
    attachment: &ConversationAttachment,
    expected: &AttachmentMediaReferenceRecord,
    now_unix_millis: u64,
) -> Result<Option<AttachmentMediaReferenceRecord>, String> {
    validate_attachment_media_reference(attachment, expected)?;
    let asset_id = attachment_asset_id(attachment)?;
    let Some(asset_root) = resolve_asset_root_from_attachment(attachment) else {
        return Ok(None);
    };
    let transaction_lock = attachment_index_transaction_lock(&asset_root)?;
    let _transaction_guard = transaction_lock.lock().map_err(|error| {
        format!(
            "failed to lock attachment index transaction `{}`: {error}",
            asset_root.display()
        )
    })?;
    let document = load_attachment_asset_index_document(&asset_root)?;
    let Some(record) = document
        .assets
        .iter()
        .find(|record| record.asset_id == asset_id)
    else {
        return Ok(None);
    };
    if record.source_sha256.as_deref() != Some(expected.source_sha256.as_str())
        || record.byte_len != Some(expected.source_byte_len)
    {
        return Ok(None);
    }
    Ok(record
        .media_references
        .iter()
        .find(|reference| {
            reference.matches_route_identity(expected)
                && reference.created_at_unix_millis <= now_unix_millis
                && now_unix_millis < reference.expires_at_unix_millis
                && !reference.provider_reference.trim().is_empty()
                && reference.provider_reference == reference.provider_reference.trim()
                && !reference.provider_reference.chars().any(char::is_control)
        })
        .cloned())
}

/// Returns the required stable asset id from one canonical attachment.
/// 从一条规范附件返回必需的稳定资产标识。
fn attachment_asset_id(attachment: &ConversationAttachment) -> Result<&str, String> {
    attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("Managed attachment media reference requires an asset id."))
}

/// Validates one internal reference against immutable attachment source facts.
/// 针对不可变附件来源事实校验一条内部引用。
fn validate_attachment_media_reference(
    attachment: &ConversationAttachment,
    reference: &AttachmentMediaReferenceRecord,
) -> Result<(), String> {
    if attachment.source_sha256.as_deref() != Some(reference.source_sha256.as_str())
        || attachment.byte_len != Some(reference.source_byte_len)
    {
        return Err(String::from(
            "Provider media reference does not match the canonical attachment digest and size.",
        ));
    }
    if !is_exact_media_reference_identity(&reference.provider_id)
        || !is_exact_media_reference_identity(&reference.service_entry_id)
        || !is_exact_media_reference_identity(&reference.upload_channel_id)
        || !is_exact_media_reference_identity(&reference.upload_contract_fingerprint)
        || !is_exact_media_reference_identity(&reference.resource_kind)
        || !is_exact_media_reference_identity(&reference.mime_type)
        || reference.created_at_unix_millis >= reference.expires_at_unix_millis
    {
        return Err(String::from(
            "Provider media reference carries an incomplete exact route identity.",
        ));
    }
    match reference.reference_scope {
        SessionMediaReferenceScope::ProviderProtocol
            if reference.connection_id.is_none() && reference.session_id.is_none() => {}
        SessionMediaReferenceScope::Credential
            if reference
                .connection_id
                .as_deref()
                .is_some_and(is_exact_media_reference_identity)
                && reference.session_id.is_none() => {}
        SessionMediaReferenceScope::Session
            if reference.connection_id.is_none()
                && reference
                    .session_id
                    .as_deref()
                    .is_some_and(is_exact_media_reference_identity) => {}
        _ => {
            return Err(String::from(
                "Provider media reference carries an invalid reuse-scope identity.",
            ));
        }
    }
    Ok(())
}

/// Validates one exact non-secret identity component without normalizing persisted scope.
/// 校验一个精确的非秘密身份分量，且不规范化持久化作用域。
///
/// # Parameters
/// 参数
/// `value` is one route identity component.
/// `value` 是一个路线身份分量。
///
/// # Returns
/// 返回值
/// `true` when the value is non-empty, already trimmed, and contains no control character.
/// 当值非空、已经去除首尾空白且不含控制字符时返回 `true`。
fn is_exact_media_reference_identity(value: &str) -> bool {
    !value.is_empty() && value == value.trim() && !value.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    //! Durable attachment media-reference regression tests.
    //! 持久化附件媒体引用回归测试。

    use super::*;
    use crate::attachment_assets::{generic_attachment_descriptor, stage_attachment_bytes};
    use tempfile::tempdir;

    /// Builds one exact route-bound reference for a staged attachment.
    /// 为一个已暂存附件构建一条精确路由绑定引用。
    fn test_reference(
        attachment: &ConversationAttachment,
        provider_id: &str,
        provider_reference: &str,
        created_at_unix_millis: u64,
        expires_at_unix_millis: u64,
    ) -> AttachmentMediaReferenceRecord {
        AttachmentMediaReferenceRecord {
            source_sha256: attachment
                .source_sha256
                .clone()
                .expect("staged attachment must have a digest"),
            source_byte_len: attachment
                .byte_len
                .expect("staged attachment must have a byte length"),
            provider_id: provider_id.to_string(),
            service_entry_id: String::from("public-api"),
            upload_channel_id: String::from("file-upload"),
            upload_contract_fingerprint: String::from("sha256:contract"),
            reference_scope: SessionMediaReferenceScope::ProviderProtocol,
            connection_id: None,
            session_id: None,
            resource_kind: String::from("file"),
            mime_type: attachment.mime_type.clone(),
            provider_reference: provider_reference.to_string(),
            created_at_unix_millis,
            expires_at_unix_millis,
        }
    }

    /// Verifies restart-safe lookup requires exact route identity and an active TTL.
    /// 验证可跨重启查找必须满足精确路由身份与有效 TTL。
    #[test]
    fn durable_reference_lookup_is_route_and_ttl_scoped() {
        let workspace = tempdir().expect("temporary workspace should be created");
        let workspace_path = workspace.path().to_string_lossy();
        let attachment = stage_attachment_bytes(
            workspace_path.as_ref(),
            &generic_attachment_descriptor(),
            "fixture.bin",
            None,
            b"immutable attachment bytes",
        )
        .expect("attachment should be staged");
        let stored = test_reference(&attachment, "provider-a", "file://provider-a", 100, 200);
        record_attachment_media_reference(&attachment, stored.clone())
            .expect("reference should be persisted");

        let found = find_attachment_media_reference(&attachment, &stored, 150)
            .expect("lookup should succeed")
            .expect("unexpired exact route should be reusable");
        assert_eq!(found.provider_reference, "file://provider-a");

        let wrong_route = test_reference(&attachment, "provider-b", "unused", 100, 200);
        assert!(
            find_attachment_media_reference(&attachment, &wrong_route, 150)
                .expect("mismatched lookup should remain controlled")
                .is_none()
        );
        assert!(
            find_attachment_media_reference(&attachment, &stored, 200)
                .expect("expired lookup should remain controlled")
                .is_none()
        );
    }

    /// Verifies one exact route key atomically replaces its stale Provider reference.
    /// 验证同一精确路由键会原子替换其过时供应商引用。
    #[test]
    fn durable_reference_replaces_the_same_route_key() {
        let workspace = tempdir().expect("temporary workspace should be created");
        let workspace_path = workspace.path().to_string_lossy();
        let attachment = stage_attachment_bytes(
            workspace_path.as_ref(),
            &generic_attachment_descriptor(),
            "fixture.bin",
            None,
            b"replacement attachment bytes",
        )
        .expect("attachment should be staged");
        let first = test_reference(&attachment, "provider-a", "file://old", 100, 200);
        let replacement = test_reference(&attachment, "provider-a", "file://new", 150, 300);
        record_attachment_media_reference(&attachment, first)
            .expect("first reference should be persisted");
        record_attachment_media_reference(&attachment, replacement.clone())
            .expect("replacement reference should be persisted");

        let found = find_attachment_media_reference(&attachment, &replacement, 250)
            .expect("lookup should succeed")
            .expect("replacement reference should remain active");
        assert_eq!(found.provider_reference, "file://new");
    }

    /// Verifies route identity comparison cannot collide through delimiter-bearing fields.
    /// 验证路线身份比较不会因字段携带分隔符而发生碰撞。
    #[test]
    fn route_identity_comparison_is_field_exact() {
        let workspace = tempdir().expect("temporary workspace should be created");
        let workspace_path = workspace.path().to_string_lossy();
        let attachment = stage_attachment_bytes(
            workspace_path.as_ref(),
            &generic_attachment_descriptor(),
            "fixture.bin",
            None,
            b"route identity bytes",
        )
        .expect("attachment should be staged");
        let mut left = test_reference(&attachment, "provider|public", "left", 100, 200);
        left.service_entry_id = String::from("api");
        let mut right = test_reference(&attachment, "provider", "right", 100, 200);
        right.service_entry_id = String::from("public|api");

        assert!(!left.matches_route_identity(&right));
    }

    /// Verifies an empty TTL window and non-canonical identity text fail closed.
    /// 验证空 TTL 窗口与非规范身份文本会封闭失败。
    #[test]
    fn durable_reference_rejects_empty_ttl_and_noncanonical_identity() {
        let workspace = tempdir().expect("temporary workspace should be created");
        let workspace_path = workspace.path().to_string_lossy();
        let attachment = stage_attachment_bytes(
            workspace_path.as_ref(),
            &generic_attachment_descriptor(),
            "fixture.bin",
            None,
            b"invalid reference bytes",
        )
        .expect("attachment should be staged");
        let zero_ttl = test_reference(&attachment, "provider-a", "zero-ttl", 100, 100);
        assert!(record_attachment_media_reference(&attachment, zero_ttl).is_err());

        let noncanonical = test_reference(&attachment, " provider-a", "whitespace", 100, 200);
        assert!(record_attachment_media_reference(&attachment, noncanonical).is_err());
    }
}
