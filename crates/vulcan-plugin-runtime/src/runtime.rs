//! Long-lived generic Vulcan Plugin session runtime backed by LuaSkills leases.
//! 由 LuaSkills 租约承载的长生命周期通用 Vulcan 插件会话运行时。

mod action_status;
mod actions;
mod authorization;
mod bridge;
mod configuration;
mod drain;
mod environment;
mod execution;
mod installation;
mod lease;
mod lifecycle;
mod local_files;
mod persistent;
mod process;
mod sessions;

// Preserve the existing public plugin API at its original module path.
// 在原模块路径保留既有公开插件API。
pub use action_status::{VulcanPluginActionIdentity, VulcanPluginActionStatus};
pub use actions::VulcanPluginActionRequest;
pub use authorization::{
    VulcanPluginInstallSecretRequirement, VulcanPluginMcpPermissionPreview,
    VulcanPluginPermissionPreview, plugin_permission_key,
};

use bridge::{VulcanPluginBridgeLeaseScope, canonical_bridge_workspace_root, required_json_string};
pub use drain::VulcanPluginDrainReport;
pub use installation::{VulcanPluginInstallationResult, VulcanPluginUpdateTransaction};
use lease::{
    call_engine_blocking, close_raw_system_lease_sync, close_system_lease_sync,
    parse_engine_response,
};
pub use lifecycle::VulcanPluginLifecycleEvent;
use local_files::{
    canonical_plugin_file, copy_plugin_directory_tree, read_plugin_json_state,
    write_plugin_json_record,
};
pub use process::VulcanPluginMcpStdioLaunchSpec;
pub use sessions::VulcanPluginInvocationRequest;
use sessions::plugin_payload;

use crate::{
    VulcanPluginCatalogSnapshot, VulcanPluginHostServices, VulcanPluginManifest,
    VulcanPluginRuntimeState,
};
use luaskills::LuaEngine;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

/// Maximum Lua execution time shared by bundle loading and lifecycle probing.
/// bundle加载与生命周期探测共用的Lua最大执行时长。
const VULCAN_PLUGIN_PROBE_TIMEOUT_MS: u64 = 5_000;

/// Default Lua execution time used when one hook declaration omits a timeout.
/// hook 声明省略超时时使用的 Lua 默认执行时长。
const VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS: u64 = 60_000;

/// Structured generic plugin runtime failure.
/// 结构化的通用插件运行时失败。
#[derive(Debug, Clone)]
pub struct VulcanPluginRuntimeError {
    /// Human-readable English diagnostic returned to the model tool layer.
    /// 返回给模型工具层的人类可读英文诊断。
    pub message: String,
}

impl VulcanPluginRuntimeError {
    /// Builds one runtime error from an exact diagnostic.
    /// 根据精确诊断构建一个运行时错误。
    ///
    /// # Parameters
    /// 参数
    /// `message` is the complete diagnostic retained by the host boundary.
    /// `message` 是宿主边界保留的完整诊断。
    ///
    /// # Returns
    /// 返回值
    /// Returns a new Vulcan Plugin runtime error.
    /// 返回新的 Vulcan 插件运行时错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for VulcanPluginRuntimeError {
    /// Renders the runtime error as its exact diagnostic text.
    /// 将运行时错误渲染为其精确诊断文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for VulcanPluginRuntimeError {}

/// One owner-bound managed plugin session.
/// 单个绑定所有者的受管插件会话。
struct ManagedVulcanPluginSession {
    /// Public host-owned session identifier.
    /// 宿主自有的公开会话标识。
    session_id: String,
    /// Stable owning plugin identifier.
    /// 稳定的所属插件标识。
    plugin_id: String,
    /// Host plugin generation captured when the session opened.
    /// 会话打开时捕获的宿主插件代次。
    plugin_generation: u64,
    /// Stable controller session owner.
    /// 稳定的控制器会话所有者。
    owner_session_id: String,
    /// LuaSkills managed lease identifier.
    /// LuaSkills 受管租约标识。
    lease_id: String,
    /// Stable LuaSkills session identifier returned with the managed lease.
    /// 与受管租约一同返回的稳定 LuaSkills 会话标识。
    sid: String,
    /// LuaSkills managed lease generation.
    /// LuaSkills 受管租约代次。
    generation: u64,
    /// Manifest revision captured when the session opened.
    /// 会话打开时捕获的插件清单。
    manifest: Arc<VulcanPluginManifest>,
    /// Serial operation gate required by ordered stateful protocols.
    /// 有序有状态协议要求的串行操作门。
    operation_gate: AsyncMutex<()>,
    /// Whole-session close gate that makes concurrent lifecycle drains wait for one owner.
    /// 使并发生命周期排空等待唯一关闭所有者的完整会话关闭门。
    close_gate: AsyncMutex<()>,
    /// Closing state used to reject late operations.
    /// 用于拒绝迟到操作的关闭状态。
    closing: AtomicBool,
    /// Last successful or attempted activity timestamp in Unix milliseconds.
    /// 最近一次成功或尝试活动的 Unix 毫秒时间戳。
    last_activity_unix_ms: AtomicU64,
}

/// One persistent plugin-level Lua lease dedicated to one handler category and plugin generation.
/// 专用于一个处理器类别和插件代次的持久插件级 Lua 租约。
struct ManagedVulcanPluginHookRuntime {
    /// Stable owning plugin identifier.
    /// 稳定的所属插件标识。
    plugin_id: String,
    /// Host plugin generation captured when the hook runtime opened.
    /// hook 运行时打开时捕获的宿主插件代次。
    plugin_generation: u64,
    /// LuaSkills managed lease identifier.
    /// LuaSkills 受管租约标识。
    lease_id: String,
    /// Stable LuaSkills session identifier returned with the managed lease.
    /// 与受管租约一同返回的稳定 LuaSkills 会话标识。
    sid: String,
    /// LuaSkills managed lease generation.
    /// LuaSkills 受管租约代次。
    generation: u64,
    /// Manifest revision captured when the hook runtime opened.
    /// hook 运行时打开时捕获的插件清单。
    manifest: Arc<VulcanPluginManifest>,
    /// Serial gate that preserves deterministic plugin-local hook state.
    /// 保持插件本地 hook 状态确定性的串行门。
    operation_gate: AsyncMutex<()>,
    /// Closing state used to reject late invocations.
    /// 用于拒绝迟到调用的关闭状态。
    closing: AtomicBool,
}

/// Persistent runtime category used to isolate latency-sensitive hooks from plugin UI actions.
/// 用于隔离延迟敏感 Hook 与插件界面 Action 的持久运行时类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum VulcanPluginPersistentRuntimeKind {
    /// Runtime dedicated to hook handlers.
    /// 专用于 Hook 处理器的运行时。
    Hook,
    /// Runtime dedicated to global plugin UI actions.
    /// 专用于全局插件界面 Action 的运行时。
    UiAction,
}

impl VulcanPluginPersistentRuntimeKind {
    /// Returns the stable managed-lease identifier for this runtime category.
    /// 返回当前运行时类别的稳定受管租约标识。
    fn lease_id(self) -> &'static str {
        match self {
            Self::Hook => "enabled-plugin-hook-runtime",
            Self::UiAction => "enabled-plugin-ui-action-runtime",
        }
    }

    /// Returns the diagnostic category label used in lifecycle errors.
    /// 返回生命周期错误使用的诊断类别标签。
    fn label(self) -> &'static str {
        match self {
            Self::Hook => "hook",
            Self::UiAction => "UI action",
        }
    }
}

/// One raw lease retained after a failed immediate cleanup so later lifecycle drains can retry it.
/// 在立即清理失败后保留的原始租约，使后续生命周期排空可以重试。
#[derive(Debug, Clone)]
struct RetainedVulcanPluginLease {
    /// Stable owning plugin identifier.
    /// 稳定的所属插件标识。
    plugin_id: String,
    /// Optional controller session that caused this lease to be created.
    /// 触发创建该租约的可选控制器会话。
    owner_session_id: Option<String>,
    /// LuaSkills managed lease identifier.
    /// LuaSkills 受管租约标识。
    lease_id: String,
    /// Stable LuaSkills session identifier.
    /// 稳定的 LuaSkills 会话标识。
    sid: String,
    /// LuaSkills managed lease generation.
    /// LuaSkills 受管租约代次。
    generation: u64,
    /// Whether successful cleanup must release one plugin session slot.
    /// 成功清理后是否必须释放一个插件会话名额。
    reserved_session_slot: bool,
}

/// Shared mutable state retained across runtime-foundation refreshes.
/// 跨运行时基础刷新保留的共享可变状态。
struct VulcanPluginManagerState {
    /// Unique runtime identity injected into every fixed bridge host call.
    /// 注入每次固定桥接宿主调用的唯一运行时身份。
    manager_id: String,
    /// Immutable plugin engine; this lock guards only one-time attachment, never Lua execution.
    /// 不可变插件引擎；此锁仅保护一次性附加，绝不包围 Lua 执行。
    engine: RwLock<Option<Arc<LuaEngine>>>,
    /// Root containing deployed Vulcan Plugin packages.
    /// 包含已部署 Vulcan 插件包的根目录。
    vulcan_plugin_root: PathBuf,
    /// User-owned plugin root merged below bundled Vulcan plugins.
    /// 合并在内建 Vulcan 插件之后的用户插件根目录。
    user_plugin_root: PathBuf,
    /// Fixed host-owned LuaSkills bridge package used to execute every real plugin bundle.
    /// 用于执行全部真实插件 bundle 的固定宿主自有 LuaSkills 桥接包。
    bridge_package_root: PathBuf,
    /// Shared host-managed Python and Node distribution root.
    /// 宿主共享管理的 Python 与 Node 发行根目录。
    managed_runtime_distribution_root: PathBuf,
    /// User-owned root containing plugin configuration, authorization, and install records.
    /// 包含插件配置、授权与安装记录的用户状态根目录。
    plugin_state_root: PathBuf,
    /// User-owned root containing isolated plugin dependency environments.
    /// 包含隔离插件依赖环境的用户自有根目录。
    plugin_runtime_root: PathBuf,
    /// Host services installed by the surrounding application runtime.
    /// 由外围应用运行时安装的宿主服务。
    host_services: RwLock<VulcanPluginHostServices>,
    /// Current immutable catalog snapshot.
    /// 当前不可变目录快照。
    snapshot: RwLock<Arc<VulcanPluginCatalogSnapshot>>,
    /// Monotonic catalog revision counter.
    /// 单调目录修订计数器。
    next_revision: AtomicU64,
    /// Monotonic sidecar request identifier counter.
    /// 单调 sidecar 请求标识计数器。
    next_request_id: AtomicU64,
    /// Active sessions keyed by public host identifier.
    /// 以宿主公开标识为键的活动会话集合。
    sessions: Mutex<BTreeMap<String, Arc<ManagedVulcanPluginSession>>>,
    /// Short gate that orders catalog generation publication against final session registration.
    /// 对目录代次发布与最终会话注册进行排序的短同步门。
    session_registration_gate: Mutex<()>,
    /// Active session counts keyed by plugin identifier.
    /// 以插件标识为键的活动会话计数。
    active_session_counts: Mutex<BTreeMap<String, usize>>,
    /// Persistent hook runtimes keyed by plugin identifier.
    /// 以插件标识为键的持久 hook 运行时集合。
    hook_runtimes: Mutex<BTreeMap<String, Arc<ManagedVulcanPluginHookRuntime>>>,
    /// Persistent global UI action runtimes keyed by plugin identifier.
    /// 以插件标识为键的持久全局界面 Action 运行时集合。
    ui_action_runtimes: Mutex<BTreeMap<String, Arc<ManagedVulcanPluginHookRuntime>>>,
    /// Raw leases retained after cleanup failures and keyed by lease identifier.
    /// 清理失败后保留并以租约标识为键的原始租约集合。
    retained_leases: Mutex<BTreeMap<String, RetainedVulcanPluginLease>>,
    /// Weak plugin/category gates shared by initialization and drain; inactive entries are reclaimed.
    /// 初始化与排空共享的插件及类别弱引用同步门；回收无人持有的条目。
    persistent_runtime_gates: Mutex<
        BTreeMap<(String, VulcanPluginPersistentRuntimeKind), std::sync::Weak<AsyncMutex<()>>>,
    >,
    /// Bounded outstanding actions shared across lifecycle generations until actual cleanup.
    /// 跨生命周期代次共享的有界在途 Action，直至实际清理完成。
    action_admissions: Mutex<BTreeMap<String, std::sync::Weak<tokio::sync::Semaphore>>>,
    /// Safe invocation records retained independently from HTTP waiters.
    /// 独立于 HTTP 等待方保留的安全调用记录。
    action_records: Mutex<BTreeMap<String, Arc<action_status::ActionRecord>>>,
    /// Monotonic runtime generations keyed by plugin identifier.
    /// 以插件标识为键的单调运行时代次。
    plugin_generations: Mutex<BTreeMap<String, u64>>,
    /// Transient lifecycle overrides keyed by plugin identifier.
    /// 以插件标识为键的临时生命周期覆盖。
    lifecycle_overrides: Mutex<BTreeMap<String, VulcanPluginRuntimeState>>,
    /// Cleanup warnings retained after a plugin becomes disabled.
    /// 插件停用后保留的清理警告。
    cleanup_diagnostics: Mutex<BTreeMap<String, Vec<String>>>,
}

impl Drop for VulcanPluginManagerState {
    /// Closes every remaining managed lease during deterministic host shutdown.
    /// 在宿主确定性关闭期间释放所有剩余受管租约。
    fn drop(&mut self) {
        let sessions = self
            .sessions
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(engine_slot) = self.engine.get_mut() else {
            return;
        };
        let Some(engine) = engine_slot.as_ref() else {
            return;
        };
        for session in sessions.values() {
            let _ = close_system_lease_sync(engine, session);
        }
        let hook_runtimes = self
            .hook_runtimes
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for runtime in hook_runtimes.values() {
            let _ = close_raw_system_lease_sync(
                engine,
                &runtime.lease_id,
                &runtime.sid,
                runtime.generation,
            );
        }
        let ui_action_runtimes = self
            .ui_action_runtimes
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for runtime in ui_action_runtimes.values() {
            let _ = close_raw_system_lease_sync(
                engine,
                &runtime.lease_id,
                &runtime.sid,
                runtime.generation,
            );
        }
        let retained_leases = self
            .retained_leases
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for lease in retained_leases.values() {
            let _ =
                close_raw_system_lease_sync(engine, &lease.lease_id, &lease.sid, lease.generation);
        }
    }
}

/// Cloneable long-lived generic Vulcan Plugin manager.
/// 可克隆的长生命周期通用 Vulcan 插件管理器。
#[derive(Clone)]
pub struct VulcanPluginRuntimeManager {
    /// Shared runtime state.
    /// 共享运行时状态。
    state: Arc<VulcanPluginManagerState>,
}

impl fmt::Debug for VulcanPluginRuntimeManager {
    /// Formats manager metadata without exposing Lua engine internals.
    /// 在不暴露 Lua 引擎内部状态的前提下格式化管理器元数据。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.snapshot();
        formatter
            .debug_struct("VulcanPluginRuntimeManager")
            .field("vulcan_plugin_root", &self.state.vulcan_plugin_root)
            .field("manager_id", &self.state.manager_id)
            .field("user_plugin_root", &self.state.user_plugin_root)
            .field("bridge_package_root", &self.state.bridge_package_root)
            .field(
                "managed_runtime_distribution_root",
                &self.state.managed_runtime_distribution_root,
            )
            .field("plugin_state_root", &self.state.plugin_state_root)
            .field("plugin_runtime_root", &self.state.plugin_runtime_root)
            .field("catalog_revision", &snapshot.revision)
            .field("plugin_count", &snapshot.plugins.len())
            .field("enabled_tool_count", &snapshot.enabled_tools.len())
            .finish()
    }
}

impl VulcanPluginRuntimeManager {
    /// Creates one manager around an immutable plugin engine and deployed plugin root.
    /// 围绕不可变插件引擎与已部署插件根目录创建管理器。
    ///
    /// # Parameters
    /// 参数
    /// `engine` is separate from mutable ordinary-skill management; `vulcan_plugin_root` is the trusted package root.
    /// `engine` 独立于可变普通技能管理；`vulcan_plugin_root` 是受信任包根。
    ///
    /// # Returns
    /// 返回值
    /// Returns a manager initialized with an empty catalog snapshot.
    /// 返回以空目录快照初始化的管理器。
    pub fn new(
        engine: Arc<LuaEngine>,
        vulcan_plugin_root: PathBuf,
        user_plugin_root: PathBuf,
        bridge_package_root: PathBuf,
        managed_runtime_distribution_root: PathBuf,
        plugin_state_root: PathBuf,
        plugin_runtime_root: PathBuf,
    ) -> Self {
        Self::new_with_optional_engine(
            Some(engine),
            vulcan_plugin_root,
            user_plugin_root,
            bridge_package_root,
            managed_runtime_distribution_root,
            plugin_state_root,
            plugin_runtime_root,
        )
    }

    /// Creates one content-capable plugin manager without a Lua execution engine.
    /// 创建一个不含 Lua 执行引擎但可加载内容组件的插件管理器。
    ///
    /// # Parameters
    /// 参数
    /// The directory arguments have the same ownership and isolation semantics as `new`.
    /// 各目录参数与 `new` 具有相同的所有权及隔离语义。
    ///
    /// # Returns
    /// 返回值
    /// A manager that can discover and manage content-only plugins immediately.
    /// 返回可立即发现并管理内容型插件的管理器。
    pub fn without_runtime(
        vulcan_plugin_root: PathBuf,
        user_plugin_root: PathBuf,
        bridge_package_root: PathBuf,
        managed_runtime_distribution_root: PathBuf,
        plugin_state_root: PathBuf,
        plugin_runtime_root: PathBuf,
    ) -> Self {
        Self::new_with_optional_engine(
            None,
            vulcan_plugin_root,
            user_plugin_root,
            bridge_package_root,
            managed_runtime_distribution_root,
            plugin_state_root,
            plugin_runtime_root,
        )
    }

    /// Creates one manager from an optional execution engine and fixed host roots.
    /// 使用可选执行引擎与固定宿主根目录创建插件管理器。
    ///
    /// # Parameters
    /// 参数
    /// `engine` is optional while every path keeps the same meaning as the public constructors.
    /// `engine` 可选，其余路径与公开构造函数含义一致。
    ///
    /// # Returns
    /// 返回值
    /// A fully initialized shared manager.
    /// 返回完整初始化的共享管理器。
    fn new_with_optional_engine(
        engine: Option<Arc<LuaEngine>>,
        vulcan_plugin_root: PathBuf,
        user_plugin_root: PathBuf,
        bridge_package_root: PathBuf,
        managed_runtime_distribution_root: PathBuf,
        plugin_state_root: PathBuf,
        plugin_runtime_root: PathBuf,
    ) -> Self {
        let snapshot = Arc::new(VulcanPluginCatalogSnapshot::empty(
            vulcan_plugin_root.clone(),
        ));
        let has_engine = engine.is_some();
        let manager = Self {
            state: Arc::new(VulcanPluginManagerState {
                manager_id: Uuid::new_v4().to_string(),
                engine: RwLock::new(engine),
                vulcan_plugin_root,
                user_plugin_root,
                bridge_package_root,
                managed_runtime_distribution_root,
                plugin_state_root,
                plugin_runtime_root,
                host_services: RwLock::new(VulcanPluginHostServices::default()),
                snapshot: RwLock::new(snapshot),
                next_revision: AtomicU64::new(1),
                next_request_id: AtomicU64::new(1),
                sessions: Mutex::new(BTreeMap::new()),
                session_registration_gate: Mutex::new(()),
                active_session_counts: Mutex::new(BTreeMap::new()),
                hook_runtimes: Mutex::new(BTreeMap::new()),
                ui_action_runtimes: Mutex::new(BTreeMap::new()),
                retained_leases: Mutex::new(BTreeMap::new()),
                persistent_runtime_gates: Mutex::new(BTreeMap::new()),
                action_admissions: Mutex::new(BTreeMap::new()),
                action_records: Mutex::new(BTreeMap::new()),
                plugin_generations: Mutex::new(BTreeMap::new()),
                lifecycle_overrides: Mutex::new(BTreeMap::new()),
                cleanup_diagnostics: Mutex::new(BTreeMap::new()),
            }),
        };
        if has_engine {
            manager.install_host_tool_bridge();
        }
        manager
    }

    /// Attaches the immutable execution engine used by plugins exactly once.
    /// 恰好附加一次插件使用的不可变执行引擎。
    ///
    /// # Parameters
    /// 参数
    /// `engine` is the initialized LuaSkills engine owned by the host.
    /// `engine` 是宿主持有的已初始化 LuaSkills 引擎。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit after the engine and fixed bridge are published.
    /// 发布引擎与固定桥接后返回空值。
    pub fn attach_runtime_engine(&self, engine: Arc<LuaEngine>) -> Result<(), String> {
        let mut slot = self
            .state
            .engine
            .write()
            .map_err(|_| String::from("Vulcan Plugin runtime engine lock is poisoned."))?;
        if slot.is_some() {
            return Err(String::from(
                "Vulcan Plugin runtime engine is already attached.",
            ));
        }
        *slot = Some(engine);
        drop(slot);
        self.install_host_tool_bridge();
        Ok(())
    }

    /// Returns the attached Lua engine or an explicit content-only diagnostic.
    /// 返回已附加 Lua 引擎，或明确的内容型模式诊断。
    ///
    /// # Returns
    /// 返回值
    /// The shared engine required by one executable plugin operation.
    /// 单次执行型插件操作所需的共享引擎。
    fn runtime_engine(&self) -> Result<Arc<LuaEngine>, VulcanPluginRuntimeError> {
        self.state
            .engine
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Vulcan Plugin runtime engine lock is poisoned."))?
            .clone()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Vulcan Plugin execution runtime is unavailable; content-only plugins remain available.",
                )
            })
    }

    /// Returns the deployed Vulcan Plugin root.
    /// 返回已部署 Vulcan 插件根目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the immutable trusted package root path.
    /// 返回不可变的受信任包根路径。
    pub fn vulcan_plugin_root(&self) -> &Path {
        &self.state.vulcan_plugin_root
    }

    /// Returns the configured user plugin root.
    /// 返回已配置的用户插件根目录。
    ///
    /// Returns the exact root merged below bundled plugins.
    /// 返回合并在内建插件之后的精确根目录。
    pub fn user_plugin_root(&self) -> &Path {
        &self.state.user_plugin_root
    }

    /// Returns the user-owned plugin state root.
    /// 返回用户持有的插件状态根目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the immutable state root used for configuration and authorization records.
    /// 返回配置与授权记录使用的不可变状态根目录。
    pub fn plugin_state_root(&self) -> &Path {
        &self.state.plugin_state_root
    }
}

/// Acquires the shared active-session registry.
/// 获取共享活动会话注册表。
fn lock_sessions(
    sessions: &Mutex<BTreeMap<String, Arc<ManagedVulcanPluginSession>>>,
) -> Result<
    MutexGuard<'_, BTreeMap<String, Arc<ManagedVulcanPluginSession>>>,
    VulcanPluginRuntimeError,
> {
    sessions.lock().map_err(|_| {
        VulcanPluginRuntimeError::new("Vulcan Plugin session registry lock is poisoned.")
    })
}

/// Acquires the shared persistent hook-runtime registry.
/// 获取共享持久 hook 运行时注册表。
fn lock_hook_runtimes(
    runtimes: &Mutex<BTreeMap<String, Arc<ManagedVulcanPluginHookRuntime>>>,
) -> Result<
    MutexGuard<'_, BTreeMap<String, Arc<ManagedVulcanPluginHookRuntime>>>,
    VulcanPluginRuntimeError,
> {
    runtimes.lock().map_err(|_| {
        VulcanPluginRuntimeError::new("Vulcan Plugin hook runtime registry lock is poisoned.")
    })
}

/// Locks the retained raw-lease registry without silently recovering poisoned state.
/// 锁定保留原始租约注册表，不静默恢复已中毒状态。
fn lock_retained_leases(
    leases: &Mutex<BTreeMap<String, RetainedVulcanPluginLease>>,
) -> Result<MutexGuard<'_, BTreeMap<String, RetainedVulcanPluginLease>>, VulcanPluginRuntimeError> {
    leases.lock().map_err(|_| {
        VulcanPluginRuntimeError::new("Vulcan Plugin retained lease registry lock is poisoned.")
    })
}

#[cfg(test)]
mod tests;
