use super::*;

/// One configured external web-search provider entry in the fallback chain.
/// 回退链中的单个外部网页搜索供应商配置项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedWebSearchProviderEntry {
    /// Stable entry id used by the settings UI.
    /// 设置界面使用的稳定条目标识。
    pub id: String,
    /// User-facing display name.
    /// 面向用户展示的显示名称。
    pub name: String,
    /// Provider implementation kind.
    /// 供应商实现类型。
    pub kind: WebSearchProviderKind,
    /// Stable target alias exposed below the current vendor target.
    /// 当前供应商目标下方暴露的稳定目标别名。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_alias: String,
    /// Whether this entry participates in the runtime chain.
    /// 当前条目是否参与运行时搜索链。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// API key or `{env:NAME}` placeholder.
    /// API Key 或 `{env:NAME}` 环境变量占位符。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Optional endpoint override.
    /// 可选的接口地址覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Optional provider-specific model override.
    /// 可选的供应商专属模型覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model: String,
    /// Optional provider-specific region or locale override.
    /// 可选的供应商专属区域或地区覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
    /// Maximum result count requested from this provider.
    /// 向当前供应商请求的最大结果数量。
    #[serde(default = "default_web_search_max_results")]
    pub max_results: usize,
}

/// One configured external image-understanding provider entry in the standalone capability layer.
/// 独立能力层中的单个外部图片理解提供方配置项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedUnderstandImageProviderEntry {
    /// Stable entry id used by the settings UI.
    /// 设置界面使用的稳定条目标识。
    pub id: String,
    /// User-facing display name.
    /// 面向用户展示的显示名称。
    pub name: String,
    /// Provider implementation kind.
    /// 提供方实现类型。
    pub kind: UnderstandImageProviderKind,
    /// Stable target alias exposed below the current vendor target.
    /// 当前提供方目标下方暴露的稳定目标别名。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub target_alias: String,
    /// Whether this entry participates in the runtime chain.
    /// 当前条目是否参与运行时能力链。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// API key or `{env:NAME}` placeholder.
    /// API Key 或 `{env:NAME}` 环境变量占位符。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Optional endpoint override.
    /// 可选的接口地址覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Optional provider-specific region override.
    /// 可选的提供方专属区域覆盖。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
}

#[allow(dead_code)]
impl PersistedUnderstandImageProviderEntry {
    /// Builds one default provider entry with a stable identifier supplied by the caller.
    /// 使用调用方提供的稳定标识构建默认提供方条目。
    pub fn new(id: impl Into<String>, kind: UnderstandImageProviderKind) -> Self {
        Self {
            id: id.into(),
            name: kind.label().to_string(),
            kind,
            target_alias: kind.default_target_alias().to_string(),
            enabled: true,
            api_key: String::new(),
            base_url: String::new(),
            region: kind.default_region().to_string(),
        }
    }

    /// Returns the stable instance alias used by semantic `target` selection.
    /// 返回语义 `target` 选择使用的稳定实例别名。
    pub fn instance_alias(&self) -> &str {
        self.kind.key()
    }

    /// Returns the stable target alias exposed by this provider instance.
    /// 返回当前提供方实例暴露的稳定目标别名。
    pub fn target_alias(&self) -> &str {
        let trimmed = self.target_alias.trim();
        if trimmed.is_empty() {
            self.kind.default_target_alias()
        } else {
            trimmed
        }
    }

    /// Returns the fully qualified `instance/target` selector exposed to the model.
    /// 返回暴露给模型使用的完整 `实例/目标` 选择器。
    pub fn qualified_target(&self) -> String {
        format!("{}/{}", self.instance_alias(), self.target_alias())
    }

    /// Returns whether one user-specified target selector resolves to this provider instance.
    /// 返回某个用户指定的目标选择器是否解析到当前提供方实例。
    pub fn matches_target(&self, target: &str) -> bool {
        let normalized = target.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return false;
        }

        normalized == self.instance_alias().to_ascii_lowercase()
            || normalized == self.qualified_target().to_ascii_lowercase()
    }

    /// Returns the resolved endpoint, falling back to the provider default.
    /// 返回解析后的接口地址，留空时回退到提供方默认地址。
    pub fn resolved_base_url(&self) -> String {
        let trimmed = self.base_url.trim();
        if trimmed.is_empty() {
            self.kind.default_base_url().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns the resolved provider region, falling back to the provider default when available.
    /// 返回解析后的提供方区域；若存在默认值则在留空时自动回退。
    pub fn resolved_region(&self) -> String {
        let trimmed = self.region.trim();
        if trimmed.is_empty() {
            self.kind.default_region().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns whether this entry is currently usable by the runtime chain.
    /// 返回当前条目是否可被运行时能力链使用。
    pub fn is_usable(&self) -> bool {
        self.enabled && !self.api_key.trim().is_empty()
    }

    /// Resolves the stored API key into a runtime-ready credential string.
    /// 将已保存的 API Key 解析为运行时可直接使用的凭据字符串。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(Some(self.api_key.as_str()), self.kind.api_key_label())
    }
}

#[allow(dead_code)]
impl PersistedWebSearchProviderEntry {
    /// Builds one default provider entry with a unique id supplied by the caller.
    /// 使用调用方提供的唯一标识构建默认供应商条目。
    pub fn new(id: impl Into<String>, kind: WebSearchProviderKind) -> Self {
        Self {
            id: id.into(),
            name: kind.label().to_string(),
            kind,
            target_alias: kind.default_target_alias().to_string(),
            enabled: true,
            api_key: String::new(),
            base_url: String::new(),
            model: kind.default_model().to_string(),
            region: kind.default_region().to_string(),
            max_results: default_web_search_max_results(),
        }
    }

    /// Returns the stable instance alias used by semantic `target` selection.
    /// 返回语义 `target` 选择使用的稳定实例别名。
    pub fn instance_alias(&self) -> &str {
        self.kind.key()
    }

    /// Returns the stable target alias exposed by this search-provider instance.
    /// 返回当前搜索供应商实例暴露的稳定目标别名。
    pub fn target_alias(&self) -> &str {
        let trimmed = self.target_alias.trim();
        if trimmed.is_empty() {
            self.kind.default_target_alias()
        } else {
            trimmed
        }
    }

    /// Returns the fully qualified `instance/target` selector exposed to the model.
    /// 返回暴露给模型使用的完整 `实例/目标` 选择器。
    pub fn qualified_target(&self) -> String {
        format!("{}/{}", self.instance_alias(), self.target_alias())
    }

    /// Returns whether one user-specified target selector resolves to this provider instance.
    /// 返回某个用户指定的目标选择器是否解析到当前供应商实例。
    pub fn matches_target(&self, target: &str) -> bool {
        let normalized = target.trim().to_ascii_lowercase();
        if normalized.is_empty() {
            return false;
        }

        normalized == self.instance_alias().to_ascii_lowercase()
            || normalized == self.qualified_target().to_ascii_lowercase()
    }

    /// Returns the resolved endpoint, falling back to the provider default.
    /// 返回解析后的接口地址，留空时回退到供应商默认地址。
    pub fn resolved_base_url(&self) -> String {
        let trimmed = self.base_url.trim();
        if trimmed.is_empty() {
            self.kind.default_base_url().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns whether this entry is currently usable by the runtime chain.
    /// 返回当前条目是否可被运行时搜索链使用。
    pub fn is_usable(&self) -> bool {
        self.enabled
            && (!self.kind.requires_api_key() || !self.api_key.trim().is_empty())
            && match self.kind {
                WebSearchProviderKind::Searxng | WebSearchProviderKind::Ollama => {
                    !self.resolved_base_url().trim().is_empty()
                }
                _ => true,
            }
    }

    /// Returns the resolved provider model, falling back to the provider default when available.
    /// 返回解析后的供应商模型；若存在默认模型则在留空时自动回退。
    pub fn resolved_model(&self) -> String {
        let trimmed = self.model.trim();
        if trimmed.is_empty() {
            self.kind.default_model().to_string()
        } else {
            trimmed.to_string()
        }
    }

    /// Returns the resolved provider region or locale, falling back to the provider default when available.
    /// 返回解析后的供应商区域或地区；若存在默认值则在留空时自动回退。
    pub fn resolved_region(&self) -> String {
        let trimmed = self.region.trim();
        if trimmed.is_empty() {
            self.kind.default_region().to_string()
        } else {
            trimmed.to_string()
        }
    }
}
