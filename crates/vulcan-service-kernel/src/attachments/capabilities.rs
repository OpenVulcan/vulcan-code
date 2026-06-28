//! Composer attachment capability helpers.
//! 输入区附件能力辅助逻辑。

use super::*;

/// Creates composer attachment gates using the same capability resolver as runtime execution.
/// 使用与运行时执行相同的能力解析器创建输入区附件门控。
///
/// # Parameters
/// 参数
/// `provider_configuration` contains the active model and provider settings.
/// `provider_configuration` 包含当前模型与 Provider 设置。
/// `ai_tool_configuration` contains the persisted AI tool bridge configuration.
/// `ai_tool_configuration` 包含持久化 AI 工具桥接配置。
///
/// # Returns
/// 返回值
/// Attachment capability summaries for the composer controls.
/// 输入区控件使用的附件能力摘要。
pub(crate) fn composer_attachment_capabilities(
    provider_configuration: &ProviderConfigurationFile,
    ai_tool_configuration: &PersistedAiToolConfiguration,
) -> Vec<ComposerAttachmentCapabilitySummary> {
    let capabilities =
        active_workbench_model_capabilities(provider_configuration, ai_tool_configuration);
    [
        ConversationAttachmentKind::Image,
        ConversationAttachmentKind::Video,
        ConversationAttachmentKind::Audio,
        ConversationAttachmentKind::Pdf,
    ]
    .into_iter()
    .map(|kind| {
        let native = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.supports_attachment_kind(kind.clone()));
        let enabled = capabilities
            .as_ref()
            .is_some_and(|capabilities| capabilities.allows_staged_attachment_kind(kind.clone()));
        ComposerAttachmentCapabilitySummary {
            kind: kind.modality_name().to_string(),
            label: composer_attachment_kind_label(&kind).to_string(),
            enabled,
            native,
            bridged: enabled && !native,
            accept: composer_attachment_kind_accept(&kind).to_string(),
        }
    })
    .collect()
}
