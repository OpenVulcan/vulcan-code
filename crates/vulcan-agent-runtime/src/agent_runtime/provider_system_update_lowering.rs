//! Provider-neutral lowering for chronological runtime system updates.
//! 面向时间顺序 runtime system update 的 Provider 中立转换逻辑。

use vulcan_context::{
    RuntimeProviderLoweringRepairEvent, RuntimeSystemUpdateEvent, RuntimeSystemUpdateLoweringHint,
    RuntimeSystemUpdateTrust,
};

/// Provider capability and policy used while lowering one runtime system update.
/// 转换单条 runtime system update 时使用的 Provider 能力与策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProviderSystemUpdateLoweringPolicy {
    /// Whether the provider supports native chronological system messages after conversation start.
    /// Provider 是否支持对话开始后的原生时间顺序 system 消息。
    pub supports_native_chronological_system: bool,
    /// Whether unsupported or untrusted updates may be represented as user-scoped wrapper text.
    /// 不支持或不受信更新是否允许表示为用户作用域包装文本。
    pub allow_user_wrapper: bool,
}

impl Default for ProviderSystemUpdateLoweringPolicy {
    /// Returns the conservative default used by providers without native chronological system support.
    /// 返回缺少原生时间顺序 system 支持的 Provider 使用的保守默认值。
    fn default() -> Self {
        Self {
            supports_native_chronological_system: false,
            allow_user_wrapper: true,
        }
    }
}

/// Result of lowering one provider-neutral runtime system update.
/// 单条 Provider 中立 runtime system update 的转换结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderSystemUpdateLowering {
    /// Native privileged system update that can be passed to provider-specific fields.
    /// 可传递到 Provider 特定字段的原生特权 system update。
    NativeSystem { text: String },
    /// User-scoped fallback text that must not be treated as privileged system content.
    /// 用户作用域回退文本，不得当作特权 system 内容处理。
    UserWrapper { text: String },
    /// Rejected update with a stable diagnostic reason.
    /// 带稳定诊断原因的被拒绝更新。
    Reject { reason: String },
}

/// Lowers one runtime system update according to provider capability, trust, and hint policy.
/// 根据 Provider 能力、信任等级与 hint 策略转换单条 runtime system update。
pub(crate) fn lower_provider_system_update(
    event: &RuntimeSystemUpdateEvent,
    policy: ProviderSystemUpdateLoweringPolicy,
) -> ProviderSystemUpdateLowering {
    let text = event.text.trim();
    if text.is_empty() {
        return ProviderSystemUpdateLowering::Reject {
            reason: String::from("empty_system_update"),
        };
    }

    let native_allowed_by_trust = matches!(event.trust, RuntimeSystemUpdateTrust::TrustedHost);
    let native_requested = matches!(
        event.lowering_hint,
        RuntimeSystemUpdateLoweringHint::Auto
            | RuntimeSystemUpdateLoweringHint::PreferNative
            | RuntimeSystemUpdateLoweringHint::RequireNative
    );
    if native_allowed_by_trust && native_requested && policy.supports_native_chronological_system {
        return ProviderSystemUpdateLowering::NativeSystem {
            text: text.to_string(),
        };
    }

    if matches!(
        event.lowering_hint,
        RuntimeSystemUpdateLoweringHint::RequireNative
    ) {
        return ProviderSystemUpdateLowering::Reject {
            reason: String::from("native_chronological_system_update_required_but_unavailable"),
        };
    }

    if !policy.allow_user_wrapper {
        return ProviderSystemUpdateLowering::Reject {
            reason: String::from("user_wrapper_system_update_fallback_disabled"),
        };
    }

    ProviderSystemUpdateLowering::UserWrapper {
        text: system_update_user_wrapper_text(event, text),
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
        ProviderSystemUpdateLowering::NativeSystem { .. } => return None,
        ProviderSystemUpdateLowering::UserWrapper { .. } => (
            "fallback",
            "system_update_lowered_to_user_scoped_wrapper".to_string(),
        ),
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

/// Builds the non-privileged user-scoped wrapper used when native system lowering is unavailable.
/// 构建原生 system 转换不可用时使用的非特权用户作用域包装文本。
fn system_update_user_wrapper_text(event: &RuntimeSystemUpdateEvent, text: &str) -> String {
    let source = event
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("runtime");
    format!(
        "[Runtime system update - user scoped]\nUpdate ID: {}\nSource: {}\n\n{}",
        event.update_id, source, text
    )
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

    /// Verifies trusted updates use native lowering when the provider supports it.
    /// 验证 Provider 支持时受信更新会使用原生转换。
    #[test]
    fn trusted_update_uses_native_lowering_when_supported() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::PreferNative);
        let lowered = lower_provider_system_update(
            &update,
            ProviderSystemUpdateLoweringPolicy {
                supports_native_chronological_system: true,
                allow_user_wrapper: true,
            },
        );

        assert!(matches!(
            lowered,
            ProviderSystemUpdateLowering::NativeSystem { text } if text.contains("compacted project rules")
        ));
    }

    /// Verifies unsupported native lowering falls back to a user-scoped wrapper by default.
    /// 验证不支持原生转换时默认降级为用户作用域包装。
    #[test]
    fn unsupported_native_lowering_uses_user_wrapper() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::Auto);
        let lowered =
            lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        let ProviderSystemUpdateLowering::UserWrapper { text } = lowered else {
            panic!("expected user wrapper fallback");
        };
        assert!(text.contains("user scoped"));
        assert!(text.contains("system-update-1"));
        assert!(text.contains("context_compaction"));
    }

    /// Verifies untrusted updates cannot be promoted to privileged native system content.
    /// 验证不受信更新不能提升为特权原生 system 内容。
    #[test]
    fn untrusted_update_never_uses_native_lowering() {
        let mut update = trusted_update(RuntimeSystemUpdateLoweringHint::PreferNative);
        update.trust = RuntimeSystemUpdateTrust::UntrustedExternal;
        let lowered = lower_provider_system_update(
            &update,
            ProviderSystemUpdateLoweringPolicy {
                supports_native_chronological_system: true,
                allow_user_wrapper: true,
            },
        );

        assert!(matches!(
            lowered,
            ProviderSystemUpdateLowering::UserWrapper { .. }
        ));
    }

    /// Verifies RequireNative rejects when native chronological system support is unavailable.
    /// 验证缺少原生时间顺序 system 支持时 RequireNative 会拒绝请求。
    #[test]
    fn require_native_rejects_without_native_support() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::RequireNative);
        let lowered =
            lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        assert!(matches!(
            lowered,
            ProviderSystemUpdateLowering::Reject { reason }
                if reason == "native_chronological_system_update_required_but_unavailable"
        ));
    }

    /// Verifies non-native system update lowering produces a runtime repair audit event.
    /// 验证非原生 system update 转换会产出运行时修复审计事件。
    #[test]
    fn user_wrapper_lowering_builds_repair_event() {
        let update = trusted_update(RuntimeSystemUpdateLoweringHint::Auto);
        let lowered =
            lower_provider_system_update(&update, ProviderSystemUpdateLoweringPolicy::default());

        let repair = provider_system_update_repair_event(
            &update,
            &lowered,
            "minimax-cn-coding-plan",
            "MiniMax-M3",
            "anthropic-messages",
        )
        .expect("user-wrapper fallback should emit a repair audit event");

        assert_eq!(repair.turn_id.as_deref(), Some("turn-1"));
        assert_eq!(repair.action, "fallback");
        assert_eq!(
            repair.reason,
            "system_update_lowered_to_user_scoped_wrapper"
        );
        assert_eq!(repair.provider_id, "minimax-cn-coding-plan");
        assert_eq!(repair.affected_items, 1);
    }
}
