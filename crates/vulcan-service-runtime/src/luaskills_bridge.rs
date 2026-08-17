//! Minimal LuaSkills bootstrap used by the Core Service runtime.
//! Core Service 运行时使用的最小 LuaSkills 引导模块。

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use crate::managed_runtime_bootstrap::{
    ManagedRuntimeBootstrapProgress, ManagedRuntimeBootstrapReport, ManagedRuntimeInstallTarget,
    bootstrap_managed_runtimes, bootstrap_managed_runtimes_for_target,
};
use luaskills::runtime_options::LuaRuntimeRunLuaPoolConfig;
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaRuntimeCapabilityOptions, LuaRuntimeDatabaseProviderMode,
    LuaRuntimeHostOptions, LuaRuntimeManagedRuntimeConfig, LuaRuntimeSpaceControllerProcessMode,
    LuaVmPoolConfig, RuntimeEntryDescriptor, RuntimeSkillConfigEventBatch,
    RuntimeSkillConfigStoreRefresh, RuntimeSkillPackageConfigDescriptor,
    RuntimeSkillPackageConfigItemDescriptor, RuntimeSkillPackageConfigStatus, RuntimeSkillRoot,
    SkillConfigDeleteResult, SkillConfigWriteResult, SkillInstallRequest, SkillInstallSourceType,
    SkillManagementAuthority, SkillManager, SkillManagerConfig, SkillMeta,
    SkillPackageConfigInputValue, SkillPackageConfigItemState, SkillUninstallOptions,
    ToolCacheConfig,
};
use vulcan_luaskills_config::{
    LuaSkillsRuntimeSettings, load_runtime_settings, runtime_settings_file_path,
    save_runtime_settings,
};
use vulcan_provider_config::config::{
    PersistedControllerConfiguration, PersistedControllerProcessMode,
};

/// Declarative bootstrap configuration for host-managed ROOT skills.
/// 宿主管理 ROOT 技能使用的声明式引导配置。
#[derive(Debug, Clone, serde::Deserialize)]
struct RootSkillBootstrapConfig {
    /// Ordered ROOT skill declarations that must exist inside the system-owned ROOT layer.
    /// 必须存在于系统自有 ROOT 层中的有序技能声明集合。
    root_skills: Vec<RootSkillBootstrapEntry>,
}

/// One host-managed ROOT skill declaration loaded from the bootstrap config file.
/// 从引导配置文件加载的单条宿主管理 ROOT 技能声明。
#[derive(Debug, Clone, serde::Deserialize)]
struct RootSkillBootstrapEntry {
    /// Stable skill identifier expected after installation completes.
    /// 安装完成后预期存在的稳定技能标识符。
    skill_id: String,
    /// Source locator consumed by the LuaSkills managed-install pipeline.
    /// 供 LuaSkills 受管安装链消费的来源定位值。
    source: String,
    /// Managed-install source type used to interpret the source locator.
    /// 用于解释来源定位值的受管安装来源类型。
    source_type: RootSkillBootstrapSourceType,
}

/// Supported source-type values for host-managed ROOT bootstrap skills.
/// 宿主管理 ROOT 引导技能支持的来源类型取值。
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
enum RootSkillBootstrapSourceType {
    /// Install from one GitHub repository source.
    /// 从某个 GitHub 仓库来源安装。
    Github,
    /// Install from one direct package/source URL.
    /// 从某个直接包地址或 source URL 安装。
    Url,
}

impl RootSkillBootstrapSourceType {
    /// Converts the declarative bootstrap source type into the upstream LuaSkills source enum.
    /// 将声明式引导来源类型转换为上游 LuaSkills 来源枚举。
    fn into_runtime_source_type(self) -> SkillInstallSourceType {
        match self {
            Self::Github => SkillInstallSourceType::Github,
            Self::Url => SkillInstallSourceType::Url,
        }
    }
}

/// LuaSkillConfigCapabilityCache remembers package versions that have no declared configuration items.
/// LuaSkillConfigCapabilityCache 记录已确认没有配置声明项的技能包版本。
#[derive(Debug, Default)]
struct LuaSkillConfigCapabilityCache {
    /// Latest configless version observed for each effective skill identifier.
    /// 每个生效技能标识最近一次确认无配置声明的版本。
    configless_versions: BTreeMap<String, String>,
}

impl LuaSkillConfigCapabilityCache {
    /// Returns whether one exact skill version is already known to have no configuration declarations.
    /// 返回某个精确技能版本是否已确认没有配置声明。
    ///
    /// `skill_id` is the effective package identifier and `version` is its manifest version.
    /// `skill_id` 是生效技能包标识，`version` 是其清单版本。
    ///
    /// Returns true only for an exact negative-cache hit.
    /// 仅在精确命中负缓存时返回 true。
    fn is_known_configless(&self, skill_id: &str, version: &str) -> bool {
        self.configless_versions
            .get(skill_id)
            .is_some_and(|cached_version| cached_version == version)
    }

    /// Records the authoritative declaration probe for one exact skill version.
    /// 记录某个精确技能版本的权威声明探测结果。
    ///
    /// `has_configuration` is true when at least one configuration item is declared.
    /// `has_configuration` 在至少声明一个配置项时为 true。
    ///
    /// Positive probes remove stale negative entries; negative probes replace the remembered version.
    /// 正向探测会移除过期负缓存；负向探测会替换已记录版本。
    fn record_probe(&mut self, skill_id: &str, version: &str, has_configuration: bool) {
        if has_configuration {
            self.configless_versions.remove(skill_id);
        } else {
            self.configless_versions
                .insert(skill_id.to_string(), version.to_string());
        }
    }

    /// Invalidates every negative result associated with one skill identifier.
    /// 使某个技能标识关联的所有负向结果失效。
    ///
    /// `skill_id` identifies the package changed by a successful management operation.
    /// `skill_id` 标识被成功管理操作修改的技能包。
    ///
    /// Returns whether a cached entry was removed.
    /// 返回是否移除了缓存条目。
    fn invalidate(&mut self, skill_id: &str) -> bool {
        self.configless_versions.remove(skill_id).is_some()
    }
}

/// Long-lived LuaSkills handle retained by the service runtime.
/// 由服务运行时保留的长生命周期 LuaSkills 句柄。
#[derive(Clone)]
pub struct TauriLuaSkillsHandle {
    /// Shared LuaSkills engine kept alive for later plugin panels and hooks.
    /// 为后续插件面板与 hook 保持存活的共享 LuaSkills 引擎。
    pub engine: Arc<Mutex<LuaEngine>>,
    /// Runtime root resolved from the prepared Vulcan output bundle.
    /// 从准备好的 Vulcan output 包解析出的运行时根目录。
    pub runtime_root: PathBuf,
    /// Host-owned shared root containing managed Python and Node distributions.
    /// 宿主持有的共享根目录，包含受管 Python 与 Node 发行包。
    pub managed_runtime_distribution_root: PathBuf,
    /// LuaSkills-owned writable root containing reusable managed environments.
    /// LuaSkills 持有的可写根目录，包含可复用受管环境。
    pub managed_runtime_environment_root: PathBuf,
    /// Output-owned Vulcan configuration directory shared by both LuaSkills engines.
    /// 两个 LuaSkills 引擎共享的 output 自有 Vulcan 配置目录。
    pub output_config_directory: PathBuf,
    /// Mounted ROOT and USER skill roots.
    /// 已挂载的 ROOT 与 USER 技能根目录。
    pub skill_roots: Vec<RuntimeSkillRoot>,
    /// Result of the application-start managed Python and Node bootstrap.
    /// 应用启动阶段受管 Python 与 Node 初始化结果。
    pub managed_runtime_report: Arc<Mutex<ManagedRuntimeBootstrapReport>>,
    /// Current native managed-runtime installation progress during an active attempt.
    /// 活动尝试期间当前的原生受管运行时安装进度。
    pub managed_runtime_progress: Arc<Mutex<Option<ManagedRuntimeBootstrapProgress>>>,
    /// Process-local negative cache for package versions without configuration declarations.
    /// 进程内技能包版本无配置声明负缓存。
    config_capability_cache: Arc<Mutex<LuaSkillConfigCapabilityCache>>,
    /// Runtime settings captured when the retained engine was created.
    /// 创建保留引擎时捕获的运行时设置。
    pub effective_runtime_settings: LuaSkillsRuntimeSettings,
}

impl std::fmt::Debug for TauriLuaSkillsHandle {
    /// Formats the LuaSkills handle without dumping opaque engine internals.
    /// 在不输出不透明引擎内部状态的前提下格式化 LuaSkills 句柄。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TauriLuaSkillsHandle")
            .field("runtime_root", &self.runtime_root)
            .field(
                "managed_runtime_distribution_root",
                &self.managed_runtime_distribution_root,
            )
            .field(
                "managed_runtime_environment_root",
                &self.managed_runtime_environment_root,
            )
            .field("output_config_directory", &self.output_config_directory)
            .field("skill_roots", &self.skill_roots)
            .field("managed_runtime_report", &self.managed_runtime_report)
            .field("managed_runtime_progress", &self.managed_runtime_progress)
            .field("config_capability_cache", &self.config_capability_cache)
            .field(
                "effective_runtime_settings",
                &self.effective_runtime_settings,
            )
            .finish()
    }
}

/// Lightweight LuaSkills catalog exposed to the WebUI snapshot.
/// 暴露给 WebUI 快照的轻量 LuaSkills 目录。
#[derive(Debug, Clone)]
pub struct TauriLuaSkillsCatalog {
    /// Runtime root used by the embedded LuaSkills host.
    /// 嵌入式 LuaSkills 宿主使用的运行时根目录。
    pub runtime_root: PathBuf,
    /// Host-owned shared root containing managed Python and Node distributions.
    /// 宿主持有的共享根目录，包含受管 Python 与 Node 发行包。
    pub managed_runtime_distribution_root: PathBuf,
    /// LuaSkills-owned writable root containing reusable managed environments.
    /// LuaSkills 持有的可写根目录，包含可复用受管环境。
    pub managed_runtime_environment_root: PathBuf,
    /// Latest managed Python and Node bootstrap result retained across explicit retries.
    /// 在显式重试之间保留的最近一次受管 Python 与 Node 初始化结果。
    pub managed_runtime_report: ManagedRuntimeBootstrapReport,
    /// Current native installation progress when a bootstrap or retry is active.
    /// 初始化或重试进行中时的当前原生安装进度。
    pub managed_runtime_progress: Option<ManagedRuntimeBootstrapProgress>,
    /// System Lua library directory used by the embedded host.
    /// 嵌入式宿主使用的系统 Lua 库目录。
    pub system_lua_lib_dir: PathBuf,
    /// Effective skill rows resolved from mounted skill roots.
    /// 从已挂载技能根解析出的生效技能行。
    pub skills: Vec<TauriLuaSkillSummary>,
    /// Total visible tool entry count.
    /// 可见工具入口总数。
    pub entry_count: usize,
    /// Vulcan-owned runtime settings currently saved on disk.
    /// 当前保存在磁盘上的 Vulcan 自有运行时设置。
    pub runtime_settings: LuaSkillsRuntimeSettings,
    /// Exact Vulcan-owned runtime settings file path.
    /// 精确的 Vulcan 自有运行时设置文件路径。
    pub runtime_settings_file: PathBuf,
    /// Whether saved settings differ from the running engine configuration.
    /// 已保存设置是否不同于运行中引擎配置。
    pub restart_required: bool,
}

/// Lightweight summary for one effective LuaSkill package.
/// 单个生效 LuaSkill 包的轻量摘要。
#[derive(Debug, Clone)]
pub struct TauriLuaSkillSummary {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub skill_id: String,
    /// Human-facing skill display name.
    /// 面向用户的技能显示名称。
    pub display_name: String,
    /// Skill package version.
    /// 技能包版本。
    pub version: String,
    /// Layer name that owns the effective skill.
    /// 拥有该生效技能的层名称。
    pub root_name: String,
    /// Filesystem directory that owns the skill package.
    /// 拥有该技能包的文件系统目录。
    pub skill_dir: PathBuf,
    /// Whether the skill is currently enabled.
    /// 该技能当前是否已启用。
    pub enabled: bool,
    /// Optional disabled reason captured by the lifecycle manager.
    /// 生命周期管理器捕获的可选停用原因。
    pub disabled_reason: Option<String>,
    /// Whether the skill is backed by a managed install record.
    /// 该技能是否由受管安装记录支撑。
    pub managed: bool,
    /// Optional managed install source type.
    /// 可选受管安装来源类型。
    pub source_type: Option<String>,
    /// Optional managed install source locator.
    /// 可选受管安装来源定位值。
    pub source_locator: Option<String>,
    /// Number of visible tool entries belonging to this skill.
    /// 该技能拥有的可见工具入口数量。
    pub entry_count: usize,
    /// Short capability text assembled from visible tool names.
    /// 根据可见工具名称拼接出的简要能力文本。
    pub capability: String,
    /// Runtime tools exposed by this skill.
    /// 该技能暴露的运行时工具集合。
    pub tools: Vec<TauriLuaSkillToolSummary>,
    /// Whether this skill belongs to the host-owned ROOT layer.
    /// 该技能是否属于宿主自有 ROOT 层。
    pub system_owned: bool,
    /// Optional package configuration declaration and status without effective values.
    /// 不包含有效值的可选技能包配置声明与状态。
    pub config: Option<RuntimeSkillPackageConfigDescriptor>,
}

/// Lightweight runtime tool summary for one LuaSkill package.
/// 单个 LuaSkill 包的轻量运行时工具摘要。
#[derive(Debug, Clone)]
pub struct TauriLuaSkillToolSummary {
    /// Canonical tool name registered by the runtime.
    /// 运行时注册的规范工具名称。
    pub canonical_name: String,
    /// Local tool name inside the skill package.
    /// 技能包内部的本地工具名称。
    pub local_name: String,
    /// Tool description from the runtime descriptor.
    /// 来自运行时描述符的工具说明。
    pub description: String,
}

/// Bootstraps LuaSkills using the controller-backed database mode shared by Vulcan Code hosts.
/// 使用 Vulcan Code 宿主共享的控制器数据库模式引导 LuaSkills。
pub fn bootstrap_luaskills<F, G>(
    controller: PersistedControllerConfiguration,
    mut progress: F,
    mut managed_progress: G,
) -> Result<TauriLuaSkillsHandle, String>
where
    F: FnMut(&str),
    G: FnMut(&ManagedRuntimeBootstrapProgress),
{
    let runtime_root =
        vulcan_runtime_assets::resolve_luaskills_runtime_root().ok_or_else(|| {
            String::from(
                "未找到 LuaSkills 运行时根目录，请先执行 make sync 生成 output/lua_runtime。",
            )
        })?;
    // runtime_root pins every derived host path to one absolute native directory identity while removing Windows verbatim prefixes before the path reaches LuaSkills.
    // runtime_root 将所有派生宿主路径固定到一个绝对原生目录身份，并在路径进入 LuaSkills 前移除 Windows verbatim 前缀。
    let runtime_root = vulcan_runtime_assets::native_host_path(
        &fs::canonicalize(&runtime_root).map_err(|error| {
            format!(
                "规范化 LuaSkills 运行时根 `{}` 失败：{error}",
                runtime_root.display()
            )
        })?,
    );
    progress("正在准备 LuaSkills 运行时目录");
    // output_config_directory is the only runtime configuration root accepted by Vulcan Code.
    // output_config_directory 是 Vulcan Code 唯一接受的运行时配置根目录。
    let output_config_directory = vulcan_runtime_assets::resolve_output_config_directory()
        .ok_or_else(|| String::from("未找到 Vulcan output/config 目录。"))?;
    // runtime_settings is the Vulcan-owned source of truth shared by both host engines.
    // runtime_settings 是两个宿主引擎共享的 Vulcan 自有事实来源。
    let runtime_settings = load_runtime_settings(&output_config_directory)?;
    // managed_runtime_distribution_root is the canonical host-owned public runtime directory.
    // managed_runtime_distribution_root 是宿主持有的规范公共运行时目录。
    let managed_runtime_distribution_root =
        vulcan_runtime_assets::resolve_managed_runtime_distribution_root()
            .ok_or_else(|| String::from("未找到 Vulcan 公共 Python/Node 发行根目录。"))?;
    fs::create_dir_all(&managed_runtime_distribution_root).map_err(|error| {
        format!(
            "创建公共 Python/Node 发行根 `{}` 失败：{error}",
            managed_runtime_distribution_root.display()
        )
    })?;
    // managed_runtime_distribution_root uses the same native-path boundary required by LuaSkills host options.
    // managed_runtime_distribution_root 使用 LuaSkills 宿主选项要求的同一原生路径边界。
    let managed_runtime_distribution_root = vulcan_runtime_assets::native_host_path(
        &fs::canonicalize(&managed_runtime_distribution_root).map_err(|error| {
            format!(
                "规范化公共 Python/Node 发行根 `{}` 失败：{error}",
                managed_runtime_distribution_root.display()
            )
        })?,
    );
    let skill_roots = ensure_runtime_root_layout(&runtime_root)?;
    // skill_config_root is the single host-owned package configuration root shared by every LuaSkills engine.
    // skill_config_root 是所有 LuaSkills 引擎共享的唯一宿主持有技能包配置根目录。
    let skill_config_root =
        vulcan_workspace_store::data_plane::application_luaskills_config_root()?;
    let managed_runtime_report = bootstrap_managed_runtimes(
        &runtime_root,
        &managed_runtime_distribution_root,
        |event: &ManagedRuntimeBootstrapProgress| {
            progress(&event.message);
            managed_progress(event);
        },
    );
    progress(&format!(
        "受管 Python/Node 运行时：{}",
        managed_runtime_report.diagnostic
    ));
    let host_options = build_host_options(
        &runtime_root,
        &managed_runtime_distribution_root,
        &skill_config_root,
        controller,
        &runtime_settings,
    );
    let pool_config = LuaVmPoolConfig {
        min_size: runtime_settings.vm_pool.min_size,
        max_size: runtime_settings.vm_pool.max_size,
        idle_ttl_secs: runtime_settings.vm_pool.idle_ttl_secs,
    };
    let mut engine = LuaEngine::new(LuaEngineOptions::new(pool_config, host_options))
        .map_err(|error| format!("创建 LuaSkills 引擎失败：{error}"))?;
    ensure_configured_root_skills_installed(&skill_roots, &mut engine, &mut progress)?;
    progress("正在加载 LuaSkills 技能目录");
    engine
        .load_from_roots(&skill_roots)
        .map_err(|error| format!("加载 LuaSkills 技能根失败：{error}"))?;

    // managed_runtime_environment_root keeps writable environments inside LuaSkills-owned data.
    // managed_runtime_environment_root 将可写环境保留在 LuaSkills 自有数据目录内。
    let managed_runtime_environment_root = runtime_root.join("dependencies").join("envs");
    Ok(TauriLuaSkillsHandle {
        engine: Arc::new(Mutex::new(engine)),
        runtime_root,
        managed_runtime_distribution_root,
        managed_runtime_environment_root,
        output_config_directory,
        skill_roots,
        managed_runtime_report: Arc::new(Mutex::new(managed_runtime_report)),
        managed_runtime_progress: Arc::new(Mutex::new(None)),
        config_capability_cache: Arc::new(Mutex::new(LuaSkillConfigCapabilityCache::default())),
        effective_runtime_settings: runtime_settings,
    })
}

/// Loads a read-only LuaSkills catalog from the retained runtime handle.
/// 从保留的运行时句柄加载只读 LuaSkills 目录。
pub fn load_luaskills_catalog(
    handle: &TauriLuaSkillsHandle,
) -> Result<TauriLuaSkillsCatalog, String> {
    let engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法读取插件目录。"))?;
    build_luaskills_catalog(handle, &engine)
}

/// Describes one effective skill package configuration while enforcing host-side secret masking.
/// 描述单个生效技能包配置，并强制执行宿主侧秘密值遮蔽。
///
/// `handle` owns the live engine and `skill_id` selects one effective package.
/// `handle` 持有活动引擎，`skill_id` 选择一个生效技能包。
///
/// Returns no descriptor for a package without declarations, otherwise returns every non-sensitive value.
/// 对未声明配置的技能包返回空，否则返回包含所有非敏感值的描述。
pub fn describe_luaskill_package_config(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
) -> Result<Option<RuntimeSkillPackageConfigDescriptor>, String> {
    let engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法读取技能包配置。"))?;
    describe_host_safe_package_config(&engine, skill_id)
}

/// Validates one effective skill package configuration without changing persisted values.
/// 校验单个生效技能包配置且不修改持久化值。
///
/// `handle` owns the live engine and `skill_id` selects the package.
/// `handle` 持有活动引擎，`skill_id` 选择目标技能包。
///
/// Returns the authoritative LuaSkills completeness and validity status.
/// 返回 LuaSkills 权威的完整性与合法性状态。
pub fn validate_luaskill_package_config(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
) -> Result<RuntimeSkillPackageConfigStatus, String> {
    let engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法校验技能包配置。"))?;
    engine.validate_skill_package_config(skill_id)
}

/// Atomically updates one non-empty typed package configuration batch using CAS.
/// 使用 CAS 原子更新一组非空类型化技能包配置。
///
/// `expected_revision` must be the revision observed by the editor.
/// `expected_revision` 必须是编辑器观察到的修订号。
///
/// Returns the write result and a freshly projected host-safe descriptor.
/// 返回写入结果与重新投影的宿主安全描述。
pub fn set_luaskill_package_config(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
    values: BTreeMap<String, SkillPackageConfigInputValue>,
    expected_revision: &str,
) -> Result<(SkillConfigWriteResult, RuntimeSkillPackageConfigDescriptor), String> {
    let mut engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法更新技能包配置。"))?;
    let result = engine.set_skill_config_values(skill_id, values, Some(expected_revision))?;
    let descriptor = describe_host_safe_package_config(&engine, skill_id)?
        .ok_or_else(|| format!("技能 `{skill_id}` 未声明技能包配置。"))?;
    Ok((result, descriptor))
}

/// Deletes one explicit or orphaned package configuration value using CAS.
/// 使用 CAS 删除单个显式或遗留技能包配置值。
///
/// `key` identifies the exact persisted key and `expected_revision` protects concurrent changes.
/// `key` 标识精确持久化键，`expected_revision` 保护并发变更。
///
/// Returns the deletion result and a freshly projected host-safe descriptor.
/// 返回删除结果与重新投影的宿主安全描述。
pub fn delete_luaskill_package_config_value(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
    key: &str,
    expected_revision: &str,
) -> Result<(SkillConfigDeleteResult, RuntimeSkillPackageConfigDescriptor), String> {
    let mut engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法删除技能包配置。"))?;
    let result = engine.delete_skill_config_value(skill_id, key, Some(expected_revision))?;
    let descriptor = describe_host_safe_package_config(&engine, skill_id)?
        .ok_or_else(|| format!("技能 `{skill_id}` 未声明技能包配置。"))?;
    Ok((result, descriptor))
}

/// Polls ordered package configuration events after one optional cursor.
/// 轮询一个可选游标之后的有序技能包配置事件。
///
/// `limit` is bounded by LuaSkills and `after_sequence` is engine-local.
/// `limit` 由 LuaSkills 约束，`after_sequence` 仅在当前引擎内有效。
///
/// Returns the authoritative bounded event batch.
/// 返回权威的有界事件批次。
pub fn poll_luaskill_package_config_events(
    handle: &TauriLuaSkillsHandle,
    after_sequence: Option<&str>,
    limit: usize,
) -> Result<RuntimeSkillConfigEventBatch, String> {
    let engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法轮询技能包配置事件。"))?;
    engine.poll_skill_config_events(after_sequence, limit)
}

/// Explicitly refreshes one selected package configuration store or both configured stores.
/// 显式刷新一个选定技能包配置存储或两个已配置存储。
///
/// `handle` owns the live engine and `store_scope` is `skills`, `system-skills`, or absent.
/// `handle` 持有活动引擎，`store_scope` 为 `skills`、`system-skills` 或缺省。
///
/// Returns one authoritative result for each refreshed store.
/// 返回每个已刷新存储对应的权威结果。
pub fn refresh_luaskill_package_config(
    handle: &TauriLuaSkillsHandle,
    store_scope: Option<&str>,
) -> Result<Vec<RuntimeSkillConfigStoreRefresh>, String> {
    let engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法刷新技能包配置。"))?;
    engine.refresh_skill_config(store_scope)
}

/// Builds one host-safe descriptor without ever requesting sensitive effective values.
/// 构建一份宿主安全描述，且绝不请求敏感有效值。
///
/// `engine` supplies the authoritative declaration and value snapshot.
/// `engine` 提供权威声明与值快照。
///
/// Returns at most one effective package descriptor for the requested skill.
/// 对请求技能最多返回一个生效技能包描述。
fn describe_host_safe_package_config(
    engine: &LuaEngine,
    skill_id: &str,
) -> Result<Option<RuntimeSkillPackageConfigDescriptor>, String> {
    let descriptors = engine.describe_skill_package_config(Some(skill_id), false)?;
    let mut descriptor = take_declared_package_config_descriptor(skill_id, descriptors)?;
    if let Some(descriptor) = descriptor.as_mut() {
        for item in &mut descriptor.items {
            item.value = host_safe_effective_config_value(engine, skill_id, item)?;
        }
    }
    Ok(descriptor)
}

/// Selects one effective package descriptor and drops descriptors without declared items.
/// 选择单个生效技能包描述，并丢弃没有声明项的描述。
///
/// `skill_id` identifies the requested package and `descriptors` is the authoritative LuaSkills response.
/// `skill_id` 标识请求的技能包，`descriptors` 是 LuaSkills 的权威响应。
///
/// Returns no descriptor for a package without configuration options and rejects inconsistent duplicates.
/// 对没有配置选项的技能包返回空，并拒绝不一致的重复描述。
fn take_declared_package_config_descriptor(
    skill_id: &str,
    mut descriptors: Vec<RuntimeSkillPackageConfigDescriptor>,
) -> Result<Option<RuntimeSkillPackageConfigDescriptor>, String> {
    let descriptor = descriptors.pop();
    if !descriptors.is_empty() {
        return Err(format!(
            "技能 `{skill_id}` 返回了多个生效配置描述，运行时状态不一致。"
        ));
    }
    Ok(descriptor.filter(|descriptor| !descriptor.items.is_empty()))
}

/// Resolves whether one package manifest declares configuration while reusing stable negative probes.
/// 解析单个技能包清单是否声明配置，并复用稳定的负向探测结果。
///
/// `handle` owns the negative cache, `meta` is the parsed manifest, and `version` is its semantic version.
/// `handle` 持有负缓存，`meta` 是已解析清单，`version` 是其语义版本。
///
/// Returns false for exact negative-cache hits or manifests without declared configuration items.
/// 对精确负缓存命中或没有配置声明项的清单返回 false。
fn package_declares_configuration(
    handle: &TauriLuaSkillsHandle,
    meta: &SkillMeta,
    skill_id: &str,
    version: &str,
) -> Result<bool, String> {
    // known_configless reads the cache under a short lock and releases it before inspecting the manifest.
    // known_configless 在短锁内读取缓存，并在检查清单前释放锁。
    let known_configless = handle
        .config_capability_cache
        .lock()
        .map_err(|_| String::from("LuaSkills 配置能力缓存锁已损坏，无法读取目录。"))?
        .is_known_configless(skill_id, version);
    if known_configless {
        return Ok(false);
    }

    // has_configuration comes directly from the strict package manifest so load failures cannot become false negative cache entries.
    // has_configuration 直接来自严格技能包清单，避免把加载失败写成错误的负缓存。
    let has_configuration = meta.package_config().next().is_some();
    handle
        .config_capability_cache
        .lock()
        .map_err(|_| String::from("LuaSkills 配置能力缓存锁已损坏，无法更新目录。"))?
        .record_probe(skill_id, version, has_configuration);
    Ok(has_configuration)
}

/// Invalidates one skill's configless capability probe after a successful package mutation.
/// 在成功的技能包变更后使单个技能的无配置能力探测失效。
///
/// `handle` owns the shared cache and `skill_id` identifies the changed package.
/// `handle` 持有共享缓存，`skill_id` 标识发生变化的技能包。
///
/// Returns an error only when the shared cache lock is poisoned.
/// 仅当共享缓存锁损坏时返回错误。
fn invalidate_config_capability_cache(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
) -> Result<(), String> {
    handle
        .config_capability_cache
        .lock()
        .map_err(|_| String::from("LuaSkills 配置能力缓存锁已损坏，无法使缓存失效。"))?
        .invalidate(skill_id);
    Ok(())
}

/// Resolves one effective value only when the declaration explicitly permits host disclosure.
/// 仅在声明明确允许宿主披露时解析单个有效值。
///
/// `engine` owns persisted values, `skill_id` selects the package, and `item` is its declaration.
/// `engine` 持有持久化值，`skill_id` 选择技能包，`item` 是对应声明。
///
/// Returns a canonical non-sensitive effective value, or no value for sensitive, missing, or invalid items.
/// 返回规范的非敏感有效值；敏感、缺失或非法配置项返回空值。
fn host_safe_effective_config_value(
    engine: &LuaEngine,
    skill_id: &str,
    item: &RuntimeSkillPackageConfigItemDescriptor,
) -> Result<Option<String>, String> {
    if item.sensitive {
        return Ok(None);
    }
    match item.state {
        SkillPackageConfigItemState::Configured => engine
            .get_skill_config_value(skill_id, &item.key)?
            .map(|value| canonical_configured_config_value(item, &value))
            .transpose(),
        SkillPackageConfigItemState::Default => canonical_default_config_value(item),
        SkillPackageConfigItemState::Unset
        | SkillPackageConfigItemState::Missing
        | SkillPackageConfigItemState::Invalid => Ok(None),
    }
}

/// Normalizes one already-valid non-sensitive persisted value for WebUI disclosure.
/// 为 WebUI 披露规范化一个已经合法的非敏感持久化值。
///
/// `item` is the authoritative declaration and `value` is its raw persisted string.
/// `item` 是权威声明，`value` 是对应原始持久化字符串。
///
/// Returns the canonical runtime string or an explicit state-consistency error.
/// 返回规范运行时字符串，或显式状态一致性错误。
fn canonical_configured_config_value(
    item: &RuntimeSkillPackageConfigItemDescriptor,
    value: &str,
) -> Result<String, String> {
    let canonical = match item.value_type.as_str() {
        "integer" => value
            .parse::<i64>()
            .map(|value| value.to_string())
            .map_err(|error| error.to_string()),
        "float" => value
            .parse::<f64>()
            .map(|value| {
                if value == 0.0 {
                    String::from("0")
                } else {
                    value.to_string()
                }
            })
            .map_err(|error| error.to_string()),
        "boolean" => value
            .parse::<bool>()
            .map(|value| value.to_string())
            .map_err(|error| error.to_string()),
        "string" | "enum" => return Ok(value.to_string()),
        value_type => {
            return Err(format!(
                "CONFIG_DECLARATION_INVALID: configuration '{}' has unknown declared type '{}'",
                item.key, value_type
            ));
        }
    };
    canonical.map_err(|error| {
        format!(
            "CONFIG_VALUE_INVALID: configuration '{}' is marked configured but cannot be normalized: {error}",
            item.key
        )
    })
}

/// Converts one already-validated typed declaration default into its canonical runtime string.
/// 把一个已经校验的类型化声明默认值转换为规范运行时字符串。
///
/// `item` is an immutable declaration descriptor produced by LuaSkills.
/// `item` 是 LuaSkills 生成的不可变声明描述。
///
/// Returns the canonical default or no value when the declaration has no default.
/// 返回规范默认值；声明没有默认值时返回空值。
fn canonical_default_config_value(
    item: &RuntimeSkillPackageConfigItemDescriptor,
) -> Result<Option<String>, String> {
    let Some(value) = item.default.as_ref() else {
        return Ok(None);
    };
    let canonical = match (item.value_type.as_str(), value) {
        ("integer", serde_json::Value::Number(number)) if number.as_i64().is_some() => {
            number.to_string()
        }
        ("float", serde_json::Value::Number(number)) => {
            // parsed is finite because LuaSkills validates manifest defaults before publishing descriptors.
            // parsed 是有限值，因为 LuaSkills 会在发布描述前校验清单默认值。
            let parsed = number.as_f64().ok_or_else(|| {
                format!(
                    "CONFIG_DECLARATION_INVALID: configuration '{}' default is not a finite float",
                    item.key
                )
            })?;
            if parsed == 0.0 {
                String::from("0")
            } else {
                parsed.to_string()
            }
        }
        ("string" | "enum", serde_json::Value::String(text)) => text.clone(),
        ("boolean", serde_json::Value::Bool(flag)) => flag.to_string(),
        _ => {
            return Err(format!(
                "CONFIG_DECLARATION_INVALID: configuration '{}' default does not match declared type '{}'",
                item.key,
                item.value_type.as_str()
            ));
        }
    };
    Ok(Some(canonical))
}

/// Persist Vulcan-owned LuaSkills runtime settings and return the refreshed catalog projection.
/// 持久化 Vulcan 自有 LuaSkills 运行时设置并返回刷新的目录投影。
///
/// `handle` retains the running engine baseline and `settings` is the complete next document.
/// `handle` 保留运行中引擎基线，`settings` 是完整的下一份文档。
/// Returns a catalog whose restart flag compares saved settings with the retained engine settings.
/// 返回目录，其重启标志会比较已保存设置与保留引擎设置。
pub fn save_luaskills_runtime_settings(
    handle: &TauriLuaSkillsHandle,
    settings: &LuaSkillsRuntimeSettings,
) -> Result<TauriLuaSkillsCatalog, String> {
    save_runtime_settings(&handle.output_config_directory, settings)?;
    load_luaskills_catalog(handle)
}

/// Retries managed Python and Node bootstrap and returns the refreshed catalog projection.
/// 重试受管 Python 与 Node 初始化并返回刷新的目录投影。
///
/// `handle` owns the exact runtime and distribution roots used by the running LuaSkills engine.
/// `handle` 持有运行中 LuaSkills 引擎使用的精确运行时根与发行根。
/// `target` selects the exact independently repairable runtime group.
/// `target` 选择精确且可独立修复的运行时组。
/// `progress` receives bounded human-readable progress updates from the shared bootstrap path.
/// `progress` 接收共享初始化路径产生的有界可读进度更新。
/// Returns the catalog carrying the latest ready, installed, unsupported, or unavailable report.
/// 返回携带最近 ready、installed、unsupported 或 unavailable 报告的目录。
pub fn retry_managed_runtimes<F>(
    handle: &TauriLuaSkillsHandle,
    target: ManagedRuntimeInstallTarget,
    mut progress: F,
) -> Result<TauriLuaSkillsCatalog, String>
where
    F: FnMut(&ManagedRuntimeBootstrapProgress),
{
    // report contains the exact outcome produced by the same bootstrap path used during startup.
    // report 包含与启动阶段相同初始化路径产生的精确结果。
    let report = bootstrap_managed_runtimes_for_target(
        &handle.runtime_root,
        &handle.managed_runtime_distribution_root,
        target,
        |event: &ManagedRuntimeBootstrapProgress| {
            if let Ok(mut retained_progress) = handle.managed_runtime_progress.lock() {
                *retained_progress = Some(event.clone());
            }
            progress(event);
        },
    );
    // retained_progress clears before the first fallible terminal-state write so later refreshes cannot resurrect stale progress.
    // retained_progress 在首个可能失败的终态写入前清空，避免后续刷新复活过期进度。
    if let Ok(mut retained_progress) = handle.managed_runtime_progress.lock() {
        *retained_progress = None;
    }
    // retained_report updates the shared handle before any catalog snapshot is rebuilt.
    // retained_report 在重建任何目录快照前更新共享句柄。
    let mut retained_report = handle
        .managed_runtime_report
        .lock()
        .map_err(|_| String::from("受管 Python/Node 初始化状态锁已损坏，无法保存重试结果。"))?;
    *retained_report = report;
    drop(retained_report);
    load_luaskills_catalog(handle)
}

/// Performs one LuaSkills management action and returns the refreshed catalog.
/// 执行一次 LuaSkills 管理动作并返回刷新后的目录。
pub fn refresh_luaskills_catalog(
    handle: &TauriLuaSkillsHandle,
) -> Result<TauriLuaSkillsCatalog, String> {
    let mut engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法刷新插件目录。"))?;
    engine
        .reload_from_roots(&handle.skill_roots)
        .map_err(|error| format!("刷新 LuaSkills 运行时失败：{error}"))?;
    build_luaskills_catalog(handle, &engine)
}

/// Installs one LuaSkills package into the ordinary USER root.
/// 将单个 LuaSkills 包安装到普通 USER 根。
pub fn install_luaskill(
    handle: &TauriLuaSkillsHandle,
    skill_id: Option<String>,
    source: String,
    source_type: String,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法安装技能。"))?;
    let user_root = ordinary_user_root(handle)?;
    let request = SkillInstallRequest {
        skill_id: normalize_optional_field(skill_id),
        source: normalize_optional_field(Some(source)),
        source_type: parse_skill_install_source_type(&source_type)?,
    };
    let result = engine
        .install_skill_in_root(&handle.skill_roots, &user_root, &request)
        .map_err(|error| format!("安装 LuaSkills 技能失败：{error}"))?;
    invalidate_config_capability_cache(handle, &result.skill_id)?;
    let catalog = reload_catalog_after_management(handle, &mut engine)?;
    Ok((result.message, catalog))
}

/// Updates one managed LuaSkills package in its owning root.
/// 在所属根中更新单个受管 LuaSkills 包。
pub fn update_luaskill(
    handle: &TauriLuaSkillsHandle,
    skill_id: String,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法更新技能。"))?;
    let target_root = target_root_for_skill(handle, &skill_id)?;
    let request = SkillInstallRequest {
        skill_id: Some(skill_id),
        source: None,
        source_type: SkillInstallSourceType::Github,
    };
    let result = if target_root.name.eq_ignore_ascii_case("ROOT") {
        engine
            .system_update_skill_in_root(
                &handle.skill_roots,
                &target_root,
                SkillManagementAuthority::System,
                &request,
            )
            .map_err(|error| format!("更新 LuaSkills 技能失败：{error}"))?
    } else {
        engine
            .update_skill_in_root(&handle.skill_roots, &target_root, &request)
            .map_err(|error| format!("更新 LuaSkills 技能失败：{error}"))?
    };
    invalidate_config_capability_cache(handle, &result.skill_id)?;
    let catalog = reload_catalog_after_management(handle, &mut engine)?;
    Ok((result.message, catalog))
}

/// Enables or disables one LuaSkills package in its owning root.
/// 在所属根中启用或停用单个 LuaSkills 包。
pub fn set_luaskill_enabled(
    handle: &TauriLuaSkillsHandle,
    skill_id: String,
    enabled: bool,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法切换技能状态。"))?;
    let target_root = target_root_for_skill(handle, &skill_id)?;
    if enabled {
        if target_root.name.eq_ignore_ascii_case("ROOT") {
            engine
                .system_enable_skill(
                    &handle.skill_roots,
                    SkillManagementAuthority::System,
                    &skill_id,
                )
                .map_err(|error| format!("启用 LuaSkills 技能失败：{error}"))?;
        } else {
            engine
                .enable_skill(&handle.skill_roots, &skill_id)
                .map_err(|error| format!("启用 LuaSkills 技能失败：{error}"))?;
        }
    } else if target_root.name.eq_ignore_ascii_case("ROOT") {
        engine
            .system_disable_skill_in_roots(
                &handle.skill_roots,
                SkillManagementAuthority::System,
                &skill_id,
                Some("disabled from vulcan-code-web"),
            )
            .map_err(|error| format!("停用 LuaSkills 技能失败：{error}"))?;
    } else {
        engine
            .disable_skill_in_roots(
                &handle.skill_roots,
                &skill_id,
                Some("disabled from vulcan-code-web"),
            )
            .map_err(|error| format!("停用 LuaSkills 技能失败：{error}"))?;
    }
    let catalog = reload_catalog_after_management(handle, &mut engine)?;
    let message = if enabled {
        format!("已启用技能 `{skill_id}`。")
    } else {
        format!("已停用技能 `{skill_id}`。")
    };
    Ok((message, catalog))
}

/// Uninstalls one ordinary LuaSkills package from its owning root.
/// 从所属根卸载单个普通 LuaSkills 包。
pub fn uninstall_luaskill(
    handle: &TauriLuaSkillsHandle,
    skill_id: String,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 引擎锁已损坏，无法卸载技能。"))?;
    let target_root = ordinary_uninstall_target_root_for_skill(handle, &skill_id)?;
    let result = engine
        .uninstall_skill_in_root(
            &handle.skill_roots,
            &target_root,
            &skill_id,
            &SkillUninstallOptions::default(),
        )
        .map_err(|error| format!("卸载 LuaSkills 技能失败：{error}"))?;
    invalidate_config_capability_cache(handle, &result.skill_id)?;
    let catalog = reload_catalog_after_management(handle, &mut engine)?;
    Ok((result.message, catalog))
}

/// Builds a full WebUI LuaSkills catalog from the retained runtime engine.
/// 从保留的运行时引擎构建完整 WebUI LuaSkills 目录。
fn build_luaskills_catalog(
    handle: &TauriLuaSkillsHandle,
    engine: &LuaEngine,
) -> Result<TauriLuaSkillsCatalog, String> {
    let visible_entries = engine
        .list_entries_for_authority(SkillManagementAuthority::System)
        .map_err(|error| format!("读取 LuaSkills 宿主可见工具失败：{error}"))?;
    let mut tools_by_skill = visible_entries.into_iter().fold(
        std::collections::BTreeMap::<String, Vec<TauriLuaSkillToolSummary>>::new(),
        |mut grouped_tools, entry: RuntimeEntryDescriptor| {
            grouped_tools
                .entry(entry.skill_id.clone())
                .or_default()
                .push(TauriLuaSkillToolSummary {
                    canonical_name: entry.canonical_name,
                    local_name: entry.local_name,
                    description: entry.description,
                });
            grouped_tools
        },
    );

    let resolved_skills = collect_effective_skill_instances_from_roots(&handle.skill_roots)
        .map_err(|error| format!("解析 LuaSkills 生效技能目录失败：{error}"))?;
    let mut skills = Vec::with_capacity(resolved_skills.len());
    for resolved in resolved_skills {
        let meta = read_skill_meta_from_directory(&resolved.actual_dir, &resolved.skill_id)?;
        let manager = build_skill_manager_for_root(handle, &resolved.root_name)?;
        let enabled = manager
            .is_skill_enabled(&resolved.skill_id)
            .map_err(|error| format!("读取技能启用状态失败（{}）：{error}", resolved.skill_id))?;
        let disabled_reason = manager
            .disabled_record(&resolved.skill_id)
            .map_err(|error| format!("读取技能停用记录失败（{}）：{error}", resolved.skill_id))?
            .and_then(|record| record.reason);
        let install_record = manager
            .install_record(&resolved.skill_id)
            .map_err(|error| format!("读取技能安装记录失败（{}）：{error}", resolved.skill_id))?;
        let (managed, source_type, source_locator) = match install_record {
            Some(record) => (
                record.managed,
                Some(skill_install_source_type_label(record.source.source_type).to_string()),
                Some(record.source.locator),
            ),
            None => (false, None, None),
        };
        let mut tools = tools_by_skill
            .remove(&resolved.skill_id)
            .unwrap_or_default();
        tools.sort_by(|left, right| {
            left.local_name
                .cmp(&right.local_name)
                .then_with(|| left.canonical_name.cmp(&right.canonical_name))
        });
        let capability = if tools.is_empty() {
            String::from("未暴露可见工具入口")
        } else {
            tools
                .iter()
                .map(|tool| tool.local_name.as_str())
                .collect::<Vec<_>>()
                .join(" / ")
        };
        let system_owned = resolved.root_name.eq_ignore_ascii_case("ROOT");
        let version = meta.version().to_string();
        let declares_configuration =
            package_declares_configuration(handle, &meta, &resolved.skill_id, &version)?;
        // config is probed only for enabled packages because LuaSkills registers effective declarations only for loaded skills.
        // config 仅对已启用技能包执行探测，因为 LuaSkills 只为已加载技能注册生效配置声明。
        let config = if enabled && declares_configuration {
            let descriptors = engine
                .describe_skill_package_config(Some(&resolved.skill_id), false)
                .map_err(|error| {
                    format!("读取技能包配置状态失败（{}）：{error}", resolved.skill_id)
                })?;
            take_declared_package_config_descriptor(&resolved.skill_id, descriptors)?
        } else {
            None
        };
        skills.push(TauriLuaSkillSummary {
            skill_id: resolved.skill_id,
            display_name: meta.name,
            version,
            root_name: resolved.root_name,
            skill_dir: vulcan_runtime_assets::native_host_path(&resolved.actual_dir),
            enabled,
            disabled_reason,
            managed,
            source_type,
            source_locator,
            entry_count: tools.len(),
            capability,
            tools,
            system_owned,
            config,
        });
    }
    skills.sort_by(|left, right| {
        left.root_name
            .cmp(&right.root_name)
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });
    // runtime_settings is loaded once so the projected values and restart flag share one file image.
    // runtime_settings 仅加载一次，使投影值与重启标志共享同一文件镜像。
    let runtime_settings = load_runtime_settings(&handle.output_config_directory)?;
    // restart_required compares persisted startup inputs with the immutable running-engine baseline.
    // restart_required 比较持久化启动输入与不可变的运行中引擎基线。
    let restart_required =
        runtime_settings.requires_engine_restart_from(&handle.effective_runtime_settings);
    // managed_runtime_report is cloned once so every projected field belongs to one completed attempt.
    // managed_runtime_report 仅克隆一次，使所有投影字段属于同一次已完成尝试。
    let managed_runtime_report = handle
        .managed_runtime_report
        .lock()
        .map_err(|_| String::from("受管 Python/Node 初始化状态锁已损坏，无法读取目录。"))?
        .clone();
    // managed_runtime_progress is cloned independently because retries update it while the engine remains live.
    // managed_runtime_progress 独立克隆，因为重试会在引擎保持运行时更新它。
    let managed_runtime_progress = handle
        .managed_runtime_progress
        .lock()
        .map_err(|_| String::from("受管 Python/Node 初始化进度锁已损坏，无法读取目录。"))?
        .clone();

    Ok(TauriLuaSkillsCatalog {
        runtime_root: vulcan_runtime_assets::native_host_path(&handle.runtime_root),
        managed_runtime_distribution_root: vulcan_runtime_assets::native_host_path(
            &handle.managed_runtime_distribution_root,
        ),
        managed_runtime_environment_root: vulcan_runtime_assets::native_host_path(
            &handle.managed_runtime_environment_root,
        ),
        managed_runtime_report,
        managed_runtime_progress,
        system_lua_lib_dir: vulcan_runtime_assets::native_host_path(
            &handle.runtime_root.join("system_lua_lib"),
        ),
        entry_count: skills.iter().map(|skill| skill.entry_count).sum(),
        skills,
        runtime_settings,
        runtime_settings_file: vulcan_runtime_assets::native_host_path(
            &runtime_settings_file_path(&handle.output_config_directory),
        ),
        restart_required,
    })
}

/// Ensures the required LuaSkills runtime directory layout exists.
/// 确保 LuaSkills 运行时所需目录布局存在。
fn ensure_runtime_root_layout(runtime_root: &Path) -> Result<Vec<RuntimeSkillRoot>, String> {
    let temp_dir = runtime_root.join("temp");
    let resources_dir = runtime_root.join("resources");
    let lua_packages_dir = runtime_root.join("lua_packages");
    let libs_root = runtime_root.join("libs");
    let bin_root = runtime_root.join("bin");
    let dependencies_dir = runtime_root.join("dependencies");
    let databases_dir = runtime_root.join("databases");
    let config_dir = runtime_root.join("config");
    let state_dir = runtime_root.join("state");
    let download_root = temp_dir.join("downloads");
    let root_skills_dir = runtime_root.join("skills");
    // user_skills_dir is normalized before it becomes a LuaSkills RuntimeSkillRoot.
    // user_skills_dir 在成为 LuaSkills RuntimeSkillRoot 前完成原生路径规范化。
    let user_skills_dir = vulcan_runtime_assets::native_host_path(&default_user_skill_root());
    let system_lua_lib_dir = runtime_root.join("system_lua_lib");

    for directory in [
        runtime_root,
        temp_dir.as_path(),
        resources_dir.as_path(),
        lua_packages_dir.as_path(),
        libs_root.as_path(),
        bin_root.as_path(),
        dependencies_dir.as_path(),
        databases_dir.as_path(),
        config_dir.as_path(),
        state_dir.as_path(),
        download_root.as_path(),
        root_skills_dir.as_path(),
        user_skills_dir.as_path(),
        system_lua_lib_dir.as_path(),
    ] {
        fs::create_dir_all(directory).map_err(|error| {
            format!(
                "创建 LuaSkills 运行时目录失败（{}）：{error}",
                directory.display()
            )
        })?;
    }

    Ok(vec![
        RuntimeSkillRoot {
            name: String::from("ROOT"),
            skills_dir: root_skills_dir,
        },
        RuntimeSkillRoot {
            name: String::from("USER"),
            skills_dir: user_skills_dir,
        },
    ])
}

/// Ensures configured host-managed ROOT skills exist before catalog loading.
/// 在加载技能目录前确保配置声明的宿主管理 ROOT 技能已经存在。
fn ensure_configured_root_skills_installed(
    skill_roots: &[RuntimeSkillRoot],
    engine: &mut LuaEngine,
    progress: &mut impl FnMut(&str),
) -> Result<(), String> {
    let root_target = resolve_named_skill_root(skill_roots, "ROOT")?;
    let configured_skills = load_root_skill_bootstrap_entries()?;
    if configured_skills.is_empty() {
        progress("未配置需要自动安装的 LuaSkills ROOT 技能");
        return Ok(());
    }

    let installed_root_skill_ids =
        collect_effective_skill_instances_from_roots(std::slice::from_ref(&root_target))
            .map_err(|error| format!("解析 ROOT 层技能目录失败：{error}"))?
            .into_iter()
            .map(|instance| instance.skill_id)
            .collect::<std::collections::BTreeSet<_>>();

    let mut installed_any_skill = false;
    for configured_skill in configured_skills {
        if installed_root_skill_ids.contains(configured_skill.skill_id.as_str()) {
            continue;
        }

        let install_message = format!("正在安装 LuaSkills 技能：{}", configured_skill.skill_id);
        progress(&install_message);
        let install_request = SkillInstallRequest {
            skill_id: Some(configured_skill.skill_id.clone()),
            source: Some(configured_skill.source.clone()),
            source_type: configured_skill.source_type.into_runtime_source_type(),
        };
        engine
            .system_install_skill_in_root(
                skill_roots,
                &root_target,
                SkillManagementAuthority::System,
                &install_request,
            )
            .map_err(|error| {
                format!(
                    "安装 ROOT 默认技能 `{}` 失败：{error}",
                    configured_skill.skill_id
                )
            })?;
        let installed_message = format!("已安装 LuaSkills 技能：{}", configured_skill.skill_id);
        progress(&installed_message);
        installed_any_skill = true;
    }

    if installed_any_skill {
        progress("正在刷新 LuaSkills 技能根");
        engine
            .reload_from_roots(skill_roots)
            .map_err(|error| format!("刷新 LuaSkills 技能根失败：{error}"))?;
    } else {
        progress("LuaSkills 默认技能已存在，正在加载目录");
    }

    Ok(())
}

/// Loads declarative ROOT skill bootstrap entries only from `output/config`.
/// 仅从 `output/config` 加载声明式 ROOT 技能引导条目。
fn load_root_skill_bootstrap_entries() -> Result<Vec<RootSkillBootstrapEntry>, String> {
    let config_path = vulcan_runtime_assets::resolve_output_config_directory()
        .ok_or_else(|| String::from("未找到 Vulcan output/config 目录。"))?
        .join("luaskills_root_skills.yaml");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    if !config_path.is_file() {
        return Err(format!(
            "ROOT 技能引导配置不是有效文件：{}",
            config_path.display()
        ));
    }

    let config_text = fs::read_to_string(&config_path).map_err(|error| {
        format!(
            "读取 ROOT 技能引导配置失败（{}）：{error}",
            config_path.display()
        )
    })?;
    let config: RootSkillBootstrapConfig = serde_yaml::from_str(&config_text).map_err(|error| {
        format!(
            "解析 ROOT 技能引导配置失败（{}）：{error}",
            config_path.display()
        )
    })?;

    let mut entries = Vec::with_capacity(config.root_skills.len());
    for entry in config.root_skills {
        let skill_id = entry.skill_id.trim().to_string();
        let source = entry.source.trim().to_string();
        if skill_id.is_empty() {
            return Err(format!(
                "ROOT 技能引导配置存在空 skill_id（{}）。",
                config_path.display()
            ));
        }
        if source.is_empty() {
            return Err(format!(
                "ROOT 技能引导配置 `{skill_id}` 的 source 不能为空。"
            ));
        }
        entries.push(RootSkillBootstrapEntry {
            skill_id,
            source,
            source_type: entry.source_type,
        });
    }

    Ok(entries)
}

/// Resolves one named skill root from the mounted root list.
/// 从已挂载根列表中解析一个具名技能根。
fn resolve_named_skill_root(
    skill_roots: &[RuntimeSkillRoot],
    root_name: &str,
) -> Result<RuntimeSkillRoot, String> {
    skill_roots
        .iter()
        .find(|root| root.name == root_name)
        .cloned()
        .ok_or_else(|| format!("未找到 LuaSkills `{root_name}` 技能根。"))
}

/// Parses a WebUI source type key into the LuaSkills install source enum.
/// 将 WebUI 来源类型键解析为 LuaSkills 安装来源枚举。
fn parse_skill_install_source_type(source_type: &str) -> Result<SkillInstallSourceType, String> {
    match source_type.trim().to_ascii_lowercase().as_str() {
        "github" => Ok(SkillInstallSourceType::Github),
        "url" => Ok(SkillInstallSourceType::Url),
        other => Err(format!("不支持的 LuaSkills 安装来源类型：{other}")),
    }
}

/// Returns a stable display key for one LuaSkills install source type.
/// 返回单个 LuaSkills 安装来源类型的稳定显示键。
fn skill_install_source_type_label(source_type: SkillInstallSourceType) -> &'static str {
    match source_type {
        SkillInstallSourceType::Github => "github",
        SkillInstallSourceType::Url => "url",
        SkillInstallSourceType::OfficialHub => "officialHub",
        SkillInstallSourceType::PrivateUrlManifest => "privateUrlManifest",
    }
}

/// Normalizes an optional text field by trimming blanks and dropping empty values.
/// 通过裁剪空白并丢弃空值来归一化可选文本字段。
fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let stable = value.trim();
        (!stable.is_empty()).then(|| stable.to_string())
    })
}

/// Reloads the LuaSkills runtime after a package management mutation.
/// 在包管理变更后重新加载 LuaSkills 运行时。
fn reload_catalog_after_management(
    handle: &TauriLuaSkillsHandle,
    engine: &mut LuaEngine,
) -> Result<TauriLuaSkillsCatalog, String> {
    engine
        .reload_from_roots(&handle.skill_roots)
        .map_err(|error| format!("同步 LuaSkills 管理操作后的运行时入口失败：{error}"))?;
    build_luaskills_catalog(handle, engine)
}

/// Resolves the ordinary USER root used by WebUI install actions.
/// 解析 WebUI 安装动作使用的普通 USER 根。
fn ordinary_user_root(handle: &TauriLuaSkillsHandle) -> Result<RuntimeSkillRoot, String> {
    resolve_named_skill_root(&handle.skill_roots, "USER")
}

/// Resolves the effective root that currently owns one visible skill.
/// 解析当前拥有单个可见技能的生效根。
fn target_root_for_skill(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
) -> Result<RuntimeSkillRoot, String> {
    let resolved = collect_effective_skill_instances_from_roots(&handle.skill_roots)
        .map_err(|error| format!("解析技能目录失败：{error}"))?
        .into_iter()
        .find(|instance| instance.skill_id == skill_id)
        .ok_or_else(|| format!("未找到技能 `{skill_id}` 对应的生效目录。"))?;
    resolve_named_skill_root(&handle.skill_roots, &resolved.root_name)
}

/// Resolves the uninstall target root and rejects host-owned ROOT skills.
/// 解析卸载目标根，并拒绝宿主自有 ROOT 技能。
fn ordinary_uninstall_target_root_for_skill(
    handle: &TauriLuaSkillsHandle,
    skill_id: &str,
) -> Result<RuntimeSkillRoot, String> {
    let resolved = target_root_for_skill(handle, skill_id)?;
    if resolved.name.eq_ignore_ascii_case("ROOT") {
        return Err(format!(
            "技能 `{skill_id}` 当前位于 ROOT 层，由宿主内建控制，不允许卸载。"
        ));
    }
    Ok(resolved)
}

/// Builds LuaSkills host options backed by the shared VLDB controller.
/// 构建由共享 VLDB 控制器支撑的 LuaSkills 宿主选项。
fn build_host_options(
    runtime_root: &Path,
    managed_runtime_distribution_root: &Path,
    skill_config_root: &Path,
    controller: PersistedControllerConfiguration,
    runtime_settings: &LuaSkillsRuntimeSettings,
) -> LuaRuntimeHostOptions {
    // lua_runtime_root enforces the native-path invariant at the final LuaSkills HostOptions boundary.
    // lua_runtime_root 在最终 LuaSkills HostOptions 边界强制执行原生路径不变量。
    let lua_runtime_root = vulcan_runtime_assets::native_host_path(runtime_root);
    // lua_managed_runtime_distribution_root applies the same invariant to managed runtime discovery.
    // lua_managed_runtime_distribution_root 对受管运行时发现路径应用相同不变量。
    let lua_managed_runtime_distribution_root =
        vulcan_runtime_assets::native_host_path(managed_runtime_distribution_root);
    let mut host_options = LuaRuntimeHostOptions::with_runtime_root(lua_runtime_root.clone());
    host_options.skill_config_root =
        Some(vulcan_runtime_assets::native_host_path(skill_config_root));
    host_options.managed_runtime_distribution_root = Some(lua_managed_runtime_distribution_root);
    host_options.managed_runtime_environment_root =
        Some(lua_runtime_root.join("dependencies").join("envs"));
    host_options.managed_runtime_config = LuaRuntimeManagedRuntimeConfig {
        worker_pool_max_size_per_environment: runtime_settings
            .managed_runtime
            .worker_pool_max_size_per_environment,
        worker_idle_ttl_secs: runtime_settings.managed_runtime.worker_idle_ttl_secs,
        persistent_session_limit_per_engine: runtime_settings
            .managed_runtime
            .persistent_session_limit_per_engine,
        persistent_session_default_buffer_limit_bytes_per_stream: runtime_settings
            .managed_runtime
            .persistent_session_default_buffer_limit_bytes_per_stream,
        invoke_default_timeout_ms: runtime_settings.managed_runtime.invoke_default_timeout_ms,
    };
    host_options.allow_network_download = runtime_settings.network.allow_downloads;
    host_options.github_base_url = runtime_settings.network.github_base_url.clone();
    host_options.github_api_base_url = runtime_settings.network.github_api_base_url.clone();
    host_options.official_skill_hub_base_url =
        runtime_settings.network.official_skill_hub_base_url.clone();
    host_options.enable_private_url_skill_install =
        runtime_settings.network.enable_private_url_skill_install;
    host_options.private_skill_source_allowlist = runtime_settings
        .network
        .private_skill_source_allowlist
        .clone();
    host_options.default_text_encoding =
        runtime_settings.capabilities.default_text_encoding.clone();
    host_options.cache_config = Some(ToolCacheConfig {
        max_entries: runtime_settings.tool_cache.max_entries,
        default_ttl_secs: runtime_settings.tool_cache.default_ttl_secs,
        max_ttl_secs: runtime_settings.tool_cache.max_ttl_secs,
    });
    host_options.runlua_pool_config = Some(LuaRuntimeRunLuaPoolConfig {
        min_size: runtime_settings.run_lua_pool.min_size,
        max_size: runtime_settings.run_lua_pool.max_size,
        idle_ttl_secs: runtime_settings.run_lua_pool.idle_ttl_secs,
    });
    host_options.capabilities = LuaRuntimeCapabilityOptions {
        enable_skill_management_bridge: runtime_settings
            .capabilities
            .enable_skill_management_bridge,
        enable_managed_io_compat: runtime_settings.capabilities.enable_managed_io_compat,
    };
    host_options.sqlite_provider_mode = LuaRuntimeDatabaseProviderMode::SpaceController;
    host_options.lancedb_provider_mode = LuaRuntimeDatabaseProviderMode::SpaceController;
    host_options.space_controller.endpoint = controller
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| Some(vulcan_runtime_assets::default_vldb_controller_endpoint()));
    host_options.space_controller.auto_spawn = controller.auto_spawn;
    host_options.space_controller.executable_path = controller
        .executable_path
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(vulcan_runtime_assets::resolve_bundled_controller_executable_path)
        .map(|path| vulcan_runtime_assets::native_host_path(&path));
    host_options.space_controller.process_mode = match controller.process_mode {
        PersistedControllerProcessMode::Service => LuaRuntimeSpaceControllerProcessMode::Service,
        PersistedControllerProcessMode::Managed => LuaRuntimeSpaceControllerProcessMode::Managed,
    };
    host_options.space_controller.minimum_uptime_secs = controller.minimum_uptime_secs;
    host_options.space_controller.idle_timeout_secs = controller.idle_timeout_secs;
    host_options.space_controller.default_lease_ttl_secs = controller.default_lease_ttl_secs;
    host_options.space_controller.connect_timeout_secs = controller.connect_timeout_secs;
    host_options.space_controller.startup_timeout_secs = controller.startup_timeout_secs;
    host_options.space_controller.startup_retry_interval_ms = controller.startup_retry_interval_ms;
    host_options.space_controller.lease_renew_interval_secs = controller.lease_renew_interval_secs;
    host_options
}

/// Returns the mutable user skill root shared by Vulcan Code hosts.
/// 返回 Vulcan Code 宿主共享的可变用户技能根目录。
fn default_user_skill_root() -> PathBuf {
    vulcan_workspace_store::data_plane::application_data_root()
        .join("luaskills-user")
        .join("skills")
}

/// Builds a skill manager for one mounted root.
/// 为单个已挂载根构建技能管理器。
fn build_skill_manager_for_root(
    handle: &TauriLuaSkillsHandle,
    root_name: &str,
) -> Result<SkillManager, String> {
    let skill_root = handle
        .skill_roots
        .iter()
        .find(|root| root.name == root_name)
        .cloned()
        .ok_or_else(|| format!("未找到技能根 `{root_name}`。"))?;
    let lifecycle_root = skill_root
        .skills_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| skill_root.skills_dir.clone())
        .join("state");

    Ok(SkillManager::new(SkillManagerConfig {
        skill_root,
        lifecycle_root,
        download_cache_root: handle.runtime_root.join("temp").join("downloads"),
        allow_network_download: true,
        github_base_url: None,
        github_api_base_url: None,
        official_skill_hub_base_url: None,
        enable_private_url_skill_install: false,
        private_skill_source_allowlist: Vec::new(),
    }))
}

/// Reads one LuaSkills public manifest from disk.
/// 从磁盘读取单个 LuaSkills 公开清单。
fn read_skill_meta_from_directory(skill_dir: &Path, skill_id: &str) -> Result<SkillMeta, String> {
    let manifest_path = skill_dir.join("skill.yaml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("读取技能清单失败（{}）：{error}", manifest_path.display()))?;
    serde_yaml::from_str::<SkillMeta>(&manifest_text)
        .map_err(|error| format!("解析技能清单失败（{skill_id}）：{error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Builds one upstream-compatible descriptor without declared configuration items.
    /// 构建一份不含配置声明项且兼容上游契约的描述。
    ///
    /// `skill_id` and `version` identify the fixture package.
    /// `skill_id` 与 `version` 标识测试技能包。
    ///
    /// Returns a descriptor suitable for testing configless package handling.
    /// 返回适合测试无配置技能包处理的描述。
    fn empty_config_descriptor(
        skill_id: &str,
        version: &str,
    ) -> RuntimeSkillPackageConfigDescriptor {
        serde_json::from_value(json!({
            "skill_id": skill_id,
            "skill_version": version,
            "complete": true,
            "revision": "0",
            "store_scope": "skills",
            "missing_count": 0,
            "invalid_count": 0,
            "business_issue_count": 0,
            "orphaned_count": 0,
            "orphaned": [],
            "items": []
        }))
        .expect("deserialize empty upstream descriptor")
    }

    /// Verifies configless descriptors are not exposed as configurable packages.
    /// 验证无配置项描述不会作为可配置技能包暴露。
    #[test]
    fn empty_package_config_descriptor_is_filtered() {
        let descriptor = take_declared_package_config_descriptor(
            "fixture",
            vec![empty_config_descriptor("fixture", "1.0.0")],
        )
        .expect("filter descriptor");

        assert!(descriptor.is_none());
    }

    /// Verifies the negative cache is exact by version and supports positive and mutation invalidation.
    /// 验证负缓存按版本精确命中，并支持正向探测与包变更失效。
    #[test]
    fn config_capability_negative_cache_tracks_version_and_invalidation() {
        let mut cache = LuaSkillConfigCapabilityCache::default();

        assert!(!cache.is_known_configless("fixture", "1.0.0"));
        cache.record_probe("fixture", "1.0.0", false);
        assert!(cache.is_known_configless("fixture", "1.0.0"));
        assert!(!cache.is_known_configless("fixture", "1.1.0"));

        cache.record_probe("fixture", "1.0.0", true);
        assert!(!cache.is_known_configless("fixture", "1.0.0"));

        cache.record_probe("fixture", "1.0.0", false);
        assert!(cache.invalidate("fixture"));
        assert!(!cache.is_known_configless("fixture", "1.0.0"));
    }

    /// Verifies declaration defaults use the same canonical strings returned by LuaSkills.
    /// 验证声明默认值使用与 LuaSkills 返回值一致的规范字符串。
    #[test]
    fn package_config_defaults_use_canonical_runtime_strings() {
        // descriptor is deserialized through the public upstream contract to keep the fixture aligned with 0.5.5.
        // descriptor 通过公开上游契约反序列化，使测试夹具与 0.5.5 保持一致。
        let descriptor: RuntimeSkillPackageConfigDescriptor = serde_json::from_value(json!({
            "skill_id": "fixture",
            "skill_version": "1.0.0",
            "complete": true,
            "revision": "2",
            "store_scope": "skills",
            "missing_count": 0,
            "invalid_count": 0,
            "business_issue_count": 0,
            "orphaned_count": 0,
            "orphaned": [],
            "items": [
                {
                    "key": "retries",
                    "type": "integer",
                    "required": false,
                    "sensitive": false,
                    "description": "retry count",
                    "constraints": {},
                    "options": [],
                    "default": 3,
                    "advanced": false,
                    "restart_required": false,
                    "deprecated": false,
                    "state": "default",
                    "satisfied": true,
                    "value": null
                },
                {
                    "key": "ratio",
                    "type": "float",
                    "required": false,
                    "sensitive": false,
                    "description": "ratio",
                    "constraints": {},
                    "options": [],
                    "default": -0.0,
                    "advanced": false,
                    "restart_required": false,
                    "deprecated": false,
                    "state": "default",
                    "satisfied": true,
                    "value": null
                }
            ]
        }))
        .expect("deserialize upstream descriptor");

        assert_eq!(
            canonical_default_config_value(&descriptor.items[0]).expect("integer default"),
            Some(String::from("3"))
        );
        assert_eq!(
            canonical_default_config_value(&descriptor.items[1]).expect("float default"),
            Some(String::from("0"))
        );
        assert_eq!(
            canonical_configured_config_value(&descriptor.items[0], "003")
                .expect("configured integer"),
            "3"
        );
        assert_eq!(
            canonical_configured_config_value(&descriptor.items[1], "-0")
                .expect("configured float"),
            "0"
        );
    }
}
