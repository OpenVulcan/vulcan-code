//! Pure delivery planning: no file reads, uploads, history mutations, or protocol fallback.
//! 纯投递规划：不读取文件、不上传、不修改历史、不回退协议。

use super::{MediaDeliveryPolicy, MediaDeliveryPurpose, MediaPurposePolicy};
use vulcan_ai_contract::ResourceKind;

/// Verified availability of a real upload and its consuming reference field.
/// 真实上传及其引用消费字段的已验证可用性。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaUploadAvailability {
    /// No applicable real upload; inline-only primitives belong in this branch.
    /// 没有适用的真实上传；仅内联原语属于此分支。
    Absent,
    /// Exact selected upload contract, with an optional declared raw-byte limit.
    /// 精确选中的上传契约，包含可选的已声明原始字节限制。
    Available {
        /// Maximum raw size of the selected interface; absence means no declared size limit.
        /// 选中接口的原始大小上限；缺失表示接口未声明大小限制。
        max_bytes: Option<u64>,
    },
    /// A declared upload exists but is invalid; this is not equivalent to no upload.
    /// 存在已声明上传但契约无效；不能视为没有上传。
    Invalid,
}

/// Original delivery identity supplied by the authoritative message history.
/// 由权威消息历史提供的原始投递身份。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDeliveryOrigin {
    /// A new explicit attachment or an independent tool invocation.
    /// 新的显式附件或独立工具调用。
    NewIntent,
    /// Proven original inline delivery, not inferred from a cache miss.
    /// 已证明的原始内联投递，不能根据缓存未命中推断。
    HistoricalInline,
    /// Proven original uploaded delivery with exact model/route ownership checks.
    /// 已证明的原始上传投递，具备精确模型与路由归属校验。
    HistoricalUpload {
        /// Whether the current target is the original model and route identity.
        /// 当前目标是否为原始模型及路由身份。
        original_target_matches: bool,
        /// Whether the original reference remains valid, scoped, unexpired and consumable.
        /// 原始引用是否仍有效、作用域匹配、未过期且可消费。
        reference_usable: bool,
    },
}

/// Explicit reason why native content is not included in this request.
/// 本轮请求没有包含原生内容的明确原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDeliveryRejection {
    /// The original resource cannot be safely resolved or verified.
    /// 原始资源无法安全解析或校验。
    SourceUnavailable,
    /// The chosen target does not support the kind, MIME or reference contract.
    /// 选中目标不支持类型、MIME 或引用契约。
    TargetUnsupported,
    /// A target hard limit is exceeded; soft policy cannot override it.
    /// 超出目标硬限制，软策略不能覆盖。
    TargetSizeLimit,
    /// A selected upload interface is too small, even if inline thresholds are larger.
    /// 选中上传接口的大小限制更小，即使内联阈值更大也不能绕过。
    UploadSizeLimit,
    /// The declared upload contract is invalid rather than absent.
    /// 已声明上传契约无效，而不是不存在。
    UploadContractInvalid,
    /// An admitted new upload failed during preparation; no alternate inline transfer is authorized.
    /// 已准入新上传在准备期间失败；不授权替代内联传输。
    UploadPreparationFailed,
    /// The target cannot consume inline content of the declared kind and MIME.
    /// 目标无法消费已声明类型和 MIME 的内联内容。
    InlineUnsupported,
    /// Inline is disabled or the raw bytes exceed the selected soft threshold.
    /// 内联已禁用，或原始字节超过选中的软阈值。
    InlineThreshold,
    /// Combined encoded content would exceed this request's frozen budget.
    /// 累计编码内容将超过本请求冻结预算。
    EncodedBudget,
    /// Uploaded history belongs to another model and may only be shown as an address.
    /// 上传历史属于另一模型，只能展示地址。
    HistoricalTargetChanged,
    /// The original upload reference is unusable; replay must not create a new upload.
    /// 原始上传引用不可用；重放不能发起新上传。
    OriginalReferenceUnavailable,
    /// Older prompt data contains no proof of its original delivery mode.
    /// 旧提示词数据没有原投递方式的证明。
    HistoricalOriginUnknown,
    /// The original intent explicitly retained only an attachment address.
    /// 原意图明确仅保留附件地址。
    OriginalAttachmentOnly,
    /// The conversation was compacted after this media; earlier media keeps only its address.
    /// 此媒体之后会话已被压缩；更早的媒体仅保留地址。
    CompactedHistory,
}

impl MediaDeliveryRejection {
    /// Returns the stable snake_case code that clients match instead of the explanation text.
    /// 返回客户端用于匹配的稳定 snake_case 代码，而非说明文本。
    pub fn code(self) -> &'static str {
        match self {
            Self::SourceUnavailable => "source_unavailable",
            Self::TargetUnsupported => "target_unsupported",
            Self::TargetSizeLimit => "target_size_limit",
            Self::UploadSizeLimit => "upload_size_limit",
            Self::UploadContractInvalid => "upload_contract_invalid",
            Self::UploadPreparationFailed => "upload_preparation_failed",
            Self::InlineUnsupported => "inline_unsupported",
            Self::InlineThreshold => "inline_threshold",
            Self::EncodedBudget => "encoded_budget",
            Self::HistoricalTargetChanged => "historical_target_changed",
            Self::OriginalReferenceUnavailable => "original_reference_unavailable",
            Self::HistoricalOriginUnknown => "historical_origin_unknown",
            Self::OriginalAttachmentOnly => "original_attachment_only",
            Self::CompactedHistory => "compacted_history",
        }
    }

    /// Describes this rejection for AI-facing errors and unsent attachment metadata.
    /// 为面向 AI 的错误与未发送附件元数据描述此拒绝原因。
    /// Returns a stable English explanation without source paths, credentials or provider URLs.
    /// 返回稳定英文说明，不包含来源路径、凭据或供应商 URL。
    pub fn explanation(self) -> &'static str {
        match self {
            Self::SourceUnavailable => "the original file is unavailable or could not be verified",
            Self::TargetUnsupported => {
                "the target does not support this media type or reference form"
            }
            Self::TargetSizeLimit => "the file is too large for the target model or protocol",
            Self::UploadSizeLimit => "the file is too large for the selected upload interface",
            Self::UploadContractInvalid => {
                "the declared upload contract is invalid or its reference cannot be consumed"
            }
            Self::UploadPreparationFailed => {
                "the selected upload could not be prepared; media content was not sent and no inline fallback was used"
            }
            Self::InlineUnsupported => "the target cannot consume this media inline",
            Self::InlineThreshold => {
                "the file exceeds the configured inline size limit or inline delivery is disabled"
            }
            Self::EncodedBudget => "the combined encoded files are too large for this request",
            Self::HistoricalTargetChanged => {
                "the original upload belongs to another model or route"
            }
            Self::OriginalReferenceUnavailable => {
                "the original upload reference is unavailable, expired or out of scope"
            }
            Self::HistoricalOriginUnknown => {
                "the original delivery of this historical attachment is unknown"
            }
            Self::OriginalAttachmentOnly => {
                "the original intent retained this resource as an address-only attachment"
            }
            Self::CompactedHistory => {
                "the conversation was compacted after this file, so earlier media is kept as an address only"
            }
        }
    }
}

/// Materialization action, keeping conversation attachment fallback separate from extension failure.
/// 物化动作，将会话附件回退与扩展失败分开表达。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDeliveryDecision {
    /// Inline original bytes; the planner has already reserved the encoded budget.
    /// 内联原始字节；规划器已预留编码预算。
    Inline {
        /// Exact Base64 byte count, excluding protocol framing.
        /// 精确 Base64 字节数，不含协议封装。
        encoded_bytes: u64,
    },
    /// Upload a new intent or reuse its exact valid cached reference.
    /// 上传新意图，或复用其精确有效缓存引用。
    Upload,
    /// Restore the original historical reference without uploading.
    /// 恢复原始历史引用，不执行上传。
    RestoreOriginalUpload,
    /// Retain the attachment and project an address plus an explicit unsent-content reason.
    /// 保留附件，投影地址和未发送内容的明确原因。
    Attachment {
        /// Exact reason to expose in safe attachment metadata.
        /// 安全附件元数据中应展示的精确原因。
        reason: MediaDeliveryRejection,
    },
    /// Fail an independent extension instead of pretending the file was understood.
    /// 使独立扩展失败，不能假装已理解文件。
    Reject {
        /// Exact failure reason for the caller's structured tool error.
        /// 调用者结构化工具错误的精确失败原因。
        reason: MediaDeliveryRejection,
    },
}

/// Facts verified by the resource owner and frozen target before side effects.
/// 资源所有者与冻结目标在副作用之前确认的事实。
#[derive(Debug, Clone, Copy)]
pub struct MediaDeliveryFacts {
    /// Exact media kind; unsupported video must never be relabelled as a file.
    /// 精确媒体类型；不能把不支持的视频改标为文件。
    pub kind: ResourceKind,
    /// Original raw size from verified resource metadata.
    /// 来自已校验资源元数据的原始大小。
    pub raw_bytes: u64,
    /// Whether the source identity and safe availability have been verified.
    /// 是否已校验源身份及安全可用性。
    pub source_available: bool,
    /// Whether model, protocol, MIME and input position accept this resource.
    /// 模型、协议、MIME 和输入位置是否接受此资源。
    pub target_supported: bool,
    /// Applicable hard raw-byte bound of the chosen target, if declared.
    /// 选中目标声明的适用原始字节硬上限。
    pub target_max_bytes: Option<u64>,
    /// Whether the exact target mapping can encode and consume inline content.
    /// 精确目标映射是否能够编码并消费内联内容。
    pub inline_supported: bool,
    /// Real upload availability verified independently of inline primitives.
    /// 独立于内联原语验证的真实上传可用性。
    pub upload: MediaUploadAvailability,
    /// Proven original intent identity; request-local metadata is not its authority.
    /// 已证明的原始意图身份；请求局部元数据不是其权威来源。
    pub origin: MediaDeliveryOrigin,
}

/// Per-request planner holding a frozen purpose and cumulative encoded allocation.
/// 持有冻结用途与累计编码分配量的逐请求规划器。
#[derive(Debug, Clone)]
pub struct MediaDeliveryPlanner {
    /// Host-owned execution purpose.
    /// 宿主持有的执行用途。
    purpose: MediaDeliveryPurpose,
    /// Immutable policy snapshot selected for that purpose.
    /// 为该用途选择的不可变策略快照。
    policy: MediaPurposePolicy,
    /// Base64 bytes reserved for earlier resources in canonical input order.
    /// 按规范输入顺序为此前资源预留的 Base64 字节。
    encoded_bytes_reserved: u64,
}

impl MediaDeliveryPlanner {
    /// Freezes validated `policy` for trusted `purpose`; invalid settings return an error.
    /// 为可信 `purpose` 冻结已校验 `policy`；设置无效时返回错误。
    pub fn new(policy: MediaDeliveryPolicy, purpose: MediaDeliveryPurpose) -> Result<Self, String> {
        policy.validate()?;
        Ok(Self {
            purpose,
            policy: policy.for_purpose(purpose),
            encoded_bytes_reserved: 0,
        })
    }

    /// Returns the exact Base64 bytes reserved so far, excluding protocol overhead.
    /// 返回目前已预留的精确 Base64 字节数，不含协议开销。
    pub fn encoded_bytes_reserved(&self) -> u64 {
        self.encoded_bytes_reserved
    }

    /// Plans verified `facts` without I/O; only inline outcomes reserve budget.
    /// 无输入输出地规划已校验 `facts`；仅内联结果预留预算。
    /// Returns an action or a trusted-entry misuse error for extension history replay.
    /// 返回动作；独立扩展错误使用历史重放时返回可信入口误用错误。
    pub fn plan(&mut self, facts: MediaDeliveryFacts) -> Result<MediaDeliveryDecision, String> {
        if self.purpose == MediaDeliveryPurpose::IndependentExtension
            && facts.origin != MediaDeliveryOrigin::NewIntent
        {
            return Err(String::from(
                "independent media extensions cannot replay conversation delivery history",
            ));
        }
        // Historical ownership wins even when the newly selected target supports uploading.
        // 即使新选目标支持上传，历史归属仍然优先。
        if let MediaDeliveryOrigin::HistoricalUpload {
            original_target_matches: false,
            ..
        } = facts.origin
        {
            return Ok(self.unavailable(MediaDeliveryRejection::HistoricalTargetChanged));
        }
        if !facts.source_available {
            return Ok(self.unavailable(MediaDeliveryRejection::SourceUnavailable));
        }
        if !facts.target_supported {
            return Ok(self.unavailable(MediaDeliveryRejection::TargetUnsupported));
        }
        if facts
            .target_max_bytes
            .is_some_and(|limit| facts.raw_bytes > limit)
        {
            return Ok(self.unavailable(MediaDeliveryRejection::TargetSizeLimit));
        }
        // Current interface hard bounds also constrain historical replay; history only bypasses soft preferences.
        // 当前接口硬边界同样限制历史重放；历史仅绕过软偏好。
        if let MediaUploadAvailability::Available {
            max_bytes: Some(limit),
        } = facts.upload
            && facts.raw_bytes > limit
        {
            return Ok(self.unavailable(MediaDeliveryRejection::UploadSizeLimit));
        }
        match facts.origin {
            MediaDeliveryOrigin::HistoricalUpload {
                reference_usable, ..
            } => {
                return Ok(if reference_usable {
                    MediaDeliveryDecision::RestoreOriginalUpload
                } else {
                    self.unavailable(MediaDeliveryRejection::OriginalReferenceUnavailable)
                });
            }
            MediaDeliveryOrigin::HistoricalInline => {
                // New-intent thresholds and upload preferences cannot change proven inline history.
                // 新意图阈值与上传偏好不能改变已证明的内联历史。
                return Ok(if facts.inline_supported {
                    self.reserve_inline(facts.raw_bytes)
                } else {
                    self.unavailable(MediaDeliveryRejection::InlineUnsupported)
                });
            }
            MediaDeliveryOrigin::NewIntent => {}
        }
        // The chosen interface's hard limit is checked before any soft inline preference.
        // 先检查选中接口的硬限制，再处理任何软内联偏好。
        match facts.upload {
            MediaUploadAvailability::Invalid => {
                return Ok(self.unavailable(MediaDeliveryRejection::UploadContractInvalid));
            }
            MediaUploadAvailability::Available {
                max_bytes: Some(limit),
            } if facts.raw_bytes > limit => {
                return Ok(self.unavailable(MediaDeliveryRejection::UploadSizeLimit));
            }
            _ => {}
        }
        // Threshold selection follows verified real-upload availability, not channel-array length.
        // 阈值根据已验证的真实上传可用性选择，不依据通道数组长度。
        let thresholds = self.policy.thresholds(facts.kind);
        match facts.upload {
            MediaUploadAvailability::Available { .. } => {
                if facts.inline_supported
                    && thresholds.with_upload_max_bytes != 0
                    && facts.raw_bytes <= thresholds.with_upload_max_bytes
                    && self.inline_fits(facts.raw_bytes)
                {
                    Ok(self.reserve_inline(facts.raw_bytes))
                } else {
                    Ok(MediaDeliveryDecision::Upload)
                }
            }
            MediaUploadAvailability::Absent => Ok(if !facts.inline_supported {
                self.unavailable(MediaDeliveryRejection::InlineUnsupported)
            } else if thresholds.without_upload_max_bytes == 0
                || facts.raw_bytes > thresholds.without_upload_max_bytes
            {
                self.unavailable(MediaDeliveryRejection::InlineThreshold)
            } else {
                self.reserve_inline(facts.raw_bytes)
            }),
            MediaUploadAvailability::Invalid => {
                unreachable!("invalid upload was rejected before planning")
            }
        }
    }

    /// Tests whether `raw_bytes` can be encoded within the remaining budget; returns no side effects.
    /// 检查 `raw_bytes` 编码后能否放入剩余预算；返回布尔值且不产生副作用。
    fn inline_fits(&self, raw_bytes: u64) -> bool {
        media_base64_encoded_len(raw_bytes).is_some_and(|encoded| {
            encoded <= self.policy.max_total_encoded_bytes - self.encoded_bytes_reserved
        })
    }

    /// Reserves `raw_bytes` once when it fits; otherwise returns the purpose-specific failure.
    /// `raw_bytes` 可容纳时仅预留一次；否则返回用途对应的失败结果。
    fn reserve_inline(&mut self, raw_bytes: u64) -> MediaDeliveryDecision {
        match media_base64_encoded_len(raw_bytes) {
            Some(encoded_bytes) if self.inline_fits(raw_bytes) => {
                self.encoded_bytes_reserved += encoded_bytes;
                MediaDeliveryDecision::Inline { encoded_bytes }
            }
            _ => self.unavailable(MediaDeliveryRejection::EncodedBudget),
        }
    }

    /// Converts `reason` to an attachment projection or an explicit extension rejection.
    /// 将 `reason` 转换为附件投影或明确的扩展拒绝结果。
    fn unavailable(&self, reason: MediaDeliveryRejection) -> MediaDeliveryDecision {
        match self.purpose {
            MediaDeliveryPurpose::Conversation => MediaDeliveryDecision::Attachment { reason },
            MediaDeliveryPurpose::IndependentExtension => MediaDeliveryDecision::Reject { reason },
        }
    }
}

/// Returns `4 * ceil(raw_bytes / 3)` without overflowing; unrepresentable sizes return `None`.
/// 无溢出地返回 `4 * ceil(raw_bytes / 3)`；无法表示的大小返回 `None`。
pub fn media_base64_encoded_len(raw_bytes: u64) -> Option<u64> {
    (raw_bytes / 3)
        .checked_mul(4)?
        .checked_add(if raw_bytes.is_multiple_of(3) { 0 } else { 4 })
}
