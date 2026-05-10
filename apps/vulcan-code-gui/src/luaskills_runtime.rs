//! LuaSkills runtime bootstrap helpers layered on top of the shared controller client.
//! 构建在共享控制器客户端之上的 LuaSkills 运行时引导辅助模块。

use crate::provider_config::{PersistedControllerConfiguration, PersistedControllerProcessMode};
use crate::runtime_assets;
use iced::futures::channel::oneshot;
use luaskills::runtime_result::RuntimeHostResult;
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaInvocationContext, LuaRuntimeDatabaseProviderMode,
    LuaRuntimeHostOptions, LuaRuntimeSpaceControllerProcessMode, LuaVmPoolConfig,
    RuntimeClientInfo, RuntimeEntryDescriptor, RuntimeInvocationResult, RuntimeRequestContext,
    RuntimeSkillRoot, SkillInstallRequest, SkillInstallSourceType, SkillManagementAuthority,
    SkillManager, SkillManagerConfig, SkillMeta, SkillUninstallOptions,
};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Relative configuration file path that declares the host-managed ROOT skill bootstrap set.
/// 声明宿主管理 ROOT 技能引导集合的相对配置文件路径。
const ROOT_SKILL_BOOTSTRAP_CONFIG_RELATIVE_PATH: &str = "config/luaskills_root_skills.yaml";

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

/// Bootstraps one minimal LuaSkills runtime after the controller data plane is ready.
/// 在控制器数据平面就绪后引导一个最小化的 LuaSkills 运行时。
pub async fn bootstrap_luaskills_runtime(
    controller: PersistedControllerConfiguration,
) -> Result<LuaSkillsBootstrapReport, String> {
    run_luaskills_blocking_task("luaskills-bootstrap", move || {
        bootstrap_luaskills_runtime_blocking(controller)
    })
}

/// Bootstraps one minimal LuaSkills runtime inside one blocking host thread.
/// 在单个阻塞宿主线程内部引导一个最小化的 LuaSkills 运行时。
fn bootstrap_luaskills_runtime_blocking(
    controller: PersistedControllerConfiguration,
) -> Result<LuaSkillsBootstrapReport, String> {
    let runtime_root = application_luaskills_runtime_root().ok_or_else(|| {
        String::from("未找到 LuaSkills 活动运行根，请先执行 make build 并从 output 目录启动应用。")
    })?;
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

    ensure_configured_root_skills_installed(&runtime_root, &skill_roots, &mut engine)?;

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

/// Execute one LuaSkills host operation on a dedicated blocking thread and return its result.
/// 在专用阻塞线程上执行单次 LuaSkills 宿主操作，并返回其结果。
fn run_luaskills_blocking_task<T, F>(thread_name: &str, operation: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    std::thread::Builder::new()
        .name(thread_name.to_string())
        .spawn(operation)
        .map_err(|error| format!("启动 LuaSkills 阻塞线程失败：{error}"))?
        .join()
        .map_err(|panic_payload| {
            let panic_message = if let Some(message) = panic_payload.downcast_ref::<String>() {
                message.clone()
            } else if let Some(message) = panic_payload.downcast_ref::<&'static str>() {
                (*message).to_string()
            } else {
                String::from("未知 panic")
            };
            format!("LuaSkills 阻塞线程执行失败：{panic_message}")
        })?
}

/// Execute one LuaSkills host operation on a dedicated blocking thread without stalling the async caller.
/// 在专用阻塞线程上执行单次 LuaSkills 宿主操作，并避免阻塞异步调用方。
async fn run_luaskills_blocking_task_async<T, F>(
    thread_name: &str,
    operation: F,
) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    let (sender, receiver) = oneshot::channel();
    let thread_name = thread_name.to_string();

    std::thread::Builder::new()
        .name(thread_name.clone())
        .spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation))
                .map_err(|panic_payload| {
                    let panic_message =
                        if let Some(message) = panic_payload.downcast_ref::<String>() {
                            message.clone()
                        } else if let Some(message) = panic_payload.downcast_ref::<&'static str>() {
                            (*message).to_string()
                        } else {
                            String::from("未知 panic")
                        };
                    format!("LuaSkills 阻塞线程执行失败：{panic_message}")
                })
                .and_then(|value| value);
            let _ = sender.send(result);
        })
        .map_err(|error| format!("启动 LuaSkills 阻塞线程失败：{error}"))?;

    receiver
        .await
        .map_err(|_| format!("LuaSkills 阻塞线程 `{thread_name}` 意外断开。"))?
}

/// Ensures the configured host-managed ROOT skills are installed before the runtime catalog is loaded.
/// 在加载运行时技能目录之前，确保配置声明的宿主管理 ROOT 技能已经完成安装。
fn ensure_configured_root_skills_installed(
    runtime_root: &Path,
    skill_roots: &[RuntimeSkillRoot],
    engine: &mut LuaEngine,
) -> Result<(), String> {
    let root_target = resolve_named_skill_root(skill_roots, "ROOT")?;
    let configured_skills = load_root_skill_bootstrap_entries(runtime_root)?;
    if configured_skills.is_empty() {
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
        installed_any_skill = true;
    }

    if installed_any_skill {
        engine
            .reload_from_roots(skill_roots)
            .map_err(|error| format!("刷新 LuaSkills 技能根失败：{error}"))?;
    }

    Ok(())
}

/// Loads the declarative host-managed ROOT skill bootstrap list from the active runtime config directory.
/// 从活动运行时配置目录加载声明式宿主管理 ROOT 技能引导列表。
fn load_root_skill_bootstrap_entries(
    runtime_root: &Path,
) -> Result<Vec<RootSkillBootstrapEntry>, String> {
    let mut candidate_paths = vec![runtime_root.join(ROOT_SKILL_BOOTSTRAP_CONFIG_RELATIVE_PATH)];
    let source_config_path =
        crate::runtime_assets::source_config_directory().join("luaskills_root_skills.yaml");
    if candidate_paths[0] != source_config_path {
        candidate_paths.push(source_config_path);
    }

    let Some(config_path) = candidate_paths.into_iter().find(|path| path.exists()) else {
        return Ok(Vec::new());
    };
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
                "ROOT 技能引导配置 `{}` 的 source 不能为空。",
                skill_id
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

/// Returns the stable runtime root used by the embedded LuaSkills host integration.
/// 返回嵌入式 LuaSkills 宿主集成使用的稳定运行时根目录。
pub fn application_luaskills_runtime_root() -> Option<PathBuf> {
    runtime_assets::resolve_luaskills_runtime_root()
}

/// Rebuild the visible LuaSkills catalog from the current runtime roots without mutating lifecycle state.
/// 在不修改生命周期状态的前提下，根据当前运行时根重建可见 LuaSkills 清单。
pub async fn load_luaskills_catalog(
    handle: LuaSkillsRuntimeHandle,
) -> Result<LuaSkillsCatalog, String> {
    run_luaskills_blocking_task("luaskills-catalog", move || {
        load_luaskills_catalog_blocking(&handle)
    })
}

/// Rebuild the visible LuaSkills catalog inside one blocking host thread.
/// 在单个阻塞宿主线程内部重建可见 LuaSkills 技能目录清单。
fn load_luaskills_catalog_blocking(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<LuaSkillsCatalog, String> {
    let engine = lock_runtime_engine(handle)?;
    build_luaskills_catalog(handle, &engine)
}

/// Execute one host-side LuaSkills management action and rebuild the visible catalog afterwards.
/// 执行单次宿主侧 LuaSkills 管理动作，并在完成后重建可见清单。
pub async fn perform_luaskills_management_action(
    handle: LuaSkillsRuntimeHandle,
    action: LuaSkillsManagementAction,
) -> Result<LuaSkillsManagementReport, String> {
    run_luaskills_blocking_task("luaskills-management", move || {
        perform_luaskills_management_action_blocking(&handle, action)
    })
}

/// Execute one host-side LuaSkills management action inside one blocking host thread.
/// 在单个阻塞宿主线程内部执行单次宿主侧 LuaSkills 管理动作。
fn perform_luaskills_management_action_blocking(
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
            let target_root = target_root_for_skill(handle, &skill_id)?;
            let request = SkillInstallRequest {
                skill_id: Some(skill_id.clone()),
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
            result.message
        }
        LuaSkillsManagementAction::Enable { skill_id } => {
            let target_root = target_root_for_skill(handle, &skill_id)?;
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
            format!("已启用技能 `{skill_id}`。")
        }
        LuaSkillsManagementAction::Disable { skill_id } => {
            let target_root = target_root_for_skill(handle, &skill_id)?;
            if target_root.name.eq_ignore_ascii_case("ROOT") {
                engine
                    .system_disable_skill_in_roots(
                        &handle.skill_roots,
                        SkillManagementAuthority::System,
                        &skill_id,
                        Some("disabled from vulcan-code-gui"),
                    )
                    .map_err(|error| format!("停用 LuaSkills 技能失败：{error}"))?;
            } else {
                engine
                    .disable_skill_in_roots(
                        &handle.skill_roots,
                        &skill_id,
                        Some("disabled from vulcan-code-gui"),
                    )
                    .map_err(|error| format!("停用 LuaSkills 技能失败：{error}"))?;
            }
            format!("已停用技能 `{skill_id}`。")
        }
        LuaSkillsManagementAction::Uninstall { skill_id } => {
            let target_root = ordinary_uninstall_target_root_for_skill(handle, &skill_id)?;
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

/// List every LuaSkills runtime entry currently visible to the host tool surface.
/// 列出当前对宿主工具表面可见的全部 LuaSkills 运行时入口。
pub fn list_luaskills_tool_entries(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<Vec<RuntimeEntryDescriptor>, String> {
    let engine = lock_runtime_engine(handle)?;
    Ok(engine.list_entries_for_authority(SkillManagementAuthority::System))
}

/// Call one LuaSkills runtime entry on a dedicated blocking thread and return the structured result.
/// 在专用阻塞线程上调用单个 LuaSkills 运行时入口，并返回结构化结果。
pub async fn call_luaskills_tool_entry(
    handle: LuaSkillsRuntimeHandle,
    tool_name: String,
    args: Value,
    invocation_context: Option<LuaInvocationContext>,
) -> Result<RuntimeInvocationResult, String> {
    run_luaskills_blocking_task_async("luaskills-tool-call", move || {
        let engine = lock_runtime_engine(&handle)?;
        engine.call_skill(&tool_name, &args, invocation_context.as_ref())
    })
    .await
}

/// Build one JSON-Schema object definition from the LuaSkills runtime entry parameter descriptors.
/// 根据 LuaSkills 运行时入口参数描述构建一份 JSON-Schema 对象定义。
pub fn luaskills_entry_args_schema(entry: &RuntimeEntryDescriptor) -> Value {
    let mut properties = Map::new();
    let mut required = Vec::new();

    for parameter in &entry.parameters {
        if parameter.required {
            required.push(Value::String(parameter.name.clone()));
        }
        properties.insert(
            parameter.name.clone(),
            json!({
                "type": schema_type_for_luaskills_parameter(&parameter.param_type),
                "description": parameter.description,
            }),
        );
    }

    let mut schema = Map::new();
    schema.insert(String::from("type"), Value::String(String::from("object")));
    schema.insert(String::from("properties"), Value::Object(properties));
    schema.insert(String::from("additionalProperties"), Value::Bool(false));
    if !required.is_empty() {
        schema.insert(String::from("required"), Value::Array(required));
    }

    Value::Object(schema)
}

/// Convert one LuaSkills runtime invocation result into one host-stable JSON object.
/// 将单个 LuaSkills 运行时调用结果转换成宿主稳定的 JSON 对象。
pub fn serialize_luaskills_invocation_result(result: &RuntimeInvocationResult) -> Value {
    json!({
        "content": result.content,
        "overflowMode": result.overflow_mode.map(|mode| match mode {
            luaskills::ToolOverflowMode::Truncate => "truncate",
            luaskills::ToolOverflowMode::Page => "page",
        }),
        "templateHint": result.template_hint,
        "contentBytes": result.content_bytes,
        "contentLines": result.content_lines,
        "hostResult": result.host_result.as_ref().map(serialize_luaskills_host_result),
    })
}

/// Build one default LuaSkills invocation context used by rig-backed tool calls.
/// 构建一份供 rig 驱动工具调用使用的默认 LuaSkills 调用上下文。
pub fn build_luaskills_tool_invocation_context(
    session_id: Option<usize>,
    transport_name: &str,
    tool_name: &str,
) -> LuaInvocationContext {
    let request_context = RuntimeRequestContext {
        request_id: Some(format!("luaskills-tool:{tool_name}")),
        client_name: Some(String::from("vulcan-code-gui")),
        transport_name: Some(transport_name.to_string()),
        session_id: session_id.map(|value| value.to_string()),
        client_info: Some(RuntimeClientInfo {
            kind: Some(String::from("desktop")),
            name: Some(String::from("VulcanCode")),
            version: Some(env!("CARGO_PKG_VERSION").to_string()),
        }),
        client_capabilities: Value::Object(Map::new()),
    };

    LuaInvocationContext::new(
        Some(request_context),
        Value::Object(Map::new()),
        Value::Object(Map::new()),
    )
}

/// Ensures the minimum runtime directory layout exists and returns the mounted skill roots.
/// 确保最小运行时目录布局存在，并返回需要挂载的技能根集合。
fn ensure_runtime_root_layout(runtime_root: &Path) -> Result<Vec<RuntimeSkillRoot>, String> {
    let temp_dir = runtime_root.join("temp");
    let resources_dir = runtime_root.join("resources");
    let lua_packages_dir = runtime_root.join("lua_packages");
    let libs_root = runtime_root.join("libs");
    let tool_root = runtime_root.join("bin").join("tools");
    let state_dir = runtime_root.join("state");
    let download_root = temp_dir.join("downloads");
    let root_skills_dir = runtime_root.join("skills");
    let user_skills_dir = default_user_skill_root();
    let system_lua_lib_dir = runtime_root.join("system_lua_lib");

    for directory in [
        runtime_root,
        temp_dir.as_path(),
        resources_dir.as_path(),
        lua_packages_dir.as_path(),
        libs_root.as_path(),
        tool_root.as_path(),
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
    host_options.allow_network_download = true;
    host_options.dependency_dir_name = String::from("dependencies");
    host_options.state_dir_name = String::from("state");
    host_options.database_dir_name = String::from("databases");
    host_options.skill_config_file_path =
        Some(runtime_root.join("config").join("skill_config.json"));
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

/// Returns the mutable USER skill root stored under the current user's VulcanCode data directory.
/// 返回存放在当前用户 VulcanCode 数据目录下的可变 USER 技能根目录。
fn default_user_skill_root() -> PathBuf {
    crate::data_plane::application_data_root()
        .join("luaskills-user")
        .join("skills")
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

/// Resolves the ordinary USER root used by the desktop panel for managed install actions.
/// 解析桌面面板用于受管安装动作的普通 USER 根。
fn ordinary_user_root(handle: &LuaSkillsRuntimeHandle) -> Result<RuntimeSkillRoot, String> {
    resolve_named_skill_root(&handle.skill_roots, "USER")
}

/// Resolves the effective target root that currently owns one visible skill identifier.
/// 解析当前拥有单个可见技能标识符的生效目标根目录。
fn target_root_for_skill(
    handle: &LuaSkillsRuntimeHandle,
    skill_id: &str,
) -> Result<RuntimeSkillRoot, String> {
    let resolved = collect_effective_skill_instances_from_roots(&handle.skill_roots)
        .map_err(|error| format!("解析技能目录失败：{error}"))?
        .into_iter()
        .find(|instance| instance.skill_id == skill_id)
        .ok_or_else(|| format!("未找到技能 `{skill_id}` 对应的生效目录。"))?;
    resolve_named_skill_root(&handle.skill_roots, &resolved.root_name)
}

/// Resolves the uninstall target root and blocks attempts to remove host-owned ROOT skills.
/// 解析卸载目标根目录，并拦截对宿主自有 ROOT 技能的移除操作。
fn ordinary_uninstall_target_root_for_skill(
    handle: &LuaSkillsRuntimeHandle,
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

/// Map one LuaSkills runtime parameter type string into a JSON-Schema primitive type label.
/// 将单个 LuaSkills 运行时参数类型字符串映射为 JSON-Schema 基元类型标签。
fn schema_type_for_luaskills_parameter(parameter_type: &str) -> &'static str {
    match parameter_type.trim().to_ascii_lowercase().as_str() {
        "int" | "integer" => "integer",
        "number" | "float" | "double" => "number",
        "bool" | "boolean" => "boolean",
        "object" | "json" | "map" => "object",
        "array" | "list" => "array",
        "string" | "path" | "file" | "url" => "string",
        _ => "string",
    }
}

/// Convert one LuaSkills host-result object into one host-stable JSON object.
/// 将单个 LuaSkills 宿主结果对象转换成宿主稳定的 JSON 对象。
fn serialize_luaskills_host_result(result: &RuntimeHostResult) -> Value {
    json!({
        "kind": result.kind,
        "payload": result.payload,
    })
}

/// Resolves one named runtime skill root from the current root chain.
/// 从当前技能根链中解析指定名称的单个运行时技能根。
fn resolve_named_skill_root(
    skill_roots: &[RuntimeSkillRoot],
    root_name: &str,
) -> Result<RuntimeSkillRoot, String> {
    skill_roots
        .iter()
        .find(|root| root.name.eq_ignore_ascii_case(root_name))
        .cloned()
        .ok_or_else(|| format!("当前运行时缺少 {root_name} 技能根。"))
}
