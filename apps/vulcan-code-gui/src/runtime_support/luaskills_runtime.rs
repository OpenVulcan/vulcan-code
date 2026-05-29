//! LuaSkills runtime bootstrap helpers layered on top of the shared controller client.
//! 构建在共享控制器客户端之上的 LuaSkills 运行时引导辅助模块。

use crate::model::SessionId;
use crate::project_temp_maintenance;
use crate::provider_config::{PersistedControllerConfiguration, PersistedControllerProcessMode};
use crate::runtime_assets;
use crate::tool_result_format::{
    EffectiveBudgetScope, HostRenderOptions, HostTextRenderInput, HostToolOverflowMode,
    ToolResultBudgetSnapshot, render_tool_result_text,
};
use crate::tooling::change_set::{
    change_set_host_verification_notice, luaskills_change_set_host_result_capabilities,
};
use iced::futures::channel::oneshot;
use luaskills::skill::manager::collect_effective_skill_instances_from_roots;
use luaskills::{
    LuaEngine, LuaEngineOptions, LuaInvocationContext, LuaRuntimeDatabaseProviderMode,
    LuaRuntimeHostOptions, LuaRuntimeSpaceControllerProcessMode, LuaVmPoolConfig,
    RuntimeClientInfo, RuntimeEntryDescriptor, RuntimeInvocationResult, RuntimeRequestContext,
    RuntimeSkillOperationProgressEvent, RuntimeSkillRoot, SkillInstallRequest,
    SkillInstallSourceType, SkillManagementAuthority, SkillManager, SkillManagerConfig, SkillMeta,
    SkillUninstallOptions, set_skill_operation_progress_callback,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Relative configuration file path that declares the host-managed ROOT skill bootstrap set.
/// 声明宿主管理 ROOT 技能引导集合的相对配置文件路径。
const ROOT_SKILL_BOOTSTRAP_CONFIG_RELATIVE_PATH: &str = "config/luaskills_root_skills.yaml";

/// Maximum queued LuaSkills progress event count retained for GUI polling snapshots.
/// 为图形界面轮询快照保留的 LuaSkills 进度事件最大排队数量。
const LUASKILLS_PROGRESS_EVENT_QUEUE_LIMIT: usize = 256;

/// Long-lived LuaSkills runtime handle kept by the GUI after startup succeeds.
/// 启动成功后由图形界面保留的长生命周期 LuaSkills 运行时句柄。
#[derive(Clone)]
pub struct LuaSkillsRuntimeHandle {
    /// Shared LuaSkills engine reused by later skill-facing panels and workflows.
    /// 供后续技能面板与工作流复用的共享 LuaSkills 引擎。
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
    /// Shared FIFO queue holding fine-grained LuaSkills operation progress events for GUI polling.
    /// 保存细粒度 LuaSkills 操作进度事件并供图形界面轮询的共享 FIFO 队列。
    pub operation_progress_events: Arc<Mutex<VecDeque<RuntimeSkillOperationProgressEvent>>>,
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
            .field(
                "operation_progress_event_count",
                &self
                    .operation_progress_events
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .len(),
            )
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

/// One visible LuaSkills runtime tool row grouped under one enabled skill package in the desktop panel.
/// 桌面面板中归属单个已启用技能包的一条可见 LuaSkills 运行时工具行。
#[derive(Debug, Clone)]
pub struct LuaSkillsCatalogTool {
    /// Stable canonical tool identifier resolved by the LuaSkills runtime.
    /// 由 LuaSkills 运行时解析出的稳定规范工具标识。
    pub canonical_name: String,
    /// Stable local tool name declared inside the owning LuaSkills package.
    /// 在所属 LuaSkills 技能包内部声明的稳定本地工具名称。
    pub local_name: String,
    /// Human-readable tool description declared by the runtime entry descriptor.
    /// 由运行时入口描述符声明的人类可读工具描述。
    pub description: String,
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
    /// Runtime tool rows currently exposed by the enabled effective skill instance.
    /// 当前由已启用生效技能实例暴露出的运行时工具行集合。
    pub tools: Vec<LuaSkillsCatalogTool>,
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
    let operation_progress_events = Arc::new(Mutex::new(VecDeque::new()));
    install_luaskills_operation_progress_callback(operation_progress_events.clone());

    Ok(LuaSkillsBootstrapReport {
        handle: LuaSkillsRuntimeHandle {
            engine: Arc::new(Mutex::new(engine)),
            runtime_root,
            system_lua_lib_dir,
            skill_roots,
            entry_count,
            operation_progress_events,
        },
    })
}

/// Installs one process-wide LuaSkills progress callback that forwards events into the shared GUI queue.
/// 安装一个进程级 LuaSkills 进度回调，并将事件转发到共享图形界面队列。
fn install_luaskills_operation_progress_callback(
    operation_progress_events: Arc<Mutex<VecDeque<RuntimeSkillOperationProgressEvent>>>,
) {
    set_skill_operation_progress_callback(Some(Arc::new(move |event| {
        let mut guard = operation_progress_events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.push_back(event.clone());
        while guard.len() > LUASKILLS_PROGRESS_EVENT_QUEUE_LIMIT {
            guard.pop_front();
        }
    })));
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

/// Drains every queued LuaSkills operation progress event captured for one runtime handle.
/// 取出某个运行时句柄当前捕获的全部已排队 LuaSkills 操作进度事件。
pub fn drain_luaskills_operation_progress_events(
    handle: &LuaSkillsRuntimeHandle,
) -> Vec<RuntimeSkillOperationProgressEvent> {
    let mut guard = handle
        .operation_progress_events
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.drain(..).collect()
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

    // Force a post-action reload because LuaSkills update/install can finalize filesystem state after its internal pre-commit reload.
    // 强制执行操作后重载，因为 LuaSkills 更新/安装可能会在其内部提交前重载之后才完成文件系统状态落定。
    engine
        .reload_from_roots(&handle.skill_roots)
        .map_err(|error| format!("同步 LuaSkills 管理操作后的运行时入口失败：{error}"))?;

    let catalog = build_luaskills_catalog(handle, &engine)?;
    Ok(LuaSkillsManagementReport { message, catalog })
}

/// List every LuaSkills runtime entry currently visible to the host tool surface.
/// 列出当前对宿主工具表面可见的全部 LuaSkills 运行时入口。
pub fn list_luaskills_tool_entries(
    handle: &LuaSkillsRuntimeHandle,
) -> Result<Vec<RuntimeEntryDescriptor>, String> {
    let engine = lock_runtime_engine(handle)?;
    Ok(list_host_visible_luaskills_entries(&engine))
}

/// One compact LuaSkills help summary matched to a runtime entry.
/// 匹配到单个运行时入口的一份紧凑 LuaSkills 帮助摘要。
#[derive(Debug, Clone)]
pub struct LuaSkillsEntryHelpSummary {
    /// Resolved help flow name used for the entry.
    /// 当前入口使用的已解析帮助流程名称。
    pub flow_name: String,
    /// Human-readable help node description.
    /// 帮助节点的人类可读说明。
    pub description: String,
    /// Rendered help body returned by LuaSkills.
    /// LuaSkills 返回的已渲染帮助正文。
    pub content: String,
}

/// Render the best matching LuaSkills help text for one runtime entry.
/// 为单个运行时入口渲染最匹配的 LuaSkills 帮助文本。
pub fn render_luaskills_entry_help_summary(
    handle: &LuaSkillsRuntimeHandle,
    entry: &RuntimeEntryDescriptor,
) -> Result<Option<LuaSkillsEntryHelpSummary>, String> {
    let engine = lock_runtime_engine(handle)?;
    let help_descriptors =
        engine.list_skill_help_for_authority(SkillManagementAuthority::DelegatedTool);
    let flow_name = help_descriptors
        .iter()
        .find(|help| help.skill_id == entry.skill_id)
        .and_then(|help| {
            std::iter::once(&help.main)
                .chain(help.flows.iter())
                .find(|node| {
                    node.related_entries
                        .iter()
                        .any(|related| related == &entry.canonical_name)
                })
                .map(|node| node.flow_name.clone())
        });
    let Some(flow_name) = flow_name else {
        return Ok(None);
    };
    let Some(detail) = engine
        .render_skill_help_detail_for_authority(
            SkillManagementAuthority::DelegatedTool,
            &entry.skill_id,
            &flow_name,
            None,
        )
        .map_err(|error| {
            format!(
                "渲染 LuaSkills `{}` 的 help `{}` 失败：{}",
                entry.canonical_name, flow_name, error
            )
        })?
    else {
        return Ok(None);
    };
    Ok(Some(LuaSkillsEntryHelpSummary {
        flow_name: detail.flow_name,
        description: detail.description,
        content: detail.content,
    }))
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
pub fn luaskills_entry_args_schema(
    entry: &RuntimeEntryDescriptor,
    hide_host_managed_sid: bool,
    hide_host_managed_pwd: bool,
) -> Value {
    let mut schema = entry.input_schema.clone();
    if hide_host_managed_sid {
        hide_reserved_luaskills_property_from_visible_schema(
            &mut schema,
            LUASKILL_SID_PARAMETER_NAME,
        );
    }
    if hide_host_managed_pwd {
        hide_reserved_luaskills_property_from_visible_schema(&mut schema, PWD_PARAMETER_NAME);
    }
    schema
}

/// Stable LuaSkills reserved managed-identity parameter name used by host projections.
/// 宿主投影使用的稳定 LuaSkills 保留托管身份参数名称。
pub const LUASKILL_SID_PARAMETER_NAME: &str = "LUASKILL_SID";

/// Stable LuaSkills reserved project-path parameter name used by host projections.
/// 宿主投影使用的稳定 LuaSkills 保留项目路径参数名称。
pub const PWD_PARAMETER_NAME: &str = "PWD";

/// Stable host-managed prefix reserved by LuaSkills for injected managed identities.
/// LuaSkills 为注入托管身份保留的稳定宿主管理前缀。
pub const LUASKILLS_HOST_MANAGED_SID_PREFIX: &str = "LUASKILLS-SID-";

/// Hide one reserved host-managed property from one visible LuaSkills input schema clone.
/// 从单份可见 LuaSkills 输入 schema 副本中隐藏一个保留宿主管理属性。
fn hide_reserved_luaskills_property_from_visible_schema(schema: &mut Value, field_name: &str) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    let Some(properties_value) = object.get_mut("properties") else {
        return;
    };
    let Some(properties) = properties_value.as_object_mut() else {
        return;
    };
    if properties.remove(field_name).is_none() {
        return;
    }
    remove_required_name_from_schema(schema, field_name);
}

/// Remove one top-level required-field name from one visible LuaSkills input schema clone.
/// 从单份可见 LuaSkills 输入 schema 副本中移除一个顶层必填字段名。
fn remove_required_name_from_schema(schema: &mut Value, field_name: &str) {
    let Some(object) = schema.as_object_mut() else {
        return;
    };
    let Some(required_value) = object.get_mut("required") else {
        return;
    };
    let Some(required) = required_value.as_array_mut() else {
        return;
    };
    required.retain(|value| value.as_str() != Some(field_name));
    if required.is_empty() {
        object.remove("required");
    }
}

/// Render one LuaSkills invocation result into the final host-managed text returned to the model.
/// 将单次 LuaSkills 调用结果渲染为返回给模型的最终宿主管理文本。
pub fn render_luaskills_invocation_result_text(
    handle: &LuaSkillsRuntimeHandle,
    tool_name: &str,
    skill_name: Option<&str>,
    result: &RuntimeInvocationResult,
    workspace_root: Option<&Path>,
) -> String {
    let client_budget = build_default_luaskills_budget_snapshot(tool_name);
    let spill_root = workspace_root
        .filter(|path| !path.as_os_str().is_empty())
        .and_then(|path| project_temp_maintenance::ensure_project_temp_dir(path).ok());
    let render_options = HostRenderOptions {
        spill_root,
        template_skill_roots: handle
            .skill_roots
            .iter()
            .map(|root| root.skills_dir.clone())
            .collect(),
        template_resources_root: Some(handle.runtime_root.join("resources")),
    };

    render_tool_result_text(
        &host_text_render_input_from_runtime_result(result),
        skill_name,
        &client_budget,
        &render_options,
    )
}

/// Convert one LuaSkills runtime result into the host-owned generic text render input.
/// 将单条 LuaSkills 运行时结果转换为宿主自有的通用文本渲染输入。
fn host_text_render_input_from_runtime_result(
    result: &RuntimeInvocationResult,
) -> HostTextRenderInput {
    let mut content = result.content.clone();
    if let Some(host_result) = result.host_result.as_ref()
        && let Some(verification_notice) = (host_result.kind == "change_set")
            .then(|| change_set_host_verification_notice(&host_result.payload))
            .flatten()
    {
        if !content.trim().is_empty() {
            content.push_str("\n\n");
        }
        content.push_str(&verification_notice);
    }
    let normalized = normalize_host_render_text(&content);
    HostTextRenderInput {
        content,
        overflow_mode: result.overflow_mode.map(|mode| match mode {
            luaskills::ToolOverflowMode::Truncate => HostToolOverflowMode::Truncate,
            luaskills::ToolOverflowMode::Page => HostToolOverflowMode::Page,
        }),
        template_hint: result.template_hint.clone(),
        content_bytes: normalized.len(),
        content_lines: if normalized.is_empty() {
            0
        } else {
            normalized.split('\n').count()
        },
    }
}

/// Build one default LuaSkills invocation context used by host-managed tool calls.
/// 构建一份供宿主管理工具调用使用的默认 LuaSkills 调用上下文。
pub fn build_luaskills_tool_invocation_context(
    session_id: Option<SessionId>,
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
        client_capabilities: luaskills_change_set_host_result_capabilities(),
    };
    let client_budget = build_default_luaskills_client_budget(tool_name);

    LuaInvocationContext::new(
        Some(request_context),
        client_budget,
        Value::Object(Map::new()),
    )
}

/// Normalizes host-render text so byte and line metrics stay deterministic after host-side warning injection.
/// 对宿主渲染文本进行规范化，使宿主侧注入警告后的字节与行数统计保持确定性。
fn normalize_host_render_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Builds one default client-budget snapshot for host-driven LuaSkills tool calls with unlimited lines and a 200KB byte ceiling.
/// 为宿主驱动的 LuaSkills 工具调用构建一份默认客户端预算快照，采用无限行与 200KB 字节上限。
fn build_default_luaskills_client_budget(tool_name: &str) -> Value {
    let budget = build_default_luaskills_budget_snapshot(tool_name);
    json!({
        "client_name": "vulcan-code-gui",
        "tool_name": tool_name,
        "matched_client_pattern": "vulcan-code-gui",
        "tool_result": {
            "bytes": budget.tool_result.bytes,
            "lines": budget.tool_result.lines
        },
        "file_read": {
            "bytes": budget.file_read.bytes,
            "lines": budget.file_read.lines
        },
        "tool_config": {}
    })
}

/// Builds one strongly typed default budget snapshot reused by host-side result rendering and Lua invocation context injection.
/// 构建一份强类型默认预算快照，供宿主侧结果渲染与 Lua 调用上下文注入复用。
fn build_default_luaskills_budget_snapshot(_tool_name: &str) -> ToolResultBudgetSnapshot {
    ToolResultBudgetSnapshot {
        tool_result: EffectiveBudgetScope {
            bytes: 200_000,
            lines: -1,
        },
        file_read: EffectiveBudgetScope {
            bytes: 200_000,
            lines: -1,
        },
    }
}

/// Ensures the minimum runtime directory layout exists and returns the mounted skill roots.
/// 确保最小运行时目录布局存在，并返回需要挂载的技能根集合。
fn ensure_runtime_root_layout(runtime_root: &Path) -> Result<Vec<RuntimeSkillRoot>, String> {
    let temp_dir = runtime_root.join("temp");
    let resources_dir = runtime_root.join("resources");
    let lua_packages_dir = runtime_root.join("lua_packages");
    let libs_root = runtime_root.join("libs");
    // BinRoot follows LuaSkills 0.4.6 fixed host-tool layout directly under runtime_root/bin.
    // BinRoot 遵循 LuaSkills 0.4.6 固定宿主工具布局，直接位于 runtime_root/bin。
    let bin_root = runtime_root.join("bin");
    // DependenciesDir stores runtime-managed dependency payloads beside the fixed skills root.
    // DependenciesDir 用于在固定 skills 根旁保存运行时托管的依赖载荷。
    let dependencies_dir = runtime_root.join("dependencies");
    // DatabasesDir stores file-backed databases when a runtime path backend is selected.
    // DatabasesDir 用于在选择路径型后端时保存文件型数据库。
    let databases_dir = runtime_root.join("databases");
    // ConfigDir owns the default unified skill_config.json resolved from runtime_root.
    // ConfigDir 拥有从 runtime_root 推导出的默认统一 skill_config.json。
    let config_dir = runtime_root.join("config");
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

/// Builds one host option object that forces LuaSkills to route both databases through the controller.
/// 构建一份宿主选项对象，强制 LuaSkills 通过控制器路由 SQLite 与 LanceDB。
fn build_host_options(
    runtime_root: &Path,
    controller: PersistedControllerConfiguration,
) -> LuaRuntimeHostOptions {
    let mut host_options = LuaRuntimeHostOptions::with_runtime_root(runtime_root.to_path_buf());
    host_options.allow_network_download = true;
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
    let visible_entries = list_host_visible_luaskills_entries(engine);
    let mut tools_by_skill = visible_entries.into_iter().fold(
        BTreeMap::<String, Vec<LuaSkillsCatalogTool>>::new(),
        |mut grouped_tools, entry| {
            grouped_tools
                .entry(entry.skill_id.clone())
                .or_default()
                .push(LuaSkillsCatalogTool {
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
                Some(record.source.source_type),
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
            entry_count: tools.len(),
            tools,
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
        entry_count: skills.iter().map(|skill| skill.entry_count).sum(),
        skills,
    })
}

/// Lists the LuaSkills runtime entries currently visible to the host-controlled System authority.
/// 列出当前对宿主控制 System authority 可见的 LuaSkills 运行时入口。
fn list_host_visible_luaskills_entries(engine: &LuaEngine) -> Vec<RuntimeEntryDescriptor> {
    engine.list_entries_for_authority(SkillManagementAuthority::System)
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
        official_skill_hub_base_url: None,
        enable_private_url_skill_install: false,
        private_skill_source_allowlist: Vec::new(),
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

#[cfg(test)]
mod tests {
    use super::luaskills_entry_args_schema;
    use super::{LUASKILL_SID_PARAMETER_NAME, PWD_PARAMETER_NAME};
    use luaskills::{RuntimeEntryDescriptor, RuntimeEntryParameterDescriptor};
    use serde_json::json;

    #[test]
    fn luaskills_schema_returns_resolved_input_schema_verbatim() {
        let entry = build_entry(
            "vulcan-codekit",
            "node-source",
            vec![RuntimeEntryParameterDescriptor {
                name: String::from("nodes"),
                param_type: String::from("array"),
                description: String::from("Required node input."),
                required: true,
            }],
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "nodes": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "file": { "type": "string" },
                                "structural_path": { "type": "string" }
                            },
                            "required": ["file", "structural_path"],
                            "additionalProperties": false
                        }
                    }
                },
                "required": ["nodes"]
            }),
        );

        let schema = luaskills_entry_args_schema(&entry, false, false);

        assert_eq!(schema, entry.input_schema);
    }

    #[test]
    fn luaskills_schema_hides_managed_sid_when_requested() {
        let entry = build_entry(
            "vulcan-stateful",
            "run",
            vec![
                RuntimeEntryParameterDescriptor {
                    name: String::from(LUASKILL_SID_PARAMETER_NAME),
                    param_type: String::from("string"),
                    description: String::from("managed sid"),
                    required: false,
                },
                RuntimeEntryParameterDescriptor {
                    name: String::from("args"),
                    param_type: String::from("array"),
                    description: String::from("argument array"),
                    required: false,
                },
            ],
            json!({
                "type": "object",
                "properties": {
                    "LUASKILL_SID": { "type": "string" },
                    "args": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                },
                "required": ["LUASKILL_SID"]
            }),
        );

        let schema = luaskills_entry_args_schema(&entry, true, false);

        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {
                    "args": {
                        "type": "array",
                        "items": { "type": "string" }
                    }
                }
            })
        );
    }

    #[test]
    fn luaskills_schema_hides_managed_pwd_when_requested() {
        let entry = build_entry(
            "vulcan-codekit",
            "run",
            vec![
                RuntimeEntryParameterDescriptor {
                    name: String::from(PWD_PARAMETER_NAME),
                    param_type: String::from("string"),
                    description: String::from("workspace root"),
                    required: false,
                },
                RuntimeEntryParameterDescriptor {
                    name: String::from("query"),
                    param_type: String::from("string"),
                    description: String::from("search query"),
                    required: false,
                },
            ],
            json!({
                "type": "object",
                "properties": {
                    "PWD": { "type": "string" },
                    "query": { "type": "string" }
                },
                "required": ["PWD", "query"]
            }),
        );

        let schema = luaskills_entry_args_schema(&entry, false, true);

        assert_eq!(
            schema,
            json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            })
        );
    }

    fn build_entry(
        skill_id: &str,
        local_name: &str,
        parameters: Vec<RuntimeEntryParameterDescriptor>,
        input_schema: serde_json::Value,
    ) -> RuntimeEntryDescriptor {
        RuntimeEntryDescriptor {
            canonical_name: format!("{skill_id}-{local_name}"),
            skill_id: skill_id.to_string(),
            local_name: local_name.to_string(),
            root_name: String::from("ROOT"),
            skill_dir: String::from("skill-dir"),
            description: String::from("entry"),
            parameters,
            input_schema,
        }
    }
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
