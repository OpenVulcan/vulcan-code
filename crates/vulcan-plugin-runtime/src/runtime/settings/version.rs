//! Explicit in-memory settings migration leaves persisted user bytes untouched.
//! 明确内存设置迁移保持用户持久原字节不变。

use super::*;

/// Read only the declared discriminator before selecting its exact strict schema.
/// 在选择对应严格 Schema 前仅读取声明的判别字段。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Version {
    /// The explicit file version chooses one decoder, never a guessed fallback.
    /// 明确文件版本选择单个解码器，绝不猜测回退。
    schema_version: u32,
}

/// Version one had no plugin overrides and therefore inherited every aggregate from its parent.
/// 第一版没有插件覆盖，因此全部聚合预算继承其父级。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VersionOne {
    /// The discriminator was already checked against the same immutable input bytes.
    /// 已针对同一不可变输入字节检查此判别字段。
    #[serde(rename = "schemaVersion")]
    _schema_version: u32,
    /// Preserve the exact upstream parent configuration, including every explicit user limit.
    /// 保留精确上游父级配置，包含全部明确用户上限。
    parent: EmbeddedRuntimeConfig,
    /// Preserve the original finite native capability budget.
    /// 保留原有限原生能力预算。
    max_capability_call_ms: u64,
    /// Preserve the original independent cleanup budget.
    /// 保留原独立清理预算。
    shutdown_timeout_ms: u64,
}

/// Version two introduced whole plugin overrides but had no purpose capacity configuration.
/// 第二版引入完整插件覆盖，但没有用途容量配置。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VersionTwo {
    /// The discriminator selects this exact historical shape before any migration.
    /// 判别字段在任何迁移前选择此精确历史形状。
    #[serde(rename = "schemaVersion")]
    _schema_version: u32,
    /// Preserve the original upstream parent budget without normalization.
    /// 保留原上游父级预算，不进行规范化。
    parent: EmbeddedRuntimeConfig,
    /// Version two already rejected duplicate plugin identities.
    /// 第二版已经拒绝重复插件身份。
    #[serde(default, deserialize_with = "budgets::deserialize_unique_budgets")]
    plugin_budgets: BTreeMap<String, EmbeddedPluginConfig>,
    /// Keep the original finite capability duration unchanged.
    /// 保持原有限能力时长不变。
    max_capability_call_ms: u64,
    /// Keep the original independent shutdown duration unchanged.
    /// 保持原独立关闭时长不变。
    shutdown_timeout_ms: u64,
}

/// Decode the exact declared version from bytes; upgrade historical versions only in memory.
/// 从字节解码精确声明版本；仅在内存中升级历史版本。
/// Returns current-format settings or an explicit error; original input is never rewritten or normalized.
/// 返回当前格式设置或明确错误；绝不回写或规范化原输入。
pub(super) fn decode(bytes: &str) -> Result<VulcanPluginRuntimeSettings, String> {
    // Parse the original bytes twice so strict decoders can still detect duplicate struct fields.
    // 两次解析原字节，使严格解码器仍可检测重复结构字段。
    let version = serde_json::from_str::<Version>(bytes).map_err(|error| error.to_string())?;
    match version.schema_version {
        1 => {
            // Every version-one field remains mandatory; new fields are rejected by that old schema.
            // 第一版全部字段仍为必需；旧 Schema 拒绝新增字段。
            let original =
                serde_json::from_str::<VersionOne>(bytes).map_err(|error| error.to_string())?;
            Ok(VulcanPluginRuntimeSettings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                parent: original.parent,
                plugin_budgets: BTreeMap::new(),
                capacity_budgets: BTreeMap::new(),
                max_capability_call_ms: original.max_capability_call_ms,
                shutdown_timeout_ms: original.shutdown_timeout_ms,
            })
        }
        2 => {
            // Parse the original strict version-two bytes so new fields and duplicates cannot be hidden.
            // 解析原严格第二版字节，使新增字段及重复不能被隐藏。
            let original =
                serde_json::from_str::<VersionTwo>(bytes).map_err(|error| error.to_string())?;
            Ok(VulcanPluginRuntimeSettings {
                schema_version: SETTINGS_SCHEMA_VERSION,
                parent: original.parent,
                plugin_budgets: original.plugin_budgets,
                capacity_budgets: BTreeMap::new(),
                max_capability_call_ms: original.max_capability_call_ms,
                shutdown_timeout_ms: original.shutdown_timeout_ms,
            })
        }
        SETTINGS_SCHEMA_VERSION => serde_json::from_str(bytes).map_err(|error| error.to_string()),
        unsupported => Err(format!(
            "Unsupported plugin runtime settings version {unsupported}."
        )),
    }
}
