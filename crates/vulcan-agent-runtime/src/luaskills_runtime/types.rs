//! LuaSkills runtime data models.
//! LuaSkills 运行时数据模型。

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
                luaskills::MAX_RUNTIME_SESSION_LEASES_PER_MANAGER,
            )),
            runlua_ai_tool_enabled: Arc::new(AtomicBool::new(runlua_ai_tool_enabled)),
            runlua_session_ai_tool_enabled: Arc::new(AtomicBool::new(
                runlua_session_ai_tool_enabled,
            )),
            runlua_sessions: Arc::new(Mutex::new(RunLuaSessionRegistry::default())),
        }
    }
}
