//! Configuration selection shares the atomic activation revision rather than introducing another mutable pointer.
//! 配置选择共用原子激活修订，不引入另一个可变指针。

use super::*;
use serde_json::Value;

/// Maximum compact JSON bytes in a newly managed configuration; all encoded record limits derive from this value.
/// 新受管配置的紧凑 JSON 字节上限；全部编码记录限制从此值派生。
pub const MAX_PLUGIN_CONFIGURATION_BYTES: u64 = 4 * 1024 * 1024;

/// Exact configuration authority: no declaration, or one complete value including an explicitly valid JSON null.
/// 精确配置权威：没有声明，或单个完整值，包含明确合法的 JSON 空值。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum VulcanPluginConfigurationSelection {
    /// The inspected package has no configuration declaration.
    /// 已检查插件包没有配置声明。
    /// Keep a struct variant so Serde rejects unknown fields that a unit variant would otherwise ignore.
    /// 保持结构体分支，使 Serde 拒绝单位分支原本会忽略的未知字段。
    NotDeclared {},
    /// One complete schema-validated document, never merged with mutable defaults or an older file.
    /// 单个完整且通过模式校验的文档，绝不与可变默认值或旧文件合并。
    Document {
        /// Frozen JSON value; it must not be included in ordinary logs or Debug output.
        /// 固定 JSON 值；不得包含于普通日志或 Debug 输出。
        value: Value,
    },
}

impl std::fmt::Debug for VulcanPluginConfigurationSelection {
    /// Format only the declared authority kind into `formatter`, excluding all user document contents.
    /// 仅向 `formatter` 格式化已声明权威种类，排除全部用户文档内容。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotDeclared {} => "NotDeclared",
            Self::Document { .. } => "Document(<redacted>)",
        })
    }
}

impl VulcanPluginConfigurationSelection {
    /// Borrow the frozen document, returning None only for a package which explicitly declares no configuration.
    /// 借用固定文档，仅对明确不声明配置的插件包返回 None。
    pub fn value(&self) -> Option<&Value> {
        match self {
            Self::NotDeclared {} => None,
            Self::Document { value } => Some(value),
        }
    }

    /// Validate the compact encoded value against its sole byte budget before persistence or runtime use.
    /// 持久化或运行时使用前，按唯一字节预算校验紧凑编码值。
    pub(super) fn validate_size(&self) -> VulcanPluginStoreResult<()> {
        if let Some(value) = self.value() {
            // Serialization diagnostics must not include potentially sensitive user configuration values.
            // 序列化诊断不得包含可能敏感的用户配置值。
            let bytes = serde_json::to_vec(value).map_err(|_| {
                VulcanPluginStoreError::invalid("Cannot encode selected plugin configuration.")
            })?;
            if bytes.len() as u64 > MAX_PLUGIN_CONFIGURATION_BYTES {
                return Err(VulcanPluginStoreError::invalid(
                    "Selected plugin configuration exceeds its encoded byte limit.",
                ));
            }
        }
        Ok(())
    }
}

/// Decode a required present selection from `deserializer`; JSON null is not an older missing field.
/// 从 `deserializer` 解码必需存在的选择；JSON 空值不是旧格式缺失字段。
pub(super) fn deserialize_present_selection<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<VulcanPluginConfigurationSelection>, D::Error> {
    VulcanPluginConfigurationSelection::deserialize(deserializer).map(Some)
}

impl PinnedVulcanPluginGeneration {
    /// Validate `selection` against this exact package's declaration and schema, returning no document contents in errors.
    /// 按此精确插件包的声明及模式校验 `selection`，错误不返回任何文档内容。
    pub(super) fn validate_configuration(
        &self,
        selection: &VulcanPluginConfigurationSelection,
    ) -> VulcanPluginStoreResult<()> {
        selection.validate_size()?;
        match (&self.configuration, selection) {
            (None, VulcanPluginConfigurationSelection::NotDeclared {}) => Ok(()),
            (Some(config), VulcanPluginConfigurationSelection::Document { value }) => {
                crate::runtime::validate_plugin_config_value(&self.package_root, config, value)
                    .map_err(|_| {
                        VulcanPluginStoreError::invalid(
                            "Selected configuration does not match the exact package schema.",
                        )
                    })
            }
            _ => Err(VulcanPluginStoreError::invalid(
                "Selected configuration does not match the exact package declaration.",
            )),
        }
    }
}

impl VulcanPluginGenerationStore {
    /// Atomically select `generation`, exact permissions, staged credentials and `configuration` after `expected_revision`.
    /// 在 `expected_revision` 之后原子选择 `generation`、精确权限、已暂存凭据及 `configuration`。
    /// Return the committed selection; errors require original-record inspection and never imply application readiness.
    /// 返回已提交选择；错误要求检查原记录，且绝不表示应用就绪。
    pub fn activate_configured(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        approved_permissions: Vec<String>,
        credential_revision: VulcanPluginCredentialRevision,
        configuration: VulcanPluginConfigurationSelection,
    ) -> VulcanPluginStoreResult<VulcanPluginActivation> {
        self.activate_approved_selection(
            generation,
            expected_revision,
            approved_permissions,
            Some(credential_revision),
            Some(configuration),
        )
    }
}
