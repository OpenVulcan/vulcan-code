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
    /// Existing declarations preserve their document; first declarations and first installs (no predecessor) use
    /// candidate defaults; removed declarations stay absent.
    /// 既有声明保留其文档；首次声明及首次安装（无前驱）使用候选默认值；移除声明保持缺失。
    /// Returns a value validated against the new package without writing state or merging documents.
    /// 返回按新插件包校验的值，不写入状态或合并文档。
    pub fn configuration_for_update(
        &self,
        candidate: &crate::PinnedVulcanPluginGeneration,
        predecessor: Option<&crate::VulcanPluginDescriptor>,
    ) -> Result<crate::VulcanPluginConfigurationSelection, VulcanPluginRuntimeError> {
        if predecessor.is_some_and(|predecessor| candidate.identity().plugin_id != predecessor.id) {
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
        let declared_predecessor = match predecessor {
            Some(predecessor) => predecessor
                .manifest
                .as_ref()
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new(
                        "Configuration predecessor has no valid manifest.",
                    )
                })?
                .config
                .is_some()
                .then_some(predecessor),
            None => None,
        };
        // Newly declared configuration uses this candidate's defaults exactly once.
        // 新声明配置仅使用此候选默认值一次。
        let value = if let Some(predecessor) = declared_predecessor {
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
    /// Managed packages read the settings their descriptor mirrors; bundled packages read the current settings
    /// section of their record, or the package defaults. Every value is validated against the descriptor's
    /// package schema without looking up a newer package generation.
    /// 受管插件包读取其描述符镜像的设置；内建插件包读取其记录的当前设置段或插件包默认值。
    /// 每个值都按描述符的插件包 Schema 校验，不查找更新的包代次。
    pub(super) fn plugin_config_for_snapshot(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let config = declared_configuration(plugin)?;
        let package_root = self.configuration_package_root(plugin)?;
        let value = match configuration_owner(plugin)? {
            // The descriptor mirrors its record's settings, including a candidate's not yet published target;
            // settings edits republish the descriptor, so later calls observe them.
            // 描述符镜像其记录的设置，包括尚未发布的候选目标；设置编辑会重新发布描述符，后续调用即可观察到。
            ConfigurationOwner::Managed(managed) => {
                managed.configuration().value().cloned().ok_or_else(|| {
                    VulcanPluginRuntimeError::new(
                        "Managed plugin configuration is explicitly undeclared.",
                    )
                })?
            }
            ConfigurationOwner::Bundled => match self.read_settings_record(&plugin.id)? {
                // Without a record the package defaults apply in memory; nothing is written.
                // 没有记录时在内存中使用插件包默认值；不写入任何内容。
                None => read_configuration_defaults(&package_root, config)?,
                Some(record)
                    if record.package() == &crate::VulcanPluginPackageSelection::Bundled {} =>
                {
                    match record.configuration() {
                        crate::VulcanPluginConfigurationSelection::Document { value } => {
                            value.clone()
                        }
                        crate::VulcanPluginConfigurationSelection::PackageDefaults {} => {
                            read_configuration_defaults(&package_root, config)?
                        }
                        crate::VulcanPluginConfigurationSelection::NotDeclared {} => {
                            return Err(VulcanPluginRuntimeError::new(
                                "Bundled plugin settings hold no configuration document.",
                            ));
                        }
                    }
                }
                Some(_) => {
                    return Err(VulcanPluginRuntimeError::new(
                        "Bundled plugin settings are owned by another package selection.",
                    ));
                }
            },
        };
        // Only compilation is reused; every freshly read configuration remains subject to validation.
        // 仅复用编译；每份刚读取配置仍必须接受校验。
        let schema_path =
            canonical_plugin_file(&package_root, &config.schema_file, "configuration schema")
                .map_err(VulcanPluginRuntimeError::new)?;
        let schema = read_plugin_json_state(&schema_path, "configuration schema")?;
        let validator = self.configuration_schema_validator(plugin, schema_path, schema)?;
        validator.validate(&value).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Invalid plugin configuration; reset it to the package defaults: {error}"
            ))
        })?;
        Ok(value)
    }

    /// Return the package root whose schema and defaults govern plugin's configuration.
    /// 返回其 Schema 及默认值约束 plugin 配置的插件包根目录。
    fn configuration_package_root(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
    ) -> Result<PathBuf, VulcanPluginRuntimeError> {
        match plugin.managed_package() {
            Some(managed) => Ok(managed.generation().package_root().to_path_buf()),
            // executionRoot pins configuration defaults and schema to the authorized package generation.
            // executionRoot 把配置默认值与 Schema 固定到已授权插件包代次。
            None => plugin_execution_package_root(&self.state, plugin)
                .map_err(VulcanPluginRuntimeError::new),
        }
    }

    /// Read the current activation record of plugin_id from the user store, without creating the store.
    /// 从用户存储读取 plugin_id 的当前激活记录，不创建存储。
    fn read_settings_record(
        &self,
        plugin_id: &str,
    ) -> Result<Option<crate::VulcanPluginActivation>, VulcanPluginRuntimeError> {
        let Some(store) =
            crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
        else {
            return Ok(None);
        };
        store
            .activation(plugin_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
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
        self.edit_plugin_config_for_snapshot(plugin, Some(value))
    }

    /// Commit value, or the package defaults when it is None, under the same gates as an ordinary update.
    /// 在与普通更新相同的门控下提交 value，为 None 时提交插件包默认值。
    /// A bundled record keeps following the package defaults; other owners store the defaults read now.
    /// 内建记录持续跟随插件包默认值；其他所有者保存此刻读取的默认值。
    fn edit_plugin_config_for_snapshot(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
        value: Option<Value>,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // Configuration mutation shares the existing publication gate.
        // 配置变更共用既有发布门。
        let registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&registration, None)?;
        // Resolve current authority once while publication cannot change it.
        // 发布不能改变权威期间，一次性解析当前权威。
        let snapshot = self.snapshot();
        // Retain the original plugin ID.
        // 保留原插件 ID。
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
        let follows_defaults = value.is_none();
        let value = match value {
            Some(value) => value,
            None => read_configuration_defaults(
                &self.configuration_package_root(current)?,
                declared_configuration(current)?,
            )?,
        };
        match configuration_owner(current)? {
            ConfigurationOwner::Managed(managed) => {
                self.validate_settings_value(current, &value)?;
                let committed = self.reconfigure_managed(current, managed, value.clone())?;
                let reconfigured = current
                    .with_reconfigured_settings(&committed, None)
                    .map_err(VulcanPluginRuntimeError::new)?;
                self.publish_settings_edit(plugin_id, Some(reconfigured))?;
                Ok(value)
            }
            ConfigurationOwner::Bundled => {
                self.validate_settings_value(current, &value)?;
                let configuration = if follows_defaults {
                    crate::VulcanPluginConfigurationSelection::PackageDefaults {}
                } else {
                    crate::VulcanPluginConfigurationSelection::Document {
                        value: value.clone(),
                    }
                };
                self.configure_bundled(plugin_id, configuration)?;
                self.publish_settings_edit(plugin_id, None)?;
                Ok(value)
            }
        }
    }

    /// Validate value against the schema of plugin's package before it becomes settings.
    /// 在 value 成为设置之前，按 plugin 插件包的 Schema 校验它。
    fn validate_settings_value(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
        value: &Value,
    ) -> Result<(), VulcanPluginRuntimeError> {
        validate_plugin_config_value(
            &self.configuration_package_root(plugin)?,
            declared_configuration(plugin)?,
            value,
        )
    }

    /// Commit value as the managed plugin's configuration while its record still selects managed's package.
    /// 受管插件记录仍选择 managed 的插件包时，将 value 提交为其配置。
    /// Returns the committed record; a concurrent edit or transaction surfaces as an error.
    /// 返回已提交记录；并发编辑或交易以错误形式暴露。
    fn reconfigure_managed(
        &self,
        plugin: &crate::VulcanPluginDescriptor,
        managed: &crate::VulcanPluginManagedPackage,
        value: Value,
    ) -> Result<crate::VulcanPluginActivation, VulcanPluginRuntimeError> {
        let store = crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Managed plugin store is missing."))?;
        let settings_revision = store
            .activation(&plugin.id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .map_or(0, |record| record.settings().revision);
        store
            .reconfigure(
                &plugin.id,
                managed.activation_revision(),
                settings_revision,
                crate::VulcanPluginSettingsChange {
                    configuration: Some(crate::VulcanPluginConfigurationSelection::Document {
                        value,
                    }),
                    credential_revision: None,
                },
            )
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
    }

    /// Commit value as the bundled plugin's settings, creating its record on the first edit.
    /// 将 value 提交为内建插件的设置，首次编辑时创建其记录。
    fn configure_bundled(
        &self,
        plugin_id: &str,
        configuration: crate::VulcanPluginConfigurationSelection,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // The first edit initializes the same host-selected user root as plugin installation.
        // 首次编辑初始化与插件安装相同的宿主选定用户根目录。
        std::fs::create_dir_all(&self.state.user_plugin_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to create user plugin root: {error}"))
        })?;
        let store = crate::VulcanPluginGenerationStore::open(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        let settings_revision = store
            .activation(plugin_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .map_or(0, |record| record.settings().revision);
        store
            .configure_bundled(
                plugin_id,
                settings_revision,
                crate::VulcanPluginSettingsChange {
                    configuration: Some(configuration),
                    credential_revision: None,
                },
            )
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        Ok(())
    }

    /// Publish a new catalog revision after a committed settings edit and retire reusable domains of plugin_id.
    /// 设置编辑提交后发布新目录修订，并退役 plugin_id 的可复用域。
    /// Returns the published snapshot.
    /// 返回已发布快照。
    /// `reconfigured` replaces a descriptor that mirrors settings or credentials; others read settings live.
    /// `reconfigured` 替换镜像设置或凭据的描述符；其他描述符实时读取设置。
    pub(super) fn publish_settings_edit(
        &self,
        plugin_id: &str,
        reconfigured: Option<crate::VulcanPluginDescriptor>,
    ) -> Result<Arc<crate::VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        let mut published = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        let mut next_snapshot = (**published).clone();
        if let Some(descriptor) = reconfigured {
            // Enabled MCP entries are copies of the descriptor's servers, including configuration and credentials.
            // 已启用 MCP 条目是描述符服务器的副本，包含配置及凭据。
            for server in &descriptor.mcp_servers {
                if let Some(entry) = next_snapshot
                    .enabled_mcp_servers
                    .get_mut(&format!("{}:{}", server.plugin_id, server.id))
                {
                    *entry = server.clone();
                }
            }
            next_snapshot
                .plugins
                .insert(plugin_id.to_owned(), descriptor);
        }
        next_snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let next_snapshot = Arc::new(next_snapshot);
        *published = Arc::clone(&next_snapshot);
        drop(published);
        // Already-running calls drain on their original VM; no later call reuses pre-update state.
        // 已运行调用在原 VM 中排空；后续调用不复用更新前状态。
        self.close_reusable_domains(Some(plugin_id), None, false)?;
        Ok(next_snapshot)
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

    /// Replace the plugin's configuration with its package defaults through the ordinary edit path.
    /// 通过普通编辑路径将插件配置替换为其插件包默认值。
    /// This is the explicit recovery when stored settings no longer satisfy an upgraded schema; returns the defaults.
    /// 这是已存设置不再满足升级后 Schema 时的显式恢复；返回默认值。
    pub fn reset_plugin_config(&self, plugin_id: &str) -> Result<Value, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        self.edit_plugin_config_for_snapshot(plugin, None)
    }
}

/// Which authority stores a plugin's configuration.
/// 哪个权威保存插件配置。
pub(super) enum ConfigurationOwner<'a> {
    /// The settings section of the managed package's activation record, mirrored by its descriptor.
    /// 受管插件包激活记录的设置段，由其描述符镜像。
    Managed(&'a crate::VulcanPluginManagedPackage),
    /// The settings section of a bundled record, or the package defaults while no record exists.
    /// 内建记录的设置段；尚无记录时为插件包默认值。
    Bundled,
}

/// Classify plugin by the authority that owns its configuration.
/// 按拥有其配置的权威对 plugin 分类。
/// A user row without a managed selection is an unavailable diagnostic row and owns no settings.
/// 没有受管选择的用户条目是不可用诊断条目，不拥有设置。
pub(super) fn configuration_owner(
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<ConfigurationOwner<'_>, VulcanPluginRuntimeError> {
    match (plugin.managed_package(), plugin.origin) {
        (Some(managed), _) => Ok(ConfigurationOwner::Managed(managed)),
        (None, crate::VulcanPluginPackageOrigin::Bundled) => Ok(ConfigurationOwner::Bundled),
        (None, crate::VulcanPluginPackageOrigin::UserInstalled) => {
            Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no managed package selection.",
                plugin.id
            )))
        }
    }
}

/// Return plugin's configuration declaration, rejecting packages that declare none.
/// 返回 plugin 的配置声明，拒绝未声明配置的插件包。
fn declared_configuration(
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<&crate::VulcanPluginConfigManifest, VulcanPluginRuntimeError> {
    let plugin_id = &plugin.id;
    plugin
        .manifest
        .as_ref()
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?
        .config
        .as_ref()
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` does not declare configuration."
            ))
        })
}

/// Read the package defaults document declared by config beneath package_root.
/// 读取 package_root 下 config 声明的插件包默认值文档。
fn read_configuration_defaults(
    package_root: &Path,
    config: &crate::VulcanPluginConfigManifest,
) -> Result<Value, VulcanPluginRuntimeError> {
    // The package validator remains authoritative for relative paths and reparse-point exclusion.
    // 插件包校验器继续对相对路径及重解析点排除具有权威。
    let defaults = canonical_plugin_file(
        package_root,
        &config.defaults_file,
        "configuration defaults",
    )
    .map_err(VulcanPluginRuntimeError::new)?;
    read_plugin_json_state(&defaults, "configuration defaults")
}

#[cfg(test)]
mod tests;
