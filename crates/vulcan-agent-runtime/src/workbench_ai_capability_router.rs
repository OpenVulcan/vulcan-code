//! Shared workbench AI capability routing helpers.
//! 共享工作台 AI 能力路由辅助函数。

use std::collections::{BTreeMap, BTreeSet};

use vulcan_provider_config::config::{
    AiToolKind, PersistedAiToolConfiguration, WebSearchRoutingMode,
};

/// Semantic AI capability kinds exposed by the workbench quick-toggle surface.
/// 工作台快捷开关界面暴露的语义 AI 能力类型集合。
pub const WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS: [AiToolKind; 7] = [
    AiToolKind::WebSearch,
    AiToolKind::UnderstandImage,
    AiToolKind::ImageGeneration,
    AiToolKind::VideoGeneration,
    AiToolKind::SpeechSynthesis,
    AiToolKind::MusicGeneration,
    AiToolKind::MusicCover,
];

/// Parses one WebUI capability key into the shared provider-config enum.
/// 将单个 WebUI 能力键解析为共享 provider-config 枚举。
///
/// # Parameters
/// 参数
/// `value` is the snake_case capability key sent by the WebUI.
/// `value` 是 WebUI 发送的 snake_case 能力键。
///
/// # Returns
/// 返回值
/// The matching AI tool kind, or `None` when the key is unknown.
/// 匹配的 AI 工具类型；未知键返回 `None`。
pub fn parse_workbench_ai_tool_kind(value: &str) -> Option<AiToolKind> {
    match value.trim().to_ascii_lowercase().as_str() {
        "web_search" => Some(AiToolKind::WebSearch),
        "understand_image" => Some(AiToolKind::UnderstandImage),
        "image_generation" => Some(AiToolKind::ImageGeneration),
        "video_generation" => Some(AiToolKind::VideoGeneration),
        "speech_synthesis" => Some(AiToolKind::SpeechSynthesis),
        "music_generation" => Some(AiToolKind::MusicGeneration),
        "music_cover" => Some(AiToolKind::MusicCover),
        _ => None,
    }
}

/// Builds the disabled capability set from explicit quick-toggle choices only.
/// 仅根据显式快捷开关选择构建禁用能力集合。
///
/// # Parameters
/// 参数
/// `configuration` is the persisted AI tool configuration that carries explicit disabled choices.
/// `configuration` 是携带显式关闭选择的持久化 AI 工具配置。
/// `requested_disabled_kinds` contains user-disabled WebUI capability keys for this prompt.
/// `requested_disabled_kinds` 包含本次提示词中用户在 WebUI 禁用的能力键。
///
/// # Returns
/// 返回值
/// The effective disabled AI tool kind set without deriving native capability availability from host targets.
/// 不基于宿主目标推导原生能力可用性后的有效禁用 AI 工具类型集合。
pub fn effective_workbench_disabled_ai_capability_kinds(
    configuration: &PersistedAiToolConfiguration,
    requested_disabled_kinds: Option<&[String]>,
) -> Result<BTreeSet<AiToolKind>, String> {
    if let Some(requested_disabled_kinds) = requested_disabled_kinds {
        Ok(parse_disabled_capability_keys(requested_disabled_kinds)?)
    } else {
        Ok(configuration.disabled_capability_kinds.clone())
    }
}

/// Applies current-message capability disables to a cloned AI tool configuration.
/// 将当前消息级能力关闭状态应用到克隆后的 AI 工具配置。
///
/// # Parameters
/// 参数
/// `configuration` is the global persisted AI tool configuration.
/// `configuration` 是全局持久化 AI 工具配置。
/// `disabled_kinds` is the effective disabled semantic capability set.
/// `disabled_kinds` 是有效的禁用语义能力集合。
/// `disabled_target_selectors` is the runtime-only set of disabled `vendor/target` selectors.
/// `disabled_target_selectors` 是仅本次运行时禁用的 `vendor/target` 选择器集合。
/// `target_order_overrides` contains runtime-only route orders keyed by semantic capability kind.
/// `target_order_overrides` 包含按语义能力类型分组的仅运行时路由顺序覆盖。
///
/// # Returns
/// 返回值
/// A runtime-only AI tool configuration snapshot for this prompt.
/// 当前提示词使用的仅运行时 AI 工具配置快照。
pub fn effective_ai_tool_configuration_for_prompt_overrides(
    configuration: &PersistedAiToolConfiguration,
    disabled_kinds: &BTreeSet<AiToolKind>,
    disabled_target_selectors: &BTreeSet<String>,
    target_order_overrides: &BTreeMap<AiToolKind, Vec<String>>,
) -> PersistedAiToolConfiguration {
    let mut effective_configuration = configuration.clone();
    for (capability_kind, target_order) in target_order_overrides {
        apply_target_order_override(&mut effective_configuration, *capability_kind, target_order);
    }
    for disabled_kind in disabled_kinds {
        if *disabled_kind == AiToolKind::WebSearch {
            effective_configuration.web_search.mode = WebSearchRoutingMode::Disabled;
            continue;
        }
        for vendor in &mut effective_configuration.vendors {
            for binding in &mut vendor.targets {
                if binding.capability_kind == *disabled_kind {
                    binding.enabled = false;
                }
            }
        }
    }

    for vendor in &mut effective_configuration.vendors {
        for binding in &mut vendor.targets {
            let selector = format!("{}/{}", vendor.vendor_id.trim(), binding.target_alias());
            if disabled_target_selectors
                .iter()
                .any(|disabled| disabled.eq_ignore_ascii_case(&selector))
            {
                binding.enabled = false;
            }
        }
    }

    effective_configuration
}

/// Applies one runtime target-order override to the cloned AI tool configuration.
/// 将单个运行时目标顺序覆盖应用到克隆后的 AI 工具配置。
///
/// # Parameters
/// 参数
/// `configuration` is the cloned runtime AI tool configuration.
/// `configuration` 是克隆后的运行时 AI 工具配置。
/// `capability_kind` identifies the semantic capability being reordered.
/// `capability_kind` 标识正在调整顺序的语义能力。
/// `target_order` is the user-selected target order for the current prompt.
/// `target_order` 是用户为当前提示词选择的目标顺序。
fn apply_target_order_override(
    configuration: &mut PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
    target_order: &[String],
) {
    let configured_targets = configuration
        .resolved_configured_targets_for_kind(capability_kind)
        .into_iter()
        .map(|target| target.target_selector)
        .collect::<Vec<_>>();
    let mut next_order = Vec::new();
    for selector in target_order {
        if configured_targets
            .iter()
            .any(|configured| configured.eq_ignore_ascii_case(selector))
            && !next_order
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(selector))
        {
            next_order.push(selector.trim().to_string());
        }
    }
    for selector in configured_targets {
        if !next_order
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&selector))
        {
            next_order.push(selector);
        }
    }
    match capability_kind {
        AiToolKind::WebSearch => configuration.web_search.target_order = next_order,
        AiToolKind::UnderstandImage => configuration.understand_image.target_order = next_order,
        AiToolKind::ImageGeneration => configuration.image_generation.target_order = next_order,
        AiToolKind::VideoGeneration => configuration.video_generation.target_order = next_order,
        AiToolKind::SpeechSynthesis => configuration.speech_synthesis.target_order = next_order,
        AiToolKind::MusicGeneration => configuration.music_generation.target_order = next_order,
        AiToolKind::MusicCover => configuration.music_cover.target_order = next_order,
        _ => {}
    }
}

/// Parses runtime-disabled capability keys from the WebUI payload.
/// 从 WebUI 载荷中解析运行时禁用的能力键。
///
/// # Parameters
/// 参数
/// `requested_disabled_kinds` contains snake_case capability keys.
/// `requested_disabled_kinds` 包含 snake_case 能力键。
///
/// # Returns
/// 返回值
/// A semantic kind set containing only workbench quick-toggle kinds.
/// 仅包含工作台快捷开关能力的语义类型集合。
fn parse_disabled_capability_keys(
    requested_disabled_kinds: &[String],
) -> Result<BTreeSet<AiToolKind>, String> {
    let mut disabled = BTreeSet::new();
    for raw_kind in requested_disabled_kinds {
        let Some(kind) = parse_workbench_ai_tool_kind(raw_kind) else {
            return Err(format!("未知的扩展能力类型：{raw_kind}"));
        };
        if kind.supports_workbench_quick_toggle() {
            disabled.insert(kind);
        }
    }
    Ok(disabled)
}

#[cfg(test)]
mod tests {
    //! Workbench AI capability router regression tests.
    //! 工作台 AI 能力路由器回归测试。

    use super::effective_workbench_disabled_ai_capability_kinds;
    use vulcan_provider_config::config::{AiToolKind, PersistedAiToolConfiguration};

    /// Keeps capabilities enabled by default even when no host target is configured.
    /// 即使未配置宿主目标，也默认保持能力不被强制关闭。
    #[test]
    fn disabled_capability_set_does_not_infer_unavailable_from_host_targets() {
        let configuration = PersistedAiToolConfiguration::default();

        let disabled = effective_workbench_disabled_ai_capability_kinds(&configuration, None)
            .expect("disabled capability set should resolve");

        assert!(
            disabled.is_empty(),
            "host target absence should not disable native-capable tools"
        );
    }

    /// Still honors explicit per-prompt disabled capability choices from the WebUI.
    /// 仍然遵守 WebUI 为单次提示词显式选择的关闭能力。
    #[test]
    fn disabled_capability_set_honors_explicit_prompt_choices() {
        let configuration = PersistedAiToolConfiguration::default();

        let disabled = effective_workbench_disabled_ai_capability_kinds(
            &configuration,
            Some(&[String::from("web_search"), String::from("understand_image")]),
        )
        .expect("explicit disabled capability set should resolve");

        assert!(disabled.contains(&AiToolKind::WebSearch));
        assert!(disabled.contains(&AiToolKind::UnderstandImage));
        assert_eq!(disabled.len(), 2);
    }
}
