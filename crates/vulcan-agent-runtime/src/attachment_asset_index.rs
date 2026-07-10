//! Host-managed attachment asset index and cleanup helpers shared by staging, replay, and settings surfaces.
//! 供暂存、回放与设置界面共享的宿主管理附件资产索引与清理辅助模块。

use crate::attachment_assets::{
    remove_attachment_preview_sidecar, resolve_managed_attachment_root,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use time::macros::format_description;
use time::{Date, OffsetDateTime, UtcOffset};
use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

/// Stable index directory name stored beside the managed attachment category directories.
/// 存放在托管附件分类目录旁边的稳定索引目录名称。
const ATTACHMENT_INDEX_DIR_NAME: &str = "index";

/// Stable index file name used to persist the managed attachment asset catalog.
/// 用于持久化托管附件资产目录的稳定索引文件名。
const ATTACHMENT_INDEX_FILE_NAME: &str = "assets.json";

/// Stable local date format used by the cleanup UI input and asset index matching.
/// 供清理界面输入与资产索引匹配使用的稳定本地日期格式。
const ATTACHMENT_DATE_FORMAT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day]");

/// Stable local timestamp format rendered into the asset index for diagnostics.
/// 写入资产索引、供诊断使用的稳定本地时间戳格式。
const ATTACHMENT_TIMESTAMP_FORMAT: &[time::format_description::FormatItem<'static>] =
    format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");

mod cleanup;
mod lifecycle;
mod replay;
mod storage;
#[cfg(test)]
mod tests;
mod timestamp;
mod types;

pub use cleanup::{execute_attachment_cleanup, preview_attachment_cleanup};
pub use lifecycle::{
    acquire_staged_attachment_ref, discard_unreferenced_staged_attachment,
    promote_staged_attachment_to_message_ref, record_staged_attachment,
    release_staged_attachment_ref,
};
pub use replay::{load_attachment_asset_record_for_attachment, resolve_attachment_replay_status};
pub use timestamp::{current_attachment_timestamp_parts, parse_attachment_cleanup_cutoff_date};
pub use types::{
    AttachmentAssetRecord, AttachmentCleanupPreview, AttachmentCleanupReport,
    AttachmentCleanupSelection, AttachmentReplayStatus, AttachmentTimestampParts,
};

use cleanup::{CleanupManagedPath, validate_cleanup_managed_path};
use lifecycle::attachment_record_has_transient_refs;
use storage::{
    attachment_index_transaction_lock, load_attachment_asset_index_document,
    resolve_asset_root_from_attachment, save_attachment_asset_index_document,
};
use types::AttachmentAssetIndexDocument;
