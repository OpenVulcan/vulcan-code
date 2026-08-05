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
/// `bridged_image_understanding` enables image attachments when the host has a valid selected image-understanding tool.
/// `bridged_image_understanding` 在宿主存在有效选中的图片理解工具时启用图片附件。
///
/// # Returns
/// 返回值
/// Attachment capability summaries for the composer controls.
/// 输入区控件使用的附件能力摘要。
pub(crate) fn composer_attachment_capabilities(
    capabilities: Option<&vulcan_ai_contract::ModelCapabilities>,
    bridged_image_understanding: bool,
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
            let bridged = matches!(kind, ConversationAttachmentKind::Image)
                && !native
                && bridged_image_understanding;
            ComposerAttachmentCapabilitySummary {
                kind: kind.modality_name().to_string(),
                label: composer_attachment_kind_label(&kind).to_string(),
                enabled: native || bridged,
                native,
                bridged,
                accept: composer_attachment_kind_accept(&kind).to_string(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies native image and video declarations directly control composer attachment gates.
    /// 验证模型原生图片与视频声明会直接控制输入区附件门控。
    #[test]
    fn native_resource_capabilities_enable_only_declared_media() {
        // capabilities differs from the default only by its declared native input resources.
        // capabilities 仅通过已声明的原生输入资源区别于默认值。
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            input_resources: vec![
                vulcan_ai_contract::ResourceKind::Image,
                vulcan_ai_contract::ResourceKind::Video,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let summaries = composer_attachment_capabilities(Some(&capabilities), false);
        let image = summaries
            .iter()
            .find(|summary| summary.kind == "image")
            .expect("image attachment summary");
        let video = summaries
            .iter()
            .find(|summary| summary.kind == "video")
            .expect("video attachment summary");
        let audio = summaries
            .iter()
            .find(|summary| summary.kind == "audio")
            .expect("audio attachment summary");

        assert!(image.enabled && image.native);
        assert!(video.enabled && video.native);
        assert!(!audio.enabled && !audio.native);
    }

    /// Verifies the selected image-understanding bridge enables only non-native image input.
    /// 验证选中的图片理解桥接只会启用非原生图片输入。
    #[test]
    fn image_understanding_bridge_enables_image_input() {
        let capabilities = vulcan_ai_contract::ModelCapabilities::default();
        let summaries = composer_attachment_capabilities(Some(&capabilities), true);
        let image = summaries
            .iter()
            .find(|summary| summary.kind == "image")
            .expect("image attachment summary");
        let video = summaries
            .iter()
            .find(|summary| summary.kind == "video")
            .expect("video attachment summary");

        assert!(image.enabled && !image.native && image.bridged);
        assert!(!video.enabled && !video.native && !video.bridged);
    }
}
