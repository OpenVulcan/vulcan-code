//! Composer attachment capability helpers.
//! 输入区附件能力辅助逻辑。

use super::*;
use std::collections::BTreeSet;

/// Creates composer attachment gates from the exact active public-model snapshot.
/// 根据精确的当前公共模型快照创建输入区附件门控。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` identifies the exact active wire projector when a model is selected.
/// `protocol_profile_id` 在模型已选择时标识精确的活动 Wire 投影器。
/// `capabilities` is the credential-scoped model capability snapshot resolved by the public runtime.
/// `capabilities` 是公共运行时解析的凭据作用域模型能力快照。
/// `bridged_media_kinds` contains every media resource backed by a valid selected understanding tool.
/// `bridged_media_kinds` 包含由有效已选理解工具承载的全部媒体资源。
///
/// # Returns
/// 返回值
/// Attachment capability summaries for the composer controls.
/// 输入区控件使用的附件能力摘要。
pub(crate) fn composer_attachment_capabilities(
    protocol_profile_id: Option<&str>,
    capabilities: Option<&vulcan_ai_contract::ModelCapabilities>,
    bridged_media_kinds: &BTreeSet<vulcan_ai_contract::ResourceKind>,
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
            // native intersects the model declaration with the exact frozen protocol projector.
            // native 将模型声明与精确冻结协议投影器取交集。
            let native = capabilities.is_some_and(|capabilities| {
                capabilities.input_resources.contains(&resource_kind)
                    && protocol_profile_id.is_some_and(|protocol_profile_id| {
                        vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_attachment_kind(
                            protocol_profile_id,
                            kind.clone(),
                        )
                    })
            });
            // bridged is active only when the selected model cannot consume the resource natively.
            // bridged 仅在所选模型无法原生消费该资源时生效。
            let bridged = !native && bridged_media_kinds.contains(&resource_kind);
            ComposerAttachmentCapabilitySummary {
                kind: kind.modality_name().to_string(),
                label: composer_attachment_kind_label(&kind).to_string(),
                // Admission is capability-independent; native and bridged remain delivery facts.
                // 准入与能力无关；native 与 bridged 继续作为传输事实保留。
                enabled: true,
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

    /// Verifies attachment admission stays open while native support remains route-specific.
    /// 验证附件准入保持开放，同时原生支持仍由精确路由决定。
    #[test]
    fn attachment_admission_is_independent_from_native_media_support() {
        // capabilities differs from the default only by its declared native input resources.
        // capabilities 仅通过已声明的原生输入资源区别于默认值。
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            input_resources: vec![
                vulcan_ai_contract::ResourceKind::Image,
                vulcan_ai_contract::ResourceKind::Video,
            ],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };

        let summaries = composer_attachment_capabilities(
            Some(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
            Some(&capabilities),
            &BTreeSet::new(),
        );
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
        let file = summaries
            .iter()
            .find(|summary| summary.kind == "file")
            .expect("file attachment summary");

        assert!(image.enabled && image.native);
        assert!(video.enabled && video.native);
        assert!(audio.enabled && !audio.native);
        assert!(file.enabled && !file.native);
    }

    /// Verifies selected understanding bridges enable every matching non-native media input.
    /// 验证选中的理解桥接会启用每一种匹配的非原生媒体输入。
    #[test]
    fn media_understanding_bridges_enable_matching_media_input() {
        let capabilities = vulcan_ai_contract::ModelCapabilities::default();
        // bridgedMediaKinds is the exact valid host-tool set for this session.
        // bridgedMediaKinds 是当前会话精确有效的宿主工具集合。
        let bridged_media_kinds = BTreeSet::from([
            vulcan_ai_contract::ResourceKind::Image,
            vulcan_ai_contract::ResourceKind::Video,
            vulcan_ai_contract::ResourceKind::Audio,
        ]);
        let summaries = composer_attachment_capabilities(
            Some(vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID),
            Some(&capabilities),
            &bridged_media_kinds,
        );
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

        assert!(image.enabled && !image.native && image.bridged);
        assert!(video.enabled && !video.native && video.bridged);
        assert!(audio.enabled && !audio.native && audio.bridged);
    }

    /// Verifies a model declaration cannot bypass a missing protocol media projector in composer gates.
    /// 验证模型声明无法在输入区门控中绕过缺失的协议媒体投影器。
    #[test]
    fn protocol_restriction_uses_understanding_bridge_in_composer() {
        // capabilities declares video even though the standard Anthropic profile cannot project it.
        // capabilities 声明视频，尽管标准 Anthropic Profile 无法投影该模态。
        let capabilities = vulcan_ai_contract::ModelCapabilities {
            input_resources: vec![vulcan_ai_contract::ResourceKind::Video],
            ..vulcan_ai_contract::ModelCapabilities::default()
        };
        // bridgedMediaKinds proves the fixed video tool is executable for this session.
        // bridgedMediaKinds 证明固定视频工具在当前会话可执行。
        let bridged_media_kinds = BTreeSet::from([vulcan_ai_contract::ResourceKind::Video]);

        let summaries = composer_attachment_capabilities(
            Some(vulcan_llm_core::ANTHROPIC_MESSAGES_PROFILE_ID),
            Some(&capabilities),
            &bridged_media_kinds,
        );
        let video = summaries
            .iter()
            .find(|summary| summary.kind == "video")
            .expect("video attachment summary");

        assert!(video.enabled);
        assert!(!video.native);
        assert!(video.bridged);
    }
}
