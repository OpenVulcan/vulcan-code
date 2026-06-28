//! LuaSkills runtime data models and management action types.
//! LuaSkills 运行时数据模型与管理动作类型。

use super::*;

/// Long-lived LuaSkills runtime handle kept by the host runtime after startup succeeds.
/// 启动成功后由宿主运行时保留的长生命周期 LuaSkills 运行时句柄。
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
    /// Cached runtime entries currently visible to provider-facing tool preparation.
    /// 当前对 Provider 工具准备链可见的运行时入口缓存。
    pub tool_entries: Arc<Mutex<Vec<RuntimeEntryDescriptor>>>,
    /// Shared FIFO queue holding fine-grained LuaSkills operation progress events for host polling.
    /// 保存细粒度 LuaSkills 操作进度事件并供宿主轮询的共享 FIFO 队列。
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
                "cached_tool_entry_count",
                &self
                    .tool_entries
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .len(),
            )
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
    /// Long-lived runtime handle retained by the host runtime.
    /// 由宿主运行时保留的长生命周期运行时句柄。
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
pub(super) struct RootSkillBootstrapConfig {
    /// Ordered ROOT skill declarations that must exist inside the system-owned ROOT layer.
    /// 必须存在于系统自有 ROOT 层中的有序技能声明集合。
    pub(super) root_skills: Vec<RootSkillBootstrapEntry>,
}

/// One host-managed ROOT skill declaration loaded from the bootstrap config file.
/// 从引导配置文件加载的单条宿主管理 ROOT 技能声明。
#[derive(Debug, Clone, serde::Deserialize)]
pub(super) struct RootSkillBootstrapEntry {
    /// Stable skill identifier expected after installation completes.
    /// 安装完成后预期存在的稳定技能标识符。
    pub(super) skill_id: String,
    /// Source locator consumed by the LuaSkills managed-install pipeline.
    /// 供 LuaSkills 受管安装链消费的来源定位值。
    pub(super) source: String,
    /// Managed-install source type used to interpret the source locator.
    /// 用于解释来源定位值的受管安装来源类型。
    pub(super) source_type: RootSkillBootstrapSourceType,
}

/// Supported source-type values for host-managed ROOT bootstrap skills.
/// 宿主管理 ROOT 引导技能支持的来源类型取值。
#[derive(Debug, Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum RootSkillBootstrapSourceType {
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
    pub(super) fn into_runtime_source_type(self) -> SkillInstallSourceType {
        match self {
            Self::Github => SkillInstallSourceType::Github,
            Self::Url => SkillInstallSourceType::Url,
        }
    }
}
