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

/// One exact descriptor identity and freshly parsed schema own a reusable validator, without retaining configuration values.
/// 单个精确描述符身份及刚解析的 Schema 拥有可复用校验器，不保留配置值。
pub(super) struct CompiledConfigurationSchema {
    /// The retained descriptor identifies the plugin whose declaration was read.
    /// 保留描述符标识被读取声明所属的插件。
    plugin_id: String,
    /// The actual descriptor generation separates package updates without a new identity authority.
    /// 实际描述符代次隔离插件包更新，不建立新的身份权威。
    generation: u64,
    /// The current checked canonical file identifies this schema declaration.
    /// 当前经过检查的规范文件标识此 Schema 声明。
    path: PathBuf,
    /// Fresh parsed file contents must match before the compiled declaration can be reused.
    /// 新解析的文件内容必须匹配后才能复用已编译声明。
    schema: Value,
    /// Each reader retains its own exact validator across concurrent replacement of the sole slot.
    /// 每个读取者跨唯一槽的并发替换保留自身精确校验器。
    validator: Arc<jsonschema::Validator>,
}

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
    /// Select a complete configuration for candidate using the exact predecessor's declared authority.
    /// 使用精确 predecessor 的声明权威，为 candidate 选择完整配置。
    /// Existing declarations preserve their document; first declarations use candidate defaults; removed declarations stay absent.
    /// 既有声明保留其文档；首次声明使用候选默认值；移除声明保持缺失。
    /// Returns a value validated against the new package without writing state or merging documents.
    /// 返回按新插件包校验的值，不写入状态或合并文档。
    pub fn configuration_for_update(
        &self,
        candidate: &crate::PinnedVulcanPluginGeneration,
        predecessor: &crate::VulcanPluginDescriptor,
    ) -> Result<crate::VulcanPluginConfigurationSelection, VulcanPluginRuntimeError> {
        if candidate.identity().plugin_id != predecessor.id {
            return Err(VulcanPluginRuntimeError::new(
                "Configuration update requires the same plugin identity.",
            ));
        }
        // Inspect only the pinned immutable candidate, never the mutable source supplied earlier.
        // 仅检查已固定不可变候选，绝不检查较早提供的可变来源。
        let package = crate::inspect_user_plugin_package(candidate.package_root())
            .map_err(VulcanPluginRuntimeError::new)?;
        if package.content_fingerprint != candidate.identity().content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(
                "Configuration candidate content changed.",
            ));
        }
        // Both declaration decisions come from validated manifests, not field probing or fallback guesses.
        // 两个声明决定均来自已校验清单，不通过字段探测或备用猜测。
        let manifest = package.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Configuration candidate has no valid manifest.")
        })?;
        let Some(config) = &manifest.config else {
            return Ok(crate::VulcanPluginConfigurationSelection::NotDeclared {});
        };
        // The predecessor declaration determines whether preserving a document is meaningful.
        // 前驱声明决定保留文档是否有意义。
        let predecessor_manifest = predecessor.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Configuration predecessor has no valid manifest.")
        })?;
        // Newly declared configuration uses this candidate's defaults exactly once, without touching legacy state.
        // 新声明配置仅使用此候选默认值一次，不触碰旧状态。
        let value = if predecessor_manifest.config.is_some() {
            self.plugin_config_for_snapshot(predecessor)?
        } else {
            // The package validator remains authoritative for relative paths and reparse-point exclusion.
            // 插件包校验器继续对相对路径及重解析点排除具有权威。
            let defaults = canonical_plugin_file(
                candidate.package_root(),
                &config.defaults_file,
                "configuration defaults",
            )
            .map_err(VulcanPluginRuntimeError::new)?;
            read_plugin_json_state(&defaults, "configuration defaults")?
        };
        validate_plugin_config_value(candidate.package_root(), config, &value).map_err(|_| {
            VulcanPluginRuntimeError::new(
                "Existing configuration does not satisfy the candidate package schema.",
            )
        })?;
        Ok(crate::VulcanPluginConfigurationSelection::Document { value })
    }

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
        // The managed selection returned above; this read still resolves and decodes the actual schema every time.
        // 受管选择已在上方返回；此读取仍在每次解析并解码实际 Schema。
        let schema_path =
            canonical_plugin_file(&execution_root, &config.schema_file, "configuration schema")
                .map_err(VulcanPluginRuntimeError::new)?;
        // Current file evidence must succeed before even an identical cached declaration can be used.
        // 当前文件证据必须成功，才能使用即使相同的已缓存声明。
        let schema = read_plugin_json_state(&schema_path, "configuration schema")?;
        // Only compilation is reused; every freshly read configuration remains subject to validation.
        // 仅复用编译；每份刚读取配置仍必须接受校验。
        let validator = self.configuration_schema_validator(plugin, schema_path, schema)?;
        validator.validate(&value).map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Invalid plugin configuration: {error}"))
        })?;
        Ok(value)
    }

    /// Reuse compilation for plugin's current path/schema or compile that exact declaration outside the cache lock.
    /// 为 plugin 的当前 path/schema 复用编译，或在缓存锁外编译该精确声明。
    /// plugin supplies the original identity; path is checked and schema freshly parsed; return its validator or original error.
    /// plugin 提供原身份；path 已检查且 schema 刚解析；返回其校验器或原错误。
    fn configuration_schema_validator(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
        path: PathBuf,
        schema: Value,
    ) -> Result<Arc<jsonschema::Validator>, VulcanPluginRuntimeError> {
        {
            // Copy only the exact matching validator; never hold this lock during compilation or value validation.
            // 仅复制精确匹配的校验器；编译或配置值校验期间绝不持有此锁。
            let slot = self.state.configuration_schema_cache.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin configuration schema cache is poisoned.")
            })?;
            // A matching entry supplies only compilation for this exact fresh declaration and original generation.
            // 匹配条目仅为此精确新声明及原代次提供编译结果。
            if let Some(compiled) = slot.as_ref()
                && compiled.plugin_id == plugin.id
                && compiled.generation == plugin.generation
                && compiled.path == path
                && compiled.schema == schema
            {
                return Ok(Arc::clone(&compiled.validator));
            }
        }
        // External-reference rejection and compilation errors use the sole existing catalog implementation.
        // 外部引用拒绝及编译错误使用唯一既有目录实现。
        let validator = Arc::new(
            crate::catalog::compile_json_schema(&schema, "plugin configuration")
                .map_err(VulcanPluginRuntimeError::new)?,
        );
        *self.state.configuration_schema_cache.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Plugin configuration schema cache is poisoned.")
        })? = Some(CompiledConfigurationSchema {
            plugin_id: plugin.id.clone(),
            generation: plugin.generation,
            path,
            schema,
            validator: Arc::clone(&validator),
        });
        Ok(validator)
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
        self.require_routing_publication_locked(&_registration, None)?;
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
        // Explicit legacy configuration writes initialize the same host-selected user root as plugin installation.
        // 显式旧格式配置写入初始化与插件安装相同的宿主选定用户根目录。
        std::fs::create_dir_all(&self.state.user_plugin_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to create user plugin root: {error}"))
        })?;
        // Opening the sole store validates the literal root and creates only its controlled private layout.
        // 打开唯一存储校验字面根目录，并仅创建其受控私有布局。
        let store = crate::VulcanPluginGenerationStore::open(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Freeze the complete actual selection before requesting the exclusive typed mutation fence.
        // 请求排他类型化变更屏障之前，固定完整实际选择。
        let selected = store
            .activation(plugin_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // A retained managed descriptor must match all selected authorities; flat or bundled packages require absence.
        // 保留受管描述符必须匹配全部已选权威；平铺或内建插件包要求选择缺失。
        match (current.managed_package(), selected.as_ref()) {
            (Some(managed), Some(activation)) if managed.matches_activation(activation) => {}
            (None, None) => {}
            _ => {
                return Err(VulcanPluginRuntimeError::new(
                    "Legacy plugin configuration activation changed before write.",
                ));
            }
        }
        // Registration precedes the same cross-process namespace fence used by update reservations and activation writers.
        // 注册门先于更新预留及激活写入者使用的相同跨进程命名空间屏障。
        let mutation = store
            .acquire_legacy_mutation_guard(plugin_id, selected.as_ref())
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        self.write_plugin_config_with_mutation_guard(plugin, value, &mutation, &_registration, None)
    }

    /// Write value for the exact lifecycle plugin under its original authority and publication gate.
    /// 在原生命周期权威及发布门内，为精确 plugin 写入 value。
    /// Scope must match the admitted event; authority lends its actual guard without reopening a namespace.
    /// scope 必须匹配已接纳事件；authority 借出实际守卫，不重开命名空间。
    /// Return the committed value, rejecting managed configuration selections before any mutation.
    /// 返回已提交值；在任何修改前拒绝受管配置选择。
    pub(super) fn update_legacy_lifecycle_plugin_config_for_snapshot(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
        value: Value,
        authority: &super::legacy_lifecycle::LegacyLifecycleExecution,
        scope: super::VulcanPluginBridgeLeaseScope,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // The same gate excludes publication throughout validation and the durable write.
        // 同一门在校验及持久写入全程排除发布。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(
            &_registration,
            Some(super::routing::ActivationRoutingOriginal::LegacyMutation(
                authority.mutation_guard(),
            )),
        )?;
        authority.validate(&self.state, plugin, scope)?;
        if plugin
            .managed_package()
            .is_some_and(|managed| managed.configuration().is_some())
        {
            return Err(VulcanPluginRuntimeError::new(
                "Managed plugin configuration updates require a configured activation transaction.",
            ));
        }
        self.write_plugin_config_with_mutation_guard(
            plugin,
            value,
            authority.mutation_guard(),
            &_registration,
            Some(super::routing::ActivationRoutingOriginal::LegacyMutation(
                authority.mutation_guard(),
            )),
        )
    }

    /// Validate and persist value for plugin while the caller retains its publication gate and original mutation guard.
    /// 调用方保留发布门及原 mutation 守卫期间，为 plugin 校验并持久化 value。
    /// Both ordinary and lifecycle writers share this sole path; registration proves the caller-held routing exclusion.
    /// 普通及生命周期写入者共用此唯一路径；registration 证明调用方持有的路由互斥。
    /// `original` borrows actual callback ownership or is absent for ordinary writes; returns the committed value.
    /// `original` 借用实际回调所有权，普通写入则缺失；返回已提交值。
    fn write_plugin_config_with_mutation_guard(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
        value: Value,
        mutation: &crate::VulcanPluginLegacyMutationGuard,
        registration: &std::sync::MutexGuard<'_, ()>,
        original: Option<super::routing::ActivationRoutingOriginal<'_>>,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        self.require_routing_publication_locked(registration, original)?;
        // The state namespace remains bound to the same immutable descriptor as its schema.
        // 状态命名空间仍绑定至与模式相同的不可变描述符。
        let plugin_id = &plugin.id;
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
        // Keep the original physical fence and revalidate its exact authority immediately before the mutable file write.
        // 保持原物理屏障，并在可变文件写入前立即重新校验其精确权威。
        mutation
            .validate_user_root(&self.state.user_plugin_root, plugin_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        mutation
            .write_legacy_configuration_atomically(
                &plugin_config_state_path(&self.state.plugin_state_root, plugin_id),
                &encoded,
            )
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
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
