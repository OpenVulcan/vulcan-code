//! Provider-independent wire protocol profiles.
//! 供应商无关的 Wire 协议 Profile。

use std::collections::BTreeSet;

pub mod anthropic;
pub mod cohere;
pub mod gemini;
pub mod interactions;
pub mod openai;
mod openai_stream;
pub mod registry;
pub mod shared;

pub use anthropic::*;
pub use cohere::*;
pub use gemini::*;
pub use interactions::*;
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
    let signature = signature.trim();
    if signature.is_empty() || signature.contains('#') {
        return None;
    }
    Some(format!("{provider}#{signature}"))
}

/// Unwraps a signature only when its explicit provenance matches the target provider.
/// 仅在显式来源与目标供应商匹配时解封签名。
pub(crate) fn compatible_provider_signature<'a>(
    provider: &str,
    signature: &'a str,
) -> Option<&'a str> {
    let (source, payload) = signature.split_once('#')?;
    let payload = payload.trim();
    (source == provider && !payload.is_empty() && !payload.contains('#')).then_some(payload)
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
}
