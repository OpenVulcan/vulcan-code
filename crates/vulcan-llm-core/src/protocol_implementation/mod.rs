//! Provider-independent wire protocol profiles.
//! 供应商无关的 Wire 协议 Profile。

use std::collections::BTreeSet;
use vulcan_ai_contract::{AiExecutionRequest, ResourceKind, ToolInputFormat};

pub mod anthropic;
pub mod cohere;
pub mod gemini;
pub mod interactions;
mod media_capabilities;
pub mod openai;
pub(crate) mod openai_stream;
pub mod registry;
pub mod shared;

pub use anthropic::*;
pub use cohere::*;
pub use gemini::*;
pub use interactions::*;
pub use media_capabilities::protocol_profile_supports_media_source;
pub use openai::*;
pub use registry::*;
pub use shared::*;

/// Stable provider family for Anthropic-native continuation signatures.
/// Anthropic 原生延续签名的稳定供应商族。
pub(crate) const SIGNATURE_PROVIDER_ANTHROPIC: &str = "anthropic";

/// Stable provider family for OpenAI-native continuation signatures.
/// OpenAI 原生延续签名的稳定供应商族。
pub(crate) const SIGNATURE_PROVIDER_OPENAI: &str = "openai";

/// Stable provider family for Gemini-native continuation signatures.
/// Gemini 原生延续签名的稳定供应商族。
pub(crate) const SIGNATURE_PROVIDER_GEMINI: &str = "gemini";

/// Wraps one provider-native signature with explicit provenance for safe replay.
/// 使用显式来源封装一个供应商原生签名，以便安全回放。
pub(crate) fn wrap_provider_signature(provider: &str, signature: &str) -> Option<String> {
    vulcan_ai_contract::wrap_provider_continuation(provider, signature)
}

/// Unwraps a signature only when its explicit provenance matches the target provider.
/// 仅在显式来源与目标供应商匹配时解封签名。
pub(crate) fn compatible_provider_signature<'a>(
    provider: &str,
    signature: &'a str,
) -> Option<&'a str> {
    vulcan_ai_contract::provider_continuation_payload(provider, signature)
}

/// Rejects freeform tool input on protocols that only implement JSON-schema functions.
/// 在只实现 JSON Schema 函数的协议上拒绝自由文本工具输入。
///
/// # Parameters
/// 参数
/// `request` owns the exact tool definitions and `protocol_id` identifies the rejecting profile.
/// `request` 持有精确工具定义，`protocol_id` 标识执行拒绝的 Profile。
///
/// # Returns
/// 返回值
/// Success when every tool is JSON-schema based, otherwise a deterministic conversion failure.
/// 全部工具都基于 JSON Schema 时成功，否则返回确定性转换失败。
pub(crate) fn require_json_schema_tool_inputs(
    request: &AiExecutionRequest,
    protocol_id: &str,
) -> Result<(), ProtocolConversionError> {
    if let Some(tool) = request
        .tools
        .iter()
        .find(|tool| !matches!(tool.input_format, ToolInputFormat::JsonSchema))
    {
        return Err(ProtocolConversionError::new(format!(
            "Protocol profile `{protocol_id}` has no registered freeform input projector for tool `{}`.",
            tool.name
        )));
    }
    Ok(())
}

/// Returns whether one exact protocol profile has a registered local-media projector for a MIME type.
/// 返回一个精确协议 Profile 是否为指定 MIME 类型注册了本地媒体投影器。
///
/// # Parameters
/// 参数
/// `protocol_profile_id` is the frozen protocol identity, `kind` is the provider-neutral media
/// category, and `media_type` is the normalized host MIME type.
/// `protocol_profile_id` 是冻结协议标识，`kind` 是供应商无关媒体类别，`media_type` 是规范化宿主 MIME 类型。
///
/// # Returns
/// 返回值
/// `true` only when the current encoder can represent the local media without protocol probing.
/// 仅当当前编码器无需协议探测即可表示该本地媒体时返回 `true`。
pub fn protocol_profile_supports_local_media_type(
    protocol_profile_id: &str,
    kind: ResourceKind,
    media_type: &str,
) -> bool {
    protocol_profile_supports_media_source(
        protocol_profile_id,
        kind,
        media_type,
        vulcan_ai_contract::ResourceSourceKind::InlineData,
    )
}

/// Rejects route-bound provider context before encoding through a different wire protocol.
/// 在通过不同 Wire 协议编码前拒绝绑定路由的供应商上下文。
pub(crate) fn require_no_route_bound_provider_context(
    request: &AiExecutionRequest,
    protocol_id: &str,
) -> Result<(), ProtocolConversionError> {
    if request
        .messages
        .iter()
        .any(|message| !message.provider_context_items.is_empty())
    {
        return Err(ProtocolConversionError::new(format!(
            "Protocol profile `{protocol_id}` cannot replay route-bound provider context items."
        )));
    }
    Ok(())
}

/// Reconstructs one streamed tool name only along prefixes in the frozen tool set.
/// 仅沿冻结工具集合中的前缀重建一个流式工具名称。
///
/// # Parameters
/// 参数
/// `current` is the name assembled so far, `fragment` is the new provider
/// fragment, and `registered_tools` is the exact request-scoped tool set.
/// `current` 是目前已组装的名称，`fragment` 是新的供应商片段，
/// `registered_tools` 是请求范围内的精确工具集合。
///
/// # Returns
/// 返回值
/// The only candidate supported by the observed fragments and registered names.
/// 返回观测片段与已注册名称共同支持的唯一候选值。
pub(crate) fn merge_streamed_tool_name(
    current: &str,
    fragment: &str,
    registered_tools: &BTreeSet<String>,
) -> String {
    if registered_tools.contains(fragment) {
        return fragment.to_string();
    }
    if registered_tools.contains(current) {
        return current.to_string();
    }
    let combined = format!("{current}{fragment}");
    if registered_tools
        .iter()
        .any(|registered| registered.starts_with(combined.as_str()))
    {
        return combined;
    }
    if registered_tools
        .iter()
        .any(|registered| registered.starts_with(fragment))
    {
        return fragment.to_string();
    }
    combined
}

#[cfg(test)]
mod signature_provenance_tests {
    use super::*;

    /// Verifies signature replay requires exact explicit provider provenance.
    /// 验证签名回放必须具备完全匹配的显式供应商来源。
    #[test]
    fn signature_provenance_is_exact_and_non_legacy() {
        let wrapped = wrap_provider_signature(SIGNATURE_PROVIDER_ANTHROPIC, "payload")
            .expect("native signature should wrap");
        assert_eq!(wrapped, "anthropic#payload");
        assert_eq!(
            compatible_provider_signature(SIGNATURE_PROVIDER_ANTHROPIC, wrapped.as_str()),
            Some("payload")
        );
        assert_eq!(
            compatible_provider_signature(SIGNATURE_PROVIDER_OPENAI, wrapped.as_str()),
            None
        );
        assert_eq!(
            compatible_provider_signature(SIGNATURE_PROVIDER_ANTHROPIC, "payload"),
            None
        );
    }

    /// Verifies protocol media admission mirrors the exact format-sensitive encoders.
    /// 验证协议媒体准入与精确的格式敏感编码器保持一致。
    #[test]
    fn local_media_type_support_matches_registered_encoders() {
        assert!(!protocol_profile_supports_local_media_type(
            OPENAI_CHAT_PROFILE_ID,
            ResourceKind::Audio,
            "audio/opus"
        ));
        assert!(protocol_profile_supports_local_media_type(
            OPENAI_CHAT_PROFILE_ID,
            ResourceKind::Audio,
            "audio/flac"
        ));
        assert!(protocol_profile_supports_local_media_type(
            GEMINI_GENERATE_CONTENT_PROFILE_ID,
            ResourceKind::Audio,
            "audio/opus"
        ));
        assert!(!protocol_profile_supports_local_media_type(
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            ResourceKind::Video,
            "video/webm"
        ));
        assert!(protocol_profile_supports_local_media_type(
            ANTHROPIC_MESSAGES_VIDEO_PROFILE_ID,
            ResourceKind::Video,
            "video/x-matroska"
        ));
    }
}
