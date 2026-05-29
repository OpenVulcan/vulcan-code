//! GUI runtime adapters for shared context-injection state and hook helpers.
//! 共享上下文注入状态与 hook helper 的 GUI 运行时适配器。

use vulcan_context_injection::{
    ContextInjectionState, InjectionHelper, InjectionInsert, InjectionKind, InjectionPanelCard,
};
use vulcan_core::types::ProviderContextInjection as CoreProviderContextInjection;

/// Marker prefix used by the built-in manual test hook inside user prompts.
/// 用户提示词中内置手动测试 hook 使用的标记前缀。
const MANUAL_TEST_INJECTION_PREFIX: &str = "@inject ";

/// Converts shared context-injection state into the core provider-facing overlay.
/// 将共享上下文注入状态转换为 core 的 Provider 可见覆盖层。
pub(crate) fn core_provider_context_injection_from_state(
    state: &ContextInjectionState,
) -> Option<CoreProviderContextInjection> {
    let view = state.resolve_view();
    let injection = CoreProviderContextInjection {
        persistent_blocks: view.persistent_blocks,
        transient_prefix: view.transient_prefix,
        fingerprint: view.fingerprint,
    };
    (!injection.is_empty()).then_some(injection)
}

/// Returns simplified side-panel cards for currently active context injections.
/// 返回当前活动上下文注入的简洁侧面板卡片。
pub(crate) fn context_injection_panel_cards(
    state: &ContextInjectionState,
) -> Vec<InjectionPanelCard> {
    state.panel_cards()
}

/// Runs the built-in manual test hook and returns collected injections plus the cleaned user prompt.
/// 运行内置手动测试 hook，并返回已收集注入以及清理后的用户提示词。
pub(crate) fn collect_builtin_manual_injection_ops_and_sanitized_prompt(
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
