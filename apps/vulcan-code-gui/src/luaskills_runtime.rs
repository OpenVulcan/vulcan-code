//! LuaSkills runtime bootstrap helpers layered on top of the shared controller client.
//! 构建在共享控制器客户端之上的 LuaSkills 运行时引导辅助模块。

use crate::provider_config::{PersistedControllerConfiguration, PersistedControllerProcessMode};
use crate::runtime_assets;
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaRuntimeDatabaseProviderMode, LuaRuntimeHostOptions,
    LuaRuntimeSpaceControllerProcessMode, LuaVmPoolConfig, RuntimeSkillRoot, SkillInstallRequest,
    SkillInstallSourceType, SkillManager, SkillManagerConfig, SkillMeta, SkillUninstallOptions,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Long-lived LuaSkills runtime handle kept by the GUI after startup succeeds.
/// 启动成功后由图形界面保留的长生命周期 LuaSkills 运行时句柄。
#[derive(Clone)]
pub struct LuaSkillsRuntimeHandle {
    /// Shared LuaSkills engine reused by later skill-facing panels and workflows.
    /// 供后续技能面板与工作流复用的共享 LuaSkills 引擎。
    #[allow(dead_code)]
    pub engine: Arc<Mutex<LuaEngine>>,
    /// Stable runtime root reserved for bundled LuaSkills resources and state.
    /// 为内建 LuaSkills 资源与状态保留的稳定运行时根目录。
    pub runtime_root: PathBuf,
    /// Stable system Lua library directory reserved for host-owned persistent runtimes.
    /// 为宿主自有持久运行时保留的稳定 system Lua 库目录。
    pub system_lua_lib_dir: PathBuf,
    /// Ordered skill roots currently mounted into the engine.
    /// 当前挂载到引擎中的有序技能根目录集合。
    pub skill_roots: Vec<RuntimeSkillRoot>,
    /// Number of runtime entries currently visible to the host.
    /// 当前对宿主可见的运行时入口数量。
    pub entry_count: usize,
}

impl std::fmt::Debug for LuaSkillsRuntimeHandle {
    /// Formats the runtime handle without dumping opaque engine internals.
    /// 在不输出不透明引擎内部状态的前提下格式化运行时句柄。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LuaSkillsRuntimeHandle")
            .field("runtime_root", &self.runtime_root)
            .field("system_lua_lib_dir", &self.system_lua_lib_dir)
            .field("skill_roots", &self.skill_roots)
            .field("entry_count", &self.entry_count)
            .finish()
    }
}

/// Summary returned after one LuaSkills bootstrap finishes.
/// 单次 LuaSkills 引导完成后返回的摘要信息。
#[derive(Debug, Clone)]
pub struct LuaSkillsBootstrapReport {
    /// Long-lived runtime handle retained by the GUI.
    /// 由图形界面保留的长生命周期运行时句柄。
    pub handle: LuaSkillsRuntimeHandle,
}

impl LuaSkillsBootstrapReport {
    /// Builds one concise human-readable summary for the footer and activity log.
    /// 为底部状态栏与活动日志构建简洁的人类可读摘要。
    pub fn summary_text(&self) -> String {
        format!(
            "LuaSkills 运行时已就绪：{} · 技能根 {} · 入口 {}",
            self.handle.runtime_root.display(),
            self.handle.skill_roots.len(),
            self.handle.entry_count
        )
    }
}

/// Catalog summarizing the effective LuaSkills visible to the current host runtime.
/// 汇总当前宿主运行时可见生效 LuaSkills 的技能目录清单。
#[derive(Debug, Clone)]
pub struct LuaSkillsCatalog {
    /// Stable runtime root currently owned by the embedded host integration.
    /// 当前嵌入式宿主集成所拥有的稳定运行时根目录。
    pub runtime_root: PathBuf,
    /// Stable system Lua library directory reserved for host-owned resident runtimes.
    /// 为宿主自有常驻运行时预留的稳定 system Lua 库目录。
    pub system_lua_lib_dir: PathBuf,
    /// Effective skill summaries resolved after root precedence and enable-state checks.
    /// 在根目录优先级与启用状态检查后解析出的生效技能摘要集合。
    pub skills: Vec<LuaSkillsCatalogSkill>,
    /// Total number of currently loaded runtime entries.
    /// 当前已加载运行时入口总数。
    pub entry_count: usize,
}

/// One visible LuaSkills summary row used by the desktop management panel.
/// 桌面管理面板使用的单条可见 LuaSkills 摘要行。
#[derive(Debug, Clone)]
pub struct LuaSkillsCatalogSkill {
    /// Stable skill identifier derived from the physical directory name.
    /// 从物理目录名称派生出的稳定技能标识符。
    pub skill_id: String,
    /// Human-readable skill package name declared by `skill.yaml`.
    /// `skill.yaml` 中声明的人类可读技能包名称。
    pub display_name: String,
    /// Semantic version currently declared by the skill manifest.
    /// 当前技能清单声明的语义化版本。
    pub version: String,
    /// Named runtime root currently owning the effective instance.
    /// 当前拥有该生效实例的命名运行时根。
    pub root_name: String,
    /// Physical effective skill directory.
    /// 当前生效的物理技能目录。
    pub skill_dir: PathBuf,
    /// Whether the current skill is enabled through the lifecycle manager state.
    /// 当前技能是否通过生命周期管理状态保持启用。
    pub enabled: bool,
    /// Optional persisted disable reason captured by the skill manager.
    /// 由技能管理器记录的可选停用原因。
    pub disabled_reason: Option<String>,
    /// Whether the current skill has one managed install record.
    /// 当前技能是否拥有一份受管安装记录。
    pub managed: bool,
    /// Optional managed install source type when the current skill is managed.
    /// 当前技能为受管技能时对应的可选安装来源类型。
    pub source_type: Option<SkillInstallSourceType>,
    /// Optional managed source locator when the current skill is managed.
    /// 当前技能为受管技能时对应的可选来源定位值。
    pub source_locator: Option<String>,
    /// Number of currently loaded runtime entries belonging to the current skill.
    /// 当前技能所属且已加载的运行时入口数量。
    pub entry_count: usize,
    /// Whether the current skill lives in the host-controlled ROOT layer.
    /// 当前技能是否位于宿主控制的 ROOT 层。
    pub system_owned: bool,
}

/// Install source choice exposed by the desktop panel.
/// 桌面面板暴露的安装来源选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuaSkillsInstallSourceChoice {
    /// Install one managed skill from a GitHub repository release.
    /// 从 GitHub 仓库发布资产安装一个受管技能。
    Github,
    /// Install one managed skill from a remote package/source URL.
    /// 从远程包地址或 source URL 安装一个受管技能。
    Url,
}

impl LuaSkillsInstallSourceChoice {
    /// Return the user-facing label of the current install source choice.
    /// 返回当前安装来源选项的用户可见标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Github => "GitHub",
            Self::Url => "包地址",
        }
    }

    /// Convert the panel-side install source choice into the upstream LuaSkills source enum.
    /// 将面板侧安装来源选项转换成上游 LuaSkills 来源枚举。
    pub fn into_runtime_source_type(self) -> SkillInstallSourceType {
        match self {
            Self::Github => SkillInstallSourceType::Github,
            Self::Url => SkillInstallSourceType::Url,
        }
    }
}

/// One host-side management action executed against the embedded LuaSkills runtime.
/// 针对嵌入式 LuaSkills 运行时执行的单次宿主管理动作。
#[derive(Debug, Clone)]
pub enum LuaSkillsManagementAction {
    /// Reload skill roots and rebuild the visible catalog.
    /// 重载技能根并重建可见清单。
    Refresh,
    /// Install one skill into the ordinary USER root.
    /// 向普通 USER 根安装单个技能。
    Install {
        /// Optional explicit skill id supplied by the user.
        /// 由用户提供的可选显式技能标识符。
        skill_id: Option<String>,
        /// Raw source locator such as `owner/repo` or one package URL.
        /// 原始来源定位值，例如 `owner/repo` 或某个包地址。
        source: String,
        /// Install source type selected in the panel.
        /// 面板中选择的安装来源类型。
        source_type: LuaSkillsInstallSourceChoice,
    },
    /// Update one already installed managed skill.
    /// 更新一个已安装的受管技能。
    Update {
        /// Stable target skill identifier.
        /// 目标技能的稳定标识符。
        skill_id: String,
    },
    /// Enable one previously disabled ordinary skill.
    /// 启用一个此前被停用的普通技能。
    Enable {
        /// Stable target skill identifier.
        /// 目标技能的稳定标识符。
        skill_id: String,
    },
    /// Disable one currently enabled ordinary skill.
    /// 停用一个当前处于启用状态的普通技能。
    Disable {
        /// Stable target skill identifier.
        /// 目标技能的稳定标识符。
        skill_id: String,
    },
    /// Uninstall one ordinary skill from the USER root while retaining databases by default.
    /// 从 USER 根卸载一个普通技能，并默认保留数据库目录。
    Uninstall {
        /// Stable target skill identifier.
        /// 目标技能的稳定标识符。
        skill_id: String,
    },
}

/// Structured host-side outcome returned after one management action completes.
/// 单次管理动作完成后返回的结构化宿主结果。
#[derive(Debug, Clone)]
pub struct LuaSkillsManagementReport {
    /// Human-readable summary message describing the finished management action.
    /// 描述已完成管理动作的人类可读摘要消息。
    pub message: String,
    /// Freshly rebuilt skill catalog captured after the management action.
    /// 在管理动作完成后重新构建得到的最新技能清单。
    pub catalog: LuaSkillsCatalog,
}

/// Bootstraps one minimal LuaSkills runtime after the controller data plane is ready.
/// 在控制器数据平面就绪后引导一个最小化的 LuaSkills 运行时。
pub async fn bootstrap_luaskills_runtime(
    controller: PersistedControllerConfiguration,
) -> Result<LuaSkillsBootstrapReport, String> {
    let runtime_root = application_luaskills_runtime_root();
    runtime_assets::sync_bundled_luaskills_runtime(&runtime_root)?;
    let skill_roots = ensure_runtime_root_layout(&runtime_root)?;
    let system_lua_lib_dir = runtime_root.join("system_lua_lib");
    let host_options = build_host_options(&runtime_root, controller);
    let pool_config = LuaVmPoolConfig {
        min_size: 1,
        max_size: 2,
        idle_ttl_secs: 30,
    };
    let mut engine = LuaEngine::new(LuaEngineOptions::new(pool_config, host_options))
        .map_err(|error| format!("创建 LuaSkills 引擎失败：{error}"))?;

    engine
        .load_from_roots(&skill_roots)
        .map_err(|error| format!("加载 LuaSkills 技能根失败：{error}"))?;

    let entry_count = engine.list_entries().len();

    Ok(LuaSkillsBootstrapReport {
        handle: LuaSkillsRuntimeHandle {
            engine: Arc::new(Mutex::new(engine)),
            runtime_root,
            system_lua_lib_dir,
            skill_roots,
            entry_count,
        },
    })
}

/// Returns the stable runtime root used by the embedded LuaSkills host integration.
/// 返回嵌入式 LuaSkills 宿主集成使用的稳定运行时根目录。
pub fn application_luaskills_runtime_root() -> PathBuf {
    crate::data_plane::application_data_root().join("luaskills-runtime")
}

/// Rebuild the visible LuaSkills catalog from the current runtime roots without mutating lifecycle state.
/// 在不修改生命周期状态的前提下，根据当前运行时根重建可见 LuaSkills 清单。
pub fn load_luaskills_catalog(handle: &LuaSkillsRuntimeHandle) -> Result<LuaSkillsCatalog, String> {
    let engine = lock_runtime_engine(handle)?;
    build_luaskills_catalog(handle, &engine)
}

/// Execute one host-side LuaSkills management action and rebuild the visible catalog afterwards.
/// 执行单次宿主侧 LuaSkills 管理动作，并在完成后重建可见清单。
pub fn perform_luaskills_management_action(
    handle: &LuaSkillsRuntimeHandle,
    action: LuaSkillsManagementAction,
) -> Result<LuaSkillsManagementReport, String> {
    let mut engine = lock_runtime_engine_mut(handle)?;
    let message = match action {
        LuaSkillsManagementAction::Refresh => {
            engine
                .reload_from_roots(&handle.skill_roots)
                .map_err(|error| format!("刷新 LuaSkills 运行时失败：{error}"))?;
            String::from("LuaSkills 技能目录已刷新。")
        }
        LuaSkillsManagementAction::Install {
            skill_id,
            source,
            source_type,
        } => {
            let user_root = ordinary_user_root(handle)?;
            let request = SkillInstallRequest {
                skill_id: normalize_optional_field(skill_id),
                source: normalize_optional_field(Some(source)),
                source_type: source_type.into_runtime_source_type(),
            };
            let result = engine
                .install_skill_in_root(&handle.skill_roots, &user_root, &request)
                .map_err(|error| format!("安装 LuaSkills 技能失败：{error}"))?;
            result.message
        }
        LuaSkillsManagementAction::Update { skill_id } => {
            let target_root = ordinary_target_root_for_skill(handle, &skill_id)?;
            let request = SkillInstallRequest {
                skill_id: Some(skill_id.clone()),
                source: None,
                source_type: SkillInstallSourceType::Github,
            };
            let result = engine
                .update_skill_in_root(&handle.skill_roots, &target_root, &request)
                .map_err(|error| format!("更新 LuaSkills 技能失败：{error}"))?;
            result.message
        }
        LuaSkillsManagementAction::Enable { skill_id } => {
            ordinary_operation_guard(handle, &skill_id)?;
            engine
                .enable_skill(&handle.skill_roots, &skill_id)
                .map_err(|error| format!("启用 LuaSkills 技能失败：{error}"))?;
            format!("已启用技能 `{skill_id}`。")
        }
        LuaSkillsManagementAction::Disable { skill_id } => {
            ordinary_operation_guard(handle, &skill_id)?;
            engine
                .disable_skill_in_roots(
                    &handle.skill_roots,
                    &skill_id,
                    Some("disabled from vulcan-code-gui"),
                )
                .map_err(|error| format!("停用 LuaSkills 技能失败：{error}"))?;
            format!("已停用技能 `{skill_id}`。")
        }
        LuaSkillsManagementAction::Uninstall { skill_id } => {
            let target_root = ordinary_target_root_for_skill(handle, &skill_id)?;
            let result = engine
                .uninstall_skill_in_root(
                    &handle.skill_roots,
                    &target_root,
                    &skill_id,
                    &SkillUninstallOptions::default(),
                )
                .map_err(|error| format!("卸载 LuaSkills 技能失败：{error}"))?;
            result.message
        }
    };

    let catalog = build_luaskills_catalog(handle, &engine)?;
    Ok(LuaSkillsManagementReport { message, catalog })
}

/// Ensures the minimum runtime directory layout exists and returns the mounted skill roots.
/// 确保最小运行时目录布局存在，并返回需要挂载的技能根集合。
fn ensure_runtime_root_layout(runtime_root: &Path) -> Result<Vec<RuntimeSkillRoot>, String> {
    let temp_dir = runtime_root.join("temp");
    let resources_dir = runtime_root.join("resources");
    let lua_packages_dir = runtime_root.join("lua_packages");
    let libs_root = runtime_root.join("libs");
    let tool_root = runtime_root.join("bin").join("tools");
    let download_root = temp_dir.join("downloads");
    let root_skills_dir = runtime_root.join("skills").join("root");
    let user_skills_dir = runtime_root.join("skills").join("user");
    let system_lua_lib_dir = runtime_root.join("system_lua_lib");

    for directory in [
        runtime_root,
        temp_dir.as_path(),
        resources_dir.as_path(),
        lua_packages_dir.as_path(),
        libs_root.as_path(),
        tool_root.as_path(),
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

/// Builds one host option object that forces LuaSkills to route both databases through the controller.
/// 构建一份宿主选项对象，强制 LuaSkills 通过控制器路由 SQLite 与 LanceDB。
fn build_host_options(
    runtime_root: &Path,
    controller: PersistedControllerConfiguration,
) -> LuaRuntimeHostOptions {
    let mut host_options = LuaRuntimeHostOptions::default();
    host_options.temp_dir = Some(runtime_root.join("temp"));
    host_options.resources_dir = Some(runtime_root.join("resources"));
    host_options.lua_packages_dir = Some(runtime_root.join("lua_packages"));
    host_options.host_provided_lua_root = Some(runtime_root.join("lua_packages"));
    host_options.host_provided_ffi_root = Some(runtime_root.join("libs"));
    host_options.host_provided_tool_root = Some(runtime_root.join("bin").join("tools"));
    host_options.system_lua_lib_dir = Some(runtime_root.join("system_lua_lib"));
    host_options.download_cache_root = Some(runtime_root.join("temp").join("downloads"));
    host_options.dependency_dir_name = String::from("dependencies");
    host_options.state_dir_name = String::from("state");
    host_options.database_dir_name = String::from("databases");
    host_options.skill_config_file_path = Some(runtime_root.join("skill-config.json"));
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
        .or_else(runtime_assets::resolve_bundled_controller_executable_path);
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

/// Acquire one immutable runtime-engine guard from the shared host handle.
/// 从共享宿主句柄中获取一份只读运行时引擎锁守卫。
fn lock_runtime_engine(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<std::sync::MutexGuard<'_, LuaEngine>, String> {
    handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 运行时锁已损坏，无法读取技能目录。"))
}

/// Acquire one mutable runtime-engine guard from the shared host handle.
/// 从共享宿主句柄中获取一份可变运行时引擎锁守卫。
fn lock_runtime_engine_mut(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<std::sync::MutexGuard<'_, LuaEngine>, String> {
    handle
        .engine
        .lock()
        .map_err(|_| String::from("LuaSkills 运行时锁已损坏，无法执行技能管理操作。"))
}

/// Build one normalized skill catalog from the active engine and the current runtime root layout.
/// 根据活动引擎与当前运行时根布局构建一份规范化技能清单。
fn build_luaskills_catalog(
    handle: &LuaSkillsRuntimeHandle,
    engine: &LuaEngine,
) -> Result<LuaSkillsCatalog, String> {
    let entry_counts =
        engine
            .list_entries()
            .into_iter()
            .fold(BTreeMap::new(), |mut counts, entry| {
                *counts.entry(entry.skill_id).or_insert(0) += 1;
                counts
            });
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
                Some(record.source.source_type),
                Some(record.source.locator),
            ),
            None => (false, None, None),
        };

        skills.push(LuaSkillsCatalogSkill {
            skill_id: resolved.skill_id.clone(),
            display_name: meta.name.clone(),
            version: meta.version().to_string(),
            root_name: resolved.root_name.clone(),
            skill_dir: resolved.actual_dir.clone(),
            enabled,
            disabled_reason,
            managed,
            source_type,
            source_locator,
            entry_count: entry_counts.get(&resolved.skill_id).copied().unwrap_or(0),
            system_owned: resolved.root_name.eq_ignore_ascii_case("ROOT"),
        });
    }

    skills.sort_by(|left, right| {
        left.root_name
            .cmp(&right.root_name)
            .then_with(|| left.skill_id.cmp(&right.skill_id))
    });

    Ok(LuaSkillsCatalog {
        runtime_root: handle.runtime_root.clone(),
        system_lua_lib_dir: handle.system_lua_lib_dir.clone(),
        entry_count: engine.list_entries().len(),
        skills,
    })
}

/// Resolve the ordinary USER root used by the desktop panel for managed install actions.
/// 解析桌面面板用于受管安装动作的普通 USER 根。
fn ordinary_user_root(handle: &LuaSkillsRuntimeHandle) -> Result<RuntimeSkillRoot, String> {
    handle
        .skill_roots
        .iter()
        .find(|root| root.name.eq_ignore_ascii_case("USER"))
        .cloned()
        .ok_or_else(|| String::from("当前运行时缺少 USER 技能根，无法执行普通安装。"))
}

/// Guard one ordinary skill-management operation against ROOT-owned effective skills.
/// 在执行普通技能管理操作前拦截由 ROOT 拥有的生效技能。
fn ordinary_operation_guard(handle: &LuaSkillsRuntimeHandle, skill_id: &str) -> Result<(), String> {
    let resolved = resolve_skill_root_for_id(handle, skill_id)?;
    if resolved.name.eq_ignore_ascii_case("ROOT") {
        return Err(format!(
            "技能 `{skill_id}` 当前位于 ROOT 层，由宿主内建控制，不能通过普通管理面直接操作。"
        ));
    }
    Ok(())
}

/// Resolve the effective ordinary target root that owns one skill identifier.
/// 解析拥有目标技能标识符的当前生效普通根目录。
fn ordinary_target_root_for_skill(
    handle: &LuaSkillsRuntimeHandle,
    skill_id: &str,
) -> Result<RuntimeSkillRoot, String> {
    let resolved = resolve_skill_root_for_id(handle, skill_id)?;
    if resolved.name.eq_ignore_ascii_case("ROOT") {
        return Err(format!(
            "技能 `{skill_id}` 当前位于 ROOT 层，由宿主内建控制，不能通过普通管理面直接操作。"
        ));
    }
    Ok(resolved)
}

/// Resolve the owning runtime root of one effective skill identifier from the current root chain.
/// 从当前根目录链中解析单个生效技能标识符所属的运行时根。
fn resolve_skill_root_for_id(
    handle: &LuaSkillsRuntimeHandle,
    skill_id: &str,
) -> Result<RuntimeSkillRoot, String> {
    let resolved = collect_effective_skill_instances_from_roots(&handle.skill_roots)
        .map_err(|error| format!("解析技能目录失败：{error}"))?
        .into_iter()
        .find(|instance| instance.skill_id == skill_id)
        .ok_or_else(|| format!("未找到技能 `{skill_id}` 对应的生效目录。"))?;
    handle
        .skill_roots
        .iter()
        .find(|root| root.name == resolved.root_name)
        .cloned()
        .ok_or_else(|| format!("未找到技能 `{skill_id}` 对应的运行时根配置。"))
}

/// Build one skill-manager instance that mirrors the runtime root layout used by the embedded host.
/// 构建一个与嵌入式宿主运行时根布局保持一致的技能管理器实例。
fn build_skill_manager_for_root(
    handle: &LuaSkillsRuntimeHandle,
    root_name: &str,
) -> Result<SkillManager, String> {
    let skill_root = handle
        .skill_roots
        .iter()
        .find(|root| root.name == root_name)
        .cloned()
        .ok_or_else(|| format!("未找到技能根 `{root_name}`。"))?;
    Ok(SkillManager::new(SkillManagerConfig {
        skill_root,
        lifecycle_root: handle.runtime_root.join("state"),
        download_cache_root: handle.runtime_root.join("temp").join("downloads"),
        allow_network_download: true,
        github_base_url: None,
        github_api_base_url: None,
    }))
}

/// Read and normalize one public skill manifest from the specified physical skill directory.
/// 从指定物理技能目录读取并规范化一份公开技能清单。
fn read_skill_meta_from_directory(skill_dir: &Path, skill_id: &str) -> Result<SkillMeta, String> {
    let manifest_path = skill_dir.join("skill.yaml");
    let manifest_text = fs::read_to_string(&manifest_path)
        .map_err(|error| format!("读取技能清单失败（{}）：{error}", manifest_path.display()))?;
    let mut meta: SkillMeta = serde_yaml::from_str(&manifest_text)
        .map_err(|error| format!("解析技能清单失败（{}）：{error}", manifest_path.display()))?;
    meta.bind_directory_skill_id(skill_id.to_string());
    Ok(meta)
}

/// Trim one optional free-form input field and drop it when the result is empty.
/// 规范化单个可选自由输入字段，并在结果为空时将其丢弃。
fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value
        .map(|field| field.trim().to_string())
        .filter(|field| !field.is_empty())
}
