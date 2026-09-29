//! Package-schema-bound plugin configuration documents.
//! 绑定插件包Schema的插件配置文档。

use super::environment::plugin_execution_package_root;
use super::{
    VulcanPluginRuntimeError, VulcanPluginRuntimeManager, canonical_plugin_file,
    read_plugin_json_state,
};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

/// Returns the deterministic configuration document path for one validated plugin identity.
/// 返回一个已校验插件身份的确定性配置文档路径。
///
/// # Parameters
/// 参数
/// `state_root` is the fixed per-user plugin state root.
/// `state_root` 是固定的用户插件状态根。
/// `plugin_id` is the validated plugin identity selecting its state directory.
/// `plugin_id` 是选择所属状态目录的已校验插件身份。
///
/// # Returns
/// 返回值
/// Returns the deterministic config.json path without filesystem mutation.
/// 返回不修改文件系统的确定性config.json路径。
pub(super) fn plugin_config_state_path(state_root: &Path, plugin_id: &str) -> PathBuf {
    state_root.join(plugin_id).join("config.json")
}

/// Validates one complete plugin configuration against its package-owned schema.
/// 根据插件包持有的 Schema 校验一份完整插件配置。
///
/// # Parameters
/// 参数
/// `package_root` owns the schema, `config` identifies its file, and `value` is untrusted state.
/// `package_root` 持有 Schema，`config` 标识其文件，`value` 是不可信状态。
///
/// # Returns
/// 返回值
/// Returns unit when the value satisfies the schema.
/// 配置值满足 Schema 时返回空值。
pub(crate) fn validate_plugin_config_value(
    package_root: &Path,
    config: &crate::VulcanPluginConfigManifest,
    value: &Value,
) -> Result<(), VulcanPluginRuntimeError> {
    let schema_path =
        canonical_plugin_file(package_root, &config.schema_file, "configuration schema")
            .map_err(VulcanPluginRuntimeError::new)?;
    let schema = read_plugin_json_state(&schema_path, "configuration schema")?;
    crate::catalog::validate_json_schema_instance(&schema, value, "plugin configuration")
        .map_err(VulcanPluginRuntimeError::new)
}

impl VulcanPluginRuntimeManager {
    /// Read configuration using the caller's exact retained package descriptor.
    /// 使用调用方保留的精确包描述符读取配置。
    /// Return validated state/defaults without looking up a newer package generation.
    /// 返回已校验状态或默认值，不查找更新的包代次。
    pub(super) fn plugin_config_for_snapshot(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // The ID and schema belong to the same immutable descriptor.
        // ID 与 Schema 属于同一不可变描述符。
        let plugin_id = &plugin.id;
        // manifest determines whether this package owns any executable resource requiring isolation.
        // manifest 决定当前插件包是否持有任何需要隔离的可执行资源。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let config = manifest.config.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` does not declare configuration."
            ))
        })?;
        // A managed selection is complete; never mix its package schema with a newer mutable legacy document.
        // 受管选择是完整的；绝不将其插件包模式与更新的可变旧文档混用。
        if let Some(selection) = plugin
            .managed_package()
            .and_then(|managed| managed.configuration())
        {
            return selection.value().cloned().ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Managed plugin configuration is explicitly undeclared.",
                )
            });
        }
        // executionRoot pins configuration defaults and schema to the authorized package generation.
        // executionRoot 把配置默认值与 Schema 固定到已授权插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let state_path = plugin_config_state_path(&self.state.plugin_state_root, plugin_id);
        let value = if state_path.exists() {
            read_plugin_json_state(&state_path, "configuration")?
        } else {
            let defaults_path = canonical_plugin_file(
                &execution_root,
                &config.defaults_file,
                "configuration defaults",
            )
            .map_err(VulcanPluginRuntimeError::new)?;
            read_plugin_json_state(&defaults_path, "configuration defaults")?
        };
        validate_plugin_config_value(&execution_root, config, &value)?;
        Ok(value)
    }

    /// Write the value under the exact retained package schema and current-generation gate.
    /// 在精确保留包 Schema 及当前代次门控下写入值。
    /// Reject a changed generation before touching persistent state; return the committed value.
    /// 在触及持久状态前拒绝已变代次；返回已提交值。
    pub(super) fn update_plugin_config_for_snapshot(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
        value: Value,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // Configuration mutation shares the existing publication gate.
        // 配置变更共用既有发布门。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        // Resolve current authority once while publication cannot change it.
        // 发布不能改变权威期间，一次性解析当前权威。
        let snapshot = self.snapshot();
        // Retain the original plugin ID, including the state-file namespace.
        // 保留原插件 ID，包括状态文件命名空间。
        let plugin_id = &plugin.id;
        // A matching name cannot substitute for the exact generation and package bytes.
        // 相同名称不能替代精确代次及包字节。
        let current = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Plugin configuration owner is no longer installed.")
        })?;
        if current.generation != plugin.generation
            || current.content_fingerprint != plugin.content_fingerprint
            || current.package_root != plugin.package_root
        {
            return Err(VulcanPluginRuntimeError::new(
                "Plugin configuration generation changed before write.",
            ));
        }
        if current
            .managed_package()
            .is_some_and(|managed| managed.configuration().is_some())
        {
            return Err(VulcanPluginRuntimeError::new(
                "Managed plugin configuration updates require a configured activation transaction.",
            ));
        }
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let config = manifest.config.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` does not declare configuration."
            ))
        })?;
        // executionRoot pins configuration validation to the authorized package generation.
        // executionRoot 把配置校验固定到已授权插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        validate_plugin_config_value(&execution_root, config, &value)?;
        let encoded = serde_json::to_vec_pretty(&value).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to encode Vulcan plugin `{plugin_id}` configuration: {error}"
            ))
        })?;
        let mut published = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        crate::atomic_file::write_file_atomically(
            &plugin_config_state_path(&self.state.plugin_state_root, plugin_id),
            &encoded,
        )
        .map_err(VulcanPluginRuntimeError::new)?;
        // Registration hooks may read this configuration while shaping model-visible tools.
        // 注册 Hook 可读取此配置来塑造模型可见工具，因此写盘成功后必须失效旧目录缓存。
        let mut next_snapshot = (**published).clone();
        next_snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        *published = Arc::new(next_snapshot);
        drop(published);
        // Already-running calls drain on their original VM; no later call reuses pre-update state.
        // 已运行调用在原 VM 中排空；后续调用不复用更新前状态。
        self.close_reusable_domains(Some(plugin_id), None, false)?;
        Ok(value)
    }
}

impl VulcanPluginRuntimeManager {
    /// Reads the effective configuration for one manifest-declared plugin.
    /// 读取一个清单声明插件的生效配置。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact catalog identity.
    /// `plugin_id` 是精确目录身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the persisted configuration or the package defaults when no state exists.
    /// 返回持久化配置；状态不存在时返回插件包默认值。
    pub fn plugin_config(&self, plugin_id: &str) -> Result<Value, VulcanPluginRuntimeError> {
        // Retain one complete package snapshot for schema and defaults.
        // 为 Schema 及默认值保留一个完整包快照。
        let snapshot = self.snapshot();
        // Resolve only the explicitly requested plugin.
        // 仅解析显式请求的插件。
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        self.plugin_config_for_snapshot(plugin)
    }

    /// Validates and atomically persists one complete plugin configuration object.
    /// 校验并原子持久化一个完整插件配置对象。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package and `value` is the complete replacement document.
    /// `plugin_id` 选择插件包，`value` 是完整替换文档。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact persisted configuration after durable publication.
    /// 在持久发布后返回精确持久化配置。
    pub fn update_plugin_config(
        &self,
        plugin_id: &str,
        value: Value,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // The write helper checks this exact descriptor again under the publication gate.
        // 写入辅助入口在发布门内再次校验此精确描述符。
        let snapshot = self.snapshot();
        // Freeze schema ownership before entering the common mutation path.
        // 在进入共同变更路径前冻结 Schema 归属。
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        self.update_plugin_config_for_snapshot(plugin, value)
    }

    /// Checks for existing state before writing one initial complete configuration document.
    /// 写入一份初始完整配置文档前检查既有状态。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package and `value` is the initial complete document.
    /// `plugin_id` 选择插件包，`value` 是初始完整文档。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` after writing or `false` when the initial existence check finds user state; this is not a concurrent create-if-absent transaction.
    /// 写入后返回true，初始存在性检查发现用户状态时返回false；这不是并发原子缺失创建事务。
    pub fn initialize_plugin_config_if_absent(
        &self,
        plugin_id: &str,
        value: Value,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // A selected managed document is already initialized regardless of any unrelated legacy state file.
        // 已选定受管文档已经初始化，与任何无关旧状态文件无关。
        let snapshot = self.snapshot();
        if let Some(selection) = snapshot
            .plugins
            .get(plugin_id)
            .and_then(|plugin| plugin.managed_package())
            .and_then(|managed| managed.configuration())
        {
            return if selection.value().is_some() {
                Ok(false)
            } else {
                Err(VulcanPluginRuntimeError::new(
                    "Managed plugin does not declare configuration.",
                ))
            };
        }
        let state_path = plugin_config_state_path(&self.state.plugin_state_root, plugin_id);
        if state_path.exists() {
            return Ok(false);
        }
        self.update_plugin_config(plugin_id, value)?;
        Ok(true)
    }
}
