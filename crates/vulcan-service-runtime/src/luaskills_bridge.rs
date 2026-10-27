//! Minimal LuaSkills bootstrap used by the Core Service runtime.
//! Core Service 运行时使用的最小 LuaSkills 引导模块。

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

use crate::managed_runtime_bootstrap::{
    ManagedRuntimeBootstrapProgress, ManagedRuntimeBootstrapReport, ManagedRuntimeInstallTarget,
    bootstrap_managed_runtimes, bootstrap_managed_runtimes_for_target,
};
use luaskills::runtime_options::LuaRuntimeRunLuaPoolConfig;
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaRuntimeCapabilityOptions, LuaRuntimeHostOptions,
    LuaRuntimeManagedRuntimeConfig, LuaRuntimeNetworkOptions, LuaRuntimeSecretString,
    LuaVmPoolConfig, RuntimeEntryDescriptor,
    RuntimeSkillConfigEventBatch, RuntimeSkillConfigStoreRefresh,
    RuntimeSkillPackageConfigDescriptor, RuntimeSkillPackageConfigItemDescriptor,
    RuntimeSkillPackageConfigStatus, RuntimeSkillRoot, SkillConfigDeleteResult,
    SkillConfigWriteResult, SkillInstallRequest, SkillInstallSourceType, SkillManagementAuthority,
    SkillManager, SkillManagerConfig, SkillMeta, SkillPackageConfigInputValue,
    SkillPackageConfigItemState, SkillUninstallOptions, ToolCacheConfig,
};
use vulcan_luaskills_config::{
    LuaSkillsNetworkSettings, LuaSkillsRuntimeSettings, load_runtime_settings,
    runtime_settings_file_path, save_runtime_settings,
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
    ///
    /// `Url` is refused rather than converted: upstream rejects
    /// `SkillInstallSourceType::Url` unconditionally by source policy, so a ROOT bootstrap
    /// declaring it would fail on every start with an upstream policy string. Refusing it
    /// here reports the reason once, in the host's own words.
    /// `Url` 被拒绝而不是被转换：上游按来源策略无条件拒绝 `SkillInstallSourceType::Url`，
    /// 因此声明它的 ROOT 引导会在每次启动时以上游策略文案失败。在此拒绝它可以用宿主自己的措辞
    /// 一次性说明原因。
    fn into_runtime_source_type(self) -> Result<SkillInstallSourceType, String> {
        match self {
            Self::Github => Ok(SkillInstallSourceType::Github),
            Self::Url => Err(String::from(
                "The `url` source type has been disabled by the upstream source policy and cannot be used; change this skill's source type to `github`.",
            )),
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
    /// Mutable ordinary-skill engine guarded for skill catalog and configuration management.
    /// 为技能目录及配置管理加锁的可变普通技能引擎。
    pub engine: Arc<RwLock<LuaEngine>>,
    /// Immutable plugin execution engine, independent of ordinary-skill reloads and their management lock.
    /// 不可变插件执行引擎，独立于普通技能重载及其管理锁。
    pub plugin_engine: Arc<LuaEngine>,
    /// Runtime root resolved from the prepared Vulcan output bundle.
    /// 从准备好的 Vulcan output 包解析出的运行时根目录。
    pub runtime_root: PathBuf,
    /// Host-owned shared root containing managed Python and Node distributions.
    /// 宿主持有的共享根目录，包含受管 Python 与 Node 发行包。
    pub managed_runtime_distribution_root: PathBuf,
    /// LuaSkills-owned writable root containing reusable managed environments.
    /// LuaSkills 持有的可写根目录，包含可复用受管环境。
    pub managed_runtime_environment_root: PathBuf,
    /// Output-owned Vulcan configuration directory shared by host LuaSkills engines.
    /// 宿主 LuaSkills 引擎共享的 output 自有 Vulcan 配置目录。
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
    /// Network settings currently applied to the engine, replaced after every settings save.
    /// 当前应用到引擎的网络设置，每次保存设置后替换。
    live_network_settings: Arc<Mutex<LuaSkillsNetworkSettings>>,
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
            .field("live_network_settings", &self.live_network_settings)
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
    /// Whether the running engine holds a GitHub token; the token itself never leaves the engine.
    /// 运行中引擎是否持有 GitHub 令牌；令牌本身绝不离开引擎。
    pub github_token_configured: bool,
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

/// Bootstraps LuaSkills using the in-process database mode shared by Vulcan Code hosts.
/// 使用 Vulcan Code 宿主共享的进程内数据库模式引导 LuaSkills。
///
/// `github_token` is the token read from the host's secure store; it only reaches engine memory.
/// `github_token` 是从宿主安全存储读取的令牌；它只进入引擎内存。
pub fn bootstrap_luaskills<F, G>(
    mut progress: F,
    mut managed_progress: G,
    github_token: Option<String>,
) -> Result<TauriLuaSkillsHandle, String>
where
    F: FnMut(&str),
    G: FnMut(&ManagedRuntimeBootstrapProgress),
{
    let runtime_root =
        vulcan_runtime_assets::resolve_luaskills_runtime_root().ok_or_else(|| {
            String::from(
                "The LuaSkills runtime root was not found; run make sync first to generate output/lua_runtime.",
            )
        })?;
    // runtime_root pins every derived host path to one absolute native directory identity while removing Windows verbatim prefixes before the path reaches LuaSkills.
    // runtime_root 将所有派生宿主路径固定到一个绝对原生目录身份，并在路径进入 LuaSkills 前移除 Windows verbatim 前缀。
    let runtime_root = vulcan_runtime_assets::native_host_path(
        &fs::canonicalize(&runtime_root).map_err(|error| {
            format!(
                "Failed to canonicalize the LuaSkills runtime root `{}`: {error}",
                runtime_root.display()
            )
        })?,
    );
    progress("Preparing the LuaSkills runtime directories");
    // output_config_directory is the only runtime configuration root accepted by Vulcan Code.
    // output_config_directory 是 Vulcan Code 唯一接受的运行时配置根目录。
    let output_config_directory = vulcan_runtime_assets::resolve_output_config_directory()
        .ok_or_else(|| String::from("The Vulcan output/config directory was not found."))?;
    // runtime_settings is the Vulcan-owned source of truth shared by both host engines.
    // runtime_settings 是两个宿主引擎共享的 Vulcan 自有事实来源。
    let runtime_settings = load_runtime_settings(&output_config_directory)?;
    // managed_runtime_distribution_root is the canonical host-owned public runtime directory.
    // managed_runtime_distribution_root 是宿主持有的规范公共运行时目录。
    let managed_runtime_distribution_root =
        vulcan_runtime_assets::resolve_managed_runtime_distribution_root().ok_or_else(|| {
            String::from("The Vulcan shared Python/Node distribution root was not found.")
        })?;
    fs::create_dir_all(&managed_runtime_distribution_root).map_err(|error| {
        format!(
            "Failed to create the shared Python/Node distribution root `{}`: {error}",
            managed_runtime_distribution_root.display()
        )
    })?;
    // managed_runtime_distribution_root uses the same native-path boundary required by LuaSkills host options.
    // managed_runtime_distribution_root 使用 LuaSkills 宿主选项要求的同一原生路径边界。
    let managed_runtime_distribution_root = vulcan_runtime_assets::native_host_path(
        &fs::canonicalize(&managed_runtime_distribution_root).map_err(|error| {
            format!(
                "Failed to canonicalize the shared Python/Node distribution root `{}`: {error}",
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
        "Managed Python/Node runtime: {}",
        managed_runtime_report.diagnostic
    ));
    let mut host_options = build_host_options(
        &runtime_root,
        &managed_runtime_distribution_root,
        &skill_config_root,
        &runtime_settings,
    );
    host_options.github_token = github_token.map(LuaRuntimeSecretString::new);
    let pool_config = LuaVmPoolConfig {
        min_size: runtime_settings.vm_pool.min_size,
        max_size: runtime_settings.vm_pool.max_size,
        idle_ttl_secs: runtime_settings.vm_pool.idle_ttl_secs,
    };
    // Retain the same resolved host settings while keeping plugin execution outside skill mutation locks.
    // 保持相同已解析宿主设置，同时让插件执行独立于技能变更锁。
    let plugin_engine = Arc::new(
        LuaEngine::new(LuaEngineOptions::new(pool_config, host_options.clone()))
            .map_err(|error| format!("Failed to create the plugin execution engine: {error}"))?,
    );
    let mut engine = LuaEngine::new(LuaEngineOptions::new(pool_config, host_options))
        .map_err(|error| format!("Failed to create the LuaSkills engine: {error}"))?;
    ensure_configured_root_skills_installed(&skill_roots, &mut engine, &mut progress)?;
    progress("Loading the LuaSkills skill catalog");
    engine
        .load_from_roots(&skill_roots)
        .map_err(|error| format!("Failed to load the LuaSkills skill roots: {error}"))?;

    // managed_runtime_environment_root keeps writable environments inside LuaSkills-owned data.
    // managed_runtime_environment_root 将可写环境保留在 LuaSkills 自有数据目录内。
    let managed_runtime_environment_root = runtime_root.join("dependencies").join("envs");
    Ok(TauriLuaSkillsHandle {
        engine: Arc::new(RwLock::new(engine)),
        plugin_engine,
        runtime_root,
        managed_runtime_distribution_root,
        managed_runtime_environment_root,
        output_config_directory,
        skill_roots,
        managed_runtime_report: Arc::new(Mutex::new(managed_runtime_report)),
        managed_runtime_progress: Arc::new(Mutex::new(None)),
        config_capability_cache: Arc::new(Mutex::new(LuaSkillConfigCapabilityCache::default())),
        live_network_settings: Arc::new(Mutex::new(runtime_settings.network.clone())),
        effective_runtime_settings: runtime_settings,
    })
}

/// Loads a read-only LuaSkills catalog from the retained runtime handle.
/// 从保留的运行时句柄加载只读 LuaSkills 目录。
pub fn load_luaskills_catalog(
    handle: &TauriLuaSkillsHandle,
) -> Result<TauriLuaSkillsCatalog, String> {
    let engine = handle.engine.read().map_err(|_| {
        String::from(
            "The LuaSkills engine read lock is poisoned; the plugin catalog cannot be read.",
        )
    })?;
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
        .read()
        .map_err(|_| String::from("The LuaSkills engine read lock is poisoned; the skill package configuration cannot be read."))?;
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
        .read()
        .map_err(|_| String::from("The LuaSkills engine read lock is poisoned; the skill package configuration cannot be validated."))?;
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
        .write()
        .map_err(|_| String::from("The LuaSkills engine write lock is poisoned; the skill package configuration cannot be updated."))?;
    let result = engine.set_skill_config_values(skill_id, values, Some(expected_revision))?;
    let descriptor = describe_host_safe_package_config(&engine, skill_id)?
        .ok_or_else(|| format!("Skill `{skill_id}` declares no package configuration."))?;
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
        .write()
        .map_err(|_| String::from("The LuaSkills engine write lock is poisoned; the skill package configuration cannot be deleted."))?;
    let result = engine.delete_skill_config_value(skill_id, key, Some(expected_revision))?;
    let descriptor = describe_host_safe_package_config(&engine, skill_id)?
        .ok_or_else(|| format!("Skill `{skill_id}` declares no package configuration."))?;
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
        .read()
        .map_err(|_| String::from("The LuaSkills engine read lock is poisoned; skill package configuration events cannot be polled."))?;
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
        .read()
        .map_err(|_| String::from("The LuaSkills engine read lock is poisoned; the skill package configuration cannot be refreshed."))?;
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
            "Skill `{skill_id}` returned multiple effective configuration descriptors; the runtime state is inconsistent."
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
        .map_err(|_| String::from("The LuaSkills configuration capability cache lock is poisoned; the catalog cannot be read."))?
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
        .map_err(|_| String::from("The LuaSkills configuration capability cache lock is poisoned; the catalog cannot be updated."))?
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
        .map_err(|_| String::from("The LuaSkills configuration capability cache lock is poisoned; the cache cannot be invalidated."))?
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
    let mut settings = settings.clone();
    settings.network.custom_github_mirrors =
        crate::luaskills_network::validated_custom_github_mirrors(
            &settings.network.custom_github_mirrors,
        )?;
    save_runtime_settings(&handle.output_config_directory, &settings)?;
    // The saved file is re-read so the engine receives exactly what normalization persisted.
    // 重新读取已保存文件，使引擎收到的正是规范化后持久化的内容。
    let saved = load_runtime_settings(&handle.output_config_directory)?;
    let mut engine = handle.engine.write().map_err(|_| {
        String::from(
            "The LuaSkills engine write lock is poisoned; the network settings cannot be applied.",
        )
    })?;
    apply_live_network_settings(handle, &mut engine, &saved.network)?;
    build_luaskills_catalog(handle, &engine)
}

/// Replaces the engine's live network policy with `network`, keeping the token it already holds.
/// 用 `network` 替换引擎的实时网络策略，并保留其已持有的令牌。
fn apply_live_network_settings(
    handle: &TauriLuaSkillsHandle,
    engine: &mut LuaEngine,
    network: &LuaSkillsNetworkSettings,
) -> Result<(), String> {
    let current = engine.network_options();
    engine.set_network_options(LuaRuntimeNetworkOptions {
        allow_network_download: network.allow_downloads,
        github_base_url: network.github_base_url.clone(),
        github_api_base_url: network.github_api_base_url.clone(),
        github_token: current.github_token,
        official_skill_hub_base_url: Some(network.effective_official_skill_hub_base_url()),
    });
    *handle.live_network_settings.lock().map_err(|_| {
        String::from("The LuaSkills network settings lock is poisoned; the Hub address cannot be updated.")
    })? = network.clone();
    Ok(())
}

/// Returns the network settings the running engine currently uses.
/// 返回运行中引擎当前使用的网络设置。
pub fn live_network_settings(
    handle: &TauriLuaSkillsHandle,
) -> Result<LuaSkillsNetworkSettings, String> {
    handle
        .live_network_settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|_| String::from("The LuaSkills network settings lock is poisoned."))
}

/// Replaces or clears the GitHub token held by the running engine and returns the refreshed catalog.
/// 替换或清除运行中引擎持有的 GitHub 令牌，并返回刷新的目录。
///
/// The caller persists the token in the secure store first; this only updates engine memory.
/// 调用方先把令牌写入安全存储；此处只更新引擎内存。
pub fn set_luaskills_github_token(
    handle: &TauriLuaSkillsHandle,
    github_token: Option<String>,
) -> Result<TauriLuaSkillsCatalog, String> {
    let mut engine = handle.engine.write().map_err(|_| {
        String::from(
            "The LuaSkills engine write lock is poisoned; the GitHub token cannot be applied.",
        )
    })?;
    let mut network = engine.network_options();
    network.github_token = github_token.map(LuaRuntimeSecretString::new);
    engine.set_network_options(network);
    build_luaskills_catalog(handle, &engine)
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
        .map_err(|_| String::from("The managed Python/Node initialization state lock is poisoned; the retry result cannot be saved."))?;
    *retained_report = report;
    drop(retained_report);
    load_luaskills_catalog(handle)
}

/// Performs one LuaSkills management action and returns the refreshed catalog.
/// 执行一次 LuaSkills 管理动作并返回刷新后的目录。
pub fn refresh_luaskills_catalog(
    handle: &TauriLuaSkillsHandle,
) -> Result<TauriLuaSkillsCatalog, String> {
    let mut engine = handle.engine.write().map_err(|_| {
        String::from(
            "The LuaSkills engine write lock is poisoned; the plugin catalog cannot be refreshed.",
        )
    })?;
    engine
        .reload_from_roots(&handle.skill_roots)
        .map_err(|error| format!("Failed to refresh the LuaSkills runtime: {error}"))?;
    build_luaskills_catalog(handle, &engine)
}

/// Formats one post-commit projection failure for a committed skill mutation.
/// 为一次已提交的技能变更格式化后置投影失败。
///
/// The engine call is the commit point: once it returns, the package change is on disk.
/// A later failure is therefore a projection failure, and its message must name the
/// mutation that completed, because every caller — including the operation-progress
/// projection that records `Succeeded`/`Failed` from this result — treats any error as
/// a failed operation.
/// 引擎调用即提交点：它一旦返回，包变更就已在盘上。因此后续失败是投影失败，其消息必须点名
/// 已完成的变更，因为每个调用方——包括据此记录 `Succeeded`/`Failed` 的操作进度投影——都会把
/// 任何错误当作操作失败。
fn post_commit_projection_failure(
    skill_id: &str,
    completed_action: &str,
    failed_step: &str,
    error: &str,
) -> String {
    format!("Skill `{skill_id}` was {completed_action}, but {failed_step}: {error}")
}

/// Installs one LuaSkills package into the ordinary USER root.
/// 将单个 LuaSkills 包安装到普通 USER 根。
pub fn install_luaskill(
    handle: &TauriLuaSkillsHandle,
    skill_id: Option<String>,
    source: String,
    source_type: String,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle.engine.write().map_err(|_| {
        String::from("The LuaSkills engine write lock is poisoned; the skill cannot be installed.")
    })?;
    let user_root = ordinary_user_root(handle)?;
    let request = SkillInstallRequest {
        skill_id: normalize_optional_field(skill_id),
        source: normalize_optional_field(Some(source)),
        source_type: parse_skill_install_source_type(&source_type)?,
        replace_source: false,
    };
    let result = engine
        .install_skill_in_root(&handle.skill_roots, &user_root, &request)
        .map_err(|error| format!("Failed to install the LuaSkills skill: {error}"))?;
    // The install is committed on disk once install_skill_in_root returns, so a later
    // failure is a projection failure rather than an install failure. Its message must
    // say the package was installed, because reporting a bare failure here would tell
    // the caller and the operation-progress projection that nothing changed.
    // install_skill_in_root 返回后安装已在盘上提交，因此后续失败是投影失败而不是安装失败。
    // 其消息必须说明包已安装，因为在此报告裸失败会让调用方与操作进度投影以为什么都没变。
    invalidate_config_capability_cache(handle, &result.skill_id).map_err(|error| {
        post_commit_projection_failure(
            &result.skill_id,
            "installed",
            "invalidating the configuration capability cache failed",
            &error,
        )
    })?;
    let catalog = reload_catalog_after_management(handle, &mut engine).map_err(|error| {
        post_commit_projection_failure(
            &result.skill_id,
            "installed",
            "refreshing the skill catalog failed",
            &error,
        )
    })?;
    Ok((result.message, catalog))
}

/// Updates one managed LuaSkills package in its owning root.
/// 在所属根中更新单个受管 LuaSkills 包。
///
/// `replacement` carries a new `(source_type, source)` pair to move the package to another
/// source; the engine refuses a different skill or an older version.
/// `replacement` 携带新的 `(source_type, source)`，用于把技能包迁移到另一来源；引擎拒绝不同技能或更旧版本。
pub fn update_luaskill(
    handle: &TauriLuaSkillsHandle,
    skill_id: String,
    replacement: Option<(String, String)>,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle.engine.write().map_err(|_| {
        String::from("The LuaSkills engine write lock is poisoned; the skill cannot be updated.")
    })?;
    let target_root = target_root_for_skill(handle, &skill_id)?;
    let request = match replacement {
        Some((source_type, source)) => SkillInstallRequest {
            skill_id: Some(skill_id),
            source: normalize_optional_field(Some(source)),
            source_type: parse_skill_install_source_type(&source_type)?,
            replace_source: true,
        },
        // A plain update keeps the recorded source; the source type here is not consulted.
        // 普通更新沿用已记录来源；此处的来源类型不会被读取。
        None => SkillInstallRequest {
            skill_id: Some(skill_id),
            source: None,
            source_type: SkillInstallSourceType::Github,
            replace_source: false,
        },
    };
    let result = if target_root.name.eq_ignore_ascii_case("ROOT") {
        engine
            .system_update_skill_in_root(
                &handle.skill_roots,
                &target_root,
                SkillManagementAuthority::System,
                &request,
            )
            .map_err(|error| format!("Failed to update the LuaSkills skill: {error}"))?
    } else {
        engine
            .update_skill_in_root(&handle.skill_roots, &target_root, &request)
            .map_err(|error| format!("Failed to update the LuaSkills skill: {error}"))?
    };
    // The update is committed on disk once the engine call returns, so a later failure
    // is a projection failure rather than an update failure. Its message must say the
    // package was updated, because a bare failure here would tell the caller and the
    // operation-progress projection that the previous version is still current.
    // 引擎调用返回后更新已在盘上提交，因此后续失败是投影失败而不是更新失败。其消息必须说明
    // 包已更新，因为裸失败会让调用方与操作进度投影以为旧版本仍然是当前版本。
    invalidate_config_capability_cache(handle, &result.skill_id).map_err(|error| {
        post_commit_projection_failure(
            &result.skill_id,
            "updated",
            "invalidating the configuration capability cache failed",
            &error,
        )
    })?;
    // LuaSkills 0.5.9 reloads the runtime inside the update transaction. Project its
    // committed state without destroying a second VM pool and its native libraries.
    // LuaSkills 0.5.9 在更新事务内重载运行时；直接投影已提交状态，避免再次销毁 VM 池及其原生库。
    let catalog = build_luaskills_catalog(handle, &engine).map_err(|error| {
        post_commit_projection_failure(
            &result.skill_id,
            "updated",
            "refreshing the skill catalog failed",
            &error,
        )
    })?;
    Ok((result.message, catalog))
}

/// Update availability of one managed LuaSkills package.
/// 单个受管 LuaSkills 包的更新可用性。
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TauriLuaSkillUpdateCheck {
    /// Stable skill identifier.
    /// 稳定技能标识。
    pub skill_id: String,
    /// Version recorded by the install workflow.
    /// 安装流程记录的版本。
    pub installed_version: Option<String>,
    /// Latest version the recorded source resolves to.
    /// 已记录来源解析出的最新版本。
    pub latest_version: Option<String>,
    /// Whether the latest version is newer than the installed one.
    /// 最新版本是否比已安装版本更新。
    pub update_available: bool,
    /// Stable source type key, such as `github` or `officialHub`.
    /// 稳定来源类型键，例如 `github` 或 `officialHub`。
    pub source_type: Option<String>,
    /// Recorded source locator.
    /// 已记录的来源定位值。
    pub source_locator: Option<String>,
    /// Failure for this package only; other packages are still reported.
    /// 仅属于本包的失败；其他包仍会照常报告。
    pub error: Option<String>,
}

/// Checks managed packages for newer versions without downloading or changing anything.
/// 检查受管技能包是否有新版本，不下载也不修改任何内容。
///
/// `skill_ids` limits the check; `None` checks every effective managed package.
/// `skill_ids` 限定检查范围；`None` 检查全部生效的受管技能包。
pub fn check_luaskill_updates(
    handle: &TauriLuaSkillsHandle,
    skill_ids: Option<Vec<String>>,
) -> Result<Vec<TauriLuaSkillUpdateCheck>, String> {
    let engine = handle.engine.read().map_err(|_| {
        String::from("The LuaSkills engine read lock is poisoned; updates cannot be checked.")
    })?;
    let instances = collect_effective_skill_instances_from_roots(&handle.skill_roots)
        .map_err(|error| format!("Failed to resolve the skill directories: {error}"))?;
    let mut checks = Vec::new();
    // managed_ids groups the packages to check by the root that owns their install record.
    // managed_ids 按持有安装记录的根对待检查的包分组。
    let mut managed_ids = BTreeMap::<String, Vec<String>>::new();
    for instance in &instances {
        let requested = skill_ids
            .as_ref()
            .is_none_or(|ids| ids.contains(&instance.skill_id));
        if !requested {
            continue;
        }
        let managed = build_skill_manager_for_root(handle, &instance.root_name)?
            .install_record(&instance.skill_id)
            .map_err(|error| {
                format!(
                    "Failed to read the skill installation record ({}): {error}",
                    instance.skill_id
                )
            })?
            .is_some_and(|record| record.managed);
        if managed {
            managed_ids
                .entry(instance.root_name.clone())
                .or_default()
                .push(instance.skill_id.clone());
        } else if skill_ids.is_some() {
            checks.push(update_check_error(
                &instance.skill_id,
                "The skill was not installed by the install workflow, so it has no source to check.",
            ));
        }
    }
    if let Some(ids) = &skill_ids {
        for skill_id in ids {
            if !instances.iter().any(|instance| &instance.skill_id == skill_id) {
                checks.push(update_check_error(
                    skill_id,
                    "No effective directory was found for this skill.",
                ));
            }
        }
    }
    for (root_name, ids) in managed_ids {
        let root = resolve_named_skill_root(&handle.skill_roots, &root_name)?;
        let results = if root.name.eq_ignore_ascii_case("ROOT") {
            engine.system_check_skill_updates_in_root(
                &handle.skill_roots,
                &root,
                SkillManagementAuthority::System,
                Some(&ids),
            )
        } else {
            engine.check_skill_updates_in_root(&handle.skill_roots, &root, Some(&ids))
        }
        .map_err(|error| format!("Failed to check LuaSkills updates: {error}"))?;
        checks.extend(results.into_iter().map(|check| TauriLuaSkillUpdateCheck {
            skill_id: check.skill_id,
            installed_version: check.installed_version,
            latest_version: check.latest_version,
            update_available: check.update_available,
            source_type: check
                .source_type
                .map(|source_type| skill_install_source_type_label(source_type).to_string()),
            source_locator: check.source_locator,
            error: check.error,
        }));
    }
    checks.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
    Ok(checks)
}

/// Builds the update check row reported for a package that could not be checked.
/// 构建无法检查的包所报告的更新检查行。
fn update_check_error(skill_id: &str, error: &str) -> TauriLuaSkillUpdateCheck {
    TauriLuaSkillUpdateCheck {
        skill_id: skill_id.to_string(),
        installed_version: None,
        latest_version: None,
        update_available: false,
        source_type: None,
        source_locator: None,
        error: Some(error.to_string()),
    }
}

/// Enables or disables one LuaSkills package in its owning root.
/// 在所属根中启用或停用单个 LuaSkills 包。
pub fn set_luaskill_enabled(
    handle: &TauriLuaSkillsHandle,
    skill_id: String,
    enabled: bool,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle.engine.write().map_err(|_| {
        String::from(
            "The LuaSkills engine write lock is poisoned; the skill state cannot be switched.",
        )
    })?;
    let target_root = target_root_for_skill(handle, &skill_id)?;
    if enabled {
        if target_root.name.eq_ignore_ascii_case("ROOT") {
            engine
                .system_enable_skill(
                    &handle.skill_roots,
                    SkillManagementAuthority::System,
                    &skill_id,
                )
                .map_err(|error| format!("Failed to enable the LuaSkills skill: {error}"))?;
        } else {
            engine
                .enable_skill(&handle.skill_roots, &skill_id)
                .map_err(|error| format!("Failed to enable the LuaSkills skill: {error}"))?;
        }
    } else if target_root.name.eq_ignore_ascii_case("ROOT") {
        engine
            .system_disable_skill_in_roots(
                &handle.skill_roots,
                SkillManagementAuthority::System,
                &skill_id,
                Some("disabled from vulcan-code-web"),
            )
            .map_err(|error| format!("Failed to disable the LuaSkills skill: {error}"))?;
    } else {
        engine
            .disable_skill_in_roots(
                &handle.skill_roots,
                &skill_id,
                Some("disabled from vulcan-code-web"),
            )
            .map_err(|error| format!("Failed to disable the LuaSkills skill: {error}"))?;
    }
    // The enable or disable is committed on disk once the engine call returns, so a
    // later failure is a projection failure. Its message must name the state change
    // that already took effect, because a bare failure here would tell the caller and
    // the catalog projection that the previous enabled state is still current.
    // 引擎调用返回后启用或停用已在盘上提交，因此后续失败是投影失败。其消息必须点名已经生效的
    // 状态变更，因为裸失败会让调用方与目录投影以为先前的启停状态仍然是当前状态。
    let completed_action = if enabled { "enabled" } else { "disabled" };
    let catalog = reload_catalog_after_management(handle, &mut engine).map_err(|error| {
        post_commit_projection_failure(
            &skill_id,
            completed_action,
            "refreshing the skill catalog failed",
            &error,
        )
    })?;
    let message = if enabled {
        format!("Enabled skill `{skill_id}`.")
    } else {
        format!("Disabled skill `{skill_id}`.")
    };
    Ok((message, catalog))
}

/// Uninstalls one ordinary LuaSkills package from its owning root.
/// 从所属根卸载单个普通 LuaSkills 包。
pub fn uninstall_luaskill(
    handle: &TauriLuaSkillsHandle,
    skill_id: String,
) -> Result<(String, TauriLuaSkillsCatalog), String> {
    let mut engine = handle.engine.write().map_err(|_| {
        String::from(
            "The LuaSkills engine write lock is poisoned; the skill cannot be uninstalled.",
        )
    })?;
    let target_root = ordinary_uninstall_target_root_for_skill(handle, &skill_id)?;
    let result = engine
        .uninstall_skill_in_root(
            &handle.skill_roots,
            &target_root,
            &skill_id,
            &SkillUninstallOptions::default(),
        )
        .map_err(|error| format!("Failed to uninstall the LuaSkills skill: {error}"))?;
    // The uninstall is committed on disk once the engine call returns, so a later
    // failure is a projection failure rather than an uninstall failure. Its message
    // must say the package was uninstalled, because a bare failure here would tell the
    // caller and the operation-progress projection that the package is still present.
    // 引擎调用返回后卸载已在盘上提交，因此后续失败是投影失败而不是卸载失败。其消息必须说明
    // 包已卸载，因为裸失败会让调用方与操作进度投影以为包仍然存在。
    invalidate_config_capability_cache(handle, &result.skill_id).map_err(|error| {
        post_commit_projection_failure(
            &result.skill_id,
            "uninstalled",
            "invalidating the configuration capability cache failed",
            &error,
        )
    })?;
    let catalog = reload_catalog_after_management(handle, &mut engine).map_err(|error| {
        post_commit_projection_failure(
            &result.skill_id,
            "uninstalled",
            "refreshing the skill catalog failed",
            &error,
        )
    })?;
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
        .map_err(|error| format!("Failed to read the LuaSkills host-visible tools: {error}"))?;
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
        .map_err(|error| {
            format!("Failed to resolve the effective LuaSkills skill directories: {error}")
        })?;
    let mut skills = Vec::with_capacity(resolved_skills.len());
    for resolved in resolved_skills {
        let meta = read_skill_meta_from_directory(&resolved.actual_dir, &resolved.skill_id)?;
        let manager = build_skill_manager_for_root(handle, &resolved.root_name)?;
        let enabled = manager
            .is_skill_enabled(&resolved.skill_id)
            .map_err(|error| {
                format!(
                    "Failed to read the skill enablement state ({}): {error}",
                    resolved.skill_id
                )
            })?;
        let disabled_reason = manager
            .disabled_record(&resolved.skill_id)
            .map_err(|error| {
                format!(
                    "Failed to read the skill disablement record ({}): {error}",
                    resolved.skill_id
                )
            })?
            .and_then(|record| record.reason);
        let install_record = manager
            .install_record(&resolved.skill_id)
            .map_err(|error| {
                format!(
                    "Failed to read the skill installation record ({}): {error}",
                    resolved.skill_id
                )
            })?;
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
            String::from("No visible tool entry is exposed")
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
                    format!(
                        "Failed to read the skill package configuration state ({}): {error}",
                        resolved.skill_id
                    )
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
        .map_err(|_| String::from("The managed Python/Node initialization state lock is poisoned; the catalog cannot be read."))?
        .clone();
    // managed_runtime_progress is cloned independently because retries update it while the engine remains live.
    // managed_runtime_progress 独立克隆，因为重试会在引擎保持运行时更新它。
    let managed_runtime_progress = handle
        .managed_runtime_progress
        .lock()
        .map_err(|_| String::from("The managed Python/Node initialization progress lock is poisoned; the catalog cannot be read."))?
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
        github_token_configured: engine.network_options().github_token.is_some(),
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
    let download_root = luaskills_download_cache_root(runtime_root);
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
                "Failed to create the LuaSkills runtime directory ({}): {error}",
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
        progress("No LuaSkills ROOT skills are configured for automatic installation");
        return Ok(());
    }

    let installed_root_skill_ids =
        collect_effective_skill_instances_from_roots(std::slice::from_ref(&root_target))
            .map_err(|error| format!("Failed to resolve the ROOT skill directory: {error}"))?
            .into_iter()
            .map(|instance| instance.skill_id)
            .collect::<std::collections::BTreeSet<_>>();

    let mut installed_any_skill = false;
    for configured_skill in configured_skills {
        if installed_root_skill_ids.contains(configured_skill.skill_id.as_str()) {
            continue;
        }

        let install_message = format!("Installing LuaSkills skill: {}", configured_skill.skill_id);
        progress(&install_message);
        // The source type is resolved before the install request is built, so a disabled
        // source type fails with the skill identifier attached instead of a bare policy
        // error, and no partially-built request reaches the engine.
        // 来源类型在构建安装请求之前解析，因此被停用的来源类型会带着技能标识失败，而不是一条
        // 裸的策略错误，并且不会有半成品请求到达引擎。
        let resolved_source_type = configured_skill
            .source_type
            .into_runtime_source_type()
            .map_err(|error| {
                format!(
                    "ROOT bootstrap skill `{}`: {error}",
                    configured_skill.skill_id
                )
            })?;
        let install_request = SkillInstallRequest {
            skill_id: Some(configured_skill.skill_id.clone()),
            source: Some(configured_skill.source.clone()),
            source_type: resolved_source_type,
            replace_source: false,
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
                    "Failed to install the ROOT default skill `{}`: {error}",
                    configured_skill.skill_id
                )
            })?;
        let installed_message = format!("Installed LuaSkills skill: {}", configured_skill.skill_id);
        progress(&installed_message);
        installed_any_skill = true;
    }

    if installed_any_skill {
        progress("Refreshing the LuaSkills skill roots");
        // Every system install above is already committed on disk, so a reload failure
        // must not read as "no default skill was installed": the next bootstrap would
        // otherwise re-attempt installs that already succeeded.
        // 上面的每次系统安装都已在盘上提交，因此重载失败不得被读成「没有安装任何默认技能」：
        // 否则下一次引导会重新尝试那些已经成功的安装。
        engine.reload_from_roots(skill_roots).map_err(|error| {
            format!("The ROOT default skills are installed, but refreshing the LuaSkills skill roots failed: {error}")
        })?;
    } else {
        progress("The LuaSkills default skills already exist; loading the catalog");
    }

    Ok(())
}

/// Loads declarative ROOT skill bootstrap entries only from `output/config`.
/// 仅从 `output/config` 加载声明式 ROOT 技能引导条目。
fn load_root_skill_bootstrap_entries() -> Result<Vec<RootSkillBootstrapEntry>, String> {
    let config_path = vulcan_runtime_assets::resolve_output_config_directory()
        .ok_or_else(|| String::from("The Vulcan output/config directory was not found."))?
        .join("luaskills_root_skills.yaml");
    if !config_path.exists() {
        return Ok(Vec::new());
    }
    if !config_path.is_file() {
        return Err(format!(
            "The ROOT skill bootstrap configuration is not a regular file: {}",
            config_path.display()
        ));
    }

    let config_text = fs::read_to_string(&config_path).map_err(|error| {
        format!(
            "Failed to read the ROOT skill bootstrap configuration ({}): {error}",
            config_path.display()
        )
    })?;
    let config: RootSkillBootstrapConfig = serde_yaml::from_str(&config_text).map_err(|error| {
        format!(
            "Failed to parse the ROOT skill bootstrap configuration ({}): {error}",
            config_path.display()
        )
    })?;

    let mut entries = Vec::with_capacity(config.root_skills.len());
    for entry in config.root_skills {
        let skill_id = entry.skill_id.trim().to_string();
        let source = entry.source.trim().to_string();
        if skill_id.is_empty() {
            return Err(format!(
                "The ROOT skill bootstrap configuration has an empty skill_id ({}).",
                config_path.display()
            ));
        }
        if source.is_empty() {
            return Err(format!(
                "The source of ROOT skill bootstrap entry `{skill_id}` must not be empty."
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
        .ok_or_else(|| format!("LuaSkills skill root `{root_name}` was not found."))
}

/// Parses a WebUI source type key into the LuaSkills install source enum.
/// 将 WebUI 来源类型键解析为 LuaSkills 安装来源枚举。
fn parse_skill_install_source_type(source_type: &str) -> Result<SkillInstallSourceType, String> {
    match source_type.trim().to_ascii_lowercase().as_str() {
        "github" => Ok(SkillInstallSourceType::Github),
        // The source is the Hub skill id; the engine resolves it against the effective Hub.
        // 来源为 Hub 技能标识；引擎按生效 Hub 解析。
        "official_hub" => Ok(SkillInstallSourceType::OfficialHub),
        // Upstream rejects `SkillInstallSourceType::Url` unconditionally by source policy
        // (`skill/manager.rs`: "public URL skill install is disabled by source policy"),
        // for install and for update alike. Accepting it here would only defer that
        // rejection by one layer and surface an upstream policy string instead of a host
        // message, so the dead source type is refused at the host boundary.
        // 上游按来源策略无条件拒绝 `SkillInstallSourceType::Url`（`skill/manager.rs`：
        // "public URL skill install is disabled by source policy"），安装与更新皆然。在此接受它
        // 只会把该拒绝推迟一层，并暴露上游策略文案而不是宿主文案，因此这个已死的来源类型在宿主
        // 边界即被拒绝。
        "url" => Err(String::from(
            "The URL install source has been disabled by the upstream source policy and cannot be used; install the LuaSkills package from a GitHub source instead.",
        )),
        // Naming the supported set keeps a typo recoverable: without it the caller only
        // learns that its value is unknown, not what to send instead.
        // 点名支持集使拼写错误可恢复：否则调用方只知道自己的取值未知，而不知道该改发什么。
        other => Err(format!(
            "Unsupported LuaSkills install source type: {other}; supported: github, official_hub."
        )),
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
        .map_err(|error| format!("Failed to sync the runtime entries after the LuaSkills management operation: {error}"))?;
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
        .map_err(|error| format!("Failed to resolve the skill directories: {error}"))?
        .into_iter()
        .find(|instance| instance.skill_id == skill_id)
        .ok_or_else(|| format!("No effective directory was found for skill `{skill_id}`."))?;
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
            "Skill `{skill_id}` is in the ROOT layer and controlled by the host; it cannot be uninstalled."
        ));
    }
    Ok(resolved)
}

/// Builds LuaSkills host options that route both databases through this process's own engine.
/// 构建使 LuaSkills 两个数据库都经由本进程自身引擎的宿主选项。
fn build_host_options(
    runtime_root: &Path,
    managed_runtime_distribution_root: &Path,
    skill_config_root: &Path,
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
    host_options.official_skill_hub_base_url = Some(
        runtime_settings
            .network
            .effective_official_skill_hub_base_url(),
    );
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
    // The mode comes from the host-provider policy rather than a local literal. It deliberately remains
    // `HostCallback` even before installation, so a broken startup order fails loudly instead of selecting the
    // retired external-controller path.
    // 模式取自宿主 provider 策略而非本地字面量。它在安装前也刻意保持 `HostCallback`，使错误的启动顺序响亮
    // 失败，而不是选择已退役的外部控制器路径。
    //
    // The LuaSkills `space_controller` options keep their crate defaults. That mode talks to an external
    // controller process, which Vulcan Code neither ships nor supports any more.
    // LuaSkills 的 `space_controller` 选项保持该 crate 的默认值。该模式与外部控制器进程通信，
    // 而 Vulcan Code 已不再随附也不再支持它。
    host_options.sqlite_provider_mode =
        vulcan_agent_runtime::luaskills_runtime::host_database_providers::provider_mode();
    host_options.lancedb_provider_mode =
        vulcan_agent_runtime::luaskills_runtime::host_database_providers::provider_mode();
    host_options
}

/// Returns the mutable user skill root shared by Vulcan Code hosts.
/// 返回 Vulcan Code 宿主共享的可变用户技能根目录。
fn default_user_skill_root() -> PathBuf {
    vulcan_workspace_store::data_plane::application_data_root()
        .join("luaskills-user")
        .join("skills")
}

/// Returns the LuaSkills skill download cache root beneath one runtime root.
/// 返回某个运行时根之下的 LuaSkills 技能下载缓存根。
///
/// Vulcan owns only this root: LuaSkills owns the file layout inside it, so no caller
/// may enumerate, prune, or assume a particular cache file path. It is deliberately a
/// sibling of the managed-runtime staging directory rather than a parent, because the
/// managed-runtime installer wipes its own staging directory before each repair.
/// Vulcan 只持有这个根：其内部文件布局由 LuaSkills 持有，因此任何调用方都不得枚举、清理或
/// 假定某个具体缓存文件路径。它刻意与受管运行时暂存目录**平级**而非其父目录，因为受管运行时
/// 安装器在每次修复前会清空它自己的暂存目录。
pub fn luaskills_download_cache_root(runtime_root: &Path) -> PathBuf {
    runtime_root.join("temp").join("downloads")
}

/// Builds a skill manager for one mounted root.
/// 为单个已挂载根构建技能管理器。
///
/// This manager serves read-only catalog metadata only: every caller uses it for
/// `is_skill_enabled`, `disabled_record`, or `install_record`, and never for an
/// install, update, or download. Network download is therefore disabled here rather
/// than left at a permissive default, so the read-only property is enforced by
/// construction instead of relying on no future caller adding a download.
/// The real install path does not use this manager: it goes through the engine,
/// whose `SkillManager` takes `allow_network_download` from `host_options`, which
/// the production bootstrap sets from the user's `network.allow_downloads` setting.
/// 本管理器只服务只读目录元数据：所有调用方都只用它做 `is_skill_enabled`、
/// `disabled_record` 或 `install_record`，绝不用于安装、更新或下载。因此这里禁用网络下载，
/// 而不是留下一处宽松默认值，使只读属性由构造强制，而不是依赖未来没有调用方新增下载。
/// 真实安装路径不使用本管理器：它经引擎，而引擎的 `SkillManager` 从 `host_options` 取
/// `allow_network_download`，生产引导再从用户的 `network.allow_downloads` 设置注入。
fn build_skill_manager_for_root(
    handle: &TauriLuaSkillsHandle,
    root_name: &str,
) -> Result<SkillManager, String> {
    let skill_root = handle
        .skill_roots
        .iter()
        .find(|root| root.name == root_name)
        .cloned()
        .ok_or_else(|| format!("Skill root `{root_name}` was not found."))?;
    let lifecycle_root = skill_root
        .skills_dir
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| skill_root.skills_dir.clone())
        .join("state");

    Ok(SkillManager::new(SkillManagerConfig {
        skill_root,
        lifecycle_root,
        download_cache_root: luaskills_download_cache_root(&handle.runtime_root),
        // Catalog metadata never downloads, so this manager is built without that
        // capability instead of inheriting a permissive default.
        // 目录元数据从不下载，因此本管理器在构建时就不具备该能力，而不是继承宽松默认值。
        allow_network_download: false,
        github_base_url: None,
        github_api_base_url: None,
        github_token: None,
        official_skill_hub_base_url: None,
        enable_private_url_skill_install: false,
        private_skill_source_allowlist: Vec::new(),
    }))
}

/// Reads one LuaSkills public manifest from disk.
/// 从磁盘读取单个 LuaSkills 公开清单。
fn read_skill_meta_from_directory(skill_dir: &Path, skill_id: &str) -> Result<SkillMeta, String> {
    let manifest_path = skill_dir.join("skill.yaml");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|error| {
        format!(
            "Failed to read the skill manifest ({}): {error}",
            manifest_path.display()
        )
    })?;
    serde_yaml::from_str::<SkillMeta>(&manifest_text)
        .map_err(|error| format!("Failed to parse the skill manifest ({skill_id}): {error}"))
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
        // descriptor is deserialized through the public upstream contract to keep the fixture aligned with 0.5.7.
        // descriptor 通过公开上游契约反序列化，使测试夹具与 0.5.7 保持一致。
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

#[cfg(test)]
#[path = "luaskills_bridge/local_github_tests.rs"]
mod local_github_tests;

/// Verifies the skill download cache and the managed-runtime staging root never overlap.
/// 验证技能下载缓存与受管运行时暂存根绝不相交。
///
/// A managed-runtime repair wipes its own staging directory before installing, so any
/// overlap with the skill download cache would destroy cached skill archives. The
/// two roots are siblings under `temp` precisely so this cannot happen.
/// 受管运行时修复在安装前会清空它自己的暂存目录，因此与技能下载缓存的重叠会摧毁已缓存的
/// 技能归档。两个根刻意是 `temp` 下的**平级**目录，正是为了杜绝这种情况。
#[test]
fn download_cache_and_managed_runtime_staging_never_overlap() {
    let runtime_root = PathBuf::from("runtime-root");
    let skill_cache = luaskills_download_cache_root(&runtime_root);
    let staging = crate::managed_runtime_installer::managed_runtime_download_root(&runtime_root);

    assert_ne!(skill_cache, staging);
    // Containment is asserted in both directions, because either nesting order would
    // let one wipe reach the other.
    // 双向断言包含关系，因为任一嵌套顺序都会让一次清空波及另一方。
    assert!(
        !skill_cache.starts_with(&staging),
        "the skill cache must not live inside managed-runtime staging"
    );
    assert!(
        !staging.starts_with(&skill_cache),
        "managed-runtime staging must not live inside the skill cache"
    );
    // The versioned component directory is what a repair actually wipes, so it is
    // asserted separately from the staging root.
    // 修复实际清空的是版本化组件目录，因此它要与暂存根分开断言。
    let component = staging.join("uv-1.2.3-x86_64-pc-windows-msvc");
    assert!(component.starts_with(&staging));
    assert!(!component.starts_with(&skill_cache));
}

/// Verifies a post-commit failure names the mutation that already completed.
/// 验证后置失败会点名已经完成的变更。
///
/// The engine call commits the package change, so an error raised afterwards must not
/// read as "nothing changed": the caller and the operation-progress projection both
/// treat any error as a failed operation.
/// 引擎调用提交了包变更，因此其后抛出的错误不得被读成「什么都没变」：调用方与操作进度投影
/// 都会把任何错误当作操作失败。
#[test]
fn post_commit_failure_names_the_completed_mutation() {
    let message = post_commit_projection_failure(
        "fixture-skill",
        "installed",
        "refreshing the skill catalog failed",
        "catalog unavailable",
    );

    // The skill id and the completed action must both be present, so the message cannot
    // be mistaken for a bare install failure.
    // 技能标识与已完成的动作必须同时出现，使该消息不会被误读为一次裸的安装失败。
    assert!(message.contains("fixture-skill"), "{message}");
    assert!(message.contains("was installed"), "{message}");
    assert!(
        message.contains("refreshing the skill catalog failed"),
        "{message}"
    );
    assert!(message.contains("catalog unavailable"), "{message}");
    // The underlying error is preserved rather than replaced by the wrapper.
    // 底层错误被保留，而不是被包装文案替换。
    assert!(
        !message.contains("Failed to install the LuaSkills skill"),
        "{message}"
    );
}

/// Verifies each mutation reports its own completed action.
/// 验证每种变更报告各自的已完成动作。
#[test]
fn post_commit_failure_distinguishes_each_mutation() {
    for (action, expected) in [
        ("installed", "was installed"),
        ("updated", "was updated"),
        ("uninstalled", "was uninstalled"),
    ] {
        let message = post_commit_projection_failure("fixture", action, "a step failed", "boom");
        assert!(message.contains(expected), "{message}");
    }
    // The three actions must not collapse into one shared label.
    // 三种动作不得塌缩成同一个共享标签。
    let installed = post_commit_projection_failure("f", "installed", "s", "e");
    let updated = post_commit_projection_failure("f", "updated", "s", "e");
    assert_ne!(installed, updated);
}

/// Verifies the host refuses the dead `url` source type instead of deferring rejection.
/// 验证宿主拒绝已死的 `url` 来源类型，而不是把拒绝推迟到上游。
///
/// Upstream rejects `SkillInstallSourceType::Url` unconditionally, so accepting it here
/// would only replace a host message with an upstream policy string after a round trip.
/// 上游无条件拒绝 `SkillInstallSourceType::Url`，因此在此接受它只会在一趟往返之后把宿主
/// 消息换成上游策略文案。
#[test]
fn url_install_source_is_refused_at_the_host_boundary() {
    let error = parse_skill_install_source_type("url")
        .expect_err("upstream rejects the url source type unconditionally");

    // The message must point at a source type that actually works, and must not be the
    // generic unknown-type message, because `url` is a known but disabled source.
    // 消息必须指向真正可用的来源类型，且不得是通用的未知类型消息，因为 `url` 是已知但被停用的
    // 来源。
    assert!(error.contains("GitHub"), "{error}");
    assert!(!error.contains("Unsupported"), "{error}");
    // GitHub stays accepted, and parsing stays case-insensitive and trimmed.
    // GitHub 保持被接受，且解析保持大小写不敏感与裁剪。
    assert!(parse_skill_install_source_type(" GitHub ").is_ok());
    assert!(parse_skill_install_source_type("GITHUB").is_ok());
    // The official Hub maps onto its own source type, never onto GitHub.
    // 官方 Hub 映射到自身来源类型，绝不映射到 GitHub。
    assert!(matches!(
        parse_skill_install_source_type(" official_hub "),
        Ok(SkillInstallSourceType::OfficialHub)
    ));
    // Source types upstream supports but the host does not implement stay unknown
    // rather than being silently mapped onto GitHub.
    // 上游支持但宿主未实现的来源类型保持未知，而不是被静默映射到 GitHub。
    let unknown = parse_skill_install_source_type("private_url_manifest")
        .expect_err("an unimplemented source type must not be accepted");
    assert!(unknown.contains("private_url_manifest"), "{unknown}");
    // Naming the supported set keeps a typo recoverable: the caller learns what to
    // send instead of only that its own value is unknown.
    // 点名支持集使拼写错误可恢复：调用方得知该改发什么，而不只是自己的取值未知。
    assert!(unknown.contains("github"), "{unknown}");
    assert!(unknown.contains("official_hub"), "{unknown}");
}

/// Verifies a ROOT bootstrap declaring the disabled `url` source type fails with a host message.
/// 验证声明了已停用 `url` 来源类型的 ROOT 引导会以宿主消息失败。
///
/// The bootstrap path reaches upstream through the System plane, but upstream still
/// rejects `SkillInstallSourceType::Url` unconditionally, so converting it would fail on
/// every start with an upstream policy string.
/// 引导路径经 System 面到达上游，但上游仍无条件拒绝 `SkillInstallSourceType::Url`，因此转换
/// 它会在每次启动时以上游策略文案失败。
#[test]
fn root_bootstrap_refuses_the_disabled_url_source_type() {
    assert!(
        RootSkillBootstrapSourceType::Github
            .into_runtime_source_type()
            .is_ok()
    );
    let error = RootSkillBootstrapSourceType::Url
        .into_runtime_source_type()
        .expect_err("upstream rejects the url source type unconditionally");

    // The message must name the replacement source type instead of only refusing.
    // 消息必须点名替代的来源类型，而不是只做拒绝。
    assert!(error.contains("github"), "{error}");
    assert!(error.contains("url"), "{error}");
}

/// Verifies the disabled variant still deserializes from configuration.
/// 验证已停用的变体仍可从配置反序列化。
///
/// Removing the variant outright would turn an existing configuration into an opaque
/// serde `unknown variant` error at parse time, before the reason above can be reported.
/// 直接移除该变体会让既有配置在解析期就变成一条不可读的 serde `unknown variant` 错误，
/// 使上面的原因无从报告。
#[test]
fn disabled_root_bootstrap_source_type_still_deserializes() {
    let parsed: RootSkillBootstrapSourceType =
        serde_json::from_str("\"url\"").expect("an existing `url` declaration must still parse");
    assert!(matches!(parsed, RootSkillBootstrapSourceType::Url));
    let github: RootSkillBootstrapSourceType =
        serde_json::from_str("\"github\"").expect("the supported declaration must parse");
    assert!(matches!(github, RootSkillBootstrapSourceType::Github));
}
