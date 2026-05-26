//! Provider prompt-layer rendering helpers shared by transport adapters.
//! Provider 传输适配器共享的提示词层渲染辅助模块。

use vulcan_core::types::ProviderRequestSnapshot as CoreProviderRequestSnapshot;

/// Render the provider-visible fallback system prompt from structured layers.
/// 从结构化层渲染 Provider 可见的回退系统提示词。
pub(crate) fn provider_fallback_system_prompt(snapshot: &CoreProviderRequestSnapshot) -> String {
    snapshot
        .prompt_layers
        .render_or_legacy(snapshot.system_prompt.as_str())
}
