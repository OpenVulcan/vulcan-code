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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginKind {
    /// Plugin bundled and installed by the Vulcan Code distribution.
    /// 由 Vulcan Code 发行包内建并安装的插件。
    BundledSystem,
    /// Plugin installed into the current user's Vulcan plugin root.
    /// 安装到当前用户 Vulcan 插件根目录的插件。
    UserInstalled,
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
    /// Dependency installation policy owned by the Vulcan host.
    /// 由 Vulcan 宿主持有的依赖安装策略。
    #[serde(default)]
    pub dependency_policy: SystemPluginDependencyPolicyManifest,
    /// Optional lifecycle callbacks exported by the Lua bundle.
    /// Lua bundle 导出的可选生命周期回调。
    #[serde(default)]
    pub lifecycle: SystemPluginLifecycleManifest,
    /// Explicit host capabilities requested by this plugin.
    /// 当前插件请求的显式宿主能力集合。
    #[serde(default)]
    pub capabilities: Vec<SystemPluginCapability>,
    /// Host-callable plugin business actions.
    /// 可由宿主调用的插件业务 action 集合。
    #[serde(default)]
    pub actions: Vec<SystemPluginActionManifest>,
    /// Dynamic WebUI mount declarations.
    /// 动态 WebUI 挂载声明。
    #[serde(default)]
    pub ui: SystemPluginUiManifest,
    /// Optional plugin configuration contract.
    /// 可选插件配置契约。
    #[serde(default)]
    pub config: Option<SystemPluginConfigManifest>,
    /// Logical executable resources resolved by the Vulcan host.
    /// 由 Vulcan 宿主解析的逻辑可执行资源。
    #[serde(default)]
    pub binaries: Vec<SystemPluginBinaryManifest>,
    /// Logical Node.js entries resolved through the shared managed runtime distribution.
    /// 通过共享受管运行时发行版解析的逻辑 Node.js 入口。
    #[serde(default)]
    pub node_entries: Vec<SystemPluginNodeEntryManifest>,
    /// Logical Python entries resolved through the shared managed runtime distribution.
    /// 通过共享受管运行时发行版解析的逻辑 Python 入口。
    #[serde(default)]
    pub python_entries: Vec<SystemPluginPythonEntryManifest>,
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

/// Dependency installation source policy for one plugin package.
/// 单个插件包的依赖安装来源策略。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginDependencyPolicyManifest {
    /// Whether dependencies must be installed from bundled assets or an approved registry.
    /// 依赖必须从随包资产还是经授权的注册表安装。
    #[serde(default)]
    pub install_mode: SystemPluginDependencyInstallMode,
}

/// Supported dependency acquisition modes.
/// 支持的依赖获取模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginDependencyInstallMode {
    /// Install exclusively from artifacts shipped inside the plugin package.
    /// 仅从插件包内随附的制品安装。
    #[default]
    Bundled,
    /// Install from a registry after explicit network authorization.
    /// 在获得明确网络授权后从注册表安装。
    Registry,
}

/// Optional Lua lifecycle function declarations.
/// 可选 Lua 生命周期函数声明。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginLifecycleManifest {
    /// Function invoked after a user plugin package is installed.
    /// 用户插件包安装完成后调用的函数。
    #[serde(default)]
    pub install: Option<String>,
    /// Function invoked before a plugin becomes enabled.
    /// 插件进入启用状态前调用的函数。
    #[serde(default)]
    pub enable: Option<String>,
    /// Function invoked while a plugin is being disabled.
    /// 插件停用过程中调用的函数。
    #[serde(default)]
    pub disable: Option<String>,
    /// Function invoked before a user plugin package is removed.
    /// 用户插件包移除前调用的函数。
    #[serde(default)]
    pub uninstall: Option<String>,
}

/// Host capability that must be declared and authorized before use.
/// 使用前必须声明并授权的宿主能力。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginCapability {
    /// Access host-allowlisted Lua runtime modules declared by this bundled plugin.
    /// 访问当前内建插件声明且经宿主白名单允许的 Lua 运行时模块。
    RuntimeLuaModules,
    /// Perform HTTPS requests through an explicitly declared runtime network client.
    /// 通过显式声明的运行时网络客户端执行 HTTPS 请求。
    NetworkHttps,
    /// Execute manifest-declared native binaries.
    /// 执行清单声明的原生二进制文件。
    ProcessBinary,
    /// Execute manifest-declared Node.js entries.
    /// 执行清单声明的 Node.js 入口。
    ProcessNode,
    /// Execute manifest-declared Python entries.
    /// 执行清单声明的 Python 入口。
    ProcessPython,
    /// Open long-lived process sessions.
    /// 打开长生命周期进程会话。
    ProcessSession,
    /// Read and update the current plugin configuration.
    /// 读取并更新当前插件配置。
    Config,
    /// Render dynamic UI with the stable host UI context.
    /// 使用稳定宿主 UI 上下文渲染动态界面。
    UiHostContext,
    /// Read the host model catalog for future workflow binding.
    /// 读取宿主模型目录以供后续工作流绑定。
    ModelCatalogRead,
    /// Invoke one user-authorized model binding.
    /// 调用一个经用户授权的模型绑定。
    ModelInvoke,
    /// Install dependencies from a remote registry.
    /// 从远程注册表安装依赖。
    DependencyRegistryNetwork,
    /// Use the current plugin's isolated SQLite database.
    /// 使用当前插件的隔离 SQLite 数据库。
    StorageSqlite,
    /// Use the current plugin's isolated LanceDB database.
    /// 使用当前插件的隔离 LanceDB 数据库。
    StorageLancedb,
}

/// One plugin business action callable through the generic service boundary.
/// 一个可通过通用服务边界调用的插件业务 action。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginActionManifest {
    /// Stable plugin-local action identifier.
    /// 稳定的插件内 action 标识。
    pub id: String,
    /// Exact Lua bundle function invoked for this action.
    /// 当前 action 调用的精确 Lua bundle 函数。
    pub function: String,
    /// Optional package-relative JSON Schema for action input.
    /// 可选的插件包相对 action 输入 JSON Schema。
    #[serde(default)]
    pub input_schema_file: Option<String>,
    /// Optional package-relative JSON Schema for action output.
    /// 可选的插件包相对 action 输出 JSON Schema。
    #[serde(default)]
    pub output_schema_file: Option<String>,
    /// Hard action execution timeout in milliseconds.
    /// action 执行硬超时毫秒数。
    pub timeout_ms: u64,
    /// Explicit caller classes allowed to invoke this action.
    /// 允许调用当前 action 的显式调用方类别。
    pub callers: Vec<SystemPluginActionCaller>,
}

/// Authorized caller class for one plugin action.
/// 单个插件 action 的授权调用方类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginActionCaller {
    /// The plugin's own Lua bundle may invoke the action.
    /// 插件自身 Lua bundle 可以调用 action。
    PluginRuntime,
    /// The plugin's mounted WebUI component may invoke the action.
    /// 插件挂载的 WebUI 组件可以调用 action。
    PluginUi,
    /// The host lifecycle state machine may invoke the action.
    /// 宿主生命周期状态机可以调用 action。
    Lifecycle,
}

/// Dynamic UI declarations for one plugin.
/// 单个插件的动态 UI 声明。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginUiManifest {
    /// Ordered UI mounts contributed by the plugin.
    /// 插件贡献的有序 UI 挂载集合。
    #[serde(default)]
    pub mounts: Vec<SystemPluginUiMountManifest>,
}

/// One uncompressed ESM module mounted into a host-owned UI location.
/// 一个挂载到宿主持有界面位置的未压缩 ESM 模块。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginUiMountManifest {
    /// Stable plugin-local mount identifier.
    /// 稳定的插件内挂载标识。
    pub id: String,
    /// Host-owned mount point.
    /// 宿主持有的挂载点。
    pub point: SystemPluginUiMountPoint,
    /// Default host navigation label for this mount.
    /// 此挂载点使用的默认宿主导航名称。
    #[serde(default)]
    pub label: Option<String>,
    /// Plugin locale message key overriding the navigation label.
    /// 覆盖导航名称的插件语言包消息键。
    #[serde(default)]
    pub label_message_key: Option<String>,
    /// Host-owned icon rendered beside the navigation label.
    /// 在导航名称旁渲染的宿主持有图标。
    #[serde(default)]
    pub icon: SystemPluginUiMountIcon,
    /// Package-relative uncompressed ESM file.
    /// 插件包相对未压缩 ESM 文件。
    pub entry: String,
    /// Named ESM export containing the mount factory.
    /// 包含挂载工厂的 ESM 命名导出。
    pub export: String,
}

/// Stable host-owned icons available to dynamic plugin navigation entries.
/// 动态插件导航条目可使用的稳定宿主图标。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginUiMountIcon {
    /// Generic plugin puzzle icon.
    /// 通用插件拼图图标。
    #[default]
    Plugin,
    /// Command-routing icon.
    /// 命令路由图标。
    Route,
    /// Usage statistics chart icon.
    /// 用量统计图表图标。
    Chart,
}

/// Stable host-owned dynamic UI mount points.
/// 稳定的宿主持有动态 UI 挂载点。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPluginUiMountPoint {
    /// Plugin detail page below the LuaSkills/Plugins area.
    /// LuaSkills/插件区域下的插件详情页。
    SkillsPluginDetail,
    /// Main workspace page used by future workflows.
    /// 供后续工作流使用的主工作区页面。
    WorkspaceMain,
    /// Conversation right-side auxiliary panel.
    /// 会话右侧辅助面板。
    ConversationRightPanel,
}

/// Plugin-owned configuration schema and defaults.
/// 插件持有的配置 Schema 与默认值。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginConfigManifest {
    /// Package-relative JSON Schema for the complete configuration object.
    /// 完整配置对象使用的插件包相对 JSON Schema。
    pub schema_file: String,
    /// Package-relative JSON file containing default configuration.
    /// 包含默认配置的插件包相对 JSON 文件。
    pub defaults_file: String,
}

/// One logical native executable with package-relative target mappings.
/// 一个带有插件包相对目标映射的逻辑原生可执行文件。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginBinaryManifest {
    /// Stable plugin-local executable identifier.
    /// 稳定的插件内可执行文件标识。
    pub id: String,
    /// Package-relative executable paths keyed by exact Rust target triple.
    /// 以精确 Rust 目标三元组为键的插件包相对可执行文件路径。
    pub targets: BTreeMap<String, String>,
    /// Environment variable names that plugin calls may set for this executable.
    /// 插件调用此可执行文件时允许设置的环境变量名。
    #[serde(default)]
    pub allowed_env: Vec<String>,
    /// Whether this binary may run in a caller workspace directory captured by the trusted bridge.
    /// 此二进制是否可以在可信桥接层捕获的调用方工作区目录内运行。
    #[serde(default)]
    pub allow_workspace_cwd: bool,
}

/// One logical Node.js script entry owned by a plugin package.
/// 单个由插件包持有的逻辑 Node.js 脚本入口。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginNodeEntryManifest {
    /// Stable plugin-local Node entry identifier.
    /// 稳定的插件内 Node 入口标识。
    pub id: String,
    /// Package-relative JavaScript module file.
    /// 插件包相对 JavaScript 模块文件。
    pub file: String,
    /// Package-relative working directory.
    /// 插件包相对工作目录。
    pub cwd: String,
}

/// One logical Python script entry owned by a plugin package.
/// 单个由插件包持有的逻辑 Python 脚本入口。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPluginPythonEntryManifest {
    /// Stable plugin-local Python entry identifier.
    /// 稳定的插件内 Python 入口标识。
    pub id: String,
    /// Package-relative Python script file.
    /// 插件包相对 Python 脚本文件。
    pub file: String,
    /// Package-relative working directory.
    /// 插件包相对工作目录。
    pub cwd: String,
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
    /// Package-relative bundled Lua source loaded through the fixed Vulcan bridge package.
    /// 通过固定 Vulcan 桥接包加载的插件包相对 Lua bundle 源文件。
    pub bundle: String,
    /// Additional package-relative regular files required before the plugin can be enabled.
    /// 插件可启用前必须存在的其他包相对普通文件集合。
    #[serde(default)]
    pub required_files: Vec<String>,
    /// Host-allowlisted Lua modules exposed through the restricted plugin runtime API.
    /// 通过受限插件运行时 API 暴露的宿主白名单 Lua 模块。
    #[serde(default)]
    pub lua_modules: Vec<String>,
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
    /// Plugin-owned UI messages keyed by stable message identifier.
    /// 由插件持有并按稳定消息标识组织的界面文案。
    #[serde(default)]
    pub messages: BTreeMap<String, String>,
}
