use super::*;

/// One enabled capability target bound under a single AI capability vendor instance.
/// 绑定在单个 AI 能力供应商实例下的一条已启用能力目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiCapabilityTargetBinding {
    /// Stable semantic capability kind owned by the binding.
    /// 当前绑定拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable target alias exposed to the model beneath the vendor id.
    /// 在供应商标识下暴露给模型的稳定目标别名。
    pub target_alias: String,
    /// Provider request model id or provider-specific target value.
    /// Provider 请求模型标识或 Provider 专属目标值。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub model_id: String,
    /// Whether the current binding is enabled.
    /// 当前绑定是否启用。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
}

/// One explicit shared-vendor connection mode used by providers that can switch between local and official cloud transports.
/// 供可在本地与官方云端传输之间切换的供应商使用的显式共享连接模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiCapabilityVendorConnectionMode {
    /// Uses one self-hosted or local endpoint.
    /// 使用自托管或本地端点。
    Local,
    /// Uses the official vendor-managed cloud endpoint.
    /// 使用供应商官方托管的云端端点。
    Cloud,
}

impl AiCapabilityVendorConnectionMode {
    /// Ordered connection-mode choices shown in the shared vendor editor.
    /// 显示在共享供应商编辑器中的有序连接模式选项。
    pub const ALL: [Self; 2] = [Self::Local, Self::Cloud];

    /// Returns the stable user-facing label for the current connection mode.
    /// 返回当前连接模式的稳定面向用户标签。
    pub const fn label(self) -> &'static str {
        match self {
            Self::Local => "本地",
            Self::Cloud => "Ollama Cloud",
        }
    }
}

impl fmt::Display for AiCapabilityVendorConnectionMode {
    /// Renders the stable user-facing label used by pick lists.
    /// 渲染供下拉选择器使用的稳定面向用户标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

#[allow(dead_code)]
impl PersistedAiCapabilityTargetBinding {
    /// Builds one default enabled target binding for the provided capability and target pair.
    /// 为给定能力与目标对构建一条默认启用的目标绑定。
    pub fn new(
        capability_kind: AiToolKind,
        target_alias: impl Into<String>,
        model_id: impl Into<String>,
    ) -> Self {
        let target_alias = target_alias.into();
        let model_id = model_id.into();
        Self {
            capability_kind,
            target_alias: target_alias.trim().to_string(),
            model_id: if model_id.trim().is_empty() {
                target_alias.trim().to_string()
            } else {
                model_id.trim().to_string()
            },
            enabled: true,
        }
    }

    /// Returns the canonical binding key used by deduplication and explicit selection.
    /// 返回供去重与显式选择使用的规范绑定键。
    pub fn binding_key(&self) -> String {
        format!("{}::{}", self.capability_kind.key(), self.target_alias())
    }

    /// Returns the effective target alias exposed to the model.
    /// 返回暴露给模型的生效目标别名。
    pub fn target_alias(&self) -> &str {
        let trimmed = self.target_alias.trim();
        if trimmed.is_empty() {
            self.model_id()
        } else {
            trimmed
        }
    }

    /// Returns the provider request model identifier attached to the binding.
    /// 返回绑定携带的 Provider 请求模型标识。
    pub fn model_id(&self) -> &str {
        let trimmed = self.model_id.trim();
        if trimmed.is_empty() {
            self.target_alias.trim()
        } else {
            trimmed
        }
    }
}

/// One shared AI capability vendor instance configured once and reused across multiple semantic capabilities.
/// 单次配置并可在多个语义能力之间复用的一条共享 AI 能力供应商实例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersistedAiCapabilityVendorEntry {
    /// Stable vendor instance id used by settings, routing, and queue serialization.
    /// 设置、路由与队列序列化使用的稳定供应商实例标识。
    pub id: String,
    /// Stable vendor catalog identifier such as `minimax` or `tavily`.
    /// 稳定供应商目录标识，例如 `minimax` 或 `tavily`。
    pub vendor_id: String,
    /// User-facing vendor instance name.
    /// 面向用户展示的供应商实例名称。
    pub name: String,
    /// Whether the current vendor instance remains available for explicit selection.
    /// 当前供应商实例是否仍可供显式选择。
    #[serde(default = "default_true", skip_serializing_if = "is_true")]
    pub enabled: bool,
    /// Shared API key or `{env:NAME}` placeholder reused by all enabled targets beneath the vendor.
    /// 在该供应商下所有已启用目标之间复用的共享 API Key 或 `{env:NAME}` 占位符。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub api_key: String,
    /// Optional connection mode used by vendors that can switch between local and cloud backends.
    /// 可由支持本地与云端后端切换的供应商使用的可选连接模式。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_mode: Option<AiCapabilityVendorConnectionMode>,
    /// Shared vendor-level base URL or root endpoint override.
    /// 共享的供应商级 Base URL 或根端点覆盖值。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub base_url: String,
    /// Shared region or locale override reused by region-aware targets beneath the vendor.
    /// 在该供应商下支持区域的目标之间复用的共享区域或地区覆盖值。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub region: String,
    /// Enabled capability targets exposed by the current vendor instance.
    /// 当前供应商实例暴露的已启用能力目标集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<PersistedAiCapabilityTargetBinding>,
}

#[allow(dead_code)]
impl PersistedAiCapabilityVendorEntry {
    /// Builds one default vendor instance that can later enable one or more capability targets.
    /// 构建一条后续可启用一个或多个能力目标的默认供应商实例。
    pub fn new(vendor_id: impl Into<String>) -> Self {
        let vendor_id = normalize_ai_capability_vendor_id(vendor_id.into());
        let vendor_name = default_ai_capability_vendor_name(&vendor_id);
        Self {
            id: vendor_id.clone(),
            vendor_id,
            name: vendor_name,
            enabled: true,
            api_key: String::new(),
            connection_mode: None,
            base_url: String::new(),
            region: default_ai_capability_vendor_region(""),
            targets: Vec::new(),
        }
    }

    /// Returns whether the current vendor instance has at least one enabled target for the requested capability.
    /// 返回当前供应商实例是否至少为所请求能力启用了一条目标。
    pub fn has_enabled_target_for(&self, capability_kind: AiToolKind) -> bool {
        self.enabled
            && self
                .targets
                .iter()
                .any(|binding| binding.enabled && binding.capability_kind == capability_kind)
    }

    /// Returns the fully qualified `vendor/target` selectors enabled for one capability.
    /// 返回单个能力当前启用的完整 `供应商/目标` 选择器集合。
    pub fn enabled_target_strings_for(&self, capability_kind: AiToolKind) -> Vec<String> {
        self.targets
            .iter()
            .filter(|binding| binding.enabled && binding.capability_kind == capability_kind)
            .map(|binding| format!("{}/{}", self.vendor_id, binding.target_alias()))
            .collect()
    }

    /// Returns the fully qualified `vendor/target` selectors configured for one capability regardless of the current enabled flag.
    /// 返回单个能力当前已配置的完整 `供应商/目标` 选择器集合，不受当前启用标记影响。
    pub fn configured_target_strings_for(&self, capability_kind: AiToolKind) -> Vec<String> {
        self.targets
            .iter()
            .filter(|binding| binding.capability_kind == capability_kind)
            .map(|binding| format!("{}/{}", self.vendor_id, binding.target_alias()))
            .collect()
    }

    /// Returns the first enabled binding that matches the provided capability and target selector.
    /// 返回首条与给定能力及目标选择器匹配的已启用绑定。
    pub fn matching_binding(
        &self,
        capability_kind: AiToolKind,
        target: &str,
    ) -> Option<&PersistedAiCapabilityTargetBinding> {
        let normalized_target = target.trim().to_ascii_lowercase();
        self.targets.iter().find(|binding| {
            binding.enabled
                && binding.capability_kind == capability_kind
                && format!("{}/{}", self.vendor_id, binding.target_alias()).to_ascii_lowercase()
                    == normalized_target
        })
    }

    /// Returns the first configured binding that matches the provided capability and target selector regardless of the enabled flag.
    /// 返回首条与给定能力及目标选择器匹配的已配置绑定，不受启用标记影响。
    pub fn matching_configured_binding(
        &self,
        capability_kind: AiToolKind,
        target: &str,
    ) -> Option<&PersistedAiCapabilityTargetBinding> {
        let normalized_target = target.trim().to_ascii_lowercase();
        self.targets.iter().find(|binding| {
            binding.capability_kind == capability_kind
                && format!("{}/{}", self.vendor_id, binding.target_alias()).to_ascii_lowercase()
                    == normalized_target
        })
    }

    /// Resolves the shared API key for the current vendor instance, honoring `{env:NAME}` placeholders.
    /// 解析当前供应商实例的共享 API Key，并支持 `{env:NAME}` 环境变量占位符。
    pub fn resolved_api_key(&self) -> Result<Option<String>, String> {
        resolve_env_backed_secret(Some(self.api_key.as_str()), "AI 能力供应商 API Key")
    }

    /// Returns whether the current shared vendor exposes an explicit local/cloud connection-mode switch.
    /// 返回当前共享供应商是否暴露显式的本地/云端连接模式切换。
    pub fn supports_connection_mode(&self) -> bool {
        self.vendor_id.eq_ignore_ascii_case("ollama")
    }

    /// Returns the effective connection mode, inferring one sensible default for legacy vendor rows that predate the explicit selector.
    /// 返回生效连接模式，并为显式选择器落地前的旧供应商条目推断合理默认值。
    pub fn resolved_connection_mode(&self) -> AiCapabilityVendorConnectionMode {
        if !self.supports_connection_mode() {
            return AiCapabilityVendorConnectionMode::Local;
        }

        self.connection_mode.unwrap_or_else(|| {
            if self.api_key.trim().is_empty() {
                AiCapabilityVendorConnectionMode::Local
            } else {
                AiCapabilityVendorConnectionMode::Cloud
            }
        })
    }

    /// Returns the effective vendor-level base URL after applying vendor-specific connection-mode defaults.
    /// 应用供应商专属连接模式默认值后，返回生效的供应商级 Base URL。
    pub fn resolved_vendor_base_url(&self) -> String {
        let trimmed = self.base_url.trim();

        if self.vendor_id.eq_ignore_ascii_case("ollama") {
            return match self.resolved_connection_mode() {
                AiCapabilityVendorConnectionMode::Local => {
                    if trimmed.is_empty() {
                        String::from("http://127.0.0.1:11434")
                    } else {
                        trimmed.to_string()
                    }
                }
                AiCapabilityVendorConnectionMode::Cloud => String::from("https://ollama.com"),
            };
        }

        trimmed.to_string()
    }

    /// Returns the effective region value for the current vendor instance.
    /// 返回当前供应商实例的生效区域值。
    pub fn resolved_region(&self) -> String {
        self.region.trim().to_string()
    }

    /// Normalizes one vendor entry so it has a stable identifier, name, and deduplicated target list.
    /// 归一化单个供应商条目，使其拥有稳定标识、名称与去重后的目标列表。
    pub fn normalize(&mut self) -> bool {
        let original = self.clone();
        let normalized_vendor_id = normalize_ai_capability_vendor_id(self.vendor_id.clone());
        self.vendor_id = normalized_vendor_id.clone();
        self.id = normalized_vendor_id.clone();
        if self.name.trim().is_empty() {
            self.name = default_ai_capability_vendor_name(&normalized_vendor_id);
        }
        if self.region.trim().is_empty() {
            self.region = default_ai_capability_vendor_region(&normalized_vendor_id);
        }
        if normalized_vendor_id == "minimax" {
            self.base_url = normalize_minimax_vendor_base_url(&self.base_url);
        }

        let mut normalized_targets = Vec::new();
        for mut binding in self.targets.drain(..) {
            binding.target_alias = binding.target_alias().to_string();
            binding.model_id = binding.model_id().to_string();
            if ai_capability_provider_catalog::ai_capability_provider_definition_by_target(
                &normalized_vendor_id,
                binding.capability_kind,
                binding.target_alias(),
            )
            .is_none()
            {
                continue;
            }
            if normalized_targets
                .iter()
                .any(|existing: &PersistedAiCapabilityTargetBinding| {
                    existing.capability_kind == binding.capability_kind
                        && existing
                            .target_alias()
                            .eq_ignore_ascii_case(binding.target_alias())
                })
            {
                continue;
            }
            normalized_targets.push(binding);
        }
        self.targets = normalized_targets;
        self != &original
    }
}
