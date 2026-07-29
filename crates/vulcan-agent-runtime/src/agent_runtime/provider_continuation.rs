//! Provider-neutral continuation scoping.
//! 供应商无关的延续状态限定。

use vulcan_core::types::ProviderContinuation;

/// Scopes one persisted continuation to the exact active provider, protocol profile, and model.
/// 将一条持久化延续状态限定到精确的当前供应商、协议配置与模型。
///
/// # Parameters
/// 参数
/// `continuation` is the saved cursor, while the three route identifiers freeze the immutable
/// execution route.
/// `continuation` 是已保存游标，三个路由标识共同冻结不可变执行路由。
///
/// # Returns
/// 返回值
/// Returns the validated cursor or an empty cursor when its ownership does not match.
/// 返回已校验游标；归属不匹配时返回空游标。
pub(crate) fn scope_continuation_to_active_provider(
    continuation: &ProviderContinuation,
    provider_id: &str,
    protocol_profile_id: &str,
    model_id: &str,
) -> ProviderContinuation {
    if continuation.provider_id.as_deref() != Some(provider_id)
        || continuation.protocol_profile_id.as_deref() != Some(protocol_profile_id)
        || continuation.model_id.as_deref() != Some(model_id)
    {
        return ProviderContinuation::default();
    }
    ProviderContinuation {
        provider_id: Some(provider_id.to_string()),
        protocol_profile_id: Some(protocol_profile_id.to_string()),
        model_id: Some(model_id.to_string()),
        last_provider_message_id: continuation
            .last_provider_message_id
            .clone()
            .filter(|value| !value.trim().is_empty()),
        last_local_message_id: continuation
            .last_local_message_id
            .clone()
            .filter(|value| !value.trim().is_empty()),
        context_cache_epoch_id: continuation.context_cache_epoch_id.clone(),
        context_anchor_revision: continuation.context_anchor_revision,
        context_rewrite_fingerprint: continuation.context_rewrite_fingerprint.clone(),
        updated_at: continuation.updated_at.clone(),
    }
}
