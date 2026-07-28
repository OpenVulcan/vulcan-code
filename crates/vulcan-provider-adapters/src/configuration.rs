//! Persisted public AI configuration without provider catalogs or secrets.
//! 不包含供应商目录或秘钥的持久化公共 AI 配置。

use crate::{PersistedConnectionGraph, SecondaryRouteSelection};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vulcan_ai_contract::{ModelParameterValue, PromptCachePolicy};

/// Identifies the exact model and account selected by the user.
/// 标识用户选择的精确模型与账号。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveAiModelSelection {
    /// Stable provider identifier.
    /// 稳定供应商标识。
    pub provider_id: String,
    /// Exact model identifier.
    /// 精确模型标识。
    pub model_id: String,
    /// Explicit credential connection identifier.
    /// 显式凭据连接标识。
    pub connection_id: String,
}

/// Stores local narrowing and parameter choices for one connection-model pair.
/// 保存一个连接与模型组合的本地收窄及参数选择。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ModelExecutionOverrides {
    /// Optional local context-window narrowing.
    /// 可选本地上下文窗口收窄。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    /// Optional local output-token narrowing.
    /// 可选本地输出 Token 收窄。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<u64>,
    /// Optional model-level prompt-cache override.
    /// 可选模型级提示词缓存覆盖。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_cache: Option<PromptCachePolicy>,
    /// Adapter-registered model-specific values.
    /// 适配器注册的模型独立参数值。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_parameters: BTreeMap<String, ModelParameterValue>,
}

/// Stores all new AI configuration while excluding runtime-discovered model catalogs.
/// 保存全部新 AI 配置，同时排除运行时发现的模型目录。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PersistedAiConfiguration {
    /// Exact active model and account selection.
    /// 精确的当前模型与账号选择。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<ActiveAiModelSelection>,
    /// Provider connections and account preferences.
    /// 供应商连接与账号偏好。
    #[serde(default)]
    pub connection_graph: PersistedConnectionGraph,
    /// Per-connection model overrides keyed by collision-resistant composite key.
    /// 按抗冲突组合键索引的连接模型覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_overrides: BTreeMap<String, ModelExecutionOverrides>,
    /// Per-connection model secondary routes keyed by collision-resistant composite key.
    /// 按抗冲突组合键索引的连接模型二级路由。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secondary_routes: BTreeMap<String, SecondaryRouteSelection>,
}

impl PersistedAiConfiguration {
    /// Builds a stable key for one connection-model scoped setting.
    /// 为一个连接模型作用域设置构建稳定键。
    ///
    /// # Parameters
    /// 参数
    /// `connection_id` and `model_id` are exact identifiers.
    /// `connection_id` 与 `model_id` 是精确标识。
    ///
    /// # Returns
    /// 返回值
    /// A collision-resistant length-prefixed key.
    /// 返回抗冲突的长度前缀键。
    pub fn connection_model_key(connection_id: &str, model_id: &str) -> String {
        format!(
            "{}:{connection_id}:{}:{model_id}",
            connection_id.len(),
            model_id.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies runtime model catalogs cannot appear in persisted AI JSON.
    /// 验证运行时模型目录不会出现在持久化 AI JSON 中。
    #[test]
    fn persisted_ai_configuration_has_no_model_catalog() {
        let encoded =
            serde_json::to_string(&PersistedAiConfiguration::default()).expect("must serialize");
        assert!(!encoded.contains("catalog"));
        assert!(!encoded.contains("models"));
    }
}
