use super::*;

/// Resolves whether one attachment can replay from the managed asset copy, source fallback, or neither.
/// 判断一个附件可从托管资产副本、来源回退路径还是都不可用来回放。
pub fn resolve_attachment_replay_status(
    attachment: &ConversationAttachment,
) -> AttachmentReplayStatus {
    let asset_record = load_attachment_asset_record_for_attachment(attachment);
    if asset_record
        .as_ref()
        .and_then(|record| record.purged_at_unix_millis)
        .is_some()
    {
        return AttachmentReplayStatus::Purged;
    }

    let managed_path = attachment.local_path.trim();
    if !managed_path.is_empty() && Path::new(managed_path).is_file() {
        return AttachmentReplayStatus::Available;
    }

    let source_path = attachment
        .source_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    if source_path.as_ref().is_some_and(|path| path.is_file()) {
        return AttachmentReplayStatus::FallbackSourceOnly;
    }

    AttachmentReplayStatus::Missing
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
