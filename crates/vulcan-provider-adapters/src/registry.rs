//! Strict provider adapter registry.
//! 严格供应商适配器注册表。

use crate::{RequestProjectorKind, ServiceEntry};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Identifies whether a provider definition is code-owned or user-owned.
/// 标识供应商定义由代码持有还是由用户持有。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Immutable provider implemented by Vulcan Code.
    /// 由 Vulcan Code 实现的不可变供应商。
    System,
    /// Editable compatibility provider created by the user.
    /// 由用户创建且可编辑的兼容供应商。
    Custom,
}

/// Describes one registered provider adapter.
/// 描述一个已注册供应商适配器。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDescriptor {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub id: String,
    /// User-visible provider name.
    /// 用户可见供应商名称。
    pub display_name: String,
    /// Definition ownership and editability kind.
    /// 定义所有权与可编辑性类别。
    pub kind: ProviderKind,
    /// Registered service entries.
    /// 已注册服务入口。
    pub service_entries: Vec<ServiceEntry>,
}

/// Stores strict provider registrations.
/// 保存严格供应商注册项。
#[derive(Debug, Clone, Default)]
pub struct ProviderRegistry {
    /// Registered providers keyed by stable identifier.
    /// 按稳定标识索引的已注册供应商。
    providers: BTreeMap<String, ProviderDescriptor>,
}

impl ProviderRegistry {
    /// Builds the enabled public-provider registry.
    /// 构建已启用公共供应商注册表。
    ///
    /// # Returns
    /// 返回值
    /// A strict registry containing every currently implemented provider.
    /// 返回包含每个当前已实现供应商的严格注册表。
    pub fn first_batch() -> Result<Self, String> {
        let mut registry = Self::default();
        registry.register(crate::providers::minimax_descriptor())?;
        registry.register(crate::providers::kimi_descriptor())?;
        registry.register(crate::providers::deepseek_descriptor())?;
        registry.register(crate::providers::alibaba_descriptor())?;
        registry.register(crate::providers::openrouter_descriptor())?;
        registry.register(crate::providers::tavily_descriptor())?;
        registry.register(crate::providers::openai_descriptor())?;
        registry.register(crate::providers::anthropic_descriptor())?;
        registry.register(crate::providers::google_descriptor())?;
        Ok(registry)
    }

    /// Builds the effective registry from immutable system providers and persisted custom providers.
    /// 根据不可变系统供应商与持久化自定义供应商构建有效注册表。
    ///
    /// # Parameters
    /// 参数
    /// `custom_providers` contains user-owned compatibility descriptors keyed by their exact identifier.
    /// `custom_providers` 包含按精确标识索引的用户自有兼容供应商描述。
    ///
    /// # Returns
    /// 返回值
    /// A validated registry containing both ownership classes.
    /// 返回包含两种所有权类别的已校验注册表。
    pub fn effective(
        custom_providers: &BTreeMap<String, ProviderDescriptor>,
    ) -> Result<Self, String> {
        let mut registry = Self::first_batch()?;
        for (provider_id, provider) in custom_providers {
            if provider_id != &provider.id {
                return Err(format!(
                    "Custom provider map key `{provider_id}` does not match descriptor `{}`.",
                    provider.id
                ));
            }
            if provider.kind != ProviderKind::Custom {
                return Err(format!(
                    "Persisted provider `{provider_id}` is not marked as custom."
                ));
            }
            if !provider.id.starts_with("custom:") {
                return Err(format!(
                    "Custom provider `{provider_id}` must use the `custom:` identifier prefix."
                ));
            }
            registry.register(provider.clone())?;
        }
        Ok(registry)
    }

    /// Registers one unique provider.
    /// 注册一个唯一供应商。
    ///
    /// # Parameters
    /// 参数
    /// `provider` is the provider descriptor.
    /// `provider` 是供应商描述。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when unique, otherwise a duplicate error.
    /// 唯一时返回 `Ok(())`，否则返回重复错误。
    pub fn register(&mut self, provider: ProviderDescriptor) -> Result<(), String> {
        if self.providers.contains_key(provider.id.as_str()) {
            return Err(format!(
                "Provider adapter `{}` is already registered.",
                provider.id
            ));
        }
        validate_request_projector_registration(&provider)?;
        self.providers.insert(provider.id.clone(), provider);
        Ok(())
    }

    /// Lists registered providers in stable identifier order.
    /// 按稳定标识顺序列出已注册供应商。
    ///
    /// # Returns
    /// 返回值
    /// Borrowed provider descriptors.
    /// 借用的供应商描述集合。
    pub fn providers(&self) -> Vec<&ProviderDescriptor> {
        self.providers.values().collect()
    }

    /// Returns one provider by its stable identifier.
    /// 按稳定标识返回一个供应商。
    ///
    /// # Parameters
    /// 参数
    /// `provider_id` is the exact registered identifier.
    /// `provider_id` 是精确的已注册标识。
    ///
    /// # Returns
    /// 返回值
    /// The provider descriptor when registered.
    /// 返回已注册的供应商描述。
    pub fn provider(&self, provider_id: &str) -> Option<&ProviderDescriptor> {
        self.providers.get(provider_id)
    }
}

/// Verifies every provider entry references the one registered projector owned by its definition.
/// 验证每个供应商入口引用其定义唯一持有的已注册投影器。
///
/// # Parameters
/// 参数
/// `provider` is the complete descriptor before registry insertion.
/// `provider` 是注册表插入前的完整描述。
///
/// # Returns
/// 返回值
/// Success when every entry matches the closed projector registry.
/// 每个入口匹配封闭投影器注册表时返回成功。
fn validate_request_projector_registration(provider: &ProviderDescriptor) -> Result<(), String> {
    let expected = match provider.kind {
        ProviderKind::Custom => RequestProjectorKind::Direct,
        ProviderKind::System => match provider.id.as_str() {
            "minimax" => RequestProjectorKind::MiniMax,
            "openrouter" => RequestProjectorKind::OpenRouter,
            "kimi" => RequestProjectorKind::Kimi,
            "deepseek" => RequestProjectorKind::DeepSeek,
            "alibaba" => RequestProjectorKind::Alibaba,
            "google" => RequestProjectorKind::Google,
            "tavily" | "openai" | "anthropic" => RequestProjectorKind::Direct,
            provider_id => {
                return Err(format!(
                    "System provider `{provider_id}` has no registered request projector owner."
                ));
            }
        },
    };
    for entry in &provider.service_entries {
        if entry.request_projector != expected {
            return Err(format!(
                "Provider `{}` entry `{}` declares projector `{:?}` but registry owner is `{:?}`.",
                provider.id, entry.id, entry.request_projector, expected
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Strict request-projector registration tests.
    //! 严格请求投影器注册测试。

    use super::ProviderRegistry;
    use crate::RequestProjectorKind;

    /// Verifies a mismatched built-in projector is rejected before runtime dispatch.
    /// 验证不匹配的内置投影器会在运行时分派前被拒绝。
    #[test]
    fn rejects_mismatched_system_projector() {
        let mut provider = crate::providers::minimax_descriptor();
        provider.service_entries[0].request_projector = RequestProjectorKind::OpenRouter;
        let mut registry = ProviderRegistry::default();
        let error = registry
            .register(provider)
            .expect_err("mismatched projector must fail");
        assert!(error.contains("registry owner"));
    }
}
