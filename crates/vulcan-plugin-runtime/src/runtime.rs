//! Long-lived generic Vulcan Plugin session runtime backed by LuaSkills leases.
//! 由 LuaSkills 租约承载的长生命周期通用 Vulcan 插件会话运行时。

use crate::{
    VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS, VULCAN_PLUGIN_SESSION_ID_ARGUMENT,
    VulcanPluginAvailability, VulcanPluginCatalogSnapshot, VulcanPluginDependencyInstallMode,
    VulcanPluginHookDescriptor, VulcanPluginHostServices, VulcanPluginLuaRuntimeManifest,
    VulcanPluginManifest, VulcanPluginMcpStdioLauncherManifest, VulcanPluginMcpTransportManifest,
    VulcanPluginPackageOrigin, VulcanPluginPermission, VulcanPluginRuntimeState,
    VulcanPluginStorageOperation, VulcanPluginStorageRequest, VulcanPluginToolDescriptor,
    VulcanPluginToolExecutionKind, VulcanPluginUiInvalidation, inspect_user_plugin_package,
    load_vulcan_plugin_catalog,
};
use luaskills::{
    LuaEngine, ManagedRuntimeKind, PackageDependencyManifest, RuntimeHostToolAction,
    RuntimeHostToolRequest, current_managed_runtime_platform_key, resolve_managed_runtime_install,
    set_host_tool_callback,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, RwLock, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;
use vulcan_hooks::{HookInvocation, HookResult};
use vulcan_runtime_assets::text_decode::decode_complete_user_text;

/// Maximum Lua execution time used while probing one plugin package.
/// 探测单个插件包时使用的 Lua 最大执行时长。
const VULCAN_PLUGIN_PROBE_TIMEOUT_MS: u64 = 5_000;

/// Default Lua execution time used when one hook declaration omits a timeout.
/// hook 声明省略超时时使用的 Lua 默认执行时长。
const VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS: u64 = 60_000;

/// Maximum Lua execution time used while shutting down one persistent hook runtime.
/// 关闭单个持久 hook 运行时时使用的 Lua 最大执行时长。
const VULCAN_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS: u64 = 5_000;

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

/// Windows creation flag that prevents a console window for a child process.
/// 防止子进程创建控制台窗口的 Windows 创建标志。
#[cfg(all(windows, not(debug_assertions)))]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply the Release-only no-console flag to a plugin dependency child process.
/// 为插件依赖子进程应用仅 Release 生效的无控制台标志。
fn apply_release_no_window_to_command(command: &mut Command) {
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    #[cfg(any(not(windows), debug_assertions))]
    {
        let _ = command;
    }
}

/// Authenticated plugin identity paired with its validated bridge payload.
/// 经过认证的插件身份及其已校验桥接载荷。
type VulcanPluginBridgeIdentityPayload<'a> = (&'a str, &'a Map<String, Value>);

/// Process-wide manager registry used by the single LuaSkills host-tool callback.
/// 单一 LuaSkills 宿主工具回调使用的进程级管理器注册表。
static VULCAN_PLUGIN_MANAGER_REGISTRY: OnceLock<
    Mutex<BTreeMap<String, Weak<VulcanPluginManagerState>>>,
> = OnceLock::new();

/// One model tool invocation projected into the generic plugin runtime.
/// 投影到通用插件运行时的单次模型工具调用。
#[derive(Debug, Clone)]
pub struct VulcanPluginInvocationRequest {
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

/// One host or WebUI invocation of a manifest-declared plugin business action.
/// 一次由宿主或 WebUI 发起的清单声明插件业务 action 调用。
#[derive(Debug, Clone)]
pub struct VulcanPluginActionRequest {
    /// Exact owning plugin identifier.
    /// 精确的所属插件标识。
    pub plugin_id: String,
    /// Exact plugin-local action identifier.
    /// 精确的插件内 action 标识。
    pub action_id: String,
    /// Raw action arguments validated against the manifest schema.
    /// 根据清单 Schema 校验的原始 action 参数。
    pub arguments: Value,
    /// Caller class enforced against the action declaration.
    /// 根据 action 声明执行校验的调用方类别。
    pub caller: crate::VulcanPluginActionCaller,
    /// Canonical workspace root exposed to a workspace-bound temporary bridge lease.
    /// 暴露给工作区绑定临时桥接租约的规范化工作区根目录。
    pub workspace_root: PathBuf,
    /// Whether the host authorizes reuse of the enabled plugin's persistent global runtime.
    /// 宿主是否授权复用已启用插件的持久全局运行时。
    pub reuse_enabled_runtime: bool,
}

/// User-facing authorization preview derived exclusively from one validated plugin manifest.
/// 完全从已校验插件清单派生的面向用户授权预览。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginPermissionPreview {
    /// Stable plugin identity.
    /// 稳定插件身份。
    pub plugin_id: String,
    /// Exact package version.
    /// 精确插件包版本。
    pub version: String,
    /// Exact package content fingerprint bound to this authorization preview.
    /// 与当前授权预览绑定的精确插件包内容指纹。
    pub content_fingerprint: String,
    /// Effective package origin selected by root precedence.
    /// 由根目录优先级选定的生效插件包来源。
    pub origin: String,
    /// Stable requested permission keys.
    /// 稳定的请求权限键集合。
    pub permissions: Vec<String>,
    /// Host hook points affected by the plugin.
    /// 插件影响的宿主 Hook 点集合。
    pub hook_points: Vec<String>,
    /// Host UI mount points affected by the plugin.
    /// 插件影响的宿主 UI 挂载点集合。
    pub ui_mount_points: Vec<String>,
    /// Logical native binary identifiers.
    /// 逻辑原生二进制标识集合。
    pub binary_ids: Vec<String>,
    /// Logical Node.js entry identifiers.
    /// 逻辑 Node.js 入口标识集合。
    pub node_entry_ids: Vec<String>,
    /// Logical Python entry identifiers.
    /// 逻辑 Python 入口标识集合。
    pub python_entry_ids: Vec<String>,
    /// Read-only Agent Skill identifiers contained in the validated package.
    /// 已校验插件包内包含的只读 Agent Skill 标识集合。
    pub skill_ids: Vec<String>,
    /// Read-only subagent identifiers contained in the validated package.
    /// 已校验插件包内包含的只读子代理标识集合。
    pub agent_ids: Vec<String>,
    /// Whether the package declares an executable Lua runtime.
    /// 当前插件包是否声明了可执行 Lua 运行时。
    pub has_lua_runtime: bool,
    /// Sanitized MCP server declarations included in the authorization surface.
    /// 纳入授权界面的脱敏 MCP 服务器声明。
    pub mcp_servers: Vec<VulcanPluginMcpPermissionPreview>,
    /// Secret values required to start local MCP servers during the installation transaction.
    /// 安装事务中启动本地 MCP 服务器所需的 Secret 值声明。
    pub install_secret_requirements: Vec<VulcanPluginInstallSecretRequirement>,
}

/// Sanitized authorization summary for one plugin-owned MCP server.
/// 单个插件自有 MCP 服务器的脱敏授权摘要。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginMcpPermissionPreview {
    /// Stable plugin-local server identifier.
    /// 稳定的插件内服务器标识。
    pub server_id: String,
    /// User-facing server name from the validated manifest.
    /// 来自已校验清单的用户可见服务器名称。
    pub name: String,
    /// Exact transport family without endpoint or credential values.
    /// 不含端点或凭据值的精确传输类型。
    pub transport: String,
    /// Exact declared Secret Store keys without their values.
    /// 不含值的精确已声明 Secret Store 键集合。
    pub secret_keys: Vec<String>,
}

/// One exact plugin-scoped Secret required for local MCP installation validation.
/// 本地 MCP 安装验证所需的单个精确插件作用域 Secret。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginInstallSecretRequirement {
    /// Plugin-local MCP server identifier owning the Secret namespace.
    /// 持有当前 Secret 命名空间的插件内 MCP 服务器标识。
    pub server_id: String,
    /// Exact manifest-declared Secret key.
    /// 清单声明的精确 Secret 键。
    pub secret_key: String,
}

/// Host-resolved process launch data for one plugin-owned stdio MCP server.
/// 单个插件自有 stdio MCP 服务器的宿主解析进程启动数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulcanPluginMcpStdioLaunchSpec {
    /// Exact executable selected without Shell or PATH lookup.
    /// 不经 Shell 或 PATH 查找选定的精确可执行文件。
    pub program: PathBuf,
    /// Host-owned arguments that must precede manifest-resolved server arguments.
    /// 必须位于清单解析服务器参数之前的宿主持有参数。
    pub base_args: Vec<String>,
    /// Canonical working directory contained by the immutable plugin environment.
    /// 受不可变插件环境约束的规范工作目录。
    pub cwd: PathBuf,
    /// Host-owned non-secret environment values required by the launcher.
    /// 启动器所需的宿主持有非敏感环境变量值。
    pub env: BTreeMap<String, String>,
}

/// Durable authorization record bound to an exact plugin version and content fingerprint.
/// 绑定到精确插件版本与内容指纹的持久授权记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VulcanPluginAuthorizationRecord {
    /// Record schema version.
    /// 记录 Schema 版本。
    schema_version: u32,
    /// Stable plugin identity.
    /// 稳定插件身份。
    plugin_id: String,
    /// Exact authorized package version.
    /// 精确授权插件包版本。
    version: String,
    /// Exact authorized content fingerprint.
    /// 精确授权内容指纹。
    content_fingerprint: String,
    /// Complete sorted permission set approved by the user.
    /// 用户批准的完整有序权限集合。
    permissions: Vec<String>,
}

/// Durable installation record for one atomically published user plugin package.
/// 一个原子发布用户插件包的持久安装记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VulcanPluginInstallationRecord {
    /// Stable record schema version.
    /// 稳定记录结构版本。
    schema_version: u32,
    /// Exact installed plugin identity.
    /// 精确的已安装插件身份。
    plugin_id: String,
    /// Exact installed package version.
    /// 精确的已安装包版本。
    version: String,
    /// Exact installed content fingerprint.
    /// 精确的已安装内容指纹。
    content_fingerprint: String,
    /// Unix timestamp in milliseconds when publication completed.
    /// 发布完成时的 Unix 毫秒时间戳。
    installed_at_ms: u64,
}

/// Immutable identity marker for one Vulcan-owned plugin process environment.
/// 单个 Vulcan 自有插件进程环境的不可变身份标记。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VulcanPluginEnvironmentMarker {
    /// Stable marker schema version.
    /// 稳定标记结构版本。
    schema_version: u32,
    /// Exact plugin identity.
    /// 精确插件身份。
    plugin_id: String,
    /// Exact package version.
    /// 精确插件包版本。
    version: String,
    /// Exact validated package fingerprint.
    /// 精确的已校验插件包指纹。
    content_fingerprint: String,
    /// Effective dependency installation policy.
    /// 生效的依赖安装策略。
    install_mode: String,
}

/// Result of one local user-plugin installation transaction.
/// 一次本地用户插件安装事务的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginInstallationResult {
    /// Exact installed plugin identity.
    /// 精确的已安装插件身份。
    pub plugin_id: String,
    /// Permission preview durably approved for the installed package.
    /// 为已安装包持久批准的权限预览。
    pub permission_preview: VulcanPluginPermissionPreview,
}

/// Pending filesystem transaction for one disabled user-plugin package update.
/// 单个已停用用户插件包更新的待提交文件系统事务。
#[derive(Debug)]
#[must_use = "a plugin update transaction must be committed or rolled back"]
pub struct VulcanPluginUpdateTransaction {
    /// Exact updated plugin identity.
    /// 精确的已更新插件身份。
    plugin_id: String,
    /// Unique private transaction root retaining the previous package until commit.
    /// 提交前保留旧插件包的唯一私有事务根目录。
    transaction_root: PathBuf,
    /// Canonical user-plugin package destination containing the candidate package.
    /// 包含候选插件包的规范化用户插件目标目录。
    destination: PathBuf,
    /// Previous authorization record bytes restored verbatim on rollback.
    /// 回滚时逐字节恢复的旧授权记录内容。
    previous_authorization: Vec<u8>,
    /// Previous installation record bytes restored verbatim on rollback.
    /// 回滚时逐字节恢复的旧安装记录内容。
    previous_installation: Vec<u8>,
    /// Candidate version used to remove only its unpublished private environment on rollback.
    /// 回滚时仅移除其未发布私有环境的候选版本。
    candidate_version: String,
    /// Candidate fingerprint used to identify the exact unpublished private environment.
    /// 用于识别精确未发布私有环境的候选指纹。
    candidate_fingerprint: String,
    /// Host-derived authorization preview for the candidate package.
    /// 候选插件包的宿主派生授权预览。
    permission_preview: VulcanPluginPermissionPreview,
}

impl VulcanPluginUpdateTransaction {
    /// Returns the exact updated plugin identifier.
    /// 返回精确的已更新插件标识。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Returns the candidate package authorization preview.
    /// 返回候选插件包的授权预览。
    pub fn permission_preview(&self) -> &VulcanPluginPermissionPreview {
        &self.permission_preview
    }
}

/// Host-owned plugin lifecycle event that may invoke one manifest callback.
/// 可调用清单回调的宿主自有插件生命周期事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginLifecycleEvent {
    /// Package installation completed and its private environment is ready.
    /// 插件包安装完成且私有环境已就绪。
    Install,
    /// Plugin is about to become enabled.
    /// 插件即将进入启用状态。
    Enable,
    /// Compensate a failed enable transition through the declared disable callback.
    /// 通过声明的停用回调补偿失败的启用转换。
    EnableRollback,
    /// Plugin has stopped accepting new work and is being disabled.
    /// 插件已停止接收新任务并正在停用。
    Disable,
    /// User package is about to be removed.
    /// 用户插件包即将被移除。
    Uninstall,
}

/// Host-injected lease scope that binds bridge calls to one exact runtime lifecycle state.
/// 宿主注入的租约作用域，把桥接调用绑定到一个精确运行时生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VulcanPluginBridgeLeaseScope {
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
    fn as_str(self) -> &'static str {
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
    fn parse(value: &str) -> Result<Self, String> {
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
    fn required_runtime_state(self) -> VulcanPluginRuntimeState {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    /// Shared LuaSkills engine used for host-managed bridge leases.
    /// 用于宿主受管桥接租约的共享 LuaSkills 引擎。
    engine: RwLock<Option<Arc<Mutex<LuaEngine>>>>,
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
    /// Global initialization gate preventing duplicate hook leases during concurrent first calls.
    /// 防止并发首次调用创建重复 hook 租约的全局初始化门。
    hook_runtime_initialization_gate: AsyncMutex<()>,
    /// Global initialization gate preventing duplicate UI action leases during concurrent first calls.
    /// 防止并发首次调用创建重复界面 Action 租约的全局初始化门。
    ui_action_runtime_initialization_gate: AsyncMutex<()>,
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

/// Result returned after bounded plugin session draining.
/// 有界插件会话排空后返回的结果。
#[derive(Debug, Clone, Default)]
pub struct VulcanPluginDrainReport {
    /// Number of sessions closed and removed successfully.
    /// 成功关闭并移除的会话数量。
    pub closed_sessions: usize,
    /// Number of persistent plugin hook runtimes closed successfully.
    /// 成功关闭的持久插件 hook 运行时数量。
    pub closed_hook_runtimes: usize,
    /// Number of persistent plugin UI action runtimes closed successfully.
    /// 成功关闭的持久插件界面 Action 运行时数量。
    pub closed_ui_action_runtimes: usize,
    /// Number of previously retained raw leases closed successfully.
    /// 成功关闭的此前保留原始租约数量。
    pub closed_retained_leases: usize,
    /// Deterministic cleanup failures retained for diagnostics and retry.
    /// 为诊断与重试保留的确定性清理失败集合。
    pub failures: Vec<String>,
}

impl VulcanPluginDrainReport {
    /// Merges another drain phase into this ordered lifecycle report.
    /// 将另一个排空阶段合并到当前有序生命周期报告。
    fn merge(&mut self, other: Self) {
        self.closed_sessions = self.closed_sessions.saturating_add(other.closed_sessions);
        self.closed_hook_runtimes = self
            .closed_hook_runtimes
            .saturating_add(other.closed_hook_runtimes);
        self.closed_ui_action_runtimes = self
            .closed_ui_action_runtimes
            .saturating_add(other.closed_ui_action_runtimes);
        self.closed_retained_leases = self
            .closed_retained_leases
            .saturating_add(other.closed_retained_leases);
        self.failures.extend(other.failures);
    }
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
        let Ok(engine) = engine.lock() else {
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
        let ui_action_runtimes = self
            .ui_action_runtimes
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for runtime in ui_action_runtimes.values() {
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
    /// Creates one manager around the shared LuaSkills engine and deployed plugin root.
    /// 围绕共享 LuaSkills 引擎与已部署插件根目录创建管理器。
    ///
    /// # Parameters
    /// 参数
    /// `engine` is the shared LuaSkills engine and `vulcan_plugin_root` is the trusted package root.
    /// `engine` 是共享 LuaSkills 引擎，`vulcan_plugin_root` 是受信任包根。
    ///
    /// # Returns
    /// 返回值
    /// Returns a manager initialized with an empty catalog snapshot.
    /// 返回以空目录快照初始化的管理器。
    pub fn new(
        engine: Arc<Mutex<LuaEngine>>,
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
        engine: Option<Arc<Mutex<LuaEngine>>>,
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
                hook_runtime_initialization_gate: AsyncMutex::new(()),
                ui_action_runtime_initialization_gate: AsyncMutex::new(()),
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

    /// Attaches the shared Lua engine used by executable plugins.
    /// 附加执行型插件使用的共享 Lua 引擎。
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
    pub fn attach_runtime_engine(&self, engine: Arc<Mutex<LuaEngine>>) -> Result<(), String> {
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
    fn runtime_engine(&self) -> Result<Arc<Mutex<LuaEngine>>, VulcanPluginRuntimeError> {
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

    /// Installs the current application-owned services used by authorized plugin bridge calls.
    /// 安装当前应用持有且供已授权插件桥接调用使用的服务。
    ///
    /// # Parameters
    /// 参数
    /// `services` contains optional storage and UI event implementations.
    /// `services` 包含可选的存储与界面事件实现。
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
    fn install_host_tool_bridge(&self) {
        let registry = VULCAN_PLUGIN_MANAGER_REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()));
        if let Ok(mut managers) = registry.lock() {
            managers.retain(|_, manager| manager.strong_count() > 0);
            managers.insert(self.state.manager_id.clone(), Arc::downgrade(&self.state));
        }
        set_host_tool_callback(Some(Arc::new(dispatch_registered_plugin_host_tool_request)));
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

    /// Reads the effective configuration for one manifest-declared plugin.
    /// 读取一个清单声明插件的生效配置。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact catalog identity.
    /// `plugin_id` 是精确目录身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the persisted configuration or the package defaults when no state exists.
    /// 返回持久化配置；状态不存在时返回插件包默认值。
    pub fn plugin_config(&self, plugin_id: &str) -> Result<Value, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        // manifest determines whether this package owns any executable resource requiring isolation.
        // manifest 决定当前插件包是否持有任何需要隔离的可执行资源。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let config = manifest.config.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` does not declare configuration."
            ))
        })?;
        // executionRoot pins configuration defaults and schema to the authorized package generation.
        // executionRoot 把配置默认值与 Schema 固定到已授权插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let state_path = plugin_config_state_path(&self.state.plugin_state_root, plugin_id);
        let value = if state_path.exists() {
            read_plugin_json_state(&state_path, "configuration")?
        } else {
            let defaults_path = canonical_plugin_file(
                &execution_root,
                &config.defaults_file,
                "configuration defaults",
            )
            .map_err(VulcanPluginRuntimeError::new)?;
            read_plugin_json_state(&defaults_path, "configuration defaults")?
        };
        validate_plugin_config_value(&execution_root, config, &value)?;
        Ok(value)
    }

    /// Validates and atomically persists one complete plugin configuration object.
    /// 校验并原子持久化一个完整插件配置对象。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package and `value` is the complete replacement document.
    /// `plugin_id` 选择插件包，`value` 是完整替换文档。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact persisted configuration after durable publication.
    /// 在持久发布后返回精确持久化配置。
    pub fn update_plugin_config(
        &self,
        plugin_id: &str,
        value: Value,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let config = manifest.config.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` does not declare configuration."
            ))
        })?;
        // executionRoot pins configuration validation to the authorized package generation.
        // executionRoot 把配置校验固定到已授权插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        validate_plugin_config_value(&execution_root, config, &value)?;
        let encoded = serde_json::to_vec_pretty(&value).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to encode Vulcan plugin `{plugin_id}` configuration: {error}"
            ))
        })?;
        crate::atomic_file::write_file_atomically(
            &plugin_config_state_path(&self.state.plugin_state_root, plugin_id),
            &encoded,
        )
        .map_err(VulcanPluginRuntimeError::new)?;
        Ok(value)
    }

    /// Persists one complete plugin configuration only when no prior state exists.
    /// 仅当不存在既有状态时持久化一个完整插件配置。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package and `value` is the initial complete document.
    /// `plugin_id` 选择插件包，`value` 是初始完整文档。
    ///
    /// # Returns
    /// 返回值
    /// Returns `true` when the initial state was written and `false` when user state already exists.
    /// 写入初始状态时返回 `true`；用户状态已存在时返回 `false`。
    pub fn initialize_plugin_config_if_absent(
        &self,
        plugin_id: &str,
        value: Value,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let state_path = plugin_config_state_path(&self.state.plugin_state_root, plugin_id);
        if state_path.exists() {
            return Ok(false);
        }
        self.update_plugin_config(plugin_id, value)?;
        Ok(true)
    }

    /// Builds the complete user-visible authorization preview for one plugin.
    /// 为一个插件构建完整的用户可见授权预览。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact catalog identity.
    /// `plugin_id` 是精确目录身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns stable capability, hook, mount, and process-entry declarations.
    /// 返回稳定的能力、Hook、挂载点与进程入口声明。
    pub fn permission_preview(
        &self,
        plugin_id: &str,
    ) -> Result<VulcanPluginPermissionPreview, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        build_permission_preview(plugin)
    }

    /// Resolves a native binary directory requested by one successfully completed plugin hook.
    /// 解析一个成功完成插件 Hook 请求的原生二进制目录。
    ///
    /// # Parameters
    /// 参数
    /// `registration_id` binds the request to an enabled hook and `binary_id` selects its resource.
    /// `registration_id` 把请求绑定到已启用 Hook，`binary_id` 选择其资源。
    ///
    /// # Returns
    /// 返回值
    /// Returns the canonical directory containing the exact current-target binary.
    /// 返回包含当前精确目标二进制文件的规范目录。
    pub fn hook_binary_directory(
        &self,
        registration_id: &str,
        binary_id: &str,
    ) -> Result<PathBuf, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let hook = snapshot.enabled_hooks.get(registration_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin hook `{registration_id}` is not enabled."
            ))
        })?;
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not installed.",
                hook.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no valid manifest.",
                hook.plugin_id
            ))
        })?;
        if !manifest
            .permissions
            .contains(&VulcanPluginPermission::ProcessBinary)
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` did not declare process_binary capability.",
                hook.plugin_id
            )));
        }
        let binary = manifest
            .execution
            .native_binaries
            .iter()
            .find(|entry| entry.id == binary_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` does not declare binary `{binary_id}`.",
                    hook.plugin_id
                ))
            })?;
        let target = current_plugin_target_triple().map_err(VulcanPluginRuntimeError::new)?;
        let relative = binary.targets.get(target).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` binary `{binary_id}` has no asset for target `{target}`.",
                hook.plugin_id
            ))
        })?;
        // executionRoot resolves the host-owned directory injected for this logical binary id.
        // executionRoot 解析当前逻辑二进制标识对应的宿主持有目录。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let executable = canonical_plugin_file(&execution_root, relative, "binary")
            .map_err(VulcanPluginRuntimeError::new)?;
        executable.parent().map(Path::to_path_buf).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin binary `{}` has no containing directory.",
                executable.display()
            ))
        })
    }

    /// Persists an exact authorization grant for one user-installed plugin generation.
    /// 为一个用户安装插件代次持久化精确授权许可。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package and `approved_permissions` must equal its full declaration.
    /// `plugin_id` 选择插件包，`approved_permissions` 必须等于其完整声明。
    ///
    /// # Returns
    /// 返回值
    /// Returns the effective permission preview after durable authorization.
    /// 持久授权后返回生效权限预览。
    pub fn authorize_user_plugin(
        &self,
        plugin_id: &str,
        mut approved_permissions: Vec<String>,
    ) -> Result<VulcanPluginPermissionPreview, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        if plugin.origin != VulcanPluginPackageOrigin::UserInstalled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` is bundled and does not accept user authorization records."
            )));
        }
        let preview = build_permission_preview(plugin)?;
        approved_permissions.sort();
        if approved_permissions
            .windows(2)
            .any(|pair| pair[0] == pair[1])
        {
            return Err(VulcanPluginRuntimeError::new(
                "Approved Vulcan plugin capabilities contain duplicates.",
            ));
        }
        if approved_permissions != preview.permissions {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` authorization must approve the complete current capability set."
            )));
        }
        let record = VulcanPluginAuthorizationRecord {
            schema_version: 1,
            plugin_id: plugin.id.clone(),
            version: plugin.version.clone().ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{plugin_id}` has no validated version."
                ))
            })?,
            content_fingerprint: plugin.content_fingerprint.clone(),
            permissions: approved_permissions,
        };
        let encoded = serde_json::to_vec_pretty(&record).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to encode Vulcan plugin `{plugin_id}` authorization: {error}"
            ))
        })?;
        crate::atomic_file::write_file_atomically(
            &plugin_authorization_state_path(&self.state.plugin_state_root, plugin_id),
            &encoded,
        )
        .map_err(VulcanPluginRuntimeError::new)?;
        Ok(preview)
    }

    /// Inspects one local user-plugin package before any filesystem mutation.
    /// 在执行任何文件系统变更前检查一个本地用户插件包。
    ///
    /// # Parameters
    /// 参数
    /// `source_root` is the exact local package directory selected by the user.
    /// `source_root` 是用户选择的精确本地插件包目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the complete capability, Hook, UI, and process permission preview.
    /// 返回完整能力、Hook、界面及进程权限预览。
    pub fn inspect_local_user_plugin(
        &self,
        source_root: &Path,
    ) -> Result<VulcanPluginPermissionPreview, VulcanPluginRuntimeError> {
        let descriptor =
            inspect_user_plugin_package(source_root).map_err(VulcanPluginRuntimeError::new)?;
        build_permission_preview(&descriptor)
    }

    /// Atomically installs and authorizes one validated local user-plugin package.
    /// 原子安装并授权一个经过校验的本地用户插件包。
    ///
    /// # Parameters
    /// 参数
    /// `source_root` selects the package, `expected_content_fingerprint` binds the prior authorization preview,
    /// `approved_permissions` must match its exact declaration, and `allow_registry_dependencies` records explicit network installation consent.
    /// `source_root` 选择插件包，`expected_content_fingerprint` 绑定此前的授权预览，
    /// `approved_permissions` 必须匹配其精确声明，`allow_registry_dependencies` 记录显式网络依赖安装许可。
    ///
    /// # Returns
    /// 返回值
    /// Returns the installed identity and durable permission preview.
    /// 返回已安装身份及持久权限预览。
    pub fn install_local_user_plugin(
        &self,
        source_root: &Path,
        expected_content_fingerprint: &str,
        mut approved_permissions: Vec<String>,
        allow_registry_dependencies: bool,
    ) -> Result<VulcanPluginInstallationResult, VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin installation gate is poisoned.")
        })?;
        let source_descriptor =
            inspect_user_plugin_package(source_root).map_err(VulcanPluginRuntimeError::new)?;
        if source_descriptor.content_fingerprint != expected_content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin package changed after authorization inspection.",
            ));
        }
        let manifest = source_descriptor.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Local user plugin has no valid manifest.")
        })?;
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Registry
            && !allow_registry_dependencies
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` requires explicit registry dependency network authorization.",
                source_descriptor.id
            )));
        }
        let preview = build_permission_preview(&source_descriptor)?;
        approved_permissions.sort();
        if approved_permissions
            .windows(2)
            .any(|pair| pair[0] == pair[1])
            || approved_permissions != preview.permissions
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` installation must approve the complete current capability set exactly once.",
                source_descriptor.id
            )));
        }
        if self
            .snapshot()
            .plugins
            .get(&source_descriptor.id)
            .is_some_and(|plugin| plugin.origin == VulcanPluginPackageOrigin::Bundled)
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is already supplied by the system distribution.",
                source_descriptor.id
            )));
        }
        fs::create_dir_all(&self.state.user_plugin_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to create user plugin root: {error}"))
        })?;
        let canonical_user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve user plugin root: {error}"
                ))
            })?;
        let destination = canonical_user_root.join(&source_descriptor.id);
        if destination.exists() {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is already installed for this user.",
                source_descriptor.id
            )));
        }
        // authorization_path, installation_path, and config_path must be absent so rollback never deletes pre-existing user state.
        // authorization_path、installation_path 与 config_path 必须不存在，确保回滚绝不删除既有用户状态。
        let authorization_path =
            plugin_authorization_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let installation_path =
            plugin_installation_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let config_path =
            plugin_config_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        for (label, path) in [
            ("authorization", &authorization_path),
            ("installation", &installation_path),
            ("configuration", &config_path),
        ] {
            if path.exists() {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` has stale {label} state at `{}`; remove or recover it before installation.",
                    source_descriptor.id,
                    path.display()
                )));
            }
        }
        let staging_root = canonical_user_root.join(format!(".install-{}", Uuid::new_v4()));
        let staged_package = staging_root.join(&source_descriptor.id);
        let transaction =
            (|| -> Result<VulcanPluginInstallationResult, VulcanPluginRuntimeError> {
                copy_plugin_directory_tree(&source_descriptor.package_root, &staged_package)
                    .map_err(VulcanPluginRuntimeError::new)?;
                let staged_descriptor = inspect_user_plugin_package(&staged_package)
                    .map_err(VulcanPluginRuntimeError::new)?;
                if staged_descriptor.content_fingerprint != source_descriptor.content_fingerprint {
                    return Err(VulcanPluginRuntimeError::new(
                        "Local plugin package changed while it was being installed.",
                    ));
                }
                fs::rename(&staged_package, &destination).map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "Failed to atomically publish Vulcan plugin `{}`: {error}",
                        source_descriptor.id
                    ))
                })?;
                let authorization = authorization_record_for_descriptor(
                    &staged_descriptor,
                    approved_permissions.clone(),
                )?;
                write_plugin_json_record(&authorization_path, &authorization, "authorization")?;
                let installation = VulcanPluginInstallationRecord {
                    schema_version: 1,
                    plugin_id: staged_descriptor.id.clone(),
                    version: staged_descriptor.version.clone().ok_or_else(|| {
                        VulcanPluginRuntimeError::new("Installed plugin has no validated version.")
                    })?,
                    content_fingerprint: staged_descriptor.content_fingerprint.clone(),
                    installed_at_ms: current_unix_time_ms(),
                };
                write_plugin_json_record(&installation_path, &installation, "installation")?;
                Ok(VulcanPluginInstallationResult {
                    plugin_id: source_descriptor.id.clone(),
                    permission_preview: preview.clone(),
                })
            })();
        match transaction {
            Ok(result) => {
                if staging_root.exists() {
                    // The published package no longer depends on its now-empty unique staging parent.
                    // 已发布插件包不再依赖其当前为空的唯一暂存父目录。
                    let _ = fs::remove_dir(&staging_root);
                }
                Ok(result)
            }
            Err(error) => {
                // rollbackFailures preserves every residual package or state cleanup failure.
                // rollbackFailures 保留每个残余插件包或状态清理失败。
                let mut rollback_failures = Vec::new();
                if destination.exists()
                    && let Err(rollback_error) = fs::remove_dir_all(&destination)
                {
                    rollback_failures.push(format!(
                        "Failed to remove published plugin package during rollback: {rollback_error}"
                    ));
                }
                for (label, path) in [
                    ("authorization", &authorization_path),
                    ("installation", &installation_path),
                ] {
                    if path.exists()
                        && let Err(rollback_error) = fs::remove_file(path)
                    {
                        rollback_failures.push(format!(
                            "Failed to remove plugin {label} state during rollback: {rollback_error}"
                        ));
                    }
                }
                if staging_root.exists()
                    && let Err(rollback_error) = fs::remove_dir_all(&staging_root)
                {
                    rollback_failures.push(format!(
                        "Failed to remove plugin staging directory during rollback: {rollback_error}"
                    ));
                }
                if rollback_failures.is_empty() {
                    Err(error)
                } else {
                    Err(VulcanPluginRuntimeError::new(format!(
                        "{error}; installation rollback failed: {}",
                        rollback_failures.join("; ")
                    )))
                }
            }
        }
    }

    /// Begins an atomic candidate swap for one disabled and fully drained user-installed plugin.
    /// 为一个已停用且完全排空的用户安装插件开始原子候选替换。
    ///
    /// # Parameters
    /// 参数
    /// `source_root` selects the replacement package, `expected_content_fingerprint` binds the prior authorization preview,
    /// `approved_permissions` must match its exact declaration, and `allow_registry_dependencies` records explicit network installation consent.
    /// `source_root` 选择替换插件包，`expected_content_fingerprint` 绑定此前的授权预览，
    /// `approved_permissions` 必须与其精确声明匹配，`allow_registry_dependencies` 记录显式网络依赖安装许可。
    ///
    /// # Returns
    /// 返回值
    /// Returns a pending transaction retaining the previous package until service-level validation commits it.
    /// 返回一份待提交事务，在 Service 层校验提交前保留旧插件包。
    pub fn begin_local_user_plugin_update(
        &self,
        source_root: &Path,
        expected_content_fingerprint: &str,
        mut approved_permissions: Vec<String>,
        allow_registry_dependencies: bool,
    ) -> Result<VulcanPluginUpdateTransaction, VulcanPluginRuntimeError> {
        let _registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan plugin update gate is poisoned.")
            })?;
        // source_descriptor is independently inspected before any installed package mutation.
        // source_descriptor 在任何已安装插件包变更前独立完成检查。
        let source_descriptor =
            inspect_user_plugin_package(source_root).map_err(VulcanPluginRuntimeError::new)?;
        if source_descriptor.content_fingerprint != expected_content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin package changed after update authorization inspection.",
            ));
        }
        let manifest = source_descriptor.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Local user plugin update has no valid manifest.")
        })?;
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Registry
            && !allow_registry_dependencies
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` update requires explicit registry dependency network authorization.",
                source_descriptor.id
            )));
        }
        let preview = build_permission_preview(&source_descriptor)?;
        approved_permissions.sort();
        if approved_permissions
            .windows(2)
            .any(|pair| pair[0] == pair[1])
            || approved_permissions != preview.permissions
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` update must approve the complete current capability set exactly once.",
                source_descriptor.id
            )));
        }
        // installed_snapshot pins the exact old generation while its package is moved into the transaction root.
        // installed_snapshot 在旧插件包移入事务根期间固定精确旧代次。
        let installed_snapshot = self.snapshot();
        let installed = installed_snapshot
            .plugins
            .get(&source_descriptor.id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` is not installed for this user.",
                    source_descriptor.id
                ))
            })?;
        if installed.origin != VulcanPluginPackageOrigin::UserInstalled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Bundled Vulcan plugin `{}` cannot be updated from a local user package.",
                source_descriptor.id
            )));
        }
        if installed.runtime_state != VulcanPluginRuntimeState::Disabled
            || installed.active_session_count != 0
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` must be disabled and fully drained before update.",
                source_descriptor.id
            )));
        }
        if installed.content_fingerprint == source_descriptor.content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` update package is identical to the installed package.",
                source_descriptor.id
            )));
        }
        let canonical_user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve user plugin root: {error}"
                ))
            })?;
        let destination = installed.package_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve installed user plugin package: {error}"
            ))
        })?;
        if destination.parent() != Some(canonical_user_root.as_path()) {
            return Err(VulcanPluginRuntimeError::new(
                "Installed user plugin package is outside the exact user plugin root.",
            ));
        }
        // authorization_path and installation_path are snapshotted verbatim for exact rollback.
        // authorization_path 与 installation_path 被逐字节快照以供精确回滚。
        let authorization_path =
            plugin_authorization_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let installation_path =
            plugin_installation_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let previous_authorization = fs::read(&authorization_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to snapshot existing plugin authorization before update: {error}"
            ))
        })?;
        let previous_installation = fs::read(&installation_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to snapshot existing plugin installation record before update: {error}"
            ))
        })?;
        let transaction_root = canonical_user_root.join(format!(".update-{}", Uuid::new_v4()));
        let staged_package = transaction_root.join(&source_descriptor.id);
        let previous_package = transaction_root.join("previous");
        fs::create_dir(&transaction_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to create Vulcan plugin update transaction root: {error}"
            ))
        })?;
        let candidate_version = source_descriptor.version.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Updated plugin has no validated version.")
        })?;
        let candidate_fingerprint = source_descriptor.content_fingerprint.clone();
        let update_result = (|| -> Result<(), VulcanPluginRuntimeError> {
            copy_plugin_directory_tree(&source_descriptor.package_root, &staged_package)
                .map_err(VulcanPluginRuntimeError::new)?;
            let staged_descriptor = inspect_user_plugin_package(&staged_package)
                .map_err(VulcanPluginRuntimeError::new)?;
            if staged_descriptor.content_fingerprint != source_descriptor.content_fingerprint {
                return Err(VulcanPluginRuntimeError::new(
                    "Local plugin package changed while its update was being staged.",
                ));
            }
            fs::rename(&destination, &previous_package).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to retain previous Vulcan plugin package for update rollback: {error}"
                ))
            })?;
            fs::rename(&staged_package, &destination).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to publish candidate Vulcan plugin package for validation: {error}"
                ))
            })?;
            let authorization = authorization_record_for_descriptor(
                &staged_descriptor,
                approved_permissions.clone(),
            )?;
            write_plugin_json_record(&authorization_path, &authorization, "authorization")?;
            let installation = VulcanPluginInstallationRecord {
                schema_version: 1,
                plugin_id: staged_descriptor.id.clone(),
                version: candidate_version.clone(),
                content_fingerprint: candidate_fingerprint.clone(),
                installed_at_ms: current_unix_time_ms(),
            };
            write_plugin_json_record(&installation_path, &installation, "installation")?;
            Ok(())
        })();
        if let Err(error) = update_result {
            // rollback_failures preserves every failure while restoring the old package and records.
            // rollback_failures 保留恢复旧插件包与记录期间的全部失败。
            let mut rollback_failures = Vec::new();
            if destination.exists()
                && previous_package.exists()
                && let Err(rollback_error) = fs::remove_dir_all(&destination)
            {
                rollback_failures.push(format!(
                    "Failed to remove candidate package during update rollback: {rollback_error}"
                ));
            }
            if previous_package.exists()
                && !destination.exists()
                && let Err(rollback_error) = fs::rename(&previous_package, &destination)
            {
                rollback_failures.push(format!(
                    "Failed to restore previous package during update rollback: {rollback_error}"
                ));
            }
            for (path, bytes, label) in [
                (
                    &authorization_path,
                    &previous_authorization,
                    "authorization",
                ),
                (&installation_path, &previous_installation, "installation"),
            ] {
                if let Err(rollback_error) = crate::atomic_file::write_file_atomically(path, bytes)
                {
                    rollback_failures.push(format!(
                        "Failed to restore previous plugin {label} record: {rollback_error}"
                    ));
                }
            }
            if transaction_root.exists()
                && let Err(rollback_error) = fs::remove_dir_all(&transaction_root)
            {
                rollback_failures.push(format!(
                    "Failed to remove update transaction root: {rollback_error}"
                ));
            }
            return if rollback_failures.is_empty() {
                Err(error)
            } else {
                Err(VulcanPluginRuntimeError::new(format!(
                    "{error}; update rollback failed: {}",
                    rollback_failures.join("; ")
                )))
            };
        }
        Ok(VulcanPluginUpdateTransaction {
            plugin_id: source_descriptor.id.clone(),
            transaction_root,
            destination,
            previous_authorization,
            previous_installation,
            candidate_version,
            candidate_fingerprint,
            permission_preview: preview,
        })
    }

    /// Verifies that a staged update candidate still matches the exact package inspected at transaction start.
    /// 验证暂存更新候选仍与事务开始时检查的精确插件包一致。
    ///
    /// # Parameters
    /// 参数
    /// `transaction` identifies the candidate that is about to enter an extension generation.
    /// `transaction` 标识即将进入扩展代次的候选插件包。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only when the complete package fingerprint remains unchanged.
    /// 仅当完整插件包指纹保持不变时返回成功。
    pub fn verify_local_user_plugin_update_candidate(
        &self,
        transaction: &VulcanPluginUpdateTransaction,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin update verification gate is poisoned.")
        })?;
        let fingerprint = crate::catalog::compute_package_fingerprint(&transaction.destination)
            .map_err(VulcanPluginRuntimeError::new)?;
        if fingerprint != transaction.candidate_fingerprint {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` candidate changed before update publication.",
                transaction.plugin_id
            )));
        }
        Ok(())
    }

    /// Commits one validated update by deleting its retained previous package and obsolete environments.
    /// 通过删除保留的旧插件包与过期环境提交一次已验证更新。
    ///
    /// # Parameters
    /// 参数
    /// `transaction` is the exact pending update previously returned by this manager.
    /// `transaction` 是此前由当前管理器返回的精确待提交更新。
    pub fn commit_local_user_plugin_update(
        &self,
        transaction: VulcanPluginUpdateTransaction,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin update commit gate is poisoned.")
        })?;
        // The second fingerprint check closes the gap between pre-publication verification and post-publication cleanup.
        // 第二次指纹检查封闭发布前复核与发布后清理之间的间隙。
        let fingerprint = crate::catalog::compute_package_fingerprint(&transaction.destination)
            .map_err(VulcanPluginRuntimeError::new)?;
        if fingerprint != transaction.candidate_fingerprint {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` candidate changed before update cleanup.",
                transaction.plugin_id
            )));
        }
        fs::remove_dir_all(&transaction.transaction_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to remove retained previous package for Vulcan plugin `{}`: {error}",
                transaction.plugin_id
            ))
        })?;
        remove_obsolete_plugin_execution_environments(
            &self.state,
            &transaction.plugin_id,
            &transaction.candidate_version,
            &transaction.candidate_fingerprint,
        )
        .map_err(VulcanPluginRuntimeError::new)
    }

    /// Rolls back one uncommitted update to the exact previous package and durable records.
    /// 将一次未提交更新回滚到精确的旧插件包与持久记录。
    ///
    /// # Parameters
    /// 参数
    /// `transaction` is the pending update whose candidate has not entered a published generation.
    /// `transaction` 是候选尚未进入已发布代次的待处理更新。
    pub fn rollback_local_user_plugin_update(
        &self,
        transaction: VulcanPluginUpdateTransaction,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin update rollback gate is poisoned.")
        })?;
        let previous_package = transaction.transaction_root.join("previous");
        let failed_candidate = transaction.transaction_root.join("failed-candidate");
        let authorization_path =
            plugin_authorization_state_path(&self.state.plugin_state_root, &transaction.plugin_id);
        let installation_path =
            plugin_installation_state_path(&self.state.plugin_state_root, &transaction.plugin_id);
        fs::rename(&transaction.destination, &failed_candidate).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to hide candidate package during update rollback: {error}"
            ))
        })?;
        if let Err(error) = fs::rename(&previous_package, &transaction.destination) {
            let restore_candidate_result = fs::rename(&failed_candidate, &transaction.destination);
            return Err(VulcanPluginRuntimeError::new(
                match restore_candidate_result {
                    Ok(()) => format!("Failed to restore previous plugin package: {error}"),
                    Err(candidate_error) => format!(
                        "Failed to restore previous plugin package: {error}; candidate restoration also failed: {candidate_error}"
                    ),
                },
            ));
        }
        let rollback_result = (|| -> Result<(), String> {
            crate::atomic_file::write_file_atomically(
                &authorization_path,
                &transaction.previous_authorization,
            )?;
            crate::atomic_file::write_file_atomically(
                &installation_path,
                &transaction.previous_installation,
            )?;
            if failed_candidate.exists() {
                fs::remove_dir_all(&failed_candidate).map_err(|error| {
                    format!("Failed to remove rolled-back candidate package: {error}")
                })?;
            }
            fs::remove_dir(&transaction.transaction_root).map_err(|error| {
                format!("Failed to remove completed update rollback root: {error}")
            })?;
            remove_plugin_execution_environment_generation(
                &self.state,
                &transaction.plugin_id,
                &transaction.candidate_version,
                &transaction.candidate_fingerprint,
            )
        })();
        rollback_result.map_err(VulcanPluginRuntimeError::new)
    }

    /// Removes one fully drained user-installed plugin package and its private state.
    /// 移除一个已完全排空的用户安装插件包及其私有状态。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact effective user plugin identity.
    /// `plugin_id` 是精确的生效用户插件身份。
    pub fn uninstall_user_plugin(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin uninstallation gate is poisoned.")
        })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        if plugin.origin != VulcanPluginPackageOrigin::UserInstalled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Bundled Vulcan plugin `{plugin_id}` cannot be uninstalled."
            )));
        }
        if plugin.active_session_count != 0
            || plugin.runtime_state != VulcanPluginRuntimeState::Disabled
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` must be disabled and fully drained before uninstall."
            )));
        }
        let canonical_user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve user plugin root: {error}"
                ))
            })?;
        let canonical_package = plugin.package_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve user plugin package: {error}"))
        })?;
        if canonical_package.parent() != Some(canonical_user_root.as_path()) {
            return Err(VulcanPluginRuntimeError::new(
                "User plugin package is outside the exact user plugin root.",
            ));
        }
        // state_paths are validated and snapshotted before the package leaves its discoverable name.
        // state_paths 在插件包离开可发现名称前完成校验与快照。
        let state_paths = [
            plugin_authorization_state_path(&self.state.plugin_state_root, plugin_id),
            plugin_installation_state_path(&self.state.plugin_state_root, plugin_id),
            plugin_config_state_path(&self.state.plugin_state_root, plugin_id),
        ];
        let state_snapshots = state_paths
            .iter()
            .map(|path| read_optional_plugin_state_file(path).map(|bytes| (path.clone(), bytes)))
            .collect::<Result<Vec<_>, _>>()?;
        // runtime_directory is canonicalized before any recursive removal authority is granted.
        // runtime_directory 在授予递归移除权限前完成规范化。
        let runtime_directory = self.state.plugin_runtime_root.join(plugin_id);
        let canonical_runtime_directory = if runtime_directory.exists() {
            let canonical_runtime_root =
                self.state
                    .plugin_runtime_root
                    .canonicalize()
                    .map_err(|error| {
                        VulcanPluginRuntimeError::new(format!(
                            "Failed to resolve Vulcan plugin runtime root: {error}"
                        ))
                    })?;
            let canonical = runtime_directory.canonicalize().map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve Vulcan plugin runtime directory: {error}"
                ))
            })?;
            if canonical.parent() != Some(canonical_runtime_root.as_path()) {
                return Err(VulcanPluginRuntimeError::new(
                    "User plugin runtime directory is outside the exact plugin runtime root.",
                ));
            }
            Some(canonical)
        } else {
            None
        };
        let tombstone =
            canonical_user_root.join(format!(".uninstall-{}-{}", plugin_id, Uuid::new_v4()));
        fs::rename(&canonical_package, &tombstone).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to hide Vulcan plugin `{plugin_id}` before uninstall: {error}"
            ))
        })?;
        let uninstall_result = (|| -> Result<(), VulcanPluginRuntimeError> {
            for path in &state_paths {
                remove_plugin_state_file(path)?;
            }
            if let Some(runtime_directory) = &canonical_runtime_directory {
                fs::remove_dir_all(runtime_directory).map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "Failed to remove Vulcan plugin `{plugin_id}` runtime environments: {error}"
                    ))
                })?;
            }
            fs::remove_dir_all(&tombstone).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to remove Vulcan plugin `{plugin_id}` package tombstone: {error}"
                ))
            })?;
            Ok(())
        })();
        if let Err(error) = uninstall_result {
            // rollback_failures preserves every restoration diagnostic instead of masking the original failure.
            // rollback_failures 保留每条恢复诊断，避免掩盖原始失败。
            let mut rollback_failures = Vec::new();
            for (path, bytes) in &state_snapshots {
                if let Some(bytes) = bytes
                    && !path.exists()
                    && let Err(restore_error) =
                        crate::atomic_file::write_file_atomically(path, bytes)
                {
                    rollback_failures.push(restore_error);
                }
            }
            if tombstone.exists()
                && let Err(restore_error) = fs::rename(&tombstone, &canonical_package)
            {
                rollback_failures.push(format!(
                    "Failed to restore user plugin package after uninstall error: {restore_error}"
                ));
            }
            return if rollback_failures.is_empty() {
                Err(error)
            } else {
                Err(VulcanPluginRuntimeError::new(format!(
                    "{error}; uninstall rollback failed: {}",
                    rollback_failures.join("; ")
                )))
            };
        }
        Ok(())
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
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut snapshot = load_vulcan_plugin_catalog(
            &self.state.vulcan_plugin_root,
            Some(&self.state.user_plugin_root),
            disabled_plugin_ids,
            enabled_plugin_ids,
            revision,
        );
        self.project_runtime_state(&mut snapshot)?;
        let snapshot = Arc::new(snapshot);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
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
    pub fn begin_enabling(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Enabling,
            &[
                VulcanPluginRuntimeState::Disabled,
                VulcanPluginRuntimeState::DisabledWithCleanupWarning,
            ],
        )
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
    pub fn begin_draining(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Draining,
            &[
                VulcanPluginRuntimeState::Enabled,
                VulcanPluginRuntimeState::DisabledWithCleanupWarning,
            ],
        )
    }

    /// Re-enters enabling from an uncommitted draining transition during service rollback.
    /// 在服务回滚期间从尚未提交的排空转换重新进入启用中状态。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact plugin whose disable transaction must be compensated.
    /// `plugin_id` 是必须补偿其停用事务的精确插件标识。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only after atomically replacing `Draining` with `Enabling`.
    /// 仅在原子地以 `Enabling` 替换 `Draining` 后返回成功。
    pub fn rollback_draining_to_enabling(
        &self,
        plugin_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.begin_lifecycle_transition(
            plugin_id,
            VulcanPluginRuntimeState::Enabling,
            &[VulcanPluginRuntimeState::Draining],
        )
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
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
            })?
            .remove(plugin_id);
        if cleanup_failures.is_empty() {
            self.clear_cleanup_diagnostics_if_fully_drained(plugin_id)?;
        } else {
            let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
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
    pub async fn probe_plugin(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if !matches!(
            plugin.availability,
            VulcanPluginAvailability::Enabled | VulcanPluginAvailability::Disabled
        ) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` is unavailable: {}",
                plugin.diagnostics.join("; ")
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        if !manifest.has_lua_runtime() {
            return Ok(());
        }
        // executionRoot prevents a post-catalog package mutation from changing probe code.
        // executionRoot 防止目录扫描后的插件包变更替换探测代码。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let probe_id = Uuid::new_v4().to_string();
        let (lease_id, sid, generation) = self
            .create_managed_lease(
                &probe_id,
                &execution_root,
                &execution_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::EnablingProbe,
            )
            .await?;
        // code probes the already loaded plugin through the fixed bridge ABI.
        // code 通过固定桥接 ABI 探测已加载插件。
        let code = String::from("local bridge = require('runtime.bridge')\nreturn bridge.probe()");
        let engine = self.runtime_engine()?;
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
                            "timeout_ms": VULCAN_PLUGIN_PROBE_TIMEOUT_MS,
                        })
                        .to_string(),
                    )
                    .map_err(VulcanPluginRuntimeError::new)?;
                let response = parse_engine_response(&raw, "probe Vulcan Plugin runtime lease")?;
                if response.get("result").and_then(Value::as_bool) != Some(true) {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin `{plugin_id}` probe did not return true."
                    )));
                }
                Ok(())
            }
        })
        .await;
        let retained_lease = RetainedVulcanPluginLease {
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
                .map_err(VulcanPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close Vulcan Plugin probe lease").map(|_| ())
        })
        .await;
        match (probe_result, close_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(probe_error), Ok(())) => Err(probe_error),
            (probe_result, Err(close_error)) => {
                let diagnostic = format!(
                    "Vulcan Plugin `{stable_plugin_id}` probe lease cleanup failed and remains retained: {close_error}"
                );
                self.retain_failed_lease(retained_lease, diagnostic.clone())?;
                let message = match probe_result {
                    Ok(()) => diagnostic,
                    Err(probe_error) => format!("{probe_error}; {diagnostic}"),
                };
                Err(VulcanPluginRuntimeError::new(message))
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
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        let sessions = lock_sessions(&self.state.sessions)?
            .values()
            .filter(|session| session.plugin_id == plugin_id)
            .cloned()
            .collect::<Vec<_>>();
        let mut report = self.drain_sessions(sessions).await?;
        self.drain_persistent_runtime(
            plugin_id,
            VulcanPluginPersistentRuntimeKind::Hook,
            &mut report,
        )
        .await?;
        self.drain_persistent_runtime(
            plugin_id,
            VulcanPluginPersistentRuntimeKind::UiAction,
            &mut report,
        )
        .await?;
        let retained = self.take_retained_leases(|lease| lease.plugin_id == plugin_id)?;
        report.merge(self.drain_retained_leases(retained).await?);
        Ok(report)
    }

    /// Drains one category-isolated persistent runtime into the shared lifecycle report.
    /// 把一个按类别隔离的持久运行时排空到共享生命周期报告中。
    async fn drain_persistent_runtime(
        &self,
        plugin_id: &str,
        runtime_kind: VulcanPluginPersistentRuntimeKind,
        report: &mut VulcanPluginDrainReport,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _lifecycle = match runtime_kind {
            VulcanPluginPersistentRuntimeKind::Hook => {
                self.state.hook_runtime_initialization_gate.lock().await
            }
            VulcanPluginPersistentRuntimeKind::UiAction => {
                self.state
                    .ui_action_runtime_initialization_gate
                    .lock()
                    .await
            }
        };
        let runtimes = match runtime_kind {
            VulcanPluginPersistentRuntimeKind::Hook => &self.state.hook_runtimes,
            VulcanPluginPersistentRuntimeKind::UiAction => &self.state.ui_action_runtimes,
        };
        let runtime = lock_hook_runtimes(runtimes)?.get(plugin_id).cloned();
        let Some(runtime) = runtime else {
            return Ok(());
        };
        match self
            .close_persistent_runtime(Arc::clone(&runtime), runtime_kind)
            .await
        {
            Ok(shutdown_diagnostic) => {
                lock_hook_runtimes(runtimes)?.remove(plugin_id);
                match runtime_kind {
                    VulcanPluginPersistentRuntimeKind::Hook => {
                        report.closed_hook_runtimes += 1;
                    }
                    VulcanPluginPersistentRuntimeKind::UiAction => {
                        report.closed_ui_action_runtimes += 1;
                    }
                }
                if let Some(diagnostic) = shutdown_diagnostic {
                    report.failures.push(diagnostic);
                }
            }
            Err(error) => {
                runtime.closing.store(false, Ordering::Release);
                let diagnostic = format!(
                    "Plugin `{plugin_id}` persistent {} runtime cleanup failed and remains retained: {error}",
                    runtime_kind.label()
                );
                self.record_cleanup_failure(plugin_id, diagnostic.clone())?;
                report.failures.push(diagnostic);
            }
        }
        Ok(())
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
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
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
        sessions: Vec<Arc<ManagedVulcanPluginSession>>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        let mut report = VulcanPluginDrainReport::default();
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
            let close_execution = required_lua_runtime(&session.manifest)?
                .tools
                .iter()
                .find(|tool| tool.execution.kind == VulcanPluginToolExecutionKind::SessionClose)
                .map(|tool| tool.execution.clone())
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin `{plugin_id}` declares no session-close operation."
                    ))
                })?;
            let payload = plugin_payload(&close_execution.operation, Map::new())?;
            let host_timeout_ms = close_execution
                .timeout_ms
                .checked_add(close_execution.timeout_grace_ms)
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Vulcan Plugin close timeout overflowed.")
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
        leases: Vec<RetainedVulcanPluginLease>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        let mut report = VulcanPluginDrainReport::default();
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
        predicate: impl Fn(&RetainedVulcanPluginLease) -> bool,
    ) -> Result<Vec<RetainedVulcanPluginLease>, VulcanPluginRuntimeError> {
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
        lease: RetainedVulcanPluginLease,
        diagnostic: String,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let plugin_id = lease.plugin_id.clone();
        let lease_id = lease.lease_id.clone();
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        if retained.contains_key(&lease_id) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Retained Vulcan Plugin lease id `{lease_id}` already exists."
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
        lease: RetainedVulcanPluginLease,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let lease_id = lease.lease_id.clone();
        let mut retained = lock_retained_leases(&self.state.retained_leases)?;
        if retained.contains_key(&lease_id) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Retained Vulcan Plugin lease id `{lease_id}` was restored concurrently."
            )));
        }
        retained.insert(lease_id, lease);
        Ok(())
    }

    /// Retains one failed-cleanup session lease while preserving its reserved capacity for retry.
    /// 保留一个清理失败的会话租约，并为重试继续占用其已预留容量。
    fn retain_failed_session_lease(
        &self,
        session: &ManagedVulcanPluginSession,
        diagnostic: String,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.retain_failed_lease(
            RetainedVulcanPluginLease {
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
    ) -> Result<(), VulcanPluginRuntimeError> {
        let mut diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
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
    fn publish_cleanup_diagnostics(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let cleanup_diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let Some(plugin_diagnostics) = cleanup_diagnostics.get(plugin_id) else {
            return Ok(());
        };
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        if plugin.runtime_state == VulcanPluginRuntimeState::Disabled {
            plugin.runtime_state = VulcanPluginRuntimeState::DisabledWithCleanupWarning;
        }
        for diagnostic in plugin_diagnostics {
            if !plugin.diagnostics.contains(diagnostic) {
                plugin.diagnostics.push(diagnostic.clone());
            }
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }

    /// Clears stale cleanup diagnostics only after every resource owned by one plugin is gone.
    /// 仅在单个插件拥有的全部资源均已消失后清除陈旧清理诊断。
    fn clear_cleanup_diagnostics_if_fully_drained(
        &self,
        plugin_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let has_sessions = lock_sessions(&self.state.sessions)?
            .values()
            .any(|session| session.plugin_id == plugin_id);
        let has_retained_leases = lock_retained_leases(&self.state.retained_leases)?
            .values()
            .any(|lease| lease.plugin_id == plugin_id);
        let has_hook_runtime =
            lock_hook_runtimes(&self.state.hook_runtimes)?.contains_key(plugin_id);
        let has_ui_action_runtime =
            lock_hook_runtimes(&self.state.ui_action_runtimes)?.contains_key(plugin_id);
        let has_active_count = self
            .state
            .active_session_counts
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
            })?
            .get(plugin_id)
            .copied()
            .unwrap_or_default()
            > 0;
        if has_sessions
            || has_retained_leases
            || has_hook_runtime
            || has_ui_action_runtime
            || has_active_count
        {
            return Ok(());
        }
        let removed_diagnostics = self
            .state
            .cleanup_diagnostics
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
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
        if plugin.runtime_state == VulcanPluginRuntimeState::DisabledWithCleanupWarning {
            plugin.runtime_state = VulcanPluginRuntimeState::Disabled;
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
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
    pub fn snapshot(&self) -> Arc<VulcanPluginCatalogSnapshot> {
        self.state
            .snapshot
            .read()
            .map(|snapshot| Arc::clone(&snapshot))
            .unwrap_or_else(|poisoned| Arc::clone(&poisoned.into_inner()))
    }

    /// Prepares one versioned private process environment owned by Vulcan Code.
    /// 准备一个由 Vulcan Code 持有的版本化插件私有进程环境。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact effective catalog identity to prepare.
    /// `plugin_id` 是需要准备的精确生效目录身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the canonical immutable environment directory for the current package fingerprint.
    /// 返回当前插件包指纹对应的规范化不可变环境目录。
    pub fn prepare_plugin_environment(
        &self,
        plugin_id: &str,
    ) -> Result<PathBuf, VulcanPluginRuntimeError> {
        // snapshot holds one stable effective generation while its trusted root is resolved.
        // snapshot 在解析可信根期间持有一个稳定的生效代次。
        let snapshot = self.snapshot();
        // plugin is the exact descriptor selected from the stable catalog snapshot.
        // plugin 是从稳定目录快照中选择的精确描述符。
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        if !manifest.has_lua_runtime()
            && manifest.mcp.servers.is_empty()
            && manifest.execution.dependencies.install_mode
                == VulcanPluginDependencyInstallMode::Bundled
            && manifest.execution.native_binaries.is_empty()
            && manifest.execution.node_entries.is_empty()
            && manifest.execution.python_entries.is_empty()
        {
            return Ok(plugin.package_root.clone());
        }
        ensure_plugin_execution_environment(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)
    }

    /// Resolves the immutable package root used to execute or render one effective plugin.
    /// 解析用于执行或渲染一个生效插件的不可变包根目录。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact effective catalog identity to resolve.
    /// `plugin_id` 是需要解析的精确生效目录身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns bundled source directly and an integrity-checked private copy for user plugins.
    /// 系统内置插件直接返回源目录，用户插件返回经过完整性校验的私有副本。
    pub fn plugin_execution_package_root(
        &self,
        plugin_id: &str,
    ) -> Result<PathBuf, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        plugin_execution_package_root(&self.state, plugin).map_err(VulcanPluginRuntimeError::new)
    }

    /// Resolves the exact process launcher for one enabled plugin-owned stdio MCP server.
    /// 解析一个已启用插件自有 stdio MCP 服务器的精确进程启动器。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` and `server_id` select the immutable catalog declaration to materialize.
    /// `plugin_id` 与 `server_id` 选择需要物化的不可变目录声明。
    ///
    /// # Returns
    /// 返回值
    /// Returns a contained Shell-free launch specification backed only by managed runtimes.
    /// 返回一份仅由受管运行时承载、受目录约束且不使用 Shell 的启动规格。
    pub fn resolve_mcp_stdio_launcher(
        &self,
        plugin_id: &str,
        server_id: &str,
    ) -> Result<VulcanPluginMcpStdioLaunchSpec, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let server_key = format!("{plugin_id}:{server_id}");
        let server = snapshot
            .enabled_mcp_servers
            .get(&server_key)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin MCP server `{server_key}` is not enabled."
                ))
            })?;
        self.resolve_mcp_stdio_launcher_descriptor(server)
    }

    /// Resolves the exact process launcher for one validated candidate stdio MCP descriptor before publication.
    /// 在发布前为一个已校验的候选 stdio MCP 描述符解析精确进程启动器。
    ///
    /// # Parameters
    /// 参数
    /// `server` is the immutable descriptor from the candidate catalog and must match the installed package generation.
    /// `server` 是候选目录中的不可变描述符，且必须匹配已安装包代次。
    ///
    /// # Returns
    /// 返回值
    /// Returns a contained Shell-free launch specification without requiring the candidate server to be enabled already.
    /// 返回一份受目录约束且不使用 Shell 的启动规格，不要求候选服务器已经启用。
    pub fn resolve_mcp_stdio_launcher_descriptor(
        &self,
        server: &crate::VulcanPluginMcpServerDescriptor,
    ) -> Result<VulcanPluginMcpStdioLaunchSpec, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&server.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not installed.",
                server.plugin_id
            ))
        })?;
        if server.plugin_generation != plugin.generation {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin MCP server `{}:{}` belongs to a stale plugin generation.",
                server.plugin_id, server.id
            )));
        }
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no valid manifest.",
                server.plugin_id
            ))
        })?;
        let crate::VulcanPluginMcpTransportManifest::Stdio { launcher, .. } =
            &server.manifest.transport
        else {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin MCP server `{}:{}` is not a stdio server.",
                server.plugin_id, server.id
            )));
        };
        resolve_plugin_mcp_stdio_launcher(&self.state, plugin, manifest, launcher)
            .map_err(VulcanPluginRuntimeError::new)
    }

    /// Invokes one optional manifest-declared lifecycle callback through a temporary bridge lease.
    /// 通过临时桥接租约调用一个可选的清单声明生命周期回调。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the exact package and `event` selects its declared callback.
    /// `plugin_id` 选择精确插件包，`event` 选择其声明的回调。
    ///
    /// # Returns
    /// 返回值
    /// Returns success when no callback is declared or after the callback and lease cleanup succeed.
    /// 未声明回调时直接成功；否则在回调与租约清理均成功后返回成功。
    pub async fn invoke_lifecycle(
        &self,
        plugin_id: &str,
        event: VulcanPluginLifecycleEvent,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` has no valid manifest."
            ))
        })?;
        let Some(lua_runtime) = manifest.lua_runtime.as_ref() else {
            return Ok(());
        };
        let (function, event_key) = match event {
            VulcanPluginLifecycleEvent::Install => {
                (lua_runtime.lifecycle.install.clone(), "install")
            }
            VulcanPluginLifecycleEvent::Enable => (lua_runtime.lifecycle.enable.clone(), "enable"),
            VulcanPluginLifecycleEvent::EnableRollback => {
                (lua_runtime.lifecycle.disable.clone(), "enable_rollback")
            }
            VulcanPluginLifecycleEvent::Disable => {
                (lua_runtime.lifecycle.disable.clone(), "disable")
            }
            VulcanPluginLifecycleEvent::Uninstall => {
                (lua_runtime.lifecycle.uninstall.clone(), "uninstall")
            }
        };
        let Some(function) = function else {
            return Ok(());
        };
        // environmentRoot provides lifecycle-owned writable dependency and process state.
        // environmentRoot 提供生命周期持有的可写依赖与进程状态。
        let environment_root = ensure_plugin_execution_environment(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        // executionRoot pins lifecycle Lua code to the trusted package generation.
        // executionRoot 把生命周期 Lua 代码固定到可信插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        self.reserve_session_slot(plugin_id, lua_runtime.max_active_sessions)?;
        let lifecycle_id = format!("lifecycle-{event_key}-{}", Uuid::new_v4());
        let lease_result = self
            .create_managed_lease(
                &lifecycle_id,
                &execution_root,
                &environment_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::from(event),
            )
            .await;
        let (lease_id, sid, generation) = match lease_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(plugin_id);
                return Err(error);
            }
        };
        let callback_result = self
            .evaluate_bridge_function(
                &lease_id,
                &sid,
                generation,
                &function,
                json!({
                    "event": event_key,
                    "plugin_id": plugin_id,
                    "environment_root": environment_root,
                }),
                VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS,
            )
            .await
            .map(|_| ());
        let close_result = self.close_raw_lease(&lease_id, &sid, generation).await;
        self.release_session_slot(plugin_id);
        match (callback_result, close_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(callback_error), Err(close_error)) => Err(VulcanPluginRuntimeError::new(format!(
                "{callback_error}; lifecycle lease cleanup failed: {close_error}"
            ))),
        }
    }

    /// Invokes one enabled manifest-declared plugin business action through a temporary lease.
    /// 通过临时租约调用一个已启用且由清单声明的插件业务 action。
    ///
    /// # Parameters
    /// 参数
    /// `request` binds plugin identity, action identity, caller, arguments, and workspace root.
    /// `request` 绑定插件身份、action 身份、调用方、参数与工作区根。
    ///
    /// # Returns
    /// 返回值
    /// Returns the schema-validated action result after persistent dispatch or temporary-lease cleanup.
    /// 在持久调度或临时租约清理后返回通过 Schema 校验的 action 结果。
    pub async fn invoke_action(
        &self,
        request: VulcanPluginActionRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&request.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not installed.",
                request.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is not enabled.",
                request.plugin_id
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no valid manifest.",
                request.plugin_id
            ))
        })?;
        let lua_runtime = required_lua_runtime(&manifest)?;
        let action = lua_runtime
            .actions
            .iter()
            .find(|action| action.id == request.action_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` does not declare action `{}`.",
                    request.plugin_id, request.action_id
                ))
            })?;
        if !action.callers.contains(&request.caller) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin action `{}` does not authorize caller `{:?}`.",
                action.id, request.caller
            )));
        }
        if request.reuse_enabled_runtime
            && request.caller != crate::VulcanPluginActionCaller::PluginUi
        {
            return Err(VulcanPluginRuntimeError::new(
                "Persistent enabled-plugin action runtimes are reserved for global plugin UI calls.",
            ));
        }
        // executionRoot pins action code and schemas to one authorized package generation.
        // executionRoot 把 Action 代码与 Schema 固定到一个已授权插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        validate_plugin_action_value(
            &execution_root,
            action.input_schema_file.as_deref(),
            &request.arguments,
            "action input",
        )?;
        let workspace_root = request.workspace_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve workspace root: {error}"))
        })?;
        if request.reuse_enabled_runtime {
            let plugin_generation = plugin.generation;
            let runtime = self
                .ensure_enabled_plugin_runtime(
                    &plugin.id,
                    plugin_generation,
                    VulcanPluginPersistentRuntimeKind::UiAction,
                )
                .await?;
            let _operation = runtime.operation_gate.lock().await;
            if runtime.closing.load(Ordering::Acquire)
                || runtime.plugin_generation != plugin_generation
            {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin `{}` persistent runtime is closing or stale.",
                    plugin.id
                )));
            }
            let result = self
                .evaluate_bridge_function(
                    &runtime.lease_id,
                    &runtime.sid,
                    runtime.generation,
                    &action.function,
                    json!({ "arguments": request.arguments }),
                    action.timeout_ms,
                )
                .await?;
            validate_plugin_action_value(
                &execution_root,
                action.output_schema_file.as_deref(),
                &result,
                "action output",
            )?;
            return Ok(result);
        }
        self.reserve_session_slot(&plugin.id, lua_runtime.max_active_sessions)?;
        let action_lease_id = format!("action-{}", Uuid::new_v4());
        let create_result = self
            .create_managed_lease(
                &action_lease_id,
                &execution_root,
                &workspace_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .await;
        let (lease_id, sid, generation) = match create_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(&plugin.id);
                return Err(error);
            }
        };
        let invocation_result = self
            .evaluate_bridge_function(
                &lease_id,
                &sid,
                generation,
                &action.function,
                json!({ "arguments": request.arguments }),
                action.timeout_ms,
            )
            .await;
        let close_result = self.close_raw_lease(&lease_id, &sid, generation).await;
        match close_result {
            Ok(()) => self.release_session_slot(&plugin.id),
            Err(close_error) => {
                let diagnostic = format!(
                    "Vulcan plugin action lease `{lease_id}` cleanup failed and remains retained: {close_error}"
                );
                self.retain_failed_lease(
                    RetainedVulcanPluginLease {
                        plugin_id: plugin.id.clone(),
                        owner_session_id: None,
                        lease_id,
                        sid,
                        generation,
                        reserved_session_slot: true,
                    },
                    diagnostic.clone(),
                )?;
                return Err(VulcanPluginRuntimeError::new(match invocation_result {
                    Ok(_) => diagnostic,
                    Err(invoke_error) => format!("{invoke_error}; {diagnostic}"),
                }));
            }
        }
        let result = invocation_result?;
        validate_plugin_action_value(
            &execution_root,
            action.output_schema_file.as_deref(),
            &result,
            "action output",
        )?;
        Ok(result)
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
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let tool = self
            .snapshot()
            .enabled_tools
            .get(&request.tool_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin tool `{}` is not enabled in the current catalog.",
                    request.tool_id
                ))
            })?;
        self.ensure_current_tool_generation(&tool)?;
        match tool.manifest.execution.kind {
            VulcanPluginToolExecutionKind::Stateless => self.invoke_stateless(&tool, request).await,
            VulcanPluginToolExecutionKind::SessionOpen => self.open_session(&tool, request).await,
            VulcanPluginToolExecutionKind::SessionCall => self.call_session(&tool, request).await,
            VulcanPluginToolExecutionKind::SessionClose => self.close_session(&tool, request).await,
        }
    }

    /// Invokes one tool through a temporary lease that is always closed before returning.
    /// 通过一个返回前始终关闭的临时租约调用单个工具。
    async fn invoke_stateless(
        &self,
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let arguments = require_argument_object(request.arguments)?;
        if arguments.contains_key(VULCAN_PLUGIN_SESSION_ID_ARGUMENT) {
            return Err(VulcanPluginRuntimeError::new(
                "A stateless Vulcan Plugin tool must not accept the host-owned session_id argument.",
            ));
        }
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        let workspace_root = request.workspace_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve workspace root: {error}"))
        })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is missing from the current catalog.",
                tool.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` has no valid manifest.",
                tool.plugin_id
            ))
        })?;
        let lua_runtime = required_lua_runtime(&manifest)?;
        // executionRoot pins stateless tool code to one trusted package generation.
        // executionRoot 把无状态工具代码固定到一个可信插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        self.reserve_session_slot(&tool.plugin_id, lua_runtime.max_active_sessions)?;
        let temporary_id = Uuid::new_v4().to_string();
        let create_result = self
            .create_managed_lease(
                &format!("stateless-{temporary_id}"),
                &execution_root,
                &workspace_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .await;
        let (lease_id, sid, generation) = match create_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(&tool.plugin_id);
                return Err(error);
            }
        };
        let session = Arc::new(ManagedVulcanPluginSession {
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
                "Stateless Vulcan Plugin lease `{}` cleanup failed and remains retained: {cleanup_error}",
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
            return Err(VulcanPluginRuntimeError::new(failures.join("; ")));
        }
        self.release_session_slot(&tool.plugin_id);
        match (invocation_result, shutdown_result) {
            (Ok(result), Ok(_)) => Ok(result),
            (Err(error), Ok(_)) | (Ok(_), Err(error)) => Err(error),
            (Err(invocation_error), Err(shutdown_error)) => Err(VulcanPluginRuntimeError::new(
                format!("{invocation_error}; {shutdown_error}"),
            )),
        }
    }

    /// Prewarms one latency-sensitive hook runtime for every enabled plugin that contributes hooks.
    /// 为每个提供 Hook 的已启用插件预热一个延迟敏感 Hook 运行时。
    ///
    /// Returns the deterministically ordered plugin identifiers whose runtimes are ready.
    /// 返回运行时已就绪的确定性排序插件标识列表。
    pub async fn prewarm_enabled_plugin_runtimes(
        &self,
    ) -> Result<Vec<String>, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin_ids = snapshot
            .plugins
            .values()
            .filter(|plugin| {
                plugin.runtime_state == VulcanPluginRuntimeState::Enabled
                    && plugin.runtime_execution_available
                    && !plugin.hooks.is_empty()
            })
            .map(|plugin| plugin.id.clone())
            .collect::<BTreeSet<_>>();
        let mut ready_plugin_ids = Vec::with_capacity(plugin_ids.len());
        for plugin_id in &plugin_ids {
            if self.prewarm_plugin_runtime(plugin_id).await? {
                ready_plugin_ids.push(plugin_id.clone());
            }
        }
        Ok(ready_plugin_ids)
    }

    /// Prewarms the persistent hook runtime for one enabled plugin when it contributes hooks.
    /// 当指定的已启用插件提供 Hook 时，预热其持久 Hook 运行时。
    ///
    /// Returns true when an eligible enabled plugin exists and its runtime is ready, otherwise false.
    /// 当存在符合条件的已启用插件且其运行时已就绪时返回 true，否则返回 false。
    pub async fn prewarm_plugin_runtime(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let Some(plugin) = snapshot.plugins.get(plugin_id) else {
            return Ok(false);
        };
        let eligible = plugin.runtime_state == VulcanPluginRuntimeState::Enabled
            && plugin.runtime_execution_available
            && !plugin.hooks.is_empty();
        if !eligible {
            return Ok(false);
        }
        let generation = plugin.generation;
        self.ensure_enabled_plugin_runtime(
            plugin_id,
            generation,
            VulcanPluginPersistentRuntimeKind::Hook,
        )
        .await?;
        Ok(true)
    }

    /// Invokes one validated hook registration with the supplied immutable invocation context.
    /// 使用给定的不可变调用上下文执行一个已验证的 Hook 注册项。
    ///
    /// The hook runtime is reused for the enabled plugin generation and is closed on disable.
    /// Hook 运行时会在插件启用代次内复用，并在停用时关闭。
    pub async fn invoke_hook(
        &self,
        hook: &VulcanPluginHookDescriptor,
        invocation: HookInvocation,
    ) -> Result<HookResult, VulcanPluginRuntimeError> {
        self.ensure_current_hook_generation(hook)?;
        let runtime = self.ensure_hook_runtime(hook).await?;
        let _operation = runtime.operation_gate.lock().await;
        if runtime.closing.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` hook runtime is closing.",
                hook.plugin_id
            )));
        }
        self.ensure_current_hook_generation(hook)?;
        if runtime.plugin_generation != hook.plugin_generation {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` hook runtime belongs to a stale lifecycle generation.",
                hook.plugin_id
            )));
        }
        // trusted_workspace_root is the normalized host path authorized for Lua bridge operations.
        // trusted_workspace_root 是授权给 Lua bridge 操作的规范化宿主路径。
        let trusted_workspace_root = trusted_hook_workspace_root(&invocation.context)?;
        // lua_context prevents the original verbatim workspace metadata from entering the Lua invocation payload.
        // lua_context 防止原始 verbatim 工作区元数据进入 Lua 调用载荷。
        let mut lua_context = invocation.context;
        // workspace_root borrows the already-normalized Lua boundary value when this hook has workspace scope.
        // workspace_root 在当前 Hook 具有工作区作用域时借用已完成规范化的 Lua 边界值。
        if let Some(workspace_root) = trusted_workspace_root.as_ref() {
            lua_context.metadata.insert(
                String::from("workspace_root"),
                Value::String(workspace_root.clone()),
            );
        }
        let invocation_json = serde_json::to_string(&json!({
            "registration": invocation.registration,
            "context": lua_context,
            "payload": invocation.payload,
        }))
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to encode Vulcan Plugin hook invocation: {error}"
            ))
        })?;
        let function = hook.manifest.function.clone();
        // code dispatches the validated hook function through the fixed bridge ABI.
        // code 通过固定桥接 ABI 调度已校验的 hook 函数。
        let code =
            String::from("local bridge = require('runtime.bridge')\nreturn bridge.invoke(args)");
        let lease_id = runtime.lease_id.clone();
        let generation = runtime.generation;
        let host_timeout_ms = hook
            .manifest
            .timeout_ms
            .unwrap_or(VULCAN_PLUGIN_DEFAULT_HOOK_TIMEOUT_MS);
        let engine = self.runtime_engine()?;
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
                            "function_name": function,
                            "workspace_root": trusted_workspace_root,
                            "plugin_args": {
                                "invocation_json": invocation_json,
                                "host_timeout_ms": host_timeout_ms,
                            },
                        },
                        "timeout_ms": host_timeout_ms,
                    })
                    .to_string(),
                )
                .map_err(VulcanPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "evaluate Vulcan Plugin hook lease")?;
            parse_plugin_result(&response)
        })
        .await?;
        serde_json::from_value(result).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to decode Vulcan Plugin hook result: {error}"
            ))
        })
    }

    /// Resolves or creates the persistent Lua lease for one current plugin generation.
    /// 解析或创建一个当前插件代次使用的持久 Lua 租约。
    async fn ensure_hook_runtime(
        &self,
        hook: &VulcanPluginHookDescriptor,
    ) -> Result<Arc<ManagedVulcanPluginHookRuntime>, VulcanPluginRuntimeError> {
        self.ensure_current_hook_generation(hook)?;
        self.ensure_enabled_plugin_runtime(
            &hook.plugin_id,
            hook.plugin_generation,
            VulcanPluginPersistentRuntimeKind::Hook,
        )
        .await
    }

    /// Resolves or creates one category-isolated persistent Lua lease for an enabled plugin.
    /// 为一个已启用插件解析或创建按类别隔离的持久 Lua 租约。
    async fn ensure_enabled_plugin_runtime(
        &self,
        plugin_id: &str,
        plugin_generation: u64,
        runtime_kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<Arc<ManagedVulcanPluginHookRuntime>, VulcanPluginRuntimeError> {
        let _initialization = match runtime_kind {
            VulcanPluginPersistentRuntimeKind::Hook => {
                self.state.hook_runtime_initialization_gate.lock().await
            }
            VulcanPluginPersistentRuntimeKind::UiAction => {
                self.state
                    .ui_action_runtime_initialization_gate
                    .lock()
                    .await
            }
        };
        let runtimes = match runtime_kind {
            VulcanPluginPersistentRuntimeKind::Hook => &self.state.hook_runtimes,
            VulcanPluginPersistentRuntimeKind::UiAction => &self.state.ui_action_runtimes,
        };
        let existing_runtime = {
            let runtimes = lock_hook_runtimes(runtimes)?;
            runtimes.get(plugin_id).cloned()
        };
        if let Some(runtime) = existing_runtime {
            if runtime.plugin_generation == plugin_generation
                && !runtime.closing.load(Ordering::Acquire)
            {
                return Ok(runtime);
            }
            if let Some(diagnostic) = self
                .close_persistent_runtime(Arc::clone(&runtime), runtime_kind)
                .await?
            {
                self.record_cleanup_failure(plugin_id, diagnostic)?;
            }
            lock_hook_runtimes(runtimes)?.remove(plugin_id);
        }
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` persistent {} runtime belongs to a stale or disabled lifecycle generation.",
                runtime_kind.label()
            )));
        }
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` has no valid manifest.",
                plugin_id
            ))
        })?;
        // executionRoot pins the category-isolated persistent runtime to one trusted package generation.
        // executionRoot 把按类别隔离的持久运行时固定到一个可信插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let (lease_id, sid, generation) = self
            .create_managed_lease(
                runtime_kind.lease_id(),
                &execution_root,
                &execution_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .await?;
        let runtime = Arc::new(ManagedVulcanPluginHookRuntime {
            plugin_id: plugin_id.to_string(),
            plugin_generation,
            lease_id,
            sid,
            generation,
            manifest,
            operation_gate: AsyncMutex::new(()),
            closing: AtomicBool::new(false),
        });
        lock_hook_runtimes(runtimes)?.insert(plugin_id.to_string(), Arc::clone(&runtime));
        Ok(runtime)
    }

    /// Stops one category-isolated persistent runtime and always attempts to release its System lease.
    /// 停止一个按类别隔离的持久运行时，并始终尝试释放其系统租约。
    async fn close_persistent_runtime(
        &self,
        runtime: Arc<ManagedVulcanPluginHookRuntime>,
        runtime_kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<Option<String>, VulcanPluginRuntimeError> {
        runtime.closing.store(true, Ordering::Release);
        let _operation = runtime.operation_gate.lock().await;
        let shutdown_result = if runtime_kind == VulcanPluginPersistentRuntimeKind::Hook
            && let Some(function) = runtime
                .manifest
                .lua_runtime
                .as_ref()
                .and_then(|lua_runtime| lua_runtime.hook_shutdown_function.clone())
        {
            let plugin_id = runtime.plugin_id.clone();
            // code dispatches graceful hook shutdown through the fixed bridge ABI.
            // code 通过固定桥接 ABI 调度 hook 的优雅关闭。
            let code = String::from(
                "local bridge = require('runtime.bridge')\nreturn bridge.invoke(args)",
            );
            let lease_id = runtime.lease_id.clone();
            let sid = runtime.sid.clone();
            let generation = runtime.generation;
            let engine = self.runtime_engine()?;
            call_engine_blocking(engine, move |engine| {
                let raw = engine
                    .eval_system_runtime_lease_json(
                        &json!({
                            "lease_id": lease_id,
                            "sid": sid,
                            "generation": generation,
                            "code": code,
                            "args": {
                                "function_name": function,
                                "plugin_args": {
                                    "payload_json": json!({
                                        "event": "hook_runtime_shutdown",
                                        "plugin_id": plugin_id,
                                    })
                                    .to_string(),
                                    "host_timeout_ms": VULCAN_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS,
                                },
                            },
                            "timeout_ms": VULCAN_PLUGIN_HOOK_SHUTDOWN_TIMEOUT_MS,
                        })
                        .to_string(),
                    )
                    .map_err(VulcanPluginRuntimeError::new)?;
                let response = parse_engine_response(&raw, "shutdown Vulcan Plugin hook lease")?;
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
                "Plugin `{}` persistent {} lease closed after graceful shutdown failed: {error}",
                runtime.plugin_id,
                runtime_kind.label()
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
    ) -> Result<(), VulcanPluginRuntimeError> {
        let lease_id = lease_id.to_owned();
        let sid = sid.to_owned();
        let engine = self.runtime_engine()?;
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
                .map_err(VulcanPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close Vulcan Plugin hook lease").map(|_| ())
        })
        .await
    }

    /// Schedules idle cleanup without retaining the plugin manager past host shutdown.
    /// 调度空闲清理，同时不让插件管理器越过宿主关闭继续存活。
    fn schedule_session_idle_reaper(&self, session: Arc<ManagedVulcanPluginSession>) {
        let Some(timeout_sec) = session
            .manifest
            .lua_runtime
            .as_ref()
            .and_then(|runtime| runtime.session_idle_timeout_sec)
        else {
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
                let manager = VulcanPluginRuntimeManager { state };
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
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let arguments = require_argument_object(request.arguments)?;
        if arguments.contains_key(VULCAN_PLUGIN_SESSION_ID_ARGUMENT) {
            return Err(VulcanPluginRuntimeError::new(
                "A session-open tool must not accept the host-owned session_id argument.",
            ));
        }
        let workspace_root = request.workspace_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve workspace root: {error}"))
        })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is missing from the current catalog.",
                tool.plugin_id
            ))
        })?;
        let manifest = plugin.manifest.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` has no valid manifest.",
                tool.plugin_id
            ))
        })?;
        let lua_runtime = required_lua_runtime(&manifest)?;
        // executionRoot pins the opened plugin session to one trusted package generation.
        // executionRoot 把打开的插件会话固定到一个可信插件包代次。
        let execution_root = plugin_execution_package_root(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        let timeout_ms = resolve_host_timeout(tool, &arguments)?;
        let payload = plugin_payload(&tool.manifest.execution.operation, arguments)?;
        self.reserve_session_slot(&tool.plugin_id, lua_runtime.max_active_sessions)?;
        let session_id = Uuid::new_v4().to_string();
        let create_result = self
            .create_managed_lease(
                &session_id,
                &execution_root,
                &workspace_root,
                &manifest,
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .await;
        let (lease_id, sid, generation) = match create_result {
            Ok(lease) => lease,
            Err(error) => {
                self.release_session_slot(&tool.plugin_id);
                return Err(error);
            }
        };
        let session = Arc::new(ManagedVulcanPluginSession {
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
                        "Vulcan Plugin session `{session_id}` open failed and lease cleanup failed; the lease remains retained: {cleanup_error}"
                    );
                    self.retain_failed_session_lease(&session, diagnostic.clone())?;
                    return Err(VulcanPluginRuntimeError::new(format!(
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
                        Err(VulcanPluginRuntimeError::new(
                            "Generated Vulcan Plugin session id already exists.",
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
            Err(_) => Err(VulcanPluginRuntimeError::new(
                "Vulcan Plugin session registration gate is poisoned.",
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
                        "Vulcan Plugin session `{session_id}` registration failed and lease cleanup failed; the lease remains retained: {cleanup_error}"
                    );
                    self.retain_failed_session_lease(&session, diagnostic.clone())?;
                    return Err(VulcanPluginRuntimeError::new(format!(
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
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
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
            return Err(VulcanPluginRuntimeError::new(diagnostic));
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
        tool: &VulcanPluginToolDescriptor,
        request: VulcanPluginInvocationRequest,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let mut arguments = require_argument_object(request.arguments)?;
        let session_id = take_session_id(&mut arguments)?;
        if !arguments.is_empty() {
            return Err(VulcanPluginRuntimeError::new(
                "A session-close tool only accepts the host-owned session_id argument.",
            ));
        }
        let session =
            self.owned_session(&session_id, &request.owner_session_id, &tool.plugin_id)?;
        if self.ensure_session_generation(tool, &session).is_err() {
            let report = self.drain_sessions(vec![Arc::clone(&session)]).await?;
            if report.closed_sessions == 0 {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Stale Vulcan Plugin session `{session_id}` could not be closed: {}",
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
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{session_id}` was already closed concurrently."
            )));
        }
        session
            .closing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin session `{session_id}` is already closing."
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
            return Err(VulcanPluginRuntimeError::new(diagnostic));
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
        manifest: &VulcanPluginManifest,
        lease_scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<(String, String, u64), VulcanPluginRuntimeError> {
        let lua_runtime = required_lua_runtime(manifest)?;
        // canonical_package_root pins the real plugin package identity before its bundle is read.
        // canonical_package_root 在读取 bundle 前固定真实插件包身份。
        let canonical_package_root = package_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve Vulcan plugin package root: {error}"
            ))
        })?;
        // canonical_workspace_root binds process cwd authorization to the lease, not plugin input.
        // canonical_workspace_root 把进程 cwd 授权绑定到租约，而不是插件输入。
        let canonical_workspace_root = workspace_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve Vulcan plugin workspace root: {error}"
            ))
        })?;
        if !canonical_workspace_root.is_dir() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin workspace root is not a directory.",
            ));
        }
        // bundle_path is resolved from the validated manifest and may not escape the real package.
        // bundle_path 从已校验清单解析，且不得逃逸真实插件包。
        let bundle_path = canonical_package_root
            .join(&lua_runtime.bundle)
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve Vulcan plugin Lua bundle: {error}"
                ))
            })?;
        if !bundle_path.starts_with(&canonical_package_root) || !bundle_path.is_file() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin Lua bundle escapes its package root or is not a regular file.",
            ));
        }
        // bundle_source is immutable input loaded into the fixed bridge lease.
        // bundle_source 是加载到固定桥接租约中的不可变输入。
        let bundle_source = fs::read_to_string(&bundle_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to read Vulcan plugin Lua bundle: {error}"
            ))
        })?;
        // bridge_package_root is the only host-owned LuaSkills bridge package used by plugins.
        // bridge_package_root 是插件使用的唯一宿主自有 LuaSkills 桥接包。
        let bridge_package_root =
            self.state
                .bridge_package_root
                .canonicalize()
                .map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "Failed to resolve Vulcan plugin bridge package root: {error}"
                    ))
                })?;
        let bridge_dependencies_file = bridge_package_root.join("dependencies.yaml");
        if !bridge_dependencies_file.is_file() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin bridge dependencies.yaml is unavailable.",
            ));
        }
        // bridgeModulePath must remain inside the canonical fixed bridge package root.
        // bridgeModulePath 必须保持在规范化的固定桥接包根目录内。
        let bridge_module_path = bridge_package_root
            .join("runtime")
            .join("bridge.lua")
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve Vulcan plugin bridge module: {error}"
                ))
            })?;
        if !bridge_module_path.starts_with(&bridge_package_root) || !bridge_module_path.is_file() {
            return Err(VulcanPluginRuntimeError::new(
                "Vulcan plugin bridge module escapes its package root or is not a regular file.",
            ));
        }
        // bridgeSource bypasses Lua package.searchpath while preserving the exact trusted package file.
        // bridgeSource 绕过 Lua package.searchpath，同时保留完全相同的可信包文件。
        let bridge_source = fs::read_to_string(&bridge_module_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to read Vulcan plugin bridge module: {error}"
            ))
        })?;
        // lua_bridge_package_root removes Windows verbatim prefixes before the bridge package root enters LuaSkills.
        // lua_bridge_package_root 在桥接包根进入 LuaSkills 前移除 Windows verbatim 前缀。
        let lua_bridge_package_root = vulcan_runtime_assets::native_host_path(&bridge_package_root);
        // lua_workspace_root removes Windows verbatim prefixes before the authorized workspace enters LuaSkills.
        // lua_workspace_root 在授权工作区进入 LuaSkills 前移除 Windows verbatim 前缀。
        let lua_workspace_root = vulcan_runtime_assets::native_host_path(&canonical_workspace_root);
        let requested_sid = format!("vulcan-plugin/{}/{session_id}", manifest.id);
        let create_request = json!({
            "sid": requested_sid.clone(),
            "ttl_sec": 0,
            "replace": false,
            "cwd": lua_bridge_package_root,
            "workspace_root": lua_workspace_root,
            "system_package": {
                "id": "vulcan-plugin-bridge",
                "root": lua_bridge_package_root,
                "dependencies_file": "dependencies.yaml",
            },
        });
        let engine = self.runtime_engine()?;
        let response = call_engine_blocking(Arc::clone(&engine), move |engine| {
            let raw = engine
                .create_system_runtime_lease_json(&create_request.to_string())
                .map_err(VulcanPluginRuntimeError::new)?;
            parse_engine_response(&raw, "create Vulcan plugin bridge lease")
        })
        .await?;
        let lease_id = required_response_string(&response, "lease_id")?;
        let sid = required_response_string(&response, "sid")?;
        if sid != requested_sid {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin bridge lease returned unexpected sid `{sid}` instead of `{requested_sid}`."
            )));
        }
        let generation = required_response_u64(&response, "generation")?;
        // load_request evaluates only the fixed bridge module; the real plugin is supplied as data.
        // load_request 只执行固定桥接模块；真实插件以数据形式提供。
        let load_request = json!({
            "lease_id": lease_id.clone(),
            "sid": sid.clone(),
            "generation": generation,
            "code": "local loader, compile_error = load(args.bridge_source, '@vulcan-plugin-bridge/runtime/bridge.lua', 't')\nif loader == nil then error('Failed to compile Vulcan plugin bridge: ' .. tostring(compile_error)) end\nlocal bridge = loader()\nif type(bridge) ~= 'table' then error('Vulcan plugin bridge must return a table') end\npackage.loaded['runtime.bridge'] = bridge\nreturn bridge.load(args.plugin)",
            "args": {
                "bridge_source": bridge_source,
                "plugin": {
                    "plugin_id": manifest.id,
                    "runtime_id": self.state.manager_id.clone(),
                    "lease_scope": lease_scope.as_str(),
                    "workspace_root": lua_workspace_root,
                    "bundle_source": bundle_source,
                    "lua_modules": lua_runtime.lua_modules,
                    "invoke_function": lua_runtime.invoke_function,
                    "shutdown_function": lua_runtime.shutdown_function,
                    "hook_shutdown_function": lua_runtime.hook_shutdown_function,
                    "hook_functions": lua_runtime
                        .hooks
                        .iter()
                        .map(|hook| hook.function.clone())
                        .collect::<Vec<_>>(),
                    "additional_functions": lua_runtime
                        .actions
                        .iter()
                        .map(|action| action.function.clone())
                        .chain([
                            lua_runtime.lifecycle.install.clone(),
                            lua_runtime.lifecycle.enable.clone(),
                            lua_runtime.lifecycle.disable.clone(),
                            lua_runtime.lifecycle.uninstall.clone(),
                        ].into_iter().flatten())
                        .collect::<BTreeSet<_>>(),
                },
            },
            "timeout_ms": VULCAN_PLUGIN_PROBE_TIMEOUT_MS,
        });
        let load_result = call_engine_blocking(engine, move |engine| {
            let raw = engine
                .eval_system_runtime_lease_json(&load_request.to_string())
                .map_err(VulcanPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "load Vulcan plugin bundle through bridge")?;
            if response.get("result").and_then(Value::as_bool) != Some(true) {
                return Err(VulcanPluginRuntimeError::new(
                    "Vulcan plugin bridge did not confirm bundle loading.",
                ));
            }
            Ok(())
        })
        .await;
        if let Err(error) = load_result {
            let cleanup_result = self.close_raw_lease(&lease_id, &sid, generation).await;
            return match cleanup_result {
                Ok(()) => Err(error),
                Err(cleanup_error) => Err(VulcanPluginRuntimeError::new(format!(
                    "{error}; failed to close rejected bridge lease: {cleanup_error}"
                ))),
            };
        }
        Ok((lease_id, sid, generation))
    }

    /// Resolves one session and enforces exact owner and plugin identity.
    /// 解析单个会话并执行精确的所有者与插件身份校验。
    fn owned_session(
        &self,
        session_id: &str,
        owner_session_id: &str,
        plugin_id: &str,
    ) -> Result<Arc<ManagedVulcanPluginSession>, VulcanPluginRuntimeError> {
        let session = lock_sessions(&self.state.sessions)?
            .get(session_id)
            .cloned()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin session `{session_id}` does not exist."
                ))
            })?;
        if session.owner_session_id != owner_session_id {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{session_id}` belongs to another conversation."
            )));
        }
        if session.plugin_id != plugin_id {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{session_id}` belongs to plugin `{}`.",
                session.plugin_id
            )));
        }
        Ok(session)
    }

    /// Requires one tool definition to match the current enabled plugin generation.
    /// 要求一个工具定义匹配当前已启用插件代次。
    fn ensure_current_tool_generation(
        &self,
        tool: &VulcanPluginToolDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&tool.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                tool.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != tool.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` changed lifecycle generation; retry with a new provider request.",
                tool.plugin_id
            )));
        }
        Ok(())
    }

    /// Requires one hook declaration to match the current enabled plugin generation.
    /// 要求一个 hook 声明匹配当前已启用插件代次。
    fn ensure_current_hook_generation(
        &self,
        hook: &VulcanPluginHookDescriptor,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&hook.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer available.",
                hook.plugin_id
            ))
        })?;
        let current_hook = snapshot
            .enabled_hooks
            .get(&hook.registration_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan Plugin hook `{}` is not enabled in the current catalog.",
                    hook.registration_id
                ))
            })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != hook.plugin_generation
            || current_hook.plugin_generation != hook.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` changed lifecycle generation; retry the hook on the current runtime.",
                hook.plugin_id
            )));
        }
        Ok(())
    }

    /// Requires one retained session to match the current tool and plugin generation.
    /// 要求一个保留会话匹配当前工具与插件代次。
    fn ensure_session_generation(
        &self,
        tool: &VulcanPluginToolDescriptor,
        session: &ManagedVulcanPluginSession,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.ensure_current_tool_generation(tool)?;
        if session.plugin_generation != tool.plugin_generation {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{}` belongs to a stale lifecycle generation.",
                session.session_id
            )));
        }
        Ok(())
    }

    /// Verifies one managed session still belongs to the currently enabled plugin generation.
    /// 验证单个受管会话是否仍属于当前已启用插件代次。
    fn ensure_session_runtime_current(
        &self,
        session: &ManagedVulcanPluginSession,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(&session.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{}` is no longer discovered.",
                session.plugin_id
            ))
        })?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || plugin.generation != session.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{}` belongs to a stale or inactive lifecycle generation.",
                session.session_id
            )));
        }
        Ok(())
    }

    /// Applies generations, transient lifecycle overrides, active counts, and cleanup diagnostics.
    /// 应用代次、临时生命周期覆盖、活动计数与清理诊断。
    fn project_runtime_state(
        &self,
        snapshot: &mut VulcanPluginCatalogSnapshot,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let previous = self.snapshot();
        let active_counts = self.state.active_session_counts.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
        })?;
        let overrides = self.state.lifecycle_overrides.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
        })?;
        let cleanup_diagnostics = self.state.cleanup_diagnostics.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin cleanup diagnostic lock is poisoned.")
        })?;
        let mut generations = self.state.plugin_generations.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin generation lock is poisoned.")
        })?;
        let runtime_engine_available = self
            .state
            .engine
            .read()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin runtime engine lock is poisoned.")
            })?
            .is_some();
        for plugin in snapshot.plugins.values_mut() {
            let requires_runtime = plugin
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.has_lua_runtime());
            plugin.runtime_execution_available = !requires_runtime || runtime_engine_available;
            if requires_runtime && !runtime_engine_available {
                plugin.diagnostics.push(String::from(
                    "The plugin package is valid, but its Lua execution runtime is unavailable; read-only skills and agents remain active.",
                ));
            }
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
            for server in &mut plugin.mcp_servers {
                server.plugin_generation = *generation;
            }
            if let Some(runtime_state) = overrides.get(&plugin.id).copied() {
                plugin.runtime_state = runtime_state;
            } else if plugin.runtime_state == VulcanPluginRuntimeState::Disabled
                && cleanup_diagnostics.contains_key(&plugin.id)
            {
                plugin.runtime_state = VulcanPluginRuntimeState::DisabledWithCleanupWarning;
            }
            if plugin.origin == VulcanPluginPackageOrigin::UserInstalled {
                match plugin_is_authorized(&self.state.plugin_state_root, plugin) {
                    Ok(true) => {}
                    Ok(false) => {
                        plugin.runtime_state = VulcanPluginRuntimeState::Unavailable;
                        plugin.diagnostics.push(String::from(
                            "User-installed plugin requires authorization for its current version, content fingerprint, and complete capability set.",
                        ));
                    }
                    Err(error) => {
                        plugin.runtime_state = VulcanPluginRuntimeState::Unavailable;
                        plugin.diagnostics.push(error);
                    }
                }
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
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` selects the package, `runtime_state` is the target, and source states are exact.
    /// `plugin_id` 选择插件包，`runtime_state` 是目标状态，源状态集合必须精确匹配。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after atomically validating and publishing the transition.
    /// 原子校验并发布转换后返回成功。
    fn begin_lifecycle_transition(
        &self,
        plugin_id: &str,
        runtime_state: VulcanPluginRuntimeState,
        allowed_source_states: &[VulcanPluginRuntimeState],
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let mut snapshot = (*self.snapshot()).clone();
        let plugin = snapshot.plugins.get_mut(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` was not discovered."
            ))
        })?;
        if !allowed_source_states.contains(&plugin.runtime_state) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` cannot enter state `{runtime_state:?}` from state `{:?}`.",
                plugin.runtime_state
            )));
        }
        if !matches!(
            plugin.availability,
            VulcanPluginAvailability::Enabled | VulcanPluginAvailability::Disabled
        ) {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` cannot enter a lifecycle transition while unavailable."
            )));
        }
        let generation = {
            let mut generations = self.state.plugin_generations.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin generation lock is poisoned.")
            })?;
            let generation = generations.entry(plugin_id.to_owned()).or_insert(1);
            *generation = generation.saturating_add(1);
            *generation
        };
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan Plugin lifecycle lock is poisoned.")
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
        for server in &mut plugin.mcp_servers {
            server.plugin_generation = generation;
        }
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        rebuild_enabled_surface(&mut snapshot);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
    }

    /// Evaluates one exact manifest-validated plugin function through the fixed bridge ABI.
    /// 通过固定桥接 ABI 执行一个经清单校验的精确插件函数。
    ///
    /// # Parameters
    /// 参数
    /// Lease identity, function name, arguments, and timeout are all supplied by the host.
    /// 租约身份、函数名、参数与超时均由宿主提供。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded plugin result envelope.
    /// 返回解码后的插件结果信封。
    async fn evaluate_bridge_function(
        &self,
        lease_id: &str,
        sid: &str,
        generation: u64,
        function_name: &str,
        plugin_args: Value,
        timeout_ms: u64,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let lease_id = lease_id.to_owned();
        let sid = sid.to_owned();
        let function_name = function_name.to_owned();
        let engine = self.runtime_engine()?;
        call_engine_blocking(engine, move |engine| {
            let eval_request = json!({
                "lease_id": lease_id,
                "sid": sid,
                "generation": generation,
                "code": "local bridge = require('runtime.bridge')\nreturn bridge.invoke(args)",
                "args": {
                    "function_name": function_name,
                    "plugin_args": plugin_args,
                },
                "timeout_ms": timeout_ms,
            });
            let raw = engine
                .eval_system_runtime_lease_json(&eval_request.to_string())
                .map_err(VulcanPluginRuntimeError::new)?;
            let response = parse_engine_response(&raw, "evaluate Vulcan plugin bridge function")?;
            parse_plugin_result(&response)
        })
        .await
    }

    /// Dispatches one exact JSON payload through the plugin Lua bundle.
    /// 通过插件 Lua bundle 调度一个精确 JSON 载荷。
    ///
    /// # Parameters
    /// 参数
    /// `session` owns the lease; payload, timeout, and shutdown select the exact ABI call.
    /// `session` 持有租约；payload、timeout 与 shutdown 选择精确 ABI 调用。
    ///
    /// # Returns
    /// 返回值
    /// Returns the decoded plugin result.
    /// 返回解码后的插件结果。
    async fn dispatch(
        &self,
        session: Arc<ManagedVulcanPluginSession>,
        mut payload: Value,
        host_timeout_ms: u64,
        shutdown: bool,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let _operation = session.operation_gate.lock().await;
        if session.closing.load(Ordering::Acquire) && !shutdown {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin session `{}` is closing.",
                session.session_id
            )));
        }
        if !shutdown {
            self.ensure_session_runtime_current(&session)?;
        }
        let request_id = self.state.next_request_id.fetch_add(1, Ordering::Relaxed);
        let object = payload.as_object_mut().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Vulcan Plugin payload must be a JSON object.")
        })?;
        object.insert(String::from("request_id"), json!(request_id.to_string()));
        object.insert(String::from("host_timeout_ms"), json!(host_timeout_ms));
        let payload_json = payload.to_string();
        let runtime = required_lua_runtime(&session.manifest)?;
        let function = if shutdown {
            runtime.shutdown_function.clone()
        } else {
            runtime.invoke_function.clone()
        };
        self.evaluate_bridge_function(
            &session.lease_id,
            &session.sid,
            session.generation,
            &function,
            json!({
                "payload_json": payload_json,
                "host_timeout_ms": host_timeout_ms,
            }),
            host_timeout_ms,
        )
        .await
    }

    /// Closes one LuaSkills lease after plugin shutdown is attempted.
    /// 在尝试插件关闭后释放单个 LuaSkills 租约。
    async fn close_lease(
        &self,
        session: Arc<ManagedVulcanPluginSession>,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let lease_id = session.lease_id.clone();
        let sid = session.sid.clone();
        let generation = session.generation;
        let engine = self.runtime_engine()?;
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
                .map_err(VulcanPluginRuntimeError::new)?;
            parse_engine_response(&raw, "close Vulcan Plugin runtime lease")
        })
        .await
    }

    /// Reserves one plugin-scoped session slot under an exact hard limit.
    /// 在精确硬上限下预留一个插件作用域会话名额。
    fn reserve_session_slot(
        &self,
        plugin_id: &str,
        maximum: usize,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let mut counts = self.state.active_session_counts.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session counter lock is poisoned.")
        })?;
        let count = counts.entry(plugin_id.to_owned()).or_default();
        if *count >= maximum {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin `{plugin_id}` reached its active session limit of {maximum}."
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
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin session registration gate is poisoned.")
        })?;
        let mut snapshot = (*self.snapshot()).clone();
        let Some(plugin) = snapshot.plugins.get_mut(plugin_id) else {
            return Ok(());
        };
        plugin.active_session_count = active_session_count;
        snapshot.revision = self.state.next_revision.fetch_add(1, Ordering::Relaxed);
        let mut guard = self.state.snapshot.write().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan Plugin catalog lock is poisoned.")
        })?;
        *guard = Arc::new(snapshot);
        Ok(())
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
        plugin_id: plugin_identity.to_string(),
        operation: storage_operation,
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

/// Requires one enabled plugin to declare an exact authorized host capability.
/// 要求一个已启用插件声明精确且已授权的宿主能力。
///
/// # Parameters
/// 参数
/// `state` owns the current catalog, `plugin_id` selects the plugin, and `capability` is required.
/// `state` 持有当前目录，`plugin_id` 选择插件，`capability` 是必需能力。
///
/// # Returns
/// 返回值
/// Returns unit only for an enabled plugin with the declared capability.
/// 仅当插件已启用且声明所需能力时返回空值。
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

/// Resolves one host-owned per-invocation workspace root from generic hook context metadata.
/// 从通用 Hook 上下文元数据解析单个宿主持有的每次调用工作区根。
///
/// # Parameters
/// 参数
/// `context` is the immutable host hook context delivered to the plugin runtime.
/// `context` 是传递给插件运行时的不可变宿主 Hook 上下文。
///
/// # Returns
/// 返回值
/// Returns no override when the host did not supply one, or one canonical existing directory.
/// 宿主未提供时不返回覆盖；否则返回一个规范化且已存在的目录。
fn trusted_hook_workspace_root(
    context: &vulcan_hooks::HookContext,
) -> Result<Option<String>, VulcanPluginRuntimeError> {
    let Some(value) = context.metadata.get("workspace_root") else {
        return Ok(None);
    };
    let raw_workspace_root = value.as_str().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Hook context workspace_root metadata must be a string.")
    })?;
    let workspace_root = PathBuf::from(raw_workspace_root)
        .canonicalize()
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve Hook context workspace root: {error}"
            ))
        })?;
    if !workspace_root.is_dir() {
        return Err(VulcanPluginRuntimeError::new(
            "Hook context workspace root is not a directory.",
        ));
    }
    Ok(Some(vulcan_runtime_assets::native_host_path_text(
        &workspace_root.to_string_lossy(),
    )))
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
fn canonical_bridge_workspace_root(request: &Map<String, Value>) -> Result<PathBuf, String> {
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

/// Resolves one plugin-owned logical process entry into an absolute LuaSkills process spec.
/// 把一个插件持有的逻辑进程入口解析为绝对 LuaSkills 进程规范。
///
/// # Parameters
/// 参数
/// `state` is the active manager state and `request_args` contains bridge-injected identity.
/// `state` 是活动管理器状态，`request_args` 包含桥接层注入的身份。
///
/// # Returns
/// 返回值
/// Returns a contained executable specification or an explicit policy error.
/// 返回受目录边界约束的可执行规范或显式策略错误。
fn resolve_plugin_process_spec(
    state: &VulcanPluginManagerState,
    request_args: &Value,
) -> Result<Value, String> {
    let request = request_args
        .as_object()
        .ok_or_else(|| "Vulcan plugin process request must be an object.".to_string())?;
    let plugin_id = required_json_string(request, "plugin_id")?;
    let lease_scope =
        VulcanPluginBridgeLeaseScope::parse(required_json_string(request, "lease_scope")?)?;
    let workspace_root = canonical_bridge_workspace_root(request)?;
    let payload = request
        .get("payload")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            "Vulcan plugin process request requires object field `payload`.".to_string()
        })?;
    let entry_id = required_json_string(payload, "entry_id")?;
    let mode = required_json_string(payload, "mode")?;
    if mode != "exec" && mode != "session" {
        return Err("Vulcan plugin process mode must be `exec` or `session`.".to_string());
    }

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
            "Vulcan plugin `{plugin_id}` runtime state does not authorize process bridge scope `{}`.",
            lease_scope.as_str()
        ));
    }
    let manifest = plugin
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` has no valid manifest."))?;
    let options = payload
        .get("options")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            "Vulcan plugin process request requires object field `options`.".to_string()
        })?;
    if mode == "session"
        && !manifest
            .permissions
            .contains(&VulcanPluginPermission::ProcessSession)
    {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` did not declare process_session capability."
        ));
    }

    let binary = manifest
        .execution
        .native_binaries
        .iter()
        .find(|entry| entry.id == entry_id);
    let node_entry = manifest
        .execution
        .node_entries
        .iter()
        .find(|entry| entry.id == entry_id);
    let python_entry = manifest
        .execution
        .python_entries
        .iter()
        .find(|entry| entry.id == entry_id);
    // executionRoot binds all logical process paths to the trusted package generation.
    // executionRoot 把所有逻辑进程路径绑定到可信插件包代次。
    let execution_root = plugin_execution_package_root(state, plugin)?;
    match (binary, node_entry, python_entry) {
        (Some(binary), None, None) => {
            if !manifest
                .permissions
                .contains(&VulcanPluginPermission::ProcessBinary)
            {
                return Err(format!(
                    "Vulcan plugin `{plugin_id}` did not declare process_binary capability."
                ));
            }
            resolve_plugin_binary_spec(
                plugin,
                &execution_root,
                binary,
                mode,
                options,
                &workspace_root,
            )
        }
        (None, Some(node_entry), None) => {
            if !manifest
                .permissions
                .contains(&VulcanPluginPermission::ProcessNode)
            {
                return Err(format!(
                    "Vulcan plugin `{plugin_id}` did not declare process_node capability."
                ));
            }
            resolve_plugin_node_spec(state, plugin, manifest, node_entry, mode, options)
        }
        (None, None, Some(python_entry)) => {
            if !manifest
                .permissions
                .contains(&VulcanPluginPermission::ProcessPython)
            {
                return Err(format!(
                    "Vulcan plugin `{plugin_id}` did not declare process_python capability."
                ));
            }
            resolve_plugin_python_spec(state, plugin, manifest, python_entry, mode, options)
        }
        (Some(_), Some(_), _) | (Some(_), _, Some(_)) | (_, Some(_), Some(_)) => Err(format!(
            "Vulcan plugin `{plugin_id}` declares duplicate process entry `{entry_id}`."
        )),
        (None, None, None) => Err(format!(
            "Vulcan plugin `{plugin_id}` does not declare process entry `{entry_id}`."
        )),
    }
}

/// Resolves one plugin MCP stdio launcher into an exact managed process specification.
/// 将单个插件 MCP stdio 启动器解析为精确的受管进程规格。
///
/// # Parameters
/// 参数
/// Runtime state owns managed distributions, while plugin, manifest, and launcher pin one generation.
/// 运行时状态持有受管发行版，plugin、manifest 与 launcher 固定到单个代次。
///
/// # Returns
/// 返回值
/// Returns a Shell-free executable, fixed arguments, contained working directory, and host environment.
/// 返回不使用 Shell 的可执行文件、固定参数、受约束工作目录与宿主环境。
fn resolve_plugin_mcp_stdio_launcher(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
    manifest: &VulcanPluginManifest,
    launcher: &VulcanPluginMcpStdioLauncherManifest,
) -> Result<VulcanPluginMcpStdioLaunchSpec, String> {
    let environment_root = ensure_plugin_execution_environment(state, plugin)?;
    match launcher {
        VulcanPluginMcpStdioLauncherManifest::NodeEntry { entry_id } => {
            let entry = manifest
                .execution
                .node_entries
                .iter()
                .find(|entry| entry.id == *entry_id)
                .ok_or_else(|| {
                    format!(
                        "Vulcan plugin `{}` does not declare Node entry `{entry_id}`.",
                        plugin.id
                    )
                })?;
            let node = resolve_plugin_managed_node(state, manifest, &environment_root)?;
            let script = canonical_plugin_file(&environment_root, &entry.file, "MCP Node entry")?;
            let cwd = canonical_plugin_directory(
                &environment_root,
                &entry.cwd,
                "MCP Node working directory",
            )?;
            Ok(VulcanPluginMcpStdioLaunchSpec {
                program: node,
                base_args: vec![vulcan_runtime_assets::native_host_path_text_from_path(
                    &script,
                )],
                cwd,
                env: BTreeMap::new(),
            })
        }
        VulcanPluginMcpStdioLauncherManifest::NativeBinary { binary_id } => {
            let binary = manifest
                .execution
                .native_binaries
                .iter()
                .find(|binary| binary.id == *binary_id)
                .ok_or_else(|| {
                    format!(
                        "Vulcan plugin `{}` does not declare native binary `{binary_id}`.",
                        plugin.id
                    )
                })?;
            let target = current_plugin_target_triple()?;
            let relative_program = binary.targets.get(target).ok_or_else(|| {
                format!(
                    "Vulcan plugin `{}` binary `{binary_id}` has no asset for target `{target}`.",
                    plugin.id
                )
            })?;
            Ok(VulcanPluginMcpStdioLaunchSpec {
                program: canonical_plugin_file(
                    &environment_root,
                    relative_program,
                    "MCP native binary",
                )?,
                base_args: Vec::new(),
                cwd: environment_root,
                env: BTreeMap::new(),
            })
        }
        VulcanPluginMcpStdioLauncherManifest::NodePackageBin { package, bin } => {
            resolve_plugin_node_package_bin(
                state,
                plugin,
                manifest,
                &environment_root,
                package,
                bin,
            )
        }
    }
}

/// Resolves the managed Node executable declared by one plugin dependency manifest.
/// 解析单个插件依赖清单声明的受管 Node 可执行文件。
fn resolve_plugin_managed_node(
    state: &VulcanPluginManagerState,
    manifest: &VulcanPluginManifest,
    environment_root: &Path,
) -> Result<PathBuf, String> {
    let dependency_manifest = manifest
        .execution
        .dependencies
        .manifest
        .as_deref()
        .ok_or_else(|| String::from("Plugin Node execution requires a dependency manifest."))?;
    let dependency_file =
        canonical_plugin_file(environment_root, dependency_manifest, "dependency manifest")?;
    let dependencies = PackageDependencyManifest::load_from_path(&dependency_file)?;
    let node = dependencies.node_runtime.as_ref().ok_or_else(|| {
        String::from("Plugin Node execution requires a node_runtime declaration.")
    })?;
    let platform = current_managed_runtime_platform_key()?;
    Ok(resolve_managed_runtime_install(
        &state.managed_runtime_distribution_root,
        ManagedRuntimeKind::Node,
        &node.version,
        &platform,
    )?
    .executable)
}

/// Resolves one exact npm package bin without executing a package-manager shim.
/// 在不执行包管理器 shim 的前提下解析单个精确 npm package bin。
fn resolve_plugin_node_package_bin(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
    manifest: &VulcanPluginManifest,
    environment_root: &Path,
    package: &str,
    bin: &str,
) -> Result<VulcanPluginMcpStdioLaunchSpec, String> {
    let dependency_manifest = manifest
        .execution
        .dependencies
        .manifest
        .as_deref()
        .ok_or_else(|| String::from("Plugin Node execution requires a dependency manifest."))?;
    let dependency_file =
        canonical_plugin_file(environment_root, dependency_manifest, "dependency manifest")?;
    let dependencies = PackageDependencyManifest::load_from_path(&dependency_file)?;
    let node = dependencies.node_runtime.as_ref().ok_or_else(|| {
        format!(
            "Vulcan plugin `{}` node_package_bin requires a node_runtime declaration.",
            plugin.id
        )
    })?;
    let package_json = canonical_plugin_file(
        environment_root,
        &node.package_json,
        "Node package manifest",
    )?;
    let package_directory = package_json
        .parent()
        .ok_or_else(|| String::from("Node package manifest has no parent directory."))?;
    let package_relative = npm_package_relative_path(package)?;
    let node_modules_root = package_directory
        .join("node_modules")
        .canonicalize()
        .map_err(|error| format!("Failed to resolve plugin node_modules directory: {error}"))?;
    let package_root = node_modules_root
        .join(package_relative)
        .canonicalize()
        .map_err(|error| format!("Failed to resolve installed npm package `{package}`: {error}"))?;
    if !package_root.starts_with(&node_modules_root) || !package_root.is_dir() {
        return Err(format!(
            "Installed npm package `{package}` escapes the plugin-private node_modules directory."
        ));
    }
    let installed_manifest_path = canonical_plugin_file(
        &package_root,
        "package.json",
        "installed npm package manifest",
    )?;
    let installed_manifest =
        read_plugin_json_state(&installed_manifest_path, "installed npm package manifest")
            .map_err(|error| error.to_string())?;
    if installed_manifest.get("name").and_then(Value::as_str) != Some(package) {
        return Err(format!(
            "Installed npm package identity does not match `{package}`."
        ));
    }
    let bin_target = resolve_npm_bin_target(&installed_manifest, package, bin)?;
    let script = canonical_plugin_file(&package_root, bin_target, "npm package bin")?;
    Ok(VulcanPluginMcpStdioLaunchSpec {
        program: resolve_plugin_managed_node(state, manifest, environment_root)?,
        base_args: vec![vulcan_runtime_assets::native_host_path_text_from_path(
            &script,
        )],
        cwd: package_directory.to_path_buf(),
        env: BTreeMap::new(),
    })
}

/// Converts one validated npm package name into its node_modules-relative path.
/// 将一个经过校验的 npm 包名转换为相对于 node_modules 的路径。
fn npm_package_relative_path(package: &str) -> Result<PathBuf, String> {
    let segments = package.split('/').collect::<Vec<_>>();
    let valid = match segments.as_slice() {
        [name] => !name.is_empty() && !name.starts_with('@'),
        [scope, name] => scope.starts_with('@') && scope.len() > 1 && !name.is_empty(),
        _ => false,
    } && segments.iter().all(|segment| {
        segment.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '@' | '-' | '_' | '.')
        }) && *segment != "."
            && *segment != ".."
    });
    if !valid {
        return Err(format!("Invalid npm package name `{package}`."));
    }
    Ok(segments.iter().collect())
}

/// Resolves an exact bin key from one installed npm package manifest.
/// 从单个已安装 npm 包清单解析精确 bin 键。
fn resolve_npm_bin_target<'a>(
    manifest: &'a Value,
    package: &str,
    bin: &str,
) -> Result<&'a str, String> {
    let declaration = manifest
        .get("bin")
        .ok_or_else(|| format!("Installed npm package `{package}` declares no bin entries."))?;
    match declaration {
        Value::Object(entries) => entries.get(bin).and_then(Value::as_str).ok_or_else(|| {
            format!("Installed npm package `{package}` declares no bin key `{bin}`.")
        }),
        Value::String(target) => {
            let default_bin = package.rsplit('/').next().unwrap_or(package);
            if bin == default_bin {
                Ok(target)
            } else {
                Err(format!(
                    "Installed npm package `{package}` string bin declaration only defines `{default_bin}`."
                ))
            }
        }
        _ => Err(format!(
            "Installed npm package `{package}` has an invalid bin declaration."
        )),
    }
}

/// Resolves one manifest-declared native binary for the current Rust target.
/// 为当前 Rust 目标解析一个清单声明的原生二进制文件。
///
/// # Parameters
/// 参数
/// `plugin` owns identity, `package_root` is trusted, `binary` selects the entry, and options are arguments.
/// `plugin` 持有身份，`package_root` 是可信根，`binary` 选择入口，options 是参数。
///
/// # Returns
/// 返回值
/// Returns a contained native process specification.
/// 返回一份受目录边界约束的原生进程规范。
fn resolve_plugin_binary_spec(
    plugin: &crate::VulcanPluginDescriptor,
    package_root: &Path,
    binary: &crate::VulcanPluginBinaryManifest,
    mode: &str,
    options: &Map<String, Value>,
    workspace_root: &Path,
) -> Result<Value, String> {
    let target = current_plugin_target_triple()?;
    let relative_program = binary.targets.get(target).ok_or_else(|| {
        format!(
            "Vulcan plugin `{}` binary `{}` has no asset for target `{target}`.",
            plugin.id, binary.id
        )
    })?;
    let program = canonical_plugin_file(package_root, relative_program, "binary")?;
    let mut bounded_options = options.clone();
    let cwd = match bounded_options.remove("cwd") {
        Some(value) => {
            if !binary.allow_workspace_cwd {
                return Err(format!(
                    "Vulcan plugin `{}` binary `{}` does not allow workspace cwd overrides.",
                    plugin.id, binary.id
                ));
            }
            let requested = value.as_str().ok_or_else(|| {
                "Vulcan plugin process `cwd` must be an absolute workspace path.".to_string()
            })?;
            let requested_path = PathBuf::from(requested);
            if !requested_path.is_absolute() {
                return Err(
                    "Vulcan plugin process `cwd` must be an absolute workspace path.".to_string(),
                );
            }
            let canonical_requested = requested_path
                .canonicalize()
                .map_err(|error| format!("Failed to resolve Vulcan plugin process cwd: {error}"))?;
            if !canonical_requested.is_dir() || !canonical_requested.starts_with(workspace_root) {
                return Err(
                    "Vulcan plugin process `cwd` must stay inside the trusted workspace root."
                        .to_string(),
                );
            }
            canonical_requested
        }
        None => package_root
            .canonicalize()
            .map_err(|error| format!("Failed to resolve Vulcan plugin package root: {error}"))?,
    };
    build_plugin_process_spec(
        program,
        cwd,
        Vec::new(),
        mode,
        &bounded_options,
        Some(&binary.allowed_env),
        BTreeMap::new(),
    )
}

/// Resolves one manifest-declared Node.js entry through the shared managed distribution.
/// 通过共享受管发行版解析一个清单声明的 Node.js 入口。
///
/// # Parameters
/// 参数
/// `state` owns the distribution root; plugin, manifest, and entry identify the exact script.
/// `state` 持有发行根；plugin、manifest 与 entry 标识精确脚本。
///
/// # Returns
/// 返回值
/// Returns a Node executable process specification with the contained script as its first argument.
/// 返回以受目录约束脚本为首个参数的 Node 可执行进程规范。
fn resolve_plugin_node_spec(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
    manifest: &VulcanPluginManifest,
    entry: &crate::VulcanPluginNodeEntryManifest,
    mode: &str,
    options: &Map<String, Value>,
) -> Result<Value, String> {
    let environment_root = ensure_plugin_execution_environment(state, plugin)?;
    let dependency_manifest = manifest
        .execution
        .dependencies
        .manifest
        .as_deref()
        .ok_or_else(|| String::from("Plugin Node execution requires a dependency manifest."))?;
    let dependency_file = canonical_plugin_file(
        &environment_root,
        dependency_manifest,
        "dependency manifest",
    )?;
    let dependencies = PackageDependencyManifest::load_from_path(&dependency_file)?;
    let node = dependencies.node_runtime.as_ref().ok_or_else(|| {
        format!(
            "Vulcan plugin `{}` Node entry `{}` requires a node_runtime declaration.",
            plugin.id, entry.id
        )
    })?;
    let platform = current_managed_runtime_platform_key()?;
    let runtime = resolve_managed_runtime_install(
        &state.managed_runtime_distribution_root,
        ManagedRuntimeKind::Node,
        &node.version,
        &platform,
    )?;
    let script = canonical_plugin_file(&environment_root, &entry.file, "Node entry")?;
    let cwd = canonical_plugin_directory(&environment_root, &entry.cwd, "Node working directory")?;
    build_plugin_process_spec(
        runtime.executable,
        cwd,
        vec![vulcan_runtime_assets::native_host_path_text_from_path(
            &script,
        )],
        mode,
        options,
        None,
        BTreeMap::new(),
    )
}

/// Resolves one manifest-declared Python entry through its isolated plugin environment.
/// 通过插件隔离环境解析一个清单声明的 Python 入口。
///
/// # Parameters
/// 参数
/// `state` owns shared runtimes and private environments; remaining values select the exact entry.
/// `state` 持有共享运行时与私有环境；其余值选择精确入口。
///
/// # Returns
/// 返回值
/// Returns a contained Python process specification with host-owned dependency paths.
/// 返回一份包含宿主自有依赖路径且受目录约束的 Python 进程规范。
fn resolve_plugin_python_spec(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
    manifest: &VulcanPluginManifest,
    entry: &crate::VulcanPluginPythonEntryManifest,
    mode: &str,
    options: &Map<String, Value>,
) -> Result<Value, String> {
    let environment_root = ensure_plugin_execution_environment(state, plugin)?;
    let dependency_manifest = manifest
        .execution
        .dependencies
        .manifest
        .as_deref()
        .ok_or_else(|| String::from("Plugin Python execution requires a dependency manifest."))?;
    let dependency_file = canonical_plugin_file(
        &environment_root,
        dependency_manifest,
        "dependency manifest",
    )?;
    let dependencies = PackageDependencyManifest::load_from_path(&dependency_file)?;
    let python = dependencies.python_runtime.as_ref().ok_or_else(|| {
        format!(
            "Vulcan plugin `{}` Python entry `{}` requires a python_runtime declaration.",
            plugin.id, entry.id
        )
    })?;
    let platform = current_managed_runtime_platform_key()?;
    let runtime = resolve_managed_runtime_install(
        &state.managed_runtime_distribution_root,
        ManagedRuntimeKind::Python,
        &python.version,
        &platform,
    )?;
    let script = canonical_plugin_file(&environment_root, &entry.file, "Python entry")?;
    let cwd =
        canonical_plugin_directory(&environment_root, &entry.cwd, "Python working directory")?;
    let mut fixed_environment = BTreeMap::new();
    let bundled_packages = environment_root.join("python").join("vendor");
    if bundled_packages.is_dir() {
        fixed_environment.insert(
            String::from("PYTHONPATH"),
            vulcan_runtime_assets::native_host_path_text_from_path(&bundled_packages),
        );
    }
    let executable = if manifest.execution.dependencies.install_mode
        == VulcanPluginDependencyInstallMode::Registry
    {
        plugin_python_environment_executable(&environment_root)
    } else {
        runtime.executable
    };
    build_plugin_process_spec(
        executable,
        cwd,
        vec![vulcan_runtime_assets::native_host_path_text_from_path(
            &script,
        )],
        mode,
        options,
        None,
        fixed_environment,
    )
}

/// Builds the final strict process table accepted by LuaSkills generic process APIs.
/// 构建 LuaSkills 通用进程 API 接受的最终严格进程表。
///
/// # Parameters
/// 参数
/// Fixed host paths are combined with validated plugin arguments and bounded process options.
/// 固定宿主路径与已校验插件参数及有界进程选项组合。
///
/// # Returns
/// 返回值
/// Returns an exact process specification without accepting program or cwd overrides.
/// 返回不接受 program 或 cwd 覆盖的精确进程规范。
fn build_plugin_process_spec(
    program: PathBuf,
    cwd: PathBuf,
    mut fixed_args: Vec<String>,
    mode: &str,
    options: &Map<String, Value>,
    allowed_env: Option<&[String]>,
    fixed_environment: BTreeMap<String, String>,
) -> Result<Value, String> {
    let allowed_option_names: BTreeSet<&str> = if mode == "exec" {
        [
            "args",
            "env",
            "stdin",
            "timeout_ms",
            "encoding",
            "stdout_encoding",
            "stderr_encoding",
            "stdin_encoding",
        ]
        .into_iter()
        .collect()
    } else {
        [
            "args",
            "encoding",
            "stdout_encoding",
            "stderr_encoding",
            "stdin_encoding",
            "buffer_limit_bytes",
        ]
        .into_iter()
        .collect()
    };
    for option_name in options.keys() {
        if !allowed_option_names.contains(option_name.as_str()) {
            return Err(format!(
                "Vulcan plugin process option `{option_name}` is not allowed in `{mode}` mode."
            ));
        }
    }
    if let Some(arguments) = options.get("args") {
        let arguments = arguments
            .as_array()
            .ok_or_else(|| "Vulcan plugin process `args` must be a string array.".to_string())?;
        for argument in arguments {
            fixed_args.push(
                argument
                    .as_str()
                    .ok_or_else(|| {
                        "Vulcan plugin process `args` must contain only strings.".to_string()
                    })?
                    .to_string(),
            );
        }
    }
    let mut spec = Map::new();
    spec.insert(
        "program".to_string(),
        Value::String(vulcan_runtime_assets::native_host_path_text_from_path(
            &program,
        )),
    );
    spec.insert("args".to_string(), json!(fixed_args));
    spec.insert(
        "cwd".to_string(),
        Value::String(vulcan_runtime_assets::native_host_path_text_from_path(&cwd)),
    );
    for encoding_field in [
        "encoding",
        "stdout_encoding",
        "stderr_encoding",
        "stdin_encoding",
    ] {
        if let Some(value) = options.get(encoding_field) {
            let encoding = value.as_str().ok_or_else(|| {
                format!("Vulcan plugin process `{encoding_field}` must be a string.")
            })?;
            spec.insert(
                encoding_field.to_string(),
                Value::String(encoding.to_string()),
            );
        }
    }
    if mode == "exec" {
        copy_optional_string(options, &mut spec, "stdin")?;
        copy_optional_positive_u64(options, &mut spec, "timeout_ms")?;
        if let Some(environment) = options.get("env") {
            let mut environment =
                validate_plugin_environment(environment, allowed_env.unwrap_or_default())?
                    .as_object()
                    .cloned()
                    .ok_or_else(|| "Validated plugin environment is not an object.".to_string())?;
            for (name, value) in &fixed_environment {
                if environment.contains_key(name) {
                    return Err(format!(
                        "Vulcan plugin process environment `{name}` is reserved by the host."
                    ));
                }
                environment.insert(name.clone(), Value::String(value.clone()));
            }
            spec.insert("env".to_string(), Value::Object(environment));
        } else if !fixed_environment.is_empty() {
            spec.insert("env".to_string(), json!(fixed_environment));
        }
    } else {
        copy_optional_positive_u64(options, &mut spec, "buffer_limit_bytes")?;
    }
    Ok(Value::Object(spec))
}

/// Validates a plugin-provided environment map against one binary allowlist.
/// 根据单个二进制环境变量白名单校验插件提供的环境映射。
///
/// # Parameters
/// 参数
/// `value` is the untrusted map and `allowed_names` is the exact manifest declaration.
/// `value` 是不可信映射，`allowed_names` 是精确清单声明。
///
/// # Returns
/// 返回值
/// Returns a string-only environment object.
/// 返回仅包含字符串值的环境对象。
fn validate_plugin_environment(value: &Value, allowed_names: &[String]) -> Result<Value, String> {
    let environment = value
        .as_object()
        .ok_or_else(|| "Vulcan plugin process `env` must be an object.".to_string())?;
    let allowed_names = allowed_names
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut validated = Map::new();
    for (name, value) in environment {
        if !allowed_names.contains(name.as_str()) {
            return Err(format!(
                "Vulcan plugin process environment variable `{name}` is not declared by its binary."
            ));
        }
        let text = value.as_str().ok_or_else(|| {
            format!("Vulcan plugin process environment variable `{name}` must be a string.")
        })?;
        validated.insert(name.clone(), Value::String(text.to_string()));
    }
    Ok(Value::Object(validated))
}

/// Copies one optional string process field after strict type validation.
/// 在严格类型校验后复制一个可选字符串进程字段。
fn copy_optional_string(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = source.get(field) {
        let value = value
            .as_str()
            .ok_or_else(|| format!("Vulcan plugin process `{field}` must be a string."))?;
        target.insert(field.to_string(), Value::String(value.to_string()));
    }
    Ok(())
}

/// Copies one optional strictly positive integer process field.
/// 复制一个可选且严格为正整数的进程字段。
fn copy_optional_positive_u64(
    source: &Map<String, Value>,
    target: &mut Map<String, Value>,
    field: &str,
) -> Result<(), String> {
    if let Some(value) = source.get(field) {
        let value = value.as_u64().filter(|value| *value > 0).ok_or_else(|| {
            format!("Vulcan plugin process `{field}` must be a positive integer.")
        })?;
        target.insert(field.to_string(), json!(value));
    }
    Ok(())
}

/// Reads one required non-empty string from a JSON object.
/// 从 JSON 对象读取一个必填非空字符串。
fn required_json_string<'a>(
    object: &'a Map<String, Value>,
    field: &str,
) -> Result<&'a str, String> {
    object
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("Vulcan plugin request requires non-empty string `{field}`."))
}

/// Canonicalizes one package-relative regular file without allowing root escape.
/// 规范化一个插件包相对普通文件，且不允许逃逸包根目录。
fn canonical_plugin_file(root: &Path, relative: &str, label: &str) -> Result<PathBuf, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan plugin package root: {error}"))?;
    let path = canonical_root
        .join(relative)
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan plugin {label}: {error}"))?;
    if !path.starts_with(&canonical_root) || !path.is_file() {
        return Err(format!(
            "Vulcan plugin {label} escapes its package root or is not a regular file."
        ));
    }
    Ok(path)
}

/// Canonicalizes one package-relative directory without allowing root escape.
/// 规范化一个插件包相对目录，且不允许逃逸包根目录。
fn canonical_plugin_directory(root: &Path, relative: &str, label: &str) -> Result<PathBuf, String> {
    let canonical_root = root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan plugin package root: {error}"))?;
    let path = canonical_root
        .join(relative)
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan plugin {label}: {error}"))?;
    if !path.starts_with(&canonical_root) || !path.is_dir() {
        return Err(format!(
            "Vulcan plugin {label} escapes its package root or is not a directory."
        ));
    }
    Ok(path)
}

/// Returns the exact Rust target triple used by packaged plugin binary mappings.
/// 返回插件二进制映射使用的精确 Rust 目标三元组。
fn current_plugin_target_triple() -> Result<&'static str, String> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return Ok("x86_64-pc-windows-msvc");
    }
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        return Ok("aarch64-pc-windows-msvc");
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        return Ok("x86_64-unknown-linux-gnu");
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        return Ok("aarch64-unknown-linux-gnu");
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        return Ok("x86_64-apple-darwin");
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        return Ok("aarch64-apple-darwin");
    }
    #[allow(unreachable_code)]
    Err(format!(
        "Unsupported Vulcan plugin binary target `{}-{}`.",
        std::env::consts::ARCH,
        std::env::consts::OS
    ))
}

/// Returns the deterministic configuration document path for one validated plugin identity.
/// 返回一个已校验插件身份的确定性配置文档路径。
fn plugin_config_state_path(state_root: &Path, plugin_id: &str) -> PathBuf {
    state_root.join(plugin_id).join("config.json")
}

/// Returns the deterministic authorization document path for one validated plugin identity.
/// 返回一个已校验插件身份的确定性授权文档路径。
fn plugin_authorization_state_path(state_root: &Path, plugin_id: &str) -> PathBuf {
    state_root.join(plugin_id).join("authorization.json")
}

/// Returns the deterministic installation record path for one validated plugin identity.
/// 返回一个已校验插件身份的确定性安装记录路径。
fn plugin_installation_state_path(state_root: &Path, plugin_id: &str) -> PathBuf {
    state_root.join(plugin_id).join("installation.json")
}

/// Builds an authorization record bound to one exact validated descriptor.
/// 构建绑定到一个精确已校验描述符的授权记录。
///
/// # Parameters
/// 参数
/// `plugin` supplies immutable identity and `permissions` is the exact approved set.
/// `plugin` 提供不可变身份，`permissions` 是精确批准集合。
///
/// # Returns
/// 返回值
/// Returns one durable record ready for atomic persistence.
/// 返回一条可供原子持久化的记录。
fn authorization_record_for_descriptor(
    plugin: &crate::VulcanPluginDescriptor,
    permissions: Vec<String>,
) -> Result<VulcanPluginAuthorizationRecord, VulcanPluginRuntimeError> {
    Ok(VulcanPluginAuthorizationRecord {
        schema_version: 1,
        plugin_id: plugin.id.clone(),
        version: plugin.version.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no validated version.",
                plugin.id
            ))
        })?,
        content_fingerprint: plugin.content_fingerprint.clone(),
        permissions,
    })
}

/// Atomically writes one strongly typed plugin state record.
/// 原子写入一条强类型插件状态记录。
///
/// # Parameters
/// 参数
/// `path` is the final state path, `record` is serialized as pretty JSON, and `label` identifies failures.
/// `path` 是最终状态路径，`record` 序列化为格式化 JSON，`label` 标识失败类型。
fn write_plugin_json_record(
    path: &Path,
    record: &impl Serialize,
    label: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    let encoded = serde_json::to_vec_pretty(record).map_err(|error| {
        VulcanPluginRuntimeError::new(format!("Failed to encode Vulcan plugin {label}: {error}"))
    })?;
    crate::atomic_file::write_file_atomically(path, &encoded).map_err(VulcanPluginRuntimeError::new)
}

/// Recursively copies a plugin package while rejecting symlinks and special files.
/// 递归复制插件包，同时拒绝符号链接与特殊文件。
///
/// # Parameters
/// 参数
/// `source` is a validated package directory and `destination` is a fresh staging directory.
/// `source` 是已校验插件包目录，`destination` 是全新的暂存目录。
fn copy_plugin_directory_tree(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "Failed to create plugin staging directory `{}`: {error}",
            destination.display()
        )
    })?;
    for entry in fs::read_dir(source).map_err(|error| {
        format!(
            "Failed to read plugin package `{}`: {error}",
            source.display()
        )
    })? {
        let entry =
            entry.map_err(|error| format!("Failed to read plugin package entry: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let metadata = fs::symlink_metadata(&source_path).map_err(|error| {
            format!(
                "Failed to inspect plugin package entry `{}`: {error}",
                source_path.display()
            )
        })?;
        if crate::metadata_is_link_or_reparse_point(&metadata) {
            return Err(format!(
                "Plugin package entry `{}` must not be a symbolic link or reparse point.",
                source_path.display()
            ));
        }
        if metadata.is_dir() {
            copy_plugin_directory_tree(&source_path, &destination_path)?;
        } else if metadata.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "Failed to copy plugin package file `{}`: {error}",
                    source_path.display()
                )
            })?;
        } else {
            return Err(format!(
                "Plugin package entry `{}` has an unsupported file type.",
                source_path.display()
            ));
        }
    }
    Ok(())
}

/// Reads one optional regular plugin state file for transactional rollback.
/// 读取一个可选普通插件状态文件以供事务回滚。
///
/// # Parameters
/// 参数
/// `path` is the exact state file owned by the selected plugin identity.
/// `path` 是所选插件身份拥有的精确状态文件。
///
/// # Returns
/// 返回值
/// Returns absent state or the exact file bytes after rejecting links and special files.
/// 在拒绝链接与特殊文件后返回缺失状态或精确文件字节。
fn read_optional_plugin_state_file(
    path: &Path,
) -> Result<Option<Vec<u8>>, VulcanPluginRuntimeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Failed to inspect plugin state file: {error}"
            )));
        }
    };
    if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Plugin state path `{}` is not a regular non-symbolic-link or reparse-point file.",
            path.display()
        )));
    }
    fs::read(path).map(Some).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to read plugin state file `{}`: {error}",
            path.display()
        ))
    })
}

/// Removes one optional plugin state file without following links.
/// 在不跟随链接的情况下移除一个可选插件状态文件。
///
/// # Parameters
/// 参数
/// `path` is the exact state file owned by the selected plugin identity.
/// `path` 是所选插件身份拥有的精确状态文件。
fn remove_plugin_state_file(path: &Path) -> Result<(), VulcanPluginRuntimeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Failed to inspect plugin state file: {error}"
            )));
        }
    };
    if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Plugin state path `{}` is not a regular non-symbolic-link or reparse-point file.",
            path.display()
        )));
    }
    fs::remove_file(path).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to remove plugin state file `{}`: {error}",
            path.display()
        ))
    })
}

/// Returns the current Unix time in milliseconds for installation records.
/// 返回用于安装记录的当前 Unix 毫秒时间。
fn current_unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

/// Selects the trusted package root used for plugin code and resource resolution.
/// 选择用于解析插件代码与资源的可信包根目录。
///
/// # Parameters
/// 参数
/// `state` owns private environments and `plugin` identifies the effective package generation.
/// `state` 持有私有环境，`plugin` 标识生效插件包代次。
///
/// # Returns
/// 返回值
/// Returns bundled source directly or an integrity-checked private user-plugin environment.
/// 系统内置插件返回源目录，用户插件返回经过完整性校验的私有环境。
fn plugin_execution_package_root(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<PathBuf, String> {
    match plugin.origin {
        VulcanPluginPackageOrigin::Bundled => plugin.package_root.canonicalize().map_err(|error| {
            format!(
                "Failed to resolve bundled Vulcan plugin package root `{}`: {error}",
                plugin.package_root.display()
            )
        }),
        VulcanPluginPackageOrigin::UserInstalled => {
            ensure_plugin_execution_environment(state, plugin)
        }
    }
}

/// Creates or reuses one immutable version-and-fingerprint plugin execution environment.
/// 创建或复用一个按版本与指纹隔离的不可变插件执行环境。
///
/// # Parameters
/// 参数
/// `state` owns the private runtime root and `plugin` is one validated effective package.
/// `state` 持有私有运行时根，`plugin` 是一个经过校验的生效插件包。
///
/// # Returns
/// 返回值
/// Returns the canonical published environment after optional registry installation succeeds.
/// 在可选注册表安装成功后返回规范化的已发布环境。
fn ensure_plugin_execution_environment(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<PathBuf, String> {
    let manifest = plugin
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Vulcan plugin `{}` has no valid manifest.", plugin.id))?;
    let version = plugin
        .version
        .as_deref()
        .ok_or_else(|| format!("Vulcan plugin `{}` has no validated version.", plugin.id))?;
    fs::create_dir_all(&state.plugin_runtime_root).map_err(|error| {
        format!(
            "Failed to create Vulcan plugin runtime root `{}`: {error}",
            state.plugin_runtime_root.display()
        )
    })?;
    let canonical_runtime_root = state.plugin_runtime_root.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve Vulcan plugin runtime root `{}`: {error}",
            state.plugin_runtime_root.display()
        )
    })?;
    let plugin_root = canonical_runtime_root.join(&plugin.id).join(version);
    fs::create_dir_all(&plugin_root).map_err(|error| {
        format!(
            "Failed to create Vulcan plugin version runtime root `{}`: {error}",
            plugin_root.display()
        )
    })?;
    let destination = plugin_root.join(&plugin.content_fingerprint);
    let expected_marker = VulcanPluginEnvironmentMarker {
        schema_version: 1,
        plugin_id: plugin.id.clone(),
        version: version.to_string(),
        content_fingerprint: plugin.content_fingerprint.clone(),
        install_mode: plugin_dependency_install_mode_key(
            manifest.execution.dependencies.install_mode,
        )
        .to_string(),
    };
    if destination.exists() {
        validate_plugin_environment_marker(&destination, &expected_marker)?;
        return destination.canonicalize().map_err(|error| {
            format!(
                "Failed to resolve Vulcan plugin environment `{}`: {error}",
                destination.display()
            )
        });
    }
    let staging = plugin_root.join(format!(".building-{}", Uuid::new_v4()));
    let build_result = (|| -> Result<(), String> {
        copy_plugin_directory_tree(&plugin.package_root, &staging)?;
        // Rehashing the copied package closes the catalog-to-copy mutation window before code runs.
        // 在代码运行前重新哈希复制包，关闭目录扫描到复制之间的篡改窗口。
        let staged_fingerprint = crate::catalog::compute_package_fingerprint(&staging)?;
        if staged_fingerprint != plugin.content_fingerprint {
            return Err(format!(
                "Vulcan plugin `{}` changed after catalog validation; refresh the catalog and authorize the new package generation.",
                plugin.id
            ));
        }
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Registry
        {
            install_plugin_registry_dependencies(state, manifest, &staging)?;
        }
        write_plugin_json_record(
            &staging.join(".vulcan-plugin-environment.json"),
            &expected_marker,
            "environment marker",
        )
        .map_err(|error| error.to_string())?;
        match fs::rename(&staging, &destination) {
            Ok(()) => Ok(()),
            Err(error) if destination.exists() => validate_plugin_environment_marker(
                &destination,
                &expected_marker,
            )
            .map_err(|validation_error| {
                format!("Concurrent environment publication failed: {error}; {validation_error}")
            }),
            Err(error) => Err(format!(
                "Failed to publish Vulcan plugin environment `{}`: {error}",
                destination.display()
            )),
        }
    })();
    if staging.exists() {
        fs::remove_dir_all(&staging).map_err(|cleanup_error| {
            format!(
                "{}; failed to clean unpublished environment `{}`: {cleanup_error}",
                build_result
                    .as_ref()
                    .err()
                    .cloned()
                    .unwrap_or_else(|| String::from("Environment publication completed")),
                staging.display()
            )
        })?;
    }
    build_result?;
    destination.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve published Vulcan plugin environment `{}`: {error}",
            destination.display()
        )
    })
}

/// Removes one exact unpublished plugin execution environment generation after update rollback.
/// 在更新回滚后移除单个精确的未发布插件执行环境代次。
///
/// # Parameters
/// 参数
/// `state` owns the runtime root; plugin, version, and fingerprint identify the exact generated directory.
/// `state` 持有运行时根；插件、版本与指纹共同标识精确生成目录。
///
/// # Returns
/// 返回值
/// Returns success when the generation is absent or removed without crossing the runtime-root boundary.
/// 当目标代次不存在或在不跨越运行时根边界的情况下被移除时返回成功。
fn remove_plugin_execution_environment_generation(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    version: &str,
    fingerprint: &str,
) -> Result<(), String> {
    let environment = state
        .plugin_runtime_root
        .join(plugin_id)
        .join(version)
        .join(fingerprint);
    if !environment.exists() {
        return Ok(());
    }
    let canonical_runtime_root = state.plugin_runtime_root.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve Vulcan plugin runtime root `{}`: {error}",
            state.plugin_runtime_root.display()
        )
    })?;
    let canonical_environment = environment.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve candidate plugin environment `{}`: {error}",
            environment.display()
        )
    })?;
    if !canonical_environment.starts_with(&canonical_runtime_root) {
        return Err(String::from(
            "Candidate plugin environment is outside the exact runtime root.",
        ));
    }
    fs::remove_dir_all(&canonical_environment).map_err(|error| {
        format!(
            "Failed to remove candidate plugin environment `{}`: {error}",
            canonical_environment.display()
        )
    })?;
    // version_root and plugin_root are removed only when empty; unrelated generations remain intact.
    // version_root 与 plugin_root 仅在为空时移除；无关代次保持不变。
    let version_root = environment
        .parent()
        .ok_or_else(|| String::from("Candidate environment has no version directory."))?;
    let plugin_root = version_root
        .parent()
        .ok_or_else(|| String::from("Candidate environment has no plugin directory."))?;
    let _ = fs::remove_dir(version_root);
    let _ = fs::remove_dir(plugin_root);
    Ok(())
}

/// Removes every obsolete execution environment after one plugin update commits.
/// 在单个插件更新提交后移除全部过期执行环境。
///
/// # Parameters
/// 参数
/// `state` owns the runtime root and the remaining arguments identify the sole committed generation.
/// `state` 持有运行时根，其余参数标识唯一已提交代次。
///
/// # Returns
/// 返回值
/// Returns success after only the committed generation remains, or a boundary-safe cleanup diagnostic.
/// 仅保留已提交代次后返回成功，失败时返回边界安全的清理诊断。
fn remove_obsolete_plugin_execution_environments(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    current_version: &str,
    current_fingerprint: &str,
) -> Result<(), String> {
    let plugin_root = state.plugin_runtime_root.join(plugin_id);
    if !plugin_root.exists() {
        return Ok(());
    }
    let canonical_runtime_root = state
        .plugin_runtime_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan plugin runtime root: {error}"))?;
    let canonical_plugin_root = plugin_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve plugin runtime directory: {error}"))?;
    if canonical_plugin_root.parent() != Some(canonical_runtime_root.as_path()) {
        return Err(String::from(
            "Plugin runtime directory is outside the exact runtime root.",
        ));
    }
    for version_entry in fs::read_dir(&canonical_plugin_root)
        .map_err(|error| format!("Failed to list plugin runtime versions: {error}"))?
    {
        let version_entry = version_entry
            .map_err(|error| format!("Failed to inspect plugin runtime version: {error}"))?;
        let metadata = fs::symlink_metadata(version_entry.path()).map_err(|error| {
            format!("Failed to inspect plugin runtime version metadata: {error}")
        })?;
        if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(format!(
                "Unexpected entry exists in plugin runtime directory `{}`.",
                version_entry.path().display()
            ));
        }
        let version_name = version_entry.file_name();
        if version_name.to_string_lossy() != current_version {
            fs::remove_dir_all(version_entry.path()).map_err(|error| {
                format!(
                    "Failed to remove obsolete plugin runtime version `{}`: {error}",
                    version_entry.path().display()
                )
            })?;
            continue;
        }
        for fingerprint_entry in fs::read_dir(version_entry.path())
            .map_err(|error| format!("Failed to list plugin runtime generations: {error}"))?
        {
            let fingerprint_entry = fingerprint_entry
                .map_err(|error| format!("Failed to inspect plugin runtime generation: {error}"))?;
            let metadata = fs::symlink_metadata(fingerprint_entry.path()).map_err(|error| {
                format!("Failed to inspect plugin runtime generation metadata: {error}")
            })?;
            if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(format!(
                    "Unexpected entry exists in plugin runtime version directory `{}`.",
                    fingerprint_entry.path().display()
                ));
            }
            if fingerprint_entry.file_name().to_string_lossy() != current_fingerprint {
                fs::remove_dir_all(fingerprint_entry.path()).map_err(|error| {
                    format!(
                        "Failed to remove obsolete plugin runtime generation `{}`: {error}",
                        fingerprint_entry.path().display()
                    )
                })?;
            }
        }
    }
    Ok(())
}

/// Validates one published plugin environment against its exact expected identity.
/// 根据精确预期身份校验一个已发布插件环境。
fn validate_plugin_environment_marker(
    environment_root: &Path,
    expected: &VulcanPluginEnvironmentMarker,
) -> Result<(), String> {
    let marker_path = environment_root.join(".vulcan-plugin-environment.json");
    let marker_bytes = fs::read(&marker_path).map_err(|error| {
        format!(
            "Failed to read Vulcan plugin environment marker `{}`: {error}",
            marker_path.display()
        )
    })?;
    let actual: VulcanPluginEnvironmentMarker =
        serde_json::from_slice(&marker_bytes).map_err(|error| {
            format!(
                "Failed to parse Vulcan plugin environment marker `{}`: {error}",
                marker_path.display()
            )
        })?;
    if &actual != expected {
        return Err(format!(
            "Vulcan plugin environment marker `{}` does not match the current package identity.",
            marker_path.display()
        ));
    }
    Ok(())
}

/// Installs registry-authorized Node and Python dependencies inside one unpublished environment.
/// 在一个未发布环境内安装已获注册表授权的 Node 与 Python 依赖。
fn install_plugin_registry_dependencies(
    state: &VulcanPluginManagerState,
    manifest: &VulcanPluginManifest,
    environment_root: &Path,
) -> Result<(), String> {
    let dependency_manifest = manifest
        .execution
        .dependencies
        .manifest
        .as_deref()
        .ok_or_else(|| String::from("Plugin dependency installation requires a manifest."))?;
    let dependency_file =
        canonical_plugin_file(environment_root, dependency_manifest, "dependency manifest")?;
    let dependencies = PackageDependencyManifest::load_from_path(&dependency_file)?;
    let platform = current_managed_runtime_platform_key()?;
    if let Some(node) = &dependencies.node_runtime {
        let runtime = resolve_managed_runtime_install(
            &state.managed_runtime_distribution_root,
            ManagedRuntimeKind::Node,
            &node.version,
            &platform,
        )?;
        let package_manager = resolve_plugin_package_manager(
            &state.managed_runtime_distribution_root,
            "node",
            "pnpm",
            &node.package_manager_version,
            "any",
        )?;
        let package_json = canonical_plugin_file(
            environment_root,
            &node.package_json,
            "Node package manifest",
        )?;
        let lockfile =
            canonical_plugin_file(environment_root, &node.lockfile, "Node dependency lockfile")?;
        if package_json.parent() != lockfile.parent() {
            return Err(String::from(
                "Vulcan plugin Node package.json and lockfile must share one directory.",
            ));
        }
        let package_directory = package_json
            .parent()
            .ok_or_else(|| "Node package manifest has no parent directory.".to_string())?;
        let store_directory = state.plugin_runtime_root.join(".cache").join("pnpm");
        fs::create_dir_all(&store_directory)
            .map_err(|error| format!("Failed to create Vulcan plugin pnpm store: {error}"))?;
        run_plugin_dependency_command(
            Command::new(vulcan_runtime_assets::native_host_path(&runtime.executable))
                .arg(vulcan_runtime_assets::native_host_path(&package_manager))
                .arg("install")
                .arg("--frozen-lockfile")
                .arg("--ignore-scripts")
                .arg("--node-linker")
                .arg("hoisted")
                .arg("--store-dir")
                .arg(vulcan_runtime_assets::native_host_path(&store_directory))
                .current_dir(vulcan_runtime_assets::native_host_path(package_directory)),
            "install Node plugin dependencies",
        )?;
    }
    if let Some(python) = &dependencies.python_runtime {
        let runtime = resolve_managed_runtime_install(
            &state.managed_runtime_distribution_root,
            ManagedRuntimeKind::Python,
            &python.version,
            &platform,
        )?;
        let package_manager = resolve_plugin_package_manager(
            &state.managed_runtime_distribution_root,
            "python",
            "uv",
            &python.package_manager_version,
            &platform,
        )?;
        let lockfile = canonical_plugin_file(
            environment_root,
            &python.lockfile,
            "Python dependency lockfile",
        )?;
        let virtual_environment = environment_root.join(".venv");
        run_plugin_dependency_command(
            Command::new(vulcan_runtime_assets::native_host_path(&package_manager))
                .arg("venv")
                .arg("--python")
                .arg(vulcan_runtime_assets::native_host_path(&runtime.executable))
                .arg(vulcan_runtime_assets::native_host_path(
                    &virtual_environment,
                ))
                .current_dir(vulcan_runtime_assets::native_host_path(environment_root))
                .env("UV_NO_CONFIG", "1"),
            "create Python plugin environment",
        )?;
        let store_directory = state.plugin_runtime_root.join(".cache").join("uv");
        fs::create_dir_all(&store_directory)
            .map_err(|error| format!("Failed to create Vulcan plugin uv store: {error}"))?;
        run_plugin_dependency_command(
            Command::new(vulcan_runtime_assets::native_host_path(&package_manager))
                .arg("pip")
                .arg("sync")
                .arg(vulcan_runtime_assets::native_host_path(&lockfile))
                .arg("--python")
                .arg(vulcan_runtime_assets::native_host_path(
                    &plugin_python_environment_executable(environment_root),
                ))
                .arg("--cache-dir")
                .arg(vulcan_runtime_assets::native_host_path(&store_directory))
                .current_dir(vulcan_runtime_assets::native_host_path(environment_root))
                .env("UV_NO_CONFIG", "1"),
            "install Python plugin dependencies",
        )?;
    }
    Ok(())
}

/// Resolves one exact shared package-manager executable from its immutable install manifest.
/// 从不可变安装清单解析一个精确的共享包管理器可执行文件。
fn resolve_plugin_package_manager(
    distribution_root: &Path,
    family: &str,
    runtime: &str,
    version: &str,
    platform: &str,
) -> Result<PathBuf, String> {
    let install_root = distribution_root.join(family).join(if runtime == "pnpm" {
        format!("pnpm-{version}")
    } else {
        format!("uv-{version}-{platform}")
    });
    let manifest = luaskills::read_install_manifest(&install_root)?;
    if manifest.runtime != runtime || manifest.version != version || manifest.platform != platform {
        return Err(format!(
            "Managed package-manager identity mismatch at `{}`.",
            install_root.display()
        ));
    }
    let relative = Path::new(&manifest.executable);
    if relative.is_absolute()
        || manifest.executable.trim().is_empty()
        || !relative
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
    {
        return Err(format!(
            "Managed package-manager executable path is unsafe at `{}`.",
            install_root.display()
        ));
    }
    let canonical_root = install_root.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve managed package-manager root `{}`: {error}",
            install_root.display()
        )
    })?;
    let executable = install_root
        .join(relative)
        .canonicalize()
        .map_err(|error| {
            format!(
                "Failed to resolve managed package-manager executable in `{}`: {error}",
                install_root.display()
            )
        })?;
    if !executable.starts_with(&canonical_root) || !executable.is_file() {
        return Err(format!(
            "Managed package-manager executable escapes `{}`.",
            install_root.display()
        ));
    }
    Ok(executable)
}

/// Executes one bounded dependency installation command and preserves its full diagnostics.
/// 执行一个有边界的依赖安装命令并保留完整诊断。
fn run_plugin_dependency_command(command: &mut Command, label: &str) -> Result<(), String> {
    apply_release_no_window_to_command(command);
    let output = command
        .output()
        .map_err(|error| format!("Failed to {label}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "Failed to {label} (status {}): stdout={} stderr={}",
        output.status,
        decode_complete_user_text(&output.stdout)
            .map(|decoded| decoded.content)
            .unwrap_or_else(|_| String::from_utf8_lossy(&output.stdout).into_owned()),
        decode_complete_user_text(&output.stderr)
            .map(|decoded| decoded.content)
            .unwrap_or_else(|_| String::from_utf8_lossy(&output.stderr).into_owned())
    ))
}

/// Returns the stable manifest key for one dependency installation mode.
/// 返回单个依赖安装模式的稳定清单键。
fn plugin_dependency_install_mode_key(mode: VulcanPluginDependencyInstallMode) -> &'static str {
    match mode {
        VulcanPluginDependencyInstallMode::Bundled => "bundled",
        VulcanPluginDependencyInstallMode::Registry => "registry",
    }
}

/// Returns the platform-specific interpreter inside one plugin-owned Python environment.
/// 返回插件自有 Python 环境内的平台专用解释器。
fn plugin_python_environment_executable(environment_root: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        environment_root
            .join(".venv")
            .join("Scripts")
            .join("python.exe")
    }
    #[cfg(not(windows))]
    {
        environment_root.join(".venv").join("bin").join("python")
    }
}

/// Builds one permission preview from a validated catalog descriptor.
/// 从一个已校验目录描述符构建权限预览。
///
/// # Parameters
/// 参数
/// `plugin` is the effective descriptor selected after root precedence.
/// `plugin` 是应用根目录优先级后选定的生效描述符。
///
/// # Returns
/// 返回值
/// Returns a complete stable preview suitable for authorization UI and persistence comparison.
/// 返回适用于授权界面与持久化比较的完整稳定预览。
fn build_permission_preview(
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<VulcanPluginPermissionPreview, VulcanPluginRuntimeError> {
    let manifest = plugin.manifest.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new(format!(
            "Vulcan plugin `{}` has no valid manifest.",
            plugin.id
        ))
    })?;
    let mut permissions = manifest
        .permissions
        .iter()
        .map(|capability| plugin_permission_key(*capability).to_string())
        .collect::<Vec<_>>();
    permissions.sort();
    // mcpServers exposes exact ownership and Secret usage without resolving endpoints or credential values.
    // mcpServers 暴露精确归属与 Secret 使用，但不解析端点或凭据值。
    let mut mcp_servers = manifest
        .mcp
        .servers
        .iter()
        .map(|server| VulcanPluginMcpPermissionPreview {
            server_id: server.id.clone(),
            name: server.name.clone(),
            transport: match &server.transport {
                VulcanPluginMcpTransportManifest::Stdio { .. } => String::from("stdio"),
                VulcanPluginMcpTransportManifest::Http { .. } => String::from("http"),
                VulcanPluginMcpTransportManifest::Sse { .. } => String::from("sse"),
            },
            secret_keys: server.declared_secret_keys().into_iter().collect(),
        })
        .collect::<Vec<_>>();
    mcp_servers.sort_by(|left, right| left.server_id.cmp(&right.server_id));
    Ok(VulcanPluginPermissionPreview {
        plugin_id: plugin.id.clone(),
        version: plugin.version.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has no validated version.",
                plugin.id
            ))
        })?,
        content_fingerprint: plugin.content_fingerprint.clone(),
        origin: match plugin.origin {
            VulcanPluginPackageOrigin::Bundled => String::from("bundled"),
            VulcanPluginPackageOrigin::UserInstalled => String::from("user_installed"),
        },
        permissions,
        hook_points: manifest
            .lua_runtime
            .as_ref()
            .map_or_else(Vec::new, |runtime| {
                runtime
                    .hooks
                    .iter()
                    .map(|hook| hook.point.clone())
                    .collect()
            }),
        ui_mount_points: manifest
            .ui
            .mounts
            .iter()
            .map(|mount| plugin_ui_mount_point_key(mount.point).to_string())
            .collect(),
        binary_ids: manifest
            .execution
            .native_binaries
            .iter()
            .map(|entry| entry.id.clone())
            .collect(),
        node_entry_ids: manifest
            .execution
            .node_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect(),
        python_entry_ids: manifest
            .execution
            .python_entries
            .iter()
            .map(|entry| entry.id.clone())
            .collect(),
        skill_ids: plugin
            .skills
            .iter()
            .map(|skill| skill.name.clone())
            .collect(),
        agent_ids: plugin.agents.iter().map(|agent| agent.id.clone()).collect(),
        has_lua_runtime: manifest.has_lua_runtime(),
        mcp_servers,
        install_secret_requirements: manifest
            .mcp
            .servers
            .iter()
            .filter(|server| {
                matches!(
                    server.transport,
                    VulcanPluginMcpTransportManifest::Stdio { .. }
                )
            })
            .flat_map(|server| {
                server.declared_secret_keys().into_iter().map(|secret_key| {
                    VulcanPluginInstallSecretRequirement {
                        server_id: server.id.clone(),
                        secret_key,
                    }
                })
            })
            .collect(),
    })
}

/// Resolves the Lua protocol required by an execution path that cannot operate without it.
/// 解析无法脱离 Lua 协议运行的执行路径所要求的 Lua 协议。
///
/// # Parameters
/// 参数
/// `manifest` is the already validated plugin manifest captured by the current generation.
/// `manifest` 是当前代次捕获且已经校验的插件清单。
///
/// # Returns
/// 返回值
/// Returns the exact Lua declaration or an explicit invariant error for a non-Lua plugin.
/// 返回精确 Lua 声明；若插件不含 Lua 协议则返回显式不变量错误。
fn required_lua_runtime(
    manifest: &VulcanPluginManifest,
) -> Result<&VulcanPluginLuaRuntimeManifest, VulcanPluginRuntimeError> {
    manifest.lua_runtime.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new(format!(
            "Vulcan plugin `{}` does not declare a Lua runtime.",
            manifest.id
        ))
    })
}

/// Returns the stable persisted key for one plugin permission.
/// 返回单个插件权限的稳定持久化键。
pub fn plugin_permission_key(capability: VulcanPluginPermission) -> &'static str {
    match capability {
        VulcanPluginPermission::RuntimeLuaModules => "runtime_lua_modules",
        VulcanPluginPermission::NetworkHttps => "network_https",
        VulcanPluginPermission::ProcessBinary => "process_binary",
        VulcanPluginPermission::ProcessNode => "process_node",
        VulcanPluginPermission::ProcessPython => "process_python",
        VulcanPluginPermission::ProcessSession => "process_session",
        VulcanPluginPermission::Config => "config",
        VulcanPluginPermission::SecretsRead => "secrets_read",
        VulcanPluginPermission::UiHostContext => "ui_host_context",
        VulcanPluginPermission::ModelCatalogRead => "model_catalog_read",
        VulcanPluginPermission::ModelInvoke => "model_invoke",
        VulcanPluginPermission::DependencyRegistryNetwork => "dependency_registry_network",
        VulcanPluginPermission::StorageSqlite => "storage_sqlite",
        VulcanPluginPermission::StorageLancedb => "storage_lancedb",
    }
}

/// Returns the stable host key for one dynamic UI mount point.
/// 返回单个动态 UI 挂载点的稳定宿主键。
fn plugin_ui_mount_point_key(point: crate::VulcanPluginUiMountPoint) -> &'static str {
    match point {
        crate::VulcanPluginUiMountPoint::SkillsPluginDetail => "skills.plugin.detail",
        crate::VulcanPluginUiMountPoint::WorkspaceMain => "workspace.main",
        crate::VulcanPluginUiMountPoint::ConversationRightPanel => "conversation.right_panel",
    }
}

/// Checks whether one effective plugin descriptor has a current complete authorization record.
/// 检查一个生效插件描述符是否拥有当前且完整的授权记录。
///
/// # Parameters
/// 参数
/// `state_root` owns authorization state and `plugin` is the exact effective package generation.
/// `state_root` 持有授权状态，`plugin` 是精确的生效插件代次。
///
/// # Returns
/// 返回值
/// Returns true for bundled plugins or an exact matching user authorization record.
/// 对内建插件或精确匹配的用户授权记录返回 true。
fn plugin_is_authorized(
    state_root: &Path,
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<bool, String> {
    if plugin.origin == VulcanPluginPackageOrigin::Bundled {
        return Ok(true);
    }
    let path = plugin_authorization_state_path(state_root, &plugin.id);
    if !path.exists() {
        return Ok(false);
    }
    let value = read_plugin_json_state(&path, "authorization").map_err(|error| error.message)?;
    let record: VulcanPluginAuthorizationRecord =
        serde_json::from_value(value).map_err(|error| {
            format!(
                "Failed to decode Vulcan plugin authorization `{}`: {error}",
                path.display()
            )
        })?;
    let preview = build_permission_preview(plugin).map_err(|error| error.message)?;
    Ok(record.schema_version == 1
        && record.plugin_id == plugin.id
        && Some(record.version.as_str()) == plugin.version.as_deref()
        && record.content_fingerprint == plugin.content_fingerprint
        && record.permissions == preview.permissions)
}

/// Reads one regular plugin JSON document after rejecting links and reparse points.
/// 在拒绝链接与重解析点后读取一个普通插件 JSON 文档。
///
/// # Parameters
/// 参数
/// `path` is the exact document path and `label` identifies it in diagnostics.
/// `path` 是精确文档路径，`label` 用于诊断标识。
///
/// # Returns
/// 返回值
/// Returns the decoded JSON value.
/// 返回解码后的 JSON 值。
fn read_plugin_json_state(path: &Path, label: &str) -> Result<Value, VulcanPluginRuntimeError> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to inspect Vulcan plugin {label} `{}`: {error}",
            path.display()
        ))
    })?;
    if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Vulcan plugin {label} `{}` must be a regular non-symbolic-link or reparse-point file.",
            path.display()
        )));
    }
    let bytes = fs::read(path).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to read Vulcan plugin {label} `{}`: {error}",
            path.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to decode Vulcan plugin {label} `{}`: {error}",
            path.display()
        ))
    })
}

/// Validates one complete plugin configuration against its package-owned schema.
/// 根据插件包持有的 Schema 校验一份完整插件配置。
///
/// # Parameters
/// 参数
/// `package_root` owns the schema, `config` identifies its file, and `value` is untrusted state.
/// `package_root` 持有 Schema，`config` 标识其文件，`value` 是不可信状态。
///
/// # Returns
/// 返回值
/// Returns unit when the value satisfies the schema.
/// 配置值满足 Schema 时返回空值。
fn validate_plugin_config_value(
    package_root: &Path,
    config: &crate::VulcanPluginConfigManifest,
    value: &Value,
) -> Result<(), VulcanPluginRuntimeError> {
    let schema_path =
        canonical_plugin_file(package_root, &config.schema_file, "configuration schema")
            .map_err(VulcanPluginRuntimeError::new)?;
    let schema = read_plugin_json_state(&schema_path, "configuration schema")?;
    crate::catalog::validate_json_schema_instance(&schema, value, "plugin configuration")
        .map_err(VulcanPluginRuntimeError::new)
}

/// Validates one plugin action value when its manifest declares a schema.
/// 当插件清单声明 Schema 时校验一个 action 值。
///
/// # Parameters
/// 参数
/// `package_root` owns the optional schema path, `value` is untrusted, and `label` names the phase.
/// `package_root` 持有可选 Schema 路径，`value` 不可信，`label` 命名校验阶段。
///
/// # Returns
/// 返回值
/// Returns unit for absent schemas or values that satisfy the declared schema.
/// Schema 不存在或值满足声明 Schema 时返回空值。
fn validate_plugin_action_value(
    package_root: &Path,
    schema_file: Option<&str>,
    value: &Value,
    label: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    let Some(schema_file) = schema_file else {
        return Ok(());
    };
    let schema_path = canonical_plugin_file(package_root, schema_file, label)
        .map_err(VulcanPluginRuntimeError::new)?;
    let schema = read_plugin_json_state(&schema_path, label)?;
    crate::catalog::validate_json_schema_instance(&schema, value, label)
        .map_err(VulcanPluginRuntimeError::new)
}

/// Requires one exact JSON object from provider-visible arguments.
/// 从 Provider 可见参数中要求一个精确 JSON 对象。
fn require_argument_object(
    arguments: Value,
) -> Result<Map<String, Value>, VulcanPluginRuntimeError> {
    arguments.as_object().cloned().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Vulcan Plugin tool arguments must be a JSON object.")
    })
}

/// Removes and validates the host-owned public session identifier.
/// 移除并校验宿主自有的公开会话标识。
fn take_session_id(arguments: &mut Map<String, Value>) -> Result<String, VulcanPluginRuntimeError> {
    arguments
        .remove(VULCAN_PLUGIN_SESSION_ID_ARGUMENT)
        .and_then(|value| value.as_str().map(str::to_owned))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| VulcanPluginRuntimeError::new("Vulcan Plugin session_id is required."))
}

/// Builds one exact plugin-owned payload without accepting an argument-level operation override.
/// 构建一个精确的插件自有载荷，同时拒绝参数层覆盖操作名。
fn plugin_payload(
    operation: &str,
    mut arguments: Map<String, Value>,
) -> Result<Value, VulcanPluginRuntimeError> {
    if let Some(reserved) = VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS
        .iter()
        .find(|reserved| arguments.contains_key(**reserved))
    {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Vulcan Plugin tool arguments must not contain host-owned field `{reserved}`."
        )));
    }
    arguments.insert(String::from("op"), Value::String(operation.to_owned()));
    Ok(Value::Object(arguments))
}

/// Resolves one bounded host timeout from the tool manifest and exact argument object.
/// 根据工具清单与精确参数对象解析一个有界宿主超时。
fn resolve_host_timeout(
    tool: &VulcanPluginToolDescriptor,
    arguments: &Map<String, Value>,
) -> Result<u64, VulcanPluginRuntimeError> {
    let execution = &tool.manifest.execution;
    let operation_timeout = match (&execution.timeout_argument, execution.max_timeout_ms) {
        (Some(argument), Some(maximum)) => match arguments.get(argument) {
            Some(value) => {
                let selected = value.as_u64().ok_or_else(|| {
                    VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin timeout argument `{argument}` must be an unsigned integer."
                    ))
                })?;
                if selected > maximum {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Vulcan Plugin timeout argument `{argument}` exceeds {maximum} ms."
                    )));
                }
                selected
            }
            None => execution.timeout_ms,
        },
        (None, None) => execution.timeout_ms,
        _ => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan Plugin tool `{}` has an invalid timeout manifest.",
                tool.id
            )));
        }
    };
    operation_timeout
        .checked_add(execution.timeout_grace_ms)
        .ok_or_else(|| VulcanPluginRuntimeError::new("Vulcan Plugin host timeout overflowed."))
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
fn rebuild_enabled_surface(snapshot: &mut VulcanPluginCatalogSnapshot) {
    snapshot.enabled_tools.clear();
    snapshot.enabled_hooks.clear();
    snapshot.enabled_mcp_servers.clear();
    snapshot.prompt_contributions.clear();
    snapshot.enabled_skills.clear();
    snapshot.enabled_agents.clear();
    for plugin in snapshot.plugins.values() {
        if plugin.availability != VulcanPluginAvailability::Enabled
            || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
        {
            continue;
        }
        for skill in &plugin.skills {
            snapshot
                .enabled_skills
                .insert(skill.name.clone(), skill.clone());
        }
        for agent in &plugin.agents {
            snapshot
                .enabled_agents
                .insert(agent.id.clone(), agent.clone());
        }
        for server in &plugin.mcp_servers {
            snapshot.enabled_mcp_servers.insert(
                format!("{}:{}", server.plugin_id, server.id),
                server.clone(),
            );
        }
        if !plugin.runtime_execution_available {
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
    operation: impl FnOnce(&LuaEngine) -> Result<T, VulcanPluginRuntimeError> + Send + 'static,
) -> Result<T, VulcanPluginRuntimeError>
where
    T: Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let engine = engine
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("LuaSkills engine lock is poisoned."))?;
        operation(&engine)
    })
    .await
    .map_err(|error| {
        VulcanPluginRuntimeError::new(format!("Vulcan Plugin runtime task failed: {error}"))
    })?
}

/// Parses one LuaSkills JSON response and requires a successful top-level state.
/// 解析一条 LuaSkills JSON 响应并要求成功的顶层状态。
fn parse_engine_response(raw: &str, operation: &str) -> Result<Value, VulcanPluginRuntimeError> {
    let response: Value = serde_json::from_str(raw).map_err(|error| {
        VulcanPluginRuntimeError::new(format!("Failed to decode {operation} response: {error}"))
    })?;
    if response.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = response
            .get("error")
            .or_else(|| response.get("message"))
            .map(render_error_value)
            .unwrap_or_else(|| response.to_string());
        return Err(VulcanPluginRuntimeError::new(format!(
            "{operation} failed: {message}"
        )));
    }
    Ok(response)
}

/// Decodes one exact plugin JSON frame nested in a System lease response.
/// 解码嵌套在系统租约响应中的精确插件 JSON 帧。
fn parse_plugin_result(response: &Value) -> Result<Value, VulcanPluginRuntimeError> {
    let frame = response
        .get("result")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new("Vulcan Plugin runtime eval returned no JSON frame.")
        })?;
    let plugin: Value = serde_json::from_str(frame).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to decode Vulcan Plugin JSON frame: {error}"
        ))
    })?;
    if plugin.get("ok").and_then(Value::as_bool) != Some(true) {
        let message = plugin
            .get("error")
            .map(render_error_value)
            .unwrap_or_else(|| String::from("Unknown Vulcan Plugin runtime error."));
        return Err(VulcanPluginRuntimeError::new(format!(
            "Vulcan Plugin request failed: {message}"
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
) -> Result<String, VulcanPluginRuntimeError> {
    response
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("System lease response omitted {field}."))
        })
}

/// Requires one unsigned integer field from a successful engine response.
/// 从成功引擎响应中要求一个无符号整数字段。
fn required_response_u64(response: &Value, field: &str) -> Result<u64, VulcanPluginRuntimeError> {
    response.get(field).and_then(Value::as_u64).ok_or_else(|| {
        VulcanPluginRuntimeError::new(format!("System lease response omitted {field}."))
    })
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

/// Closes one LuaSkills System lease synchronously during manager drop.
/// 在管理器释放期间同步关闭一个 LuaSkills 系统租约。
fn close_system_lease_sync(
    engine: &LuaEngine,
    session: &ManagedVulcanPluginSession,
) -> Result<Value, VulcanPluginRuntimeError> {
    let raw = engine
        .close_system_runtime_lease_json(
            &json!({
                "lease_id": session.lease_id,
                "sid": session.sid,
                "generation": session.generation,
            })
            .to_string(),
        )
        .map_err(VulcanPluginRuntimeError::new)?;
    parse_engine_response(&raw, "close Vulcan Plugin runtime lease")
}

/// Closes one raw LuaSkills System lease synchronously during manager drop.
/// 在管理器释放期间同步关闭一个原始 LuaSkills 系统租约。
fn close_raw_system_lease_sync(
    engine: &LuaEngine,
    lease_id: &str,
    sid: &str,
    generation: u64,
) -> Result<Value, VulcanPluginRuntimeError> {
    let raw = engine
        .close_system_runtime_lease_json(
            &json!({
                "lease_id": lease_id,
                "sid": sid,
                "generation": generation,
            })
            .to_string(),
        )
        .map_err(VulcanPluginRuntimeError::new)?;
    parse_engine_response(&raw, "close Vulcan Plugin hook lease")
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
    use crate::{VulcanPluginStorageService, VulcanPluginUiEventPublisher};
    use luaskills::{LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    use std::fs;
    use vulcan_hooks::{
        HookActor, HookCapabilitySet, HookContext, HookMode, HookPayload, HookRegistration,
        HookScope,
    };

    /// Verifies fixed bridge requests preserve strict JSON object payload semantics.
    /// 验证固定桥请求严格保留 JSON 对象载荷语义。
    #[test]
    fn bridge_request_requires_object_payload() {
        // objectRequest represents the typed object emitted by LuaSkills 0.5.5.
        // objectRequest 表示 LuaSkills 0.5.5 发出的强类型对象。
        let object_request = json!({"plugin_id": "fixture", "payload": {}});
        // extracted is the exact borrowed identity and object payload.
        // extracted 是精确借用的身份与对象载荷。
        let (plugin_id, payload) = bridge_request_identity_and_payload(&object_request)
            .expect("accept object bridge payload");
        assert_eq!(plugin_id, "fixture");
        assert!(payload.is_empty());

        // arrayRequest proves the removed Lua empty-table compatibility path cannot return.
        // arrayRequest 证明已移除的 Lua 空表兼容路径不会重新出现。
        let array_request = json!({"plugin_id": "fixture", "payload": []});
        // error is the stable protocol rejection for a non-object payload.
        // error 是非对象载荷对应的稳定协议拒绝信息。
        let error = bridge_request_identity_and_payload(&array_request)
            .expect_err("reject array bridge payload");
        assert_eq!(
            error,
            "Vulcan plugin host request requires object field `payload`."
        );
    }

    /// RecordingStorageService simulates the host database boundary for real plugin Lua tests.
    /// RecordingStorageService 为真实插件 Lua 测试模拟宿主数据库边界。
    #[derive(Default)]
    struct RecordingStorageService {
        /// Ordered authenticated storage calls received from the fixed bridge.
        /// 从固定桥接层收到的有序已认证存储调用。
        calls: Mutex<Vec<VulcanPluginStorageRequest>>,
    }

    impl VulcanPluginStorageService for RecordingStorageService {
        /// Records one request and returns deterministic rows required by the statistics Hook.
        /// 记录一条请求，并返回统计 Hook 所需的确定性数据行。
        ///
        /// # Parameters
        /// 参数
        /// `request` is the authenticated fixed-bridge storage operation.
        /// `request` 是经过认证的固定桥接存储操作。
        ///
        /// # Returns
        /// 返回值
        /// Returns deterministic migration, write, and query envelopes.
        /// 返回确定性的迁移、写入与查询信封。
        fn execute(&self, request: VulcanPluginStorageRequest) -> Result<Value, String> {
            // sql is captured before the request moves into the ordered assertion log.
            // sql 在请求移入有序断言日志前被提取。
            let sql = request
                .payload
                .get("sql")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            // operation selects the exact production response shape expected by bridge.lua.
            // operation 选择 bridge.lua 期望的精确生产响应形状。
            let operation = request.operation;
            self.calls
                .lock()
                .expect("lock recording storage calls")
                .push(request);
            match operation {
                VulcanPluginStorageOperation::SqliteMigrate => {
                    Ok(json!({"applied": true, "version": 1}))
                }
                VulcanPluginStorageOperation::SqliteExecute => Ok(json!({"affected_rows": 1})),
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("SELECT model_ref FROM models") =>
                {
                    Ok(json!({"rows": [{"model_ref": 1}], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains(
                        "SELECT model_ref AS modelRef FROM models WHERE model_ref = ?1",
                    ) =>
                {
                    Ok(json!({"rows": [{"modelRef": 1}], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("SELECT date(?1) AS normalized_date") =>
                {
                    Ok(json!({"rows": [{"normalized_date": "2026-07-14"}], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("SELECT COUNT(*) AS count") =>
                {
                    // expiredCount forces the automatic-retention branch without requiring a real SQLite fixture.
                    // expiredCount 强制进入自动保留期分支，无需依赖真实 SQLite 测试夹具。
                    Ok(json!({"rows": [{"count": 1}], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("FROM statistics_meta") =>
                {
                    Ok(json!({"rows": [{"revision": 1}], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("SELECT COUNT(*) AS requests") =>
                {
                    Ok(json!({"rows": [{
                        "requests": 1,
                        "successes": 1,
                        "failures": 0,
                        "cancelled": 0,
                        "inputTokens": 10,
                        "outputTokens": 20,
                        "reasoningTokens": 5,
                        "providerTotalTokens": 35,
                        "totalTokens": 35,
                        "cachedInputTokens": 14,
                        "cacheCreationInputTokens": 2,
                        "unknownTokenRequests": 0,
                        "inconsistentTokenRequests": 0
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("COALESCE(SUM(request_count)") =>
                {
                    Ok(json!({"rows": [{
                        "requests": 1,
                        "successes": 1,
                        "failures": 0,
                        "cancelled": 0,
                        "retries": 0,
                        "inputTokens": 10,
                        "outputTokens": 20,
                        "reasoningTokens": 5,
                        "totalTokens": 35,
                        "providerTotalTokens": 35,
                        "unknownTokenRequests": 0,
                        "inconsistentTokenRequests": 0,
                        "cachedInputTokens": 14,
                        "cacheCreationInputTokens": 2,
                        "durationMs": 500,
                        "averageDurationMs": 500,
                        "mainAgentTokens": 35,
                        "subagentTokens": 0,
                        "internalTokens": 0,
                        "longestRequestMs": 500,
                        "peakRequestTokens": 35,
                        "averageFirstTokenMs": 120,
                        "activeDays": 1
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery if sql.contains("WITH active AS") => {
                    Ok(json!({"rows": [{
                        "longestStreakDays": 1,
                        "currentStreakDays": 1
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery if sql.contains("WITH filtered AS") => {
                    Ok(json!({"rows": [{
                        "date": "2026-07-14",
                        "value": 35,
                        "requests": 1,
                        "successes": 1,
                        "topProviderName": "Provider One",
                        "topModelName": "Model One"
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery if sql.contains("MAX(daily_tokens)") => {
                    Ok(json!({"rows": [{"peakDailyTokens": 35}], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery if sql.contains("WITH ranked AS") => {
                    Ok(json!({"rows": [{
                        "date": "2026-07-14",
                        "seriesRef": 1,
                        "seriesKey": "provider-1/model-1",
                        "label": "Model One",
                        "value": 10
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("JOIN models m ON m.model_ref = d.model_ref") =>
                {
                    Ok(json!({"rows": [{
                        "modelRef": 1,
                        "providerKey": "provider-1",
                        "providerName": "Provider One",
                        "modelKey": "model-1",
                        "modelName": "Model One",
                        "totalTokens": 35,
                        "requests": 1,
                        "sharePercent": 100
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("SELECT model_ref AS modelRef") =>
                {
                    Ok(json!({"rows": [{
                        "modelRef": 1,
                        "providerName": "Provider One",
                        "modelName": "Model One"
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("SELECT e.request_id AS requestId") =>
                {
                    Ok(json!({"rows": [{
                        "requestId": "request-1",
                        "completedAtMs": 1_500,
                        "providerName": "Provider One",
                        "modelName": "Model One",
                        "ownerKind": "main_agent",
                        "ownerName": "Main Agent",
                        "status": "succeeded",
                        "inputTokens": 10,
                        "outputTokens": 20,
                        "totalTokens": 35,
                        "providerTotalTokens": 35,
                        "totalTokenSource": "reported",
                        "cachedInputTokens": 14,
                        "timeToFirstTokenMs": 120,
                        "durationMs": 500,
                        "retryCount": 0,
                        "errorCategory": null,
                        "errorCode": null
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery
                    if sql.contains("WHERE e.status = 'failed'") =>
                {
                    Ok(json!({"rows": [{
                        "errorCategory": "provider_http",
                        "errorCode": "429",
                        "count": 1
                    }], "row_count": 1}))
                }
                VulcanPluginStorageOperation::SqliteQuery => {
                    Ok(json!({"rows": [], "row_count": 0}))
                }
                VulcanPluginStorageOperation::SqliteFlush => Ok(json!({"flushed": true})),
                _ => Err(String::from(
                    "Unexpected storage operation in SQLite Hook test.",
                )),
            }
        }
    }

    /// RecordingUiEventPublisher captures plugin-scoped revision notifications.
    /// RecordingUiEventPublisher 捕获插件作用域的修订通知。
    #[derive(Default)]
    struct RecordingUiEventPublisher {
        /// Ordered invalidation events published after durable writes.
        /// 持久写入后发布的有序失效事件。
        events: Mutex<Vec<VulcanPluginUiInvalidation>>,
    }

    impl VulcanPluginUiEventPublisher for RecordingUiEventPublisher {
        /// Records one validated plugin invalidation.
        /// 记录一条已经校验的插件失效通知。
        fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
            self.events
                .lock()
                .expect("lock recording UI events")
                .push(event);
            Ok(())
        }
    }

    /// Writes one hook-only package whose Lua module retains an invocation counter.
    /// 写入一个仅含 hook 的插件包，其 Lua 模块会保留调用计数器。
    fn write_hook_package(root: &Path) {
        let package = root.join("hook-plugin");
        fs::create_dir_all(package.join("runtime")).expect("create hook runtime directory");
        fs::write(package.join("dependencies.yaml"), "{}\n").expect("write dependencies");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
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
function M.ui_action(_args)
    count = count + 100
    return '{"ok":true,"result":{"count":' .. tostring(count) .. '}}'
end
function M.shutdown_hooks(_args)
    return '{"ok":true,"result":{}}'
end
function M.on_disable(_args)
    return '{"ok":false,"error":"enable-rollback-marker"}'
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
default_enabled: true
display:
  name: Hook Plugin
  capability: Persistent hook fixture
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  hook_shutdown_function: shutdown_hooks
  max_active_sessions: 1
  lifecycle:
    disable: on_disable
  actions:
    - id: ui-action
      function: ui_action
      timeout_ms: 1000
      callers:
        - plugin_ui
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
            package.join("runtime/plugin.bundle.lua"),
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
default_enabled: true
display:
  name: Idle Plugin
  capability: Idle cleanup fixture
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
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

    /// Writes one minimal user-installed package for installation transaction tests.
    /// 为安装事务测试写入一个最小用户安装插件包。
    ///
    /// # Parameters
    /// 参数
    /// `package` is the exact local source directory and `plugin_id` is its stable identity.
    /// `package` 是精确本地来源目录，`plugin_id` 是其稳定身份。
    fn write_user_plugin_package(package: &Path, plugin_id: &str) {
        fs::create_dir_all(package.join("runtime")).expect("create user plugin runtime directory");
        fs::write(package.join("dependencies.yaml"), "{}\n")
            .expect("write user plugin dependencies");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
            r#"local M = {}
function M.dispatch()
    return '{"ok":true,"result":{}}'
end
function M.shutdown()
    return '{"ok":true,"result":{}}'
end
return M
"#,
        )
        .expect("write user plugin bundle");
        fs::write(
            package.join("plugin.yaml"),
            format!(
                "schema_version: 1\nid: {plugin_id}\nversion: 1.0.0\ndefault_enabled: false\ndisplay:\n  name: Local Plugin\n  capability: Local installation fixture\nexecution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\nlua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\n"
            ),
        )
        .expect("write user plugin manifest");
    }

    /// Writes one runtime-free bundled plugin with a skill and an agent attached to that skill.
    /// 写入一个不含运行时、包含 Skill 及绑定该 Skill 的 Agent 的内置插件。
    ///
    /// # Parameters
    /// 参数
    /// `package` is the exact plugin package root created by the test.
    /// `package` 是测试创建的精确插件包根目录。
    fn write_content_plugin_package(package: &Path) {
        fs::create_dir_all(package.join("skills/review-helper"))
            .expect("create content skill directory");
        fs::create_dir_all(package.join("agents")).expect("create content agent directory");
        fs::write(
            package.join("skills/review-helper/SKILL.md"),
            "---\nname: review-helper\ndescription: Review one code change.\n---\n\n# Review helper\nInspect the exact diff.",
        )
        .expect("write content skill");
        fs::write(
            package.join("agents/reviewer.md"),
            "---\nname: Content Reviewer\nskills:\n  - review-helper\n---\nReview the requested change.",
        )
        .expect("write content agent");
        fs::write(
            package.join("plugin.yaml"),
            "schema_version: 1\nid: content-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Content Plugin\n  capability: Runtime-free review guidance\n",
        )
        .expect("write content plugin manifest");
    }

    /// Builds one minimal real LuaSkills engine for System lease integration tests.
    /// 为系统租约集成测试构建一个最小真实 LuaSkills 引擎。
    fn test_engine(runtime_root: &Path) -> LuaEngine {
        // native_runtime_root keeps the integration-test Lua package paths free of Windows verbatim prefixes.
        // native_runtime_root 使集成测试的 Lua package 路径不包含 Windows verbatim 前缀。
        let native_runtime_root = vulcan_runtime_assets::native_host_path(
            &runtime_root
                .canonicalize()
                .expect("canonicalize LuaSkills runtime root"),
        );
        let host_options = LuaRuntimeHostOptions::with_runtime_root(native_runtime_root);
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

    /// Builds one plugin manager with a real fixed bridge package and isolated catalog roots.
    /// 使用真实固定桥接包与隔离目录根构建一个插件管理器。
    ///
    /// # Parameters
    /// 参数
    /// `runtime_root` owns LuaSkills assets and `vulcan_plugin_root` owns real plugin packages.
    /// `runtime_root` 持有 LuaSkills 资产，`vulcan_plugin_root` 持有真实插件包。
    ///
    /// # Returns
    /// 返回值
    /// Returns a manager ready for catalog refresh and lease integration tests.
    /// 返回可用于目录刷新与租约集成测试的管理器。
    fn test_manager(
        runtime_root: &Path,
        vulcan_plugin_root: PathBuf,
    ) -> VulcanPluginRuntimeManager {
        let bridge_root = runtime_root
            .join("system_lua_lib")
            .join("vulcan-plugin-bridge");
        fs::create_dir_all(bridge_root.join("runtime")).expect("create bridge runtime directory");
        fs::write(bridge_root.join("dependencies.yaml"), "{}\n")
            .expect("write bridge dependencies");
        fs::write(
            bridge_root.join("runtime/bridge.lua"),
            include_str!("../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
        )
        .expect("write bridge module");
        let user_plugin_root = runtime_root.join("user_plugins");
        let managed_runtime_distribution_root = runtime_root.join("managed_runtime");
        fs::create_dir_all(&user_plugin_root).expect("create user plugin root");
        fs::create_dir_all(&managed_runtime_distribution_root)
            .expect("create managed runtime distribution root");
        VulcanPluginRuntimeManager::new(
            Arc::new(Mutex::new(test_engine(runtime_root))),
            vulcan_plugin_root,
            user_plugin_root,
            bridge_root,
            managed_runtime_distribution_root,
            runtime_root.join("plugin_state"),
            runtime_root.join("plugin_runtime"),
        )
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

    /// Verifies a runtime-free plugin remains fully usable when no Lua engine exists.
    /// 验证不存在 Lua 引擎时，无运行时插件仍保持完整可用。
    #[test]
    fn content_plugin_operates_without_lua_runtime_or_execution_environment() {
        // temp owns every isolated catalog and runtime directory used by this test.
        // temp 持有本测试使用的全部隔离目录与运行时目录。
        let temp = tempfile::tempdir().expect("create content plugin runtime directory");
        // vulcan_plugin_root contains the only bundled content plugin package.
        // vulcan_plugin_root 包含唯一的内置内容插件包。
        let vulcan_plugin_root = temp.path().join("vulcan_plugins");
        // user_plugin_root is present but intentionally empty.
        // user_plugin_root 存在但刻意保持为空。
        let user_plugin_root = temp.path().join("user_plugins");
        // plugin_runtime_root would receive execution environments if one were incorrectly created.
        // plugin_runtime_root 会在错误创建执行环境时接收相关文件。
        let plugin_runtime_root = temp.path().join("plugin_runtime");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan plugin root");
        fs::create_dir_all(&user_plugin_root).expect("create user plugin root");
        write_content_plugin_package(&vulcan_plugin_root.join("content-plugin"));
        write_hook_package(&vulcan_plugin_root);
        // manager deliberately has no Lua engine or bridge runtime.
        // manager 刻意不包含 Lua 引擎或桥接运行时。
        let manager = VulcanPluginRuntimeManager::without_runtime(
            vulcan_plugin_root,
            user_plugin_root,
            temp.path().join("bridge"),
            temp.path().join("managed_runtime"),
            temp.path().join("plugin_state"),
            plugin_runtime_root.clone(),
        );
        // catalog proves content components publish even when the execution adapter is absent.
        // catalog 证明执行适配器缺失时内容组件仍会发布。
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load content plugin without Lua");
        let plugin = &catalog.plugins["content-plugin"];
        assert_eq!(plugin.availability, VulcanPluginAvailability::Enabled);
        assert!(plugin.runtime_execution_available);
        assert!(
            catalog
                .enabled_skills
                .contains_key("content-plugin:review-helper")
        );
        assert!(
            catalog
                .enabled_agents
                .contains_key("content-plugin:reviewer")
        );
        assert_eq!(plugin.active_session_count, 0);
        let runtime_plugin = &catalog.plugins["hook-plugin"];
        assert!(!runtime_plugin.runtime_execution_available);
        assert!(
            runtime_plugin
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("Lua execution runtime is unavailable"))
        );
        assert!(catalog.enabled_hooks.is_empty());
        assert_eq!(
            manager
                .prepare_plugin_environment("content-plugin")
                .expect("content plugin preparation should be a no-op"),
            plugin.package_root
        );
        // runtime executes asynchronous no-op lifecycle and probe paths deterministically.
        // runtime 确定性执行异步空生命周期与探测路径。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("create content plugin Tokio runtime");
        runtime
            .block_on(
                manager.invoke_lifecycle("content-plugin", VulcanPluginLifecycleEvent::Enable),
            )
            .expect("content lifecycle should not require Lua");
        runtime
            .block_on(manager.probe_plugin("content-plugin"))
            .expect("content probe should not require Lua");
        assert!(
            runtime
                .block_on(manager.prewarm_enabled_plugin_runtimes())
                .expect("content prewarm should not require Lua")
                .is_empty()
        );
        assert!(!plugin_runtime_root.exists());
        assert_eq!(
            manager.snapshot().plugins["content-plugin"].active_session_count,
            0
        );
    }

    /// Verifies enable rollback invokes the disable callback while runtime state remains enabling.
    /// 验证启用回滚会在运行时仍处于启用中状态时调用停用回调。
    #[test]
    fn enable_rollback_uses_declared_disable_callback_in_enabling_scope() {
        // temp owns the isolated bundled plugin and bridge fixture.
        // temp 持有隔离的内建插件与桥接测试环境。
        let temp = tempfile::tempdir().expect("create lifecycle rollback directory");
        // runtimeRoot is the deterministic manager root for this lifecycle test.
        // runtimeRoot 是当前生命周期测试使用的确定性管理器根目录。
        let runtime_root = temp.path().join("runtime");
        // vulcanPluginRoot contains the lifecycle-enabled hook fixture.
        // vulcanPluginRoot 包含支持生命周期的 Hook 测试插件。
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_hook_package(&vulcan_plugin_root);
        // manager owns the exact catalog transition tested below.
        // manager 持有下方测试的精确目录转换。
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        // disabled makes the fixture eligible to enter a host-controlled enabling transition.
        // disabled 使测试插件能够进入宿主控制的启用中转换。
        let disabled = BTreeSet::from([String::from("hook-plugin")]);
        manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("load disabled lifecycle fixture");
        manager
            .begin_enabling("hook-plugin")
            .expect("begin lifecycle enabling transition");
        // runtime executes the asynchronous compensation lease deterministically.
        // runtime 确定性执行异步补偿租约。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("create lifecycle Tokio runtime");
        // error proves the manifest disable callback ran through the rollback scope.
        // error 证明清单停用回调通过回滚作用域得到执行。
        let error = runtime
            .block_on(
                manager.invoke_lifecycle("hook-plugin", VulcanPluginLifecycleEvent::EnableRollback),
            )
            .expect_err("surface rollback marker from disable callback");
        assert!(error.to_string().contains("enable-rollback-marker"));
    }

    /// Verifies disable compensation can re-enter the only state authorized for enable probing.
    /// 验证停用补偿能够重新进入唯一允许启用探测的状态。
    #[test]
    fn draining_rollback_reenters_enabling_before_probe() {
        // temp owns the isolated plugin package and fixed bridge runtime.
        // temp 持有隔离的插件包与固定桥接运行时。
        let temp = tempfile::tempdir().expect("create draining rollback directory");
        // runtime_root is the deterministic root for every test-owned runtime asset.
        // runtime_root 是所有测试自有运行时资产的确定性根目录。
        let runtime_root = temp.path().join("runtime");
        // vulcan_plugin_root contains the enabled hook fixture selected by the transition.
        // vulcan_plugin_root 包含转换选择的已启用 Hook 测试插件。
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_hook_package(&vulcan_plugin_root);
        // manager owns the exact Enabled -> Draining -> Enabling sequence under test.
        // manager 持有本测试的精确 Enabled -> Draining -> Enabling 序列。
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load enabled lifecycle fixture");
        manager
            .begin_draining("hook-plugin")
            .expect("begin draining transition");
        manager
            .rollback_draining_to_enabling("hook-plugin")
            .expect("re-enter enabling transition");
        assert_eq!(
            manager.snapshot().plugins["hook-plugin"].runtime_state,
            VulcanPluginRuntimeState::Enabling
        );
        // runtime proves the enabling probe lease is accepted after the reverse transition.
        // runtime 证明反向转换后允许创建启用探测租约。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("create lifecycle Tokio runtime");
        runtime
            .block_on(manager.probe_plugin("hook-plugin"))
            .expect("probe plugin while re-entering enabling");
    }

    /// Verifies hook calls reuse persistent Lua state and draining rejects stale generations.
    /// 验证 hook 调用会复用持久 Lua 状态，且 draining 会拒绝陈旧代次。
    #[test]
    fn hook_runtime_reuses_state_and_drains_by_plugin_generation() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_hook_package(&vulcan_plugin_root);
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
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
        let ui_action = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("hook-plugin"),
                action_id: String::from("ui-action"),
                arguments: json!({}),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: runtime_root.clone(),
                reuse_enabled_runtime: true,
            }))
            .expect("invoke persistent UI action");
        let second = runtime
            .block_on(manager.invoke_hook(&hook, hook_invocation(&hook.registration_id)))
            .expect("invoke second hook");
        assert_eq!(first.payload.expect("first payload").value["count"], 1);
        assert_eq!(ui_action["count"], 100);
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
        assert_eq!(report.closed_ui_action_runtimes, 1);
        assert!(report.failures.is_empty());
    }

    /// Verifies the repository usage-statistics plugin manifest, Lua bundle, and configuration action.
    /// 验证仓库用量统计插件的清单、Lua Bundle 与配置 Action。
    #[test]
    fn bundled_usage_statistics_plugin_loads_through_fixed_bridge() {
        // temp owns the isolated LuaSkills bridge and plugin runtime state.
        // temp 持有隔离的 LuaSkills 桥接与插件运行时状态。
        let temp = tempfile::tempdir().expect("create usage statistics plugin test directory");
        // runtimeRoot is the deterministic manager root for this integration test.
        // runtimeRoot 是当前集成测试使用的确定性管理器根目录。
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("create usage statistics test runtime root");
        // repositoryRoot is derived from the current crate instead of a process working directory.
        // repositoryRoot 从当前 Crate 派生，不依赖进程工作目录。
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("resolve repository root");
        // manager loads the real repository package through the production catalog path.
        // manager 通过生产目录路径加载真实仓库插件包。
        let manager = test_manager(&runtime_root, repository_root.join("plugins"));
        // storage records the exact SQL and parameter protocol emitted by the real Lua bundle.
        // storage 记录真实 Lua Bundle 发出的精确 SQL 与参数协议。
        let storage = Arc::new(RecordingStorageService::default());
        // uiEvents records the revision-only notification emitted after the terminal fact.
        // uiEvents 记录终态事实写入后发出的仅含修订号通知。
        let ui_events = Arc::new(RecordingUiEventPublisher::default());
        manager
            .install_host_services(VulcanPluginHostServices {
                storage: Some(storage.clone()),
                ui_events: Some(ui_events.clone()),
            })
            .expect("install recording usage statistics host services");
        // catalog proves the package passed manifest, schema, asset, and locale validation.
        // catalog 证明插件包已通过清单、Schema、资产与语言包校验。
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load repository plugin catalog");
        assert!(
            catalog.plugins.contains_key("vulcan-usage-statistics"),
            "catalog diagnostics: {:?}",
            catalog.diagnostics
        );
        // runtime executes the real uncompressed Lua bundle deterministically.
        // runtime 确定性执行真实未压缩 Lua Bundle。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create usage statistics Tokio runtime");
        runtime
            .block_on(manager.prewarm_plugin_runtime("vulcan-usage-statistics"))
            .expect("prewarm usage statistics plugin runtime");
        // settings is the exact host-validated default configuration.
        // settings 是经宿主精确校验的默认配置。
        let settings = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("settings-get"),
                arguments: json!({}),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("invoke usage statistics settings action");
        assert_eq!(settings["enabled"], true);
        assert_eq!(settings["retentionDays"], 180);
        assert_eq!(settings["defaultRangeDays"], 30);
        assert_eq!(settings["defaultMetric"], "total_tokens");
        // permanentSettings validates the zero-day permanent-retention sentinel through the production config boundary.
        // permanentSettings 通过生产配置边界验证零天永久保留哨兵。
        let permanent_settings = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("settings-update"),
                arguments: json!({
                    "enabled": true,
                    "retentionDays": 0,
                    "defaultRangeDays": 30,
                    "defaultMetric": "total_tokens"
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("persist permanent usage statistics retention");
        assert_eq!(permanent_settings["retentionDays"], 0);
        // completedHook is the real repository Hook descriptor, including its payload schema.
        // completedHook 是包含载荷 Schema 的真实仓库 Hook 描述符。
        let completed_hook = catalog
            .enabled_hooks
            .get("vulcan-plugin/vulcan-usage-statistics/provider-request-completed")
            .cloned()
            .expect("resolve usage statistics completed Hook");
        // completedPayload contains every host-owned field required by schema version one.
        // completedPayload 包含 Schema 版本一要求的全部宿主持有字段。
        let mut completed_payload = json!({
            "schema_version": 1,
            "request_id": "request-1",
            "execution_id": "execution-1",
            "session_id": "session-1",
            "project_id": "project-1",
            "workspace_id": repository_root.to_string_lossy(),
            "owner_kind": "main_agent",
            "owner_id": "main-agent",
            "owner_name": "Main Agent",
            "provider_key": "provider-1",
            "provider_name": "Provider One",
            "service_entry_id": "entry-1",
            "connection_id": "connection-1",
            "protocol_profile_id": "openai-chat",
            "secondary_route_id": "route-1",
            "secondary_provider_name": "Secondary Provider",
            "secondary_endpoint_name": "Secondary Endpoint",
            "model_key": "model-1",
            "model_name": "Model One",
            "attempt_index": 0,
            "retry_count": 0,
            "status": "succeeded",
            "started_at_ms": 1_000,
            "first_event_at_ms": 1_100,
            "first_token_at_ms": 1_120,
            "completed_at_ms": 1_500,
            "time_to_first_event_ms": 100,
            "time_to_first_token_ms": 120,
            "duration_ms": 500,
            "input_tokens": 10,
            "uncached_input_tokens": 10,
            "output_tokens": 20,
            "reasoning_tokens": 5,
            "total_tokens": 35,
            "cached_input_tokens": 14,
            "cache_creation_input_tokens": 2,
            "finish_reason": "stop",
            "error_category": null,
            "error_code": null,
            "calendar_date": "2026-07-14",
            "timezone_offset_minutes": 480
        });
        let completed_object = completed_payload
            .as_object_mut()
            .expect("completed Provider telemetry payload must be an object");
        completed_object.insert(
            String::from("operation"),
            Value::String(String::from("embedding_create")),
        );
        completed_object.insert(
            String::from("capability_revision"),
            Value::String(String::from("capability-1")),
        );
        completed_object.insert(
            String::from("catalog_revision"),
            Value::String(String::from("catalog-1")),
        );
        completed_object.insert(String::from("input_item_count"), Value::from(2));
        completed_object.insert(String::from("output_item_count"), Value::from(2));
        completed_object.insert(String::from("vector_dimensions"), Value::from(1024));
        completed_object.insert(String::from("cost"), Value::String(String::from("0.0004")));
        // completedInvocation uses the exact runtime context expected by a session-scoped effect Hook.
        // completedInvocation 使用会话作用域 Effect Hook 期望的精确运行时上下文。
        let completed_invocation = HookInvocation {
            registration: HookRegistration::new(
                completed_hook.registration_id.clone(),
                "provider.request.completed",
                HookMode::Effect,
            ),
            context: HookContext {
                run_id: String::from("execution-1"),
                point: String::from("provider.request.completed"),
                scope: HookScope::Session,
                actor: HookActor::new("runtime-test"),
                workspace_id: Some(repository_root.to_string_lossy().into_owned()),
                project_id: Some(String::from("project-1")),
                session_id: Some(String::from("session-1")),
                conversation_id: Some(String::from("session-1")),
                user_id: None,
                capabilities: HookCapabilitySet::default(),
                metadata: BTreeMap::new(),
            },
            payload: HookPayload {
                value: completed_payload,
            },
        };
        runtime
            .block_on(manager.invoke_hook(&completed_hook, completed_invocation.clone()))
            .expect("persist one real usage statistics terminal Hook");
        // recent validates that an omitted optional owner does not mutate shared Lua parameters.
        // recent 验证省略可选归属时不会修改共享 Lua 参数。
        let recent = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("recent-get"),
                arguments: json!({ "limit": 20 }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("query recent usage statistics requests");
        assert!(recent["requests"].is_array());
        assert_eq!(recent["requests"][0]["requestId"], "request-1");
        assert!(recent["requests"][0]["errorCategory"].is_null());
        assert!(recent["requests"][0]["errorCode"].is_null());
        // dashboard validates the real aggregate SQL path and strict action output schema.
        // dashboard 验证真实聚合 SQL 路径与严格 Action 输出 Schema。
        let dashboard = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("dashboard-get"),
                arguments: json!({
                    "rangeDays": 30,
                    "metric": "input_tokens"
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("query usage statistics dashboard");
        assert_eq!(dashboard["summary"]["totalTokens"], 35);
        // cacheHitRate proves cached input may exceed uncached input without violating the percentage schema.
        // cacheHitRate 证明缓存输入可以高于未缓存输入，同时仍满足百分比 Schema。
        let cache_hit_rate = dashboard["summary"]["cacheHitRate"]
            .as_f64()
            .expect("read usage statistics cache hit rate");
        assert!((cache_hit_rate - (14.0 * 100.0 / 24.0)).abs() < 0.000_001);
        assert_eq!(dashboard["heatmap"][0]["topModelName"], "Model One");
        assert_eq!(dashboard["series"][0]["value"], 10);
        assert_eq!(dashboard["availableModels"][0]["modelRef"], 1);
        // groupedErrors validates the same omitted-owner contract after prior action reuse.
        // groupedErrors 验证此前 Action 复用后的同一省略归属契约。
        let grouped_errors = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("errors-get"),
                arguments: json!({
                    "rangeDays": 30,
                    "metric": "input_tokens"
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("query grouped usage statistics errors");
        assert!(grouped_errors["errors"].is_array());
        assert_eq!(
            grouped_errors["errors"][0]["errorCategory"],
            "provider_http"
        );
        assert_eq!(grouped_errors["errors"][0]["errorCode"], "429");
        // filteredDashboard proves range, owner, and model filters share one ordered aggregate projection.
        // filteredDashboard 证明范围、归属与模型筛选共享一个有序聚合投影。
        let filtered_dashboard = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("dashboard-get"),
                arguments: json!({
                    "rangeDays": 30,
                    "metric": "total_tokens",
                    "ownerKind": "main_agent",
                    "modelRef": 1
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("query model-filtered usage statistics dashboard");
        assert_eq!(filtered_dashboard["summary"]["totalTokens"], 35);
        // filteredRecent proves raw request facts use the same exact owner and model identity.
        // filteredRecent 证明原始请求事实使用相同的精确归属与模型身份。
        let filtered_recent = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("recent-get"),
                arguments: json!({
                    "limit": 20,
                    "ownerKind": "main_agent",
                    "modelRef": 1
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("query model-filtered recent usage statistics requests");
        assert_eq!(filtered_recent["requests"][0]["requestId"], "request-1");
        // filteredErrors proves grouped failure facts honor the identical model boundary.
        // filteredErrors 证明分组失败事实遵守完全相同的模型边界。
        let filtered_errors = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("errors-get"),
                arguments: json!({
                    "rangeDays": 30,
                    "metric": "total_tokens",
                    "ownerKind": "main_agent",
                    "modelRef": 1
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("query model-filtered grouped usage statistics errors");
        assert_eq!(filtered_errors["errors"][0]["errorCode"], "429");
        // allTime validates the zero-day sentinel without synthetic date parameters.
        // allTime 验证零天累计哨兵，且不生成虚假的日期参数。
        let all_time = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("dashboard-get"),
                arguments: json!({
                    "rangeDays": 0,
                    "metric": "total_tokens"
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("query all-time usage statistics dashboard");
        assert_eq!(all_time["rangeDays"], 0);
        // cleanupPreview and cleanupApply validate the explicit inclusive date scope and revision fence.
        // cleanupPreview 与 cleanupApply 验证明确的本地日期闭区间和修订号围栏。
        let cleanup_preview = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("statistics-clear-preview"),
                arguments: json!({
                    "fromDate": "2026-07-14",
                    "toDate": "2026-07-14"
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("preview usage statistics cleanup");
        assert_eq!(cleanup_preview["summary"]["requests"], 1);
        let cleanup_apply = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("statistics-clear-apply"),
                arguments: json!({
                    "fromDate": "2026-07-14",
                    "toDate": "2026-07-14",
                    "expectedRevision": cleanup_preview["revision"]
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("apply usage statistics cleanup");
        assert!(cleanup_apply["deletedRequests"].is_number());
        let calls = storage.calls.lock().expect("read recording storage calls");
        // cleanupScript uses validated SQL literals because the host rejects parameters on multi-statement scripts.
        // cleanupScript 使用已校验的 SQL 字面量，因为宿主拒绝为多语句脚本绑定参数。
        let cleanup_script = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| {
                        sql.contains("CREATE TEMP TABLE IF NOT EXISTS statistics_clear_state")
                    })
            })
            .expect("record statistics cleanup transaction script");
        let cleanup_sql = cleanup_script.payload["sql"]
            .as_str()
            .expect("statistics cleanup SQL text");
        // assertModelCleanupSql verifies orphan model deletion follows aggregate rebuilding in both cleanup paths.
        // assertModelCleanupSql 验证孤立模型删除位于聚合重建之后，并覆盖两条清理路径。
        let assert_model_cleanup_sql = |sql: &str| {
            let aggregate_position = sql
                .find("DELETE FROM usage_daily_model")
                .expect("cleanup must rebuild daily aggregates");
            let model_position = sql
                .find("DELETE FROM models")
                .expect("cleanup must delete orphan models");
            let revision_position = sql
                .find("UPDATE statistics_meta")
                .expect("cleanup must update statistics revision");
            assert!(aggregate_position < model_position);
            assert!(model_position < revision_position);
            assert_eq!(sql.matches("WHERE NOT EXISTS").count(), 1);
            assert_eq!(sql.matches("AND NOT EXISTS").count(), 1);
            assert!(sql.contains("api_request_events.model_ref = models.model_ref"));
            assert!(sql.contains("usage_daily_model.model_ref = models.model_ref"));
        };
        assert_model_cleanup_sql(cleanup_sql);
        assert!(cleanup_sql.contains("calendar_date >= '2026-07-14'"));
        assert!(cleanup_sql.contains("calendar_date <= '2026-07-14'"));
        assert!(!cleanup_sql.contains('?'));
        assert_eq!(
            cleanup_script.payload["params"]
                .as_array()
                .expect("statistics cleanup parameters")
                .len(),
            0
        );
        // calls proves multi-value statements use numbered placeholders accepted by Controller.
        // calls 证明多值语句使用 Controller 接受的编号占位符。
        assert!(
            !calls.iter().any(|request| request
                .payload
                .get("sql")
                .and_then(Value::as_str)
                .is_some_and(|sql| {
                    sql.contains("DELETE FROM api_request_events WHERE calendar_date < date")
                })),
            "permanent retention must not delete raw request facts"
        );
        let model_insert = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| sql.contains("INSERT INTO models"))
            })
            .expect("record model dimension insert");
        assert!(
            model_insert.payload["sql"]
                .as_str()
                .expect("model insert SQL text")
                .contains("?6")
        );
        assert_eq!(
            model_insert.payload["params"]
                .as_array()
                .expect("model insert parameters")
                .len(),
            6
        );
        let fact_insert = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| sql.contains("INSERT OR IGNORE INTO api_request_events"))
            })
            .expect("record request fact insert");
        assert!(
            fact_insert.payload["sql"]
                .as_str()
                .expect("request fact insert SQL text")
                .contains("?46")
        );
        assert_eq!(
            fact_insert.payload["params"]
                .as_array()
                .expect("request fact insert parameters")
                .len(),
            46
        );
        // summaryQuery must retain only the date-range parameter after recent-get appends its limit.
        // summaryQuery 在 recent-get 追加 limit 后仍必须只保留日期范围参数。
        let summary_query = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| sql.contains("COALESCE(SUM(request_count)"))
            })
            .expect("record dashboard summary query");
        assert_eq!(
            summary_query.payload["params"]
                .as_array()
                .expect("finite dashboard summary parameters")
                .len(),
            1
        );
        // allTimeSummary is the final summary query and has no range or owner parameters.
        // allTimeSummary 是最后一条汇总查询，不包含范围或归属参数。
        let all_time_summary = calls
            .iter()
            .rev()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| sql.contains("COALESCE(SUM(request_count)"))
            })
            .expect("record all-time dashboard summary query");
        assert_eq!(
            all_time_summary.payload["params"]
                .as_array()
                .expect("all-time dashboard summary parameters")
                .len(),
            0
        );
        // errorsQuery also retains only its date-range parameter with no owner filter.
        // errorsQuery 在无归属筛选时同样只保留日期范围参数。
        let errors_query = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| sql.contains("WHERE e.status = 'failed'"))
            })
            .expect("record grouped errors query");
        assert_eq!(
            errors_query.payload["params"]
                .as_array()
                .expect("grouped error query parameters")
                .len(),
            1
        );
        // filteredSummaryQuery binds range, owner, and model in one exact numbered sequence.
        // filteredSummaryQuery 以一个精确编号序列绑定范围、归属与模型。
        let filtered_summary_query = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| {
                        sql.contains("COALESCE(SUM(request_count)")
                            && sql.contains("model_ref = ?3")
                    })
            })
            .expect("record model-filtered dashboard summary query");
        assert_eq!(
            filtered_summary_query.payload["params"]
                .as_array()
                .expect("model-filtered dashboard summary parameters")
                .len(),
            3
        );
        // filteredRecentQuery binds owner, model, and row limit without reusing mutable parameter arrays.
        // filteredRecentQuery 绑定归属、模型与行数限制，且不复用可变参数数组。
        let filtered_recent_query = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| {
                        sql.contains("SELECT e.request_id AS requestId")
                            && sql.contains("e.model_ref = ?2")
                            && sql.contains("LIMIT ?3")
                    })
            })
            .expect("record model-filtered recent request query");
        assert_eq!(
            filtered_recent_query.payload["params"]
                .as_array()
                .expect("model-filtered recent request parameters")
                .len(),
            3
        );
        // filteredErrorsQuery binds range, owner, and model before grouping redacted failures.
        // filteredErrorsQuery 在聚合脱敏失败信息前绑定范围、归属与模型。
        let filtered_errors_query = calls
            .iter()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| {
                        sql.contains("WHERE e.status = 'failed'")
                            && sql.contains("e.model_ref = ?3")
                    })
            })
            .expect("record model-filtered grouped errors query");
        assert_eq!(
            filtered_errors_query.payload["params"]
                .as_array()
                .expect("model-filtered grouped error parameters")
                .len(),
            3
        );
        drop(calls);
        assert_eq!(
            ui_events
                .events
                .lock()
                .expect("read recording UI events")
                .as_slice(),
            &[VulcanPluginUiInvalidation {
                plugin_id: String::from("vulcan-usage-statistics"),
                topic: String::from("statistics.changed"),
                revision: 1,
            }]
        );
        // automaticRetention proves the periodic cleanup script also obeys the host parameter contract.
        // automaticRetention 证明周期性清理脚本同样遵守宿主参数契约。
        runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-usage-statistics"),
                action_id: String::from("settings-update"),
                arguments: json!({
                    "enabled": true,
                    "retentionDays": 30,
                    "defaultRangeDays": 30,
                    "defaultMetric": "total_tokens"
                }),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: repository_root.to_path_buf(),
                reuse_enabled_runtime: true,
            }))
            .expect("enable finite usage statistics retention");
        let mut retention_invocation = completed_invocation.clone();
        retention_invocation.context.run_id = String::from("execution-retention");
        let retention_payload = retention_invocation
            .payload
            .value
            .as_object_mut()
            .expect("retention Provider telemetry payload must be an object");
        retention_payload.insert(
            String::from("request_id"),
            Value::String(String::from("request-retention")),
        );
        retention_payload.insert(
            String::from("execution_id"),
            Value::String(String::from("execution-retention")),
        );
        retention_payload.insert(String::from("completed_at_ms"), Value::from(2_500));
        retention_payload.insert(String::from("started_at_ms"), Value::from(2_000));
        runtime
            .block_on(manager.invoke_hook(&completed_hook, retention_invocation))
            .expect("persist automatic retention usage statistics terminal Hook");
        let retention_calls = storage.calls.lock().expect("read retention storage calls");
        let retention_cleanup_script = retention_calls
            .iter()
            .rev()
            .find(|request| {
                request
                    .payload
                    .get("sql")
                    .and_then(Value::as_str)
                    .is_some_and(|sql| {
                        sql.contains("DELETE FROM api_request_events WHERE calendar_date < date('2026-07-14', '-30 days')")
                    })
            })
            .expect("record automatic retention cleanup transaction script");
        let retention_sql = retention_cleanup_script.payload["sql"]
            .as_str()
            .expect("automatic retention cleanup SQL text");
        assert_model_cleanup_sql(retention_sql);
        assert!(!retention_sql.contains('?'));
        assert_eq!(
            retention_cleanup_script.payload["params"]
                .as_array()
                .expect("automatic retention cleanup parameters")
                .len(),
            0
        );
        drop(retention_calls);
        // disableReport proves the real plugin can drain its persistent lease before lifecycle cleanup.
        // disableReport 证明真实插件可在生命周期清理前排空其持久租约。
        manager
            .begin_draining("vulcan-usage-statistics")
            .expect("begin usage statistics plugin draining");
        let disable_report = runtime
            .block_on(manager.drain_plugin_sessions("vulcan-usage-statistics"))
            .expect("drain usage statistics plugin sessions");
        assert!(disable_report.failures.is_empty());
        runtime
            .block_on(manager.invoke_lifecycle(
                "vulcan-usage-statistics",
                VulcanPluginLifecycleEvent::Disable,
            ))
            .expect("invoke usage statistics disable lifecycle");
        // flushRequest proves an argument-free host call remains an object across the Lua/JSON boundary.
        // flushRequest 证明无参数宿主调用跨越 Lua/JSON 边界后仍保持对象语义。
        let flush_request = storage
            .calls
            .lock()
            .expect("read disable lifecycle storage calls")
            .last()
            .cloned()
            .expect("record usage statistics disable flush");
        assert_eq!(
            flush_request.operation,
            VulcanPluginStorageOperation::SqliteFlush
        );
        assert_eq!(flush_request.payload, json!({}));
    }

    /// Verifies the repository Context7 bundle loads through the LuaSkills typed JSON surface.
    /// 验证仓库 Context7 Bundle 通过 LuaSkills 强类型 JSON 表面完成加载。
    #[test]
    fn bundled_context7_plugin_loads_with_typed_json_surface() {
        // temp owns the isolated fixed bridge lease used by this load-only check.
        // temp 持有当前仅加载检查使用的隔离固定桥租约。
        let temp = tempfile::tempdir().expect("create Context7 plugin test directory");
        // runtimeRoot is the deterministic Vulcan Plugin runtime directory.
        // runtimeRoot 是确定性的 Vulcan Plugin 运行时目录。
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("create Context7 test runtime root");
        // repositoryRoot identifies the real checked-in Context7 package.
        // repositoryRoot 标识仓库内真实的 Context7 插件包。
        let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(Path::parent)
            .expect("resolve repository root");
        // manager loads the production catalog and fixed bridge sources.
        // manager 加载生产插件目录与固定桥源码。
        let manager = test_manager(&runtime_root, repository_root.join("plugins"));
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load repository plugin catalog");
        // runtime executes the real Context7 module initialization under LuaSkills 0.5.5.
        // runtime 在 LuaSkills 0.5.5 下执行真实 Context7 模块初始化。
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Context7 Tokio runtime");
        runtime
            .block_on(manager.prewarm_plugin_runtime("context7"))
            .expect("prewarm Context7 plugin runtime");
    }

    /// Verifies the repository RTK plugin executes its actions, shell transform, and Git context transform through the fixed bridge.
    /// 验证仓库 RTK 插件通过固定桥接层执行 Action、Shell 转换与 Git 上下文转换。
    #[cfg(target_os = "windows")]
    #[test]
    fn bundled_rtk_plugin_uses_generic_action_and_hook_boundaries() {
        let temp = tempfile::tempdir().expect("create RTK plugin test directory");
        let runtime_root = temp.path().join("runtime");
        fs::create_dir_all(&runtime_root).expect("create RTK test runtime root");
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let repository_root = crate_root
            .parent()
            .and_then(Path::parent)
            .expect("resolve repository root");
        let workspace_root = repository_root.to_path_buf();
        let manager = test_manager(&runtime_root, repository_root.join("plugins"));
        let catalog = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load repository plugin catalog");
        let hook = catalog
            .enabled_hooks
            .get("vulcan-plugin/vulcan-rtk/shell-command-rewrite")
            .cloned()
            .expect("resolve RTK shell hook");
        // git_hook is the production Git commit-context registration whose contribution protocol differs from the shell hook.
        // git_hook 是生产 Git 提交上下文注册，其贡献协议不同于 Shell Hook。
        let git_hook = catalog
            .enabled_hooks
            .get("vulcan-plugin/vulcan-rtk/git-commit-context")
            .cloned()
            .expect("resolve RTK Git context hook");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio RTK test runtime");
        runtime
            .block_on(manager.prewarm_plugin_runtime("vulcan-rtk"))
            .expect("prewarm RTK plugin runtime");
        let self_check = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-rtk"),
                action_id: String::from("self_check"),
                arguments: json!({}),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: workspace_root.clone(),
                reuse_enabled_runtime: true,
            }))
            .expect("invoke RTK self-check action");
        assert_eq!(self_check["available"], true);
        assert!(
            self_check["version"]
                .as_str()
                .is_some_and(|value| value.starts_with("rtk "))
        );
        let statistics = runtime
            .block_on(manager.invoke_action(VulcanPluginActionRequest {
                plugin_id: String::from("vulcan-rtk"),
                action_id: String::from("statistics-get"),
                arguments: json!({}),
                caller: crate::VulcanPluginActionCaller::PluginUi,
                workspace_root: workspace_root.clone(),
                reuse_enabled_runtime: true,
            }))
            .expect("invoke RTK statistics action");
        assert_eq!(statistics["available"], true);
        assert!(statistics["byCommand"].is_array());

        let invocation = HookInvocation {
            registration: HookRegistration::new(
                hook.registration_id.clone(),
                "tool.shell_command.before_execute",
                HookMode::Transform,
            ),
            context: HookContext {
                run_id: Uuid::new_v4().to_string(),
                point: String::from("tool.shell_command.before_execute"),
                scope: HookScope::Workspace,
                actor: HookActor::new("runtime-test"),
                workspace_id: Some(workspace_root.to_string_lossy().into_owned()),
                project_id: None,
                session_id: Some(String::from("session-rtk")),
                conversation_id: Some(String::from("session-rtk")),
                user_id: None,
                capabilities: HookCapabilitySet::default(),
                metadata: BTreeMap::from([(
                    String::from("workspace_root"),
                    Value::String(workspace_root.to_string_lossy().into_owned()),
                )]),
            },
            payload: HookPayload {
                value: json!({
                    "schema_version": 1,
                    "command": "git status --short",
                    "cwd": workspace_root.to_string_lossy(),
                    "shell": "auto",
                    "environment": {},
                    "binary_path_requests": [],
                    "contributions": [],
                }),
            },
        };
        let hook_result = runtime
            .block_on(manager.invoke_hook(&hook, invocation))
            .expect("invoke RTK shell transform");
        let transformed = hook_result
            .payload
            .clone()
            .expect("RTK hook should return transformed payload")
            .value;
        assert_eq!(
            transformed["command"], "rtk git status --short",
            "RTK hook diagnostics: {:?}",
            hook_result.diagnostics
        );
        assert_eq!(transformed["binary_path_requests"][0]["binary_id"], "rtk");
        assert_eq!(transformed["contributions"][0]["badge"], "RTK");
        assert!(
            transformed["environment"].is_object(),
            "LuaSkills must preserve an empty shell environment as a JSON object"
        );
        // git_invocation uses the exact strict service-kernel payload contract for commit-summary context transforms.
        // git_invocation 使用提交摘要上下文转换的精确严格 Service Kernel 载荷契约。
        let git_invocation = HookInvocation {
            registration: HookRegistration::new(
                git_hook.registration_id.clone(),
                "git.commit_summary.context",
                HookMode::Transform,
            ),
            context: HookContext {
                run_id: Uuid::new_v4().to_string(),
                point: String::from("git.commit_summary.context"),
                scope: HookScope::Workspace,
                actor: HookActor::new("runtime-test"),
                workspace_id: Some(workspace_root.to_string_lossy().into_owned()),
                project_id: None,
                session_id: Some(String::from("session-rtk-git")),
                conversation_id: Some(String::from("session-rtk-git")),
                user_id: None,
                capabilities: HookCapabilitySet::default(),
                metadata: BTreeMap::from([(
                    String::from("workspace_root"),
                    Value::String(workspace_root.to_string_lossy().into_owned()),
                )]),
            },
            payload: HookPayload {
                value: json!({
                    "schema_version": 1,
                    "repository_root": workspace_root.to_string_lossy(),
                    "include_unstaged": false,
                    "sections": [{
                        "id": "staged",
                        "title": "Staged changes",
                        "body": "fixture",
                        "code_fence": false,
                    }],
                    "notices": [],
                    "contributions": [],
                }),
            },
        };
        // git_hook_result is the real fixed-bridge transform result validated before service-kernel decoding.
        // git_hook_result 是在 Service Kernel 解码前取得的真实固定桥转换结果。
        let git_hook_result = runtime
            .block_on(manager.invoke_hook(&git_hook, git_invocation))
            .expect("invoke RTK Git context transform");
        // git_transformed is the exact host-owned payload returned by the RTK plugin.
        // git_transformed 是 RTK 插件返回的精确宿主持有载荷。
        let git_transformed = git_hook_result
            .payload
            .expect("RTK Git hook should return transformed payload")
            .value;
        assert_eq!(
            git_transformed["contributions"][0],
            json!({
                "label": "Git context",
                "value": "compressed by vulcan-rtk",
            })
        );
        // resolvedBinaryDirectory proves the logical hook request resolves to the repository plugin bin.
        // resolvedBinaryDirectory 证明逻辑 Hook 请求会解析到仓库插件 bin 目录。
        let resolved_binary_directory = manager
            .hook_binary_directory(&hook.registration_id, "rtk")
            .expect("resolve RTK hook binary directory");
        // expectedBinaryDirectory is canonicalized independently from the plugin manifest path.
        // expectedBinaryDirectory 从插件清单路径独立执行规范化。
        let expected_binary_directory = repository_root
            .join("plugins/vulcan-rtk/bin/x86_64-pc-windows-msvc")
            .canonicalize()
            .expect("resolve repository RTK plugin bin directory");
        assert_eq!(resolved_binary_directory, expected_binary_directory);
    }

    /// Verifies authorization preview includes every MCP server transport and declared Secret key without values.
    /// 验证授权预览包含每个 MCP 服务器传输与已声明 Secret 键，但不包含值。
    #[test]
    fn user_plugin_permission_preview_includes_sanitized_mcp_ownership() {
        // temp owns the source package and isolated runtime state for preview construction.
        // temp 持有用于构建预览的来源包与隔离运行时状态。
        let temp = tempfile::tempdir().expect("create MCP permission preview directory");
        // runtime_root provides deterministic manager state without installed user packages.
        // runtime_root 提供不含已安装用户插件的确定性管理器状态。
        let runtime_root = temp.path().join("runtime");
        // vulcan_plugin_root is the empty bundled source required by the test manager.
        // vulcan_plugin_root 是测试管理器要求的空内置来源。
        let vulcan_plugin_root = runtime_root.join("plugins");
        // source_package contains one remote MCP declaration backed by a namespaced Secret.
        // source_package 包含一个由命名空间 Secret 支撑的远程 MCP 声明。
        let source_package = temp.path().join("source").join("local-plugin");
        fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
        write_user_plugin_package(&source_package, "local-plugin");
        // manifest extends the ordinary fixture with one valid remote MCP server.
        // manifest 使用一个有效远程 MCP 服务器扩展普通夹具。
        let mut manifest = fs::read_to_string(source_package.join("plugin.yaml"))
            .expect("read local plugin manifest");
        manifest.push_str(
            r#"permissions:
  - network_https
  - secrets_read
mcp:
  servers:
    - id: remote-docs
      name: Remote Docs
      transport:
        type: http
        url:
          source: literal
          value: https://example.test/mcp
        headers:
          Authorization:
            source: secret
            key: access-token
"#,
        );
        fs::write(source_package.join("plugin.yaml"), manifest)
            .expect("write MCP permission preview manifest");
        // manager performs the same strict source inspection used by the authorization UI.
        // manager 执行授权界面使用的同一严格来源检查。
        let manager = test_manager(&runtime_root, vulcan_plugin_root);

        // preview is safe to serialize because it contains only server ownership and Secret key names.
        // preview 可安全序列化，因为它只包含服务器归属与 Secret 键名。
        let preview = manager
            .inspect_local_user_plugin(&source_package)
            .expect("inspect MCP permission preview package");
        assert_eq!(
            preview.mcp_servers,
            vec![VulcanPluginMcpPermissionPreview {
                server_id: String::from("remote-docs"),
                name: String::from("Remote Docs"),
                transport: String::from("http"),
                secret_keys: vec![String::from("access-token")],
            }]
        );
        assert!(preview.install_secret_requirements.is_empty());
    }

    /// Verifies installation rejects a package whose contents changed after authorization inspection.
    /// 验证安装会拒绝在授权检查后内容发生变化的插件包。
    #[test]
    fn user_plugin_install_binds_authorization_to_exact_content_fingerprint() {
        // temp owns the isolated source package and all runtime state for this authorization test.
        // temp 持有当前授权测试的隔离来源包与全部运行时状态。
        let temp = tempfile::tempdir().expect("create installation authorization directory");
        // runtime_root is the deterministic state root used to detect forbidden publication.
        // runtime_root 是用于检测违规发布的确定性状态根目录。
        let runtime_root = temp.path().join("runtime");
        // vulcan_plugin_root provides an empty bundled-plugin source for the manager.
        // vulcan_plugin_root 为管理器提供空的内置插件来源。
        let vulcan_plugin_root = runtime_root.join("plugins");
        // source_package is the user-selected package whose inspected contents are later changed.
        // source_package 是用户选择并在检查后被修改内容的插件包。
        let source_package = temp.path().join("source").join("local-plugin");
        fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
        write_user_plugin_package(&source_package, "local-plugin");
        // manager owns the exact catalog, authorization, and installation transaction state.
        // manager 持有精确目录、授权与安装事务状态。
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load initial plugin catalog");
        // preview captures the package identity the user actually reviewed.
        // preview 捕获用户实际复核的插件包身份。
        let preview = manager
            .inspect_local_user_plugin(&source_package)
            .expect("inspect installation candidate");
        fs::write(
            source_package.join("runtime/plugin.bundle.lua"),
            "return { changed_after_inspection = true }\n",
        )
        .expect("change installation candidate after inspection");

        // error proves the stale authorization cannot publish a different package generation.
        // error 证明过期授权无法发布不同的插件包代次。
        let error = manager
            .install_local_user_plugin(
                &source_package,
                &preview.content_fingerprint,
                preview.permissions,
                false,
            )
            .expect_err("reject changed installation candidate");
        assert!(
            error
                .to_string()
                .contains("changed after authorization inspection")
        );
        assert!(!runtime_root.join("user_plugins/local-plugin").exists());
        assert!(
            !runtime_root
                .join("plugin_state/local-plugin.auth.json")
                .exists()
        );
    }

    /// Verifies update authorization cannot be reused after the inspected source package changes.
    /// 验证检查后的来源包发生变化时不能复用更新授权。
    #[test]
    fn user_plugin_update_binds_authorization_to_exact_content_fingerprint() {
        // temp owns the installed package, mutable update source, and transaction state.
        // temp 持有已安装包、可变更新来源与事务状态。
        let temp = tempfile::tempdir().expect("create update authorization directory");
        // runtime_root is the deterministic root containing the committed version-one package.
        // runtime_root 是包含已提交 1.0.0 插件包的确定性根目录。
        let runtime_root = temp.path().join("runtime");
        // vulcan_plugin_root provides an empty bundled-plugin source for the manager.
        // vulcan_plugin_root 为管理器提供空的内置插件来源。
        let vulcan_plugin_root = runtime_root.join("plugins");
        // source_package is reused as the independently inspected update candidate.
        // source_package 被复用为独立检查的更新候选包。
        let source_package = temp.path().join("source").join("local-plugin");
        fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
        write_user_plugin_package(&source_package, "local-plugin");
        // manager owns the exact installed generation and update gate.
        // manager 持有精确已安装代次与更新门禁。
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load initial plugin catalog");
        // initial_preview binds the first installation to its exact source contents.
        // initial_preview 将首次安装绑定到精确来源内容。
        let initial_preview = manager
            .inspect_local_user_plugin(&source_package)
            .expect("inspect initial plugin package");
        manager
            .install_local_user_plugin(
                &source_package,
                &initial_preview.content_fingerprint,
                initial_preview.permissions,
                false,
            )
            .expect("install initial plugin package");
        // disabled is the exact policy state required before an update transaction may begin.
        // disabled 是更新事务开始前要求的精确策略状态。
        let disabled = BTreeSet::from([String::from("local-plugin")]);
        manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("load disabled initial plugin");
        // manifest_path selects the sole version declaration changed for the candidate package.
        // manifest_path 选择候选包中唯一被修改的版本声明。
        let manifest_path = source_package.join("plugin.yaml");
        // version_two_manifest advances the candidate without changing its capability set.
        // version_two_manifest 在不改变能力集合的情况下推进候选版本。
        let version_two_manifest = fs::read_to_string(&manifest_path)
            .expect("read initial source manifest")
            .replace("version: 1.0.0", "version: 2.0.0");
        fs::write(&manifest_path, version_two_manifest).expect("write update candidate manifest");
        // update_preview captures the exact version-two package reviewed by the user.
        // update_preview 捕获用户复核的精确 2.0.0 插件包。
        let update_preview = manager
            .inspect_local_user_plugin(&source_package)
            .expect("inspect update candidate");
        fs::write(
            source_package.join("runtime/plugin.bundle.lua"),
            "return { replaced_after_update_inspection = true }\n",
        )
        .expect("change update candidate after inspection");

        // error proves the installed package is untouched when the authorization fingerprint is stale.
        // error 证明授权指纹过期时已安装插件包保持不变。
        let error = manager
            .begin_local_user_plugin_update(
                &source_package,
                &update_preview.content_fingerprint,
                update_preview.permissions,
                false,
            )
            .expect_err("reject changed update candidate");
        assert!(
            error
                .to_string()
                .contains("changed after update authorization inspection")
        );
        let catalog = manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("reload unchanged installed plugin");
        assert_eq!(
            catalog.plugins["local-plugin"].version.as_deref(),
            Some("1.0.0")
        );
        assert!(
            fs::read_dir(runtime_root.join("user_plugins"))
                .expect("list user plugin root")
                .all(|entry| !entry
                    .expect("read user plugin entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".update-"))
        );
    }

    /// Verifies local package installation, private environment publication, and uninstall are closed transactions.
    /// 验证本地插件包安装、私有环境发布与卸载构成闭环事务。
    #[test]
    fn user_plugin_install_environment_and_uninstall_are_transactional() {
        let temp = tempfile::tempdir().expect("create user plugin transaction directory");
        let runtime_root = temp.path().join("runtime");
        let vulcan_plugin_root = runtime_root.join("plugins");
        let source_package = temp.path().join("source").join("local-plugin");
        fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
        write_user_plugin_package(&source_package, "local-plugin");
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load initial plugin catalog");
        let preview = manager
            .inspect_local_user_plugin(&source_package)
            .expect("inspect local user plugin");
        assert_eq!(preview.plugin_id, "local-plugin");
        assert!(preview.permissions.is_empty());
        let installation = manager
            .install_local_user_plugin(
                &source_package,
                &preview.content_fingerprint,
                Vec::new(),
                false,
            )
            .expect("install local user plugin");
        assert_eq!(installation.plugin_id, "local-plugin");
        let disabled = BTreeSet::from([String::from("local-plugin")]);
        let catalog = manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("load installed user plugin");
        assert_eq!(
            catalog.plugins["local-plugin"].origin,
            VulcanPluginPackageOrigin::UserInstalled
        );
        let environment = manager
            .prepare_plugin_environment("local-plugin")
            .expect("prepare private plugin environment");
        assert!(
            environment
                .join(".vulcan-plugin-environment.json")
                .is_file()
        );
        manager
            .uninstall_user_plugin("local-plugin")
            .expect("uninstall local user plugin");
        assert!(!runtime_root.join("user_plugins/local-plugin").exists());
        assert!(!runtime_root.join("plugin_runtime/local-plugin").exists());
        assert!(
            !runtime_root
                .join("plugin_state/local-plugin.auth.json")
                .exists()
        );
    }

    /// Verifies user-plugin updates retain the previous package until commit and restore it exactly on rollback.
    /// 验证用户插件更新会保留旧插件包直到提交，并在回滚时精确恢复旧包。
    #[test]
    fn user_plugin_update_supports_exact_rollback_and_commit() {
        // temp owns package, state, and private environment generations for both update outcomes.
        // temp 持有两种更新结果使用的插件包、状态与私有环境代次。
        let temp = tempfile::tempdir().expect("create user plugin update directory");
        let runtime_root = temp.path().join("runtime");
        let vulcan_plugin_root = runtime_root.join("plugins");
        let source_package = temp.path().join("source").join("local-plugin");
        fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
        write_user_plugin_package(&source_package, "local-plugin");
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load initial plugin catalog");
        manager
            .install_local_user_plugin(
                &source_package,
                &manager
                    .inspect_local_user_plugin(&source_package)
                    .expect("inspect initial user plugin")
                    .content_fingerprint,
                Vec::new(),
                false,
            )
            .expect("install initial user plugin");
        let disabled = BTreeSet::from([String::from("local-plugin")]);
        manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("load disabled initial plugin");
        let previous_environment = manager
            .prepare_plugin_environment("local-plugin")
            .expect("prepare previous plugin environment");

        // version_two_manifest changes both the semantic version and package fingerprint.
        // version_two_manifest 同时变更语义版本与插件包指纹。
        let manifest_path = source_package.join("plugin.yaml");
        let version_two_manifest = fs::read_to_string(&manifest_path)
            .expect("read source manifest")
            .replace("version: 1.0.0", "version: 2.0.0");
        fs::write(&manifest_path, version_two_manifest).expect("write version two manifest");
        let version_two_preview = manager
            .inspect_local_user_plugin(&source_package)
            .expect("inspect version two user plugin");

        let rollback_transaction = manager
            .begin_local_user_plugin_update(
                &source_package,
                &version_two_preview.content_fingerprint,
                Vec::new(),
                false,
            )
            .expect("begin rollback update candidate");
        assert_eq!(rollback_transaction.plugin_id(), "local-plugin");
        assert_eq!(rollback_transaction.permission_preview().version, "2.0.0");
        manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("load rollback candidate catalog");
        let rolled_back_environment = manager
            .prepare_plugin_environment("local-plugin")
            .expect("prepare rollback candidate environment");
        manager
            .verify_local_user_plugin_update_candidate(&rollback_transaction)
            .expect("verify unchanged rollback candidate");
        fs::write(
            rollback_transaction
                .destination
                .join("tampered-after-validation.txt"),
            b"changed",
        )
        .expect("mutate rollback candidate after preparation");
        assert!(
            manager
                .verify_local_user_plugin_update_candidate(&rollback_transaction)
                .is_err(),
            "candidate mutation must be rejected before extension publication"
        );
        manager
            .rollback_local_user_plugin_update(rollback_transaction)
            .expect("rollback candidate package");
        let restored_catalog = manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("reload restored plugin catalog");
        assert_eq!(
            restored_catalog.plugins["local-plugin"].version.as_deref(),
            Some("1.0.0")
        );
        assert!(previous_environment.exists());
        assert!(!rolled_back_environment.exists());

        let commit_transaction = manager
            .begin_local_user_plugin_update(
                &source_package,
                &version_two_preview.content_fingerprint,
                Vec::new(),
                false,
            )
            .expect("begin committed update candidate");
        manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("load committed candidate catalog");
        let committed_environment = manager
            .prepare_plugin_environment("local-plugin")
            .expect("prepare committed candidate environment");
        manager
            .verify_local_user_plugin_update_candidate(&commit_transaction)
            .expect("verify committed candidate before publication");
        manager
            .commit_local_user_plugin_update(commit_transaction)
            .expect("commit candidate package");
        let committed_catalog = manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("reload committed plugin catalog");
        assert_eq!(
            committed_catalog.plugins["local-plugin"].version.as_deref(),
            Some("2.0.0")
        );
        assert!(committed_environment.exists());
        assert!(!previous_environment.exists());
        assert!(
            fs::read_dir(runtime_root.join("user_plugins"))
                .expect("list user plugin root")
                .all(|entry| !entry
                    .expect("read user plugin entry")
                    .file_name()
                    .to_string_lossy()
                    .starts_with(".update-"))
        );
    }

    /// Verifies a user package changed after authorization cannot seed an execution environment.
    /// 验证授权后发生变化的用户插件包不能生成执行环境。
    #[test]
    fn user_plugin_environment_rejects_post_authorization_mutation() {
        // temp owns the complete isolated installation and runtime fixture.
        // temp 持有完整的隔离安装与运行时测试环境。
        let temp = tempfile::tempdir().expect("create user plugin mutation directory");
        // runtimeRoot is the deterministic Vulcan runtime root used by the fixture manager.
        // runtimeRoot 是测试管理器使用的确定性 Vulcan 运行时根目录。
        let runtime_root = temp.path().join("runtime");
        // vulcanPluginRoot keeps the bundled root present without adding a competing plugin id.
        // vulcanPluginRoot 保持 Vulcan 插件根存在，同时不加入冲突插件标识。
        let vulcan_plugin_root = runtime_root.join("plugins");
        // sourcePackage is the package generation approved by the installation transaction.
        // sourcePackage 是安装事务批准的插件包代次。
        let source_package = temp.path().join("source").join("local-plugin");
        fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
        write_user_plugin_package(&source_package, "local-plugin");
        // manager owns catalog, authorization, and private environment state for the fixture.
        // manager 持有测试目录、授权与私有环境状态。
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load initial plugin catalog");
        manager
            .install_local_user_plugin(
                &source_package,
                &manager
                    .inspect_local_user_plugin(&source_package)
                    .expect("inspect local user plugin")
                    .content_fingerprint,
                Vec::new(),
                false,
            )
            .expect("install local user plugin");
        // disabled keeps the installed package valid without starting its runtime.
        // disabled 让已安装包保持有效，同时不启动其运行时。
        let disabled = BTreeSet::from([String::from("local-plugin")]);
        manager
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("load authorized user plugin generation");
        // fingerprint is the exact authorized generation directory name expected by publication.
        // fingerprint 是发布过程预期使用的精确授权代次目录名。
        let fingerprint = manager.snapshot().plugins["local-plugin"]
            .content_fingerprint
            .clone();
        // installedBundle is deliberately changed after the catalog captured its SHA-256 identity.
        // installedBundle 在目录捕获 SHA-256 身份后被刻意修改。
        let installed_bundle =
            runtime_root.join("user_plugins/local-plugin/runtime/plugin.bundle.lua");
        fs::write(&installed_bundle, "return { mutated = true }\n")
            .expect("mutate installed plugin bundle");

        // error proves the private copy is rehashed before publication or execution.
        // error 证明私有副本会在发布或执行前重新计算哈希。
        let error = manager
            .prepare_plugin_environment("local-plugin")
            .expect_err("reject changed authorized package");
        assert!(
            error
                .to_string()
                .contains("changed after catalog validation")
        );
        assert!(
            !runtime_root
                .join("plugin_runtime/local-plugin/1.0.0")
                .join(fingerprint)
                .exists()
        );
    }

    /// Verifies the host reaps an idle owner-bound session without another tool invocation.
    /// 验证宿主无需额外工具调用即可回收空闲的所有者绑定会话。
    #[test]
    fn idle_session_timeout_reaps_managed_lease() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let runtime_root = temp.path().join("runtime");
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_idle_session_package(&vulcan_plugin_root);
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        let stateless = runtime
            .block_on(manager.invoke(VulcanPluginInvocationRequest {
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
            .block_on(manager.invoke(VulcanPluginInvocationRequest {
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
            // The reaper starts at the one-second deadline, while asynchronous shutdown may finish later under full-workspace load.
            // 回收器在一秒截止点启动，而异步关闭在全仓负载下可能稍后才完成。
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if manager.snapshot().plugins["idle-plugin"].active_session_count == 0 {
                        break;
                    }
                    tokio::time::sleep(Duration::from_millis(25)).await;
                }
            })
            .await
            .expect("idle session should be reaped within the bounded shutdown window");
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
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_idle_session_package(&vulcan_plugin_root);
        let manager = test_manager(&runtime_root, vulcan_plugin_root.clone());
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("create Tokio test runtime");
        let opened = runtime
            .block_on(manager.invoke(VulcanPluginInvocationRequest {
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
            vulcan_plugin_root.join("idle-plugin/dependencies.yaml"),
            "# generation change\n",
        )
        .expect("change package fingerprint");
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("refresh changed package");

        let closed = runtime
            .block_on(manager.invoke(VulcanPluginInvocationRequest {
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
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_idle_session_package(&vulcan_plugin_root);
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        let opened = runtime
            .block_on(manager.invoke(VulcanPluginInvocationRequest {
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
            .block_on(manager.invoke(VulcanPluginInvocationRequest {
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
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_idle_session_package(&vulcan_plugin_root);
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        runtime.block_on(async {
            let opened = manager
                .invoke(VulcanPluginInvocationRequest {
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
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_idle_session_package(&vulcan_plugin_root);
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load idle plugin catalog");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("create Tokio test runtime");
        runtime.block_on(async {
            manager
                .invoke(VulcanPluginInvocationRequest {
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
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        write_hook_package(&vulcan_plugin_root);
        let manager = test_manager(&runtime_root, vulcan_plugin_root.clone());
        let first = manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("load first catalog");
        let first_generation = first.plugins["hook-plugin"].generation;
        fs::write(
            vulcan_plugin_root.join("hook-plugin/dependencies.yaml"),
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
        let vulcan_plugin_root = runtime_root.join("plugins");
        fs::create_dir_all(&vulcan_plugin_root).expect("create Vulcan Plugin root");
        let manager = test_manager(&runtime_root, vulcan_plugin_root);
        manager
            .retain_failed_lease(
                RetainedVulcanPluginLease {
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

    /// Verifies plugin process specifications expose standard Windows paths to the Lua bridge.
    /// 验证插件进程规格向 Lua 桥暴露标准 Windows 路径。
    #[cfg(windows)]
    #[test]
    fn plugin_process_spec_normalizes_windows_verbatim_paths() {
        let spec = build_plugin_process_spec(
            PathBuf::from(r"\\?\D:\openvulcan\runtime\node.exe"),
            PathBuf::from(r"\\?\D:\openvulcan\plugins\context7"),
            Vec::new(),
            "exec",
            &Map::new(),
            None,
            BTreeMap::new(),
        )
        .expect("build plugin process specification");

        assert_eq!(
            spec.get("program").and_then(Value::as_str),
            Some(r"D:\openvulcan\runtime\node.exe")
        );
        assert_eq!(
            spec.get("cwd").and_then(Value::as_str),
            Some(r"D:\openvulcan\plugins\context7")
        );
    }
}
