//! Vendor target resolution and validation for media-generation capabilities.
//! 媒体生成能力的供应商目标解析与校验。

use crate::provider_config::{
    AiToolKind, PersistedAiCapabilityVendorEntry, PersistedAiToolConfiguration,
};

use super::types::ResolvedVendorTarget;

/// Resolves one ordered vendor-target attempt list from the configured capability routing state.
/// 根据当前能力路由配置解析一组有序的供应商目标尝试列表。
pub(super) fn resolve_vendor_targets(
    configuration: &PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
    explicit_target: Option<&str>,
) -> Result<Vec<ResolvedVendorTarget>, String> {
    let target_order = if let Some(explicit_target) = explicit_target
        .map(str::trim)
        .filter(|value| !value.is_empty() && !value.eq_ignore_ascii_case("auto"))
    {
        vec![explicit_target.to_string()]
    } else {
        configuration.usable_targets_for_kind(capability_kind)
    };
    if target_order.is_empty() {
        return Err(format!(
            "No usable targets are configured for `{}`.",
            capability_kind.key()
        ));
    }

    let mut resolved = Vec::new();
    for target_selector in target_order {
        let Some((vendor_id, _)) = split_vendor_target_selector(&target_selector) else {
            continue;
        };
        let Some(vendor) = configuration.vendor_by_id(vendor_id).cloned() else {
            continue;
        };
        if !vendor.enabled {
            continue;
        }
        let Some(binding) = vendor
            .matching_binding(capability_kind, &target_selector)
            .cloned()
        else {
            continue;
        };
        resolved.push(ResolvedVendorTarget {
            vendor,
            binding,
            target_selector,
        });
    }

    if resolved.is_empty() {
        return Err(format!(
            "The requested target for `{}` does not exist or is not available. Available targets: {}.",
            capability_kind.key(),
            configuration
                .usable_targets_for_kind(capability_kind)
                .join(", ")
        ));
    }

    Ok(resolved)
}

/// Ensures the requested target currently belongs to the MiniMax vendor and matches the expected capability kind.
/// 确保请求目标当前属于 MiniMax 供应商，且与预期能力类型一致。
pub(super) fn ensure_minimax_vendor(
    target: &ResolvedVendorTarget,
    capability_kind: AiToolKind,
) -> Result<(), String> {
    if target.vendor.vendor_id != "minimax" || target.binding.capability_kind != capability_kind {
        return Err(format!(
            "Only MiniMax `{}` capability targets are currently implemented.",
            capability_kind.key()
        ));
    }
    if crate::ai_capability_provider_catalog::ai_capability_provider_definition_by_target(
        &target.vendor.vendor_id,
        capability_kind,
        target.binding.target_alias(),
    )
    .is_none()
    {
        return Err(format!(
            "The MiniMax `{}` target `{}` is not registered in the capability resource catalog.",
            capability_kind.key(),
            target.binding.target_alias()
        ));
    }
    Ok(())
}

/// Resolves and validates the shared API key for one vendor entry.
/// 解析并校验单个供应商条目的共享 API Key。
pub(super) fn require_vendor_api_key(
    vendor: &PersistedAiCapabilityVendorEntry,
    provider_label: &str,
) -> Result<String, String> {
    vendor
        .resolved_api_key()?
        .ok_or_else(|| format!("{provider_label} is missing a valid API key."))
}

/// Splits one fully qualified `vendor/target` selector into its vendor and target components.
/// 将单个完整 `供应商/目标` 选择器拆分为供应商与目标两部分。
fn split_vendor_target_selector(target: &str) -> Option<(&str, &str)> {
    let trimmed = target.trim();
    let (vendor_id, target_alias) = trimmed.split_once('/')?;
    let vendor_id = vendor_id.trim();
    let target_alias = target_alias.trim();
    if vendor_id.is_empty() || target_alias.is_empty() {
        return None;
    }
    Some((vendor_id, target_alias))
}
