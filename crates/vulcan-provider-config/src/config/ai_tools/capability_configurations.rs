use super::*;

/// Persisted configuration for the unified web-search capability.
/// 统一网页搜索能力的持久化配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWebSearchConfiguration {
    /// User-selected web-search routing mode.
    /// 用户选择的网页搜索路由模式。
    #[serde(default)]
    pub mode: WebSearchRoutingMode,
    /// Ordered fully qualified `vendor/target` selectors used when the semantic tool receives `target=auto`.
    /// 当语义工具收到 `target=auto` 时使用的完整 `供应商/目标` 选择器顺序。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
    /// Legacy ordered external provider fallback chain kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版按顺序排列的外部供应商回退链。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<PersistedWebSearchProviderEntry>,
    /// Legacy dispatch mode kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版调度模式。
    #[serde(default, skip_serializing_if = "is_default")]
    pub dispatch_mode: WebSearchProviderDispatchMode,
    /// Default result-count hint used when the model does not provide an explicit `max_results`.
    /// 当模型未显式提供 `max_results` 时使用的默认结果数量提示。
    #[serde(default = "default_web_search_max_results")]
    pub max_results: usize,
}

/// Persisted configuration for the standalone image-understanding capability.
/// 独立图片理解能力的持久化配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedUnderstandImageConfiguration {
    /// Ordered fully qualified `vendor/target` selectors used when the semantic tool receives `target=auto`.
    /// 当语义工具收到 `target=auto` 时使用的完整 `供应商/目标` 选择器顺序。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
    /// Legacy ordered external provider list kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版按顺序排列的外部提供方列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<PersistedUnderstandImageProviderEntry>,
}

impl Default for PersistedUnderstandImageConfiguration {
    /// Builds the default image-understanding configuration.
    /// 构建默认图片理解配置。
    fn default() -> Self {
        Self {
            target_order: Vec::new(),
            providers: Vec::new(),
        }
    }
}

/// Persisted route-order configuration shared by host-owned AI capability tools that only need ordered `vendor/target` selectors.
/// 仅需要有序 `供应商/目标` 选择器的宿主 AI 能力工具共享的持久化路由顺序配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiCapabilityRouteConfiguration {
    /// Ordered fully qualified `vendor/target` selectors used when the semantic tool receives `target=auto`.
    /// 当语义工具收到 `target=auto` 时使用的完整 `供应商/目标` 选择器顺序。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_order: Vec<String>,
}

#[allow(dead_code)]
impl PersistedAiCapabilityRouteConfiguration {
    /// Returns whether the route-order configuration still equals the built-in default.
    /// 返回路由顺序配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes the persisted route-order array into one deduplicated selector list.
    /// 将持久化路由顺序数组归一化为去重后的选择器列表。
    pub fn normalize(&mut self) -> bool {
        let original = self.target_order.clone();
        self.target_order = normalize_route_target_order(&self.target_order);
        self.target_order != original
    }
}

#[allow(dead_code)]
impl PersistedUnderstandImageConfiguration {
    /// Returns whether the image-understanding configuration still equals the built-in default.
    /// 返回图片理解配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Returns whether at least one configured provider can currently be used.
    /// 返回当前是否至少存在一个可用的已配置提供方。
    pub fn has_usable_provider_chain(&self) -> bool {
        !self.target_order.is_empty() || !self.providers.is_empty()
    }

    /// Returns the ordered list of currently usable semantic image-understanding targets.
    /// 返回当前可用语义图片理解目标的有序列表。
    pub fn usable_targets(&self) -> Vec<String> {
        self.target_order.clone()
    }

    /// Normalizes persisted provider rows so each provider kind appears at most once with one stable identifier.
    /// 归一化持久化提供方行，确保每种提供方类型至多出现一次且拥有稳定标识。
    pub fn normalize(&mut self) -> bool {
        let original = self.clone();
        let mut normalized = Vec::new();

        for mut provider in self.providers.drain(..) {
            if !understand_image_provider_kind_is_supported(provider.kind) {
                continue;
            }
            if normalized
                .iter()
                .any(|existing: &PersistedUnderstandImageProviderEntry| {
                    existing.kind == provider.kind
                })
            {
                continue;
            }

            provider.id = provider.kind.key().to_string();
            if provider.target_alias.trim().is_empty() {
                provider.target_alias = provider.kind.default_target_alias().to_string();
            }
            if provider.name.trim().is_empty() {
                provider.name = provider.kind.label().to_string();
            }
            normalized.push(provider);
        }

        self.providers = normalized;
        self.target_order = normalize_route_target_order(&self.target_order);
        self != &original
    }
}

impl Default for PersistedWebSearchConfiguration {
    /// Builds the default web-search configuration.
    /// 构建默认网页搜索配置。
    fn default() -> Self {
        Self {
            mode: WebSearchRoutingMode::default(),
            target_order: Vec::new(),
            providers: Vec::new(),
            dispatch_mode: WebSearchProviderDispatchMode::default(),
            max_results: default_web_search_max_results(),
        }
    }
}

#[allow(dead_code)]
impl PersistedWebSearchConfiguration {
    /// Returns whether the web-search configuration still equals the built-in default.
    /// 返回网页搜索配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Returns whether at least one configured provider can be used.
    /// 返回是否至少存在一个可用的已配置供应商。
    pub fn has_usable_provider_chain(&self) -> bool {
        !self.target_order.is_empty() || !self.providers.is_empty()
    }

    /// Normalizes persisted provider rows so each provider kind appears at most once with one stable identifier.
    /// 归一化持久化提供方行，确保每种提供方类型至多出现一次且拥有稳定标识。
    pub fn normalize(&mut self) -> bool {
        let original = self.clone();
        let mut normalized = Vec::new();

        for mut provider in self.providers.drain(..) {
            if !web_search_provider_kind_is_supported(provider.kind) {
                continue;
            }
            if normalized
                .iter()
                .any(|existing: &PersistedWebSearchProviderEntry| existing.kind == provider.kind)
            {
                continue;
            }

            provider.id = provider.kind.key().to_string();
            if provider.target_alias.trim().is_empty() {
                provider.target_alias = provider.kind.default_target_alias().to_string();
            }
            if provider.name.trim().is_empty() {
                provider.name = provider.kind.label().to_string();
            }
            normalized.push(provider);
        }

        self.providers = normalized;
        self.target_order = normalize_route_target_order(&self.target_order);
        if self.mode == WebSearchRoutingMode::Disabled {
            self.mode = WebSearchRoutingMode::default();
        }
        if self.max_results == 0 {
            self.max_results = default_web_search_max_results();
        }
        self != &original
    }

    /// Returns the ordered list of currently usable semantic web-search targets.
    /// 返回当前可用语义网页搜索目标的有序列表。
    pub fn usable_targets(&self) -> Vec<String> {
        self.target_order.clone()
    }
}
