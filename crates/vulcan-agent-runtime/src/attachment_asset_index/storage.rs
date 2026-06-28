use super::*;

pub(super) fn load_attachment_asset_index_document(
    asset_root: &Path,
) -> Result<AttachmentAssetIndexDocument, String> {
    let index_file = resolve_attachment_index_file(asset_root);
    if !index_file.is_file() {
        return Ok(AttachmentAssetIndexDocument::default());
    }
    let raw = fs::read_to_string(&index_file).map_err(|error| {
        format!(
            "读取附件资产索引 `{}` 失败：{}。",
            index_file.display(),
            error
        )
    })?;
    serde_json::from_str::<AttachmentAssetIndexDocument>(&raw).map_err(|error| {
        format!(
            "解析附件资产索引 `{}` 失败：{}。",
            index_file.display(),
            error
        )
    })
}

/// Saves the provided asset-index document back into the managed attachment root using an atomic temp-file swap.
/// 使用原子临时文件替换的方式，将给定资产索引文档写回托管附件根目录。
pub(super) fn save_attachment_asset_index_document(
    asset_root: &Path,
    document: &AttachmentAssetIndexDocument,
) -> Result<(), String> {
    let index_directory = resolve_attachment_index_directory(asset_root);
    fs::create_dir_all(&index_directory).map_err(|error| {
        format!(
            "创建附件资产索引目录 `{}` 失败：{}。",
            index_directory.display(),
            error
        )
    })?;
    let index_file = resolve_attachment_index_file(asset_root);
    let temporary_file = index_directory.join("assets.json.tmp");
    let serialized = serde_json::to_string_pretty(document)
        .map_err(|error| format!("序列化附件资产索引失败：{}。", error))?;
    fs::write(&temporary_file, serialized).map_err(|error| {
        format!(
            "写入附件资产索引临时文件 `{}` 失败：{}。",
            temporary_file.display(),
            error
        )
    })?;
    fs::rename(&temporary_file, &index_file).map_err(|error| {
        let _ = fs::remove_file(&temporary_file);
        format!(
            "替换附件资产索引文件 `{}` 失败：{}。",
            index_file.display(),
            error
        )
    })
}

/// Resolves the index directory for the provided managed attachment root.
/// 为给定托管附件根目录解析索引目录路径。
pub(super) fn resolve_attachment_index_directory(asset_root: &Path) -> PathBuf {
    asset_root.join(ATTACHMENT_INDEX_DIR_NAME)
}

/// Resolves the index file path for the provided managed attachment root.
/// 为给定托管附件根目录解析索引文件路径。
pub(super) fn resolve_attachment_index_file(asset_root: &Path) -> PathBuf {
    resolve_attachment_index_directory(asset_root).join(ATTACHMENT_INDEX_FILE_NAME)
}

/// Resolves the managed attachment root from one canonical conversation attachment local path.
/// 根据一条规范会话附件的本地路径解析托管附件根目录。
pub(super) fn resolve_asset_root_from_attachment(
    attachment: &ConversationAttachment,
) -> Option<PathBuf> {
    let managed_path = attachment.local_path.trim();
    if managed_path.is_empty() {
        return None;
    }
    let managed_path = PathBuf::from(managed_path);
    let parent = managed_path.parent()?;
    let parent_name = parent.file_name()?.to_str()?.trim().to_ascii_lowercase();
    if parent_name == "attachments" {
        return Some(parent.to_path_buf());
    }
    if matches!(
        parent_name.as_str(),
        "images" | "audio" | "video" | "pdf" | "index"
    ) {
        return parent.parent().map(Path::to_path_buf);
    }
    None
}
