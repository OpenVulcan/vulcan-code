use super::*;

pub(super) enum CleanupManagedPath {
    /// Managed file still exists and has been canonicalized inside the current attachment root.
    /// 托管文件仍然存在，并且已规范化到当前附件根目录内部。
    Existing(PathBuf),
    /// Managed file is already missing but its recorded path is still lexically inside the current attachment root.
    /// 托管文件已经缺失，但记录路径在词法上仍位于当前附件根目录内部。
    Missing,
}

/// Builds one cleanup preview for the provided workspace root, cutoff date, and attachment-kind selection.
/// 根据提供的工作区根目录、截止日期与附件类型选择构建一份清理预览。
pub fn preview_attachment_cleanup(
    workspace_path: &str,
    cutoff_date_text: &str,
    selection: AttachmentCleanupSelection,
) -> Result<AttachmentCleanupPreview, String> {
    let cutoff_date = parse_attachment_cleanup_cutoff_date(cutoff_date_text)?;
    let asset_root = resolve_managed_attachment_root(workspace_path);
    let document = load_attachment_asset_index_document(&asset_root)?;
    let cutoff_text = cutoff_date
        .format(ATTACHMENT_DATE_FORMAT)
        .map_err(|error| format!("格式化清理截止日期失败：{error}。"))?;

    let mut matched_asset_count = 0usize;
    let mut matched_byte_len = 0_u64;
    let mut impacted_sessions = BTreeSet::new();
    let mut per_kind_counts = BTreeMap::new();

    for record in document.assets.iter().filter(|record| {
        record.purged_at_unix_millis.is_none()
            && !attachment_record_has_transient_refs(record)
            && selection.includes(record.kind.clone())
            && record.staged_date_text.as_str() < cutoff_text.as_str()
    }) {
        matched_asset_count += 1;
        matched_byte_len = matched_byte_len.saturating_add(record.byte_len.unwrap_or(0));
        impacted_sessions.extend(record.session_ids.iter().cloned());
        *per_kind_counts
            .entry(kind_summary_key(record.kind.clone()))
            .or_insert(0) += 1;
    }

    Ok(AttachmentCleanupPreview {
        cutoff_date_text: cutoff_text,
        matched_asset_count,
        matched_byte_len,
        impacted_session_count: impacted_sessions.len(),
        per_kind_counts,
    })
}

/// Executes one destructive cleanup that purges managed attachment copies staged before the provided cutoff date.
/// 执行一次破坏性清理，清除在给定截止日期之前暂存的托管附件副本。
pub fn execute_attachment_cleanup(
    workspace_path: &str,
    cutoff_date_text: &str,
    selection: AttachmentCleanupSelection,
) -> Result<AttachmentCleanupReport, String> {
    let cutoff_date = parse_attachment_cleanup_cutoff_date(cutoff_date_text)?;
    let cutoff_text = cutoff_date
        .format(ATTACHMENT_DATE_FORMAT)
        .map_err(|error| format!("格式化清理截止日期失败：{error}。"))?;
    let asset_root = resolve_managed_attachment_root(workspace_path);
    // transaction_lock excludes staging and reference updates during the destructive cleanup transaction.
    // transaction_lock 在破坏性清理事务期间排除暂存与引用更新。
    let transaction_lock = attachment_index_transaction_lock(&asset_root)?;
    // transaction_guard covers selection, purge marking, file deletion, and final diagnostic persistence.
    // transaction_guard 覆盖选择、清理标记、文件删除与最终诊断持久化。
    let _transaction_guard = transaction_lock.lock().map_err(|error| {
        format!(
            "failed to lock attachment index transaction `{}`: {error}",
            asset_root.display()
        )
    })?;
    let mut document =
        super::lifecycle::load_attachment_asset_index_document_for_mutation(&asset_root)?;
    let purge_timestamp = current_attachment_timestamp_parts();
    let matched_indices = document
        .assets
        .iter()
        .enumerate()
        .filter_map(|(index, record)| {
            (record.purged_at_unix_millis.is_none()
                && !attachment_record_has_transient_refs(record)
                && selection.includes(record.kind.clone())
                && record.staged_date_text.as_str() < cutoff_text.as_str())
            .then_some(index)
        })
        .collect::<Vec<_>>();
    let planned_paths = matched_indices
        .iter()
        .map(|index| validate_cleanup_managed_path(&asset_root, &document.assets[*index]))
        .collect::<Result<Vec<_>, _>>()?;
    let mut purged_asset_count = 0usize;
    let mut deleted_file_count = 0usize;
    let mut already_missing_file_count = 0usize;
    let mut failed_delete_count = 0usize;
    let mut purged_byte_len = 0_u64;
    let mut impacted_sessions = BTreeSet::new();
    let mut delete_failure_reason_changed = false;

    for &index in &matched_indices {
        {
            let record = &mut document.assets[index];
            record.purged_at_local_text = Some(purge_timestamp.local_timestamp_text.clone());
            record.purged_at_unix_millis = Some(purge_timestamp.unix_millis);
            record.purge_reason = Some(format!("purged-before-{}", cutoff_text));
            record.deletion_pending = false;
        }
    }
    if !matched_indices.is_empty() {
        save_attachment_asset_index_document(&asset_root, &document)?;
    }

    for (index, planned_path) in matched_indices.into_iter().zip(planned_paths.into_iter()) {
        let record = &document.assets[index];
        purged_asset_count += 1;
        purged_byte_len = purged_byte_len.saturating_add(record.byte_len.unwrap_or(0));
        impacted_sessions.extend(record.session_ids.iter().cloned());

        match planned_path {
            CleanupManagedPath::Existing(managed_path) => match fs::remove_file(&managed_path) {
                Ok(()) => {
                    let preview_cleanup_attachment = ConversationAttachment {
                        kind: record.kind.clone(),
                        file_name: record.original_file_name.clone(),
                        local_path: vulcan_runtime_assets::native_host_path_text_from_path(
                            &managed_path,
                        ),
                        source_path: None,
                        asset_id: Some(record.asset_id.clone()),
                        byte_len: record.byte_len,
                        preview_local_path: None,
                        mime_type: record.mime_type.clone(),
                    };
                    remove_attachment_preview_sidecar(&preview_cleanup_attachment);
                    deleted_file_count += 1;
                }
                Err(error) => {
                    failed_delete_count += 1;
                    document.assets[index].purge_reason = Some(format!(
                        "purged-before-{}; delete-failed: {}",
                        cutoff_text, error
                    ));
                    delete_failure_reason_changed = true;
                }
            },
            CleanupManagedPath::Missing => {
                let preview_cleanup_attachment = ConversationAttachment {
                    kind: record.kind.clone(),
                    file_name: record.original_file_name.clone(),
                    local_path: vulcan_runtime_assets::native_host_path_text(&record.managed_path),
                    source_path: None,
                    asset_id: Some(record.asset_id.clone()),
                    byte_len: record.byte_len,
                    preview_local_path: None,
                    mime_type: record.mime_type.clone(),
                };
                remove_attachment_preview_sidecar(&preview_cleanup_attachment);
                already_missing_file_count += 1;
            }
        }
    }

    if delete_failure_reason_changed {
        save_attachment_asset_index_document(&asset_root, &document)?;
    }

    Ok(AttachmentCleanupReport {
        cutoff_date_text: cutoff_text,
        purged_asset_count,
        deleted_file_count,
        already_missing_file_count,
        failed_delete_count,
        purged_byte_len,
        impacted_session_count: impacted_sessions.len(),
    })
}
/// Validates one indexed managed path before cleanup so corrupted index data cannot delete files outside the attachment store.
/// 在清理前校验一条索引托管路径，避免损坏的索引数据删除附件存储之外的文件。
pub(super) fn validate_cleanup_managed_path(
    asset_root: &Path,
    record: &AttachmentAssetRecord,
) -> Result<CleanupManagedPath, String> {
    let managed_path_text = record.managed_path.trim();
    if managed_path_text.is_empty() {
        return Err(format!(
            "附件资产 `{}` 的托管路径为空，已停止清理以避免误删。",
            record.asset_id
        ));
    }

    let managed_path = PathBuf::from(managed_path_text);
    if !managed_path.is_absolute() || path_contains_parent_component(&managed_path) {
        return Err(format!(
            "附件资产 `{}` 的托管路径 `{}` 不安全，已停止清理以避免误删。",
            record.asset_id,
            managed_path.display()
        ));
    }

    let expected_category_dir = asset_root.join(attachment_kind_directory_name(&record.kind));
    let case_insensitive_comparison =
        path_text_comparison_is_case_insensitive(asset_root, &expected_category_dir);
    if managed_path.is_file() {
        let canonical_managed_path = fs::canonicalize(&managed_path).map_err(|error| {
            format!(
                "规范化托管附件 `{}` 失败：{}。",
                managed_path.display(),
                error
            )
        })?;
        let canonical_category_dir = fs::canonicalize(&expected_category_dir).map_err(|error| {
            format!(
                "规范化托管附件分类目录 `{}` 失败：{}。",
                expected_category_dir.display(),
                error
            )
        })?;
        if !path_is_inside_for_host(
            &canonical_managed_path,
            &canonical_category_dir,
            case_insensitive_comparison,
        ) {
            return Err(format!(
                "附件资产 `{}` 的托管路径 `{}` 不在当前项目附件目录内，已停止清理以避免误删。",
                record.asset_id,
                managed_path.display()
            ));
        }
        return Ok(CleanupManagedPath::Existing(canonical_managed_path));
    }

    let Some(parent) = managed_path.parent() else {
        return Err(format!(
            "附件资产 `{}` 的托管路径 `{}` 缺少父目录，已停止清理以避免误删。",
            record.asset_id,
            managed_path.display()
        ));
    };
    if !paths_equal_for_host(parent, &expected_category_dir, case_insensitive_comparison) {
        return Err(format!(
            "附件资产 `{}` 的缺失托管路径 `{}` 不属于当前项目附件分类目录，已停止清理以避免误删。",
            record.asset_id,
            managed_path.display()
        ));
    }

    Ok(CleanupManagedPath::Missing)
}

/// Returns whether a path contains `..`, which is never expected in host-created managed attachment paths.
/// 返回路径是否包含 `..`，宿主创建的托管附件路径中不应出现该片段。
pub(super) fn path_contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

/// Returns whether `child` is inside `parent`, using host-appropriate case sensitivity for path text comparison.
/// 使用符合宿主大小写语义的路径文本比较，返回 `child` 是否位于 `parent` 内部。
pub(super) fn path_is_inside_for_host(child: &Path, parent: &Path, case_insensitive: bool) -> bool {
    let child_text = normalized_host_path_text(child, case_insensitive);
    let parent_text = normalized_host_path_text(parent, case_insensitive);
    child_text == parent_text || child_text.starts_with(&format!("{parent_text}/"))
}

/// Returns whether two paths identify the same lexical location for the current host platform.
/// 返回两条路径在当前宿主平台上是否指向同一个词法位置。
pub(super) fn paths_equal_for_host(left: &Path, right: &Path, case_insensitive: bool) -> bool {
    normalized_host_path_text(left, case_insensitive)
        == normalized_host_path_text(right, case_insensitive)
}

/// Converts a path into normalized comparison text without changing filesystem state.
/// 将路径转换为规范化比较文本，同时不改变文件系统状态。
pub(super) fn normalized_host_path_text(path: &Path, case_insensitive: bool) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    while text.ends_with('/') && text.len() > 1 {
        text.pop();
    }
    if case_insensitive {
        text = text.to_ascii_lowercase();
    }
    text
}

/// Returns whether lexical path comparison should ignore ASCII case for the current managed attachment filesystem.
/// 返回当前托管附件文件系统上的词法路径比较是否应忽略 ASCII 大小写。
pub(super) fn path_text_comparison_is_case_insensitive(
    asset_root: &Path,
    expected_category_dir: &Path,
) -> bool {
    #[cfg(windows)]
    {
        let _ = asset_root;
        let _ = expected_category_dir;
        true
    }
    #[cfg(not(windows))]
    {
        if !cfg!(target_os = "macos") {
            let _ = expected_category_dir;
            return false;
        }
        let probe_root = if expected_category_dir.is_dir() {
            expected_category_dir
        } else {
            asset_root
        };
        directory_path_is_case_insensitive(probe_root)
    }
}

/// Probes whether one existing directory path resolves case-insensitively on the current filesystem.
/// 探测某个已存在目录路径在当前文件系统上是否按大小写不敏感方式解析。
#[cfg(not(windows))]
pub(super) fn directory_path_is_case_insensitive(reference_dir: &Path) -> bool {
    let Some(file_name) = reference_dir.file_name().and_then(|value| value.to_str()) else {
        return false;
    };
    let toggled_name = toggle_first_ascii_alpha_case(file_name);
    if toggled_name == file_name {
        return false;
    }
    let probe_path = reference_dir.with_file_name(toggled_name);
    match (
        fs::canonicalize(reference_dir),
        fs::canonicalize(&probe_path),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

/// Builds one best-effort alternate spelling by toggling the first ASCII alphabetic codepoint in the provided text.
/// 通过翻转首个 ASCII 字母的大小写，为给定文本构造一份尽力而为的替代拼写。
#[cfg(not(windows))]
pub(super) fn toggle_first_ascii_alpha_case(value: &str) -> String {
    let mut toggled = String::with_capacity(value.len());
    let mut switched = false;
    for character in value.chars() {
        if !switched && character.is_ascii_alphabetic() {
            if character.is_ascii_lowercase() {
                toggled.push(character.to_ascii_uppercase());
            } else {
                toggled.push(character.to_ascii_lowercase());
            }
            switched = true;
        } else {
            toggled.push(character);
        }
    }
    toggled
}

/// Returns the managed attachment category directory name for one canonical attachment kind.
/// 返回某个规范附件类型对应的托管附件分类目录名称。
pub(super) fn attachment_kind_directory_name(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "images",
        ConversationAttachmentKind::Audio => "audio",
        ConversationAttachmentKind::Video => "video",
        ConversationAttachmentKind::Pdf => "pdf",
    }
}
pub(super) fn kind_summary_key(kind: ConversationAttachmentKind) -> String {
    match kind {
        ConversationAttachmentKind::Image => String::from("image"),
        ConversationAttachmentKind::Audio => String::from("audio"),
        ConversationAttachmentKind::Video => String::from("video"),
        ConversationAttachmentKind::Pdf => String::from("pdf"),
    }
}
