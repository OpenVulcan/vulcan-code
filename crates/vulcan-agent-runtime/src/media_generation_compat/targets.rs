//! Vendor target resolution and validation for media-generation capabilities.
//! 媒体生成能力的供应商目标解析与校验。

use crate::provider_config::{
    AiToolKind, PersistedAiCapabilityVendorEntry, PersistedAiToolConfiguration,
};

use super::types::ResolvedVendorTarget;

/// Resolves the one vendor target frozen by the configured capability binding.
/// 解析由当前能力绑定冻结的唯一供应商目标。
pub(super) fn resolve_vendor_target(
    configuration: &PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
) -> Result<ResolvedVendorTarget, String> {
    let configured_targets = configuration.usable_targets_for_kind(capability_kind);
    if configured_targets.is_empty() {
        return Err(format!(
            "No usable target is bound for `{}`. Bind exactly one target before registering the tool.",
            capability_kind.key()
        ));
    }
    if configured_targets.len() != 1 {
        return Err(format!(
            "The `{}` tool has {} usable targets configured. Bind exactly one target; automatic routing and provider fallback are not supported.",
            capability_kind.key(),
            configured_targets.len()
        ));
    }

    let target_selector = configured_targets
        .into_iter()
        .next()
        .expect("the exact-target length check guarantees one configured target");
    let (vendor_id, _) = split_vendor_target_selector(&target_selector).ok_or_else(|| {
        format!(
            "The configured target `{target_selector}` for `{}` is invalid.",
            capability_kind.key()
        )
    })?;
    let vendor = configuration
        .vendor_by_id(vendor_id)
        .filter(|vendor| vendor.enabled)
        .cloned()
        .ok_or_else(|| {
            format!(
                "The configured target `{target_selector}` for `{}` belongs to a missing or disabled vendor.",
                capability_kind.key()
            )
        })?;
    let binding = vendor
        .matching_binding(capability_kind, &target_selector)
        .cloned()
        .ok_or_else(|| {
            format!(
                "The configured target `{target_selector}` for `{}` does not resolve to an enabled capability binding.",
                capability_kind.key()
            )
        })?;
    Ok(ResolvedVendorTarget {
        vendor,
        binding,
        target_selector,
    })
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
