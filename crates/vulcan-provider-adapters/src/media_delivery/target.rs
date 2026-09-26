//! Shared exact-target admission for conversation replay and independent media extensions.
//! 会话重放和独立媒体扩展共用的精确目标准入。

use super::{MediaDeliveryFacts, MediaDeliveryOrigin, MediaUploadAvailability};
use crate::{
    ConversationMediaUploadContract, InputResourceConstraint, ResourceReferencePolicy,
    UploadReferenceEncoding,
};
use vulcan_ai_contract::{ResourceKind, ResourceSourceKind};

/// Frozen model, protocol and upload facts; callers supply the host-owned purpose through their request planner.
/// 冻结模型、协议及上传事实；调用方通过请求规划器提供宿主持有的用途。
pub struct MediaDeliveryTarget<'a> {
    /// Exact target profile; provider names never choose media capabilities.
    /// 精确目标Profile；供应商名称绝不决定媒体能力。
    pub profile_id: &'a str,
    /// Native input kinds declared by the resolved model.
    /// 已解析模型声明的原生输入类型。
    pub model_inputs: &'a [ResourceKind],
    /// Resolved model restriction for this exact kind.
    /// 为本精确类型解析的模型限制。
    pub constraint: Option<&'a InputResourceConstraint>,
    /// Actual executable upload, excluding inline-only primitives.
    /// 实际可执行上传，不包含仅内联原语。
    pub upload: Option<&'a ConversationMediaUploadContract>,
}

impl MediaDeliveryTarget<'_> {
    /// Builds verified target facts for one local `kind`/`mime` source of `raw_bytes`, proven `origin` and native `count`.
    /// 为一个`kind`及`mime`本地来源构建已验证目标事实，使用`raw_bytes`、已证明`origin`及原生数量`count`。
    /// Returns pure planner input without source reads, uploads or policy selection.
    /// 返回纯规划器输入，不读取来源、不上传、不选择策略。
    pub fn facts(
        &self,
        kind: ResourceKind,
        mime: &str,
        raw_bytes: u64,
        origin: MediaDeliveryOrigin,
        count: u64,
    ) -> MediaDeliveryFacts {
        // Remote-only declarations are not permission to upload a local source implicitly.
        // 仅远程声明不代表可以隐式上传本地来源。
        let target_supported = count > 0
            && self.model_inputs.contains(&kind)
            && self.constraint.is_none_or(|constraint| {
                constraint.resource_kind == kind
                    && constraint.reference_policy != ResourceReferencePolicy::RemoteOnly
                    && constraint.max_count.is_none_or(|limit| count <= limit)
                    && (constraint.accepted_mime_types.is_empty()
                        || constraint
                            .accepted_mime_types
                            .iter()
                            .any(|accepted| accepted.eq_ignore_ascii_case(mime)))
            });
        // A real channel whose reference cannot be consumed is invalid, not an absent upload branch.
        // 返回引用无法消费的真实通道属于无效契约，而不是无上传分支。
        // Local materialization uploads one file per call; channel count limits are not per-message model limits.
        // 本地物化每次调用上传一个文件；通道数量限制不是模型的逐消息限制。
        let upload = match self.upload {
            None => MediaUploadAvailability::Absent,
            Some(contract)
                if contract.max_count.is_some_and(|limit| limit < 1)
                    || !vulcan_llm_core::protocol_profile_supports_media_source(
                        self.profile_id,
                        kind,
                        mime,
                        media_upload_source_kind(contract.reference_encoding),
                    ) =>
            {
                MediaUploadAvailability::Invalid
            }
            Some(contract) => MediaUploadAvailability::Available {
                max_bytes: contract.max_bytes,
            },
        };
        MediaDeliveryFacts {
            kind,
            raw_bytes,
            source_available: raw_bytes != 0,
            target_supported,
            target_max_bytes: self.constraint.and_then(|constraint| constraint.max_bytes),
            inline_supported: vulcan_llm_core::protocol_profile_supports_media_source(
                self.profile_id,
                kind,
                mime,
                ResourceSourceKind::InlineData,
            ),
            upload,
            origin,
        }
    }
}

/// Maps exact manifest `encoding` to its consuming source kind, without guessing from returned URL text.
/// 将精确清单`encoding`映射至消费来源类型，不根据返回URL文本猜测。
pub fn media_upload_source_kind(encoding: UploadReferenceEncoding) -> ResourceSourceKind {
    match encoding {
        UploadReferenceEncoding::ProviderFileId => ResourceSourceKind::ProviderFileId,
        UploadReferenceEncoding::ResponseUrl | UploadReferenceEncoding::Template => {
            ResourceSourceKind::RemoteUrl
        }
    }
}
