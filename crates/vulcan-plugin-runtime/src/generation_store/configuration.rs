//! Configuration selection shares the atomic activation revision rather than introducing another mutable pointer.
//! 配置选择共用原子激活修订，不引入另一个可变指针。

use super::*;
use serde_json::Value;

/// Maximum compact JSON bytes in a newly managed configuration; all encoded record limits derive from this value.
/// 新受管配置的紧凑 JSON 字节上限；全部编码记录限制从此值派生。
pub const MAX_PLUGIN_CONFIGURATION_BYTES: u64 = 4 * 1024 * 1024;

/// Exact configuration authority: no declaration, one complete value including an explicitly valid JSON null,
/// or the current defaults of a bundled package.
/// 精确配置权威：没有声明、单个完整值（包含明确合法的 JSON 空值），或内建插件包的当前默认值。
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
    /// The current package defaults, resolved at use; only a bundled settings record may hold it,
    /// so an upgraded bundled package brings its new defaults.
    /// 使用时解析的当前插件包默认值；仅内建设置记录可持有，使升级后的内建插件包带来新默认值。
    PackageDefaults {},
}

impl std::fmt::Debug for VulcanPluginConfigurationSelection {
    /// Format only the declared authority kind into `formatter`, excluding all user document contents.
    /// 仅向 `formatter` 格式化已声明权威种类，排除全部用户文档内容。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::NotDeclared {} => "NotDeclared",
            Self::Document { .. } => "Document(<redacted>)",
            Self::PackageDefaults {} => "PackageDefaults",
        })
    }
}

impl VulcanPluginConfigurationSelection {
    /// Borrow the frozen document; None when no configuration is declared or the package defaults apply.
    /// 借用固定文档；未声明配置或适用插件包默认值时返回 None。
    pub fn value(&self) -> Option<&Value> {
        match self {
            Self::NotDeclared {} | Self::PackageDefaults {} => None,
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
