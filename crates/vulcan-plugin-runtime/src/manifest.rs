//! Declarative manifest models for bundled System Plugin packages.
//! 内建系统插件包使用的声明式清单模型。

use serde::Deserialize;
use std::collections::BTreeMap;
use vulcan_hooks::{HookCapability, HookFailurePolicy, HookMode, HookScope};

/// Current manifest schema version accepted by the host.
/// 当前宿主接受的清单结构版本。
pub const SYSTEM_PLUGIN_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Stable host-owned argument used to address one managed plugin session.
/// 用于寻址单个受管插件会话的稳定宿主参数。
pub const SYSTEM_PLUGIN_SESSION_ID_ARGUMENT: &str = "session_id";

/// Host-owned payload fields that plugin tool schemas and model arguments must never declare.
/// 插件工具 Schema 与模型参数绝不能声明的宿主自有载荷字段。
pub const SYSTEM_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS: [&str; 3] =
    ["op", "request_id", "host_timeout_ms"];

/// Exact trusted plugin kind supported by the first host implementation.
/// 第一版宿主实现支持的精确信任插件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginKind {
    /// Plugin bundled and installed by the Vulcan Code distribution.
    /// 由 Vulcan Code 发行包内建并安装的插件。
    BundledSystem,
}

/// One complete System Plugin package manifest.
/// 一份完整的系统插件包清单。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginManifest {
    /// Manifest schema version.
    /// 清单结构版本。
    pub schema_version: u32,
    /// Stable plugin identifier.
    /// 稳定插件标识。
    pub id: String,
    /// Package semantic version.
    /// 插件包语义版本。
    pub version: String,
    /// Trusted plugin distribution kind.
    /// 可信插件分发类型。
    pub kind: SystemPluginKind,
    /// Whether the plugin is enabled before user overrides are applied.
    /// 应用用户覆盖前插件是否默认启用。
    pub default_enabled: bool,
    /// Default user-facing metadata.
    /// 默认的用户可见元数据。
    pub display: SystemPluginDisplayManifest,
    /// Platform availability declaration.
    /// 平台可用性声明。
    #[serde(default)]
    pub platforms: SystemPluginPlatformManifest,
    /// Managed Lua runtime declaration.
    /// 受管 Lua 运行时声明。
    pub runtime: SystemPluginRuntimeManifest,
    /// Atomic provider-visible tool group.
    /// 原子化的 Provider 可见工具组。
    #[serde(default)]
    pub tools: Vec<SystemPluginToolManifest>,
    /// Lua hook handlers registered into host-owned hook points.
    /// 注册到宿主自有 hook 点位的 Lua hook 处理器。
    #[serde(default)]
    pub hooks: Vec<SystemPluginHookManifest>,
    /// Prompt contributions activated with their required tools.
    /// 随所需工具共同激活的提示词贡献。
    #[serde(default)]
    pub prompt_contributions: Vec<SystemPluginPromptManifest>,
    /// Locale file paths keyed by locale identifier.
    /// 以语言标识为键的本地化文件路径。
    #[serde(default)]
    pub locales: BTreeMap<String, String>,
}

/// Default user-facing plugin metadata.
/// 默认的用户可见插件元数据。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginDisplayManifest {
    /// Default plugin name.
    /// 默认插件名称。
    pub name: String,
    /// Default concise capability description.
    /// 默认的简洁能力描述。
    pub capability: String,
}

/// Target-platform availability declaration.
/// 目标平台可用性声明。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginPlatformManifest {
    /// Exact Rust target triples excluded by the package.
    /// 插件包明确排除的 Rust 目标三元组。
    #[serde(default)]
    pub excluded_targets: Vec<String>,
}

/// Managed Lua runtime entry declaration.
/// 受管 Lua 运行时入口声明。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginRuntimeManifest {
    /// Dependency manifest relative to the package root.
    /// 相对于插件包根目录的依赖清单。
    pub dependencies_file: String,
    /// Additional package-relative regular files required before the plugin can be enabled.
    /// 插件可启用前必须存在的其他包相对普通文件集合。
    #[serde(default)]
    pub required_files: Vec<String>,
    /// Lua module loaded inside each managed System lease.
    /// 每个受管系统租约中加载的 Lua 模块。
    pub module: String,
    /// Lua function used for normal operations.
    /// 常规操作使用的 Lua 函数。
    pub invoke_function: String,
    /// Lua function used before closing a session lease.
    /// 关闭会话租约前使用的 Lua 函数。
    pub shutdown_function: String,
    /// Optional Lua function invoked before closing the persistent hook lease.
    /// 关闭持久 hook 租约前调用的可选 Lua 函数。
    #[serde(default)]
    pub hook_shutdown_function: Option<String>,
    /// Hard upper bound for concurrently active sessions.
    /// 并发活动会话的硬上限。
    pub max_active_sessions: usize,
    /// Optional idle timeout that triggers host-owned session cleanup.
    /// 触发宿主自有会话清理的可选空闲超时秒数。
    #[serde(default)]
    pub session_idle_timeout_sec: Option<u64>,
}

/// One Lua handler bound to an existing host-owned hook point.
/// 绑定到现有宿主自有 hook 点位的单个 Lua 处理器。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginHookManifest {
    /// Stable hook identifier within the owning plugin.
    /// 所属插件内部稳定的 hook 标识。
    pub id: String,
    /// Exact host hook point subscribed by this handler.
    /// 该处理器订阅的精确宿主 hook 点位。
    pub point: String,
    /// Existing host hook behavior mode used for result aggregation.
    /// 结果聚合所使用的现有宿主 hook 行为模式。
    pub mode: HookMode,
    /// Configuration scope projected into the host registration.
    /// 投影到宿主注册项的配置作用域。
    #[serde(default)]
    pub scope: HookScope,
    /// Lua module function that receives the exact serialized invocation frame.
    /// 接收精确序列化调用帧的 Lua 模块函数。
    pub function: String,
    /// Lower priority values execute before larger values.
    /// 优先级数值越低越早执行。
    #[serde(default)]
    pub priority: i32,
    /// Host-side execution timeout in milliseconds.
    /// 宿主侧执行超时时间，单位毫秒。
    #[serde(default)]
    pub timeout_ms: Option<u64>,
    /// Failure behavior delegated to the existing hook dispatcher.
    /// 委托给现有 hook 调度器的失败处理行为。
    #[serde(default)]
    pub failure_policy: HookFailurePolicy,
    /// Host capabilities requested by the handler.
    /// 处理器请求的宿主能力集合。
    #[serde(default)]
    pub requested_capabilities: Vec<HookCapability>,
    /// Optional user-facing handler label.
    /// 可选的用户可见处理器标签。
    #[serde(default)]
    pub display_name: Option<String>,
    /// Optional progress label for blocking preflight execution.
    /// 阻塞式前置执行使用的可选进度标签。
    #[serde(default)]
    pub progress_label: Option<String>,
}

/// Supported generic tool execution lifecycle.
/// 支持的通用工具执行生命周期。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginToolExecutionKind {
    /// Create, invoke, and close one temporary managed lease for a single call.
    /// 为单次调用创建、执行并关闭一个临时受管租约。
    Stateless,
    /// Open and register one owner-bound managed session.
    /// 打开并注册一个绑定所有者的受管会话。
    SessionOpen,
    /// Invoke one operation through an existing owner-bound session.
    /// 通过已有的所有者绑定会话调用操作。
    SessionCall,
    /// Close and retire one existing owner-bound session.
    /// 关闭并回收一个已有的所有者绑定会话。
    SessionClose,
}

/// Runtime dispatch declaration attached to one plugin tool.
/// 附着在单个插件工具上的运行时调度声明。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginToolExecutionManifest {
    /// Generic session lifecycle kind.
    /// 通用会话生命周期类型。
    pub kind: SystemPluginToolExecutionKind,
    /// Plugin-owned operation name inserted into the exact JSON payload.
    /// 插入精确 JSON 载荷的插件自有操作名。
    pub operation: String,
    /// Fixed host timeout when no bounded timeout argument is declared.
    /// 未声明有界超时参数时使用的固定宿主超时。
    pub timeout_ms: u64,
    /// Optional argument whose value extends the fixed host timeout.
    /// 可选参数，其数值会扩展固定宿主超时。
    #[serde(default)]
    pub timeout_argument: Option<String>,
    /// Maximum accepted value for the optional timeout argument.
    /// 可选超时参数允许的最大值。
    #[serde(default)]
    pub max_timeout_ms: Option<u64>,
    /// Grace window added after the selected operation timeout.
    /// 选定操作超时后追加的宽限窗口。
    #[serde(default)]
    pub timeout_grace_ms: u64,
}

/// One provider-visible tool declared by a plugin package.
/// 插件包声明的单个 Provider 可见工具。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginToolManifest {
    /// Stable globally unique tool identifier.
    /// 全局唯一的稳定工具标识。
    pub id: String,
    /// Atomic tool group identifier.
    /// 原子工具组标识。
    pub group: String,
    /// English model-visible tool description.
    /// 面向模型的英文工具描述。
    pub description: String,
    /// JSON Schema file relative to the package root.
    /// 相对于插件包根目录的 JSON Schema 文件。
    pub parameters_file: String,
    /// Whether the tool may execute in Plan mode.
    /// 工具是否允许在计划模式中执行。
    #[serde(default)]
    pub available_in_plan: bool,
    /// Generic runtime dispatch declaration.
    /// 通用运行时调度声明。
    pub execution: SystemPluginToolExecutionManifest,
}

/// One plugin-level model instruction contribution.
/// 单个插件级模型说明贡献。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginPromptManifest {
    /// Stable contribution identifier within the plugin.
    /// 插件内部稳定的贡献标识。
    pub id: String,
    /// Markdown file relative to the package root.
    /// 相对于插件包根目录的 Markdown 文件。
    pub file: String,
    /// Exact tools that must all be visible before this contribution is emitted.
    /// 发出此贡献前必须全部可见的精确工具集合。
    pub required_tools: Vec<String>,
}

/// Optional localized user-facing metadata loaded from one locale file.
/// 从单个语言文件加载的可选用户可见元数据。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginLocaleManifest {
    /// Localized plugin name.
    /// 本地化插件名称。
    pub name: String,
    /// Localized concise capability description.
    /// 本地化简洁能力描述。
    pub capability: String,
}
