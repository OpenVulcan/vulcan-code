use super::*;

/// One vendor-target choice exposed beneath a shared AI capability vendor instance.
/// 暴露在共享 AI 能力供应商实例下方的一条供应商目标选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiCapabilityVendorTargetChoice {
    /// Stable semantic capability kind owned by the current target.
    /// 当前目标拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Stable target alias exposed below the vendor id.
    /// 暴露在供应商标识下方的稳定目标别名。
    pub target_alias: String,
    /// Default provider request model attached to the target.
    /// 绑定到当前目标的默认 Provider 请求模型。
    pub default_model_id: String,
    /// User-facing compact label rendered beside the target checkbox.
    /// 渲染在目标勾选项旁的人类可读紧凑标签。
    pub label: String,
    /// Whether the current target exposes one editable model field.
    /// 当前目标是否暴露可编辑模型字段。
    pub supports_model_override: bool,
    /// Optional finite request-model options shown as fixed child items when the target should not use free-form model text input.
    /// 当目标不应使用自由输入模型文本框时，显示为固定子项的可选请求模型集合。
    pub model_options: Vec<String>,
}

/// One resolved settings-facing route entry rendered for an enabled semantic AI capability target.
/// 为某条已启用语义 AI 能力目标渲染使用的已解析设置路由条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAiCapabilityRouteTarget {
    /// Stable semantic capability kind owned by the route entry.
    /// 当前路由条目拥有的稳定语义能力类型。
    pub capability_kind: AiToolKind,
    /// Fully qualified `vendor/target` selector exposed to the model and runtime.
    /// 暴露给模型与运行时的完整 `供应商/目标` 选择器。
    pub target_selector: String,
    /// Stable vendor identifier owning the target.
    /// 拥有当前目标的稳定供应商标识。
    pub vendor_id: String,
    /// User-facing vendor display name.
    /// 面向用户展示的供应商名称。
    pub vendor_name: String,
    /// Stable target alias exposed beneath the vendor identifier.
    /// 暴露在供应商标识下方的稳定目标别名。
    pub target_alias: String,
    /// User-facing target display name resolved from the capability provider catalog.
    /// 从能力提供方目录解析出的面向用户目标显示名。
    pub target_name: String,
    /// Provider request model or provider-specific request value attached to the target.
    /// 绑定到当前目标的 Provider 请求模型或 Provider 专属请求值。
    pub model_id: String,
    /// Whether the current configured target is effectively enabled for routing.
    /// 当前已配置目标是否在路由中处于有效启用状态。
    pub enabled: bool,
}

#[allow(dead_code)]
impl ResolvedAiCapabilityRouteTarget {
    /// Returns the compact `Vendor / Target` label shown in enabled-target lists.
    /// 返回展示在已启用目标列表中的紧凑 `供应商 / 目标` 标签。
    pub fn display_label(&self) -> String {
        if self.vendor_name.eq_ignore_ascii_case(&self.target_name) {
            self.vendor_name.clone()
        } else {
            format!("{} / {}", self.vendor_name, self.target_name)
        }
    }
}

/// Returns the currently known capability targets exposed by one shared vendor instance.
/// 返回单个共享供应商实例当前已知的能力目标集合。
pub fn ai_capability_vendor_target_choices(vendor_id: &str) -> Vec<AiCapabilityVendorTargetChoice> {
    let normalized_vendor_id = normalize_ai_capability_vendor_id(vendor_id.to_string());
    ai_capability_provider_catalog::ai_capability_provider_targets_for_vendor(&normalized_vendor_id)
        .into_iter()
        .map(|target| AiCapabilityVendorTargetChoice {
            capability_kind: target.capability_kind,
            target_alias: target.target_alias.clone(),
            default_model_id: if target.supports_model_field {
                if target.default_model.trim().is_empty() {
                    target.target_alias.clone()
                } else {
                    target.default_model.clone()
                }
            } else if target.default_model.trim().is_empty() {
                target.target_alias.clone()
            } else {
                target.default_model.clone()
            },
            label: target.name.clone(),
            supports_model_override: target.supports_model_field,
            model_options: target.model_options.clone(),
        })
        .collect()
}

/// Returns the shared vendor display name shown by the AI capability settings pages when one provider instance should not expose a custom editable alias.
/// 返回 AI 能力设置页在供应商实例不应暴露自定义别名时展示的共享供应商名称。
pub fn ai_capability_vendor_display_name(vendor_id: &str) -> String {
    default_ai_capability_vendor_name(&normalize_ai_capability_vendor_id(vendor_id.to_string()))
}
