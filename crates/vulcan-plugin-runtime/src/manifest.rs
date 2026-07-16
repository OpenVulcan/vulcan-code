//! Declarative manifest models for bundled Vulcan Plugin packages.
//! Vulcan 插件包使用的声明式清单模型。

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use vulcan_hooks::{HookCapability, HookFailurePolicy, HookMode, HookScope};

/// Current manifest schema version accepted by the host.
/// 当前宿主接受的清单结构版本。
pub const VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Stable host-owned argument used to address one managed plugin session.
/// 用于寻址单个受管插件会话的稳定宿主参数。
pub const VULCAN_PLUGIN_SESSION_ID_ARGUMENT: &str = "session_id";

/// Host-owned payload fields that plugin tool schemas and model arguments must never declare.
/// 插件工具 Schema 与模型参数绝不能声明的宿主自有载荷字段。
pub const VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS: [&str; 3] =
    ["op", "request_id", "host_timeout_ms"];

/// One complete Vulcan Plugin package manifest.
/// 一份完整的 Vulcan 插件包清单。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginManifest {
    /// Manifest schema version.
    /// 清单结构版本。
    pub schema_version: u32,
    /// Stable plugin identifier.
    /// 稳定插件标识。
    pub id: String,
    /// Package semantic version.
    /// 插件包语义版本。
    pub version: String,
    /// Whether the plugin is enabled before user overrides are applied.
    /// 应用用户覆盖前插件是否默认启用。
    pub default_enabled: bool,
    /// Default user-facing metadata.
    /// 默认的用户可见元数据。
    pub display: VulcanPluginDisplayManifest,
    /// Platform availability declaration.
    /// 平台可用性声明。
    #[serde(default)]
    pub platforms: VulcanPluginPlatformManifest,
    /// Explicit host permissions requested by this plugin.
    /// 当前插件请求的显式宿主权限集合。
    #[serde(default)]
    pub permissions: Vec<VulcanPluginPermission>,
    /// Package-relative regular files required for every enabled plugin component.
    /// 每个已启用插件组件都要求存在的插件包相对普通文件集合。
    #[serde(default)]
    pub required_files: Vec<String>,
    /// Host-managed execution resources shared by executable plugin protocols.
    /// 由可执行插件协议共享的宿主管理执行资源。
    #[serde(default)]
    pub execution: VulcanPluginExecutionManifest,
    /// Optional managed Lua protocol declaration.
    /// 可选的受管 Lua 协议声明。
    #[serde(default)]
    pub lua_runtime: Option<VulcanPluginLuaRuntimeManifest>,
    /// Optional Model Context Protocol server declarations.
    /// 可选的模型上下文协议服务器声明。
    #[serde(default)]
    pub mcp: VulcanPluginMcpManifest,
    /// Dynamic WebUI mount declarations.
    /// 动态 WebUI 挂载声明。
    #[serde(default)]
    pub ui: VulcanPluginUiManifest,
    /// Optional plugin configuration contract.
    /// 可选插件配置契约。
    #[serde(default)]
    pub config: Option<VulcanPluginConfigManifest>,
    /// Prompt contributions activated with their required tools.
    /// 随所需工具共同激活的提示词贡献。
    #[serde(default)]
    pub prompt_contributions: Vec<VulcanPluginPromptManifest>,
    /// Locale file paths keyed by locale identifier.
    /// 以语言标识为键的本地化文件路径。
    #[serde(default)]
    pub locales: BTreeMap<String, String>,
}

impl VulcanPluginManifest {
    /// Returns whether this package declares the managed Lua protocol.
    /// 返回当前插件包是否声明受管 Lua 协议。
    ///
    /// # Returns
    /// 返回值
    /// `true` when the optional Lua protocol is present.
    /// 当可选 Lua 协议存在时返回 `true`。
    pub fn has_lua_runtime(&self) -> bool {
        self.lua_runtime.is_some()
    }

    /// Returns whether a content-only package declares an execution-owned feature.
    /// 返回内容型插件包是否声明了执行侧能力。
    ///
    /// # Returns
    /// 返回值
    /// `true` when any declaration requires an executable runtime or host capability.
    /// 任一声明需要可执行运行时或宿主能力时返回 `true`。
    pub fn has_execution_declarations(&self) -> bool {
        !self.permissions.is_empty()
            || !self.ui.mounts.is_empty()
            || self.config.is_some()
            || !self.execution.native_binaries.is_empty()
            || !self.execution.node_entries.is_empty()
            || !self.execution.python_entries.is_empty()
            || self.lua_runtime.is_some()
            || !self.mcp.servers.is_empty()
            || !self.prompt_contributions.is_empty()
            || !self.required_files.is_empty()
    }
}

/// Host-managed execution resources shared by Lua and MCP protocols.
/// 由 Lua 与 MCP 协议共享的宿主管理执行资源。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginExecutionManifest {
    /// Dependency graph and acquisition policy for the private plugin environment.
    /// 插件私有环境的依赖图与获取策略。
    #[serde(default)]
    pub dependencies: VulcanPluginDependenciesManifest,
    /// Logical native executable resources resolved by the Vulcan host.
    /// 由 Vulcan 宿主解析的逻辑原生可执行资源。
    #[serde(default)]
    pub native_binaries: Vec<VulcanPluginBinaryManifest>,
    /// Logical Node.js entries resolved through the managed runtime distribution.
    /// 通过受管运行时发行版解析的逻辑 Node.js 入口。
    #[serde(default)]
    pub node_entries: Vec<VulcanPluginNodeEntryManifest>,
    /// Logical Python entries resolved through the managed runtime distribution.
    /// 通过受管运行时发行版解析的逻辑 Python 入口。
    #[serde(default)]
    pub python_entries: Vec<VulcanPluginPythonEntryManifest>,
}

/// Dependency declaration for one plugin-private execution environment.
/// 单个插件私有执行环境的依赖声明。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginDependenciesManifest {
    /// Package-relative dependency manifest consumed by the managed runtime installer.
    /// 由受管运行时安装器消费的插件包相对依赖清单。
    #[serde(default)]
    pub manifest: Option<String>,
    /// Whether dependencies come from bundled assets or an approved registry.
    /// 依赖来自随包资产还是经授权的注册表。
    #[serde(default)]
    pub install_mode: VulcanPluginDependencyInstallMode,
}

/// Supported dependency acquisition modes.
/// 支持的依赖获取模式。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginDependencyInstallMode {
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
pub struct VulcanPluginLifecycleManifest {
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

/// Host permission that must be declared and authorized before use.
/// 使用前必须声明并授权的宿主权限。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginPermission {
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
    /// Read explicitly referenced secrets scoped to the current plugin.
    /// 读取限定到当前插件且被显式引用的 Secret。
    SecretsRead,
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
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginActionManifest {
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
    pub callers: Vec<VulcanPluginActionCaller>,
}

/// Authorized caller class for one plugin action.
/// 单个插件 action 的授权调用方类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginActionCaller {
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
pub struct VulcanPluginUiManifest {
    /// Ordered UI mounts contributed by the plugin.
    /// 插件贡献的有序 UI 挂载集合。
    #[serde(default)]
    pub mounts: Vec<VulcanPluginUiMountManifest>,
}

/// One uncompressed ESM module mounted into a host-owned UI location.
/// 一个挂载到宿主持有界面位置的未压缩 ESM 模块。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginUiMountManifest {
    /// Stable plugin-local mount identifier.
    /// 稳定的插件内挂载标识。
    pub id: String,
    /// Host-owned mount point.
    /// 宿主持有的挂载点。
    pub point: VulcanPluginUiMountPoint,
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
    pub icon: VulcanPluginUiMountIcon,
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
pub enum VulcanPluginUiMountIcon {
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
pub enum VulcanPluginUiMountPoint {
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
pub struct VulcanPluginConfigManifest {
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
pub struct VulcanPluginBinaryManifest {
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
pub struct VulcanPluginNodeEntryManifest {
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
pub struct VulcanPluginPythonEntryManifest {
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
pub struct VulcanPluginDisplayManifest {
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
pub struct VulcanPluginPlatformManifest {
    /// Exact Rust target triples excluded by the package.
    /// 插件包明确排除的 Rust 目标三元组。
    #[serde(default)]
    pub excluded_targets: Vec<String>,
}

/// Optional managed Lua protocol implemented by one plugin bundle.
/// 由单个插件 bundle 实现的可选受管 Lua 协议。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginLuaRuntimeManifest {
    /// Package-relative bundled Lua source loaded through the fixed Vulcan bridge package.
    /// 通过固定 Vulcan 桥接包加载的插件包相对 Lua bundle 源文件。
    pub bundle: String,
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
    /// Lifecycle callbacks exported by the Lua bundle.
    /// Lua bundle 导出的生命周期回调。
    #[serde(default)]
    pub lifecycle: VulcanPluginLifecycleManifest,
    /// Host-callable plugin business actions implemented by Lua functions.
    /// 由 Lua 函数实现且可由宿主调用的插件业务 action 集合。
    #[serde(default)]
    pub actions: Vec<VulcanPluginActionManifest>,
    /// Atomic provider-visible Lua tool group.
    /// 原子化的 Provider 可见 Lua 工具组。
    #[serde(default)]
    pub tools: Vec<VulcanPluginToolManifest>,
    /// Lua hook handlers registered into host-owned hook points.
    /// 注册到宿主自有 hook 点位的 Lua hook 处理器。
    #[serde(default)]
    pub hooks: Vec<VulcanPluginHookManifest>,
}

/// Model Context Protocol servers contributed by one plugin.
/// 单个插件贡献的模型上下文协议服务器集合。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginMcpManifest {
    /// Independently managed servers keyed by their plugin-local identifiers.
    /// 以插件内标识独立管理的服务器集合。
    #[serde(default)]
    pub servers: Vec<VulcanPluginMcpServerManifest>,
}

/// One plugin-owned Model Context Protocol server.
/// 单个插件自有的模型上下文协议服务器。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginMcpServerManifest {
    /// Stable server identifier within the owning plugin.
    /// 所属插件内部稳定的服务器标识。
    pub id: String,
    /// Default user-facing server name.
    /// 默认的用户可见服务器名称。
    pub name: String,
    /// Strictly selected transport and its transport-specific settings.
    /// 严格选择的传输方式及其专属设置。
    pub transport: VulcanPluginMcpTransportManifest,
}

impl VulcanPluginMcpServerManifest {
    /// Returns the exact deduplicated Secret Store key allowlist declared by this server.
    /// 返回当前服务器声明的精确去重 Secret Store 键白名单。
    pub fn declared_secret_keys(&self) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        collect_mcp_transport_secret_keys(&self.transport, &mut keys);
        keys
    }
}

/// Collects secret keys from every typed value source in one MCP transport.
/// 从单个 MCP 传输的每个带类型值来源收集 Secret 键。
fn collect_mcp_transport_secret_keys(
    transport: &VulcanPluginMcpTransportManifest,
    keys: &mut BTreeSet<String>,
) {
    match transport {
        VulcanPluginMcpTransportManifest::Stdio { args, env, .. } => {
            for source in args.iter().chain(env.values()) {
                collect_mcp_value_source_secret_key(source, keys);
            }
        }
        VulcanPluginMcpTransportManifest::Http {
            url,
            headers,
            oauth,
        }
        | VulcanPluginMcpTransportManifest::Sse {
            url,
            headers,
            oauth,
        } => {
            collect_mcp_value_source_secret_key(url, keys);
            for source in headers.values() {
                collect_mcp_value_source_secret_key(source, keys);
            }
            if let Some(oauth) = oauth {
                for source in [
                    oauth.client_id.as_ref(),
                    oauth.client_secret.as_ref(),
                    oauth.redirect_uri.as_ref(),
                ]
                .into_iter()
                .flatten()
                {
                    collect_mcp_value_source_secret_key(source, keys);
                }
            }
        }
    }
}

/// Adds one Secret source key and ignores non-secret sources by construction.
/// 添加单个 Secret 来源键，并按结构忽略非 Secret 来源。
fn collect_mcp_value_source_secret_key(
    source: &VulcanPluginValueSourceManifest,
    keys: &mut BTreeSet<String>,
) {
    if let VulcanPluginValueSourceManifest::Secret { key } = source {
        keys.insert(key.clone());
    }
}

/// Transport-specific settings for one plugin MCP server.
/// 单个插件 MCP 服务器的传输专属设置。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum VulcanPluginMcpTransportManifest {
    /// Local stdio transport launched through a manifest-owned managed entry.
    /// 通过清单自有受管入口启动的本地 stdio 传输。
    Stdio {
        /// Managed launcher resolved without Shell or PATH lookup.
        /// 不经 Shell 或 PATH 查找解析的受管启动器。
        launcher: VulcanPluginMcpStdioLauncherManifest,
        /// Ordered launcher arguments resolved from explicit value sources.
        /// 从显式值来源解析的有序启动参数。
        #[serde(default)]
        args: Vec<VulcanPluginValueSourceManifest>,
        /// Child environment values keyed by exact variable name.
        /// 以精确变量名为键的子进程环境变量值。
        #[serde(default)]
        env: BTreeMap<String, VulcanPluginValueSourceManifest>,
    },
    /// Streamable HTTP transport connected to an explicit remote endpoint.
    /// 连接到显式远程端点的 Streamable HTTP 传输。
    Http {
        /// Remote endpoint resolved from one typed value source.
        /// 从唯一带类型值来源解析的远程端点。
        url: VulcanPluginValueSourceManifest,
        /// Request headers keyed by exact header name.
        /// 以精确 Header 名称为键的请求 Header。
        #[serde(default)]
        headers: BTreeMap<String, VulcanPluginValueSourceManifest>,
        /// Optional host-driven OAuth client settings.
        /// 可选的宿主驱动 OAuth 客户端设置。
        #[serde(default)]
        oauth: Option<VulcanPluginMcpOAuthManifest>,
    },
    /// Legacy Server-Sent Events transport connected to an explicit endpoint.
    /// 连接到显式端点的旧式服务器发送事件传输。
    Sse {
        /// Remote endpoint resolved from one typed value source.
        /// 从唯一带类型值来源解析的远程端点。
        url: VulcanPluginValueSourceManifest,
        /// Request headers keyed by exact header name.
        /// 以精确 Header 名称为键的请求 Header。
        #[serde(default)]
        headers: BTreeMap<String, VulcanPluginValueSourceManifest>,
        /// Optional host-driven OAuth client settings.
        /// 可选的宿主驱动 OAuth 客户端设置。
        #[serde(default)]
        oauth: Option<VulcanPluginMcpOAuthManifest>,
    },
}

/// Host-driven OAuth settings for one plugin-owned remote MCP server.
/// 单个插件自有远端 MCP 服务器的宿主驱动 OAuth 设置。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginMcpOAuthManifest {
    /// Optional pre-registered client identifier.
    /// 可选的预注册客户端标识。
    #[serde(default)]
    pub client_id: Option<VulcanPluginValueSourceManifest>,
    /// Optional pre-registered client secret from the host Secret Store.
    /// 可选的宿主 Secret Store 预注册客户端密钥。
    #[serde(default)]
    pub client_secret: Option<VulcanPluginValueSourceManifest>,
    /// Ordered OAuth scopes requested during authorization.
    /// 授权期间请求的有序 OAuth scope 集合。
    #[serde(default)]
    pub scopes: Vec<String>,
    /// Optional exact redirect URI.
    /// 可选的精确回调 URI。
    #[serde(default)]
    pub redirect_uri: Option<VulcanPluginValueSourceManifest>,
}

/// Managed local launcher variants accepted by plugin MCP stdio servers.
/// 插件 MCP stdio 服务器允许使用的受管本地启动器变体。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum VulcanPluginMcpStdioLauncherManifest {
    /// Execute a declared Node.js module through the managed Node runtime.
    /// 通过受管 Node 运行时执行已声明的 Node.js 模块。
    NodeEntry {
        /// Identifier from `execution.node_entries`.
        /// 来自 `execution.node_entries` 的标识。
        entry_id: String,
    },
    /// Execute one exact package bin from the plugin-private dependency environment.
    /// 从插件私有依赖环境执行一个精确的 package bin。
    NodePackageBin {
        /// Exact dependency package name.
        /// 精确依赖包名称。
        package: String,
        /// Exact bin key declared by the installed package.
        /// 已安装依赖包声明的精确 bin 键。
        bin: String,
    },
    /// Execute a declared native binary directly without a Shell.
    /// 不经 Shell 直接执行已声明的原生二进制文件。
    NativeBinary {
        /// Identifier from `execution.native_binaries`.
        /// 来自 `execution.native_binaries` 的标识。
        binary_id: String,
    },
}

/// One unambiguous source for a plugin configuration value.
/// 单个插件配置值的无歧义来源。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case", deny_unknown_fields)]
pub enum VulcanPluginValueSourceManifest {
    /// Inline non-secret value stored in the plugin manifest.
    /// 存储在插件清单中的内联非敏感值。
    Literal {
        /// Exact literal value.
        /// 精确字面值。
        value: String,
    },
    /// Reference to one key from the plugin configuration contract.
    /// 对插件配置契约中单个键的引用。
    Config {
        /// Exact plugin configuration key.
        /// 精确插件配置键。
        key: String,
    },
    /// Reference to one host-managed secret scoped to the plugin.
    /// 对限定到当前插件的单个宿主管理 Secret 的引用。
    Secret {
        /// Exact plugin-scoped secret key.
        /// 精确插件作用域 Secret 键。
        key: String,
    },
    /// Reference to one explicitly named host environment variable.
    /// 对单个显式命名宿主环境变量的引用。
    Environment {
        /// Exact environment variable name.
        /// 精确环境变量名称。
        name: String,
    },
}

/// One Lua handler bound to an existing host-owned hook point.
/// 绑定到现有宿主自有 hook 点位的单个 Lua 处理器。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginHookManifest {
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
pub enum VulcanPluginToolExecutionKind {
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
pub struct VulcanPluginToolExecutionManifest {
    /// Generic session lifecycle kind.
    /// 通用会话生命周期类型。
    pub kind: VulcanPluginToolExecutionKind,
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
pub struct VulcanPluginToolManifest {
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
    pub execution: VulcanPluginToolExecutionManifest,
}

/// One plugin-level model instruction contribution.
/// 单个插件级模型说明贡献。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginPromptManifest {
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
pub struct VulcanPluginLocaleManifest {
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
