//! host runtime adapters for shared context-injection state and hook helpers.
//! 共享上下文注入状态与 hook helper 的宿主运行时适配器。

use vulcan_context_injection::{
    ContextInjectionState, InjectionHelper, InjectionInsert, InjectionKind, InjectionPanelCard,
    InjectionRenderOptions, InjectionRendererRegistry,
};
use vulcan_core::types::{
    ProviderContextAnchor as CoreProviderContextAnchor,
    ProviderContextInjection as CoreProviderContextInjection,
    ProviderContextMutationKind as CoreProviderContextMutationKind,
};
use vulcan_vmm::{VmmRuntimeFeaturePolicy, VmmSessionMemoryState};

/// Marker prefix used by the built-in manual test hook inside user prompts.
/// 用户提示词中内置手动测试 hook 使用的标记前缀。
const MANUAL_TEST_INJECTION_PREFIX: &str = "@inject ";
/// Converts shared context-injection state into the core overlay for one logical turn.
/// 将共享上下文注入状态转换为某个逻辑回合的 core 覆盖层。
pub fn core_provider_context_injection_from_state_for_turn(
    state: &ContextInjectionState,
    vmm_state: &VmmSessionMemoryState,
    current_turn_marker: Option<&str>,
) -> Option<CoreProviderContextInjection> {
    let registry = context_injection_renderer_registry();
    let options = InjectionRenderOptions {
        registry: &registry,
        current_turn_marker,
    };
    let view = state.resolve_view_with_options(&options);
    // feature_policy independently gates state consumption and provider anchor rendering.
    // feature_policy 分别控制状态消费与 Provider 锚点渲染。
    let feature_policy = VmmRuntimeFeaturePolicy::from_env();
    let anchored_prefixes = if feature_policy.session_state_enabled
        && feature_policy.stable_anchors_enabled
        && vmm_state.session_memory_status.allows_runtime_memory()
        && vmm_state.stable_anchor_rendering_enabled
    {
        vmm_state
            .cache_epoch
            .anchors
            .iter()
            .map(|anchor| CoreProviderContextAnchor {
                anchor_id: anchor.anchor_id.clone(),
                target_message_id: anchor.target_message_id.clone(),
                prefix: anchor.prefix.clone(),
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let mutation_kind = if !view.transient_prefix.trim().is_empty() {
        CoreProviderContextMutationKind::HistoricalRewrite
    } else if anchored_prefixes.is_empty() {
        CoreProviderContextMutationKind::None
    } else {
        CoreProviderContextMutationKind::AppendOnly
    };
    let fingerprint = provider_context_injection_fingerprint(
        &view.fingerprint,
        vmm_state.cache_epoch.epoch_id.as_str(),
        &anchored_prefixes,
    );
    // Only persistent generic layers need fingerprint comparison across provider cursors.
    // 只有持久化通用层需要跨 Provider 游标比较改写指纹。
    let rewrite_fingerprint =
        (!view.persistent_blocks.is_empty()).then(|| view.fingerprint.clone());
    // cache_pressure contains body-free aggregate capacity diagnostics for the active VMM epoch.
    // cache_pressure 包含当前 VMM 纪元不含正文的聚合容量诊断。
    let cache_pressure = if feature_policy.session_state_enabled
        && vmm_state.session_memory_status.allows_runtime_memory()
    {
        vmm_state.cache_pressure()
    } else {
        vulcan_vmm::VmmCachePressure::default()
    };
    let injection = CoreProviderContextInjection {
        persistent_blocks: view.persistent_blocks,
        transient_prefix: view.transient_prefix,
        anchored_prefixes,
        mutation_kind,
        cache_epoch_id: (!vmm_state.cache_epoch.anchors.is_empty())
            .then(|| vmm_state.cache_epoch.epoch_id.clone()),
        anchor_revision: (!vmm_state.cache_epoch.anchors.is_empty())
            .then_some(vmm_state.cache_epoch.anchor_revision),
        rewrite_fingerprint,
        active_memory_body_bytes: cache_pressure.active_body_bytes,
        active_memory_estimated_tokens: cache_pressure.active_body_estimated_tokens,
        reclaimable_anchor_bytes: cache_pressure.reclaimable_anchor_bytes,
        reclaimable_anchor_estimated_tokens: cache_pressure.reclaimable_anchor_estimated_tokens,
        fingerprint,
    };
    (!injection.is_empty()).then_some(injection)
}

/// Builds a stable diagnostic fingerprint from generic state and immutable VMM anchor identities.
/// 根据通用状态与不可变 VMM 锚点身份构建稳定诊断指纹。
///
/// # Parameters
/// 参数
/// `generic_fingerprint` is the shared context-injection view fingerprint.
/// `generic_fingerprint` 是共享上下文注入视图指纹。
/// `cache_epoch_id` is the active VMM cache epoch identity.
/// `cache_epoch_id` 是当前 VMM 缓存纪元身份。
/// `anchors` are the exact provider anchors included in this overlay.
/// `anchors` 是该覆盖层包含的精确 Provider 锚点。
///
/// # Returns
/// 返回值
/// A stable text fingerprint that does not change on score-only lease refreshes.
/// 返回不会因仅评分变化的租约刷新而改变的稳定文本指纹。
fn provider_context_injection_fingerprint(
    generic_fingerprint: &str,
    cache_epoch_id: &str,
    anchors: &[CoreProviderContextAnchor],
) -> String {
    if anchors.is_empty() {
        return generic_fingerprint.to_string();
    }
    let anchor_ids = anchors
        .iter()
        .map(|anchor| anchor.anchor_id.as_str())
        .collect::<Vec<_>>()
        .join(",");
    if generic_fingerprint.trim().is_empty() {
        format!("vmm:{cache_epoch_id}:{anchor_ids}")
    } else {
        format!("{generic_fingerprint}|vmm:{cache_epoch_id}:{anchor_ids}")
    }
}

/// Returns simplified side-panel cards filtered by injection kind when requested.
/// 按需根据注入类型过滤并返回简洁侧面板卡片。
pub fn context_injection_panel_cards_for_kind(
    state: &ContextInjectionState,
    kind: Option<InjectionKind>,
) -> Vec<InjectionPanelCard> {
    let registry = context_injection_renderer_registry();
    let mut state = state.clone();
    if let Some(kind) = kind {
        state.groups.retain(|_, group| group.kind == kind);
    }
    state.panel_cards_with_registry(&registry)
}

/// Builds the renderer registry used by host runtime context-injection views.
/// 构建宿主运行时上下文注入视图使用的渲染器注册表。
fn context_injection_renderer_registry() -> InjectionRendererRegistry {
    InjectionRendererRegistry::default().with_renderer(
        vulcan_vmm::adapter::VMM_PRECHECK_MEMORY_RENDERER_ID,
        vulcan_vmm::adapter::vmm_precheck_memory_renderer(),
    )
}

/// Runs the built-in manual test hook and returns collected injections plus the cleaned user prompt.
/// 运行内置手动测试 hook，并返回已收集注入以及清理后的用户提示词。
pub fn collect_builtin_manual_injection_ops_and_sanitized_prompt(
    prompt: &str,
) -> (Vec<InjectionInsert>, String) {
    let mut helper = InjectionHelper::new();
    let mut sanitized_lines = Vec::new();
    for line in prompt.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(MANUAL_TEST_INJECTION_PREFIX) else {
            sanitized_lines.push(line);
            continue;
        };
        let Some((ttl_text, content)) = rest.split_once(' ') else {
            sanitized_lines.push(line);
            continue;
        };
        let Ok(ttl_turns) = ttl_text.trim().parse::<u32>() else {
            sanitized_lines.push(line);
            continue;
        };
        let content = content.trim();
        if content.is_empty() || ttl_turns == 0 {
            sanitized_lines.push(line);
            continue;
        }
        helper.inject(
            InjectionKind::Transient,
            "manual_inject",
            "手动测试注入内容，仅用于验证当前会话的上下文注入链路。",
            content,
            Some(ttl_turns),
        );
    }
    (
        helper.finish(),
        sanitized_lines.join("\n").trim().to_string(),
    )
}

#[cfg(test)]
mod tests {
    //! Tests for host context-injection runtime adapters.
    //! 宿主上下文注入运行时适配器测试。

    use super::*;
    use vulcan_vmm::{ContextItem, VmmRecallBatch};

    /// Builds one persistent injection operation for adapter tests.
    /// 为适配器测试构建一条持久注入操作。
    fn persistent_insert(tag: &str, content: &str) -> InjectionInsert {
        InjectionInsert::single(InjectionKind::Persistent, tag, "test", content, None)
    }

    /// Verifies panel cards retain their explicit persistent and transient kind filtering.
    /// 验证面板卡片保留明确的持久与临时类型过滤。
    #[test]
    fn panel_cards_filter_by_injection_kind() {
        let mut state = ContextInjectionState::default();
        state.apply_inserts(
            vec![
                persistent_insert("persistent_memory", "stable memory"),
                InjectionInsert::single(
                    InjectionKind::Transient,
                    "vmm_inject",
                    "memory",
                    "ttl memory",
                    Some(2),
                ),
            ],
            "1",
        );

        let dynamic =
            context_injection_panel_cards_for_kind(&state, Some(InjectionKind::Transient));
        let static_cards =
            context_injection_panel_cards_for_kind(&state, Some(InjectionKind::Persistent));

        assert_eq!(dynamic.len(), 1);
        assert_eq!(dynamic[0].tag, "vmm_inject");
        assert_eq!(static_cards.len(), 1);
        assert_eq!(static_cards[0].tag, "persistent_memory");
    }

    /// Verifies cancelled injection groups no longer render into provider overlays.
    /// 验证已取消的注入分组不会继续渲染进 Provider 覆盖层。
    #[test]
    fn provider_overlay_excludes_cancelled_injection_groups() {
        let mut state = ContextInjectionState::default();
        state.apply_inserts(
            vec![
                persistent_insert("persistent_memory", "stable memory"),
                InjectionInsert::single(
                    InjectionKind::Transient,
                    "turn_memory",
                    "memory",
                    "dynamic memory",
                    Some(2),
                ),
            ],
            "1",
        );

        assert!(state.remove_group(Some(InjectionKind::Persistent), "persistent_memory"));
        assert!(state.remove_group(Some(InjectionKind::Transient), "turn_memory"));

        let overlay = core_provider_context_injection_from_state_for_turn(
            &state,
            &VmmSessionMemoryState::default(),
            Some("turn-a"),
        );

        assert!(overlay.is_none());
    }

    /// Verifies a disabled stable-anchor epoch never leaks retained VMM bodies into provider overlays.
    /// 验证禁用稳定锚点的纪元绝不会把保留的 VMM 正文泄漏进 Provider 覆盖层。
    #[test]
    fn provider_overlay_omits_vmm_anchors_when_epoch_rendering_is_disabled() {
        let mut vmm_state = VmmSessionMemoryState::default();
        vmm_state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &VmmRecallBatch {
                    ttl_turns: 3,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![ContextItem {
                        text: String::from("retained-but-not-rendered"),
                        memory_id: 7,
                        ..ContextItem::default()
                    }],
                },
                "1",
            )
            .expect("initial recall should succeed");
        vmm_state
            .synchronize_stable_anchor_rendering(false, "2")
            .expect("anchor rendering should disable through an epoch reset");

        let overlay = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &vmm_state,
            Some("turn-a"),
        );

        assert!(overlay.is_none());
        assert!(vmm_state.active_leases.contains_key("memory:7"));
    }

    /// Verifies TTL expiry changes diagnostics but never changes provider anchors or the cache fingerprint.
    /// 验证 TTL 到期只改变诊断指标，绝不会改变 Provider 锚点或缓存指纹。
    #[test]
    fn provider_overlay_remains_cache_stable_when_vmm_lease_expires() {
        let mut vmm_state = VmmSessionMemoryState::default();
        vmm_state
            .apply_recall_batch(
                "session-a",
                "user-1",
                &VmmRecallBatch {
                    ttl_turns: 1,
                    injection_tag: String::from("VMM_MEMORY"),
                    injection_description: String::from("Relevant memories."),
                    items: vec![ContextItem {
                        text: String::from("cache-stable-memory"),
                        memory_id: 7,
                        ..ContextItem::default()
                    }],
                },
                "1",
            )
            .expect("initial recall should succeed");
        let overlay_before_expiry = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &vmm_state,
            Some("turn-1"),
        )
        .expect("active VMM anchor should render");
        vmm_state
            .register_active_turn("turn-1", "1")
            .expect("logical turn should register");
        vmm_state
            .consume_turn("turn-1", "2")
            .expect("terminal turn should retain the expired resident");

        let overlay_after_expiry = core_provider_context_injection_from_state_for_turn(
            &ContextInjectionState::default(),
            &vmm_state,
            Some("turn-2"),
        )
        .expect("expired resident anchor should remain in the cache epoch");

        assert_eq!(
            overlay_after_expiry.anchored_prefixes,
            overlay_before_expiry.anchored_prefixes
        );
        assert_eq!(
            overlay_after_expiry.fingerprint,
            overlay_before_expiry.fingerprint
        );
        assert_eq!(
            overlay_after_expiry.cache_epoch_id,
            overlay_before_expiry.cache_epoch_id
        );
        assert_eq!(
            overlay_after_expiry.anchor_revision,
            overlay_before_expiry.anchor_revision
        );
        assert_eq!(
            overlay_after_expiry.mutation_kind,
            overlay_before_expiry.mutation_kind
        );
        assert!(overlay_before_expiry.active_memory_body_bytes > 0);
        assert_eq!(overlay_after_expiry.active_memory_body_bytes, 0);
        assert!(overlay_after_expiry.reclaimable_anchor_bytes > 0);
    }
}
