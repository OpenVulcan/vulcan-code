//! LuaSkills runtime data models and management action types.
//! LuaSkills 运行时数据模型与管理动作类型。

use super::*;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Semaphore;

/// Compile-time proof that one shared Lua engine may be reached through a read/write lock.
/// 共享 Lua 引擎可通过读写锁并发访问的编译期证明。
///
/// The host holds the engine in an `Arc<RwLock<LuaEngine>>`, so concurrent readers are only sound
/// while the engine stays `Send + Sync`. Asserting it here fails the build instead of silently
/// allowing a later dependency change to reintroduce an engine type that cannot be shared.
/// 宿主以 `Arc<RwLock<LuaEngine>>` 持有引擎，因此只有在引擎保持 `Send + Sync` 时并发读才是安全的。
/// 在此断言可在依赖变化导致引擎不可共享时直接让构建失败，而不是静默放行。
const _: () = {
    /// Asserts that one type may cross threads and be shared behind a lock.
    /// 断言某类型可跨线程并在锁后共享。
    const fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<LuaEngine>();
};

/// One host-owned public Lua runtime lease bound to an exact conversation owner.
/// 一条绑定到精确会话所有者的宿主自有公开 Lua 运行时租约。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RunLuaOwnedSession {
    /// Conversation session that exclusively owns this lease.
    /// 独占当前租约的会话 Session。
    pub(super) owner_session_id: String,
    /// Human-oriented session name supplied by the AI call.
    /// AI 调用提供的面向使用者的会话名称。
    pub(super) name: String,
    /// Stable LuaSkills SID used for identity validation.
    /// 用于身份校验的稳定 LuaSkills SID。
    pub(super) sid: String,
    /// Opaque LuaSkills lease identifier exposed as the public session ID.
    /// 作为公开 Session ID 暴露的不透明 LuaSkills 租约标识。
    pub(super) lease_id: String,
    /// SID-local generation returned by LuaSkills.
    /// LuaSkills 返回的 SID 内部代际编号。
    pub(super) generation: u64,
    /// Finite idle lease lifetime in seconds.
    /// 有限的闲置租约生命周期秒数。
    pub(super) ttl_sec: u64,
}

/// Shared ownership registry for AI-created public Lua runtime leases.
/// AI 创建的公开 Lua 运行时租约共享归属注册表。
#[derive(Debug, Default)]
pub(super) struct RunLuaSessionRegistry {
    /// Active leases keyed by the opaque LuaSkills lease identifier.
    /// 以不透明 LuaSkills 租约标识索引的活跃租约。
    pub(super) leases: BTreeMap<String, RunLuaOwnedSession>,
    /// Owner/name pairs reserved while an upstream lease is being created.
    /// 上游租约创建期间已经预留的所有者/名称组合。
    pub(super) pending_starts: BTreeSet<(String, String)>,
}

/// Long-lived LuaSkills runtime handle kept by the host runtime after startup succeeds.
/// 启动成功后由宿主运行时保留的长生命周期 LuaSkills 运行时句柄。
#[derive(Clone)]
pub struct LuaSkillsRuntimeHandle {
    /// Shared LuaSkills engine reused by later skill-facing panels and workflows.
    /// 供后续技能面板与工作流复用的共享 LuaSkills 引擎。
    ///
    /// Ordinary execution takes a read guard so independent calls overlap, while the upstream
    /// methods requiring `&mut LuaEngine` take the write guard and form the engine change barrier.
    /// 普通执行取读守卫以使相互独立的调用可以重叠；要求 `&mut LuaEngine` 的上游方法取写守卫，
    /// 并构成引擎变更屏障。
    pub engine: Arc<RwLock<LuaEngine>>,
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
    /// Host admission matching the configured ordinary Skill VM-pool capacity.
    /// 与普通 Skill VM 池配置容量一致的宿主入场器。
    pub(super) skill_call_admission: Arc<Semaphore>,
    /// Host admission bounding RunLua operation workers with the configured isolated-pool capacity.
    /// 以隔离 RunLua 池配置容量约束 RunLua 操作工作线程的宿主入场器。
    pub(super) runlua_call_admission: Arc<Semaphore>,
    /// Host admission matching LuaSkills' independent persistent-session lease capacity.
    /// 与 LuaSkills 独立持久会话租约容量一致的宿主入场器。
    pub(super) runlua_session_call_admission: Arc<Semaphore>,
    /// Live host-only switch controlling AI visibility of isolated RunLua execution.
    /// 控制隔离 RunLua 执行 AI 可见性的实时宿主开关。
    pub(super) runlua_ai_tool_enabled: Arc<AtomicBool>,
    /// Live host-only switch controlling AI visibility of persistent RunLua sessions.
    /// 控制持久 RunLua 会话 AI 可见性的实时宿主开关。
    pub(super) runlua_session_ai_tool_enabled: Arc<AtomicBool>,
    /// Conversation ownership registry for public persistent RunLua leases.
    /// 公开持久 RunLua 租约的会话归属注册表。
    pub(super) runlua_sessions: Arc<Mutex<RunLuaSessionRegistry>>,
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
                "runlua_ai_tool_enabled",
                &self.runlua_ai_tool_enabled.load(Ordering::Acquire),
            )
            .field(
                "runlua_session_ai_tool_enabled",
                &self.runlua_session_ai_tool_enabled.load(Ordering::Acquire),
            )
            .field(
                "runlua_session_count",
                &self
                    .runlua_sessions
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .leases
                    .len(),
            )
            .finish()
    }
}

impl LuaSkillsRuntimeHandle {
    /// Creates an agent-facing handle from one already initialized shared LuaSkills engine.
    /// 根据一个已初始化的共享 LuaSkills 引擎创建面向 Agent 的句柄。
    ///
    /// `engine` and the runtime/catalog arguments retain the exact production engine projection;
    /// `skill_call_capacity` mirrors the ordinary pool and `runlua_call_capacity` mirrors only the
    /// isolated one-shot pool. Persistent sessions use the host's release-compatible mirror of the
    /// engine-wide fixed lease limit.
    /// `engine` 与运行时/目录参数保留精确生产引擎投影；`skill_call_capacity` 镜像普通池，
    /// `runlua_call_capacity` 仅镜像隔离的一次性池；持久会话使用宿主对引擎级固定租约上限的发布版兼容镜像。
    ///
    /// Returns one cloneable handle whose async callers queue before creating blocking workers.
    /// 返回一个可克隆句柄，使异步调用方在创建阻塞工作线程之前排队。
    pub fn from_shared_engine(
        engine: Arc<RwLock<LuaEngine>>,
        runtime_root: PathBuf,
        skill_roots: Vec<RuntimeSkillRoot>,
        entry_count: usize,
        tool_entries: Vec<RuntimeEntryDescriptor>,
        runlua_ai_tool_enabled: bool,
        runlua_session_ai_tool_enabled: bool,
        skill_call_capacity: usize,
        runlua_call_capacity: usize,
    ) -> Self {
        // system_lua_lib_dir is derived from the single verified runtime root contract.
        // system_lua_lib_dir 根据唯一且已验证的运行时根目录契约派生。
        let system_lua_lib_dir = runtime_root.join("system_lua_lib");
        Self {
            engine,
            runtime_root,
            system_lua_lib_dir,
            skill_roots,
            entry_count,
            tool_entries: Arc::new(Mutex::new(tool_entries)),
            skill_call_admission: Arc::new(Semaphore::new(skill_call_capacity.max(1))),
            runlua_call_admission: Arc::new(Semaphore::new(runlua_call_capacity.max(1))),
            runlua_session_call_admission: Arc::new(Semaphore::new(
                LUASKILLS_RUNTIME_SESSION_LEASE_CAPACITY,
            )),
            runlua_ai_tool_enabled: Arc::new(AtomicBool::new(runlua_ai_tool_enabled)),
            runlua_session_ai_tool_enabled: Arc::new(AtomicBool::new(
                runlua_session_ai_tool_enabled,
            )),
            runlua_sessions: Arc::new(Mutex::new(RunLuaSessionRegistry::default())),
        }
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
    ///
    /// `Url` is refused rather than converted: upstream rejects
    /// `SkillInstallSourceType::Url` unconditionally by source policy, so a choice whose
    /// own label advertises a package URL would fail on every attempt. This type has no
    /// caller today, which is exactly why the refusal lives here rather than in a caller
    /// that could be written later without noticing.
    /// `Url` 被拒绝而不是被转换：上游按来源策略无条件拒绝 `SkillInstallSourceType::Url`，
    /// 因此一个自身标签就在宣传包地址的选项会在每次尝试时失败。该类型当前没有任何调用方，
    /// 这正是把拒绝放在这里、而不是留给日后可能在不察觉的情况下写出的调用方的原因。
    pub fn into_runtime_source_type(self) -> Result<SkillInstallSourceType, String> {
        match self {
            Self::Github => Ok(SkillInstallSourceType::Github),
            Self::Url => Err(String::from(
                "来源类型 `url` 已被上游来源策略停用，无法使用；请将安装来源改为 GitHub。",
            )),
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

impl LuaSkillsManagementAction {
    /// Return one stable English diagnostic label for write-lock contention reporting.
    /// 返回一个用于写锁竞争上报的稳定英文诊断标签。
    ///
    /// The label never contains a skill id or a source locator, so slow write-lock diagnostics stay
    /// free of user identifiers while remaining attributable to one management action.
    /// 该标签不包含技能标识或来源定位值，因此写锁慢诊断既能归因到单次管理动作，又不携带用户标识。
    pub(super) fn diagnostic_label(&self) -> &'static str {
        match self {
            Self::Refresh => "refresh",
            Self::Install { .. } => "install",
            Self::Update { .. } => "update",
            Self::Enable { .. } => "enable",
            Self::Disable { .. } => "disable",
            Self::Uninstall { .. } => "uninstall",
        }
    }
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
    ///
    /// `Url` is refused rather than converted: upstream rejects
    /// `SkillInstallSourceType::Url` unconditionally by source policy, so a ROOT bootstrap
    /// declaring it would fail on every start with an upstream policy string. Refusing it
    /// here reports the reason once, in the host's own words.
    /// `Url` 被拒绝而不是被转换：上游按来源策略无条件拒绝 `SkillInstallSourceType::Url`，
    /// 因此声明它的 ROOT 引导会在每次启动时以上游策略文案失败。在此拒绝它可以用宿主自己的措辞
    /// 一次性说明原因。
    pub(super) fn into_runtime_source_type(self) -> Result<SkillInstallSourceType, String> {
        match self {
            Self::Github => Ok(SkillInstallSourceType::Github),
            Self::Url => Err(String::from(
                "来源类型 `url` 已被上游来源策略停用，无法使用；请将该技能的来源类型改为 `github`。",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LuaSkillsInstallSourceChoice;
    use luaskills::SkillInstallSourceType;

    /// Verifies every host-side source choice refuses the disabled `url` source type.
    /// 验证每个宿主侧来源选项都拒绝已停用的 `url` 来源类型。
    ///
    /// Upstream rejects `SkillInstallSourceType::Url` unconditionally by source policy, so
    /// a choice whose own label advertises a package URL would fail on every attempt. The
    /// refusal therefore lives on the type rather than on one of its callers, which is what
    /// keeps a future caller from inheriting the dead path unnoticed.
    /// 上游按来源策略无条件拒绝 `SkillInstallSourceType::Url`，因此一个自身标签就在宣传包地址
    /// 的选项会在每次尝试时失败。拒绝因此落在类型上而不是它的某个调用方上，这正是使未来新增
    /// 调用方不会在无人察觉的情况下继承这条死路的原因。
    #[test]
    fn install_source_choice_refuses_the_disabled_url_kind() {
        assert!(matches!(
            LuaSkillsInstallSourceChoice::Github.into_runtime_source_type(),
            Ok(SkillInstallSourceType::Github)
        ));
        let error = LuaSkillsInstallSourceChoice::Url
            .into_runtime_source_type()
            .expect_err("upstream rejects the url source type unconditionally");
        // The message names both the rejected kind and its replacement.
        // 消息同时点名被拒绝的类型与其替代者。
        assert!(error.contains("url"), "{error}");
        assert!(error.contains("GitHub"), "{error}");
    }

    /// Verifies the user-facing label still describes both kinds truthfully.
    /// 验证用户可见标签仍如实描述两种类型。
    ///
    /// The label is independent of the refusal: the choice remains displayable so an
    /// existing value can be read back, while converting it to a request is what fails.
    /// 标签与拒绝相互独立：该选项仍可显示，以便已有的值能被读出，失败的是把它转成请求。
    #[test]
    fn install_source_choice_labels_stay_truthful() {
        assert_eq!(LuaSkillsInstallSourceChoice::Github.label(), "GitHub");
        assert_eq!(LuaSkillsInstallSourceChoice::Url.label(), "包地址");
    }
}
