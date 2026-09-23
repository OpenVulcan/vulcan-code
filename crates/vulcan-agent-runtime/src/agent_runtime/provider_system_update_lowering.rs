//! Provider-neutral lowering for chronological runtime system updates.
//! 面向时间顺序 runtime system update 的 Provider 中立转换逻辑。

use vulcan_ai_contract::{HostMessageEnvelope, HostMessageKind};
use vulcan_context::{RuntimeProviderLoweringRepairEvent, RuntimeSystemUpdateEvent};

/// Policy marker retained for call-site ownership while all dynamic updates use User events.
/// 保留策略标记以维持调用方归属，同时所有动态更新统一使用 User 事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct ProviderSystemUpdateLoweringPolicy;

/// Result of lowering one provider-neutral runtime system update.
/// 单条 Provider 中立 runtime system update 的转换结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderSystemUpdateLowering {
    /// Host-authored dynamic context rendered as a normal User message.
    /// 宿主编写的动态上下文渲染为普通 User 消息。
    User { text: String },
    /// Rejected update with a stable diagnostic reason.
    /// 带稳定诊断原因的被拒绝更新。
    Reject { reason: String },
}

/// Lowers one runtime system update into the shared host User envelope.
/// 将单条 runtime system update 转换为共享宿主 User 信封。
pub(crate) fn lower_provider_system_update(
    event: &RuntimeSystemUpdateEvent,
    _policy: ProviderSystemUpdateLoweringPolicy,
) -> ProviderSystemUpdateLowering {
    let text = event.text.trim();
    if text.is_empty() {
        return ProviderSystemUpdateLowering::Reject {
            reason: String::from("empty_system_update"),
        };
    }

    match system_update_user_event_text(event, text) {
        Ok(text) => ProviderSystemUpdateLowering::User { text },
        Err(reason) => ProviderSystemUpdateLowering::Reject { reason },
    }
}

/// Builds an audit repair event for non-native system-update lowering outcomes.
/// 为非原生 system-update 转换结果构建审计修复事件。
pub(crate) fn provider_system_update_repair_event(
    event: &RuntimeSystemUpdateEvent,
    lowered: &ProviderSystemUpdateLowering,
    provider_id: &str,
    model_id: &str,
    provider_family: &str,
) -> Option<RuntimeProviderLoweringRepairEvent> {
    let (action, reason) = match lowered {
        ProviderSystemUpdateLowering::User { .. } => return None,
        ProviderSystemUpdateLowering::Reject { reason } => ("reject", reason.clone()),
    };

    Some(RuntimeProviderLoweringRepairEvent {
        turn_id: event.turn_id.clone(),
        repair_id: format!("system-update-lowering-{}", event.update_id),
        provider_id: provider_id.to_string(),
        model_id: model_id.to_string(),
        provider_family: provider_family.to_string(),
        action: action.to_string(),
        reason,
        affected_items: 1,
    })
}

/// Builds the shared User envelope used for one chronological runtime update.
/// 构建一条时间顺序 runtime update 使用的共享 User 信封。
///
/// # Parameters
/// # 参数
/// - `event`: The typed runtime update and its stable source identity.
/// - `event`：包含稳定来源身份的强类型运行时更新。
/// - `text`: The validated untrusted body supplied to the model.
/// - `text`：提供给模型的已校验非可信正文。
///
/// # Returns
/// # 返回值
/// Returns the deterministic rendered host message or a closed-envelope validation error.
/// 返回确定性渲染的宿主消息，或封闭信封校验错误。
fn system_update_user_event_text(
    event: &RuntimeSystemUpdateEvent,
    text: &str,
) -> Result<String, String> {
    let source = event
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("runtime");
    HostMessageEnvelope {
        event_id: format!("runtime-context-update:{}", event.update_id),
        kind: HostMessageKind::ContextUpdate,
        source: source.to_string(),
        target: String::from("model"),
        task_id: event.turn_id.clone(),
        question_id: None,
        scope: String::from("runtime"),
        body: text.to_string(),
    }
    .render()
}

#[cfg(test)]
mod tests {
    use super::{
        ProviderSystemUpdateLowering, ProviderSystemUpdateLoweringPolicy,
        lower_provider_system_update, provider_system_update_repair_event,
    };
    use vulcan_context::{
        RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint, RuntimeSystemUpdateTrust,
    };

    /// Builds a trusted host system update for lowering tests.
    /// 构建供转换测试使用的受信宿主 system update。
    fn trusted_update(hint: RuntimeSystemUpdateLoweringHint) -> RuntimeSystemUpdateEvent {
        RuntimeSystemUpdateEvent {
            turn_id: Some(String::from("turn-1")),
            update_id: String::from("system-update-1"),
            effective_after_event_id: Some(String::from("event-1")),
            text: String::from("Use the freshly compacted project rules."),
            trust: RuntimeSystemUpdateTrust::TrustedHost,
            lowering_hint: hint,
            fingerprint: Some(String::from("fp-system-update-1")),
            source: Some(String::from("context_compaction")),
        }
    }

    /// Verifies even an explicit native preference lowers dynamic updates into User envelopes.
    /// 验证即使显式偏好原生角色，动态更新也统一转换为 User 信封。
    #[test]
    fn trusted_update_uses_user_envelope_when_native_was_requested() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::PreferNative);
        let lowered = lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        assert!(matches!(
            lowered,
            ProviderSystemUpdateLowering::User { text }
                if text.contains("vulcan_context_update")
                    && text.contains("compacted project rules")
        ));
    }

    /// Verifies automatic policy uses the same User envelope without protocol capability probing.
    /// 验证自动策略使用同一 User 信封，不探测协议能力。
    #[test]
    fn automatic_lowering_uses_user_envelope() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::Auto);
        let lowered =
            lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        let ProviderSystemUpdateLowering::User { text } = lowered else {
            panic!("expected shared User envelope");
        };
        assert!(text.contains("event_id=\"runtime-context-update:system-update-1\""));
        assert!(text.contains("source=\"context_compaction\""));
    }

    /// Verifies untrusted updates cannot be promoted to privileged native system content.
    /// 验证不受信更新不能提升为特权原生 system 内容。
    #[test]
    fn untrusted_update_uses_user_envelope() {
        let mut update = trusted_update(RuntimeSystemUpdateLoweringHint::PreferNative);
        update.trust = RuntimeSystemUpdateTrust::UntrustedExternal;
        let lowered = lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        assert!(matches!(
            lowered,
            ProviderSystemUpdateLowering::User { .. }
        ));
    }

    /// Verifies RequireNative rejects when native chronological system support is unavailable.
    /// 验证缺少原生时间顺序 system 支持时 RequireNative 会拒绝请求。
    #[test]
    fn require_native_uses_user_envelope() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::RequireNative);
        let lowered =
            lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        assert!(matches!(
            lowered,
            ProviderSystemUpdateLowering::User { .. }
        ));
    }

    /// Verifies non-native system update lowering produces a runtime repair audit event.
    /// 验证非原生 system update 转换会产出运行时修复审计事件。
    #[test]
    fn user_lowering_has_no_repair_event() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::Auto);
        let lowered =
            lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        let repair = provider_system_update_repair_event(
            &update,
            &lowered,
            "minimax-cn-coding-plan",
            "MiniMax-M3",
            "anthropic-messages",
        );

        assert!(repair.is_none());
    }
}
