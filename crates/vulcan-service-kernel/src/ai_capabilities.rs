//! Shared AI capability vendor and target settings services.
//! 共享 AI 能力供应商与目标设置服务。

use std::collections::{BTreeMap, BTreeSet};

use serde::Deserialize;
use vulcan_agent_runtime::{
    agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_tool_visibility_cache,
    provider_capabilities::{ResolvedModelCapabilities, resolve_effective_model_capabilities},
    provider_config,
    workbench_ai_capability_router::WORKBENCH_QUICK_TOGGLE_AI_TOOL_KINDS,
};
use vulcan_provider_config::config::{
    AiCapabilityVendorConnectionMode, AiToolKind, PersistedAiCapabilityTargetBinding,
    PersistedAiCapabilityVendorEntry, PersistedAiToolConfiguration, ProviderConfigurationFile,
};
use vulcan_service_api::{
    AiCapabilitySummary, AiCapabilityTargetSummary, AiCapabilityVendorCatalogSummary,
    AiCapabilityVendorSummary, AiCapabilityVendorTargetChoiceSummary, ServiceEventEmitter,
    WEBUI_REDACTED_SECRET_PLACEHOLDER, WorkbenchSnapshot,
};

mod parsing;
mod services;
mod summaries;

pub(super) use parsing::*;
pub(super) use services::*;
pub(super) use summaries::*;

use crate::{
    SharedServiceBridgeState, emit_workbench_event, parse_ai_tool_kind,
    refresh_settings_projection_locked,
};

/// Applies a complete selector order for one AI capability kind.
/// 为单个 AI 能力类型应用完整目标选择器顺序。
///
/// # Parameters
/// 参数
/// `configuration` is the mutable persisted AI tool configuration.
/// `configuration` 是可变的持久化 AI 工具配置。
/// `capability_kind` selects which capability-specific order list should be replaced.
/// `capability_kind` 选择要替换的能力专属排序列表。
/// `ordered_selectors` is the complete selector order supplied by the caller.
/// `ordered_selectors` 是调用方提供的完整选择器顺序。
///
/// # Returns
/// 返回值
/// `Ok(())` after the capability order is replaced, or an error for unsupported kinds.
/// 能力顺序替换后返回 `Ok(())`；不支持的能力类型返回错误。
pub(super) fn set_ai_capability_target_order_for_kind(
    configuration: &mut PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
    ordered_selectors: Vec<String>,
) -> Result<(), String> {
    let target_order = match capability_kind {
        AiToolKind::WebSearch => &mut configuration.web_search.target_order,
        AiToolKind::UnderstandImage => &mut configuration.understand_image.target_order,
        AiToolKind::ImageGeneration => &mut configuration.image_generation.target_order,
        AiToolKind::VideoGeneration => &mut configuration.video_generation.target_order,
        AiToolKind::SpeechSynthesis => &mut configuration.speech_synthesis.target_order,
        AiToolKind::MusicGeneration => &mut configuration.music_generation.target_order,
        AiToolKind::MusicCover => &mut configuration.music_cover.target_order,
        _ => {
            return Err(format!(
                "AI 能力 `{}` 不支持目标排序。",
                capability_kind.key()
            ));
        }
    };
    *target_order = ordered_selectors;
    Ok(())
}
