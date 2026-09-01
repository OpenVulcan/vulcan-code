use super::source_hold::AttachmentIndexTransaction;
use super::*;
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::{Arc, LazyLock, Mutex, Weak};

/// Retains weak per-index locks so unrelated workspace attachment indexes can mutate concurrently.
/// 保留按索引划分的弱引用锁，使无关工作区附件索引可以并发变更。
static ATTACHMENT_INDEX_TRANSACTION_LOCKS: LazyLock<
    Mutex<HashMap<PathBuf, Weak<Mutex<AttachmentIndexTransaction>>>>,
> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// Resolves the shared transaction lock for one exact attachment index file.
/// 解析单个精确附件索引文件的共享事务锁。
///
/// # Parameters
/// 参数
/// `asset_root` is the canonical managed attachment root owning the index file.
/// `asset_root` 是拥有索引文件的规范托管附件根目录。
///
/// # Returns
/// 返回值
/// A shared mutex that serializes the complete read-modify-save cycle for this index only.
/// 返回一个共享互斥锁，仅串行化当前索引的完整读取、变更与保存周期。
pub(super) fn attachment_index_transaction_lock(
    asset_root: &Path,
) -> Result<Arc<Mutex<AttachmentIndexTransaction>>, String> {
    // Existing roots are canonicalized so Windows separator/case aliases cannot create two owners for one index.
    // 对已存在的根目录进行规范化，避免Windows分隔符或大小写别名为同一索引创建两个所有者。
    // A missing root cannot contain an indexed source or active hold, so its prospective path remains sufficient.
    // 不存在的根目录不可能包含已索引来源或活动持有，因此其预期路径已经足够。
    let lock_root = if asset_root.exists() {
        fs::canonicalize(asset_root).map_err(|error| {
            format!(
                "failed to canonicalize attachment index root `{}`: {error}",
                asset_root.display()
            )
        })?
    } else {
        asset_root.to_path_buf()
    };
    // index_file is the physical persistence target and therefore the only valid lock identity.
    // index_file 是真实物理持久化目标，因此是唯一有效的锁身份。
    let index_file = resolve_attachment_index_file(&lock_root);
    // registry holds weak references so completed workspaces do not accumulate permanent lock entries.
    // registry 保存弱引用，使已经结束的工作区不会永久积累锁条目。
    let mut registry = ATTACHMENT_INDEX_TRANSACTION_LOCKS.lock().map_err(|error| {
        format!("failed to lock attachment index transaction registry: {error}")
    })?;
    registry.retain(|_, transaction_lock| transaction_lock.strong_count() > 0);
    if let Some(transaction_lock) = registry.get(&index_file).and_then(Weak::upgrade) {
        return Ok(transaction_lock);
    }

    // transaction_lock is inserted before the registry guard is released, closing duplicate-lock races.
    // transaction_lock 在注册表守卫释放前插入，从而关闭重复建锁竞态。
    let transaction_lock = Arc::new(Mutex::new(AttachmentIndexTransaction::default()));
    registry.insert(index_file, Arc::downgrade(&transaction_lock));
    Ok(transaction_lock)
}

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
    // index_directory is shared by the staged and final files so publication never crosses filesystems.
    // index_directory 由暂存文件与最终文件共享，使发布永不跨越文件系统。
    let index_directory = resolve_attachment_index_directory(asset_root);
    fs::create_dir_all(&index_directory).map_err(|error| {
        format!(
            "创建附件资产索引目录 `{}` 失败：{}。",
            index_directory.display(),
            error
        )
    })?;
    // index_file is the authoritative durable catalog path.
    // index_file 是权威持久目录路径。
    let index_file = resolve_attachment_index_file(asset_root);
    // existing_permissions preserves the current index access policy across inode replacement.
    // existing_permissions 在 inode 替换期间保留当前索引访问策略。
    let existing_permissions = match fs::symlink_metadata(&index_file) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(format!(
                "拒绝替换符号链接附件资产索引 `{}`。",
                index_file.display()
            ));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => {
            return Err(format!(
                "读取附件资产索引 `{}` 元数据失败：{}。",
                index_file.display(),
                error
            ));
        }
    };
    // serialized contains the complete next index image before any filesystem state changes.
    // serialized 在任何文件系统状态变化前包含完整的下一版本索引内容。
    let serialized = serde_json::to_vec_pretty(document)
        .map_err(|error| format!("序列化附件资产索引失败：{}。", error))?;
    // staged_path and staged_file are exclusively created for this exact save attempt.
    // staged_path 与 staged_file 为当前精确保存尝试独占创建。
    let (staged_path, mut staged_file) = create_attachment_index_staged_file(&index_directory)?;
    // staging_result reaches durable file contents and permissions before the atomic publication step.
    // staging_result 在原子发布步骤前完成文件内容与权限持久化。
    let staging_result = (|| -> std::io::Result<()> {
        staged_file.write_all(&serialized)?;
        if let Some(permissions) = existing_permissions {
            staged_file.set_permissions(permissions)?;
        }
        staged_file.sync_all()
    })();
    if let Err(error) = staging_result {
        drop(staged_file);
        remove_attachment_index_staged_file(&staged_path);
        return Err(format!(
            "写入并同步附件资产索引临时文件 `{}` 失败：{}。",
            staged_path.display(),
            error
        ));
    }
    drop(staged_file);

    if let Err(error) = fs::rename(&staged_path, &index_file) {
        remove_attachment_index_staged_file(&staged_path);
        return Err(format!(
            "使用临时文件 `{}` 替换附件资产索引 `{}` 失败：{}。",
            staged_path.display(),
            index_file.display(),
            error
        ));
    }

    // POSIX requires the renamed directory entry to be synchronized separately for crash durability.
    // POSIX 要求单独同步重命名后的目录项以保证崩溃持久性。
    #[cfg(unix)]
    synchronize_attachment_index_directory(&index_directory)?;

    Ok(())
}

/// Creates one unique attachment-index staging file with cross-process collision detection.
/// 创建一个具有跨进程碰撞检测的唯一附件索引暂存文件。
///
/// # Parameters
/// 参数
/// `index_directory` is the existing directory shared with the final index file.
/// `index_directory` 是与最终索引文件共享的现有目录。
///
/// # Returns
/// 返回值
/// The unique staged path and its exclusively created writable handle.
/// 返回唯一暂存路径及其独占创建的可写句柄。
fn create_attachment_index_staged_file(index_directory: &Path) -> Result<(PathBuf, File), String> {
    // create_new is the authoritative uniqueness check; UUID generation only minimizes collision attempts.
    // create_new 是权威唯一性检查；UUID 生成仅用于减少碰撞尝试。
    loop {
        // staged_path is a sibling of assets.json so rename remains atomic.
        // staged_path 是 assets.json 的同级文件，使重命名保持原子性。
        let staged_path = index_directory.join(format!(
            ".assets.json.vulcan-stage-{}.tmp",
            uuid::Uuid::new_v4()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staged_path)
        {
            Ok(file) => return Ok((staged_path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "创建附件资产索引临时文件 `{}` 失败：{}。",
                    staged_path.display(),
                    error
                ));
            }
        }
    }
}

/// Removes one exclusively owned staging file after a failed save attempt.
/// 在保存尝试失败后删除一个独占暂存文件。
///
/// # Parameters
/// 参数
/// `staged_path` is the temporary path owned by the current save attempt.
/// `staged_path` 是当前保存尝试独占的临时路径。
///
/// # Returns
/// 返回值
/// Best-effort cleanup intentionally returns nothing because the primary persistence error is authoritative.
/// 尽力清理刻意不返回值，因为首要持久化错误保持权威性。
fn remove_attachment_index_staged_file(staged_path: &Path) {
    let _ = fs::remove_file(staged_path);
}

/// Synchronizes the attachment-index directory after a successful POSIX rename.
/// 在 POSIX 重命名成功后同步附件索引目录。
///
/// # Parameters
/// 参数
/// `index_directory` contains the newly published attachment index entry.
/// `index_directory` 包含新发布的附件索引目录项。
///
/// # Returns
/// 返回值
/// `Ok(())` after directory metadata reaches durable storage.
/// 目录元数据到达持久存储后返回 `Ok(())`。
#[cfg(unix)]
fn synchronize_attachment_index_directory(index_directory: &Path) -> Result<(), String> {
    // directory stays open only for the post-rename synchronization operation.
    // directory 仅在重命名后的同步操作期间保持打开。
    let directory = File::open(index_directory).map_err(|error| {
        format!(
            "打开附件资产索引目录 `{}` 以执行同步失败：{}。",
            index_directory.display(),
            error
        )
    })?;
    directory.sync_all().map_err(|error| {
        format!(
            "同步附件资产索引目录 `{}` 失败：{}。",
            index_directory.display(),
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
        "images" | "audio" | "video" | "pdf" | "files" | "index"
    ) {
        return parent.parent().map(Path::to_path_buf);
    }
    None
}
