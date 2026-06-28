use super::*;

/// Records one newly staged managed attachment copy in the durable asset index.
/// 将一份新暂存的托管附件副本记录到持久化资产索引中。
pub fn record_staged_attachment(
    workspace_path: &str,
    attachment: &ConversationAttachment,
    timestamp: &AttachmentTimestampParts,
) -> Result<(), String> {
    let Some(asset_id) = attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let mut document = load_attachment_asset_index_document(&asset_root)?;
    if let Some(existing_record) = document
        .assets
        .iter_mut()
        .find(|record| record.asset_id == asset_id)
    {
        existing_record.kind = attachment.kind.clone();
        existing_record.original_file_name = attachment.file_name.clone();
        existing_record.managed_path = attachment.local_path.clone();
        existing_record.mime_type = attachment.mime_type.clone();
        existing_record.byte_len = attachment.byte_len;
        existing_record.staged_at_local_text = timestamp.local_timestamp_text.clone();
        existing_record.staged_date_text = timestamp.local_date_text.clone();
        existing_record.staged_at_unix_millis = timestamp.unix_millis;
        existing_record.source_path = attachment.source_path.clone();
        existing_record.purged_at_local_text = None;
        existing_record.purged_at_unix_millis = None;
        existing_record.purge_reason = None;
    } else {
        document.assets.push(AttachmentAssetRecord {
            asset_id: asset_id.to_string(),
            kind: attachment.kind.clone(),
            original_file_name: attachment.file_name.clone(),
            managed_path: attachment.local_path.clone(),
            mime_type: attachment.mime_type.clone(),
            byte_len: attachment.byte_len,
            staged_at_local_text: timestamp.local_timestamp_text.clone(),
            staged_date_text: timestamp.local_date_text.clone(),
            staged_at_unix_millis: timestamp.unix_millis,
            source_path: attachment.source_path.clone(),
            session_ids: Vec::new(),
            message_ids: Vec::new(),
            staging_refs: Vec::new(),
            purged_at_local_text: None,
            purged_at_unix_millis: None,
            purge_reason: None,
        });
    }
    save_attachment_asset_index_document(&asset_root, &document)
}

/// Adds one transient staging reference scope to each provided managed attachment.
/// 为每条给定托管附件增加一个瞬时暂存引用作用域。
pub fn acquire_staged_attachment_ref(
    workspace_path: &str,
    scope: &str,
    attachments: &[ConversationAttachment],
) -> Result<bool, String> {
    update_staged_attachment_refs(workspace_path, scope, attachments, true)
}

/// Removes one transient staging reference scope and deletes assets that become fully unreferenced.
/// 移除一个瞬时暂存引用作用域，并删除因此变成完全未引用的资产。
pub fn release_staged_attachment_ref(
    workspace_path: &str,
    scope: &str,
    attachments: &[ConversationAttachment],
) -> Result<bool, String> {
    update_staged_attachment_refs(workspace_path, scope, attachments, false)
}

/// Promotes staged attachments into canonical message references and releases their transient staging scopes.
/// 将已暂存附件提升为规范消息引用，并释放其瞬时暂存作用域。
pub fn promote_staged_attachment_to_message_ref(
    workspace_path: &str,
    session_id: &str,
    message_id: &str,
    attachments: &[ConversationAttachment],
    released_staging_scopes: &[String],
) -> Result<(), String> {
    if attachments.is_empty() {
        return Ok(());
    }
    let released_staging_scopes = released_staging_scopes
        .iter()
        .map(|scope| scope.trim())
        .filter(|scope| !scope.is_empty())
        .collect::<BTreeSet<_>>();
    let asset_ids = collect_attachment_asset_ids(attachments)?;
    if asset_ids.is_empty() {
        return Ok(());
    }
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let mut document = load_attachment_asset_index_document(&asset_root)?;
    ensure_attachment_asset_ids_are_indexed(&document, &asset_ids, "提升暂存附件引用")?;
    let mut changed = false;

    for record in document
        .assets
        .iter_mut()
        .filter(|record| asset_ids.contains(&record.asset_id))
    {
        if !record
            .session_ids
            .iter()
            .any(|existing| existing == session_id)
        {
            record.session_ids.push(session_id.to_string());
            changed = true;
        }
        if !record
            .message_ids
            .iter()
            .any(|existing| existing == message_id)
        {
            record.message_ids.push(message_id.to_string());
            changed = true;
        }
        if !released_staging_scopes.is_empty() && !record.staging_refs.is_empty() {
            let previous_len = record.staging_refs.len();
            record
                .staging_refs
                .retain(|scope| !released_staging_scopes.contains(scope.as_str()));
            if record.staging_refs.len() != previous_len {
                changed = true;
            }
        }
    }

    if changed {
        save_attachment_asset_index_document(&asset_root, &document)?;
    }
    Ok(())
}

/// Removes one staged managed attachment that was never referenced by a persisted conversation message.
/// 移除一条尚未被持久化会话消息引用过的已暂存托管附件。
pub fn discard_unreferenced_staged_attachment(
    workspace_path: &str,
    attachment: &ConversationAttachment,
) -> Result<bool, String> {
    let Some(asset_id) = attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(false);
    };
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let mut document = load_attachment_asset_index_document(&asset_root)?;
    let Some(index) = document
        .assets
        .iter()
        .position(|record| record.asset_id == asset_id)
    else {
        return Ok(false);
    };
    if attachment_record_has_refs(&document.assets[index]) {
        return Ok(false);
    }

    if let CleanupManagedPath::Existing(managed_path) =
        validate_cleanup_managed_path(&asset_root, &document.assets[index])?
    {
        let preview_cleanup_attachment = ConversationAttachment {
            kind: document.assets[index].kind.clone(),
            file_name: document.assets[index].original_file_name.clone(),
            local_path: managed_path.to_string_lossy().into_owned(),
            source_path: None,
            asset_id: Some(document.assets[index].asset_id.clone()),
            byte_len: document.assets[index].byte_len,
            preview_local_path: None,
            mime_type: document.assets[index].mime_type.clone(),
        };
        fs::remove_file(&managed_path).map_err(|error| {
            format!(
                "删除未引用托管附件 `{}` 失败：{}。",
                managed_path.display(),
                error
            )
        })?;
        remove_attachment_preview_sidecar(&preview_cleanup_attachment);
    } else {
        let preview_cleanup_attachment = ConversationAttachment {
            kind: document.assets[index].kind.clone(),
            file_name: document.assets[index].original_file_name.clone(),
            local_path: document.assets[index].managed_path.clone(),
            source_path: None,
            asset_id: Some(document.assets[index].asset_id.clone()),
            byte_len: document.assets[index].byte_len,
            preview_local_path: None,
            mime_type: document.assets[index].mime_type.clone(),
        };
        remove_attachment_preview_sidecar(&preview_cleanup_attachment);
    }

    document.assets.remove(index);
    save_attachment_asset_index_document(&asset_root, &document)?;
    Ok(true)
}

/// Updates staging reference scopes for a batch of managed attachments.
/// 批量更新托管附件的暂存引用作用域。
pub(super) fn update_staged_attachment_refs(
    workspace_path: &str,
    scope: &str,
    attachments: &[ConversationAttachment],
    acquire: bool,
) -> Result<bool, String> {
    let scope = scope.trim();
    if scope.is_empty() || attachments.is_empty() {
        return Ok(false);
    }
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let mut document = load_attachment_asset_index_document(&asset_root)?;
    let mut changed = false;
    let asset_ids = collect_attachment_asset_ids(attachments)?;
    if asset_ids.is_empty() {
        return Ok(false);
    }
    if acquire {
        ensure_attachment_asset_ids_are_indexed(&document, &asset_ids, "记录暂存附件租约")?;
    }

    for record in document
        .assets
        .iter_mut()
        .filter(|record| asset_ids.contains(&record.asset_id))
    {
        if acquire {
            if !record.staging_refs.iter().any(|existing| existing == scope) {
                record.staging_refs.push(scope.to_string());
                changed = true;
            }
        } else {
            let previous_len = record.staging_refs.len();
            record.staging_refs.retain(|existing| existing != scope);
            if record.staging_refs.len() != previous_len {
                changed = true;
            }
        }
    }

    if !acquire {
        let removable_indices = document
            .assets
            .iter()
            .enumerate()
            .filter_map(|(index, record)| {
                (asset_ids.contains(&record.asset_id) && !attachment_record_has_refs(record))
                    .then_some(index)
            })
            .collect::<Vec<_>>();
        for index in removable_indices.into_iter().rev() {
            discard_attachment_record_at(&asset_root, &mut document, index)?;
            changed = true;
        }
    }

    if changed {
        save_attachment_asset_index_document(&asset_root, &document)?;
    }
    Ok(changed)
}

/// Deletes one fully unreferenced asset record and its managed file when the file still exists.
/// 删除一条完全未引用的资产记录，并在托管文件仍存在时同步删除该文件。
pub(super) fn discard_attachment_record_at(
    asset_root: &Path,
    document: &mut AttachmentAssetIndexDocument,
    index: usize,
) -> Result<(), String> {
    if attachment_record_has_refs(&document.assets[index]) {
        return Ok(());
    }
    if let CleanupManagedPath::Existing(managed_path) =
        validate_cleanup_managed_path(asset_root, &document.assets[index])?
    {
        let preview_cleanup_attachment = ConversationAttachment {
            kind: document.assets[index].kind.clone(),
            file_name: document.assets[index].original_file_name.clone(),
            local_path: managed_path.to_string_lossy().into_owned(),
            source_path: None,
            asset_id: Some(document.assets[index].asset_id.clone()),
            byte_len: document.assets[index].byte_len,
            preview_local_path: None,
            mime_type: document.assets[index].mime_type.clone(),
        };
        fs::remove_file(&managed_path).map_err(|error| {
            format!(
                "删除未引用托管附件 `{}` 失败：{}。",
                managed_path.display(),
                error
            )
        })?;
        remove_attachment_preview_sidecar(&preview_cleanup_attachment);
    } else {
        let preview_cleanup_attachment = ConversationAttachment {
            kind: document.assets[index].kind.clone(),
            file_name: document.assets[index].original_file_name.clone(),
            local_path: document.assets[index].managed_path.clone(),
            source_path: None,
            asset_id: Some(document.assets[index].asset_id.clone()),
            byte_len: document.assets[index].byte_len,
            preview_local_path: None,
            mime_type: document.assets[index].mime_type.clone(),
        };
        remove_attachment_preview_sidecar(&preview_cleanup_attachment);
    }
    document.assets.remove(index);
    Ok(())
}

/// Returns the stable asset id for one attachment when it can participate in the managed index.
/// 当附件可参与托管索引时，返回它的稳定资产标识。
pub(super) fn attachment_asset_id(attachment: &ConversationAttachment) -> Option<String> {
    attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Collects managed asset identifiers from canonical attachment payloads and rejects malformed managed entries.
/// 从规范附件载荷中收集托管资产标识，并拒绝缺少标识的异常托管条目。
pub(super) fn collect_attachment_asset_ids(
    attachments: &[ConversationAttachment],
) -> Result<BTreeSet<String>, String> {
    let mut asset_ids = BTreeSet::new();
    let mut missing_labels = Vec::new();
    for attachment in attachments {
        if let Some(asset_id) = attachment_asset_id(attachment) {
            asset_ids.insert(asset_id);
        } else {
            missing_labels.push(attachment.file_name.clone());
        }
    }
    if !missing_labels.is_empty() {
        return Err(format!(
            "附件 `{}` 缺少资产标识，请重新添加附件后再试。",
            missing_labels.join("`, `")
        ));
    }
    Ok(asset_ids)
}

/// Ensures every referenced managed asset id still has a persistent index record before mutating leases.
/// 在变更租约前，确保每个被引用的托管资产标识仍存在持久索引记录。
pub(super) fn ensure_attachment_asset_ids_are_indexed(
    document: &AttachmentAssetIndexDocument,
    asset_ids: &BTreeSet<String>,
    operation_label: &str,
) -> Result<(), String> {
    let indexed_ids = document
        .assets
        .iter()
        .map(|record| record.asset_id.as_str())
        .collect::<BTreeSet<_>>();
    let missing_ids = asset_ids
        .iter()
        .filter(|asset_id| !indexed_ids.contains(asset_id.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if missing_ids.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{operation_label}失败：附件资产索引缺少 `{}`，请重新添加附件后再试。",
        missing_ids.join("`, `")
    ))
}

/// Returns whether an indexed attachment is still retained by any durable or transient reference.
/// 返回索引附件是否仍被任何持久或瞬时引用保留。
pub(super) fn attachment_record_has_refs(record: &AttachmentAssetRecord) -> bool {
    !record.session_ids.is_empty()
        || !record.message_ids.is_empty()
        || !record.staging_refs.is_empty()
}

/// Returns whether an indexed attachment is still retained by a transient composer, queue, or undo owner.
/// 返回索引附件是否仍被输入框、队列或撤销栈等瞬时所有者持有。
pub(super) fn attachment_record_has_transient_refs(record: &AttachmentAssetRecord) -> bool {
    !record.staging_refs.is_empty()
}
