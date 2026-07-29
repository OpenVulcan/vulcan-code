//! Composer attachment capability helpers.
//! 输入区附件能力辅助逻辑。

use super::*;

/// Creates composer attachment gates from the exact active public-model snapshot.
/// 根据精确的当前公共模型快照创建输入区附件门控。
///
/// # Parameters
/// 参数
/// `capabilities` is the credential-scoped model capability snapshot resolved by the public runtime.
/// `capabilities` 是公共运行时解析的凭据作用域模型能力快照。
///
/// # Returns
/// 返回值
/// Attachment capability summaries for the composer controls.
/// 输入区控件使用的附件能力摘要。
pub(crate) fn composer_attachment_capabilities(
    capabilities: Option<&vulcan_ai_contract::ModelCapabilities>,
) -> Vec<ComposerAttachmentCapabilitySummary> {
    composer_attachment_kinds()
        .into_iter()
        .map(|kind| {
            let resource_kind = match &kind {
                ConversationAttachmentKind::Image => vulcan_ai_contract::ResourceKind::Image,
                ConversationAttachmentKind::Audio => vulcan_ai_contract::ResourceKind::Audio,
                ConversationAttachmentKind::Video => vulcan_ai_contract::ResourceKind::Video,
                ConversationAttachmentKind::Pdf => vulcan_ai_contract::ResourceKind::File,
                ConversationAttachmentKind::File => vulcan_ai_contract::ResourceKind::File,
            };
            let native = capabilities
                .is_some_and(|capabilities| capabilities.input_resources.contains(&resource_kind));
            ComposerAttachmentCapabilitySummary {
                kind: kind.modality_name().to_string(),
                label: composer_attachment_kind_label(&kind).to_string(),
                enabled: native,
                native,
                bridged: false,
                accept: composer_attachment_kind_accept(&kind).to_string(),
            }
        })
        .collect()
}
