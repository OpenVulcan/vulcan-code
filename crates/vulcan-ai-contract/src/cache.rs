//! Provider-neutral prompt-cache policies and capabilities.
//! 供应商无关的提示词缓存策略与能力。

use serde::{Deserialize, Serialize};

/// Selects how Vulcan should influence provider prompt caching.
/// 选择 Vulcan 应如何影响供应商提示词缓存。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum PromptCachePolicy {
    /// Omits host-injected cache controls and follows provider defaults.
    /// 省略宿主注入的缓存控制并遵循供应商默认行为。
    #[default]
    ProviderDefault,
    /// Requests adapter-managed explicit prompt caching.
    /// 请求由适配器管理的显式提示词缓存。
    ManagedExplicit {
        /// Selected TTL policy.
        /// 选定的 TTL 策略。
        ttl: PromptCacheTtlSelection,
    },
}

/// Selects an adapter-declared cache TTL.
/// 选择适配器声明的缓存 TTL。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptCacheTtlSelection {
    /// Uses the protocol profile default.
    /// 使用协议 Profile 默认值。
    ProfileDefault,
    /// Uses one capability option identifier.
    /// 使用一个能力选项标识。
    CapabilityOption {
        /// Adapter-declared option identifier.
        /// 适配器声明的选项标识。
        option_id: String,
    },
}

/// Describes one supported cache TTL option.
/// 描述一个受支持的缓存 TTL 选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptCacheTtlOption {
    /// Stable option identifier.
    /// 稳定选项标识。
    pub id: String,
    /// User-visible label.
    /// 用户可见标签。
    pub display_name: String,
    /// Optional billing or behavior note.
    /// 可选计费或行为说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Describes prompt-cache support for one executable route.
/// 描述一条可执行路由的提示词缓存支持。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptCacheCapability {
    /// User-visible description of provider-default behavior.
    /// 供应商默认行为的用户可见说明。
    pub provider_default_behavior: String,
    /// Whether managed explicit caching is available.
    /// 是否支持托管显式缓存。
    pub supports_managed_explicit: bool,
    /// Supported TTL options.
    /// 支持的 TTL 选项。
    #[serde(default)]
    pub supported_ttl_options: Vec<PromptCacheTtlOption>,
    /// Default TTL option identifier when one exists.
    /// 存在时的默认 TTL 选项标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_default_ttl_option_id: Option<String>,
    /// Minimum cacheable token count when known.
    /// 已知时的最小可缓存 Token 数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_cacheable_tokens: Option<u64>,
    /// Maximum explicit breakpoint count when known.
    /// 已知时的最大显式断点数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_breakpoints: Option<u32>,
}

impl Default for PromptCacheCapability {
    /// Builds a capability that performs no host-managed cache injection.
    /// 构建一个不执行宿主管理缓存注入的能力。
    ///
    /// # Returns
    /// 返回值
    /// An explicit unsupported managed-cache capability.
    /// 返回明确不支持托管缓存的能力。
    fn default() -> Self {
        Self {
            provider_default_behavior: String::from(
                "Follow provider defaults without host-injected cache controls.",
            ),
            supports_managed_explicit: false,
            supported_ttl_options: Vec::new(),
            profile_default_ttl_option_id: None,
            minimum_cacheable_tokens: None,
            maximum_breakpoints: None,
        }
    }
}

impl PromptCacheCapability {
    /// Validates one public cache policy against this capability.
    /// 根据当前能力校验一个公共缓存策略。
    ///
    /// # Parameters
    /// 参数
    /// `policy` is the resolved public policy for one execution.
    /// `policy` 是一次执行解析后的公共策略。
    ///
    /// # Returns
    /// 返回值
    /// `Ok(())` when valid, otherwise a controlled validation message.
    /// 有效时返回 `Ok(())`，否则返回受控校验消息。
    pub fn validate_policy(&self, policy: &PromptCachePolicy) -> Result<(), String> {
        match policy {
            PromptCachePolicy::ProviderDefault => Ok(()),
            PromptCachePolicy::ManagedExplicit { ttl } => {
                if !self.supports_managed_explicit {
                    return Err(String::from(
                        "Managed explicit prompt caching is not supported by this route.",
                    ));
                }
                match ttl {
                    PromptCacheTtlSelection::ProfileDefault => self
                        .profile_default_ttl_option_id
                        .as_ref()
                        .filter(|default_id| {
                            self.supported_ttl_options
                                .iter()
                                .any(|option| option.id.as_str() == default_id.as_str())
                        })
                        .map(|_| ())
                        .ok_or_else(|| {
                            String::from(
                                "Managed explicit caching has no registered profile-default TTL.",
                            )
                        }),
                    PromptCacheTtlSelection::CapabilityOption { option_id } => self
                        .supported_ttl_options
                        .iter()
                        .any(|option| option.id == *option_id)
                        .then_some(())
                        .ok_or_else(|| {
                            format!(
                                "Prompt-cache TTL option `{option_id}` is not supported by this route."
                            )
                        }),
                }
            }
        }
    }
}
