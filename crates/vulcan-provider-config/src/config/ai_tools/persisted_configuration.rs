use super::*;

/// Root persisted API-native tool configuration.
/// 根级持久化 API 原生工具配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiToolConfiguration {
    /// Shared AI capability vendor instances configured once and reused across multiple semantic capabilities.
    /// 单次配置并可在多个语义能力之间复用的共享 AI 能力供应商实例集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vendors: Vec<PersistedAiCapabilityVendorEntry>,
    /// Semantic capability kinds that stay disabled in the workbench quick-toggle surface until the user explicitly re-enables them.
    /// 在工作台快捷开关界面中保持关闭、直到用户显式重新开启的语义能力类型集合。
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub disabled_capability_kinds: BTreeSet<AiToolKind>,
    /// Unified web-search configuration.
    /// 统一网页搜索配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedWebSearchConfiguration::is_default"
    )]
    pub web_search: PersistedWebSearchConfiguration,
    /// Standalone image-understanding configuration.
    /// 独立图片理解配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedUnderstandImageConfiguration::is_default"
    )]
    pub understand_image: PersistedUnderstandImageConfiguration,
    /// Standalone image-generation configuration.
    /// 独立图片生成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub image_generation: PersistedAiCapabilityRouteConfiguration,
    /// Standalone video-generation configuration.
    /// 独立视频生成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub video_generation: PersistedAiCapabilityRouteConfiguration,
    /// Standalone speech-synthesis configuration.
    /// 独立语音合成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub speech_synthesis: PersistedAiCapabilityRouteConfiguration,
    /// Standalone music-generation configuration.
    /// 独立音乐生成配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub music_generation: PersistedAiCapabilityRouteConfiguration,
    /// Standalone music-cover configuration.
    /// 独立音乐翻唱配置。
    #[serde(
        default,
        skip_serializing_if = "PersistedAiCapabilityRouteConfiguration::is_default"
    )]
    pub music_cover: PersistedAiCapabilityRouteConfiguration,
}

#[allow(dead_code)]
impl PersistedAiToolConfiguration {
    /// Returns whether all API-native tool settings are still defaults.
    /// 返回全部 API 原生工具设置是否仍为默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes nested capability-provider sections into their canonical standalone shape.
    /// 将嵌套能力提供方分区归一化为其规范独立形态。
    pub fn normalize(&mut self) -> bool {
        let original_disabled_capability_kinds = self.disabled_capability_kinds.clone();
        self.disabled_capability_kinds
            .retain(|kind| kind.supports_workbench_quick_toggle());
        let web_search_changed = self.web_search.normalize();
        let understand_image_changed = self.understand_image.normalize();
        let image_generation_changed = self.image_generation.normalize();
        let video_generation_changed = self.video_generation.normalize();
        let speech_synthesis_changed = self.speech_synthesis.normalize();
        let music_generation_changed = self.music_generation.normalize();
        let music_cover_changed = self.music_cover.normalize();
        let migration_changed = self.migrate_legacy_capability_entries_into_vendors();
        let vendor_changed = normalize_ai_capability_vendors(&mut self.vendors);
        let route_changed = self.normalize_capability_route_orders();
        let disabled_capability_changed =
            self.disabled_capability_kinds != original_disabled_capability_kinds;
        web_search_changed
            || disabled_capability_changed
            || understand_image_changed
            || image_generation_changed
            || video_generation_changed
            || speech_synthesis_changed
            || music_generation_changed
            || music_cover_changed
            || migration_changed
            || vendor_changed
            || route_changed
    }

    /// Returns the ordered web-search provider entries projected from the vendor-centric configuration.
    /// 返回从供应商中心配置投影出的有序网页搜索供应商条目。
    pub fn projected_web_search_providers(&self) -> Vec<PersistedWebSearchProviderEntry> {
        self.projected_target_order_for_kind(AiToolKind::WebSearch)
            .into_iter()
            .filter_map(|target| self.projected_web_search_provider_for_target(&target))
            .collect()
    }

    /// Returns the ordered image-understanding provider entries projected from the vendor-centric configuration.
    /// 返回从供应商中心配置投影出的有序图片理解提供方条目。
    pub fn projected_understand_image_providers(
        &self,
    ) -> Vec<PersistedUnderstandImageProviderEntry> {
        self.projected_target_order_for_kind(AiToolKind::UnderstandImage)
            .into_iter()
            .filter_map(|target| self.projected_understand_image_provider_for_target(&target))
            .collect()
    }

    /// Returns whether at least one enabled vendor target currently exists for the requested semantic capability.
    /// 返回所请求语义能力当前是否至少存在一条已启用供应商目标。
    pub fn has_usable_targets_for_kind(&self, capability_kind: AiToolKind) -> bool {
        !self
            .projected_target_order_for_kind(capability_kind)
            .is_empty()
    }

    /// Returns the total count of currently usable enabled targets for one semantic capability.
    /// 返回单个语义能力当前可用且已启用目标的总数量。
    pub fn enabled_target_count_for_kind(&self, capability_kind: AiToolKind) -> usize {
        self.vendors
            .iter()
            .filter(|vendor| vendor.enabled)
            .map(|vendor| {
                vendor
                    .targets
                    .iter()
                    .filter(|binding| binding.enabled && binding.capability_kind == capability_kind)
                    .count()
            })
            .sum()
    }

    /// Returns the ordered list of enabled fully qualified `vendor/target` selectors for one semantic capability.
    /// 返回单个语义能力当前启用的完整 `供应商/目标` 选择器有序列表。
    pub fn usable_targets_for_kind(&self, capability_kind: AiToolKind) -> Vec<String> {
        self.projected_target_order_for_kind(capability_kind)
    }

    /// Returns the persisted route-order slice configured for one semantic capability before enabled targets are reconciled.
    /// 返回在对齐启用目标前为某个语义能力配置的持久化路由顺序切片。
    pub fn configured_target_order_for_kind(&self, capability_kind: AiToolKind) -> &[String] {
        match capability_kind {
            AiToolKind::WebSearch => &self.web_search.target_order,
            AiToolKind::UnderstandImage => &self.understand_image.target_order,
            AiToolKind::ImageGeneration => &self.image_generation.target_order,
            AiToolKind::VideoGeneration => &self.video_generation.target_order,
            AiToolKind::SpeechSynthesis => &self.speech_synthesis.target_order,
            AiToolKind::MusicGeneration => &self.music_generation.target_order,
            AiToolKind::MusicCover => &self.music_cover.target_order,
            _ => &EMPTY_TARGET_ORDER,
        }
    }

    /// Reorders one fully qualified capability target before another target or to the tail inside the persisted route preference list.
    /// 将单条完整能力目标重排到另一目标之前，或重排到持久化路由偏好列表尾部。
    pub fn reorder_target_for_kind(
        &mut self,
        capability_kind: AiToolKind,
        dragged_target: &str,
        before_target: Option<&str>,
    ) -> bool {
        let route_order = match capability_kind {
            AiToolKind::WebSearch => &mut self.web_search.target_order,
            AiToolKind::UnderstandImage => &mut self.understand_image.target_order,
            AiToolKind::ImageGeneration => &mut self.image_generation.target_order,
            AiToolKind::VideoGeneration => &mut self.video_generation.target_order,
            AiToolKind::SpeechSynthesis => &mut self.speech_synthesis.target_order,
            AiToolKind::MusicGeneration => &mut self.music_generation.target_order,
            AiToolKind::MusicCover => &mut self.music_cover.target_order,
            _ => return false,
        };
        let normalized_dragged_target = dragged_target.trim().to_ascii_lowercase();
        let Some(current_index) = route_order
            .iter()
            .position(|existing| existing.eq_ignore_ascii_case(&normalized_dragged_target))
        else {
            return false;
        };
        let mut target_index = if let Some(before_target) = before_target {
            let normalized_before_target = before_target.trim().to_ascii_lowercase();
            let Some(index) = route_order
                .iter()
                .position(|existing| existing.eq_ignore_ascii_case(&normalized_before_target))
            else {
                return false;
            };
            index
        } else {
            route_order.len()
        };
        let entry = route_order.remove(current_index);
        if current_index < target_index {
            target_index = target_index.saturating_sub(1);
        }
        if target_index == current_index {
            route_order.insert(current_index, entry);
            return false;
        }
        let bounded_target_index = target_index.min(route_order.len());
        route_order.insert(bounded_target_index, entry);
        true
    }

    /// Returns the ordered route entries rendered in capability settings pages for one semantic capability.
    /// 返回某个语义能力在设置页面中渲染使用的有序路由条目集合。
    pub fn resolved_route_targets_for_kind(
        &self,
        capability_kind: AiToolKind,
    ) -> Vec<ResolvedAiCapabilityRouteTarget> {
        self.projected_target_order_for_kind(capability_kind)
            .into_iter()
            .filter_map(|target| self.resolve_route_target_for_kind(capability_kind, &target))
            .collect()
    }

    /// Returns the ordered route entries rendered in settings and quick panels for all configured targets of one semantic capability, including currently disabled items.
    /// 返回设置页与快捷面板为某个语义能力渲染的全部已配置目标有序条目，包含当前已禁用项。
    pub fn resolved_configured_targets_for_kind(
        &self,
        capability_kind: AiToolKind,
    ) -> Vec<ResolvedAiCapabilityRouteTarget> {
        build_stable_route_target_order(
            self.configured_target_order_for_kind(capability_kind),
            &self.vendors,
            capability_kind,
        )
        .into_iter()
        .filter_map(|target| self.resolve_route_target_for_kind(capability_kind, &target))
        .collect()
    }

    /// Returns the vendor instance matching the provided identifier when one exists.
    /// 返回与给定标识匹配的供应商实例（若存在）。
    pub fn vendor_by_id(&self, vendor_id: &str) -> Option<&PersistedAiCapabilityVendorEntry> {
        let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
        self.vendors
            .iter()
            .find(|vendor| vendor.vendor_id == normalized_vendor_id)
    }

    /// Returns the mutable vendor instance matching the provided identifier when one exists.
    /// 返回与给定标识匹配的可变供应商实例（若存在）。
    pub fn vendor_by_id_mut(
        &mut self,
        vendor_id: &str,
    ) -> Option<&mut PersistedAiCapabilityVendorEntry> {
        let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
        self.vendors
            .iter_mut()
            .find(|vendor| vendor.vendor_id == normalized_vendor_id)
    }

    /// Migrates legacy capability-scoped provider rows into the shared vendor list, merging shared credentials when possible.
    /// 将旧版按能力分散的提供方行迁移到共享供应商列表中，并在可能时合并共享凭据。
    fn migrate_legacy_capability_entries_into_vendors(&mut self) -> bool {
        let original = self.clone();

        for provider in self.web_search.providers.clone() {
            self.upsert_vendor_target_from_web_search_provider(provider);
        }
        for provider in self.understand_image.providers.clone() {
            self.upsert_vendor_target_from_understand_image_provider(provider);
        }
        if !self.web_search.providers.is_empty() {
            self.web_search.providers.clear();
        }
        if !self.understand_image.providers.is_empty() {
            self.understand_image.providers.clear();
        }

        self != &original
    }

    /// Ensures route-order arrays stay aligned with the currently enabled vendor targets.
    /// 确保路由顺序数组始终与当前已启用的供应商目标保持对齐。
    fn normalize_capability_route_orders(&mut self) -> bool {
        let original = (
            self.web_search.target_order.clone(),
            self.understand_image.target_order.clone(),
            self.image_generation.target_order.clone(),
            self.video_generation.target_order.clone(),
            self.speech_synthesis.target_order.clone(),
            self.music_generation.target_order.clone(),
            self.music_cover.target_order.clone(),
        );
        self.web_search.target_order = build_stable_route_target_order(
            &self.web_search.target_order,
            &self.vendors,
            AiToolKind::WebSearch,
        );
        self.understand_image.target_order = build_stable_route_target_order(
            &self.understand_image.target_order,
            &self.vendors,
            AiToolKind::UnderstandImage,
        );
        self.image_generation.target_order = build_stable_route_target_order(
            &self.image_generation.target_order,
            &self.vendors,
            AiToolKind::ImageGeneration,
        );
        self.video_generation.target_order = build_stable_route_target_order(
            &self.video_generation.target_order,
            &self.vendors,
            AiToolKind::VideoGeneration,
        );
        self.speech_synthesis.target_order = build_stable_route_target_order(
            &self.speech_synthesis.target_order,
            &self.vendors,
            AiToolKind::SpeechSynthesis,
        );
        self.music_generation.target_order = build_stable_route_target_order(
            &self.music_generation.target_order,
            &self.vendors,
            AiToolKind::MusicGeneration,
        );
        self.music_cover.target_order = build_stable_route_target_order(
            &self.music_cover.target_order,
            &self.vendors,
            AiToolKind::MusicCover,
        );
        original.0 != self.web_search.target_order
            || original.1 != self.understand_image.target_order
            || original.2 != self.image_generation.target_order
            || original.3 != self.video_generation.target_order
            || original.4 != self.speech_synthesis.target_order
            || original.5 != self.music_generation.target_order
            || original.6 != self.music_cover.target_order
    }

    /// Projects one fully qualified web-search target into the legacy provider-entry shape reused by the runtime adapter.
    /// 将单条完整网页搜索目标投影为运行时适配器复用的旧版供应商条目形态。
    fn projected_web_search_provider_for_target(
        &self,
        target: &str,
    ) -> Option<PersistedWebSearchProviderEntry> {
        let (vendor_id, _) = split_vendor_target_selector(target)?;
        let vendor = self.vendor_by_id(vendor_id)?;
        let binding = vendor.matching_binding(AiToolKind::WebSearch, target)?;
        let kind = WebSearchProviderKind::from_key(vendor_id)?;
        Some(PersistedWebSearchProviderEntry {
            id: format!("{}::{}", vendor.vendor_id, binding.target_alias()),
            name: format_ai_capability_target_name(&vendor.name, binding.target_alias()),
            kind,
            target_alias: binding.target_alias().to_string(),
            enabled: vendor.enabled && binding.enabled,
            api_key: vendor.api_key.clone(),
            base_url: vendor.resolved_vendor_base_url(),
            model: binding.model_id().to_string(),
            region: vendor.region.clone(),
            max_results: self.web_search.max_results.max(1),
        })
    }

    /// Projects one fully qualified image-understanding target into the legacy provider-entry shape reused by the runtime adapter.
    /// 将单条完整图片理解目标投影为运行时适配器复用的旧版提供方条目形态。
    fn projected_understand_image_provider_for_target(
        &self,
        target: &str,
    ) -> Option<PersistedUnderstandImageProviderEntry> {
        let (vendor_id, _) = split_vendor_target_selector(target)?;
        let vendor = self.vendor_by_id(vendor_id)?;
        let binding = vendor.matching_binding(AiToolKind::UnderstandImage, target)?;
        let kind = UnderstandImageProviderKind::ALL
            .into_iter()
            .find(|kind| kind.key() == vendor_id)?;
        Some(PersistedUnderstandImageProviderEntry {
            id: format!("{}::{}", vendor.vendor_id, binding.target_alias()),
            name: format_ai_capability_target_name(&vendor.name, binding.target_alias()),
            kind,
            target_alias: binding.target_alias().to_string(),
            enabled: vendor.enabled && binding.enabled,
            api_key: vendor.api_key.clone(),
            base_url: vendor.base_url.clone(),
            region: vendor.region.clone(),
        })
    }

    /// Returns the effective target order for the requested capability by honoring configured route preference and then appending any newly enabled targets.
    /// 返回请求能力的生效目标顺序：先尊重已配置路由顺序，再补上新启用目标。
    fn projected_target_order_for_kind(&self, capability_kind: AiToolKind) -> Vec<String> {
        let route_order = match capability_kind {
            AiToolKind::WebSearch => &self.web_search.target_order,
            AiToolKind::UnderstandImage => &self.understand_image.target_order,
            AiToolKind::ImageGeneration => &self.image_generation.target_order,
            AiToolKind::VideoGeneration => &self.video_generation.target_order,
            AiToolKind::SpeechSynthesis => &self.speech_synthesis.target_order,
            AiToolKind::MusicGeneration => &self.music_generation.target_order,
            AiToolKind::MusicCover => &self.music_cover.target_order,
            _ => &EMPTY_TARGET_ORDER,
        };
        build_effective_route_target_order(route_order, &self.vendors, capability_kind)
    }

    /// Resolves one fully qualified target into the settings-facing route entry rendered in capability pages.
    /// 将单条完整目标解析成能力页面渲染使用的设置条目。
    fn resolve_route_target_for_kind(
        &self,
        capability_kind: AiToolKind,
        target: &str,
    ) -> Option<ResolvedAiCapabilityRouteTarget> {
        let (vendor_id, target_alias) = split_vendor_target_selector(target)?;
        let vendor = self.vendor_by_id(vendor_id)?;
        let binding = vendor.matching_configured_binding(capability_kind, target)?;
        let target_definition =
            ai_capability_provider_catalog::ai_capability_provider_target_by_target(
                vendor_id,
                capability_kind,
                target_alias,
            );
        let target_name = target_definition
            .map(|definition| definition.name)
            .unwrap_or_else(|| binding.target_alias().to_string());
        Some(ResolvedAiCapabilityRouteTarget {
            capability_kind,
            target_selector: format!("{}/{}", vendor.vendor_id, binding.target_alias()),
            vendor_id: vendor.vendor_id.clone(),
            vendor_name: vendor.name.clone(),
            target_alias: binding.target_alias().to_string(),
            target_name,
            model_id: binding.model_id().to_string(),
            enabled: vendor.enabled && binding.enabled,
        })
    }

    /// Upserts one legacy web-search provider into the shared vendor list.
    /// 将单条旧版网页搜索供应商配置上推合并到共享供应商列表中。
    fn upsert_vendor_target_from_web_search_provider(
        &mut self,
        provider: PersistedWebSearchProviderEntry,
    ) {
        if !web_search_provider_kind_is_supported(provider.kind) {
            return;
        }
        let vendor_id = provider.kind.key();
        let target_alias = provider.target_alias().to_string();
        let target_selector = format!("{vendor_id}/{target_alias}");
        let vendor = ensure_ai_capability_vendor_entry(&mut self.vendors, vendor_id);
        merge_vendor_credentials_from_legacy_provider(
            vendor,
            &provider.name,
            &provider.api_key,
            &provider.base_url,
            &provider.region,
        );
        upsert_ai_capability_binding(
            &mut vendor.targets,
            PersistedAiCapabilityTargetBinding::new(
                AiToolKind::WebSearch,
                target_alias.clone(),
                provider.resolved_model(),
            ),
            provider.enabled,
        );
        if self.web_search.target_order.is_empty()
            || !self.web_search.target_order.contains(&target_selector)
        {
            self.web_search.target_order.push(target_selector);
        }
    }

    /// Upserts one legacy image-understanding provider into the shared vendor list.
    /// 将单条旧版图片理解提供方配置上推合并到共享供应商列表中。
    fn upsert_vendor_target_from_understand_image_provider(
        &mut self,
        provider: PersistedUnderstandImageProviderEntry,
    ) {
        if !understand_image_provider_kind_is_supported(provider.kind) {
            return;
        }
        let vendor_id = provider.kind.key();
        let target_alias = provider.target_alias().to_string();
        let target_selector = format!("{vendor_id}/{target_alias}");
        let vendor = ensure_ai_capability_vendor_entry(&mut self.vendors, vendor_id);
        merge_vendor_credentials_from_legacy_provider(
            vendor,
            &provider.name,
            &provider.api_key,
            &provider.base_url,
            &provider.region,
        );
        upsert_ai_capability_binding(
            &mut vendor.targets,
            PersistedAiCapabilityTargetBinding::new(
                AiToolKind::UnderstandImage,
                target_alias.clone(),
                target_alias.clone(),
            ),
            provider.enabled,
        );
        if self.understand_image.target_order.is_empty()
            || !self
                .understand_image
                .target_order
                .contains(&target_selector)
        {
            self.understand_image.target_order.push(target_selector);
        }
    }
}

/// Shared empty route-order slice reused by capability kinds that do not yet persist explicit route state.
/// 供尚未持久化显式路由状态的能力类型复用的共享空顺序切片。
static EMPTY_TARGET_ORDER: Vec<String> = Vec::new();
