//! Strict provider adapter registry.
//! 严格供应商适配器注册表。

use crate::ServiceEntry;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
        registry.register(crate::providers::openrouter_descriptor())?;
        registry.register(crate::providers::openai_descriptor())?;
        registry.register(crate::providers::anthropic_descriptor())?;
        registry.register(crate::providers::google_descriptor())?;
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
