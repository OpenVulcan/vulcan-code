//! Resource-backed AI capability provider catalog used by the standalone capability layer.
//! 由资源文件驱动的 AI 能力提供方目录，供独立能力层使用。

use crate::provider_config::AiToolKind;
#[cfg(test)]
use crate::provider_config::{UnderstandImageProviderKind, WebSearchProviderKind};
use crate::runtime_assets;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

/// API-key requirement level declared by one AI capability provider definition.
/// 单个 AI 能力提供方定义声明的 API Key 要求级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiCapabilityProviderApiKeyRequirement {
    /// No API key is needed.
    /// 无需 API Key。
    None,
    /// The API key is optional.
    /// API Key 为可选项。
    Optional,
    /// A non-empty API key is required.
    /// 必须提供非空 API Key。
    Required,
}

#[allow(dead_code)]
impl AiCapabilityProviderApiKeyRequirement {
    /// Returns whether the definition requires a non-empty API key.
    /// 返回当前定义是否要求非空 API Key。
    pub const fn requires_api_key(self) -> bool {
        matches!(self, Self::Required)
    }

    /// Returns whether the definition accepts an optional API key.
    /// 返回当前定义是否接受可选 API Key。
    pub const fn api_key_optional(self) -> bool {
        matches!(self, Self::Optional)
    }
}

/// Generic resource-backed AI capability provider definition shared by all standalone host capabilities.
/// 供全部独立宿主能力共享的通用资源化 AI 能力提供方定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCapabilityProviderTargetDefinition {
    /// Stable target alias exposed below one configured provider instance.
    /// 暴露在单个已配置提供方实例下方的稳定目标别名。
    pub target_alias: String,
    /// User-facing target display name.
    /// 面向用户展示的目标名称。
    pub name: String,
    /// Whether the target exposes one editable request-model field.
    /// 当前目标是否暴露可编辑请求模型字段。
    #[serde(default)]
    pub supports_model_field: bool,
    /// Label shown above the model field.
    /// 显示在模型字段上方的标签。
    #[serde(default)]
    pub model_label: String,
    /// Placeholder shown inside the model field.
    /// 显示在模型字段内的占位提示。
    #[serde(default)]
    pub model_placeholder: String,
    /// Default provider request model suggested by the target definition.
    /// 当前目标定义建议的默认 Provider 请求模型。
    #[serde(default)]
    pub default_model: String,
    /// Optional finite model choices exposed as one fixed sub-item list instead of a free-form text field.
    /// 作为固定子项列表暴露、而不是自由输入框的可选有限模型集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_options: Vec<String>,
}

/// Generic resource-backed AI capability provider definition shared by all standalone host capabilities.
/// 供全部独立宿主能力共享的通用资源化 AI 能力提供方定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCapabilityProviderDefinition {
    /// Stable provider identifier shared with persisted vendor instances.
    /// 与持久化供应商实例共享的稳定提供方标识。
    pub id: String,
    /// User-facing provider display name.
    /// 面向用户展示的提供方名称。
    pub name: String,
    /// Whether the current provider definition should remain available for new configuration and runtime routing.
    /// 当前提供方定义是否仍应对新的配置与运行时路由保持可用。
    #[serde(default = "default_definition_enabled")]
    pub enabled: bool,
    /// Stable semantic capability kind owned by this definition.
    /// 当前定义拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable target alias exposed below one configured provider instance.
    /// 暴露在单个已配置提供方实例下方的稳定目标别名。
    pub target_alias: String,
    /// Stable grouping index used by the settings add-provider button matrix.
    /// 设置页新增提供方按钮矩阵使用的稳定分组索引。
    #[serde(default)]
    pub add_button_group: u16,
    /// Stable ordering index used inside one add-provider button group.
    /// 单个新增提供方按钮分组内部使用的稳定排序索引。
    #[serde(default)]
    pub add_button_order: u16,
    /// Default endpoint used when the user leaves the base URL empty.
    /// 当用户留空基址时使用的默认端点。
    pub default_base_url: String,
    /// API-key requirement level declared by the provider.
    /// 提供方声明的 API Key 要求级别。
    pub api_key_requirement: AiCapabilityProviderApiKeyRequirement,
    /// Label shown above the API key field.
    /// 显示在 API Key 字段上方的标签。
    pub api_key_label: String,
    /// Placeholder shown inside the API key field.
    /// 显示在 API Key 字段内的占位提示。
    pub api_key_placeholder: String,
    /// Whether the provider exposes one editable request-model field.
    /// 当前提供方是否暴露可编辑的请求模型字段。
    #[serde(default)]
    pub supports_model_field: bool,
    /// Label shown above the model field.
    /// 显示在模型字段上方的标签。
    #[serde(default)]
    pub model_label: String,
    /// Placeholder shown inside the model field.
    /// 显示在模型字段内的占位提示。
    #[serde(default)]
    pub model_placeholder: String,
    /// Default provider request model suggested by the definition.
    /// 当前定义建议的默认 Provider 请求模型。
    #[serde(default)]
    pub default_model: String,
    /// Optional finite model choices exposed as one fixed sub-item list instead of a free-form text field.
    /// 作为固定子项列表暴露、而不是自由输入框的可选有限模型集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub model_options: Vec<String>,
    /// Optional vendor-level target collection used when one provider exposes multiple fixed targets under shared credentials.
    /// 当单个提供方在共享凭据下暴露多个固定目标时使用的可选供应商级目标集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targets: Vec<AiCapabilityProviderTargetDefinition>,
    /// Whether the provider exposes one region-like field.
    /// 当前提供方是否暴露区域类字段。
    #[serde(default)]
    pub supports_region_field: bool,
    /// Label shown above the region field.
    /// 显示在区域字段上方的标签。
    #[serde(default)]
    pub region_label: String,
    /// Placeholder shown inside the region field.
    /// 显示在区域字段内的占位提示。
    #[serde(default)]
    pub region_placeholder: String,
    /// Default region or locale suggested by the provider.
    /// 提供方建议的默认区域或地区值。
    #[serde(default)]
    pub default_region: String,
    /// Optional fixed region choices rendered as one dropdown when the provider only supports a known finite set.
    /// 当提供方只支持已知有限集合时，渲染为下拉框的可选固定区域列表。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub region_options: Vec<String>,
    /// Optional provider-specific hard ceiling applied to result count for search-like capabilities.
    /// 对搜索类能力施加的可选提供方专属结果数硬上限。
    #[serde(default)]
    pub max_results_limit: usize,
    /// Optional provider-specific note shown in the settings page.
    /// 显示在设置页中的可选提供方专属说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_note: Option<String>,
}

/// Flattened capability target entry derived from one shared vendor definition.
/// 从单个共享供应商定义推导出的扁平化能力目标条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCapabilityProviderCatalogTarget {
    /// Stable semantic capability kind owned by the target.
    /// 当前目标拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable vendor identifier owning the target.
    /// 拥有当前目标的稳定供应商标识。
    pub vendor_id: String,
    /// Stable target alias exposed below the vendor identifier.
    /// 暴露在供应商标识下方的稳定目标别名。
    pub target_alias: String,
    /// User-facing target display name.
    /// 面向用户展示的目标名称。
    pub name: String,
    /// Whether the current target exposes one editable request-model field.
    /// 当前目标是否暴露可编辑请求模型字段。
    pub supports_model_field: bool,
    /// Label shown above the model field.
    /// 显示在模型字段上方的标签。
    pub model_label: String,
    /// Placeholder shown inside the model field.
    /// 显示在模型字段内的占位提示。
    pub model_placeholder: String,
    /// Default provider request model suggested by the target definition.
    /// 当前目标定义建议的默认 Provider 请求模型。
    pub default_model: String,
    /// Optional finite model choices exposed as one fixed sub-item list instead of a free-form text field.
    /// 作为固定子项列表暴露、而不是自由输入框的可选有限模型集合。
    pub model_options: Vec<String>,
}

impl AiCapabilityProviderDefinition {
    /// Returns whether the current vendor definition uses nested target entries instead of one top-level target.
    /// 返回当前供应商定义是否使用嵌套目标条目，而不是单个顶层目标。
    fn uses_nested_targets(&self) -> bool {
        !self.targets.is_empty()
    }

    /// Flattens the current vendor definition into one or more target entries sharing the same credentials and endpoint metadata.
    /// 将当前供应商定义扁平化为一个或多个共享相同凭据与端点元数据的目标条目。
    fn flattened_targets(&self) -> Vec<AiCapabilityProviderCatalogTarget> {
        if self.uses_nested_targets() {
            return self
                .targets
                .iter()
                .map(|target| AiCapabilityProviderCatalogTarget {
                    capability_kind: self.capability_kind,
                    vendor_id: self.id.clone(),
                    target_alias: target.target_alias.clone(),
                    name: target.name.clone(),
                    supports_model_field: target.supports_model_field,
                    model_label: if target.model_label.trim().is_empty() {
                        self.model_label.clone()
                    } else {
                        target.model_label.clone()
                    },
                    model_placeholder: if target.model_placeholder.trim().is_empty() {
                        self.model_placeholder.clone()
                    } else {
                        target.model_placeholder.clone()
                    },
                    default_model: if target.default_model.trim().is_empty() {
                        target.target_alias.clone()
                    } else {
                        target.default_model.clone()
                    },
                    model_options: target.model_options.clone(),
                })
                .collect();
        }

        vec![AiCapabilityProviderCatalogTarget {
            capability_kind: self.capability_kind,
            vendor_id: self.id.clone(),
            target_alias: self.target_alias.clone(),
            name: self.name.clone(),
            supports_model_field: self.supports_model_field,
            model_label: self.model_label.clone(),
            model_placeholder: self.model_placeholder.clone(),
            default_model: if self.default_model.trim().is_empty() {
                self.target_alias.clone()
            } else {
                self.default_model.clone()
            },
            model_options: self.model_options.clone(),
        }]
    }
}

/// Resource-backed third-party web-search provider definition used by the standalone capability layer.
/// 独立能力层使用的资源化第三方网页搜索提供方定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSearchProviderDefinition {
    /// Stable provider identifier shared with persisted provider kinds.
    /// 与持久化供应商类型共享的稳定提供方标识。
    pub id: String,
    /// User-facing provider display name.
    /// 面向用户展示的提供方名称。
    pub name: String,
    /// Whether the current provider definition should remain available for new configuration and runtime routing.
    /// 当前提供方定义是否仍应对新的配置与运行时路由保持可用。
    #[serde(default = "default_definition_enabled")]
    pub enabled: bool,
    /// Stable semantic capability kind owned by this definition.
    /// 当前定义拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable default target alias exposed below one configured provider instance.
    /// 暴露在单个配置实例下方的稳定默认目标别名。
    pub target_alias: String,
    /// Stable grouping index used by the settings add-provider button matrix.
    /// 设置页新增提供方按钮矩阵使用的稳定分组索引。
    #[serde(default)]
    pub add_button_group: u16,
    /// Stable ordering index used inside one add-provider button group.
    /// 单个新增提供方按钮分组内部使用的稳定排序索引。
    #[serde(default)]
    pub add_button_order: u16,
    /// Default endpoint used when the user leaves the base URL empty.
    /// 当用户留空基址时使用的默认端点。
    pub default_base_url: String,
    /// API-key requirement level declared by the provider.
    /// 提供方声明的 API Key 要求级别。
    pub api_key_requirement: AiCapabilityProviderApiKeyRequirement,
    /// Label shown above the API key field.
    /// 显示在 API Key 字段上方的标签。
    pub api_key_label: String,
    /// Placeholder shown inside the API key field.
    /// 显示在 API Key 字段内的占位提示。
    pub api_key_placeholder: String,
    /// Whether the provider exposes one model field.
    /// 当前提供方是否暴露模型字段。
    #[serde(default)]
    pub supports_model_field: bool,
    /// Label shown above the model field.
    /// 显示在模型字段上方的标签。
    #[serde(default)]
    pub model_label: String,
    /// Placeholder shown inside the model field.
    /// 显示在模型字段内的占位提示。
    #[serde(default)]
    pub model_placeholder: String,
    /// Default model id suggested by the provider.
    /// 提供方建议的默认模型标识。
    #[serde(default)]
    pub default_model: String,
    /// Whether the provider exposes one region-like field.
    /// 当前提供方是否暴露区域类字段。
    #[serde(default)]
    pub supports_region_field: bool,
    /// Label shown above the region field.
    /// 显示在区域字段上方的标签。
    #[serde(default)]
    pub region_label: String,
    /// Placeholder shown inside the region field.
    /// 显示在区域字段内的占位提示。
    #[serde(default)]
    pub region_placeholder: String,
    /// Default region or locale suggested by the provider.
    /// 提供方建议的默认区域或地区值。
    #[serde(default)]
    pub default_region: String,
    /// Provider-specific hard ceiling applied to the result count.
    /// 对结果数量施加的提供方专属硬上限。
    pub max_results_limit: usize,
    /// Optional provider-specific note shown in the settings page.
    /// 显示在设置页中的可选提供方专属说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_note: Option<String>,
}

/// Resource-backed image-understanding provider definition used by the standalone capability layer.
/// 独立能力层使用的资源化图片理解提供方定义。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnderstandImageProviderDefinition {
    /// Stable provider identifier shared with persisted provider kinds.
    /// 与持久化提供方类型共享的稳定提供方标识。
    pub id: String,
    /// User-facing provider display name.
    /// 面向用户展示的提供方名称。
    pub name: String,
    /// Whether the current provider definition should remain available for new configuration and runtime routing.
    /// 当前提供方定义是否仍应对新的配置与运行时路由保持可用。
    #[serde(default = "default_definition_enabled")]
    pub enabled: bool,
    /// Stable semantic capability kind owned by this definition.
    /// 当前定义拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable default target alias exposed below one configured provider instance.
    /// 暴露在单个配置实例下方的稳定默认目标别名。
    pub target_alias: String,
    /// Stable grouping index used by the settings add-provider button matrix.
    /// 设置页新增提供方按钮矩阵使用的稳定分组索引。
    #[serde(default)]
    pub add_button_group: u16,
    /// Stable ordering index used inside one add-provider button group.
    /// 单个新增提供方按钮分组内部使用的稳定排序索引。
    #[serde(default)]
    pub add_button_order: u16,
    /// Default endpoint used when the user leaves the base URL empty.
    /// 当用户留空基址时使用的默认端点。
    pub default_base_url: String,
    /// API-key requirement level declared by the provider.
    /// 提供方声明的 API Key 要求级别。
    pub api_key_requirement: AiCapabilityProviderApiKeyRequirement,
    /// Label shown above the API key field.
    /// 显示在 API Key 字段上方的标签。
    pub api_key_label: String,
    /// Placeholder shown inside the API key field.
    /// 显示在 API Key 字段内的占位提示。
    pub api_key_placeholder: String,
    /// Whether the provider exposes one region-like field.
    /// 当前提供方是否暴露区域类字段。
    #[serde(default)]
    pub supports_region_field: bool,
    /// Label shown above the region field.
    /// 显示在区域字段上方的标签。
    #[serde(default)]
    pub region_label: String,
    /// Placeholder shown inside the region field.
    /// 显示在区域字段内的占位提示。
    #[serde(default)]
    pub region_placeholder: String,
    /// Default region or locale suggested by the provider.
    /// 提供方建议的默认区域或地区值。
    #[serde(default)]
    pub default_region: String,
    /// Optional provider-specific note shown in the settings page.
    /// 显示在设置页中的可选提供方专属说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub configuration_note: Option<String>,
}

/// Returns all supported web-search provider definitions loaded from independent resource files.
/// 返回从独立资源文件加载出的全部受支持网页搜索提供方定义。
pub fn supported_web_search_provider_definitions() -> &'static [WebSearchProviderDefinition] {
    static DEFINITIONS: OnceLock<Vec<WebSearchProviderDefinition>> = OnceLock::new();

    DEFINITIONS.get_or_init(load_web_search_provider_definitions)
}

/// Resolves one web-search provider definition by its stable identifier.
/// 根据稳定标识解析单个网页搜索提供方定义。
pub fn web_search_provider_definition_by_id(
    provider_id: &str,
) -> Option<&'static WebSearchProviderDefinition> {
    supported_web_search_provider_definitions()
        .iter()
        .find(|definition| definition.id == provider_id)
}

/// Returns whether one web-search provider definition is still enabled for new configuration and runtime routing.
/// 返回单个网页搜索提供方定义是否仍对新的配置与运行时路由保持启用。
pub fn web_search_provider_enabled(provider_id: &str) -> bool {
    web_search_provider_definition_by_id(provider_id)
        .map(|definition| definition.enabled)
        .unwrap_or(false)
}

/// Returns all supported image-understanding provider definitions loaded from independent resource files.
/// 返回从独立资源文件加载出的全部受支持图片理解提供方定义。
pub fn supported_understand_image_provider_definitions()
-> &'static [UnderstandImageProviderDefinition] {
    static DEFINITIONS: OnceLock<Vec<UnderstandImageProviderDefinition>> = OnceLock::new();

    DEFINITIONS.get_or_init(load_understand_image_provider_definitions)
}

/// Resolves one image-understanding provider definition by its stable identifier.
/// 根据稳定标识解析单个图片理解提供方定义。
pub fn understand_image_provider_definition_by_id(
    provider_id: &str,
) -> Option<&'static UnderstandImageProviderDefinition> {
    supported_understand_image_provider_definitions()
        .iter()
        .find(|definition| definition.id == provider_id)
}

/// Returns whether one image-understanding provider definition is still enabled for new configuration and runtime routing.
/// 返回单个图片理解提供方定义是否仍对新的配置与运行时路由保持启用。
pub fn understand_image_provider_enabled(provider_id: &str) -> bool {
    understand_image_provider_definition_by_id(provider_id)
        .map(|definition| definition.enabled)
        .unwrap_or(false)
}

/// Returns all standalone AI capability provider definitions loaded from the shared resource directory.
/// 返回从共享资源目录加载出的全部独立 AI 能力提供方定义。
pub fn supported_ai_capability_provider_definitions() -> &'static [AiCapabilityProviderDefinition] {
    static DEFINITIONS: OnceLock<Vec<AiCapabilityProviderDefinition>> = OnceLock::new();

    DEFINITIONS.get_or_init(load_ai_capability_provider_definitions)
}

/// Returns all capability definitions that belong to one shared vendor identifier.
/// 返回归属于单个共享供应商标识的全部能力定义。
pub fn ai_capability_provider_definitions_for_vendor(
    vendor_id: &str,
) -> Vec<&'static AiCapabilityProviderDefinition> {
    supported_ai_capability_provider_definitions()
        .iter()
        .filter(|definition| definition.enabled && definition.id.eq_ignore_ascii_case(vendor_id))
        .collect()
}

/// Returns all flattened target entries that belong to one shared vendor identifier.
/// 返回归属于单个共享供应商标识的全部扁平化目标条目。
pub fn ai_capability_provider_targets_for_vendor(
    vendor_id: &str,
) -> Vec<AiCapabilityProviderCatalogTarget> {
    ai_capability_provider_definitions_for_vendor(vendor_id)
        .into_iter()
        .flat_map(|definition| definition.flattened_targets())
        .collect()
}

/// Resolves one capability definition by vendor id, semantic capability, and target alias.
/// 根据供应商标识、语义能力与目标别名解析单条能力定义。
pub fn ai_capability_provider_definition_by_target(
    vendor_id: &str,
    capability_kind: AiToolKind,
    target_alias: &str,
) -> Option<&'static AiCapabilityProviderDefinition> {
    supported_ai_capability_provider_definitions()
        .iter()
        .find(|definition| {
            definition.enabled
                && definition.id.eq_ignore_ascii_case(vendor_id)
                && definition
                    .flattened_targets()
                    .into_iter()
                    .any(|target| {
                        target.capability_kind == capability_kind
                            && target.target_alias.eq_ignore_ascii_case(target_alias)
                    })
        })
}

/// Resolves one flattened target entry by vendor id, semantic capability, and target alias.
/// 根据供应商标识、语义能力与目标别名解析单条扁平化目标条目。
pub fn ai_capability_provider_target_by_target(
    vendor_id: &str,
    capability_kind: AiToolKind,
    target_alias: &str,
) -> Option<AiCapabilityProviderCatalogTarget> {
    supported_ai_capability_provider_definitions()
        .iter()
        .filter(|definition| definition.enabled && definition.id.eq_ignore_ascii_case(vendor_id))
        .flat_map(|definition| definition.flattened_targets())
        .find(|target| {
            target.capability_kind == capability_kind
                && target.target_alias.eq_ignore_ascii_case(target_alias)
        })
}

/// Returns the ordered unique vendor identifiers currently exposed by the capability resource directory.
/// 返回能力资源目录当前暴露的有序唯一供应商标识集合。
pub fn supported_ai_capability_vendor_ids() -> Vec<String> {
    let mut ids = Vec::new();

    for definition in supported_ai_capability_provider_definitions() {
        if !definition.enabled {
            continue;
        }
        if ids
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&definition.id))
        {
            continue;
        }
        ids.push(definition.id.clone());
    }

    ids
}

/// Returns the deduplicated fixed region choices exposed by one shared vendor identifier.
/// 返回单个共享供应商标识暴露的去重固定区域选项列表。
pub fn ai_capability_vendor_region_options(vendor_id: &str) -> Vec<String> {
    let mut options = Vec::new();

    for definition in ai_capability_provider_definitions_for_vendor(vendor_id) {
        for option in &definition.region_options {
            let normalized = option.trim();
            if normalized.is_empty() {
                continue;
            }
            if options
                .iter()
                .any(|existing: &String| existing.eq_ignore_ascii_case(normalized))
            {
                continue;
            }
            options.push(normalized.to_string());
        }
    }

    options
}

/// Returns grouped web-search provider kinds used by the settings add-provider button matrix.
/// 返回设置页新增提供方按钮矩阵使用的分组网页搜索提供方类型集合。
#[cfg(test)]
pub fn grouped_web_search_provider_kinds() -> Vec<Vec<WebSearchProviderKind>> {
    let mut rows = Vec::new();
    let mut current_group = None::<u16>;

    for definition in supported_web_search_provider_definitions() {
        if !definition.enabled {
            continue;
        }
        let Some(kind) = WebSearchProviderKind::from_key(&definition.id) else {
            continue;
        };

        if current_group != Some(definition.add_button_group) {
            rows.push(Vec::new());
            current_group = Some(definition.add_button_group);
        }

        if let Some(last_row) = rows.last_mut() {
            last_row.push(kind);
        }
    }

    rows
}

/// Returns grouped image-understanding provider kinds used by the settings add-provider button matrix.
/// 返回设置页新增提供方按钮矩阵使用的分组图片理解提供方类型集合。
#[cfg(test)]
pub fn grouped_understand_image_provider_kinds() -> Vec<Vec<UnderstandImageProviderKind>> {
    let mut rows = Vec::new();
    let mut current_group = None::<u16>;

    for definition in supported_understand_image_provider_definitions() {
        let Some(kind) = UnderstandImageProviderKind::ALL
            .into_iter()
            .find(|kind| kind.key() == definition.id)
        else {
            continue;
        };

        if current_group != Some(definition.add_button_group) {
            rows.push(Vec::new());
            current_group = Some(definition.add_button_group);
        }

        if let Some(last_row) = rows.last_mut() {
            last_row.push(kind);
        }
    }

    rows
}

/// Loads and sorts all resource-backed web-search provider definitions from the AI capability provider directory.
/// 从 AI 能力提供方目录加载并排序全部资源化网页搜索提供方定义。
fn load_web_search_provider_definitions() -> Vec<WebSearchProviderDefinition> {
    let directory = ai_capability_provider_catalog_directory();
    let mut definitions = fs::read_dir(&directory)
        .unwrap_or_else(|error| {
            panic!(
                "AI capability provider catalog directory must exist at `{}`: {error}",
                directory.display()
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "AI capability provider resource `{}` must stay readable: {error}",
                    path.display()
                )
            });
            let raw_value = serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|error| {
                panic!(
                    "AI capability provider resource `{}` must stay valid JSON: {error}",
                    path.display()
                )
            });
            let capability_kind = raw_value
                .get("capabilityKind")
                .and_then(serde_json::Value::as_str);
            if capability_kind != Some("web_search") {
                return None;
            }

            Some(
                serde_json::from_value::<WebSearchProviderDefinition>(raw_value).unwrap_or_else(|error| {
                    panic!(
                        "AI capability provider resource `{}` must match the web-search definition shape: {error}",
                        path.display()
                    )
                }),
            )
        })
        .collect::<Vec<_>>();

    definitions.sort_by(|left, right| {
        (
            left.add_button_group,
            left.add_button_order,
            left.name.clone(),
            left.id.clone(),
        )
            .cmp(&(
                right.add_button_group,
                right.add_button_order,
                right.name.clone(),
                right.id.clone(),
            ))
    });
    definitions
}

/// Loads and sorts all resource-backed image-understanding provider definitions from the AI capability provider directory.
/// 从 AI 能力提供方目录加载并排序全部资源化图片理解提供方定义。
fn load_understand_image_provider_definitions() -> Vec<UnderstandImageProviderDefinition> {
    let directory = ai_capability_provider_catalog_directory();
    let mut definitions = fs::read_dir(&directory)
        .unwrap_or_else(|error| {
            panic!(
                "AI capability provider catalog directory must exist at `{}`: {error}",
                directory.display()
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .filter_map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "AI capability provider resource `{}` must stay readable: {error}",
                    path.display()
                )
            });
            let raw_value = serde_json::from_str::<serde_json::Value>(&content).unwrap_or_else(|error| {
                panic!(
                    "AI capability provider resource `{}` must stay valid JSON: {error}",
                    path.display()
                )
            });
            let capability_kind = raw_value
                .get("capabilityKind")
                .and_then(serde_json::Value::as_str);
            if capability_kind != Some("understand_image") {
                return None;
            }

            Some(
                serde_json::from_value::<UnderstandImageProviderDefinition>(raw_value).unwrap_or_else(|error| {
                    panic!(
                        "AI capability provider resource `{}` must match the understand-image definition shape: {error}",
                        path.display()
                    )
                }),
            )
        })
        .collect::<Vec<_>>();

    definitions.sort_by(|left, right| {
        (
            left.add_button_group,
            left.add_button_order,
            left.name.clone(),
            left.id.clone(),
        )
            .cmp(&(
                right.add_button_group,
                right.add_button_order,
                right.name.clone(),
                right.id.clone(),
            ))
    });
    definitions
}

/// Loads and sorts all generic AI capability provider definitions from the shared resource directory.
/// 从共享资源目录加载并排序全部通用 AI 能力提供方定义。
fn load_ai_capability_provider_definitions() -> Vec<AiCapabilityProviderDefinition> {
    let directory = ai_capability_provider_catalog_directory();
    let mut definitions = fs::read_dir(&directory)
        .unwrap_or_else(|error| {
            panic!(
                "AI capability provider catalog directory must exist at `{}`: {error}",
                directory.display()
            )
        })
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .map(|entry| {
            let path = entry.path();
            let content = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "AI capability provider resource `{}` must stay readable: {error}",
                    path.display()
                )
            });
            serde_json::from_str::<AiCapabilityProviderDefinition>(&content).unwrap_or_else(
                |error| {
                    panic!(
                        "AI capability provider resource `{}` must match the generic definition shape: {error}",
                        path.display()
                    )
                },
            )
        })
        .collect::<Vec<_>>();

    definitions.sort_by(|left, right| {
        (
            left.add_button_group,
            left.add_button_order,
            left.id.clone(),
            left.capability_kind.key(),
            left.target_alias.clone(),
        )
            .cmp(&(
                right.add_button_group,
                right.add_button_order,
                right.id.clone(),
                right.capability_kind.key(),
                right.target_alias.clone(),
            ))
    });
    definitions
}

/// Returns the directory path containing the standalone AI capability provider catalog files.
/// 返回包含独立 AI 能力提供方目录文件的目录路径。
fn ai_capability_provider_catalog_directory() -> PathBuf {
    runtime_assets::resolve_ai_capability_provider_catalog_directory()
}

/// Returns the default enabled state used by AI capability provider resource definitions.
/// 返回 AI 能力提供方资源定义使用的默认启用状态。
const fn default_definition_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{
        ai_capability_provider_target_by_target, ai_capability_provider_targets_for_vendor,
        ai_capability_vendor_region_options, grouped_understand_image_provider_kinds,
        grouped_web_search_provider_kinds, supported_understand_image_provider_definitions,
        supported_web_search_provider_definitions, understand_image_provider_definition_by_id,
        web_search_provider_definition_by_id,
    };
    use crate::provider_config::{AiToolKind, UnderstandImageProviderKind, WebSearchProviderKind};

    /// Ensures every persisted web-search provider kind stays backed by one independent resource definition.
    /// 确保持久化网页搜索提供方类型始终都有一个独立资源定义作为后盾。
    #[test]
    fn every_web_search_provider_kind_has_one_resource_definition() {
        for kind in WebSearchProviderKind::ALL {
            let definition =
                web_search_provider_definition_by_id(kind.key()).unwrap_or_else(|| {
                    panic!(
                        "missing web search provider definition for `{}`",
                        kind.key()
                    )
                });
            assert_eq!(definition.capability_kind, AiToolKind::WebSearch);
            assert!(!definition.name.trim().is_empty());
            assert!(!definition.target_alias.trim().is_empty());
            assert!(!definition.default_base_url.trim().is_empty());
        }
    }

    /// Ensures the grouped add-provider rows stay aligned with the full supported provider list.
    /// 确保新增提供方按钮分组始终与完整受支持提供方列表保持对齐。
    #[test]
    fn grouped_web_search_provider_rows_cover_all_supported_kinds() {
        let grouped = grouped_web_search_provider_kinds();
        let flattened = grouped.into_iter().flatten().collect::<Vec<_>>();
        let expected = supported_web_search_provider_definitions()
            .iter()
            .filter(|definition| definition.enabled)
            .filter_map(|definition| WebSearchProviderKind::from_key(&definition.id))
            .collect::<Vec<_>>();

        assert_eq!(flattened, expected);
    }

    /// Ensures every persisted image-understanding provider kind stays backed by one independent resource definition.
    /// 确保持久化图片理解提供方类型始终都有一个独立资源定义作为后盾。
    #[test]
    fn every_understand_image_provider_kind_has_one_resource_definition() {
        for kind in UnderstandImageProviderKind::ALL {
            let definition =
                understand_image_provider_definition_by_id(kind.key()).unwrap_or_else(|| {
                    panic!(
                        "missing understand-image provider definition for `{}`",
                        kind.key()
                    )
                });
            assert_eq!(definition.capability_kind, AiToolKind::UnderstandImage);
            assert!(!definition.name.trim().is_empty());
            assert!(!definition.target_alias.trim().is_empty());
            assert!(!definition.default_base_url.trim().is_empty());
        }
    }

    /// Ensures the grouped image-understanding add-provider rows stay aligned with the full supported provider list.
    /// 确保图片理解新增提供方按钮分组始终与完整受支持提供方列表保持对齐。
    #[test]
    fn grouped_understand_image_provider_rows_cover_all_supported_kinds() {
        let grouped = grouped_understand_image_provider_kinds();
        let flattened = grouped.into_iter().flatten().collect::<Vec<_>>();
        let expected = supported_understand_image_provider_definitions()
            .iter()
            .filter_map(|definition| {
                UnderstandImageProviderKind::ALL
                    .into_iter()
                    .find(|kind| kind.key() == definition.id)
            })
            .collect::<Vec<_>>();

        assert_eq!(flattened, expected);
    }

    /// Ensures vendor-level fixed region choices can be derived from the shared capability catalog when one provider exposes a finite region set.
    /// 确保当某个供应商暴露有限区域集合时，可以从共享能力目录中推导出供应商级固定区域选项。
    #[test]
    fn minimax_vendor_region_options_are_exposed() {
        let options = ai_capability_vendor_region_options("minimax");

        assert_eq!(options, vec![String::from("global"), String::from("cn")]);
    }

    /// Ensures one vendor-level Kimi definition flattens into multiple independently selectable search targets.
    /// 确保单个供应商级 Kimi 定义会被扁平化成多个可独立勾选的搜索目标。
    #[test]
    fn kimi_vendor_targets_are_flattened() {
        let targets = ai_capability_provider_targets_for_vendor("kimi");

        assert!(targets.iter().any(|target| {
            target.capability_kind == AiToolKind::WebSearch
                && target.target_alias == "kimi-k2.5"
                && target.name == "kimi-k2.5"
        }));
        assert!(targets.iter().any(|target| {
            target.capability_kind == AiToolKind::WebSearch
                && target.target_alias == "kimi-k2-thinking"
                && target.name == "kimi-k2-thinking"
        }));
        let thinking_turbo = ai_capability_provider_target_by_target(
            "kimi",
            AiToolKind::WebSearch,
            "kimi-k2-thinking-turbo",
        )
        .expect("kimi target should stay resolvable");
        assert_eq!(thinking_turbo.default_model, "kimi-k2-thinking-turbo");
    }

    /// Ensures disabled provider definitions stay hidden from new configuration while still remaining queryable for compatibility cleanup.
    /// 确保已禁用的提供方定义会对新配置保持隐藏，同时仍可被查询以支持兼容清理。
    #[test]
    fn duckduckgo_definition_is_disabled_for_new_configuration() {
        let definition = web_search_provider_definition_by_id("duckduckgo")
            .expect("duckduckgo definition must exist");

        assert!(!definition.enabled);
        assert!(!super::web_search_provider_enabled("duckduckgo"));
    }
}
