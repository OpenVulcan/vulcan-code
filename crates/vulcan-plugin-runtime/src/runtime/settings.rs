//! Host-owned parent and plugin aggregate budgets for the formal runtime.
//! 正式运行时的宿主自有父级及插件聚合预算。

use luaskills::runtime::embedded::{EmbeddedPluginConfig, EmbeddedRuntimeConfig};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Exact schema version understood by this host settings reader and writer projection.
/// 此宿主设置读取器及写出投影理解的精确 Schema 版本。
const SETTINGS_SCHEMA_VERSION: u32 = 2;

mod budgets;
mod version;

#[cfg(test)]
mod budget_tests;

pub use budgets::VulcanPluginBudgetSource;

/// Persisted host configuration; core budget fields use the upstream type without a duplicated schema.
/// 持久化宿主配置；核心预算字段使用上游类型，不复制 Schema。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginRuntimeSettings {
    /// Exact host settings format; unsupported versions are never silently normalized.
    /// 精确宿主设置格式；绝不静默规范化不支持的版本。
    pub schema_version: u32,
    /// One parent budget shared by all formal plugin execution domains.
    /// 全部正式插件执行域共享的单个父级预算。
    pub parent: EmbeddedRuntimeConfig,
    /// Exact plugin-wide overrides; an absent plugin inherits the parent limits as one whole budget.
    /// 精确插件级覆盖；缺省插件将父级上限整体继承为预算。
    #[serde(default, deserialize_with = "budgets::deserialize_unique_budgets")]
    pub plugin_budgets: BTreeMap<String, EmbeddedPluginConfig>,
    /// Maximum duration of one native capability, also constrained by its original operation deadline.
    /// 单项原生能力的最大时长，同时受原操作截止时间约束。
    pub max_capability_call_ms: u64,
    /// Independent default shutdown budget for host-created module finalizers.
    /// 宿主创建模块关闭器的独立默认关闭预算。
    pub shutdown_timeout_ms: u64,
}

impl Default for VulcanPluginRuntimeSettings {
    /// Return the sole compiled host defaults without creating or rewriting a settings file.
    /// 返回唯一编译宿主默认值，不创建或改写设置文件。
    fn default() -> Self {
        Self {
            schema_version: SETTINGS_SCHEMA_VERSION,
            parent: EmbeddedRuntimeConfig {
                max_registered_plugins: 64,
                max_registered_pools: 256,
                max_sessions: 128,
                max_registered_capabilities: 64,
                max_resident_vms: 32,
                max_running_calls: 4,
                max_queued_calls: 128,
                max_queued_bytes: 32 * 1024 * 1024,
                max_operations: 512,
                max_effect_records_per_operation: 256,
                max_effect_bytes_per_operation: 1024 * 1024,
                max_host_requests: 32,
                max_host_request_bytes: 32 * 1024 * 1024,
                max_value_bytes: 8 * 1024 * 1024,
            },
            plugin_budgets: BTreeMap::new(),
            max_capability_call_ms: 60_000,
            shutdown_timeout_ms: super::VULCAN_PLUGIN_DEFAULT_SHUTDOWN_TIMEOUT_MS,
        }
    }
}

impl VulcanPluginRuntimeSettings {
    /// Validate the exact format, upstream parent limits and representable finite host deadlines.
    /// 校验精确格式、上游父级限制及可表示的有限宿主截止时间。
    /// Returns a diagnostic before any runtime worker or native capability is allocated.
    /// 在分配任何运行时工作线程或原生能力前返回诊断。
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SETTINGS_SCHEMA_VERSION {
            return Err(format!(
                "Unsupported plugin runtime settings version {}.",
                self.schema_version
            ));
        }
        self.parent.validate().map_err(|error| error.to_string())?;
        for (plugin_id, budget) in &self.plugin_budgets {
            crate::catalog::validate_identifier(plugin_id, "budget owner")?;
            budget
                .validate(&self.parent)
                .map_err(|error| format!("Invalid plugin budget for {plugin_id}: {error}"))?;
        }
        for (name, milliseconds) in [
            ("maxCapabilityCallMs", self.max_capability_call_ms),
            ("shutdownTimeoutMs", self.shutdown_timeout_ms),
        ] {
            if milliseconds == 0
                || Instant::now()
                    .checked_add(Duration::from_millis(milliseconds))
                    .is_none()
            {
                return Err(format!(
                    "Plugin runtime {name} must be a representable positive duration."
                ));
            }
        }
        Ok(())
    }
}

/// Load one exact host settings file below `plugin_state_root`; absence uses memory-only defaults.
/// 加载 `plugin_state_root` 下唯一宿主设置文件；缺失时使用内存默认值。
/// Invalid JSON, unknown fields or unsupported versions return errors without modifying the original bytes.
/// 非法 JSON、未知字段或不支持的版本返回错误，不修改原字节。
pub fn load_plugin_runtime_settings(
    plugin_state_root: &Path,
) -> Result<VulcanPluginRuntimeSettings, String> {
    // This is host state, outside every plugin package and plugin-owned configuration namespace.
    // 这是宿主状态，位于每个插件包及插件自有配置命名空间之外。
    let path = plugin_state_root.join("plugin-runtime-settings.json");
    // Only an absent file has defined default semantics; all malformed or unreadable existing files fail.
    // 仅文件缺失具有已定义的默认语义；全部格式错误或无法读取的既有文件均失败。
    let settings = match std::fs::read_to_string(&path) {
        Ok(bytes) => version::decode(&bytes).map_err(|error| {
            format!(
                "Invalid plugin runtime settings {}: {error}",
                path.display()
            )
        })?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            VulcanPluginRuntimeSettings::default()
        }
        Err(error) => {
            return Err(format!(
                "Failed to read plugin runtime settings {}: {error}",
                path.display()
            ));
        }
    };
    settings.validate()?;
    Ok(settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Missing host settings must not create directories or persist defaults.
    /// 宿主设置缺失时不得创建目录或持久化默认值。
    #[test]
    fn missing_plugin_runtime_settings_remain_memory_only() {
        let root = tempfile::tempdir().expect("create settings fixture");
        let missing = root.path().join("uncreated");
        assert_eq!(
            load_plugin_runtime_settings(&missing).expect("load defaults"),
            VulcanPluginRuntimeSettings::default()
        );
        assert!(!missing.exists());
    }

    /// Persisted settings are strict and invalid bytes remain available for diagnosis.
    /// 持久设置严格校验，并保留无效原字节供诊断。
    #[test]
    fn plugin_runtime_settings_reject_invalid_configuration_without_rewriting() {
        let root = tempfile::tempdir().expect("create settings fixture");
        let path = root.path().join("plugin-runtime-settings.json");
        let valid = serde_json::to_value(VulcanPluginRuntimeSettings::default())
            .expect("serialize default settings");
        for mutation in ["version", "unknown", "missing", "parent", "deadline"] {
            let mut invalid = valid.clone();
            match mutation {
                "version" => {
                    invalid["schemaVersion"] = serde_json::json!(SETTINGS_SCHEMA_VERSION + 1)
                }
                "unknown" => invalid["fallback"] = serde_json::json!(true),
                "missing" => {
                    invalid
                        .as_object_mut()
                        .expect("settings object")
                        .remove("parent");
                }
                "parent" => invalid["parent"]["max_running_calls"] = serde_json::json!(0),
                "deadline" => invalid["shutdownTimeoutMs"] = serde_json::json!(0),
                _ => unreachable!("fixed mutation case"),
            }
            let bytes = serde_json::to_vec(&invalid).expect("serialize invalid settings");
            std::fs::write(&path, &bytes).expect("write invalid settings");
            assert!(
                load_plugin_runtime_settings(root.path()).is_err(),
                "{mutation} must be rejected"
            );
            assert_eq!(std::fs::read(&path).expect("read original settings"), bytes);
        }
        let selected = VulcanPluginRuntimeSettings::default();
        let bytes = serde_json::to_vec(&selected).expect("serialize selected settings");
        std::fs::write(&path, &bytes).expect("write selected settings");
        assert_eq!(
            load_plugin_runtime_settings(root.path()).expect("load selected settings"),
            selected
        );
        assert_eq!(std::fs::read(&path).expect("read selected settings"), bytes);
    }
}
