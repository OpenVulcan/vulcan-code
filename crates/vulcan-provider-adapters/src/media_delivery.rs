//! Shared, bounded media policy for conversation and independent extension delivery.
//! 主会话与独立扩展共用的有界媒体投递策略。

mod decision;
mod target;
#[cfg(test)]
mod tests;

pub use decision::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
pub use target::*;
use vulcan_ai_contract::ResourceKind;

/// Version of the exact media settings contract, independent of manifest generations.
/// 精确媒体设置契约版本，与清单代际相互独立。
pub const MEDIA_DELIVERY_POLICY_VERSION: u32 = 1;
/// Maximum encoded media allocation per request, not an upstream capability declaration.
/// 单请求编码媒体分配上限，不代表上游能力声明。
pub const MEDIA_INLINE_ENCODED_HARD_LIMIT_BYTES: u64 = 64 * 1024 * 1024;
/// Largest raw threshold whose Base64 representation fits the host allocation ceiling.
/// Base64 表示能够放入宿主分配上限的最大原始字节阈值。
pub const MEDIA_INLINE_RAW_HARD_LIMIT_BYTES: u64 = MEDIA_INLINE_ENCODED_HARD_LIMIT_BYTES / 4 * 3;

/// Trusted execution purpose supplied by host entry points, never by model arguments.
/// 由宿主入口提供的可信执行用途，不能由模型参数指定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaDeliveryPurpose {
    /// User and agent conversations, including child-agent conversations.
    /// 用户及智能体会话，包括子智能体会话。
    Conversation,
    /// A separate media extension invocation whose input is not conversation history.
    /// 输入不进入会话历史的独立媒体扩展调用。
    IndependentExtension,
}

/// Raw-byte inline thresholds selected by the availability of a real upload contract.
/// 按真实上传契约是否可用选择的原始字节内联阈值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaInlineThresholds {
    /// Inclusive inline threshold when a valid upload contract exists; zero disables inline.
    /// 存在有效上传契约时允许内联的包含边界阈值；零禁用内联。
    pub with_upload_max_bytes: u64,
    /// Inclusive inline threshold without an upload contract; zero disables inline.
    /// 不存在上传契约时允许内联的包含边界阈值；零禁用内联。
    pub without_upload_max_bytes: u64,
}

/// Independent limits for all four media kinds and their combined encoded payload.
/// 四类媒体及其累计编码载荷的独立限制。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaPurposePolicy {
    /// Image input thresholds.
    /// 图片输入阈值。
    pub image: MediaInlineThresholds,
    /// Audio input thresholds.
    /// 音频输入阈值。
    pub audio: MediaInlineThresholds,
    /// Video input thresholds.
    /// 视频输入阈值。
    pub video: MediaInlineThresholds,
    /// File input thresholds, including PDF documents.
    /// 文件输入阈值，包括 PDF 文档。
    pub file: MediaInlineThresholds,
    /// Combined Base64 bytes, excluding data-URL prefixes and final protocol JSON overhead.
    /// 累计 Base64 字节，不含数据地址前缀与最终协议 JSON 开销。
    pub max_total_encoded_bytes: u64,
}

impl MediaPurposePolicy {
    /// Builds default kind limits from raw and encoded budgets; returns a complete policy.
    /// 根据原始与编码预算构建各类型默认限制，返回完整策略。
    const fn defaults(raw_bytes: u64, encoded_bytes: u64) -> Self {
        // The four copied values are independent fields after construction.
        // 构建后四个复制值为相互独立的字段。
        let thresholds = MediaInlineThresholds {
            with_upload_max_bytes: 0,
            without_upload_max_bytes: raw_bytes,
        };
        Self {
            image: thresholds,
            audio: thresholds,
            video: thresholds,
            file: thresholds,
            max_total_encoded_bytes: encoded_bytes,
        }
    }

    /// Returns thresholds for the exact declared kind; PDF retains the file kind.
    /// 返回精确声明类型的阈值；PDF 保持文件类型。
    pub const fn thresholds(self, kind: ResourceKind) -> MediaInlineThresholds {
        match kind {
            ResourceKind::Image => self.image,
            ResourceKind::Audio => self.audio,
            ResourceKind::Video => self.video,
            ResourceKind::File => self.file,
        }
    }

    /// Validates every field against host ceilings; `purpose` labels explicit errors.
    /// 对照宿主上限校验全部字段；`purpose` 标注明确错误，合法时返回成功。
    fn validate(self, purpose: &str) -> Result<(), String> {
        for (kind, thresholds) in [
            ("image", self.image),
            ("audio", self.audio),
            ("video", self.video),
            ("file", self.file),
        ] {
            for (mode, bytes) in [
                ("with_upload_max_bytes", thresholds.with_upload_max_bytes),
                (
                    "without_upload_max_bytes",
                    thresholds.without_upload_max_bytes,
                ),
            ] {
                if bytes > MEDIA_INLINE_RAW_HARD_LIMIT_BYTES {
                    return Err(format!(
                        "media_delivery_policy.{purpose}.{kind}.{mode} exceeds the host raw-byte limit {MEDIA_INLINE_RAW_HARD_LIMIT_BYTES}"
                    ));
                }
            }
        }
        if self.max_total_encoded_bytes > MEDIA_INLINE_ENCODED_HARD_LIMIT_BYTES {
            return Err(format!(
                "media_delivery_policy.{purpose}.max_total_encoded_bytes exceeds the host encoded-byte limit {MEDIA_INLINE_ENCODED_HARD_LIMIT_BYTES}"
            ));
        }
        Ok(())
    }
}

/// Versioned settings snapshot with sixteen independently writable raw-byte thresholds.
/// 包含十六项可独立写入原始字节阈值的版本化设置快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaDeliveryPolicy {
    /// Exact schema version; unsupported values are rejected rather than reinterpreted.
    /// 精确结构版本；不支持的值直接拒绝，不能重新解释。
    pub version: u32,
    /// Main conversation and historical replay settings.
    /// 主会话与历史重放设置。
    pub conversation: MediaPurposePolicy,
    /// Separate AI extension settings; these never inherit conversation thresholds.
    /// 独立 AI 扩展设置，不能继承会话阈值。
    pub independent_extension: MediaPurposePolicy,
}

impl Default for MediaDeliveryPolicy {
    /// Returns upload-first defaults with 512 KiB conversation and 10 MiB extension inline limits.
    /// 返回优先上传的默认策略，会话内联限制为 512 KiB，扩展为 10 MiB。
    fn default() -> Self {
        Self {
            version: MEDIA_DELIVERY_POLICY_VERSION,
            conversation: MediaPurposePolicy::defaults(512 * 1024, 2 * 1024 * 1024),
            independent_extension: MediaPurposePolicy::defaults(10 * 1024 * 1024, 16 * 1024 * 1024),
        }
    }
}

impl MediaDeliveryPolicy {
    /// Validates version and all limits; returns an explicit error without normalizing values.
    /// 校验版本及全部限制；遇到错误明确返回，不规范化改写数值。
    pub fn validate(self) -> Result<(), String> {
        if self.version != MEDIA_DELIVERY_POLICY_VERSION {
            return Err(format!(
                "unsupported media_delivery_policy.version: {}",
                self.version
            ));
        }
        self.conversation.validate("conversation")?;
        self.independent_extension.validate("independent_extension")
    }

    /// Selects the complete settings for a host-supplied purpose and returns a frozen copy.
    /// 根据宿主提供的用途选择完整设置，返回冻结副本。
    pub const fn for_purpose(self, purpose: MediaDeliveryPurpose) -> MediaPurposePolicy {
        match purpose {
            MediaDeliveryPurpose::Conversation => self.conversation,
            MediaDeliveryPurpose::IndependentExtension => self.independent_extension,
        }
    }

    /// Hashes validated settings into a stable revision; returns errors instead of hashing invalid data.
    /// 将已校验设置散列为稳定修订标识；设置无效时返回错误，不生成指纹。
    pub fn fingerprint(self) -> Result<String, String> {
        self.validate()?;
        // Serde struct order is fixed, and this snapshot contains no maps or credentials.
        // Serde 结构体顺序固定，且此快照不包含映射或凭据。
        let encoded = serde_json::to_vec(&self).map_err(|error| error.to_string())?;
        Ok(hex::encode(Sha256::digest(encoded)))
    }
}

impl crate::PersistedAiConfiguration {
    /// Returns configured policy or runtime defaults without modifying persisted state.
    /// 返回已配置策略或运行时默认策略，不修改持久化状态。
    pub fn effective_media_delivery_policy(&self) -> MediaDeliveryPolicy {
        self.media_delivery_policy.unwrap_or_default()
    }

    /// Validates and saves `policy` atomically in memory; invalid input leaves the owner unchanged.
    /// 校验并在内存原子保存 `policy`；输入无效时保持所有者不变。
    pub fn set_media_delivery_policy(&mut self, policy: MediaDeliveryPolicy) -> Result<(), String> {
        policy.validate()?;
        self.media_delivery_policy = Some(policy);
        Ok(())
    }

    /// Explicitly restores inherited runtime defaults; returns without altering unrelated settings.
    /// 显式恢复继承的运行时默认值；返回时不改变无关设置。
    pub fn reset_media_delivery_policy(&mut self) {
        self.media_delivery_policy = None;
    }
}
