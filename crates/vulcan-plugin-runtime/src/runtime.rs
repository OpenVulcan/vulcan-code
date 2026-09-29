//! Generic Vulcan Plugin execution owned by the formal LuaSkills parent runtime.
//! 由正式 LuaSkills 父运行时持有的通用 Vulcan 插件执行。

mod action_status;
mod actions;
mod authorization;
pub(crate) use authorization::plugin_required_permissions;
mod bridge;
mod candidates;
pub use candidates::VulcanPluginPreparedMcpCandidate;
mod configuration;
pub(crate) use configuration::validate_plugin_config_value;
mod drain;
mod environment;
mod execution;
mod formal;
mod installation;
mod lease;
mod lifecycle;
mod local_files;
mod persistent;
mod process;
mod recovery;
mod requirements;
mod sessions;
mod settings;

// Preserve the existing public plugin API at its original module path.
// 在原模块路径保留既有公开插件API。
pub use action_status::{VulcanPluginActionIdentity, VulcanPluginActionStatus};
pub use actions::VulcanPluginActionRequest;
pub use authorization::{
    VulcanPluginInstallSecretRequirement, VulcanPluginMcpPermissionPreview,
    VulcanPluginPermissionPreview, plugin_permission_key,
};

pub use bridge::{
    VulcanPluginBridgeLeaseScope, VulcanPluginBundlePoolRequest, VulcanPluginEmbeddedHost,
};
use bridge::{canonical_bridge_workspace_root, required_json_string};
pub use drain::VulcanPluginDrainReport;
pub use formal::{
    VulcanPluginAutomaticPrewarmOrigin, VulcanPluginAutomaticPrewarmPermit,
    VulcanPluginAutomaticPrewarmTrigger, VulcanPluginCapacityControlAction,
    VulcanPluginCapacityControlRequest, VulcanPluginCapacityRegistrationRequest,
    VulcanPluginFormalCallStatus, VulcanPluginFormalCapacityCatalog,
    VulcanPluginFormalCapacityStatus, VulcanPluginFormalPluginStatus,
    VulcanPluginFormalRuntimeStatus, VulcanPluginFormalSessionStatus,
    VulcanPluginPrewarmTargetRequest, VulcanPluginPrewarmTargetStatus,
    VulcanPluginReusableDomainStatus,
};
pub use installation::{VulcanPluginInstallationResult, VulcanPluginUpdateTransaction};
pub use lifecycle::VulcanPluginLifecycleEvent;
use local_files::{
    canonical_plugin_file, copy_plugin_directory_tree, read_plugin_json_state,
    write_plugin_json_record,
};
pub use process::VulcanPluginMcpStdioLaunchSpec;
pub use sessions::VulcanPluginInvocationRequest;
use sessions::plugin_payload;
pub use settings::{
    VulcanPluginAutomaticPrewarmRule, VulcanPluginBudgetSource, VulcanPluginCapacityBudgetSource,
    VulcanPluginCapacityPolicyPhase, VulcanPluginCapacityPolicyRecord,
    VulcanPluginCapacityPolicyUpdate, VulcanPluginCapacityPurpose, VulcanPluginPrewarmPolicyPhase,
    VulcanPluginPrewarmPolicyRecord, VulcanPluginPrewarmPolicyStatus,
    VulcanPluginPrewarmPolicyUpdate, VulcanPluginRuntimeSettings, load_plugin_runtime_settings,
};

use crate::{VulcanPluginCatalogSnapshot, VulcanPluginHostServices, VulcanPluginRuntimeState};
use luaskills::LuaEngine;
use std::collections::BTreeMap;
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex, RwLock};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;

/// Sole enabling-probe budget covers registered preparation and actual module initialization.
/// 唯一启用探测预算覆盖已登记准备及实际模块初始化。
const VULCAN_PLUGIN_PROBE_TIMEOUT_MS: u64 = 5_000;

/// Default Lua execution time used when one hook declaration omits a timeout.
/// hook 声明省略超时时使用的 Lua 默认执行时长。
const VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS: u64 = 60_000;

/// Sole default shutdown budget for formal host finalizers and explicit host drains.
/// 正式宿主关闭器及显式宿主排空的唯一默认关闭预算。
const VULCAN_PLUGIN_DEFAULT_SHUTDOWN_TIMEOUT_MS: u64 = 5_000;

/// Exact backend implemented by every production plugin route and checked against manifest requirements.
/// 全部生产插件路由实现并用于清单要求检查的精确后端。
pub(crate) const VULCAN_PLUGIN_EXECUTION_BACKEND: luaskills::runtime::embedded::ExecutionBackend =
    luaskills::runtime::embedded::ExecutionBackend::InProcess;

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
    /// Returns the diagnostic category label used in lifecycle errors.
    /// 返回生命周期错误使用的诊断类别标签。
    fn label(self) -> &'static str {
        match self {
            Self::Hook => "hook",
            Self::UiAction => "UI action",
        }
    }
}

/// Weak admission gates are keyed by the exact plugin and persistent purpose.
/// 弱入场门以精确插件及持久用途为键。
type PersistentRuntimeGates =
    BTreeMap<(String, VulcanPluginPersistentRuntimeKind), std::sync::Weak<AsyncMutex<()>>>;

/// Shared mutable state retained across runtime-foundation refreshes.
/// 跨运行时基础刷新保留的共享可变状态。
struct VulcanPluginManagerState {
    /// Unique runtime identity injected into every fixed bridge host call.
    /// 注入每次固定桥接宿主调用的唯一运行时身份。
    manager_id: String,
    /// Immutable plugin engine; this lock guards only one-time attachment, never Lua execution.
    /// 不可变插件引擎；此锁仅保护一次性附加，绝不包围 Lua 执行。
    engine: RwLock<Option<Arc<LuaEngine>>>,
    /// One formal parent runtime attached to this manager; the lock never spans Lua execution.
    /// 附加到此管理器的单个正式父运行时；此锁绝不包围 Lua 执行。
    formal_runtime: Mutex<Option<Arc<formal::ManagedFormalRuntime>>>,
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
    /// Short gate that orders catalog generation publication against final session registration.
    /// 对目录代次发布与最终会话注册进行排序的短同步门。
    session_registration_gate: Mutex<()>,
    /// Active session counts keyed by plugin identifier.
    /// 以插件标识为键的活动会话计数。
    active_session_counts: Mutex<BTreeMap<String, usize>>,
    /// Weak plugin/category gates shared by initialization and drain; inactive entries are reclaimed.
    /// 初始化与排空共享的插件及类别弱引用同步门；回收无人持有的条目。
    persistent_runtime_gates: Mutex<PersistentRuntimeGates>,
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
    /// Original startup recovery obligations survive refreshes and missing journal files.
    /// 原始启动恢复义务在刷新及日志文件缺失后仍保留。
    activation_recovery: Mutex<Option<recovery::ActivationRecovery>>,
}

impl Drop for VulcanPluginManagerState {
    /// Request formal shutdown; unfinished physical work remains owned by the core.
    /// 请求正式关闭；未完成物理工作继续由核心持有。
    /// No input or return value; only subsequent core polling proves termination.
    /// 无参数或返回值；仅后续核心轮询证明终止。
    fn drop(&mut self) {
        if let Some(owner) = self
            .formal_runtime
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_ref()
        {
            let _ = owner.host.runtime().request_close();
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
                formal_runtime: Mutex::new(None),
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
                session_registration_gate: Mutex::new(()),
                active_session_counts: Mutex::new(BTreeMap::new()),
                persistent_runtime_gates: Mutex::new(BTreeMap::new()),
                action_admissions: Mutex::new(BTreeMap::new()),
                action_records: Mutex::new(BTreeMap::new()),
                plugin_generations: Mutex::new(BTreeMap::new()),
                lifecycle_overrides: Mutex::new(BTreeMap::new()),
                cleanup_diagnostics: Mutex::new(BTreeMap::new()),
                activation_recovery: Mutex::new(None),
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

#[cfg(test)]
mod tests;
