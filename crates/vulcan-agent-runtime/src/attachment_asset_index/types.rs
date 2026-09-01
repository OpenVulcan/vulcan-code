use super::*;

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
    /// SHA-256 digest of the exact immutable managed bytes.
    /// 精确不可变托管字节的 SHA-256 摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
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
    /// Original media intent per live composer draft lease, never a file-global last-model cache.
    /// 每个活动输入区草稿租约的原媒体意图，绝不是文件全局最近模型缓存。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub draft_media: BTreeMap<String, super::draft_media::AttachmentDraftMediaOrigin>,
    /// Exact upload handoffs captured before draft lease release, keyed by original intent rather than current scope.
    /// 草稿租约释放前捕获的精确上传交接，以原意图而非当前作用域为键。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub draft_media_handoffs: BTreeMap<String, super::draft_media::AttachmentDraftMediaHandoff>,
    /// Provider-internal media references bound to this immutable asset and an exact route scope.
    /// 绑定到该不可变资产及精确路线作用域的供应商内部媒体引用。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_references: Vec<AttachmentMediaReferenceRecord>,
    /// Whether an unreferenced managed file deletion was durably planned but not fully committed.
    /// 是否已经持久规划但尚未完整提交未引用托管文件删除。
    #[serde(default, skip_serializing_if = "boolean_is_false")]
    pub deletion_pending: bool,
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

/// Stores one provider-internal reference without projecting it into canonical conversation data.
/// 保存一条供应商内部引用，但不把它投影到规范会话数据中。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AttachmentMediaReferenceRecord {
    /// SHA-256 digest of the exact immutable source bytes.
    /// 精确不可变源字节的 SHA-256 摘要。
    pub source_sha256: String,
    /// Byte length covered by the source digest.
    /// 源摘要覆盖的字节长度。
    pub source_byte_len: u64,
    /// Provider identity owning the uploaded object.
    /// 拥有已上传对象的供应商标识。
    pub provider_id: String,
    /// Service-entry identity owning the upload channel.
    /// 拥有上传通道的服务入口标识。
    pub service_entry_id: String,
    /// Exact upload-channel identity.
    /// 精确上传通道标识。
    pub upload_channel_id: String,
    /// Content fingerprint of the upload contract.
    /// 上传契约的内容指纹。
    pub upload_contract_fingerprint: String,
    /// Closed provider-protocol, credential, or session reuse scope.
    /// 封闭的供应商协议、凭据或会话复用作用域。
    pub reference_scope: SessionMediaReferenceScope,
    /// Exact connection identity required by credential-scoped references.
    /// 凭据作用域引用要求的精确连接身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_id: Option<String>,
    /// Exact session identity required by session-scoped references.
    /// 会话作用域引用要求的精确会话身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// Stable resource-kind label.
    /// 稳定资源类型标签。
    pub resource_kind: String,
    /// Exact normalized MIME type.
    /// 精确规范 MIME 类型。
    pub mime_type: String,
    /// Provider-native reference consumed only at the final transport boundary.
    /// 仅在最终传输边界消费的供应商原生引用。
    pub provider_reference: String,
    /// Creation timestamp in Unix milliseconds.
    /// 创建时间的 Unix 毫秒值。
    pub created_at_unix_millis: u64,
    /// Exclusive reuse expiry in Unix milliseconds.
    /// 复用到期的 Unix 毫秒值。
    pub expires_at_unix_millis: u64,
}

impl AttachmentMediaReferenceRecord {
    /// Compares every immutable source and route field without delimiter-based key encoding.
    /// 不使用基于分隔符的键编码，逐项比较全部不可变来源及路线字段。
    ///
    /// # Parameters
    /// 参数
    /// `other` is the candidate reference identity.
    /// `other` 是候选引用身份。
    ///
    /// # Returns
    /// 返回值
    /// `true` only when every non-secret source and route field matches exactly.
    /// 仅当全部非秘密来源及路线字段精确一致时返回 `true`。
    pub fn matches_route_identity(&self, other: &Self) -> bool {
        self.source_sha256 == other.source_sha256
            && self.source_byte_len == other.source_byte_len
            && self.provider_id == other.provider_id
            && self.service_entry_id == other.service_entry_id
            && self.upload_channel_id == other.upload_channel_id
            && self.upload_contract_fingerprint == other.upload_contract_fingerprint
            && self.reference_scope == other.reference_scope
            && self.connection_id == other.connection_id
            && self.session_id == other.session_id
            && self.resource_kind == other.resource_kind
            && self.mime_type == other.mime_type
    }
}

/// Returns whether one persisted boolean is false for compact serde output.
/// 返回一个持久化布尔值是否为 false，以生成紧凑 serde 输出。
///
/// # Parameters
/// 参数
/// `value` is the boolean field inspected by serde.
/// `value` 是由 serde 检查的布尔字段。
///
/// # Returns
/// 返回值
/// `true` only when the field contains `false` and may be omitted.
/// 仅当字段为 `false` 且可以省略时返回 `true`。
fn boolean_is_false(value: &bool) -> bool {
    !*value
}

/// Persistent asset-index document stored inside the managed attachment index directory.
/// 存放在托管附件索引目录中的持久化资产索引文档。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub(super) struct AttachmentAssetIndexDocument {
    /// Flat asset record list tracked for the current workspace attachment store.
    /// 为当前工作区附件存储跟踪的扁平资产记录列表。
    #[serde(default)]
    pub(super) assets: Vec<AttachmentAssetRecord>,
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
    /// Whether generic file assets should participate in cleanup preview or execution.
    /// 通用文件资产是否应参与清理预览或执行。
    pub file: bool,
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
            file: true,
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
            ConversationAttachmentKind::File => self.file,
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
