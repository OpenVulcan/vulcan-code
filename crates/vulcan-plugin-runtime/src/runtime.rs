//! Long-lived generic System Plugin session runtime backed by LuaSkills leases.
//! 由 LuaSkills 租约承载的长生命周期通用系统插件会话运行时。

use crate::{
    SYSTEM_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS, SYSTEM_PLUGIN_SESSION_ID_ARGUMENT,
    SystemPluginAvailability, SystemPluginCatalogSnapshot, SystemPluginHookDescriptor,
    SystemPluginManifest, SystemPluginRuntimeState, SystemPluginToolDescriptor,
    SystemPluginToolExecutionKind, load_system_plugin_catalog,
};
use luaskills::LuaEngine;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;
use vulcan_hooks::{HookInvocation, HookResult};

/// Maximum Lua execution time used while probing one plugin package.
/// 探测单个插件包时使用的 Lua 最大执行时长。
const SYSTEM_PLUGIN_PROBE_TIMEOUT_MS: u64 = 5_000;

/// Default Lua execution time used when one hook declaration omits a timeout.
/// hook 声明省略超时时使用的 Lua 默认执行时长。
const SYSTEM_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS: u64 = 60_000;

/// Maximum Lua execution time used while shutting down one persistent hook runtime.
/// 关闭单个持久 hook 运行时时使用的 Lua 最大执行时长。
const SYSTEM_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS: u64 = 5_000;

/// One model tool invocation projected into the generic plugin runtime.
/// 投影到通用插件运行时的单次模型工具调用。
#[derive(Debug, Clone)]
pub struct SystemPluginInvocationRequest {
    /// Exact plugin tool identifier.
    /// 精确的插件工具标识。
    pub tool_id: String,
    /// Raw JSON argument object validated by the provider tool schema.
    /// 已由 Provider 工具 Schema 校验的原始 JSON 参数对象。
    pub arguments: Value,
    /// Stable controller session that owns opened plugin sessions.
    /// 拥有已打开插件会话的稳定控制器会话。
    pub owner_session_id: String,
    /// Canonical workspace root exposed to the managed lease.
    /// 暴露给受管租约的规范化工作区根目录。
    pub workspace_root: PathBuf,
}

/// Structured generic plugin runtime failure.
/// 结构化的通用插件运行时失败。
#[derive(Debug, Clone)]
pub struct SystemPluginRuntimeError {
    /// Human-readable English diagnostic returned to the model tool layer.
    /// 返回给模型工具层的人类可读英文诊断。
    pub message: String,
}

impl SystemPluginRuntimeError {
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
    /// Returns a new System Plugin runtime error.
    /// 返回新的系统插件运行时错误。
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for SystemPluginRuntimeError {
    /// Renders the runtime error as its exact diagnostic text.
    /// 将运行时错误渲染为其精确诊断文本。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.message)
    }
}

impl Error for SystemPluginRuntimeError {}

/// One owner-bound managed plugin session.
/// 单个绑定所有者的受管插件会话。
struct ManagedSystemPluginSession {
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
    manifest: Arc<SystemPluginManifest>,
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

/// One persistent plugin-level Lua lease shared by all handlers from the same plugin generation.
/// 由同一插件代次的全部处理器共享的单个持久插件级 Lua 租约。
struct ManagedSystemPluginHookRuntime {
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
    manifest: Arc<SystemPluginManifest>,
    /// Serial gate that preserves deterministic plugin-local hook state.
    /// 保持插件本地 hook 状态确定性的串行门。
    operation_gate: AsyncMutex<()>,
    /// Closing state used to reject late invocations.
    /// 用于拒绝迟到调用的关闭状态。
    closing: AtomicBool,
}

/// One raw lease retained after a failed immediate cleanup so later lifecycle drains can retry it.
/// 在立即清理失败后保留的原始租约，使后续生命周期排空可以重试。
#[derive(Debug, Clone)]
struct RetainedSystemPluginLease {
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
struct SystemPluginManagerState {
    /// Shared LuaSkills engine used for managed System leases.
    /// 用于受管系统租约的共享 LuaSkills 引擎。
    engine: Arc<Mutex<LuaEngine>>,
    /// Root containing deployed System Plugin packages.
    /// 包含已部署系统插件包的根目录。
    system_plugin_root: PathBuf,
    /// Current immutable catalog snapshot.
    /// 当前不可变目录快照。
    snapshot: RwLock<Arc<SystemPluginCatalogSnapshot>>,
    /// Monotonic catalog revision counter.
    /// 单调目录修订计数器。
    next_revision: AtomicU64,
    /// Monotonic sidecar request identifier counter.
    /// 单调 sidecar 请求标识计数器。
    next_request_id: AtomicU64,
    /// Active sessions keyed by public host identifier.
    /// 以宿主公开标识为键的活动会话集合。
    sessions: Mutex<BTreeMap<String, Arc<ManagedSystemPluginSession>>>,
    /// Short gate that orders catalog generation publication against final session registration.
    /// 对目录代次发布与最终会话注册进行排序的短同步门。
    session_registration_gate: Mutex<()>,
    /// Active session counts keyed by plugin identifier.
    /// 以插件标识为键的活动会话计数。
    active_session_counts: Mutex<BTreeMap<String, usize>>,
    /// Persistent hook runtimes keyed by plugin identifier.
    /// 以插件标识为键的持久 hook 运行时集合。
    hook_runtimes: Mutex<BTreeMap<String, Arc<ManagedSystemPluginHookRuntime>>>,
    /// Raw leases retained after cleanup failures and keyed by lease identifier.
    /// 清理失败后保留并以租约标识为键的原始租约集合。
    retained_leases: Mutex<BTreeMap<String, RetainedSystemPluginLease>>,
    /// Global initialization gate preventing duplicate hook leases during concurrent first calls.
    /// 防止并发首次调用创建重复 hook 租约的全局初始化门。
    hook_runtime_initialization_gate: AsyncMutex<()>,
    /// Monotonic runtime generations keyed by plugin identifier.
    /// 以插件标识为键的单调运行时代次。
    plugin_generations: Mutex<BTreeMap<String, u64>>,
    /// Transient lifecycle overrides keyed by plugin identifier.
    /// 以插件标识为键的临时生命周期覆盖。
    lifecycle_overrides: Mutex<BTreeMap<String, SystemPluginRuntimeState>>,
    /// Cleanup warnings retained after a plugin becomes disabled.
    /// 插件停用后保留的清理警告。
    cleanup_diagnostics: Mutex<BTreeMap<String, Vec<String>>>,
}

/// Result returned after bounded plugin session draining.
/// 有界插件会话排空后返回的结果。
#[derive(Debug, Clone, Default)]
pub struct SystemPluginDrainReport {
    /// Number of sessions closed and removed successfully.
    /// 成功关闭并移除的会话数量。
    pub closed_sessions: usize,
    /// Number of persistent plugin hook runtimes closed successfully.
    /// 成功关闭的持久插件 hook 运行时数量。
    pub closed_hook_runtimes: usize,
    /// Number of previously retained raw leases closed successfully.
    /// 成功关闭的此前保留原始租约数量。
    pub closed_retained_leases: usize,
    /// Deterministic cleanup failures retained for diagnostics and retry.
    /// 为诊断与重试保留的确定性清理失败集合。
    pub failures: Vec<String>,
}

impl SystemPluginDrainReport {
    /// Merges another drain phase into this ordered lifecycle report.
    /// 将另一个排空阶段合并到当前有序生命周期报告。
    fn merge(&mut self, other: Self) {
        self.closed_sessions = self.closed_sessions.saturating_add(other.closed_sessions);
        self.closed_hook_runtimes = self
            .closed_hook_runtimes
            .saturating_add(other.closed_hook_runtimes);
        self.closed_retained_leases = self
            .closed_retained_leases
            .saturating_add(other.closed_retained_leases);
        self.failures.extend(other.failures);
    }
}

impl Drop for SystemPluginManagerState {
    /// Closes every remaining managed lease during deterministic host shutdown.
    /// 在宿主确定性关闭期间释放所有剩余受管租约。
    fn drop(&mut self) {
        let sessions = self
            .sessions
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Ok(engine) = self.engine.lock() else {
            return;
        };
        for session in sessions.values() {
            let _ = close_system_lease_sync(&engine, session);
        }
        let hook_runtimes = self
            .hook_runtimes
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for runtime in hook_runtimes.values() {
            let _ = close_raw_system_lease_sync(
                &engine,
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
                close_raw_system_lease_sync(&engine, &lease.lease_id, &lease.sid, lease.generation);
        }
    }
}

/// Cloneable long-lived generic System Plugin manager.
/// 可克隆的长生命周期通用系统插件管理器。
#[derive(Clone)]
pub struct SystemPluginRuntimeManager {
    /// Shared runtime state.
    /// 共享运行时状态。
    state: Arc<SystemPluginManagerState>,
}

impl fmt::Debug for SystemPluginRuntimeManager {
    /// Formats manager metadata without exposing Lua engine internals.
    /// 在不暴露 Lua 引擎内部状态的前提下格式化管理器元数据。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.snapshot();
        formatter
            .debug_struct("SystemPluginRuntimeManager")
            .field("system_plugin_root", &self.state.system_plugin_root)
            .field("catalog_revision", &snapshot.revision)
            .field("plugin_count", &snapshot.plugins.len())
            .field("enabled_tool_count", &snapshot.enabled_tools.len())
            .finish()
    }
}

impl SystemPluginRuntimeManager {
    /// Creates one manager around the shared LuaSkills engine and deployed plugin root.
    /// 围绕共享 LuaSkills 引擎与已部署插件根目录创建管理器。
    ///
    /// # Parameters
    /// 参数
    /// `engine` is the shared LuaSkills engine and `system_plugin_root` is the trusted package root.
    /// `engine` 是共享 LuaSkills 引擎，`system_plugin_root` 是受信任包根。
    ///
    /// # Returns
    /// 返回值
    /// Returns a manager initialized with an empty catalog snapshot.
    /// 返回以空目录快照初始化的管理器。
    pub fn new(engine: Arc<Mutex<LuaEngine>>, system_plugin_root: PathBuf) -> Self {
        let snapshot = Arc::new(SystemPluginCatalogSnapshot::empty(
            system_plugin_root.clone(),
        ));
        Self {
            state: Arc::new(SystemPluginManagerState {
                engine,
                system_plugin_root,
                snapshot: RwLock::new(snapshot),
                next_revision: AtomicU64::new(1),
                next_request_id: AtomicU64::new(1),
                sessions: Mutex::new(BTreeMap::new()),
                session_registration_gate: Mutex::new(()),
                active_session_counts: Mutex::new(BTreeMap::new()),
                hook_runtimes: Mutex::new(BTreeMap::new()),
                retained_leases: Mutex::new(BTreeMap::new()),
                hook_runtime_initialization_gate: AsyncMutex::new(()),
                plugin_generations: Mutex::new(BTreeMap::new()),
                lifecycle_overrides: Mutex::new(BTreeMap::new()),
                cleanup_diagnostics: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    /// Returns the deployed System Plugin root.
    /// 返回已部署系统插件根目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the immutable trusted package root path.
    /// 返回不可变的受信任包根路径。
    pub fn system_plugin_root(&self) -> &Path {
        &self.state.system_plugin_root
    }

    /// Atomically replaces the catalog snapshot after a full package scan.
    /// 在完整扫描插件包后原子替换目录快照。
    ///
    /// # Parameters
    /// 参数
    /// The two sets contain explicit disabled and enabled plugin identifiers.
    /// 两个集合分别包含显式停用与显式启用的插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns the newly published immutable catalog snapshot.
    /// 返回新发布的不可变目录快照。
    pub fn refresh_catalog(
        &self,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
    ) -> Result<Arc<SystemPluginCatalogSnapshot>, SystemPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin session registration gate is poisoned.")
        })?;
        let revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut snapshot = load_system_plugin_catalog(
            &self.state.system_plugin_root,
            disabled_plugin_ids,
            enabled_plugin_ids,
            revision,
        );
        self.project_runtime_state(&mut snapshot)?;
        let snapshot = Arc::new(snapshot);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::clone(&snapshot);
        Ok(snapshot)
    }

    /// Marks one valid plugin as enabling and immediately removes any stale provider surface.
    /// 将一个有效插件标记为正在启用，并立即移除任何陈旧 Provider 工具面。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact discovered plugin identifier.
    /// `plugin_id` 是精确的已发现插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after publishing the transient lifecycle state.
    /// 在发布临时生命周期状态后返回成功。
    pub fn begin_enabling(&self, plugin_id: &str) -> Result<(), SystemPluginRuntimeError> {
        self.begin_lifecycle_transition(plugin_id, SystemPluginRuntimeState::Enabling)
    }

    /// Marks one enabled plugin as draining and immediately rejects every new invocation.
    /// 将一个已启用插件标记为正在排空，并立即拒绝所有新调用。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact discovered plugin identifier.
    /// `plugin_id` 是精确的已发现插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after tools and hooks leave the enabled surface.
    /// 在工具与 Hook 离开启用面后返回成功。
    pub fn begin_draining(&self, plugin_id: &str) -> Result<(), SystemPluginRuntimeError> {
        self.begin_lifecycle_transition(plugin_id, SystemPluginRuntimeState::Draining)
    }

    /// Clears a transient lifecycle transition and rebuilds the catalog from persisted policy.
    /// 清除临时生命周期转换，并根据持久化策略重建目录。
    ///
    /// # Parameters
    /// 参数
    /// The plugin id selects the transition, policy sets are persisted overrides, and failures are unresolved cleanup diagnostics.
    /// 插件标识选择转换，策略集合是持久化覆盖，失败集合是未解决清理诊断。
    ///
    /// # Returns
    /// 返回值
    /// Returns the final immutable catalog snapshot.
    /// 返回最终不可变目录快照。
    pub fn finish_lifecycle_transition(
        &self,
        plugin_id: &str,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
        cleanup_failures: Vec<String>,
    ) -> Result<Arc<SystemPluginCatalogSnapshot>, SystemPluginRuntimeError> {
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                SystemPluginRuntimeError::new("System Plugin lifecycle lock is poisoned.")
            })?
            .remove(plugin_id);
        if cleanup_failures.is_empty() {
            self.clear_cleanup_diagnostics_if_fully_drained(plugin_id)?;
        } else {
            let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
                SystemPluginRuntimeError::new("System Plugin cleanup diagnostic lock is poisoned.")
            })?;
            diagnostics.insert(plugin_id.to_owned(), cleanup_failures);
        }
        self.refresh_catalog(disabled_plugin_ids, enabled_plugin_ids)
    }

    /// Probes dependency resolution and Lua module loading without opening a business session.
    /// 在不打开业务会话的前提下探测依赖解析与 Lua 模块加载。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin to probe.
    /// `plugin_id` 是需要探测的精确插件。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after the probe lease is also closed successfully.
    /// 仅在探测租约也成功关闭后返回成功。
    pub async fn probe_plugin(&self, plugin_id: &str) -> Result<(), SystemPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if !matches!(
            plugin.availability,
            SystemPluginAvailability::Enabled | SystemPluginAvailability::Disabled
        ) {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin `{plugin_id}` is unavailable: {}",
                plugin.diagnostics.join("; ")
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let probe_id = Uuid::new_v4().to_string();
        let (lease_id, sid, generation) = self
            .create_managed_lease(
                &probe_id,
                &plugin.package_root,
                &plugin.package_root,
                &manifest,
            )
            .await?;
        let mut required_functions = vec![
            manifest.runtime.invoke_function.as_str(),
            manifest.runtime.shutdown_function.as_str(),
        ];
        required_functions.extend(manifest.hooks.iter().map(|hook| hook.function.as_str()));
        if let Some(function) = manifest.runtime.hook_shutdown_function.as_deref() {
            required_functions.push(function);
        }
        let function_checks = required_functions
            .into_iter()
            .map(|function| format!("type(plugin.{function}) == 'function'"))
            .collect::<Vec<_>>()
            .join(" and ");
        let code = format!(
            "local plugin = require('{}')\nreturn type(plugin) == 'table' and {function_checks}",
            manifest.runtime.module
        );
        let engine = Arc::clone(&self.state.engine);
        let stable_plugin_id = plugin_id.to_owned();
        let probe_result = call_engine_blocking(Arc::clone(&engine), {
            let lease_id = lease_id.clone();
            let sid = sid.clone();
            let plugin_id = stable_plugin_id.clone();
            move |engine| {
                let raw = engine
                    .eval_system_runtime_lease_json(
                        &json!({
                            "lease_id": lease_id,
                            "sid": sid,
                            "generation": generation,
                            "code": code,
                            "args": {},
                            "timeout_ms": SYSTEM_PLUGIN_PROBE_TIMEOUT_MS,
                        })
                        .to_string(),
                    )
                    .map_err(SystemPluginRuntimeError::new)?;
                let response = parse_engine_response(&raw, "probe System Plugin runtime lease")?;
                if response.get("result").and_then(Value::as_bool) != Some(true) {
                    return Err(SystemPluginRuntimeError::new(format!(
                        "System Plugin `{plugin_id}` probe did not return true."
                    )));
                }
                Ok(())
            }
        })
        .await;
        let retained_lease = RetainedSystemPluginLease {
            plugin_id: stable_plugin_id.clone(),
            owner_session_id: None,
            lease_id: lease_id.clone(),
            sid: sid.clone(),
            generation,
            reserved_session_slot: false,
        };
        let close_result = call_engine_blocking(engine, move |engine| {
            let raw = engine
                .close_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id,
                        "sid": sid,
                        "generation": generation,
                    })
                    .to_string(),
                )
                .map_err(SystemPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close System Plugin probe lease").map(|_| ())
        })
        .await;
        match (probe_result, close_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(probe_error), Ok(())) => Err(probe_error),
            (probe_result, Err(close_error)) => {
                let diagnostic = format!(
                    "System Plugin `{stable_plugin_id}` probe lease cleanup failed and remains retained: {close_error}"
                );
                self.retain_failed_lease(retained_lease, diagnostic.clone())?;
                let message = match probe_result {
                    Ok(()) => diagnostic,
                    Err(probe_error) => format!("{probe_error}; {diagnostic}"),
                };
                Err(SystemPluginRuntimeError::new(message))
            }
        }
    }

    /// Closes every retained session owned by one draining plugin.
    /// 关闭一个正在排空的插件所拥有的全部保留会话。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin whose runtime resources must be drained.
    /// `plugin_id` 是需要排空运行时资源的精确插件。
    ///
    /// # Returns
    /// 返回值
    /// Returns counts and non-fatal cleanup failures for the drain attempt.
    /// 返回本次排空尝试的计数与非致命清理失败。
    pub async fn drain_plugin_sessions(
        &self,
        plugin_id: &str,
    ) -> Result<SystemPluginDrainReport, SystemPluginRuntimeError> {
        let sessions = lock_sessions(&self.state.sessions)?
            .values()
            .filter(|session| session.plugin_id == plugin_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut report = self.drain_sessions(sessions).await?;
        let _hook_lifecycle = self.state.hook_runtime_initialization_gate.lock().await;
        let hook_runtime = {
            let runtimes = lock_hook_runtimes(&self.state.hook_runtimes)?;
            runtimes.get(plugin_id).cloned()
        };
        if let Some(runtime) = hook_runtime {
            match self.close_hook_runtime(Arc::clone(&runtime)).await {
                Ok(shutdown_diagnostic) => {
                    lock_hook_runtimes(&self.state.hook_runtimes)?.remove(plugin_id);
                    report.closed_hook_runtimes += 1;
                    if let Some(diagnostic) = shutdown_diagnostic {
                        report.failures.push(diagnostic);
                    }
                }
                Err(error) => {
                    runtime.closing.store(false, Ordering::Release);
                    let diagnostic = format!(
                        "Plugin `{plugin_id}` hook runtime cleanup failed and remains retained: {error}"
                    );
                    self.record_cleanup_failure(plugin_id, diagnostic.clone())?;
                    report.failures.push(diagnostic);
                }
            }
        }
        let retained = self.take_retained_leases(|lease| lease.plugin_id == plugin_id)?;
        report.merge(self.drain_retained_leases(retained).await?);
        Ok(report)
    }

    /// Closes every retained plugin session owned by one archived conversation.
    /// 关闭一个已归档会话所拥有的全部保留插件会话。
    ///
    /// # Parameters
    /// 参数
    /// `owner_session_id` is the stable owner identifier attached at session creation.
    /// `owner_session_id` 是创建会话时绑定的稳定所有者标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns counts and non-fatal cleanup failures for all matching resources.
    /// 返回所有匹配资源的计数与非致命清理失败。
    pub async fn drain_owner_sessions(
        &self,
        owner_session_id: &str,
    ) -> Result<SystemPluginDrainReport, SystemPluginRuntimeError> {
        let sessions = lock_sessions(&self.state.sessions)?
            .values()
            .filter(|session| session.owner_session_id == owner_session_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut affected_plugin_ids = sessions
            .iter()
            .map(|session| session.plugin_id.clone())
            .collect::<BTreeSet<_>>();
        let mut report = self.drain_sessions(sessions).await?;
        let retained = self.take_retained_leases(|lease| {
            lease.owner_session_id.as_deref() == Some(owner_session_id)
        })?;
        affected_plugin_ids.extend(retained.iter().map(|lease| lease.plugin_id.clone()));
        report.merge(self.drain_retained_leases(retained).await?);
        for plugin_id in affected_plugin_ids {
            self.clear_cleanup_diagnostics_if_fully_drained(&plugin_id)?;
        }
        Ok(report)
    }

    /// Performs deterministic shutdown and lease cleanup for an exact session set.
    /// 对精确会话集合执行确定性关闭与租约清理。
    async fn drain_sessions(
        &self,
        sessions: Vec<Arc<ManagedSystemPluginSession>>,
    ) -> Result<SystemPluginDrainReport, SystemPluginRuntimeError> {
        let mut report = SystemPluginDrainReport::default();
        for session in sessions {
            let plugin_id = session.plugin_id.clone();
            let _close = session.close_gate.lock().await;
            let is_registered = lock_sessions(&self.state.sessions)?
                .get(&session.session_id)
                .is_some_and(|current| Arc::ptr_eq(current, &session));
            if !is_registered {
                continue;
            }
            if session
                .closing
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            let close_execution = session
                .manifest
                .tools
                .iter()
                .find(|tool| tool.execution.kind == SystemPluginToolExecutionKind::SessionClose)
                .map(|tool| tool.execution.clone())
                .ok_or_else(|| {
                    SystemPluginRuntimeError::new(format!(
                        "System Plugin `{plugin_id}` declares no session-close operation."
                    ))
                })?;
            let payload = plugin_payload(&close_execution.operation, Map::new())?;
            let host_timeout_ms = close_execution
                .timeout_ms
                .checked_add(close_execution.timeout_grace_ms)
                .ok_or_else(|| {
                    SystemPluginRuntimeError::new("System Plugin close timeout overflowed.")
                })?;
            let shutdown_result = self
                .dispatch(Arc::clone(&session), payload, host_timeout_ms, true)
                .await;
            let close_result = self.close_lease(Arc::clone(&session)).await;
            match close_result {
                Ok(_) => {
                    lock_sessions(&self.state.sessions)?.remove(&session.session_id);
                    self.release_session_slot(&plugin_id);
                    report.closed_sessions += 1;
                    if let Err(error) = shutdown_result {
                        report.failures.push(format!(
                            "Session `{}` lease closed after graceful shutdown failed: {}",
                            session.session_id, error
                        ));
                    }
                }
                Err(error) => {
                    session.closing.store(false, Ordering::Release);
                    let diagnostic = format!(
                        "Session `{}` lease cleanup failed and remains retained: {}",
                        session.session_id, error
                    );
                    self.record_cleanup_failure(&plugin_id, diagnostic.clone())?;
                    report.failures.push(diagnostic);
                }
            }
        }
        Ok(report)
    }

    /// Retries exact closure for raw leases retained by earlier failed cleanup paths.
    /// 对此前清理失败路径保留的原始租约重试精确关闭。
    async fn drain_retained_leases(
        &self,
        leases: Vec<RetainedSystemPluginLease>,
    ) -> Result<SystemPluginDrainReport, SystemPluginRuntimeError> {
        let mut report = SystemPluginDrainReport::default();
        for lease in leases {
            match self
                .close_raw_lease(&lease.lease_id, &lease.sid, lease.generation)
                .await
            {
                Ok(()) => {
                    if lease.reserved_session_slot {
                        self.release_session_slot(&lease.plugin_id);
                    }
                    report.closed_retained_leases += 1;
                }
                Err(error) => {
                    self.restore_retained_lease(lease.clone())?;
                    let diagnostic = format!(
                        "Retained lease `{}` cleanup retry failed: {error}",
                        lease.lease_id
                    );
                    self.record_cleanup_failure(&lease.plugin_id, diagnostic.clone())?;
                    report.failures.push(diagnostic);
                }
            }
        }
        Ok(report)
    }

    /// Atomically claims retained raw leases that match one exact lifecycle predicate.
    /// 原子领取匹配一个精确生命周期谓词的保留原始租约。
    fn take_retained_leases(
        &self,
        predicate: impl Fn(&RetainedSystemPluginLease) -> bool,
    ) -> Result<Vec<RetainedSystemPluginLease>, SystemPluginRuntimeError> {
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        let lease_ids = retained
            .values()
            .filter(|lease| predicate(lease))
            .map(|lease| lease.lease_id.clone())
            .collect::<Vec<_>>();
        Ok(lease_ids
            .into_iter()
            .filter_map(|lease_id| retained.remove(&lease_id))
            .collect())
    }

    /// Retains one raw lease after immediate cleanup fails and records a visible diagnostic.
    /// 在立即清理失败后保留一个原始租约，并记录可见诊断。
    fn retain_failed_lease(
        &self,
        lease: RetainedSystemPluginLease,
        diagnostic: String,
    ) -> Result<(), SystemPluginRuntimeError> {
        let plugin_id = lease.plugin_id.clone();
        let lease_id = lease.lease_id.clone();
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        if retained.contains_key(&lease_id) {
            return Err(SystemPluginRuntimeError::new(format!(
                "Retained System Plugin lease id `{lease_id}` already exists."
            )));
        }
        retained.insert(lease_id, lease);
        drop(retained);
        self.record_cleanup_failure(&plugin_id, diagnostic)
    }

    /// Restores one atomically claimed lease after its cleanup retry fails.
    /// 在清理重试失败后恢复一个已被原子领取的租约。
    fn restore_retained_lease(
        &self,
        lease: RetainedSystemPluginLease,
    ) -> Result<(), SystemPluginRuntimeError> {
        let lease_id = lease.lease_id.clone();
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        if retained.contains_key(&lease_id) {
            return Err(SystemPluginRuntimeError::new(format!(
                "Retained System Plugin lease id `{lease_id}` was restored concurrently."
            )));
        }
        retained.insert(lease_id, lease);
        Ok(())
    }

    /// Retains one failed-cleanup session lease while preserving its reserved capacity for retry.
    /// 保留一个清理失败的会话租约，并为重试继续占用其已预留容量。
    fn retain_failed_session_lease(
        &self,
        session: &ManagedSystemPluginSession,
        diagnostic: String,
    ) -> Result<(), SystemPluginRuntimeError> {
        self.retain_failed_lease(
            RetainedSystemPluginLease {
                plugin_id: session.plugin_id.clone(),
                owner_session_id: Some(session.owner_session_id.clone()),
                lease_id: session.lease_id.clone(),
                sid: session.sid.clone(),
                generation: session.generation,
                reserved_session_slot: true,
            },
            diagnostic,
        )
    }

    /// Retains one cleanup diagnostic so UI and later refreshes cannot hide leaked resources.
    /// 保留一条清理诊断，确保界面与后续刷新不会隐藏泄漏资源。
    fn record_cleanup_failure(
        &self,
        plugin_id: &str,
        diagnostic: String,
    ) -> Result<(), SystemPluginRuntimeError> {
        let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let plugin_diagnostics = diagnostics.entry(plugin_id.to_owned()).or_default();
        if !plugin_diagnostics.contains(&diagnostic) {
            plugin_diagnostics.push(diagnostic);
        }
        drop(diagnostics);
        self.publish_cleanup_diagnostics(plugin_id)
    }

    /// Publishes newly recorded cleanup failures without waiting for a later catalog refresh.
    /// 无需等待后续目录刷新即可发布新记录的清理失败。
    fn publish_cleanup_diagnostics(&self, plugin_id: &str) -> Result<(), SystemPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin session registration gate is poisoned.")
        })?;
        let cleanup_diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let Some(plugin_diagnostics) = cleanup_diagnostics.get(plugin_id) else {
            return Ok(());
        };
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        if plugin.runtime_state == SystemPluginRuntimeState::Disabled {
            plugin.runtime_state = SystemPluginRuntimeState::DisabledWithCleanupWarning;
        }
        for diagnostic in plugin_diagnostics {
            if !plugin.diagnostics.contains(diagnostic) {
                plugin.diagnostics.push(diagnostic.clone());
            }
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }

    /// Clears stale cleanup diagnostics only after every resource owned by one plugin is gone.
    /// 仅在单个插件拥有的全部资源均已消失后清除陈旧清理诊断。
    fn clear_cleanup_diagnostics_if_fully_drained(
        &self,
        plugin_id: &str,
    ) -> Result<(), SystemPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin session registration gate is poisoned.")
        })?;
        let has_sessions = lock_sessions(&self.state.sessions)?
            .values()
            .any(|session| session.plugin_id == plugin_id);
        let has_retained_leases = lock_retained_leases(&self.state.retained_leases)?
            .values()
            .any(|lease| lease.plugin_id == plugin_id);
        let has_hook_runtime =
            lock_hook_runtimes(&self.state.hook_runtimes)?.contains_key(plugin_id);
        let has_active_count = self
            .state
            .active_session_counts
            .lock()
            .map_err(|_| {
                SystemPluginRuntimeError::new("System Plugin session counter lock is poisoned.")
            })?
            .get(plugin_id)
            .copied()
            .unwrap_or_default()
            > 0;
        if has_sessions || has_retained_leases || has_hook_runtime || has_active_count {
            return Ok(());
        }
        let removed_diagnostics = self
            .state
            .cleanup_diagnostics
            .lock()
            .map_err(|_| {
                SystemPluginRuntimeError::new("System Plugin cleanup diagnostic lock is poisoned.")
            })?
            .remove(plugin_id)
            .unwrap_or_default();
        if removed_diagnostics.is_empty() {
            return Ok(());
        }
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        plugin
            .diagnostics
            .retain(|diagnostic| !removed_diagnostics.contains(diagnostic));
        if plugin.runtime_state == SystemPluginRuntimeState::DisabledWithCleanupWarning {
            plugin.runtime_state = SystemPluginRuntimeState::Disabled;
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }

    /// Returns one immutable current catalog snapshot.
    /// 返回一份不可变的当前目录快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns a reference-counted snapshot that remains stable for its consumer.
    /// 返回供消费者稳定持有的引用计数快照。
    pub fn snapshot(&self) -> Arc<SystemPluginCatalogSnapshot> {
        self.state
            .snapshot
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .unwrap_or_else(|poisoned| Arc::clone(&poisoned.into_inner()))
    }

    /// Invokes one currently enabled plugin tool through the generic session lifecycle.
    /// 通过通用会话生命周期调用一个当前已启用的插件工具。
    ///
    /// # Parameters
    /// 参数
    /// `request` contains the tool id, model arguments, owner id, and workspace root.
    /// `request` 包含工具标识、模型参数、所有者标识和工作区根。
    ///
    /// # Returns
    /// 返回值
    /// Returns the plugin result with host session metadata when applicable.
    /// 返回插件结果，并在适用时包含宿主会话元数据。
    pub async fn invoke(
        &self,
        request: SystemPluginInvocationRequest,
    ) -> Result<Value, SystemPluginRuntimeError> {
        let tool = self
            .snapshot()
            .enabled_tools
            .get(&request.tool_id)
            .cloned()
            .ok_or_else(|| {
                SystemPluginRuntimeError::new(format!(
                    "System Plugin tool `{}` is not enabled in the current catalog.",
                    request.tool_id
                ))
            })?;
        self.ensure_current_tool_generation(&tool)?;
        match tool.manifest.execution.kind {
            SystemPluginToolExecutionKind::Stateless => self.invoke_stateless(&tool, request).await,
            SystemPluginToolExecutionKind::SessionOpen => self.open_session(&tool, request).await,
            SystemPluginToolExecutionKind::SessionCall => self.call_session(&tool, request).await,
            SystemPluginToolExecutionKind::SessionClose => self.close_session(&tool, request).await,
        }
    }

    /// Invokes one tool through a temporary lease that is always closed before returning.
    /// 通过一个返回前始终关闭的临时租约调用单个工具。
    async fn invoke_stateless(
        &self,
        tool: &SystemPluginToolDescriptor,
        request: SystemPluginInvocationRequest,
    ) -> Result<Value, SystemPluginRuntimeError> {
        let arguments = require_argument_object(request.arguments)?;
        if arguments.contains_key(SYSTEM_PLUGIN_SESSION_ID_ARGUMENT) {
            return Err(SystemPluginRuntimeError::new(
                "A stateless System Plugin tool must not accept the host-owned session_id argument.",
            ));
        }
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        let workspace_root = request.workspace_root.canonicalize().map_err(|error| {
            SystemPluginRuntimeError::new(format!("Failed to resolve workspace root: {error}"))
        })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` is missing from the current catalog.",
                tool.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` has no valid manifest.",
                tool.plugin_id
            ))
        })?;
        self.reserve_session_slot(&tool.plugin_id, manifest.runtime.max_active_sessions)?;
        let temporary_id = Uuid::new_v4().to_string();
        let create_result = self
            .create_managed_lease(
                &format!("stateless-{temporary_id}"),
                &plugin.package_root,
                &workspace_root,
                &manifest,
            )
            .await;
        let (lease_id, sid, generation) = match create_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(&tool.plugin_id);
                return Err(error);
            }
        };
        let session = Arc::new(ManagedSystemPluginSession {
            session_id: temporary_id,
            plugin_id: tool.plugin_id.clone(),
            plugin_generation: tool.plugin_generation,
            owner_session_id: request.owner_session_id,
            lease_id,
            sid,
            generation,
            manifest,
            operation_gate: AsyncMutex::new(()),
            close_gate: AsyncMutex::new(()),
            closing: AtomicBool::new(false),
            last_activity_unix_ms: AtomicU64::new(current_unix_millis_u64()),
        });
        let invocation_result = self
            .dispatch(Arc::clone(&session), payload, timeout_ms, false)
            .await;
        session.closing.store(true, Ordering::Release);
        let shutdown_payload = plugin_payload("shutdown", Map::new())?;
        let shutdown_result = self
            .dispatch(Arc::clone(&session), shutdown_payload, timeout_ms, true)
            .await;
        let close_result = self.close_lease(Arc::clone(&session)).await;
        if let Err(cleanup_error) = close_result {
            let diagnostic = format!(
                "Stateless System Plugin lease `{}` cleanup failed and remains retained: {cleanup_error}",
                session.lease_id
            );
            self.retain_failed_session_lease(&session, diagnostic.clone())?;
            let mut failures = Vec::new();
            if let Err(error) = invocation_result {
                failures.push(error.to_string());
            }
            if let Err(error) = shutdown_result {
                failures.push(error.to_string());
            }
            failures.push(diagnostic);
            return Err(SystemPluginRuntimeError::new(failures.join("; ")));
        }
        self.release_session_slot(&tool.plugin_id);
        match (invocation_result, shutdown_result) {
            (Ok(result), Ok(_)) => Ok(result),
            (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
            (Err(invocation_error), Err(shutdown_error)) => Err(SystemPluginRuntimeError::new(
                format!("{invocation_error}; {shutdown_error}"),
            )),
        }
    }

    /// Invokes one enabled declarative Lua hook through a persistent plugin-level lease.
    /// 通过持久插件级租约调用一个已启用的声明式 Lua hook。
    ///
    /// # Parameters
    /// 参数
    /// `hook` is the exact catalog descriptor and `invocation` is the existing host hook frame.
    /// `hook` 是精确目录描述，`invocation` 是现有宿主 Hook 帧。
    ///
    /// # Returns
    /// 返回值
    /// Returns the existing host `HookResult` decoded from the Lua handler response.
    /// 返回从 Lua 处理器响应解码出的现有宿主 `HookResult`。
    pub async fn invoke_hook(
        &self,
        hook: &SystemPluginHookDescriptor,
        invocation: HookInvocation,
    ) -> Result<HookResult, SystemPluginRuntimeError> {
        self.ensure_current_hook_generation(hook)?;
        let runtime = self.ensure_hook_runtime(hook).await?;
        let _operation = runtime.operation_gate.lock().await;
        if runtime.closing.load(Ordering::Acquire) {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` hook runtime is closing.",
                hook.plugin_id
            )));
        }
        self.ensure_current_hook_generation(hook)?;
        if runtime.plugin_generation != hook.plugin_generation {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` hook runtime belongs to a stale lifecycle generation.",
                hook.plugin_id
            )));
        }
        let invocation_json = serde_json::to_string(&json!({
            "registration": invocation.registration,
            "context": invocation.context,
            "payload": invocation.payload,
        }))
        .map_err(|error| {
            SystemPluginRuntimeError::new(format!(
                "Failed to encode System Plugin hook invocation: {error}"
            ))
        })?;
        let function = hook.manifest.function.clone();
        let module = runtime.manifest.runtime.module.clone();
        let code = format!("local plugin = require('{module}')\nreturn plugin.{function}(args)");
        let lease_id = runtime.lease_id.clone();
        let generation = runtime.generation;
        let host_timeout_ms = hook
            .manifest
            .timeout_ms
            .unwrap_or(SYSTEM_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS);
        let engine = Arc::clone(&self.state.engine);
        let sid = runtime.sid.clone();
        let result = call_engine_blocking(engine, move |engine| {
            let raw = engine
                .eval_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id,
                        "sid": sid,
                        "generation": generation,
                        "code": code,
                        "args": {
                            "invocation_json": invocation_json,
                            "host_timeout_ms": host_timeout_ms,
                        },
                        "timeout_ms": host_timeout_ms,
                    })
                    .to_string(),
                )
                .map_err(SystemPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "evaluate System Plugin hook lease")?;
            parse_plugin_result(&response)
        })
        .await?;
        serde_json::from_value(result).map_err(|error| {
            SystemPluginRuntimeError::new(format!(
                "Failed to decode System Plugin hook result: {error}"
            ))
        })
    }

    /// Resolves or creates the persistent Lua lease for one current plugin generation.
    /// 解析或创建一个当前插件代次使用的持久 Lua 租约。
    async fn ensure_hook_runtime(
        &self,
        hook: &SystemPluginHookDescriptor,
    ) -> Result<Arc<ManagedSystemPluginHookRuntime>, SystemPluginRuntimeError> {
        let _initialization = self.state.hook_runtime_initialization_gate.lock().await;
        let existing_runtime = {
            let runtimes = lock_hook_runtimes(&self.state.hook_runtimes)?;
            runtimes.get(&hook.plugin_id).cloned()
        };
        if let Some(runtime) = existing_runtime {
            if runtime.plugin_generation == hook.plugin_generation
                && !runtime.closing.load(Ordering::Acquire)
            {
                return Ok(runtime);
            }
            if let Some(diagnostic) = self.close_hook_runtime(Arc::clone(&runtime)).await? {
                self.record_cleanup_failure(&hook.plugin_id, diagnostic)?;
            }
            lock_hook_runtimes(&self.state.hook_runtimes)?.remove(&hook.plugin_id);
        }
        self.ensure_current_hook_generation(hook)?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` is no longer available.",
                hook.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` has no valid manifest.",
                hook.plugin_id
            ))
        })?;
        let (lease_id, sid, generation) = self
            .create_managed_lease(
                "hook-runtime",
                &plugin.package_root,
                &plugin.package_root,
                &manifest,
            )
            .await?;
        let runtime = Arc::new(ManagedSystemPluginHookRuntime {
            plugin_id: hook.plugin_id.clone(),
            plugin_generation: hook.plugin_generation,
            lease_id,
            sid,
            generation,
            manifest,
            operation_gate: AsyncMutex::new(()),
            closing: AtomicBool::new(false),
        });
        lock_hook_runtimes(&self.state.hook_runtimes)?
            .insert(hook.plugin_id.clone(), Arc::clone(&runtime));
        Ok(runtime)
    }

    /// Stops one persistent hook runtime and always attempts to release its System lease.
    /// 停止一个持久 hook 运行时，并始终尝试释放其系统租约。
    async fn close_hook_runtime(
        &self,
        runtime: Arc<ManagedSystemPluginHookRuntime>,
    ) -> Result<Option<String>, SystemPluginRuntimeError> {
        runtime.closing.store(true, Ordering::Release);
        let _operation = runtime.operation_gate.lock().await;
        let shutdown_result = if let Some(function) = runtime
            .manifest
            .runtime
            .hook_shutdown_function
            .as_ref()
            .cloned()
        {
            let module = runtime.manifest.runtime.module.clone();
            let plugin_id = runtime.plugin_id.clone();
            let code =
                format!("local plugin = require('{module}')\nreturn plugin.{function}(args)");
            let lease_id = runtime.lease_id.clone();
            let sid = runtime.sid.clone();
            let generation = runtime.generation;
            let engine = Arc::clone(&self.state.engine);
            call_engine_blocking(engine, move |engine| {
                let raw = engine
                    .eval_system_runtime_lease_json(
                        &json!({
                            "lease_id": lease_id,
                            "sid": sid,
                            "generation": generation,
                            "code": code,
                            "args": {
                                "payload_json": json!({
                                    "event": "hook_runtime_shutdown",
                                    "plugin_id": plugin_id,
                                })
                                .to_string(),
                                "host_timeout_ms": SYSTEM_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS,
                            },
                            "timeout_ms": SYSTEM_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS,
                        })
                        .to_string(),
                    )
                    .map_err(SystemPluginRuntimeError::new)?;
                let response = parse_engine_response(&raw, "shutdown System Plugin hook lease")?;
                parse_plugin_result(&response).map(|_| ())
            })
            .await
        } else {
            Ok(())
        };
        self.close_raw_lease(&runtime.lease_id, &runtime.sid, runtime.generation)
            .await?;
        Ok(shutdown_result.err().map(|error| {
            format!(
                "Plugin `{}` hook lease closed after graceful shutdown failed: {error}",
                runtime.plugin_id
            )
        }))
    }

    /// Closes one raw LuaSkills System lease by exact id and generation.
    /// 按精确标识与代次关闭一个原始 LuaSkills 系统租约。
    async fn close_raw_lease(
        &self,
        lease_id: &str,
        sid: &str,
        generation: u64,
    ) -> Result<(), SystemPluginRuntimeError> {
        let lease_id = lease_id.to_owned();
        let sid = sid.to_owned();
        let engine = Arc::clone(&self.state.engine);
        call_engine_blocking(engine, move |engine| {
            let raw = engine
                .close_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id,
                        "sid": sid,
                        "generation": generation,
                    })
                    .to_string(),
                )
                .map_err(SystemPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close System Plugin hook lease").map(|_| ())
        })
        .await
    }

    /// Schedules idle cleanup without retaining the plugin manager past host shutdown.
    /// 调度空闲清理，同时不让插件管理器越过宿主关闭继续存活。
    fn schedule_session_idle_reaper(&self, session: Arc<ManagedSystemPluginSession>) {
        let Some(timeout_sec) = session.manifest.runtime.session_idle_timeout_sec else {
            return;
        };
        let timeout_ms = timeout_sec.saturating_mul(1000);
        let weak_state = Arc::downgrade(&self.state);
        tokio::spawn(async move {
            loop {
                let last_activity = session.last_activity_unix_ms.load(Ordering::Acquire);
                let elapsed = current_unix_millis_u64().saturating_sub(last_activity);
                if elapsed < timeout_ms {
                    tokio::time::sleep(Duration::from_millis(
                        timeout_ms.saturating_sub(elapsed).max(1),
                    ))
                    .await;
                    continue;
                }
                let Some(state) = weak_state.upgrade() else {
                    return;
                };
                let manager = SystemPluginRuntimeManager { state };
                let is_current = lock_sessions(&manager.state.sessions)
                    .ok()
                    .and_then(|sessions| sessions.get(&session.session_id).cloned())
                    .is_some_and(|current| Arc::ptr_eq(&current, &session));
                if !is_current || session.closing.load(Ordering::Acquire) {
                    return;
                }
                let refreshed_elapsed = current_unix_millis_u64()
                    .saturating_sub(session.last_activity_unix_ms.load(Ordering::Acquire));
                if refreshed_elapsed < timeout_ms {
                    continue;
                }
                match manager.drain_sessions(vec![Arc::clone(&session)]).await {
                    Ok(report) => {
                        for diagnostic in report.failures {
                            let _ = manager.record_cleanup_failure(&session.plugin_id, diagnostic);
                        }
                    }
                    Err(error) => {
                        let _ = manager.record_cleanup_failure(
                            &session.plugin_id,
                            format!(
                                "Session `{}` idle cleanup failed: {error}",
                                session.session_id
                            ),
                        );
                    }
                }
                return;
            }
        });
    }

    /// Opens and registers one new owner-bound managed session.
    /// 打开并注册一个新的所有者绑定受管会话。
    async fn open_session(
        &self,
        tool: &SystemPluginToolDescriptor,
        request: SystemPluginInvocationRequest,
    ) -> Result<Value, SystemPluginRuntimeError> {
        let arguments = require_argument_object(request.arguments)?;
        if arguments.contains_key(SYSTEM_PLUGIN_SESSION_ID_ARGUMENT) {
            return Err(SystemPluginRuntimeError::new(
                "A session-open tool must not accept the host-owned session_id argument.",
            ));
        }
        let workspace_root = request.workspace_root.canonicalize().map_err(|error| {
            SystemPluginRuntimeError::new(format!("Failed to resolve workspace root: {error}"))
        })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` is missing from the current catalog.",
                tool.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` has no valid manifest.",
                tool.plugin_id
            ))
        })?;
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        self.reserve_session_slot(&tool.plugin_id, manifest.runtime.max_active_sessions)?;
        let session_id = Uuid::new_v4().to_string();
        let create_result = self
            .create_managed_lease(
                &session_id,
                &plugin.package_root,
                &workspace_root,
                &manifest,
            )
            .await;
        let (lease_id, sid, generation) = match create_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(&tool.plugin_id);
                return Err(error);
            }
        };
        let session = Arc::new(ManagedSystemPluginSession {
            session_id: session_id.clone(),
            plugin_id: tool.plugin_id.clone(),
            plugin_generation: tool.plugin_generation,
            owner_session_id: request.owner_session_id,
            lease_id,
            sid,
            generation,
            manifest,
            operation_gate: AsyncMutex::new(()),
            close_gate: AsyncMutex::new(()),
            closing: AtomicBool::new(false),
            last_activity_unix_ms: AtomicU64::new(current_unix_millis_u64()),
        });
        let open_result = self
            .dispatch(Arc::clone(&session), payload, timeout_ms, false)
            .await;
        let result = match open_result {
            Ok(result) => result,
            Err(error) => match self.close_lease(Arc::clone(&session)).await {
                Ok(_) => {
                    self.release_session_slot(&tool.plugin_id);
                    return Err(error);
                }
                Err(cleanup_error) => {
                    let diagnostic = format!(
                        "System Plugin session `{session_id}` open failed and lease cleanup failed; the lease remains retained: {cleanup_error}"
                    );
                    self.retain_failed_session_lease(&session, diagnostic.clone())?;
                    return Err(SystemPluginRuntimeError::new(format!(
                        "{error}; {diagnostic}"
                    )));
                }
            },
        };
        session
            .last_activity_unix_ms
            .store(current_unix_millis_u64(), Ordering::Release);
        let registration = match self.state.session_registration_gate.lock() {
            Ok(_registration) => match self.ensure_session_runtime_current(&session) {
                Ok(()) => match lock_sessions(&self.state.sessions) {
                    Ok(sessions) if sessions.contains_key(&session_id) => {
                        Err(SystemPluginRuntimeError::new(
                            "Generated System Plugin session id already exists.",
                        ))
                    }
                    Ok(mut sessions) => {
                        sessions.insert(session_id.clone(), Arc::clone(&session));
                        Ok(())
                    }
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            Err(_) => Err(SystemPluginRuntimeError::new(
                "System Plugin session registration gate is poisoned.",
            )),
        };
        if let Err(error) = registration {
            match self.close_lease(Arc::clone(&session)).await {
                Ok(_) => {
                    self.release_session_slot(&tool.plugin_id);
                    return Err(error);
                }
                Err(cleanup_error) => {
                    let diagnostic = format!(
                        "System Plugin session `{session_id}` registration failed and lease cleanup failed; the lease remains retained: {cleanup_error}"
                    );
                    self.retain_failed_session_lease(&session, diagnostic.clone())?;
                    return Err(SystemPluginRuntimeError::new(format!(
                        "{error}; {diagnostic}"
                    )));
                }
            }
        }
        self.schedule_session_idle_reaper(Arc::clone(&session));
        Ok(wrap_session_result(&session_id, result, false))
    }

    /// Calls one operation through an existing owner-bound session.
    /// 通过一个已有的所有者绑定会话调用操作。
    async fn call_session(
        &self,
        tool: &SystemPluginToolDescriptor,
        request: SystemPluginInvocationRequest,
    ) -> Result<Value, SystemPluginRuntimeError> {
        let mut arguments = require_argument_object(request.arguments)?;
        let session_id = take_session_id(&mut arguments)?;
        let session =
            self.owned_session(&session_id, &request.owner_session_id, &tool.plugin_id)?;
        if let Err(stale_error) = self.ensure_session_generation(tool, &session) {
            let report = self.drain_sessions(vec![Arc::clone(&session)]).await?;
            let diagnostic = if report.failures.is_empty() {
                stale_error.to_string()
            } else {
                format!("{stale_error}; {}", report.failures.join("; "))
            };
            return Err(SystemPluginRuntimeError::new(diagnostic));
        }
        session
            .last_activity_unix_ms
            .store(current_unix_millis_u64(), Ordering::Release);
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        let result = self.dispatch(session, payload, timeout_ms, false).await?;
        Ok(wrap_session_result(&session_id, result, false))
    }

    /// Closes one existing owner-bound session and retires its managed lease.
    /// 关闭一个已有的所有者绑定会话并回收其受管租约。
    async fn close_session(
        &self,
        tool: &SystemPluginToolDescriptor,
        request: SystemPluginInvocationRequest,
    ) -> Result<Value, SystemPluginRuntimeError> {
        let mut arguments = require_argument_object(request.arguments)?;
        let session_id = take_session_id(&mut arguments)?;
        if !arguments.is_empty() {
            return Err(SystemPluginRuntimeError::new(
                "A session-close tool only accepts the host-owned session_id argument.",
            ));
        }
        let session =
            self.owned_session(&session_id, &request.owner_session_id, &tool.plugin_id)?;
        if self.ensure_session_generation(tool, &session).is_err() {
            let report = self.drain_sessions(vec![Arc::clone(&session)]).await?;
            if report.closed_sessions == 0 {
                return Err(SystemPluginRuntimeError::new(format!(
                    "Stale System Plugin session `{session_id}` could not be closed: {}",
                    report.failures.join("; ")
                )));
            }
            return Ok(wrap_session_result(
                &session_id,
                json!({
                    "closed_stale_generation": true,
                    "diagnostics": report.failures,
                }),
                true,
            ));
        }
        let _close = session.close_gate.lock().await;
        let is_registered = lock_sessions(&self.state.sessions)?
            .get(&session_id)
            .is_some_and(|current| Arc::ptr_eq(current, &session));
        if !is_registered {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin session `{session_id}` was already closed concurrently."
            )));
        }
        session
            .closing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                SystemPluginRuntimeError::new(format!(
                    "System Plugin session `{session_id}` is already closing."
                ))
            })?;
        let payload = plugin_payload(&tool.manifest.execution.operation, Map::new())?;
        let timeout_ms = resolve_host_timeout(tool, &Map::new())?;
        let shutdown_result = self
            .dispatch(Arc::clone(&session), payload, timeout_ms, true)
            .await;
        let close_result = self.close_lease(Arc::clone(&session)).await;
        if let Err(error) = close_result {
            session.closing.store(false, Ordering::Release);
            let diagnostic =
                format!("Session `{session_id}` lease cleanup failed and remains active: {error}");
            self.record_cleanup_failure(&tool.plugin_id, diagnostic.clone())?;
            return Err(SystemPluginRuntimeError::new(diagnostic));
        }
        lock_sessions(&self.state.sessions)?.remove(&session_id);
        self.release_session_slot(&tool.plugin_id);
        let result = shutdown_result.unwrap_or_else(|error| {
            json!({
                "graceful_shutdown": false,
                "diagnostic": error.message,
            })
        });
        Ok(wrap_session_result(&session_id, result, true))
    }

    /// Creates one LuaSkills managed System lease for an exact validated package.
    /// 为一个精确且通过校验的插件包创建 LuaSkills 受管系统租约。
    async fn create_managed_lease(
        &self,
        session_id: &str,
        package_root: &Path,
        workspace_root: &Path,
        manifest: &SystemPluginManifest,
    ) -> Result<(String, String, u64), SystemPluginRuntimeError> {
        let requested_sid = format!("system-plugin/{}/{session_id}", manifest.id);
        let create_request = json!({
            "sid": requested_sid.clone(),
            "ttl_sec": 0,
            "replace": false,
            "cwd": package_root.to_string_lossy(),
            "workspace_root": workspace_root.to_string_lossy(),
            "system_package": {
                "id": manifest.id,
                "root": package_root.to_string_lossy(),
                "dependencies_file": manifest.runtime.dependencies_file,
            },
        });
        let engine = Arc::clone(&self.state.engine);
        let response = call_engine_blocking(engine, move |engine| {
            let raw = engine
                .create_system_runtime_lease_json(&create_request.to_string())
                .map_err(SystemPluginRuntimeError::new)?;
            parse_engine_response(&raw, "create System Plugin runtime lease")
        })
        .await?;
        let lease_id = required_response_string(&response, "lease_id")?;
        let sid = required_response_string(&response, "sid")?;
        if sid != requested_sid {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin lease returned unexpected sid `{sid}` instead of `{requested_sid}`."
            )));
        }
        let generation = required_response_u64(&response, "generation")?;
        Ok((lease_id, sid, generation))
    }

    /// Resolves one session and enforces exact owner and plugin identity.
    /// 解析单个会话并执行精确的所有者与插件身份校验。
    fn owned_session(
        &self,
        session_id: &str,
        owner_session_id: &str,
        plugin_id: &str,
    ) -> Result<Arc<ManagedSystemPluginSession>, SystemPluginRuntimeError> {
        let session = lock_sessions(&self.state.sessions)?
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                SystemPluginRuntimeError::new(format!(
                    "System Plugin session `{session_id}` does not exist."
                ))
            })?;
        if session.owner_session_id != owner_session_id {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin session `{session_id}` belongs to another conversation."
            )));
        }
        if session.plugin_id != plugin_id {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin session `{session_id}` belongs to plugin `{}`.",
                session.plugin_id
            )));
        }
        Ok(session)
    }

    /// Requires one tool definition to match the current enabled plugin generation.
    /// 要求一个工具定义匹配当前已启用插件代次。
    fn ensure_current_tool_generation(
        &self,
        tool: &SystemPluginToolDescriptor,
    ) -> Result<(), SystemPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` is no longer available.",
                tool.plugin_id
            ))
        })?;
        if plugin.runtime_state != SystemPluginRuntimeState::Enabled
            || plugin.generation != tool.plugin_generation
        {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` changed lifecycle generation; retry with a new provider request.",
                tool.plugin_id
            )));
        }
        Ok(())
    }

    /// Requires one hook declaration to match the current enabled plugin generation.
    /// 要求一个 hook 声明匹配当前已启用插件代次。
    fn ensure_current_hook_generation(
        &self,
        hook: &SystemPluginHookDescriptor,
    ) -> Result<(), SystemPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` is no longer available.",
                hook.plugin_id
            ))
        })?;
        let current_hook = snapshot
            .enabled_hooks
            .get(&hook.registration_id)
            .ok_or_else(|| {
                SystemPluginRuntimeError::new(format!(
                    "System Plugin hook `{}` is not enabled in the current catalog.",
                    hook.registration_id
                ))
            })?;
        if plugin.runtime_state != SystemPluginRuntimeState::Enabled
            || plugin.generation != hook.plugin_generation
            || current_hook.plugin_generation != hook.plugin_generation
        {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` changed lifecycle generation; retry the hook on the current runtime.",
                hook.plugin_id
            )));
        }
        Ok(())
    }

    /// Requires one retained session to match the current tool and plugin generation.
    /// 要求一个保留会话匹配当前工具与插件代次。
    fn ensure_session_generation(
        &self,
        tool: &SystemPluginToolDescriptor,
        session: &ManagedSystemPluginSession,
    ) -> Result<(), SystemPluginRuntimeError> {
        self.ensure_current_tool_generation(tool)?;
        if session.plugin_generation != tool.plugin_generation {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin session `{}` belongs to a stale lifecycle generation.",
                session.session_id
            )));
        }
        Ok(())
    }

    /// Verifies one managed session still belongs to the currently enabled plugin generation.
    /// 验证单个受管会话是否仍属于当前已启用插件代次。
    fn ensure_session_runtime_current(
        &self,
        session: &ManagedSystemPluginSession,
    ) -> Result<(), SystemPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&session.plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{}` is no longer discovered.",
                session.plugin_id
            ))
        })?;
        if plugin.runtime_state != SystemPluginRuntimeState::Enabled
            || plugin.generation != session.plugin_generation
        {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin session `{}` belongs to a stale or inactive lifecycle generation.",
                session.session_id
            )));
        }
        Ok(())
    }

    /// Applies generations, transient lifecycle overrides, active counts, and cleanup diagnostics.
    /// 应用代次、临时生命周期覆盖、活动计数与清理诊断。
    fn project_runtime_state(
        &self,
        snapshot: &mut SystemPluginCatalogSnapshot,
    ) -> Result<(), SystemPluginRuntimeError> {
        let previous = self.snapshot();
        let active_counts = self.state.active_session_counts.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin session counter lock is poisoned.")
        })?;
        let overrides = self.state.lifecycle_overrides.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin lifecycle lock is poisoned.")
        })?;
        let cleanup_diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let mut generations = self.state.plugin_generations.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin generation lock is poisoned.")
        })?;
        for plugin in snapshot.plugins.values_mut() {
            let generation = generations.entry(plugin.id.clone()).or_insert(1);
            let changed = previous.plugins.get(&plugin.id).is_some_and(|prior| {
                prior.version != plugin.version
                    || prior.content_fingerprint != plugin.content_fingerprint
                    || prior.desired_enabled != plugin.desired_enabled
                    || prior.availability != plugin.availability
            });
            if changed {
                *generation = generation.saturating_add(1);
            }
            plugin.generation = *generation;
            plugin.active_session_count = active_counts.get(&plugin.id).copied().unwrap_or(0);
            for tool in &mut plugin.tools {
                tool.plugin_generation = *generation;
            }
            for hook in &mut plugin.hooks {
                hook.plugin_generation = *generation;
            }
            if let Some(runtime_state) = overrides.get(&plugin.id).copied() {
                plugin.runtime_state = runtime_state;
            } else if plugin.runtime_state == SystemPluginRuntimeState::Disabled
                && cleanup_diagnostics.contains_key(&plugin.id)
            {
                plugin.runtime_state = SystemPluginRuntimeState::DisabledWithCleanupWarning;
            }
            if let Some(diagnostics) = cleanup_diagnostics.get(&plugin.id) {
                plugin.diagnostics.extend(diagnostics.iter().cloned());
            }
        }
        rebuild_enabled_surface(snapshot);
        Ok(())
    }

    /// Begins one transient lifecycle state and publishes a generation-bumped snapshot.
    /// 开始一个临时生命周期状态，并发布代次递增的快照。
    fn begin_lifecycle_transition(
        &self,
        plugin_id: &str,
        runtime_state: SystemPluginRuntimeState,
    ) -> Result<(), SystemPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin session registration gate is poisoned.")
        })?;
        let mut snapshot = (*self.snapshot()).clone();
        let plugin = snapshot.plugins.get_mut(plugin_id).ok_or_else(|| {
            SystemPluginRuntimeError::new(format!(
                "System Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if !matches!(
            plugin.availability,
            SystemPluginAvailability::Enabled | SystemPluginAvailability::Disabled
        ) {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin `{plugin_id}` cannot enter a lifecycle transition while unavailable."
            )));
        }
        let generation = {
            let mut generations = self.state.plugin_generations.lock().map_err(|_| {
                SystemPluginRuntimeError::new("System Plugin generation lock is poisoned.")
            })?;
            let generation = generations.entry(plugin_id.to_owned()).or_insert(1);
            *generation = generation.saturating_add(1);
            *generation
        };
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                SystemPluginRuntimeError::new("System Plugin lifecycle lock is poisoned.")
            })?
            .insert(plugin_id.to_owned(), runtime_state);
        plugin.runtime_state = runtime_state;
        plugin.generation = generation;
        for tool in &mut plugin.tools {
            tool.plugin_generation = generation;
        }
        for hook in &mut plugin.hooks {
            hook.plugin_generation = generation;
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        rebuild_enabled_surface(&mut snapshot);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }

    /// Dispatches one exact JSON payload through the plugin Lua module.
    /// 通过插件 Lua 模块调度一个精确 JSON 载荷。
    async fn dispatch(
        &self,
        session: Arc<ManagedSystemPluginSession>,
        mut payload: Value,
        host_timeout_ms: u64,
        shutdown: bool,
    ) -> Result<Value, SystemPluginRuntimeError> {
        let _operation = session.operation_gate.lock().await;
        if session.closing.load(Ordering::Acquire) && !shutdown {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin session `{}` is closing.",
                session.session_id
            )));
        }
        if !shutdown {
            self.ensure_session_runtime_current(&session)?;
        }
        let request_id = self.state.next_request_id.fetch_add(1, Ordering::Relaxed);
        let object = payload.as_object_mut().ok_or_else(|| {
            SystemPluginRuntimeError::new("System Plugin payload must be a JSON object.")
        })?;
        object.insert(String::from("request_id"), json!(request_id.to_string()));
        object.insert(String::from("host_timeout_ms"), json!(host_timeout_ms));
        let payload_json = payload.to_string();
        let runtime = &session.manifest.runtime;
        let function = if shutdown {
            runtime.shutdown_function.as_str()
        } else {
            runtime.invoke_function.as_str()
        };
        let code = format!(
            "local plugin = require('{}')\nreturn plugin.{}(args)",
            runtime.module, function
        );
        let lease_id = session.lease_id.clone();
        let sid = session.sid.clone();
        let generation = session.generation;
        let engine = Arc::clone(&self.state.engine);
        call_engine_blocking(engine, move |engine| {
            let eval_request = json!({
                "lease_id": lease_id,
                "sid": sid,
                "generation": generation,
                "code": code,
                "args": {
                    "payload_json": payload_json,
                    "host_timeout_ms": host_timeout_ms,
                },
                "timeout_ms": host_timeout_ms,
            });
            let raw = engine
                .eval_system_runtime_lease_json(&eval_request.to_string())
                .map_err(SystemPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "evaluate System Plugin runtime lease")?;
            parse_plugin_result(&response)
        })
        .await
    }

    /// Closes one LuaSkills lease after plugin shutdown is attempted.
    /// 在尝试插件关闭后释放单个 LuaSkills 租约。
    async fn close_lease(
        &self,
        session: Arc<ManagedSystemPluginSession>,
    ) -> Result<Value, SystemPluginRuntimeError> {
        let lease_id = session.lease_id.clone();
        let sid = session.sid.clone();
        let generation = session.generation;
        let engine = Arc::clone(&self.state.engine);
        call_engine_blocking(engine, move |engine| {
            let raw = engine
                .close_system_runtime_lease_json(
                    &json!({
                        "lease_id": lease_id,
                        "sid": sid,
                        "generation": generation,
                    })
                    .to_string(),
                )
                .map_err(SystemPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close System Plugin runtime lease")
        })
        .await
    }

    /// Reserves one plugin-scoped session slot under an exact hard limit.
    /// 在精确硬上限下预留一个插件作用域会话名额。
    fn reserve_session_slot(
        &self,
        plugin_id: &str,
        maximum: usize,
    ) -> Result<(), SystemPluginRuntimeError> {
        let mut counts = self.state.active_session_counts.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin session counter lock is poisoned.")
        })?;
        let count = counts.entry(plugin_id.to_owned()).or_default();
        if *count >= maximum {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin `{plugin_id}` reached its active session limit of {maximum}."
            )));
        }
        *count += 1;
        let active_count = *count;
        drop(counts);
        if let Err(error) = self.publish_active_session_count(plugin_id, active_count) {
            self.release_session_slot(plugin_id);
            return Err(error);
        }
        Ok(())
    }

    /// Releases one previously reserved plugin-scoped session slot.
    /// 释放一个此前预留的插件作用域会话名额。
    fn release_session_slot(&self, plugin_id: &str) {
        let Ok(mut counts) = self.state.active_session_counts.lock() else {
            return;
        };
        let Some(count) = counts.get_mut(plugin_id) else {
            return;
        };
        *count = count.saturating_sub(1);
        let active_count = *count;
        if active_count == 0 {
            counts.remove(plugin_id);
        }
        drop(counts);
        let _ = self.publish_active_session_count(plugin_id, active_count);
    }

    /// Publishes one active-session count without changing plugin lifecycle generation.
    /// 发布单个活动会话计数，同时不改变插件生命周期代次。
    fn publish_active_session_count(
        &self,
        plugin_id: &str,
        active_session_count: usize,
    ) -> Result<(), SystemPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin session registration gate is poisoned.")
        })?;
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        plugin.active_session_count = active_session_count;
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            SystemPluginRuntimeError::new("System Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }
}

/// Requires one exact JSON object from provider-visible arguments.
/// 从 Provider 可见参数中要求一个精确 JSON 对象。
fn require_argument_object(
    arguments: Value,
) -> Result<Map<String, Value>, SystemPluginRuntimeError> {
    arguments.as_object().cloned().ok_or_else(|| {
        SystemPluginRuntimeError::new("System Plugin tool arguments must be a JSON object.")
    })
}

/// Removes and validates the host-owned public session identifier.
/// 移除并校验宿主自有的公开会话标识。
fn take_session_id(arguments: &mut Map<String, Value>) -> Result<String, SystemPluginRuntimeError> {
    arguments
        .remove(SYSTEM_PLUGIN_SESSION_ID_ARGUMENT)
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SystemPluginRuntimeError::new("System Plugin session_id is required."))
}

/// Builds one exact plugin-owned payload without accepting an argument-level operation override.
/// 构建一个精确的插件自有载荷，同时拒绝参数层覆盖操作名。
fn plugin_payload(
    operation: &str,
    mut arguments: Map<String, Value>,
) -> Result<Value, SystemPluginRuntimeError> {
    if let Some(reserved) = SYSTEM_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS
        .iter()
        .find(|reserved| arguments.contains_key(**reserved))
    {
        return Err(SystemPluginRuntimeError::new(format!(
            "System Plugin tool arguments must not contain host-owned field `{reserved}`."
        )));
    }
    arguments.insert(String::from("op"), Value::String(operation.to_owned()));
    Ok(Value::Object(arguments))
}

/// Resolves one bounded host timeout from the tool manifest and exact argument object.
/// 根据工具清单与精确参数对象解析一个有界宿主超时。
fn resolve_host_timeout(
    tool: &SystemPluginToolDescriptor,
    arguments: &Map<String, Value>,
) -> Result<u64, SystemPluginRuntimeError> {
    let execution = &tool.manifest.execution;
    let operation_timeout = match (&execution.timeout_argument, execution.max_timeout_ms) {
        (Some(argument), Some(maximum)) => match arguments.get(argument) {
            Some(value) => {
                let selected = value.as_u64().ok_or_else(|| {
                    SystemPluginRuntimeError::new(format!(
                        "System Plugin timeout argument `{argument}` must be an unsigned integer."
                    ))
                })?;
                if selected > maximum {
                    return Err(SystemPluginRuntimeError::new(format!(
                        "System Plugin timeout argument `{argument}` exceeds {maximum} ms."
                    )));
                }
                selected
            }
            None => execution.timeout_ms,
        },
        (None, None) => execution.timeout_ms,
        _ => {
            return Err(SystemPluginRuntimeError::new(format!(
                "System Plugin tool `{}` has an invalid timeout manifest.",
                tool.id
            )));
        }
    };
    operation_timeout
        .checked_add(execution.timeout_grace_ms)
        .ok_or_else(|| SystemPluginRuntimeError::new("System Plugin host timeout overflowed."))
}

/// Wraps one plugin-owned result with stable host session metadata.
/// 使用稳定宿主会话元数据包装一个插件自有结果。
fn wrap_session_result(session_id: &str, result: Value, closed: bool) -> Value {
    json!({
        "session_id": session_id,
        "closed": closed,
        "result": result,
    })
}

/// Rebuilds provider-visible tools and prompt contributions from enabled plugin descriptors.
/// 根据已启用插件描述重新构建 Provider 可见工具与提示词贡献。
fn rebuild_enabled_surface(snapshot: &mut SystemPluginCatalogSnapshot) {
    snapshot.enabled_tools.clear();
    snapshot.enabled_hooks.clear();
    snapshot.prompt_contributions.clear();
    for plugin in snapshot.plugins.values() {
        if plugin.availability != SystemPluginAvailability::Enabled
            || plugin.runtime_state != SystemPluginRuntimeState::Enabled
        {
            continue;
        }
        for tool in &plugin.tools {
            snapshot.enabled_tools.insert(tool.id.clone(), tool.clone());
        }
        for hook in &plugin.hooks {
            snapshot
                .enabled_hooks
                .insert(hook.registration_id.clone(), hook.clone());
        }
        snapshot
            .prompt_contributions
            .extend(plugin.prompt_contributions.iter().cloned());
    }
}

/// Runs one synchronous LuaSkills operation outside the async executor thread.
/// 在异步执行器线程之外运行一个同步 LuaSkills 操作。
async fn call_engine_blocking<T>(
    engine: Arc<Mutex<LuaEngine>>,
    operation: impl FnOnce(&LuaEngine) -> Result<T, SystemPluginRuntimeError> + Send + 'static,
) -> Result<T, SystemPluginRuntimeError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let engine = engine
            .lock()
            .map_err(|_| SystemPluginRuntimeError::new("LuaSkills engine lock is poisoned."))?;
        operation(&engine)
    })
    .await
    .map_err(|error| {
        SystemPluginRuntimeError::new(format!("System Plugin runtime task failed: {error}"))
    })?
}

/// Parses one LuaSkills JSON response and requires a successful top-level state.
/// 解析一条 LuaSkills JSON 响应并要求成功的顶层状态。
fn parse_engine_response(raw: &str, operation: &str) -> Result<Value, SystemPluginRuntimeError> {
    let response: Value = serde_json::from_str(raw).map_err(|error| {
        SystemPluginRuntimeError::new(format!("Failed to decode {operation} response: {error}"))
    })?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = response
            .get("error")
            .or_else(|| response.get("message"))
            .map(render_error_value)
            .unwrap_or_else(|| response.to_string());
        return Err(SystemPluginRuntimeError::new(format!(
            "{operation} failed: {message}"
        )));
    }
    Ok(response)
}

/// Decodes one exact plugin JSON frame nested in a System lease response.
/// 解码嵌套在系统租约响应中的精确插件 JSON 帧。
fn parse_plugin_result(response: &Value) -> Result<Value, SystemPluginRuntimeError> {
    let frame = response
        .get("result")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            SystemPluginRuntimeError::new("System Plugin runtime eval returned no JSON frame.")
        })?;
    let plugin: Value = serde_json::from_str(frame).map_err(|error| {
        SystemPluginRuntimeError::new(format!(
            "Failed to decode System Plugin JSON frame: {error}"
        ))
    })?;
    if plugin.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = plugin
            .get("error")
            .map(render_error_value)
            .unwrap_or_else(|| String::from("Unknown System Plugin runtime error."));
        return Err(SystemPluginRuntimeError::new(format!(
            "System Plugin request failed: {message}"
        )));
    }
    Ok(plugin.get("result").cloned().unwrap_or(Value::Null))
}

/// Renders one structured or scalar runtime error without losing upstream detail.
/// 渲染一个结构化或标量运行时错误，同时不丢失上游细节。
fn render_error_value(error: &Value) -> String {
    error
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| error.to_string())
}

/// Requires one non-empty string field from a successful engine response.
/// 从成功引擎响应中要求一个非空字符串字段。
fn required_response_string(
    response: &Value,
    field: &str,
) -> Result<String, SystemPluginRuntimeError> {
    response
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            SystemPluginRuntimeError::new(format!("System lease response omitted {field}."))
        })
}

/// Requires one unsigned integer field from a successful engine response.
/// 从成功引擎响应中要求一个无符号整数字段。
fn required_response_u64(response: &Value, field: &str) -> Result<u64, SystemPluginRuntimeError> {
    response.get(field).and_then(Value::as_u64).ok_or_else(|| {
        SystemPluginRuntimeError::new(format!("System lease response omitted {field}."))
    })
}

/// Acquires the shared active-session registry.
/// 获取共享活动会话注册表。
fn lock_sessions(
    sessions: &Mutex<BTreeMap<String, Arc<ManagedSystemPluginSession>>>,
) -> Result<
    MutexGuard<'_, BTreeMap<String, Arc<ManagedSystemPluginSession>>>,
    SystemPluginRuntimeError,
> {
    sessions.lock().map_err(|_| {
        SystemPluginRuntimeError::new("System Plugin session registry lock is poisoned.")
    })
}

/// Acquires the shared persistent hook-runtime registry.
/// 获取共享持久 hook 运行时注册表。
fn lock_hook_runtimes(
    runtimes: &Mutex<BTreeMap<String, Arc<ManagedSystemPluginHookRuntime>>>,
) -> Result<
    MutexGuard<'_, BTreeMap<String, Arc<ManagedSystemPluginHookRuntime>>>,
    SystemPluginRuntimeError,
> {
    runtimes.lock().map_err(|_| {
        SystemPluginRuntimeError::new("System Plugin hook runtime registry lock is poisoned.")
    })
}

/// Locks the retained raw-lease registry without silently recovering poisoned state.
/// 锁定保留原始租约注册表，不静默恢复已中毒状态。
fn lock_retained_leases(
    leases: &Mutex<BTreeMap<String, RetainedSystemPluginLease>>,
) -> Result<MutexGuard<'_, BTreeMap<String, RetainedSystemPluginLease>>, SystemPluginRuntimeError> {
    leases.lock().map_err(|_| {
        SystemPluginRuntimeError::new("System Plugin retained lease registry lock is poisoned.")
    })
}

/// Closes one LuaSkills System lease synchronously during manager drop.
/// 在管理器释放期间同步关闭一个 LuaSkills 系统租约。
fn close_system_lease_sync(
    engine: &LuaEngine,
    session: &ManagedSystemPluginSession,
) -> Result<Value, SystemPluginRuntimeError> {
    let raw = engine
        .close_system_runtime_lease_json(
            &json!({
                "lease_id": session.lease_id,
                "sid": session.sid,
                "generation": session.generation,
            })
            .to_string(),
        )
        .map_err(SystemPluginRuntimeError::new)?;
    parse_engine_response(&raw, "close System Plugin runtime lease")
}

/// Closes one raw LuaSkills System lease synchronously during manager drop.
/// 在管理器释放期间同步关闭一个原始 LuaSkills 系统租约。
fn close_raw_system_lease_sync(
    engine: &LuaEngine,
    lease_id: &str,
    sid: &str,
    generation: u64,
) -> Result<Value, SystemPluginRuntimeError> {
    let raw = engine
        .close_system_runtime_lease_json(
            &json!({
                "lease_id": lease_id,
                "sid": sid,
                "generation": generation,
            })
            .to_string(),
        )
        .map_err(SystemPluginRuntimeError::new)?;
    parse_engine_response(&raw, "close System Plugin hook lease")
}

/// Returns a saturating Unix timestamp in milliseconds for idle-session accounting.
/// 返回用于空闲会话计时的饱和 Unix 毫秒时间戳。
fn current_unix_millis_u64() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use luaskills::{LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    use std::fs;
    use vulcan_hooks::{
        HookActor, HookCapabilitySet, HookContext, HookMode, HookPayload, HookRegistration,
        HookScope,
    };

    /// Writes one hook-only package whose Lua module retains an invocation counter.
    /// 写入一个仅含 hook 的插件包，其 Lua 模块会保留调用计数器。
    fn write_hook_package(root: &Path) {
        let package = root.join("hook-plugin");
        fs::create_dir_all(package.join("runtime")).expect("create hook runtime directory");
        fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
        fs::write(
            package.join("runtime/plugin.lua"),
            r#"local M = {}
local count = 0
function M.dispatch()
    return '{"ok":true,"result":{}}'
end
function M.shutdown()
    return '{"ok":true,"result":{}}'
end
function M.on_precheck(_args)
    count = count + 1
    return '{"ok":true,"result":{"payload":{"value":{"count":' .. tostring(count) .. '}}}}'
end
function M.shutdown_hooks(_args)
    return '{"ok":true,"result":{}}'
end
return M
"#,
        )
        .expect("write hook runtime module");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: hook-plugin
version: 1.0.0
kind: bundled_system
default_enabled: true
display:
  name: Hook Plugin
  capability: Persistent hook fixture
runtime:
  dependencies_file: dependencies.yaml
  module: runtime.plugin
  invoke_function: dispatch
  shutdown_function: shutdown
  hook_shutdown_function: shutdown_hooks
  max_active_sessions: 1
hooks:
  - id: precheck
    point: conversation.turn.precheck
    mode: transform
    function: on_precheck
    timeout_ms: 1000
    failure_policy: stop
"#,
        )
        .expect("write hook manifest");
    }

    /// Writes one minimal session plugin with a one-second idle timeout.
    /// 写入一个空闲超时为一秒的最小会话插件。
    fn write_idle_session_package(root: &Path) {
        let package = root.join("idle-plugin");
        fs::create_dir_all(package.join("runtime")).expect("create idle runtime directory");
        fs::create_dir_all(package.join("schemas")).expect("create idle schema directory");
        fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
        fs::write(
            package.join("runtime/plugin.lua"),
            r#"local M = {}
function M.dispatch(_args) return '{"ok":true,"result":{"accepted":true}}' end
function M.shutdown(_args) return '{"ok":true,"result":{"closed":true}}' end
return M
"#,
        )
        .expect("write idle runtime module");
        fs::write(
            package.join("schemas/open.json"),
            r#"{"type":"object","properties":{}}"#,
        )
        .expect("write open schema");
        fs::write(
            package.join("schemas/close.json"),
            r#"{"type":"object","properties":{"session_id":{"type":"string"}},"required":["session_id"],"additionalProperties":false}"#,
        )
        .expect("write close schema");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: idle-plugin
version: 1.0.0
kind: bundled_system
default_enabled: true
display:
  name: Idle Plugin
  capability: Idle cleanup fixture
runtime:
  dependencies_file: dependencies.yaml
  module: runtime.plugin
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 1
  session_idle_timeout_sec: 1
tools:
  - id: idle_ping
    group: idle_tools
    description: Run one stateless idle test operation.
    parameters_file: schemas/open.json
    execution:
      kind: stateless
      operation: ping
      timeout_ms: 1000
  - id: idle_open
    group: idle_tools
    description: Open one idle test session.
    parameters_file: schemas/open.json
    execution:
      kind: session_open
      operation: open
      timeout_ms: 1000
  - id: idle_close
    group: idle_tools
    description: Close one idle test session.
    parameters_file: schemas/close.json
    execution:
      kind: session_close
      operation: close
      timeout_ms: 1000
"#,
        )
        .expect("write idle manifest");
    }

    /// Builds one minimal real LuaSkills engine for System lease integration tests.
    /// 为系统租约集成测试构建一个最小真实 LuaSkills 引擎。
    fn test_engine(runtime_root: &Path) -> LuaEngine {
        let host_options = LuaRuntimeHostOptions::with_runtime_root(
            runtime_root
                .canonicalize()
                .expect("canonicalize LuaSkills runtime root"),
        );
        LuaEngine::new(LuaEngineOptions::new(
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            host_options,
        ))
        .expect("create LuaSkills test engine")
    }

    /// Builds one exact host invocation for the fixture hook.
    /// 为夹具 hook 构建一次精确宿主调用。
    fn hook_invocation(registration_id: &str) -> HookInvocation {
        HookInvocation {
            registration: HookRegistration::new(
                registration_id,
                "conversation.turn.precheck",
                HookMode::Transform,
            ),
            context: HookContext {
                run_id: Uuid::new_v4().to_string(),
                point: String::from("conversation.turn.precheck"),
                scope: HookScope::Session,
                actor: HookActor::new("runtime-test"),
                workspace_id: None,
                project_id: None,
                session_id: Some(String::from("session-1")),
                conversation_id: Some(String::from("session-1")),
                user_id: None,
                capabilities: HookCapabilitySet::default(),
                metadata: BTreeMap::new(),
            },
            payload: HookPayload::empty_object(),
        }
    }

    /// Verifies hook calls reuse persistent Lua state and draining rejects stale generations.
    /// 验证 hook 调用会复用持久 Lua 状态，且 draining 会拒绝陈旧代次。
    #[test]
    fn hook_runtime_reuses_state_and_drains_by_plugin_generation() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        write_hook_package(&system_plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root,
        );
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load hook catalog");
        let hook = catalog
            .enabled_hooks
            .values()
            .next()
            .cloned()
            .expect("resolve enabled hook");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("create Tokio test runtime");
        let first = runtime
            .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
            .expect("invoke first hook");
        let second = runtime
            .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
            .expect("invoke second hook");
        assert_eq!(first.payload.expect("first payload").value["count"], 1);
        assert_eq!(second.payload.expect("second payload").value["count"], 2);
        manager
            .begin_draining("hook-plugin")
            .expect("begin plugin draining");
        let stale_error = runtime
            .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
            .expect_err("stale hook must be rejected");
        assert!(stale_error.message.contains("not enabled"));
        let report = runtime
            .block_on(manager.drain_plugin_sessions("hook-plugin"))
            .expect("drain hook runtime");
        assert_eq!(report.closed_hook_runtimes, 1);
        assert!(report.failures.is_empty());
    }

    /// Verifies the host reaps an idle owner-bound session without another tool invocation.
    /// 验证宿主无需额外工具调用即可回收空闲的所有者绑定会话。
    #[test]
    fn idle_session_timeout_reaps_managed_lease() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        write_idle_session_package(&system_plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root,
        );
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        let stateless = runtime
            .block_on(manager.invoke(SystemPluginInvocationRequest {
                tool_id: String::from("idle_ping"),
                arguments: json!({}),
                owner_session_id: String::from("owner-1"),
                workspace_root: temp.path().to_path_buf(),
            }))
            .expect("invoke stateless tool");
        assert_eq!(stateless["accepted"], true);
        assert_eq!(
            manager.snapshot().plugins["idle-plugin"].active_session_count,
            0
        );
        runtime
            .block_on(manager.invoke(SystemPluginInvocationRequest {
                tool_id: String::from("idle_open"),
                arguments: json!({}),
                owner_session_id: String::from("owner-1"),
                workspace_root: temp.path().to_path_buf(),
            }))
            .expect("open idle session");
        assert_eq!(
            manager.snapshot().plugins["idle-plugin"].active_session_count,
            1
        );
        runtime.block_on(async {
            tokio::time::sleep(Duration::from_millis(1_500)).await;
        });
        assert_eq!(
            manager.snapshot().plugins["idle-plugin"].active_session_count,
            0
        );
    }

    /// Verifies a current close tool retires an owned session from an older package generation.
    /// 验证当前关闭工具可以回收属于旧插件代次的所有者会话。
    #[test]
    fn close_tool_retires_stale_generation_session() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        write_idle_session_package(&system_plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root.clone(),
        );
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("create Tokio test runtime");
        let opened = runtime
            .block_on(manager.invoke(SystemPluginInvocationRequest {
                tool_id: String::from("idle_open"),
                arguments: json!({}),
                owner_session_id: String::from("owner-stale"),
                workspace_root: temp.path().to_path_buf(),
            }))
            .expect("open stale fixture session");
        let session_id = opened["session_id"]
            .as_str()
            .expect("read opened session id")
            .to_owned();
        fs::write(
            system_plugin_root.join("idle-plugin/dependencies.yaml"),
            "# generation change\n",
        )
        .expect("change package fingerprint");
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("refresh changed package");

        let closed = runtime
            .block_on(manager.invoke(SystemPluginInvocationRequest {
                tool_id: String::from("idle_close"),
                arguments: json!({ "session_id": session_id }),
                owner_session_id: String::from("owner-stale"),
                workspace_root: temp.path().to_path_buf(),
            }))
            .expect("close stale fixture session");
        assert_eq!(closed["closed"], true);
        assert_eq!(closed["result"]["closed_stale_generation"], true);
        assert_eq!(
            manager.snapshot().plugins["idle-plugin"].active_session_count,
            0
        );
    }

    /// Verifies an explicit close failure is projected as a visible cleanup diagnostic.
    /// 验证显式关闭失败会投影为可见的清理诊断。
    #[test]
    fn explicit_close_failure_publishes_cleanup_diagnostic() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        write_idle_session_package(&system_plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root,
        );
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        let opened = runtime
            .block_on(manager.invoke(SystemPluginInvocationRequest {
                tool_id: String::from("idle_open"),
                arguments: json!({}),
                owner_session_id: String::from("owner-close-failure"),
                workspace_root: temp.path().to_path_buf(),
            }))
            .expect("open close-failure fixture session");
        let session_id = opened["session_id"]
            .as_str()
            .expect("read opened session id")
            .to_owned();
        let session = lock_sessions(&manager.state.sessions)
            .expect("lock close-failure sessions")
            .get(&session_id)
            .cloned()
            .expect("resolve close-failure session");
        runtime
            .block_on(manager.close_raw_lease(&session.lease_id, &session.sid, session.generation))
            .expect("close fixture lease before explicit close");

        let error = runtime
            .block_on(manager.invoke(SystemPluginInvocationRequest {
                tool_id: String::from("idle_close"),
                arguments: json!({ "session_id": session_id }),
                owner_session_id: String::from("owner-close-failure"),
                workspace_root: temp.path().to_path_buf(),
            }))
            .expect_err("reject explicit close after lease disappeared");
        assert!(
            error
                .message
                .contains("lease cleanup failed and remains active")
        );
        let plugin = &manager.snapshot().plugins["idle-plugin"];
        assert_eq!(plugin.active_session_count, 1);
        assert!(
            plugin.diagnostics.iter().any(|diagnostic| {
                diagnostic.contains("lease cleanup failed and remains active")
            })
        );

        lock_sessions(&manager.state.sessions)
            .expect("lock fixture sessions for cleanup")
            .remove(&session_id);
        manager.release_session_slot("idle-plugin");
    }

    /// Verifies a competing lifecycle drain waits for the unique session close owner.
    /// 验证竞争的生命周期排空会等待唯一会话关闭所有者。
    #[test]
    fn concurrent_session_drain_waits_for_close_owner() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        write_idle_session_package(&system_plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root,
        );
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        runtime.block_on(async {
            let opened = manager
                .invoke(SystemPluginInvocationRequest {
                    tool_id: String::from("idle_open"),
                    arguments: json!({}),
                    owner_session_id: String::from("owner-race"),
                    workspace_root: temp.path().to_path_buf(),
                })
                .await
                .expect("open race fixture session");
            let session_id = opened["session_id"]
                .as_str()
                .expect("read race session id")
                .to_owned();
            let session = lock_sessions(&manager.state.sessions)
                .expect("lock race sessions")
                .get(&session_id)
                .cloned()
                .expect("resolve race session");
            assert!(!session.closing.load(Ordering::Acquire));
            manager
                .record_cleanup_failure("idle-plugin", String::from("race cleanup fixture"))
                .expect("record race cleanup diagnostic");
            let close_owner = session.close_gate.lock().await;
            let drain_manager = manager.clone();
            let drain =
                tokio::spawn(async move { drain_manager.drain_owner_sessions("owner-race").await });
            tokio::time::sleep(Duration::from_millis(25)).await;
            assert!(!drain.is_finished());
            drop(close_owner);
            let report = drain
                .await
                .expect("join competing drain")
                .expect("complete competing drain");
            let still_registered = lock_sessions(&manager.state.sessions)
                .expect("inspect race sessions")
                .contains_key(&session_id);
            assert_eq!(
                report.closed_sessions,
                1,
                "closing={} registered={still_registered}",
                session.closing.load(Ordering::Acquire)
            );
            assert!(report.failures.is_empty());
            assert!(
                !manager.snapshot().plugins["idle-plugin"]
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic == "race cleanup fixture")
            );
        });
    }

    /// Verifies lifecycle rollback cannot hide cleanup diagnostics while resources remain active.
    /// 验证生命周期回滚无法在资源仍活动时隐藏清理诊断。
    #[test]
    fn lifecycle_rollback_preserves_cleanup_diagnostic_for_active_session() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        write_idle_session_package(&system_plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root,
        );
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        runtime.block_on(async {
            manager
                .invoke(SystemPluginInvocationRequest {
                    tool_id: String::from("idle_open"),
                    arguments: json!({}),
                    owner_session_id: String::from("owner-rollback"),
                    workspace_root: temp.path().to_path_buf(),
                })
                .await
                .expect("open rollback fixture session");
            manager
                .record_cleanup_failure("idle-plugin", String::from("rollback cleanup fixture"))
                .expect("record rollback cleanup diagnostic");
            manager
                .begin_draining("idle-plugin")
                .expect("begin rollback fixture transition");
            let rolled_back = manager
                .finish_lifecycle_transition(
                    "idle-plugin",
                    &BTreeSet::new(),
                    &BTreeSet::new(),
                    Vec::new(),
                )
                .expect("finish rollback fixture transition");
            assert!(
                rolled_back.plugins["idle-plugin"]
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic == "rollback cleanup fixture")
            );

            let report = manager
                .drain_owner_sessions("owner-rollback")
                .await
                .expect("drain rollback fixture session");
            assert_eq!(report.closed_sessions, 1);
            assert!(
                !manager.snapshot().plugins["idle-plugin"]
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic == "rollback cleanup fixture")
            );
        });
    }

    /// Verifies same-version package edits advance the lifecycle generation on refresh.
    /// 验证同版本插件编辑会在刷新时推进生命周期代次。
    #[test]
    fn same_version_package_change_advances_plugin_generation() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        write_hook_package(&system_plugin_root);
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root.clone(),
        );
        let first = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load first catalog");
        let first_generation = first.plugins["hook-plugin"].generation;
        fs::write(
            system_plugin_root.join("hook-plugin/dependencies.yaml"),
            "{}\n# changed without version bump\n",
        )
        .expect("update package content");
        let second = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load second catalog");
        assert!(second.plugins["hook-plugin"].generation > first_generation);
    }

    /// Verifies retained lease retry ownership is atomically claimed and restored after failure.
    /// 验证保留租约的重试所有权会被原子领取，并可在失败后恢复。
    #[test]
    fn retained_lease_retry_claim_is_atomic() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let system_plugin_root = runtime_root.join("system_lua_lib");
        fs::create_dir_all(&system_plugin_root).expect("create System Plugin root");
        let manager = SystemPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(&runtime_root))),
            system_plugin_root,
        );
        manager
            .retain_failed_lease(
                RetainedSystemPluginLease {
                    plugin_id: String::from("claim-plugin"),
                    owner_session_id: Some(String::from("claim-owner")),
                    lease_id: String::from("claim-lease"),
                    sid: String::from("claim-sid"),
                    generation: 1,
                    reserved_session_slot: false,
                },
                String::from("claim fixture"),
            )
            .expect("retain fixture lease");

        let first = manager
            .take_retained_leases(|lease| lease.plugin_id == "claim-plugin")
            .expect("claim retained lease");
        let second = manager
            .take_retained_leases(|lease| lease.plugin_id == "claim-plugin")
            .expect("repeat retained lease claim");
        assert_eq!(first.len(), 1);
        assert!(second.is_empty());
        manager
            .restore_retained_lease(first[0].clone())
            .expect("restore claimed lease");
        assert_eq!(
            manager
                .take_retained_leases(|lease| lease.plugin_id == "claim-plugin")
                .expect("claim restored lease")
                .len(),
            1
        );
    }
}
