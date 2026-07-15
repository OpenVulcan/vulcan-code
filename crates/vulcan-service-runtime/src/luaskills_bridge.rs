//! Minimal LuaSkills bootstrap used by the Core Service runtime.
//! Core Service 运行时使用的最小 LuaSkills 引导模块。

use std::{
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
    LuaVmPoolConfig, RuntimeEntryDescriptor, RuntimeSkillRoot, SkillInstallRequest,
    SkillInstallSourceType, SkillManagementAuthority, SkillManager, SkillManagerConfig, SkillMeta,
    SkillUninstallOptions, ToolCacheConfig,
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
                "未找到 LuaSkills 运行时根目录，请先执行 make build 生成 output/lua_runtime。",
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
        .map(str::to_string);
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
