use super::*;

/// Persisted configuration for the unified web-search capability.
/// 统一网页搜索能力的持久化配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWebSearchConfiguration {
    /// Optional exact `vendor/target` binding selected by the user.
    /// 用户显式选择的可选精确 `供应商/目标` 绑定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_target: Option<String>,
    /// Legacy external provider records kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版外部供应商记录。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<PersistedWebSearchProviderEntry>,
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
    /// Optional exact `vendor/target` binding selected by the user.
    /// 用户显式选择的可选精确 `供应商/目标` 绑定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_target: Option<String>,
    /// Legacy external provider records kept only for compatibility migration.
    /// 仅为兼容迁移保留的旧版外部提供方记录。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub providers: Vec<PersistedUnderstandImageProviderEntry>,
}

impl Default for PersistedUnderstandImageConfiguration {
    /// Builds the default image-understanding configuration.
    /// 构建默认图片理解配置。
    fn default() -> Self {
        Self {
            selected_target: None,
            providers: Vec::new(),
        }
    }
}

/// Persisted target-binding configuration shared by host-owned AI capability tools.
/// 宿主 AI 能力工具共享的持久化目标绑定配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiCapabilityRouteConfiguration {
    /// Optional exact `vendor/target` binding selected by the user.
    /// 用户显式选择的可选精确 `供应商/目标` 绑定。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_target: Option<String>,
}

#[allow(dead_code)]
impl PersistedAiCapabilityRouteConfiguration {
    /// Returns whether the exact-target configuration still equals the built-in default.
    /// 返回精确目标配置是否仍等于内置默认值。
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }

    /// Normalizes the optional exact target selector.
    /// 归一化可选的精确目标选择器。
    pub fn normalize(&mut self) -> bool {
        let original = self.selected_target.clone();
        self.selected_target = normalize_optional_target_selector(self.selected_target.as_deref());
        self.selected_target != original
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
    pub fn has_usable_provider_binding(&self) -> bool {
        self.selected_target.is_some() || !self.providers.is_empty()
    }

    /// Returns the ordered list of currently usable semantic image-understanding targets.
    /// 返回当前可用语义图片理解目标的有序列表。
    pub fn usable_targets(&self) -> Vec<String> {
        self.selected_target.iter().cloned().collect()
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
        self.selected_target = normalize_optional_target_selector(self.selected_target.as_deref());
        self != &original
    }
}

impl Default for PersistedWebSearchConfiguration {
    /// Builds the default web-search configuration.
    /// 构建默认网页搜索配置。
    fn default() -> Self {
        Self {
            selected_target: None,
            providers: Vec::new(),
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
    pub fn has_usable_provider_binding(&self) -> bool {
        self.selected_target.is_some() || !self.providers.is_empty()
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
        self.selected_target = normalize_optional_target_selector(self.selected_target.as_deref());
        if self.max_results == 0 {
            self.max_results = default_web_search_max_results();
        }
        self != &original
    }

    /// Returns the ordered list of currently usable semantic web-search targets.
    /// 返回当前可用语义网页搜索目标的有序列表。
    pub fn usable_targets(&self) -> Vec<String> {
        self.selected_target.iter().cloned().collect()
    }
}
