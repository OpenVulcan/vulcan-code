//! Fixed plugin host-call routing and trusted lifecycle scope validation.
//! 固定插件宿主调用路由与可信生命周期作用域校验。

use super::process::resolve_plugin_process_spec;
use super::{
    VulcanPluginLifecycleEvent, VulcanPluginManagerState, VulcanPluginRuntimeManager,
    plugin_permission_key,
};
use crate::{
    VulcanPluginHostServices, VulcanPluginPermission, VulcanPluginRuntimeState,
    VulcanPluginStorageOperation, VulcanPluginStorageRequest, VulcanPluginUiInvalidation,
    VulcanPluginVldbOperation, VulcanPluginVldbRequest,
};
use luaskills::{RuntimeHostToolAction, RuntimeHostToolRequest, set_host_tool_callback};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// Fixed LuaSkills host-tool name used to resolve one manifest-declared plugin process.
/// 用于解析单个清单声明插件进程的固定 LuaSkills 宿主工具名称。
const VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL: &str = "vulcan.plugin.process.resolve";

/// Fixed host-tool name used by a plugin bundle to read its own configuration.
/// 插件 bundle 用于读取自身配置的固定宿主工具名称。
const VULCAN_PLUGIN_CONFIG_GET_TOOL: &str = "vulcan.plugin.config.get";

/// Fixed host-tool name used by a plugin bundle to update its own configuration.
/// 插件 bundle 用于更新自身配置的固定宿主工具名称。
const VULCAN_PLUGIN_CONFIG_UPDATE_TOOL: &str = "vulcan.plugin.config.update";

/// Fixed host-tool name used to apply one ordered SQLite migration.
/// 用于应用一组有序 SQLite 迁移的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_SQLITE_MIGRATE_TOOL: &str = "vulcan.plugin.storage.sqlite.migrate";

/// Fixed host-tool name used to execute one SQLite statement.
/// 用于执行一条 SQLite 语句的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_TOOL: &str = "vulcan.plugin.storage.sqlite.execute";

/// Fixed host-tool name used to execute one SQLite statement batch.
/// 用于执行一组 SQLite 语句的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_BATCH_TOOL: &str =
    "vulcan.plugin.storage.sqlite.execute_batch";

/// Fixed host-tool name used to query isolated plugin SQLite rows.
/// 用于查询隔离插件 SQLite 行的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_SQLITE_QUERY_TOOL: &str = "vulcan.plugin.storage.sqlite.query";

/// Fixed host-tool name used to flush isolated plugin SQLite writes.
/// 用于刷新隔离插件 SQLite 写入的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_SQLITE_FLUSH_TOOL: &str = "vulcan.plugin.storage.sqlite.flush";

/// Fixed host-tool name used to create one isolated LanceDB table.
/// 用于创建一张隔离 LanceDB 表的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_LANCEDB_CREATE_TABLE_TOOL: &str =
    "vulcan.plugin.storage.lancedb.create_table";

/// Fixed host-tool name used to upsert isolated LanceDB rows.
/// 用于写入或更新隔离 LanceDB 行的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_LANCEDB_UPSERT_TOOL: &str = "vulcan.plugin.storage.lancedb.upsert";

/// Fixed host-tool name used to search one isolated LanceDB table.
/// 用于检索一张隔离 LanceDB 表的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_LANCEDB_SEARCH_TOOL: &str = "vulcan.plugin.storage.lancedb.search";

/// Fixed host-tool name used to delete isolated LanceDB rows.
/// 用于删除隔离 LanceDB 行的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_LANCEDB_DELETE_TOOL: &str = "vulcan.plugin.storage.lancedb.delete";

/// Fixed host-tool name used to drop one isolated LanceDB table.
/// 用于删除一张隔离 LanceDB 表的固定宿主工具名称。
const VULCAN_PLUGIN_STORAGE_LANCEDB_DROP_TABLE_TOOL: &str =
    "vulcan.plugin.storage.lancedb.drop_table";

/// Fixed host-tool name used to invalidate current plugin UI data.
/// 用于使当前插件界面数据失效的固定宿主工具名称。
const VULCAN_PLUGIN_UI_INVALIDATE_TOOL: &str = "vulcan.plugin.ui.invalidate";

/// Fixed host-tool name used to query one registered host SQLite binding.
/// 用于查询一个已注册宿主 SQLite 绑定的固定宿主工具名称。
const VULCAN_VLDB_SQLITE_QUERY_V1_TOOL: &str = "vulcan.vldb.sqlite.query.v1";

/// Fixed host-tool name used to execute one write against a registered host SQLite binding.
/// 用于对一个已注册宿主 SQLite 绑定执行写入的固定宿主工具名称。
const VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL: &str = "vulcan.vldb.sqlite.execute.v1";

/// Fixed host-tool name used to execute one atomic batch against a registered host SQLite binding.
/// 用于对一个已注册宿主 SQLite 绑定执行原子批次的固定宿主工具名称。
const VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL: &str = "vulcan.vldb.sqlite.execute_batch.v1";

/// Fixed host-tool name used to checkpoint one registered host SQLite binding.
/// 用于检查点化一个已注册宿主 SQLite 绑定的固定宿主工具名称。
const VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL: &str = "vulcan.vldb.sqlite.flush.v1";

/// Authenticated plugin identity paired with its validated bridge payload.
/// 经过认证的插件身份及其已校验桥接载荷。
type VulcanPluginBridgeIdentityPayload<'a> = (&'a str, &'a Map<String, Value>);

/// Process-wide manager registry used by the single LuaSkills host-tool callback.
/// 单一 LuaSkills 宿主工具回调使用的进程级管理器注册表。
static VULCAN_PLUGIN_MANAGER_REGISTRY: OnceLock<
    Mutex<BTreeMap<String, Weak<VulcanPluginManagerState>>>,
> = OnceLock::new();

/// Host-injected lease scope that binds bridge calls to one exact runtime lifecycle state.
/// 宿主注入的租约作用域，把桥接调用绑定到一个精确运行时生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VulcanPluginBridgeLeaseScope {
    /// Ordinary action, tool, or hook work accepted only while enabled.
    /// 仅在已启用状态接收的普通 Action、工具或 Hook 工作。
    Enabled,
    /// Host probe executed during the enabling transition.
    /// 在启用转换期间执行的宿主探测。
    EnablingProbe,
    /// Installation callback executed while the new package remains disabled.
    /// 新插件包保持停用时执行的安装回调。
    LifecycleInstall,
    /// Enable callback executed during the enabling transition.
    /// 在启用转换期间执行的启用回调。
    LifecycleEnable,
    /// Disable callback used to compensate while the plugin still remains in enabling state.
    /// 插件仍处于启用中状态时用于补偿的停用回调。
    LifecycleEnableRollback,
    /// Disable callback executed after new work is rejected and sessions are drained.
    /// 拒绝新任务并排空会话后执行的停用回调。
    LifecycleDisable,
    /// Uninstall callback executed for one drained disabled user package.
    /// 针对已排空且停用的用户插件包执行的卸载回调。
    LifecycleUninstall,
}

impl VulcanPluginBridgeLeaseScope {
    /// Returns the immutable wire value injected into the fixed Lua bridge.
    /// 返回注入固定 Lua 桥接层的不可变线协议值。
    ///
    /// # Returns
    /// 返回值
    /// Returns one stable ASCII scope identifier.
    /// 返回一个稳定的 ASCII 作用域标识。
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::EnablingProbe => "enabling_probe",
            Self::LifecycleInstall => "lifecycle_install",
            Self::LifecycleEnable => "lifecycle_enable",
            Self::LifecycleEnableRollback => "lifecycle_enable_rollback",
            Self::LifecycleDisable => "lifecycle_disable",
            Self::LifecycleUninstall => "lifecycle_uninstall",
        }
    }

    /// Parses one host-injected wire value without accepting aliases or fallbacks.
    /// 解析一个宿主注入的线协议值，不接受别名或回退值。
    ///
    /// # Parameters
    /// 参数
    /// `value` is the exact bridge scope received from the fixed Lua environment.
    /// `value` 是从固定 Lua 环境收到的精确桥接作用域。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact scope or an explicit protocol error.
    /// 返回精确作用域或明确的协议错误。
    pub(super) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "enabled" => Ok(Self::Enabled),
            "enabling_probe" => Ok(Self::EnablingProbe),
            "lifecycle_install" => Ok(Self::LifecycleInstall),
            "lifecycle_enable" => Ok(Self::LifecycleEnable),
            "lifecycle_enable_rollback" => Ok(Self::LifecycleEnableRollback),
            "lifecycle_disable" => Ok(Self::LifecycleDisable),
            "lifecycle_uninstall" => Ok(Self::LifecycleUninstall),
            _ => Err(format!("Unsupported Vulcan plugin bridge scope `{value}`.")),
        }
    }

    /// Returns the only runtime state authorized for this lease scope.
    /// 返回此租约作用域唯一授权的运行时状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact lifecycle state required by the host transition protocol.
    /// 返回宿主转换协议要求的精确生命周期状态。
    pub(super) fn required_runtime_state(self) -> VulcanPluginRuntimeState {
        match self {
            Self::Enabled => VulcanPluginRuntimeState::Enabled,
            Self::EnablingProbe | Self::LifecycleEnable | Self::LifecycleEnableRollback => {
                VulcanPluginRuntimeState::Enabling
            }
            Self::LifecycleInstall | Self::LifecycleUninstall => VulcanPluginRuntimeState::Disabled,
            Self::LifecycleDisable => VulcanPluginRuntimeState::Draining,
        }
    }
}

impl From<VulcanPluginLifecycleEvent> for VulcanPluginBridgeLeaseScope {
    /// Maps a public lifecycle event to its exact host bridge scope.
    /// 把公开生命周期事件映射到精确的宿主桥接作用域。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the lifecycle event selected by the host service.
    /// `event` 是宿主服务选择的生命周期事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns the corresponding immutable lifecycle lease scope.
    /// 返回对应的不可变生命周期租约作用域。
    fn from(event: VulcanPluginLifecycleEvent) -> Self {
        match event {
            VulcanPluginLifecycleEvent::Install => Self::LifecycleInstall,
            VulcanPluginLifecycleEvent::Enable => Self::LifecycleEnable,
            VulcanPluginLifecycleEvent::EnableRollback => Self::LifecycleEnableRollback,
            VulcanPluginLifecycleEvent::Disable => Self::LifecycleDisable,
            VulcanPluginLifecycleEvent::Uninstall => Self::LifecycleUninstall,
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Installs the current application-owned services used by authorized plugin bridge calls.
    /// 安装当前应用持有且供已授权插件桥接调用使用的服务。
    ///
    /// # Parameters
    /// 参数
    /// `services` contains optional storage, host VLDB, and UI event implementations.
    /// `services` 包含可选的存储、宿主VLDB与界面事件实现。
    ///
    /// # Returns
    /// 返回值
    /// Returns unit after atomic publication or a lock diagnostic.
    /// 原子发布后返回空值，否则返回锁诊断。
    pub fn install_host_services(&self, services: VulcanPluginHostServices) -> Result<(), String> {
        // hostServicesGuard serializes publication without holding the lock during service calls.
        // hostServicesGuard 负责串行发布，服务调用期间不会持有此锁。
        let mut host_services_guard = self
            .state
            .host_services
            .write()
            .map_err(|_| "Vulcan plugin host services lock is poisoned.".to_string())?;
        *host_services_guard = services;
        Ok(())
    }

    /// Installs the process-wide LuaSkills host-tool callback for the fixed plugin bridge.
    /// 为固定插件桥安装进程级 LuaSkills 宿主工具回调。
    ///
    /// The callback captures a weak manager state so LuaSkills cannot extend manager lifetime.
    /// 回调捕获弱管理器状态，避免 LuaSkills 延长管理器生命周期。
    ///
    /// Returns no value.
    /// 不返回值。
    pub(super) fn install_host_tool_bridge(&self) {
        let registry = VULCAN_PLUGIN_MANAGER_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()));
        if let Ok(mut managers) = registry.lock() {
            managers.retain(|_, manager| manager.strong_count() > 0);
            managers.insert(self.state.manager_id.clone(), Arc::downgrade(&self.state));
        }
        set_host_tool_callback(Some(Arc::new(dispatch_registered_plugin_host_tool_request)));
    }
}

/// Routes one process-wide LuaSkills callback to the exact manager injected by the fixed bridge.
/// 把一次进程级 LuaSkills 回调路由到固定桥接层注入的精确管理器。
///
/// # Parameters
/// 参数
/// `request` is the exact LuaSkills host-tool request.
/// `request` 是精确的 LuaSkills 宿主工具请求。
///
/// # Returns
/// 返回值
/// Returns generic discovery data or the value produced by one registered manager.
/// 返回通用发现数据或单个已注册管理器产生的值。
fn dispatch_registered_plugin_host_tool_request(
    request: &RuntimeHostToolRequest,
) -> Result<Value, String> {
    let _timing = super::action_status::HostCallTimer::start();
    if request.action != RuntimeHostToolAction::Call {
        return dispatch_plugin_host_tool_request(&Weak::new(), request);
    }
    let runtime_id = request
        .args
        .as_object()
        .and_then(|args| args.get("runtime_id"))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Vulcan plugin host request requires runtime_id.".to_string())?;
    let registry = VULCAN_PLUGIN_MANAGER_REGISTRY
        .get()
        .ok_or_else(|| "Vulcan plugin manager registry is unavailable.".to_string())?;
    let weak_state = registry
        .lock()
        .map_err(|_| "Vulcan plugin manager registry lock is poisoned.".to_string())?
        .get(runtime_id)
        .cloned()
        .ok_or_else(|| format!("Vulcan plugin runtime `{runtime_id}` is not registered."))?;
    dispatch_plugin_host_tool_request(&weak_state, request)
}

/// Dispatches one LuaSkills host-tool request issued through the fixed Vulcan plugin bridge.
/// 分发一条通过固定 Vulcan 插件桥发出的 LuaSkills 宿主工具请求。
///
/// # Parameters
/// 参数
/// `weak_state` identifies the active plugin manager and `request` is the exact LuaSkills request.
/// `weak_state` 标识活动插件管理器，`request` 是精确的 LuaSkills 请求。
///
/// # Returns
/// 返回值
/// Returns the supported tool catalog, an existence flag, or one validated process specification.
/// 返回受支持工具目录、存在性标志或一份已校验的进程规范。
fn dispatch_plugin_host_tool_request(
    weak_state: &Weak<VulcanPluginManagerState>,
    request: &RuntimeHostToolRequest,
) -> Result<Value, String> {
    match request.action {
        RuntimeHostToolAction::List => Ok(json!([
            VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL,
            VULCAN_PLUGIN_CONFIG_GET_TOOL,
            VULCAN_PLUGIN_CONFIG_UPDATE_TOOL,
            VULCAN_PLUGIN_STORAGE_SQLITE_MIGRATE_TOOL,
            VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_TOOL,
            VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_BATCH_TOOL,
            VULCAN_PLUGIN_STORAGE_SQLITE_QUERY_TOOL,
            VULCAN_PLUGIN_STORAGE_SQLITE_FLUSH_TOOL,
            VULCAN_PLUGIN_STORAGE_LANCEDB_CREATE_TABLE_TOOL,
            VULCAN_PLUGIN_STORAGE_LANCEDB_UPSERT_TOOL,
            VULCAN_PLUGIN_STORAGE_LANCEDB_SEARCH_TOOL,
            VULCAN_PLUGIN_STORAGE_LANCEDB_DELETE_TOOL,
            VULCAN_PLUGIN_STORAGE_LANCEDB_DROP_TABLE_TOOL,
            VULCAN_VLDB_SQLITE_QUERY_V1_TOOL,
            VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL,
            VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL,
            VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL,
            VULCAN_PLUGIN_UI_INVALIDATE_TOOL,
        ])),
        RuntimeHostToolAction::Has => Ok(json!(matches!(
            request.tool_name.as_deref(),
            Some(VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL)
                | Some(VULCAN_PLUGIN_CONFIG_GET_TOOL)
                | Some(VULCAN_PLUGIN_CONFIG_UPDATE_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_SQLITE_MIGRATE_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_BATCH_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_SQLITE_QUERY_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_SQLITE_FLUSH_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_LANCEDB_CREATE_TABLE_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_LANCEDB_UPSERT_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_LANCEDB_SEARCH_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_LANCEDB_DELETE_TOOL)
                | Some(VULCAN_PLUGIN_STORAGE_LANCEDB_DROP_TABLE_TOOL)
                | Some(VULCAN_VLDB_SQLITE_QUERY_V1_TOOL)
                | Some(VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL)
                | Some(VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL)
                | Some(VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL)
                | Some(VULCAN_PLUGIN_UI_INVALIDATE_TOOL)
        ))),
        RuntimeHostToolAction::Call => {
            let state = weak_state
                .upgrade()
                .ok_or_else(|| "Vulcan plugin runtime is no longer available.".to_string())?;
            // value is always wrapped because LuaSkills treats an object callback result as its own envelope.
            // value 始终需要包裹，因为 LuaSkills 会把对象型回调结果视为其自身信封。
            let value = match request.tool_name.as_deref() {
                Some(VULCAN_PLUGIN_PROCESS_RESOLVE_TOOL) => {
                    resolve_plugin_process_spec(&state, &request.args)
                }
                Some(VULCAN_PLUGIN_CONFIG_GET_TOOL) => {
                    let plugin_id = bridge_request_plugin_id(&request.args)?;
                    let lease_scope = bridge_request_lease_scope(&request.args)?;
                    ensure_plugin_capability(
                        &state,
                        plugin_id,
                        VulcanPluginPermission::Config,
                        lease_scope,
                    )?;
                    VulcanPluginRuntimeManager { state }
                        .plugin_config(plugin_id)
                        .map_err(|error| error.message)
                }
                Some(VULCAN_PLUGIN_CONFIG_UPDATE_TOOL) => {
                    let (plugin_id, payload) = bridge_request_identity_and_payload(&request.args)?;
                    let lease_scope = bridge_request_lease_scope(&request.args)?;
                    ensure_plugin_capability(
                        &state,
                        plugin_id,
                        VulcanPluginPermission::Config,
                        lease_scope,
                    )?;
                    let value = payload.get("value").cloned().ok_or_else(|| {
                        "Vulcan plugin configuration update requires field `value`.".to_string()
                    })?;
                    VulcanPluginRuntimeManager { state }
                        .update_plugin_config(plugin_id, value)
                        .map_err(|error| error.message)
                }
                Some(tool_name) if plugin_storage_operation(tool_name).is_some() => {
                    dispatch_plugin_storage_request(&state, tool_name, &request.args)
                }
                Some(tool_name) if plugin_vldb_operation(tool_name).is_some() => {
                    dispatch_plugin_vldb_request(&state, tool_name, &request.args)
                }
                Some(VULCAN_PLUGIN_UI_INVALIDATE_TOOL) => {
                    dispatch_plugin_ui_invalidation(&state, &request.args)
                }
                _ => Err(format!(
                    "Unsupported Vulcan plugin host tool `{}`.",
                    request.tool_name.as_deref().unwrap_or("<missing>")
                )),
            }?;
            Ok(json!({
                "ok": true,
                "value": value,
            }))
        }
    }
}

/// Maps one fixed storage host-tool name to its exact operation.
/// 将一个固定存储宿主工具名称映射到精确操作。
///
/// # Parameters
/// 参数
/// `tool_name` is the untrusted requested host-tool name.
/// `tool_name` 是不受信任的请求宿主工具名称。
///
/// # Returns
/// 返回值
/// Returns the exact operation for a registered storage tool.
/// 对已注册存储工具返回精确操作。
fn plugin_storage_operation(tool_name: &str) -> Option<VulcanPluginStorageOperation> {
    match tool_name {
        VULCAN_PLUGIN_STORAGE_SQLITE_MIGRATE_TOOL => {
            Some(VulcanPluginStorageOperation::SqliteMigrate)
        }
        VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_TOOL => {
            Some(VulcanPluginStorageOperation::SqliteExecute)
        }
        VULCAN_PLUGIN_STORAGE_SQLITE_EXECUTE_BATCH_TOOL => {
            Some(VulcanPluginStorageOperation::SqliteExecuteBatch)
        }
        VULCAN_PLUGIN_STORAGE_SQLITE_QUERY_TOOL => Some(VulcanPluginStorageOperation::SqliteQuery),
        VULCAN_PLUGIN_STORAGE_SQLITE_FLUSH_TOOL => Some(VulcanPluginStorageOperation::SqliteFlush),
        VULCAN_PLUGIN_STORAGE_LANCEDB_CREATE_TABLE_TOOL => {
            Some(VulcanPluginStorageOperation::LanceDbCreateTable)
        }
        VULCAN_PLUGIN_STORAGE_LANCEDB_UPSERT_TOOL => {
            Some(VulcanPluginStorageOperation::LanceDbUpsert)
        }
        VULCAN_PLUGIN_STORAGE_LANCEDB_SEARCH_TOOL => {
            Some(VulcanPluginStorageOperation::LanceDbSearch)
        }
        VULCAN_PLUGIN_STORAGE_LANCEDB_DELETE_TOOL => {
            Some(VulcanPluginStorageOperation::LanceDbDelete)
        }
        VULCAN_PLUGIN_STORAGE_LANCEDB_DROP_TABLE_TOOL => {
            Some(VulcanPluginStorageOperation::LanceDbDropTable)
        }
        _ => None,
    }
}

/// Resolves one registered host VLDB tool name into its fixed operation.
/// 将一个已注册宿主 VLDB 工具名称解析为固定操作。
///
/// # Parameters
/// 参数
/// `tool_name` is the exact name received from the LuaSkills host callback.
/// `tool_name` 是从 LuaSkills 宿主回调收到的精确名称。
///
/// # Returns
/// 返回值
/// Returns the matching operation, or `None` when the name is not a VLDB tool.
/// 返回匹配操作；名称不是 VLDB 工具时返回 `None`。
fn plugin_vldb_operation(tool_name: &str) -> Option<VulcanPluginVldbOperation> {
    match tool_name {
        VULCAN_VLDB_SQLITE_QUERY_V1_TOOL => Some(VulcanPluginVldbOperation::SqliteQuery),
        VULCAN_VLDB_SQLITE_EXECUTE_V1_TOOL => Some(VulcanPluginVldbOperation::SqliteExecute),
        VULCAN_VLDB_SQLITE_EXECUTE_BATCH_V1_TOOL => {
            Some(VulcanPluginVldbOperation::SqliteExecuteBatch)
        }
        VULCAN_VLDB_SQLITE_FLUSH_V1_TOOL => Some(VulcanPluginVldbOperation::SqliteFlush),
        _ => None,
    }
}

/// Dispatches one capability-checked storage request without exposing host binding metadata.
/// 分发一条经过能力校验且不暴露宿主绑定元数据的存储请求。
///
/// # Parameters
/// 参数
/// `state` owns host services, `tool_name` selects the operation, and `request_args` carries bridge identity.
/// `state` 持有宿主服务，`tool_name` 选择操作，`request_args` 携带桥接身份。
///
/// # Returns
/// 返回值
/// Returns the JSON-safe storage result.
/// 返回 JSON 安全的存储结果。
fn dispatch_plugin_storage_request(
    state: &Arc<VulcanPluginManagerState>,
    tool_name: &str,
    request_args: &Value,
) -> Result<Value, String> {
    // storageOperation is fixed by the registered host-tool name.
    // storageOperation 由已注册的宿主工具名称固定决定。
    let storage_operation = plugin_storage_operation(tool_name)
        .ok_or_else(|| format!("Unsupported Vulcan plugin storage tool `{tool_name}`."))?;
    // pluginIdentity and payload are extracted only from the trusted bridge envelope.
    // pluginIdentity 与 payload 仅从可信桥接信封中提取。
    let (plugin_identity, payload) = bridge_request_identity_and_payload(request_args)?;
    // leaseScope binds capability checks to the exact runtime lifecycle state.
    // leaseScope 将能力校验绑定到精确运行时生命周期状态。
    let lease_scope = bridge_request_lease_scope(request_args)?;
    // requiredCapability prevents SQLite and LanceDB permissions from substituting for each other.
    // requiredCapability 防止 SQLite 与 LanceDB 权限彼此替代。
    let required_capability = if storage_operation.is_sqlite() {
        VulcanPluginPermission::StorageSqlite
    } else {
        VulcanPluginPermission::StorageLancedb
    };
    ensure_plugin_capability(state, plugin_identity, required_capability, lease_scope)?;
    // storageService is cloned before invocation so no runtime service lock crosses the call boundary.
    // storageService 在调用前克隆，避免运行时服务锁跨越调用边界。
    let storage_service = state
        .host_services
        .read()
        .map_err(|_| "Vulcan plugin host services lock is poisoned.".to_string())?
        .storage
        .clone()
        .ok_or_else(|| "Vulcan plugin storage service is unavailable.".to_string())?;
    storage_service.execute(VulcanPluginStorageRequest {
        deadline: super::action_status::callback_deadline(),
        plugin_id: plugin_identity.to_string(),
        operation: storage_operation,
        payload: Value::Object(payload.clone()),
    })
}

/// Dispatches one capability-checked request to a host-registered VLDB binding.
/// 向宿主注册的 VLDB 绑定分发一条经过能力校验的请求。
///
/// # Parameters
/// 参数
/// `state` owns host services, `tool_name` fixes the operation, and `request_args` carries bridge identity.
/// `state` 持有宿主服务，`tool_name` 固定操作，`request_args` 携带桥接身份。
///
/// # Returns
/// 返回值
/// Returns the JSON-safe host VLDB result.
/// 返回 JSON 安全的宿主 VLDB 结果。
fn dispatch_plugin_vldb_request(
    state: &Arc<VulcanPluginManagerState>,
    tool_name: &str,
    request_args: &Value,
) -> Result<Value, String> {
    // operation is fixed by the registered host-tool name and cannot be supplied by plugin payload.
    // operation 由已注册宿主工具名称固定，插件载荷不能自行提供。
    let operation = plugin_vldb_operation(tool_name)
        .ok_or_else(|| format!("Unsupported Vulcan plugin VLDB tool `{tool_name}`."))?;
    // pluginIdentity and payload are extracted only from the trusted bridge envelope.
    // pluginIdentity 与 payload 仅从可信桥接信封中提取。
    let (plugin_identity, payload) = bridge_request_identity_and_payload(request_args)?;
    // leaseScope binds capability checks to the exact runtime lifecycle state.
    // leaseScope 将能力校验绑定到精确运行时生命周期状态。
    let lease_scope = bridge_request_lease_scope(request_args)?;
    ensure_plugin_capability(
        state,
        plugin_identity,
        VulcanPluginPermission::SystemVldbAccess,
        lease_scope,
    )?;
    // vldbService is cloned before invocation so no runtime service lock crosses the call boundary.
    // vldbService 在调用前克隆，避免运行时服务锁跨越调用边界。
    let vldb_service = state
        .host_services
        .read()
        .map_err(|_| "Vulcan plugin host services lock is poisoned.".to_string())?
        .vldb
        .clone()
        .ok_or_else(|| "Vulcan plugin VLDB service is unavailable.".to_string())?;
    vldb_service.execute(VulcanPluginVldbRequest {
        deadline: super::action_status::callback_deadline(),
        plugin_id: plugin_identity.to_string(),
        operation,
        payload: Value::Object(payload.clone()),
    })
}

/// Publishes one capability-checked plugin UI invalidation event.
/// 发布一条经过能力校验的插件界面失效事件。
///
/// # Parameters
/// 参数
/// `state` owns host services and `request_args` carries bridge identity and event data.
/// `state` 持有宿主服务，`request_args` 携带桥接身份与事件数据。
///
/// # Returns
/// 返回值
/// Returns an acknowledgement object after publication.
/// 发布后返回确认对象。
fn dispatch_plugin_ui_invalidation(
    state: &Arc<VulcanPluginManagerState>,
    request_args: &Value,
) -> Result<Value, String> {
    // pluginIdentity and payload are extracted only from the trusted bridge envelope.
    // pluginIdentity 与 payload 仅从可信桥接信封中提取。
    let (plugin_identity, payload) = bridge_request_identity_and_payload(request_args)?;
    // leaseScope binds capability checks to the exact runtime lifecycle state.
    // leaseScope 将能力校验绑定到精确运行时生命周期状态。
    let lease_scope = bridge_request_lease_scope(request_args)?;
    ensure_plugin_capability(
        state,
        plugin_identity,
        VulcanPluginPermission::UiHostContext,
        lease_scope,
    )?;
    // topic is plugin-local and cannot address another plugin channel.
    // topic 属于插件内部，不能寻址其他插件通道。
    let topic = payload
        .get("topic")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 128)
        .ok_or_else(|| {
            "Vulcan plugin UI invalidation requires a topic of 1 to 128 bytes.".to_string()
        })?;
    // revision is a monotonic plugin-owned data version.
    // revision 是插件持有的单调数据版本。
    let revision = payload
        .get("revision")
        .and_then(Value::as_u64)
        .ok_or_else(|| "Vulcan plugin UI invalidation requires u64 revision.".to_string())?;
    // uiEventPublisher is cloned before invocation so no runtime service lock crosses the call boundary.
    // uiEventPublisher 在调用前克隆，避免运行时服务锁跨越调用边界。
    let ui_event_publisher = state
        .host_services
        .read()
        .map_err(|_| "Vulcan plugin host services lock is poisoned.".to_string())?
        .ui_events
        .clone()
        .ok_or_else(|| "Vulcan plugin UI event publisher is unavailable.".to_string())?;
    ui_event_publisher.publish(VulcanPluginUiInvalidation {
        plugin_id: plugin_identity.to_string(),
        topic: topic.to_string(),
        revision,
    })?;
    Ok(json!({ "published": true }))
}

/// Requires one plugin to match its lease lifecycle state and declare the exact host capability.
/// 要求插件匹配租约生命周期状态并声明精确宿主能力。
///
/// # Parameters
/// 参数
/// `state` owns the catalog; `plugin_id` and `capability` select the declaration, while `lease_scope` fixes the allowed state.
/// `state` 持有目录；`plugin_id` 与 `capability` 选择声明，`lease_scope` 固定允许的状态。
///
/// # Returns
/// 返回值
/// Returns unit only when the exact lifecycle state and declared capability both match.
/// 仅当精确生命周期状态与声明能力均匹配时返回空值。
fn ensure_plugin_capability(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    capability: VulcanPluginPermission,
    lease_scope: VulcanPluginBridgeLeaseScope,
) -> Result<(), String> {
    let snapshot = state
        .snapshot
        .read()
        .map_err(|_| "Vulcan plugin catalog lock was poisoned.".to_string())?;
    let plugin = snapshot
        .plugins
        .get(plugin_id)
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is not installed."))?;
    let required_runtime_state = lease_scope.required_runtime_state();
    if plugin.runtime_state != required_runtime_state {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` runtime state does not authorize bridge scope `{}`.",
            lease_scope.as_str()
        ));
    }
    let manifest = plugin
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` has no valid manifest."))?;
    if !manifest.permissions.contains(&capability) {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` did not declare `{}` capability.",
            plugin_permission_key(capability)
        ));
    }
    Ok(())
}

/// Reads the bridge-injected plugin identity from one host-tool request.
/// 从单个宿主工具请求读取桥接层注入的插件身份。
///
/// # Parameters
/// 参数
/// `request_args` is the exact bridge-owned request object.
/// `request_args` 是精确的桥接层持有请求对象。
///
/// # Returns
/// 返回值
/// Returns the validated plugin identifier.
/// 返回已校验插件标识。
fn bridge_request_plugin_id(request_args: &Value) -> Result<&str, String> {
    let request = request_args
        .as_object()
        .ok_or_else(|| "Vulcan plugin host request must be an object.".to_string())?;
    required_json_string(request, "plugin_id")
}

/// Parses the immutable lease scope injected by the fixed Lua bridge environment.
/// 解析固定 Lua 桥接环境注入的不可变租约作用域。
///
/// # Parameters
/// 参数
/// `request_args` is the exact host callback envelope produced by the bridge.
/// `request_args` 是桥接层产生的精确宿主回调信封。
///
/// # Returns
/// 返回值
/// Returns one exact scope or an explicit bridge protocol error.
/// 返回一个精确作用域或明确的桥接协议错误。
fn bridge_request_lease_scope(
    request_args: &Value,
) -> Result<VulcanPluginBridgeLeaseScope, String> {
    let request = request_args
        .as_object()
        .ok_or_else(|| "Vulcan plugin host request must be an object.".to_string())?;
    VulcanPluginBridgeLeaseScope::parse(required_json_string(request, "lease_scope")?)
}

/// Resolves the trusted workspace root injected by the fixed bridge into one host request.
/// 解析固定桥接层注入到单个宿主请求中的可信工作区根目录。
///
/// # Parameters
/// 参数
/// `request` is the bridge-owned outer request object and never plugin-owned payload data.
/// `request` 是桥接层持有的外层请求对象，绝不是插件持有的载荷数据。
///
/// # Returns
/// 返回值
/// Returns one canonical existing workspace directory.
/// 返回一个规范化且已存在的工作区目录。
pub(super) fn canonical_bridge_workspace_root(
    request: &Map<String, Value>,
) -> Result<PathBuf, String> {
    let raw_workspace_root = required_json_string(request, "workspace_root")?;
    let workspace_root = PathBuf::from(raw_workspace_root)
        .canonicalize()
        .map_err(|error| format!("Failed to resolve trusted plugin workspace root: {error}"))?;
    if !workspace_root.is_dir() {
        return Err("Trusted plugin workspace root is not a directory.".to_string());
    }
    Ok(workspace_root)
}

/// Reads bridge-injected plugin identity and its exact payload object.
/// 读取桥接层注入的插件身份及其精确载荷对象。
///
/// # Parameters
/// 参数
/// `request_args` is the exact bridge-owned request object.
/// `request_args` 是精确的桥接层持有请求对象。
///
/// # Returns
/// 返回值
/// Returns the plugin identifier and payload object.
/// 返回插件标识与载荷对象。
fn bridge_request_identity_and_payload(
    request_args: &Value,
) -> Result<VulcanPluginBridgeIdentityPayload<'_>, String> {
    let request = request_args
        .as_object()
        .ok_or_else(|| "Vulcan plugin host request must be an object.".to_string())?;
    let plugin_id = required_json_string(request, "plugin_id")?;
    // payload must preserve the object type guaranteed by the LuaSkills typed JSON boundary.
    // payload 必须保留 LuaSkills 强类型 JSON 边界保证的对象类型。
    let payload = request
        .get("payload")
        .and_then(Value::as_object)
        .ok_or_else(|| "Vulcan plugin host request requires object field `payload`.".to_string())?;
    Ok((plugin_id, payload))
}

/// Reads one required non-empty string from a JSON object.
/// 从 JSON 对象读取一个必填非空字符串。
/// Reads exact field from object; returns a borrowed nonempty string or a protocol diagnostic.
/// 从object读取精确field；返回借用的非空字符串或协议诊断。
pub(super) fn required_json_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Vulcan plugin request requires non-empty string `{field}`."))
}

#[cfg(test)]
mod tests;
