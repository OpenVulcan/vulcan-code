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
    // transaction_lock serializes this index's complete load-modify-save cycle across staging tasks.
    // transaction_lock 串行化当前索引跨暂存任务的完整读取、变更与保存周期。
    let transaction_lock = attachment_index_transaction_lock(&asset_root)?;
    // transaction_guard remains held until the updated durable document has been published.
    // transaction_guard 保持到更新后的持久文档完成发布。
    let _transaction_guard = transaction_lock.lock().map_err(|error| {
        format!(
            "failed to lock attachment index transaction `{}`: {error}",
            asset_root.display()
        )
    })?;
    let mut document = load_attachment_asset_index_document_for_mutation(&asset_root)?;
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
        existing_record.deletion_pending = false;
        existing_record.purged_at_local_text = None;
        existing_record.purged_at_unix_millis = None;
        existing_record.purge_reason = None;
    } else {
        document.assets.push(AttachmentAssetRecord {
            asset_id: asset_id.to_string(),
            kind: attachment.kind.clone(),
            original_file_name: attachment.file_name.clone(),
            managed_path: vulcan_runtime_assets::native_host_path_text(&attachment.local_path),
            mime_type: attachment.mime_type.clone(),
            byte_len: attachment.byte_len,
            staged_at_local_text: timestamp.local_timestamp_text.clone(),
            staged_date_text: timestamp.local_date_text.clone(),
            staged_at_unix_millis: timestamp.unix_millis,
            source_path: attachment
                .source_path
                .as_deref()
                .map(vulcan_runtime_assets::native_host_path_text),
            session_ids: Vec::new(),
            message_ids: Vec::new(),
            staging_refs: Vec::new(),
            deletion_pending: false,
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
    // transaction_lock prevents reference promotion from overwriting a concurrent stage or cleanup update.
    // transaction_lock 防止引用提升覆盖并发暂存或清理更新。
    let transaction_lock = attachment_index_transaction_lock(&asset_root)?;
    // transaction_guard covers validation, mutation, and optional persistence as one critical section.
    // transaction_guard 将校验、变更与可选持久化覆盖为一个临界区。
    let _transaction_guard = transaction_lock.lock().map_err(|error| {
        format!(
            "failed to lock attachment index transaction `{}`: {error}",
            asset_root.display()
        )
    })?;
    let mut document = load_attachment_asset_index_document_for_mutation(&asset_root)?;
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
    // transaction_lock keeps reference checks, managed-file deletion, and index publication ordered.
    // transaction_lock 使引用检查、托管文件删除与索引发布保持有序。
    let transaction_lock = attachment_index_transaction_lock(&asset_root)?;
    // transaction_guard prevents a concurrent reference acquisition after the unreferenced check.
    // transaction_guard 防止并发引用在无引用检查之后被新增。
    let _transaction_guard = transaction_lock.lock().map_err(|error| {
        format!(
            "failed to lock attachment index transaction `{}`: {error}",
            asset_root.display()
        )
    })?;
    let mut document = load_attachment_asset_index_document_for_mutation(&asset_root)?;
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
    discard_attachment_record_at(&asset_root, &mut document, index)?;
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
    // transaction_lock serializes lease changes with every other mutation of this exact index.
    // transaction_lock 将租约变更与当前精确索引的全部其他变更串行化。
    let transaction_lock = attachment_index_transaction_lock(&asset_root)?;
    // transaction_guard spans missing-record validation, lease mutation, deletion, and persistence.
    // transaction_guard 覆盖缺失记录校验、租约变更、删除与持久化。
    let _transaction_guard = transaction_lock.lock().map_err(|error| {
        format!(
            "failed to lock attachment index transaction `{}`: {error}",
            asset_root.display()
        )
    })?;
    let mut document = load_attachment_asset_index_document_for_mutation(&asset_root)?;
    let mut changed = false;
    // persisted_during_discard tracks whether a two-phase deletion already saved all preceding lease mutations.
    // persisted_during_discard 记录两阶段删除是否已经保存全部先前租约变更。
    let mut persisted_during_discard = false;
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
            persisted_during_discard = true;
        }
    }

    if changed && !persisted_during_discard {
        save_attachment_asset_index_document(&asset_root, &document)?;
    }
    Ok(changed)
}

/// Durably plans and completes deletion of one fully unreferenced attachment asset.
/// 持久规划并完成一条完全未引用附件资产的删除。
pub(super) fn discard_attachment_record_at(
    asset_root: &Path,
    document: &mut AttachmentAssetIndexDocument,
    index: usize,
) -> Result<(), String> {
    if attachment_record_has_refs(&document.assets[index]) {
        return Ok(());
    }
    // discard_timestamp records the durable deletion intent before any managed file is removed.
    // discard_timestamp 在删除任何托管文件前记录持久删除意图。
    let discard_timestamp = current_attachment_timestamp_parts();
    {
        // record enters a replay-safe pending tombstone state before the irreversible file operation.
        // record 在不可逆文件操作前进入回放安全的待删除墓碑状态。
        let record = &mut document.assets[index];
        record.deletion_pending = true;
        record.purged_at_local_text = Some(discard_timestamp.local_timestamp_text);
        record.purged_at_unix_millis = Some(discard_timestamp.unix_millis);
        record.purge_reason = Some(String::from("discarded-unreferenced-staged-attachment"));
    }
    save_attachment_asset_index_document(asset_root, document)?;
    finish_pending_attachment_deletion_at(asset_root, document, index)
}

/// Loads one index for mutation and resumes every durable pending deletion first.
/// 加载一份用于变更的索引，并优先恢复每条持久待删除记录。
///
/// # Parameters
/// 参数
/// `asset_root` owns the exact attachment index currently protected by its transaction lock.
/// `asset_root` 拥有当前由事务锁保护的精确附件索引。
///
/// # Returns
/// 返回值
/// The recovered mutable index document after all pending deletions complete.
/// 全部待删除操作完成后的已恢复可变索引文档。
pub(super) fn load_attachment_asset_index_document_for_mutation(
    asset_root: &Path,
) -> Result<AttachmentAssetIndexDocument, String> {
    // document starts from the latest atomically published index image.
    // document 从最新原子发布的索引内容开始。
    let mut document = load_attachment_asset_index_document(asset_root)?;
    recover_pending_attachment_deletions(asset_root, &mut document)?;
    Ok(document)
}

/// Resumes every pending unreferenced-attachment deletion in reverse index order.
/// 按索引逆序恢复每条待处理的未引用附件删除。
///
/// # Parameters
/// 参数
/// `asset_root` owns the managed files and durable index.
/// `asset_root` 拥有托管文件与持久索引。
/// `document` is the latest index image mutated as recovery removes completed tombstones.
/// `document` 是恢复移除已完成墓碑时变更的最新索引内容。
///
/// # Returns
/// 返回值
/// `Ok(())` after every pending file deletion and record removal is durably committed.
/// 每条待处理文件删除与记录移除完成持久提交后返回 `Ok(())`。
fn recover_pending_attachment_deletions(
    asset_root: &Path,
    document: &mut AttachmentAssetIndexDocument,
) -> Result<(), String> {
    // pending_indices are reversed during execution so record removals never invalidate later positions.
    // pending_indices 在执行时逆序处理，使记录移除不会破坏后续位置。
    let pending_indices = document
        .assets
        .iter()
        .enumerate()
        .filter_map(|(index, record)| record.deletion_pending.then_some(index))
        .collect::<Vec<_>>();
    for index in pending_indices.into_iter().rev() {
        finish_pending_attachment_deletion_at(asset_root, document, index)?;
    }
    Ok(())
}

/// Finishes one already-persisted deletion tombstone and removes its index record.
/// 完成一条已经持久化的删除墓碑并移除其索引记录。
///
/// # Parameters
/// 参数
/// `asset_root` owns the validated managed file path.
/// `asset_root` 拥有已经校验的托管文件路径。
/// `document` contains the durable pending record.
/// `document` 包含持久待处理记录。
/// `index` selects the exact pending record to finish.
/// `index` 选择需要完成的精确待处理记录。
///
/// # Returns
/// 返回值
/// `Ok(())` after the file, preview sidecar, and pending record are all removed.
/// 文件、预览 sidecar 与待处理记录全部移除后返回 `Ok(())`。
fn finish_pending_attachment_deletion_at(
    asset_root: &Path,
    document: &mut AttachmentAssetIndexDocument,
    index: usize,
) -> Result<(), String> {
    if !document.assets[index].deletion_pending {
        return Ok(());
    }
    // planned_path is validated before deletion so corrupted index data cannot escape the managed root.
    // planned_path 在删除前完成校验，使损坏索引数据无法越出托管根目录。
    let planned_path = validate_cleanup_managed_path(asset_root, &document.assets[index])?;
    // preview_cleanup_attachment carries the exact path used to remove any deterministic preview sidecar.
    // preview_cleanup_attachment 携带用于删除确定性预览 sidecar 的精确路径。
    let preview_cleanup_attachment = ConversationAttachment {
        kind: document.assets[index].kind.clone(),
        file_name: document.assets[index].original_file_name.clone(),
        local_path: match &planned_path {
            CleanupManagedPath::Existing(managed_path) => {
                vulcan_runtime_assets::native_host_path_text_from_path(managed_path)
            }
            CleanupManagedPath::Missing => {
                vulcan_runtime_assets::native_host_path_text(&document.assets[index].managed_path)
            }
        },
        source_path: None,
        asset_id: Some(document.assets[index].asset_id.clone()),
        byte_len: document.assets[index].byte_len,
        preview_local_path: None,
        mime_type: document.assets[index].mime_type.clone(),
    };

    if let CleanupManagedPath::Existing(managed_path) = planned_path
        && let Err(error) = fs::remove_file(&managed_path)
    {
        // deletion_error remains authoritative while the updated tombstone records retry diagnostics.
        // deletion_error 保持权威性，同时更新后的墓碑记录重试诊断。
        let deletion_error = format!(
            "删除待处理托管附件 `{}` 失败：{}。",
            managed_path.display(),
            error
        );
        document.assets[index].purge_reason = Some(format!(
            "discarded-unreferenced-staged-attachment; delete-failed: {error}"
        ));
        if let Err(persist_error) = save_attachment_asset_index_document(asset_root, document) {
            return Err(format!(
                "{deletion_error} 同时保存待删除诊断失败：{persist_error}"
            ));
        }
        return Err(deletion_error);
    }

    remove_attachment_preview_sidecar(&preview_cleanup_attachment);
    document.assets.remove(index);
    save_attachment_asset_index_document(asset_root, document)
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
