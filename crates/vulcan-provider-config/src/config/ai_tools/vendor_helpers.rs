use super::*;

pub(super) fn normalize_ai_capability_vendor_id(vendor_id: impl Into<String>) -> String {
    vendor_id
        .into()
        .trim()
        .to_ascii_lowercase()
        .replace(['-', ' '], "_")
}

/// Returns the default vendor display name shown before the user customizes it.
/// 返回用户尚未自定义前显示的默认供应商名称。
pub(super) fn default_ai_capability_vendor_name(vendor_id: &str) -> String {
    match vendor_id.trim() {
        "duckduckgo" => String::from("DuckDuckGo"),
        "brave" => String::from("Brave"),
        "tavily" => String::from("Tavily"),
        "exa" => String::from("Exa"),
        "firecrawl" => String::from("Firecrawl"),
        "gemini" => String::from("Gemini"),
        "grok" => String::from("Grok"),
        "kimi" => String::from("Kimi"),
        "minimax" => String::from("MiniMax"),
        "ollama" => String::from("Ollama"),
        "perplexity" => String::from("Perplexity"),
        "searxng" => String::from("SearXNG"),
        "serpapi" => String::from("SerpAPI"),
        other if !other.is_empty() => other.to_string(),
        _ => String::from("AI Capability Vendor"),
    }
}

/// Returns the default vendor region value used before the user fills an explicit override.
/// 返回用户尚未填写显式覆盖值前使用的默认供应商区域。
pub(super) fn default_ai_capability_vendor_region(vendor_id: &str) -> String {
    if normalize_ai_capability_vendor_id(vendor_id.to_string()) == "minimax" {
        String::from("global")
    } else {
        String::new()
    }
}

/// Returns whether one shared AI capability vendor identifier is still supported by the current capability catalog.
/// 返回单个共享 AI 能力供应商标识当前是否仍受现有能力目录支持。
pub(super) fn ai_capability_vendor_is_supported(vendor_id: &str) -> bool {
    !ai_capability_provider_catalog::ai_capability_provider_definitions_for_vendor(vendor_id)
        .is_empty()
}

/// Returns whether one legacy web-search provider kind is still supported by the current capability catalog.
/// 返回单个旧版网页搜索提供方类型当前是否仍受现有能力目录支持。
pub(super) fn web_search_provider_kind_is_supported(kind: WebSearchProviderKind) -> bool {
    ai_capability_provider_catalog::web_search_provider_enabled(kind.key())
}

/// Returns whether one legacy image-understanding provider kind is still supported by the current capability catalog.
/// 返回单个旧版图片理解提供方类型当前是否仍受现有能力目录支持。
pub(super) fn understand_image_provider_kind_is_supported(
    kind: UnderstandImageProviderKind,
) -> bool {
    ai_capability_provider_catalog::understand_image_provider_enabled(kind.key())
}

/// Normalizes one MiniMax vendor-level base URL so shared credentials can drive multiple capability endpoints safely.
/// 归一化单个 MiniMax 供应商级 Base URL，使共享凭据能够安全驱动多个能力端点。
pub(super) fn normalize_minimax_vendor_base_url(base_url: &str) -> String {
    let trimmed = base_url.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let Ok(mut url) = reqwest::Url::parse(trimmed) else {
        return trimmed.to_string();
    };
    let normalized_path = url.path().trim_end_matches('/').to_ascii_lowercase();
    if normalized_path.ends_with("/v1/coding_plan/search")
        || normalized_path.ends_with("/v1/coding_plan/vlm")
        || normalized_path.ends_with("/v1/image_generation")
        || normalized_path.ends_with("/v1/video_generation")
        || normalized_path.ends_with("/v1/query/video_generation")
        || normalized_path.ends_with("/v1/t2a_v2")
        || normalized_path.ends_with("/v1/music_generation")
    {
        url.set_path("");
        url.set_query(None);
        return url.to_string().trim_end_matches('/').to_string();
    }
    trimmed.to_string()
}

/// Normalizes one route-order array into a deduplicated, non-empty selector list.
/// 将单条路由顺序数组归一化为去重且非空的选择器列表。
pub(super) fn normalize_route_target_order(target_order: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    for target in target_order {
        let normalized_target = target.trim();
        if normalized_target.is_empty() {
            continue;
        }
        if normalized
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(normalized_target))
        {
            continue;
        }
        normalized.push(normalized_target.to_string());
    }
    normalized
}

/// Normalizes all vendor instances in-place and removes duplicate vendor ids.
/// 原地归一化全部供应商实例并移除重复供应商标识。
pub(super) fn normalize_ai_capability_vendors(
    vendors: &mut Vec<PersistedAiCapabilityVendorEntry>,
) -> bool {
    let original = vendors.clone();
    let mut normalized = Vec::new();
    for mut vendor in vendors.drain(..) {
        vendor.normalize();
        if !ai_capability_vendor_is_supported(&vendor.vendor_id) {
            continue;
        }
        if normalized
            .iter()
            .any(|existing: &PersistedAiCapabilityVendorEntry| {
                existing.vendor_id == vendor.vendor_id
            })
        {
            continue;
        }
        normalized.push(vendor);
    }
    *vendors = normalized;
    *vendors != original
}

/// Returns the effective route target order by honoring the configured preference and then appending any enabled targets not yet mentioned.
/// 返回生效路由目标顺序：先尊重已配置偏好，再补齐尚未出现的已启用目标。
pub(super) fn build_effective_route_target_order(
    configured_order: &[String],
    vendors: &[PersistedAiCapabilityVendorEntry],
    capability_kind: AiToolKind,
) -> Vec<String> {
    let mut natural_targets = Vec::new();
    for vendor in vendors {
        if !vendor.enabled {
            continue;
        }
        natural_targets.extend(vendor.enabled_target_strings_for(capability_kind));
    }

    let mut ordered = Vec::new();
    for target in normalize_route_target_order(configured_order) {
        if natural_targets
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&target))
            && !ordered
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&target))
        {
            ordered.push(target);
        }
    }
    for target in natural_targets {
        if !ordered
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&target))
        {
            ordered.push(target);
        }
    }
    if ordered.len() > MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND {
        ordered.truncate(MAX_ENABLED_AI_CAPABILITY_TARGETS_PER_KIND);
    }
    ordered
}

/// Returns the stable persisted route target order by honoring the configured preference and then appending any configured targets not yet mentioned.
/// 返回稳定持久化的路由目标顺序：先尊重已配置偏好，再补齐尚未出现的全部已配置目标。
pub(super) fn build_stable_route_target_order(
    configured_order: &[String],
    vendors: &[PersistedAiCapabilityVendorEntry],
    capability_kind: AiToolKind,
) -> Vec<String> {
    let mut configured_targets = Vec::new();
    for vendor in vendors {
        configured_targets.extend(vendor.configured_target_strings_for(capability_kind));
    }

    let mut ordered = Vec::new();
    for target in normalize_route_target_order(configured_order) {
        if configured_targets
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&target))
            && !ordered
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(&target))
        {
            ordered.push(target);
        }
    }
    for target in configured_targets {
        if !ordered
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&target))
        {
            ordered.push(target);
        }
    }
    ordered
}

/// Ensures one vendor entry exists inside the shared vendor list and returns a mutable reference to it.
/// 确保共享供应商列表中存在单个供应商条目，并返回其可变引用。
pub(super) fn ensure_ai_capability_vendor_entry<'a>(
    vendors: &'a mut Vec<PersistedAiCapabilityVendorEntry>,
    vendor_id: &str,
) -> &'a mut PersistedAiCapabilityVendorEntry {
    let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
    if let Some(index) = vendors
        .iter()
        .position(|vendor| vendor.vendor_id == normalized_vendor_id)
    {
        return &mut vendors[index];
    }
    vendors.push(PersistedAiCapabilityVendorEntry::new(
        normalized_vendor_id.clone(),
    ));
    vendors
        .last_mut()
        .expect("vendor list should contain the newly inserted vendor entry")
}

/// Merges legacy capability-specific credential fields into one shared vendor entry without overwriting newer explicit values.
/// 将旧版按能力分散的凭据字段合并进共享供应商条目，且不覆盖更新的显式值。
pub(super) fn merge_vendor_credentials_from_legacy_provider(
    vendor: &mut PersistedAiCapabilityVendorEntry,
    provider_name: &str,
    api_key: &str,
    base_url: &str,
    region: &str,
) {
    if vendor.name.trim().is_empty() {
        vendor.name = provider_name.trim().to_string();
    }
    if vendor.api_key.trim().is_empty() && !api_key.trim().is_empty() {
        vendor.api_key = api_key.trim().to_string();
    }
    if vendor.base_url.trim().is_empty() && !base_url.trim().is_empty() {
        vendor.base_url = if vendor.vendor_id == "minimax" {
            normalize_minimax_vendor_base_url(base_url)
        } else {
            base_url.trim().to_string()
        };
    }
    if vendor.region.trim().is_empty() && !region.trim().is_empty() {
        vendor.region = region.trim().to_string();
    }
}

/// Upserts one binding into the provided binding list while preserving the shared capability-target identity.
/// 将单条绑定上推写入给定绑定列表，并保持共享的能力目标身份唯一性。
pub(super) fn upsert_ai_capability_binding(
    bindings: &mut Vec<PersistedAiCapabilityTargetBinding>,
    binding: PersistedAiCapabilityTargetBinding,
    enabled: bool,
) {
    if let Some(existing) = bindings.iter_mut().find(|existing| {
        existing.capability_kind == binding.capability_kind
            && existing
                .target_alias()
                .eq_ignore_ascii_case(binding.target_alias())
    }) {
        if existing.model_id.trim().is_empty() {
            existing.model_id = binding.model_id().to_string();
        }
        existing.enabled = existing.enabled || enabled;
        return;
    }

    let mut binding = binding;
    binding.enabled = enabled;
    bindings.push(binding);
}

/// Builds one compact target display name from a vendor display name plus one target alias.
/// 根据供应商显示名与目标别名构建紧凑目标显示名称。
pub(super) fn format_ai_capability_target_name(vendor_name: &str, target_alias: &str) -> String {
    let normalized_target_alias = target_alias.trim();
    if normalized_target_alias.is_empty()
        || vendor_name
            .trim()
            .eq_ignore_ascii_case(normalized_target_alias)
    {
        vendor_name.trim().to_string()
    } else {
        format!("{}/{}", vendor_name.trim(), normalized_target_alias)
    }
}

/// Splits one `vendor/target` selector into its two stable components.
/// 将单条 `供应商/目标` 选择器拆分成两个稳定组成部分。
pub(super) fn split_vendor_target_selector(target: &str) -> Option<(&str, &str)> {
    let trimmed = target.trim();
    let (vendor_id, target_alias) = trimmed.split_once('/')?;
    let vendor_id = vendor_id.trim();
    let target_alias = target_alias.trim();
    if vendor_id.is_empty() || target_alias.is_empty() {
        return None;
    }
    Some((vendor_id, target_alias))
}
