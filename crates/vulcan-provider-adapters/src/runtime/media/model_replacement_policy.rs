//! Admission for a fresh, host-owned model-replacement media extension.
//! 新的宿主所有模型代替媒体扩展的准入。

use crate::{
    ConversationMediaUploadContract, InputResourceConstraint, LocalMediaMaterialization,
    MediaDeliveryDecision, MediaDeliveryOrigin, MediaDeliveryPlanner, MediaDeliveryPolicy,
    MediaDeliveryPurpose, MediaDeliveryTarget,
};
use vulcan_ai_contract::ResourceKind;

/// Frozen target facts used before cache lookup, source reads or network upload.
/// 在缓存查询、源文件读取或网络上传之前使用的冻结目标事实。
pub(super) struct ModelReplacementMediaTarget<'a> {
    /// Complete policy snapshot owned by this invocation's resolved selection.
    /// 本次调用已解析选择持有的完整策略快照。
    pub policy: MediaDeliveryPolicy,
    /// Exact protocol profile, not a provider family guess.
    /// 精确协议 Profile，不根据供应商族猜测。
    pub profile_id: &'a str,
    /// Input kinds declared by the exact selected model.
    /// 精确选中模型声明的输入类型。
    pub model_inputs: &'a [ResourceKind],
    /// Optional model-resolved route restriction for this exact resource kind.
    /// 此精确资源类型可选的、经模型解析的路由限制。
    pub constraint: Option<&'a InputResourceConstraint>,
    /// Actual executable upload contract, excluding inline-only channels.
    /// 实际可执行上传契约，不包含仅内联通道。
    pub upload: Option<&'a ConversationMediaUploadContract>,
}

impl ModelReplacementMediaTarget<'_> {
    /// Plans one new local `kind`/`mime` source of verified `source_byte_len` bytes.
    /// 为一个新的本地 `kind`/`mime` 来源规划投递，其已验证大小为 `source_byte_len` 字节。
    /// Returns an exact materialization action or an explicit extension error; never an address-only success.
    /// 返回精确物化动作或明确扩展错误；绝不把仅地址结果当作成功。
    pub fn plan(
        self,
        kind: ResourceKind,
        mime: &str,
        source_byte_len: u64,
    ) -> Result<LocalMediaMaterialization, String> {
        // Both host purposes consume the same resolved target admission contract.
        // 两种宿主用途消费同一已解析目标准入契约。
        let target = MediaDeliveryTarget {
            profile_id: self.profile_id,
            model_inputs: self.model_inputs,
            constraint: self.constraint,
            upload: self.upload,
        };
        // The host entry owns IndependentExtension; neither AI arguments nor operation names select it.
        // 宿主入口持有 IndependentExtension；AI 参数与操作名称均不能选择此用途。
        let mut planner =
            MediaDeliveryPlanner::new(self.policy, MediaDeliveryPurpose::IndependentExtension)?;
        match planner.plan(target.facts(
            kind,
            mime,
            source_byte_len,
            MediaDeliveryOrigin::NewIntent,
            1,
        ))? {
            MediaDeliveryDecision::Inline { .. } => {
                Ok(LocalMediaMaterialization::Inline { source_byte_len })
            }
            MediaDeliveryDecision::Upload => {
                Ok(LocalMediaMaterialization::ProviderUpload { source_byte_len })
            }
            MediaDeliveryDecision::Reject { reason } => Err(format!(
                "Media extension cannot understand/process this file ({source_byte_len} bytes): {}. Native content was not sent.",
                reason.explanation()
            )),
            MediaDeliveryDecision::Attachment { .. }
            | MediaDeliveryDecision::RestoreOriginalUpload => Err(String::from(
                "Independent media extension returned an invalid historical delivery action.",
            )),
        }
    }
}
