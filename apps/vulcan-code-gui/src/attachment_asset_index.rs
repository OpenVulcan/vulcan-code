//! Host-managed attachment asset index and cleanup helpers shared by staging, replay, and settings surfaces.
//! 供暂存、回放与设置界面共享的宿主管理附件资产索引与清理辅助模块。

use crate::attachment_assets::resolve_managed_attachment_root;
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

/// Structured timestamp parts reused by attachment staging and cleanup index records.
/// 被附件暂存与清理索引记录复用的结构化时间戳片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentTimestampParts {
    /// Compact local timestamp prefix used by managed attachment file names.
    /// 用于托管附件文件名的紧凑本地时间戳前缀。
    pub compact_prefix: String,
    /// Human-readable local timestamp text stored in the asset index for diagnostics.
    /// 为诊断写入资产索引的人类可读本地时间戳文本。
    pub local_timestamp_text: String,
    /// Stable local date text used by date-based cleanup queries.
    /// 供按日期清理查询使用的稳定本地日期文本。
    pub local_date_text: String,
    /// Unix-millis timestamp retained for future sorting and diagnostics.
    /// 为未来排序与诊断保留的 Unix 毫秒时间戳。
    pub unix_millis: i64,
}

/// Durable asset-index record describing one managed attachment copy staged by the host.
/// 描述一份由宿主暂存的托管附件副本的持久化资产索引记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentAssetRecord {
    /// Stable managed asset identifier shared with the canonical conversation attachment payload.
    /// 与规范会话附件载荷共享的稳定托管资产标识。
    pub asset_id: String,
    /// Stable attachment kind used for directory partitioning and cleanup filtering.
    /// 用于目录分区与清理筛选的稳定附件类型。
    pub kind: ConversationAttachmentKind,
    /// Human-readable original file name retained for diagnostics and settings previews.
    /// 为诊断与设置预览保留的人类可读原始文件名。
    pub original_file_name: String,
    /// Current managed attachment path tracked by the host asset store.
    /// 由宿主资产存储跟踪的当前托管附件路径。
    pub managed_path: String,
    /// Stable MIME type retained for replay diagnostics.
    /// 为回放诊断保留的稳定 MIME 类型。
    pub mime_type: String,
    /// Optional byte length retained for cleanup statistics and budget hints.
    /// 为清理统计与预算提示保留的可选字节长度。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_len: Option<u64>,
    /// Stable local timestamp text recorded when the attachment was staged.
    /// 附件被暂存时记录的稳定本地时间戳文本。
    pub staged_at_local_text: String,
    /// Stable local date text recorded when the attachment was staged.
    /// 附件被暂存时记录的稳定本地日期文本。
    pub staged_date_text: String,
    /// Unix-millis timestamp recorded when the attachment was staged.
    /// 附件被暂存时记录的 Unix 毫秒时间戳。
    pub staged_at_unix_millis: i64,
    /// Optional original source path retained for diagnostics and non-purged fallback replays.
    /// 为诊断与未被清理的回退回放保留的可选原始来源路径。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    /// Session identifiers that have referenced this asset through canonical conversation messages.
    /// 通过规范会话消息引用过该资产的会话标识集合。
    #[serde(default)]
    pub session_ids: Vec<String>,
    /// Message identifiers that have referenced this asset through canonical conversation messages.
    /// 通过规范会话消息引用过该资产的消息标识集合。
    #[serde(default)]
    pub message_ids: Vec<String>,
    /// Transient staging scopes that currently retain this managed asset before it becomes message history.
    /// 在该托管资产成为消息历史前，当前保留它的瞬时暂存作用域集合。
    #[serde(default)]
    pub staging_refs: Vec<String>,
    /// Optional local timestamp text recorded when the managed copy was purged.
    /// 托管副本被清理时记录的可选本地时间戳文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purged_at_local_text: Option<String>,
    /// Optional Unix-millis timestamp recorded when the managed copy was purged.
    /// 托管副本被清理时记录的可选 Unix 毫秒时间戳。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purged_at_unix_millis: Option<i64>,
    /// Optional purge reason retained for diagnostics.
    /// 为诊断保留的可选清理原因。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purge_reason: Option<String>,
}

/// Persistent asset-index document stored inside the managed attachment index directory.
/// 存放在托管附件索引目录中的持久化资产索引文档。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
struct AttachmentAssetIndexDocument {
    /// Flat asset record list tracked for the current workspace attachment store.
    /// 为当前工作区附件存储跟踪的扁平资产记录列表。
    #[serde(default)]
    assets: Vec<AttachmentAssetRecord>,
}

/// Runtime replay status derived from the managed asset index and the current file-system state.
/// 根据托管资产索引与当前文件系统状态推导出的运行时回放状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttachmentReplayStatus {
    /// Managed attachment copy still exists and can be replayed directly.
    /// 托管附件副本仍然存在，可直接回放。
    Available,
    /// Managed attachment copy is gone but the original source path is still readable.
    /// 托管附件副本已不存在，但原始来源路径仍可读取。
    FallbackSourceOnly,
    /// Managed attachment copy was intentionally purged by the cleanup tool.
    /// 托管附件副本已被清理工具主动清除。
    Purged,
    /// Neither the managed copy nor the original source path is available anymore.
    /// 托管副本与原始来源路径当前都已不可用。
    Missing,
}

/// Structured cleanup-kind selection collected from the settings panel checkboxes.
/// 从设置面板勾选框收集得到的结构化清理类型选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AttachmentCleanupSelection {
    /// Whether image assets should participate in cleanup preview or execution.
    /// 图片资产是否应参与清理预览或执行。
    pub image: bool,
    /// Whether audio assets should participate in cleanup preview or execution.
    /// 音频资产是否应参与清理预览或执行。
    pub audio: bool,
    /// Whether video assets should participate in cleanup preview or execution.
    /// 视频资产是否应参与清理预览或执行。
    pub video: bool,
    /// Whether PDF assets should participate in cleanup preview or execution.
    /// PDF 资产是否应参与清理预览或执行。
    pub pdf: bool,
}

impl AttachmentCleanupSelection {
    /// Returns the all-enabled cleanup selection used by the initial settings state.
    /// 返回供设置界面初始状态使用的全量启用清理选择。
    pub const fn all_enabled() -> Self {
        Self {
            image: true,
            audio: true,
            video: true,
            pdf: true,
        }
    }

    /// Returns whether the provided attachment kind is currently selected for cleanup.
    /// 返回给定附件类型当前是否已被选中参与清理。
    pub const fn includes(&self, kind: ConversationAttachmentKind) -> bool {
        match kind {
            ConversationAttachmentKind::Image => self.image,
            ConversationAttachmentKind::Audio => self.audio,
            ConversationAttachmentKind::Video => self.video,
            ConversationAttachmentKind::Pdf => self.pdf,
        }
    }
}

/// Structured cleanup preview summary shown before the user confirms a destructive purge.
/// 在用户确认破坏性清理前展示的结构化清理预览摘要。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentCleanupPreview {
    /// Cutoff date text applied by the preview in local `YYYY-MM-DD` format.
    /// 预览使用的本地 `YYYY-MM-DD` 截止日期文本。
    pub cutoff_date_text: String,
    /// Total indexed assets that match the cutoff date and kind filters.
    /// 匹配截止日期与类型筛选条件的索引资产总数。
    pub matched_asset_count: usize,
    /// Total byte length aggregated from the matched assets when byte lengths are known.
    /// 在已知字节长度时由匹配资产聚合得到的总字节数。
    pub matched_byte_len: u64,
    /// Unique session count impacted by the matched assets.
    /// 受匹配资产影响的唯一会话数量。
    pub impacted_session_count: usize,
    /// Per-kind asset counts rendered into the settings preview detail line.
    /// 渲染到设置预览详情行中的按类型统计数量。
    pub per_kind_counts: BTreeMap<String, usize>,
}

/// Structured cleanup execution report returned after one purge request finishes.
/// 一次清理请求完成后返回的结构化执行报告。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachmentCleanupReport {
    /// Cutoff date text applied by the purge operation in local `YYYY-MM-DD` format.
    /// 清理操作使用的本地 `YYYY-MM-DD` 截止日期文本。
    pub cutoff_date_text: String,
    /// Total asset records marked as purged by the operation.
    /// 被本次操作标记为已清理的资产记录总数。
    pub purged_asset_count: usize,
    /// Total managed files physically deleted from disk by the operation.
    /// 被本次操作从磁盘物理删除的托管文件总数。
    pub deleted_file_count: usize,
    /// Total managed files that were already missing before the purge started.
    /// 在清理开始前就已经缺失的托管文件总数。
    pub already_missing_file_count: usize,
    /// Total managed files that could not be physically deleted after replay had been revoked.
    /// 在回放已经撤销后，未能从磁盘物理删除的托管文件总数。
    pub failed_delete_count: usize,
    /// Total byte length aggregated from the purged assets when byte lengths are known.
    /// 在已知字节长度时由被清理资产聚合得到的总字节数。
    pub purged_byte_len: u64,
    /// Unique session count impacted by the purged assets.
    /// 受被清理资产影响的唯一会话数量。
    pub impacted_session_count: usize,
}

/// Validated managed file state used by the cleanup executor before any destructive operation runs.
/// 在任何破坏性操作执行前，由清理执行器使用的已校验托管文件状态。
#[derive(Debug, Clone, PartialEq, Eq)]
enum CleanupManagedPath {
    /// Managed file still exists and has been canonicalized inside the current attachment root.
    /// 托管文件仍然存在，并且已规范化到当前附件根目录内部。
    Existing(PathBuf),
    /// Managed file is already missing but its recorded path is still lexically inside the current attachment root.
    /// 托管文件已经缺失，但记录路径在词法上仍位于当前附件根目录内部。
    Missing,
}

/// Builds one best-effort local timestamp snapshot used by managed attachment staging and cleanup bookkeeping.
/// 构建一份尽力使用本地时区的时间戳快照，供托管附件暂存与清理账本复用。
pub fn current_attachment_timestamp_parts() -> AttachmentTimestampParts {
    let local_now = OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc().to_offset(UtcOffset::UTC));
    build_attachment_timestamp_parts(local_now)
}

/// Parses one cleanup cutoff date text in `YYYY-MM-DD` format.
/// 解析一段 `YYYY-MM-DD` 格式的清理截止日期文本。
pub fn parse_attachment_cleanup_cutoff_date(cutoff_date_text: &str) -> Result<Date, String> {
    let trimmed_date = cutoff_date_text.trim();
    if trimmed_date.is_empty() {
        return Err(String::from("请输入清理截止日期，格式为 YYYY-MM-DD。"));
    }
    Date::parse(trimmed_date, ATTACHMENT_DATE_FORMAT).map_err(|error| {
        format!("清理截止日期 `{trimmed_date}` 无效：{error}。请使用 YYYY-MM-DD 格式。")
    })
}

/// Records one newly staged managed attachment into the workspace-scoped asset index.
/// 将一条新暂存的托管附件记录写入工作区级资产索引。
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
        fs::remove_file(&managed_path).map_err(|error| {
            format!(
                "删除未引用托管附件 `{}` 失败：{}。",
                managed_path.display(),
                error
            )
        })?;
    }

    document.assets.remove(index);
    save_attachment_asset_index_document(&asset_root, &document)?;
    Ok(true)
}

/// Updates staging reference scopes for a batch of managed attachments.
/// 批量更新托管附件的暂存引用作用域。
fn update_staged_attachment_refs(
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
fn discard_attachment_record_at(
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
        fs::remove_file(&managed_path).map_err(|error| {
            format!(
                "删除未引用托管附件 `{}` 失败：{}。",
                managed_path.display(),
                error
            )
        })?;
    }
    document.assets.remove(index);
    Ok(())
}

/// Returns the stable asset id for one attachment when it can participate in the managed index.
/// 当附件可参与托管索引时，返回它的稳定资产标识。
fn attachment_asset_id(attachment: &ConversationAttachment) -> Option<String> {
    attachment
        .asset_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Collects managed asset identifiers from canonical attachment payloads and rejects malformed managed entries.
/// 从规范附件载荷中收集托管资产标识，并拒绝缺少标识的异常托管条目。
fn collect_attachment_asset_ids(
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
fn ensure_attachment_asset_ids_are_indexed(
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
fn attachment_record_has_refs(record: &AttachmentAssetRecord) -> bool {
    !record.session_ids.is_empty()
        || !record.message_ids.is_empty()
        || !record.staging_refs.is_empty()
}

/// Returns whether an indexed attachment is still retained by a transient composer, queue, or undo owner.
/// 返回索引附件是否仍被输入框、队列或撤销栈等瞬时所有者持有。
fn attachment_record_has_transient_refs(record: &AttachmentAssetRecord) -> bool {
    !record.staging_refs.is_empty()
}

/// Resolves the current replay status for one canonical conversation attachment by combining index metadata with file existence checks.
/// 结合索引元数据与文件存在性检查，为一条规范会话附件解析当前回放状态。
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
    let mut document = load_attachment_asset_index_document(&asset_root)?;
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

    for (index, planned_path) in matched_indices.into_iter().zip(planned_paths.into_iter()) {
        {
            let record = &mut document.assets[index];
            record.purged_at_local_text = Some(purge_timestamp.local_timestamp_text.clone());
            record.purged_at_unix_millis = Some(purge_timestamp.unix_millis);
            record.purge_reason = Some(format!("purged-before-{}", cutoff_text));
        }
        save_attachment_asset_index_document(&asset_root, &document)?;

        let record = &document.assets[index];
        purged_asset_count += 1;
        purged_byte_len = purged_byte_len.saturating_add(record.byte_len.unwrap_or(0));
        impacted_sessions.extend(record.session_ids.iter().cloned());

        match planned_path {
            CleanupManagedPath::Existing(managed_path) => match fs::remove_file(&managed_path) {
                Ok(()) => {
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
    {
        if let Some(record) = document
            .assets
            .iter()
            .find(|record| record.asset_id == asset_id)
        {
            return Some(record.clone());
        }
    }
    let managed_path = attachment.local_path.trim();
    document
        .assets
        .iter()
        .find(|record| record.managed_path == managed_path)
        .cloned()
}

/// Validates one indexed managed path before cleanup so corrupted index data cannot delete files outside the attachment store.
/// 在清理前校验一条索引托管路径，避免损坏的索引数据删除附件存储之外的文件。
fn validate_cleanup_managed_path(
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
        if !path_is_inside_for_host(&canonical_managed_path, &canonical_category_dir) {
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
    if !paths_equal_for_host(parent, &expected_category_dir) {
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
fn path_contains_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

/// Returns whether `child` is inside `parent`, using host-appropriate case sensitivity for path text comparison.
/// 使用符合宿主大小写语义的路径文本比较，返回 `child` 是否位于 `parent` 内部。
fn path_is_inside_for_host(child: &Path, parent: &Path) -> bool {
    let child_text = normalized_host_path_text(child);
    let parent_text = normalized_host_path_text(parent);
    child_text == parent_text || child_text.starts_with(&format!("{parent_text}/"))
}

/// Returns whether two paths identify the same lexical location for the current host platform.
/// 返回两条路径在当前宿主平台上是否指向同一个词法位置。
fn paths_equal_for_host(left: &Path, right: &Path) -> bool {
    normalized_host_path_text(left) == normalized_host_path_text(right)
}

/// Converts a path into normalized comparison text without changing filesystem state.
/// 将路径转换为规范化比较文本，同时不改变文件系统状态。
fn normalized_host_path_text(path: &Path) -> String {
    let mut text = path.to_string_lossy().replace('\\', "/");
    while text.ends_with('/') && text.len() > 1 {
        text.pop();
    }
    #[cfg(windows)]
    {
        text = text.to_ascii_lowercase();
    }
    text
}

/// Returns the managed attachment category directory name for one canonical attachment kind.
/// 返回某个规范附件类型对应的托管附件分类目录名称。
fn attachment_kind_directory_name(kind: &ConversationAttachmentKind) -> &'static str {
    match kind {
        ConversationAttachmentKind::Image => "images",
        ConversationAttachmentKind::Audio => "audio",
        ConversationAttachmentKind::Video => "video",
        ConversationAttachmentKind::Pdf => "pdf",
    }
}

/// Builds one deterministic local timestamp-part bundle from the provided local or UTC offset datetime.
/// 根据提供的本地或 UTC 偏移时间构建一份确定性的时间片段集合。
fn build_attachment_timestamp_parts(now: OffsetDateTime) -> AttachmentTimestampParts {
    let compact_prefix = now
        .format(format_description!(
            "[year][month][day]-[hour][minute][second]"
        ))
        .unwrap_or_else(|_| String::from("19700101-000000"));
    let local_timestamp_text = now
        .format(ATTACHMENT_TIMESTAMP_FORMAT)
        .unwrap_or_else(|_| String::from("1970-01-01 00:00:00"));
    let local_date_text = now
        .format(ATTACHMENT_DATE_FORMAT)
        .unwrap_or_else(|_| String::from("1970-01-01"));
    let unix_millis = now.unix_timestamp_nanos().div_euclid(1_000_000) as i64;
    AttachmentTimestampParts {
        compact_prefix,
        local_timestamp_text,
        local_date_text,
        unix_millis,
    }
}

/// Loads the persistent asset-index document from the provided managed attachment root.
/// 从给定托管附件根目录载入持久化资产索引文档。
fn load_attachment_asset_index_document(
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
fn save_attachment_asset_index_document(
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
fn resolve_attachment_index_directory(asset_root: &Path) -> PathBuf {
    asset_root.join(ATTACHMENT_INDEX_DIR_NAME)
}

/// Resolves the index file path for the provided managed attachment root.
/// 为给定托管附件根目录解析索引文件路径。
fn resolve_attachment_index_file(asset_root: &Path) -> PathBuf {
    resolve_attachment_index_directory(asset_root).join(ATTACHMENT_INDEX_FILE_NAME)
}

/// Resolves the managed attachment root from one canonical conversation attachment local path.
/// 根据一条规范会话附件的本地路径解析托管附件根目录。
fn resolve_asset_root_from_attachment(attachment: &ConversationAttachment) -> Option<PathBuf> {
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

/// Returns the stable summary key used by cleanup previews for one attachment kind.
/// 返回供清理预览使用的某个附件类型的稳定摘要键。
fn kind_summary_key(kind: ConversationAttachmentKind) -> String {
    match kind {
        ConversationAttachmentKind::Image => String::from("image"),
        ConversationAttachmentKind::Audio => String::from("audio"),
        ConversationAttachmentKind::Video => String::from("video"),
        ConversationAttachmentKind::Pdf => String::from("pdf"),
    }
}

#[cfg(test)]
mod tests {
    //! Attachment asset-index regression tests covering staging records, cleanup previews, purge execution, and replay-state derivation.
    //! 覆盖暂存记录、清理预览、清理执行与回放状态推导的附件资产索引回归测试。

    use super::{
        AttachmentCleanupSelection, AttachmentReplayStatus, acquire_staged_attachment_ref,
        current_attachment_timestamp_parts, discard_unreferenced_staged_attachment,
        execute_attachment_cleanup, load_attachment_asset_index_document,
        load_attachment_asset_record_for_attachment, parse_attachment_cleanup_cutoff_date,
        preview_attachment_cleanup, promote_staged_attachment_to_message_ref,
        record_staged_attachment, release_staged_attachment_ref, resolve_attachment_replay_status,
        save_attachment_asset_index_document,
    };
    use crate::attachment_assets::{
        attachment_descriptor_from_mime_type, resolve_attachment_replay_path,
        resolve_managed_attachment_root, stage_attachment_bytes,
    };
    use std::time::{SystemTime, UNIX_EPOCH};
    use vulcan_core::types::{ConversationAttachment, ConversationAttachmentKind};

    /// Builds one unique temporary workspace directory path for attachment index tests.
    /// 为附件索引测试构建一条唯一的临时工作区目录路径。
    fn unique_workspace_directory() -> std::path::PathBuf {
        let unique_id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let directory = std::env::temp_dir().join(format!("vulcan-attachment-index-{unique_id}"));
        std::fs::create_dir_all(&directory).expect("temporary workspace directory should create");
        directory
    }

    /// Stages one temporary managed attachment inside a unique workspace directory for testing.
    /// 在唯一临时工作区目录中暂存一条供测试使用的临时托管附件。
    fn build_test_attachment(
        workspace_directory: &std::path::Path,
        file_name: &str,
        kind: ConversationAttachmentKind,
        mime_type: &str,
    ) -> ConversationAttachment {
        let descriptor = attachment_descriptor_from_mime_type(mime_type)
            .expect("descriptor should resolve for the provided MIME type");
        let attachment = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            file_name,
            None,
            &[0_u8, 1, 2, 3],
        )
        .expect("attachment bytes should stage successfully");
        assert_eq!(attachment.kind, kind);
        attachment
    }

    /// Verifies staged attachments are indexed and later enriched with session/message references.
    /// 验证已暂存附件会写入索引，并可在后续补充会话/消息引用关系。
    #[test]
    fn staged_attachment_records_can_be_enriched_with_references() {
        let workspace_directory = unique_workspace_directory();
        let attachment = build_test_attachment(
            &workspace_directory,
            "diagram.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        let timestamp = current_attachment_timestamp_parts();

        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
            &timestamp,
        )
        .expect("staged attachment should record");
        promote_staged_attachment_to_message_ref(
            &workspace_directory.to_string_lossy(),
            "session-1",
            "message-1",
            std::slice::from_ref(&attachment),
            &[],
        )
        .expect("attachment references should record");

        let record = load_attachment_asset_record_for_attachment(&attachment)
            .expect("asset record should load");
        assert_eq!(
            record.asset_id,
            attachment.asset_id.expect("asset id should exist")
        );
        assert_eq!(record.session_ids, vec![String::from("session-1")]);
        assert_eq!(record.message_ids, vec![String::from("message-1")]);

        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies transient lease acquisition fails when a managed attachment no longer has an index record.
    /// 验证当托管附件不再拥有索引记录时，瞬时租约获取会失败。
    #[test]
    fn acquire_staged_attachment_ref_rejects_missing_index_record() {
        let workspace_directory = unique_workspace_directory();
        let mut attachment = build_test_attachment(
            &workspace_directory,
            "diagram.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        attachment.asset_id = Some(String::from("missing-index-asset"));

        let error = acquire_staged_attachment_ref(
            &workspace_directory.to_string_lossy(),
            "composer:test",
            std::slice::from_ref(&attachment),
        )
        .expect_err("missing index record should reject lease acquisition");

        assert!(error.contains("附件资产索引缺少"));
        assert!(std::path::Path::new(&attachment.local_path).is_file());
        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies durable message promotion fails when the managed asset index is missing.
    /// 验证当托管资产索引缺失时，持久消息提升会失败。
    #[test]
    fn promote_staged_attachment_rejects_missing_index_record() {
        let workspace_directory = unique_workspace_directory();
        let mut attachment = build_test_attachment(
            &workspace_directory,
            "diagram.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        attachment.asset_id = Some(String::from("missing-index-asset"));

        let error = promote_staged_attachment_to_message_ref(
            &workspace_directory.to_string_lossy(),
            "session-1",
            "message-1",
            std::slice::from_ref(&attachment),
            &[],
        )
        .expect_err("missing index record should reject message promotion");

        assert!(error.contains("附件资产索引缺少"));
        assert!(std::path::Path::new(&attachment.local_path).is_file());
        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies staging leases protect accepted composer assets until the final scope is released.
    /// 验证暂存租约会保护已进入输入框的资产，直到最后一个作用域被释放。
    #[test]
    fn staging_refs_protect_assets_until_release() {
        let workspace_directory = unique_workspace_directory();
        let attachment = build_test_attachment(
            &workspace_directory,
            "diagram.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        let timestamp = current_attachment_timestamp_parts();
        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
            &timestamp,
        )
        .expect("staged attachment should record");

        acquire_staged_attachment_ref(
            &workspace_directory.to_string_lossy(),
            "composer:test",
            std::slice::from_ref(&attachment),
        )
        .expect("staging ref should acquire");
        assert!(
            !discard_unreferenced_staged_attachment(
                &workspace_directory.to_string_lossy(),
                &attachment,
            )
            .expect("leased attachment should not discard")
        );
        assert!(std::path::Path::new(&attachment.local_path).is_file());

        release_staged_attachment_ref(
            &workspace_directory.to_string_lossy(),
            "composer:test",
            std::slice::from_ref(&attachment),
        )
        .expect("staging ref should release");

        assert!(load_attachment_asset_record_for_attachment(&attachment).is_none());
        assert!(!std::path::Path::new(&attachment.local_path).exists());
        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies promoting staged assets releases only the dispatch lease while preserving unrelated transient owners.
    /// 验证提升暂存资产只释放派发租约，并保留无关瞬时所有者。
    #[test]
    fn promote_staged_attachment_releases_only_requested_staging_refs() {
        let workspace_directory = unique_workspace_directory();
        let attachment = build_test_attachment(
            &workspace_directory,
            "diagram.pdf",
            ConversationAttachmentKind::Pdf,
            "application/pdf",
        );
        let timestamp = current_attachment_timestamp_parts();
        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
            &timestamp,
        )
        .expect("staged attachment should record");
        acquire_staged_attachment_ref(
            &workspace_directory.to_string_lossy(),
            "dispatch:test",
            std::slice::from_ref(&attachment),
        )
        .expect("staging ref should acquire");
        acquire_staged_attachment_ref(
            &workspace_directory.to_string_lossy(),
            "composer-undo:test",
            std::slice::from_ref(&attachment),
        )
        .expect("unrelated staging ref should acquire");

        promote_staged_attachment_to_message_ref(
            &workspace_directory.to_string_lossy(),
            "session-1",
            "message-1",
            std::slice::from_ref(&attachment),
            &[String::from("dispatch:test")],
        )
        .expect("staged attachment should promote");

        let record = load_attachment_asset_record_for_attachment(&attachment)
            .expect("promoted asset record should remain");
        assert_eq!(
            record.staging_refs,
            vec![String::from("composer-undo:test")]
        );
        assert_eq!(record.session_ids, vec![String::from("session-1")]);
        assert_eq!(record.message_ids, vec![String::from("message-1")]);
        release_staged_attachment_ref(
            &workspace_directory.to_string_lossy(),
            "composer-undo:test",
            std::slice::from_ref(&attachment),
        )
        .expect("unrelated staging ref should release");
        assert!(
            !discard_unreferenced_staged_attachment(
                &workspace_directory.to_string_lossy(),
                &attachment,
            )
            .expect("message-referenced asset should not discard")
        );
        assert!(std::path::Path::new(&attachment.local_path).is_file());
        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies cleanup preview counts one staged asset before a future cutoff date.
    /// 验证当截止日期位于未来时，清理预览会统计一条已暂存资产。
    #[test]
    fn cleanup_preview_counts_matching_assets() {
        let workspace_directory = unique_workspace_directory();
        let attachment = build_test_attachment(
            &workspace_directory,
            "diagram.pdf",
            ConversationAttachmentKind::Pdf,
            "application/pdf",
        );
        let timestamp = current_attachment_timestamp_parts();

        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
            &timestamp,
        )
        .expect("staged attachment should record");
        let preview = preview_attachment_cleanup(
            &workspace_directory.to_string_lossy(),
            "9999-12-31",
            AttachmentCleanupSelection::all_enabled(),
        )
        .expect("cleanup preview should build");

        assert_eq!(preview.matched_asset_count, 1);
        assert_eq!(preview.per_kind_counts.get("pdf"), Some(&1));

        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies cleanup can purge durable history attachments while keeping transient staging leases protected.
    /// 验证清理可以清除持久历史附件，同时继续保护瞬时暂存租约。
    #[test]
    fn cleanup_purges_durable_history_but_skips_transient_staging_refs() {
        let workspace_directory = unique_workspace_directory();
        let historical_attachment = build_test_attachment(
            &workspace_directory,
            "history.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        let staged_attachment = build_test_attachment(
            &workspace_directory,
            "composer.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        let timestamp = current_attachment_timestamp_parts();
        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &historical_attachment,
            &timestamp,
        )
        .expect("historical attachment should record");
        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &staged_attachment,
            &timestamp,
        )
        .expect("staged attachment should record");
        promote_staged_attachment_to_message_ref(
            &workspace_directory.to_string_lossy(),
            "session-1",
            "message-1",
            std::slice::from_ref(&historical_attachment),
            &[],
        )
        .expect("historical attachment references should record");
        acquire_staged_attachment_ref(
            &workspace_directory.to_string_lossy(),
            "composer:test",
            std::slice::from_ref(&staged_attachment),
        )
        .expect("composer staging ref should acquire");

        let preview = preview_attachment_cleanup(
            &workspace_directory.to_string_lossy(),
            "9999-12-31",
            AttachmentCleanupSelection::all_enabled(),
        )
        .expect("cleanup preview should build");
        assert_eq!(preview.matched_asset_count, 1);
        assert_eq!(preview.impacted_session_count, 1);

        let report = execute_attachment_cleanup(
            &workspace_directory.to_string_lossy(),
            "9999-12-31",
            AttachmentCleanupSelection::all_enabled(),
        )
        .expect("cleanup execution should succeed");

        assert_eq!(report.purged_asset_count, 1);
        assert_eq!(report.impacted_session_count, 1);
        assert_eq!(
            resolve_attachment_replay_status(&historical_attachment),
            AttachmentReplayStatus::Purged
        );
        assert_eq!(
            resolve_attachment_replay_status(&staged_attachment),
            AttachmentReplayStatus::Available
        );
        assert!(!std::path::Path::new(&historical_attachment.local_path).exists());
        assert!(std::path::Path::new(&staged_attachment.local_path).is_file());

        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies purge execution marks the asset as purged and stops replay even when an original source path still exists.
    /// 验证清理执行会把资产标记为已清理，并且即使原始来源路径仍存在，也会停止回放。
    #[test]
    fn purge_execution_prevents_source_fallback_replay() {
        let workspace_directory = unique_workspace_directory();
        let original_source_path = workspace_directory.join("original-diagram.png");
        std::fs::write(&original_source_path, [0_u8, 1, 2, 3])
            .expect("original source should write");
        let descriptor = attachment_descriptor_from_mime_type("image/png")
            .expect("png descriptor should resolve");
        let attachment = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            "diagram.png",
            Some(original_source_path.as_path()),
            &[0_u8, 1, 2, 3],
        )
        .expect("attachment should stage");
        let timestamp = current_attachment_timestamp_parts();
        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
            &timestamp,
        )
        .expect("staged attachment should record");

        let report = execute_attachment_cleanup(
            &workspace_directory.to_string_lossy(),
            "9999-12-31",
            AttachmentCleanupSelection::all_enabled(),
        )
        .expect("cleanup execution should succeed");

        assert_eq!(report.purged_asset_count, 1);
        assert_eq!(
            resolve_attachment_replay_status(&attachment),
            AttachmentReplayStatus::Purged
        );
        assert!(!std::path::Path::new(&attachment.local_path).exists());
        assert!(original_source_path.is_file());

        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies date parsing rejects malformed cleanup cutoff values early.
    /// 验证日期解析会尽早拒绝格式错误的清理截止值。
    #[test]
    fn cleanup_cutoff_date_parser_rejects_malformed_input() {
        assert!(parse_attachment_cleanup_cutoff_date("2026-05-15").is_ok());
        assert!(parse_attachment_cleanup_cutoff_date("20260515").is_err());
    }

    /// Verifies missing managed copies fall back to the original source path only when the asset was not explicitly purged.
    /// 验证仅当资产未被显式清理时，缺失的托管副本才会回退到原始来源路径。
    #[test]
    fn replay_status_uses_source_fallback_when_asset_was_not_purged() {
        let workspace_directory = unique_workspace_directory();
        let original_source_path = workspace_directory.join("original-audio.mp3");
        std::fs::write(&original_source_path, [0_u8, 1, 2, 3])
            .expect("original source should write");
        let descriptor = attachment_descriptor_from_mime_type("audio/mpeg")
            .expect("audio descriptor should resolve");
        let attachment = stage_attachment_bytes(
            &workspace_directory.to_string_lossy(),
            &descriptor,
            "clip.mp3",
            Some(original_source_path.as_path()),
            &[0_u8, 1, 2, 3],
        )
        .expect("attachment should stage");
        let timestamp = current_attachment_timestamp_parts();
        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
            &timestamp,
        )
        .expect("staged attachment should record");
        std::fs::remove_file(&attachment.local_path).expect("managed copy should remove");

        assert_eq!(
            resolve_attachment_replay_status(&attachment),
            AttachmentReplayStatus::FallbackSourceOnly
        );

        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies cleanup refuses corrupted index records that point outside the current project attachment store.
    /// 验证清理会拒绝指向当前项目附件存储之外的损坏索引记录。
    #[test]
    fn cleanup_rejects_index_paths_outside_attachment_store() {
        let workspace_directory = unique_workspace_directory();
        let outside_file = workspace_directory.join("outside.png");
        std::fs::write(&outside_file, [9_u8]).expect("outside file should write");
        let attachment = build_test_attachment(
            &workspace_directory,
            "diagram.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        let timestamp = current_attachment_timestamp_parts();
        record_staged_attachment(
            &workspace_directory.to_string_lossy(),
            &attachment,
            &timestamp,
        )
        .expect("staged attachment should record");

        let asset_root = resolve_managed_attachment_root(&workspace_directory.to_string_lossy());
        let mut document =
            load_attachment_asset_index_document(&asset_root).expect("index should load");
        let asset_id = attachment
            .asset_id
            .as_deref()
            .expect("staged attachment should have an asset id");
        let record = document
            .assets
            .iter_mut()
            .find(|record| record.asset_id == asset_id)
            .expect("asset record should exist");
        record.staged_date_text = String::from("1970-01-01");
        record.managed_path = outside_file.to_string_lossy().into_owned();
        save_attachment_asset_index_document(&asset_root, &document)
            .expect("tampered index should save");

        let error = execute_attachment_cleanup(
            &workspace_directory.to_string_lossy(),
            "9999-12-31",
            AttachmentCleanupSelection::all_enabled(),
        )
        .expect_err("cleanup should reject unsafe managed paths");

        assert!(error.contains("不在当前项目附件目录内"));
        assert!(outside_file.is_file());
        let record = load_attachment_asset_record_for_attachment(&attachment)
            .expect("asset record should still load");
        assert!(record.purged_at_unix_millis.is_none());

        let _ = std::fs::remove_dir_all(&workspace_directory);
    }

    /// Verifies an explicit purge marker blocks replay even if the managed file still exists after a failed delete.
    /// 验证即使删除失败后托管文件仍然存在，显式清理标记也会阻止附件继续回放。
    #[test]
    fn purged_marker_prevents_replay_when_managed_file_still_exists() {
        let workspace_directory = unique_workspace_directory();
        let attachment = build_test_attachment(
            &workspace_directory,
            "diagram.png",
            ConversationAttachmentKind::Image,
            "image/png",
        );
        let asset_root = resolve_managed_attachment_root(&workspace_directory.to_string_lossy());
        let mut document =
            load_attachment_asset_index_document(&asset_root).expect("index should load");
        let asset_id = attachment
            .asset_id
            .as_deref()
            .expect("staged attachment should have an asset id");
        let record = document
            .assets
            .iter_mut()
            .find(|record| record.asset_id == asset_id)
            .expect("asset record should exist");
        record.purged_at_local_text = Some(String::from("2026-05-16 00:00:00"));
        record.purged_at_unix_millis = Some(1_779_000_000_000);
        record.purge_reason = Some(String::from("purged-before-2026-05-16; delete-failed"));
        save_attachment_asset_index_document(&asset_root, &document)
            .expect("purged index should save");

        assert!(std::path::Path::new(&attachment.local_path).is_file());
        assert_eq!(
            resolve_attachment_replay_status(&attachment),
            AttachmentReplayStatus::Purged
        );
        assert!(resolve_attachment_replay_path(&attachment).is_none());

        let _ = std::fs::remove_dir_all(&workspace_directory);
    }
}
