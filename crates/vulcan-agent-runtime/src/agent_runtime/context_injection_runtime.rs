//! GUI runtime adapters for shared context-injection state and hook helpers.
//! 共享上下文注入状态与 hook helper 的 GUI 运行时适配器。

use vulcan_context_injection::{
    ContextInjectionState, InjectionHelper, InjectionInsert, InjectionKind, InjectionPanelCard,
    InjectionRenderOptions, InjectionRendererRegistry,
};
use vulcan_core::types::ProviderContextInjection as CoreProviderContextInjection;

/// Marker prefix used by the built-in manual test hook inside user prompts.
/// 用户提示词中内置手动测试 hook 使用的标记前缀。
const MANUAL_TEST_INJECTION_PREFIX: &str = "@inject ";
/// Legacy VMM profile tag that used to live in hidden-user injection state.
/// 旧版曾写入隐藏用户注入态的 VMM 画像标签。
const LEGACY_VMM_PROFILE_CONTEXT_INJECTION_TAG: &str = "vmm_profile";

/// Converts shared context-injection state into the core overlay for one logical turn.
/// 将共享上下文注入状态转换为某个逻辑回合的 core 覆盖层。
pub fn core_provider_context_injection_from_state_for_turn(
    state: &ContextInjectionState,
    current_turn_marker: Option<&str>,
) -> Option<CoreProviderContextInjection> {
    let registry = context_injection_renderer_registry();
    let options = InjectionRenderOptions {
        registry: &registry,
        current_turn_marker,
    };
    let state = state_without_legacy_vmm_profile_injection(state);
    let view = state.resolve_view_with_options(&options);
    let injection = CoreProviderContextInjection {
        persistent_blocks: view.persistent_blocks,
        transient_prefix: view.transient_prefix,
        fingerprint: view.fingerprint,
    };
    (!injection.is_empty()).then_some(injection)
}

/// Returns simplified side-panel cards filtered by injection kind when requested.
/// 按需根据注入类型过滤并返回简洁侧面板卡片。
pub fn context_injection_panel_cards_for_kind(
    state: &ContextInjectionState,
    kind: Option<InjectionKind>,
) -> Vec<InjectionPanelCard> {
    let registry = context_injection_renderer_registry();
    let mut state = state_without_legacy_vmm_profile_injection(state);
    if let Some(kind) = kind {
        state.groups.retain(|_, group| group.kind == kind);
    }
    state.panel_cards_with_registry(&registry)
}

/// Removes legacy VMM profile hidden-user state and reports whether anything changed.
/// 删除旧版 VMM 画像隐藏用户状态，并返回是否发生变化。
pub fn remove_legacy_vmm_profile_context_injection_state(
    state: &mut ContextInjectionState,
) -> bool {
    state.remove_group(
        Some(InjectionKind::Persistent),
        LEGACY_VMM_PROFILE_CONTEXT_INJECTION_TAG,
    )
}

/// Clones one state and removes hidden-user VMM profile leftovers before rendering.
/// 克隆一份状态并在渲染前移除隐藏用户 VMM 画像残留。
fn state_without_legacy_vmm_profile_injection(
    state: &ContextInjectionState,
) -> ContextInjectionState {
    let mut state = state.clone();
    remove_legacy_vmm_profile_context_injection_state(&mut state);
    state
}

/// Builds the renderer registry used by GUI runtime context-injection views.
/// 构建 GUI 运行时上下文注入视图使用的渲染器注册表。
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
    //! Tests for GUI context-injection runtime adapters.
    //! GUI 上下文注入运行时适配器测试。

    use super::*;

    /// Builds one persistent injection operation for adapter tests.
    /// 为适配器测试构建一条持久注入操作。
    fn persistent_insert(tag: &str, content: &str) -> InjectionInsert {
        InjectionInsert::single(InjectionKind::Persistent, tag, "test", content, None)
    }

    #[test]
    fn provider_overlay_filters_legacy_vmm_profile_hidden_user_state() {
        let mut state = ContextInjectionState::default();
        state.apply_inserts(
            vec![
                persistent_insert("vmm_profile", "base profile"),
                persistent_insert("vmm_profile_delta", "delta profile"),
            ],
            "1",
        );

        let overlay = core_provider_context_injection_from_state_for_turn(&state, Some("turn-a"))
            .expect("overlay");

        assert!(
            !overlay
                .persistent_blocks
                .iter()
                .any(|block| block.contains("base profile"))
        );
        assert!(
            overlay
                .persistent_blocks
                .iter()
                .any(|block| block.contains("delta profile"))
        );
    }

    #[test]
    fn panel_cards_filter_by_kind_and_remove_legacy_profile_tag() {
        let mut state = ContextInjectionState::default();
        state.apply_inserts(
            vec![
                persistent_insert("vmm_profile", "base profile"),
                persistent_insert("vmm_profile_delta", "delta profile"),
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
        assert_eq!(static_cards[0].tag, "vmm_profile_delta");
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

        let overlay = core_provider_context_injection_from_state_for_turn(&state, Some("turn-a"));

        assert!(overlay.is_none());
    }
}
