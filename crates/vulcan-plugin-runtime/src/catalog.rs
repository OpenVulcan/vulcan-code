//! Secure package discovery and immutable Vulcan Plugin catalog snapshots.
//! 安全的插件包发现与不可变 Vulcan 插件目录快照。

mod generations;
pub use generations::VulcanPluginManagedPackage;
pub(crate) use generations::load_pinned_package;

use crate::{
    VULCAN_PLUGIN_HOOK_TOOL_AFTER, VULCAN_PLUGIN_HOOK_TOOL_BEFORE,
    VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION, VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS,
    VULCAN_PLUGIN_SESSION_ID_ARGUMENT, VulcanPluginDependencyInstallMode, VulcanPluginHookManifest,
    VulcanPluginLocaleManifest, VulcanPluginManifest, VulcanPluginMcpScopeManifest,
    VulcanPluginMcpServerManifest, VulcanPluginMcpStdioLauncherManifest,
    VulcanPluginMcpTransportManifest, VulcanPluginPermission, VulcanPluginPromptManifest,
    VulcanPluginToolExecutionKind, VulcanPluginToolManifest, VulcanPluginValueSourceManifest,
};
use luaskills::PackageDependencyManifest;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use vulcan_hooks::HookMode;
use vulcan_prompt_skills::{PromptSkill, load_plugin_prompt_skills};
use vulcan_task_session::{SubagentDefinition, load_plugin_subagent_catalog};

/// Fixed package manifest file name recognized by the host.
/// 宿主识别的固定插件包清单文件名。
pub const VULCAN_PLUGIN_MANIFEST_FILE: &str = "plugin.yaml";

/// Filesystem origin selected for one effective Vulcan plugin package.
/// 单个生效 Vulcan 插件包选定的文件系统来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginPackageOrigin {
    /// Plugin shipped below the active Vulcan Code root.
    /// 随当前 Vulcan Code 根目录交付的插件。
    Bundled,
    /// Plugin installed below the current user's Vulcan plugin root.
    /// 安装在当前用户 Vulcan 插件根目录下的插件。
    UserInstalled,
}

/// Effective package availability after platform and user policy projection.
/// 应用平台与用户策略投影后的插件包有效可用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginAvailability {
    /// Package is valid and enabled.
    /// 插件包有效且已启用。
    Enabled,
    /// Package is valid but disabled by default or user policy.
    /// 插件包有效，但被默认策略或用户策略停用。
    Disabled,
    /// Package is valid but explicitly unsupported on the current target.
    /// 插件包有效，但明确不支持当前目标平台。
    UnsupportedPlatform,
    /// Package discovery or validation failed.
    /// 插件包发现或校验失败。
    Unavailable,
}

/// Runtime lifecycle state projected independently from package availability.
/// 与插件包可用性独立投影的运行时生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginRuntimeState {
    /// Plugin is fully initialized and accepts new calls.
    /// 插件已完成初始化并接受新调用。
    Enabled,
    /// Plugin is valid but disabled.
    /// 插件有效但已停用。
    Disabled,
    /// Plugin enablement probe is in progress.
    /// 插件启用探测正在进行。
    Enabling,
    /// Plugin rejects new calls while existing sessions are being closed.
    /// 插件正在关闭既有会话，并拒绝新调用。
    Draining,
    /// Plugin is disabled but one or more cleanup operations failed.
    /// 插件已停用，但一个或多个清理操作失败。
    DisabledWithCleanupWarning,
    /// Plugin is not supported on the current target.
    /// 插件不支持当前目标平台。
    UnsupportedPlatform,
    /// Plugin validation failed or an original durable activation requires recovery before admission.
    /// 插件校验失败，或原始持久激活要求恢复后才能入场。
    Unavailable,
}

/// One validated model-visible tool definition loaded from a plugin package.
/// 从插件包加载并通过校验的单个模型可见工具定义。
#[derive(Debug, Clone)]
pub struct VulcanPluginToolDescriptor {
    /// Stable owning plugin identifier.
    /// 稳定的所属插件标识。
    pub plugin_id: String,
    /// Owning plugin generation captured with this tool definition.
    /// 随当前工具定义捕获的所属插件代次。
    pub plugin_generation: u64,
    /// Stable globally unique tool identifier.
    /// 全局唯一的稳定工具标识。
    pub id: String,
    /// Atomic group identifier.
    /// 原子工具组标识。
    pub group: String,
    /// English model-visible description.
    /// 面向模型的英文描述。
    pub description: String,
    /// Parsed JSON Schema for tool arguments.
    /// 已解析的工具参数 JSON Schema。
    pub parameters_schema: Value,
    /// Whether the tool may execute in Plan mode.
    /// 工具是否允许在计划模式执行。
    pub available_in_plan: bool,
    /// Source manifest retained for generic dispatch.
    /// 为通用调度保留的源清单。
    pub manifest: VulcanPluginToolManifest,
}

/// One validated Lua hook declaration loaded from a plugin package.
/// 从插件包加载并通过校验的单个 Lua hook 声明。
#[derive(Debug, Clone)]
pub struct VulcanPluginHookDescriptor {
    /// Stable owning plugin identifier.
    /// 稳定的所属插件标识。
    pub plugin_id: String,
    /// Owning plugin generation captured with this hook declaration.
    /// 随当前 hook 声明捕获的所属插件代次。
    pub plugin_generation: u64,
    /// Globally unique host registration identifier.
    /// 全局唯一的宿主注册标识。
    pub registration_id: String,
    /// Source manifest retained for host registration and Lua dispatch.
    /// 为宿主注册与 Lua 调度保留的源清单。
    pub manifest: VulcanPluginHookManifest,
}

/// One validated MCP server declaration loaded from a plugin package.
/// 从插件包加载并通过校验的单个 MCP 服务器声明。
#[derive(Debug, Clone)]
pub struct VulcanPluginMcpServerDescriptor {
    /// Stable owning plugin identifier.
    /// 稳定的所属插件标识。
    pub plugin_id: String,
    /// Owning plugin generation captured with this server declaration.
    /// 随当前服务器声明捕获的所属插件代次。
    pub plugin_generation: u64,
    /// Exact credential namespace captured with this declaration; old selections explicitly use legacy storage.
    /// 随此声明捕获的精确凭据命名空间；旧选择明确使用旧存储。
    pub credential_binding: crate::VulcanPluginCredentialBinding,
    /// Real retention follows standalone descriptor clones, preventing collection during later secret reads.
    /// 实际保留随独立描述符克隆传递，防止后续秘密读取期间被回收。
    pub(crate) credential_lease: Option<crate::PinnedVulcanPluginCredentials>,
    /// Version-four configuration follows this exact descriptor; absence denotes the legacy live-file authority.
    /// 第四版配置随此精确描述符保留；缺失表示旧实时文件权威。
    pub(crate) configuration: Option<std::sync::Arc<crate::VulcanPluginConfigurationSelection>>,
    /// Stable server identifier within the owning plugin.
    /// 所属插件内部稳定的服务器标识。
    pub id: String,
    /// Default user-facing server name.
    /// 默认的用户可见服务器名称。
    pub name: String,
    /// Source manifest retained for runtime materialization.
    /// 为运行时物化保留的源清单。
    pub manifest: VulcanPluginMcpServerManifest,
}

/// One validated plugin-level model instruction contribution.
/// 单个通过校验的插件级模型说明贡献。
#[derive(Debug, Clone)]
pub struct VulcanPluginPromptContribution {
    /// Stable owning plugin identifier.
    /// 稳定的所属插件标识。
    pub plugin_id: String,
    /// Stable contribution identifier within the plugin.
    /// 插件内部稳定的贡献标识。
    pub id: String,
    /// Markdown instruction body.
    /// Markdown 说明正文。
    pub body: String,
    /// Exact required tool identifiers.
    /// 精确的必需工具标识集合。
    pub required_tools: Vec<String>,
}

/// One discovered package projected for runtime and UI consumers.
/// 面向运行时与界面消费者投影的单个已发现插件包。
#[derive(Debug, Clone)]
pub struct VulcanPluginDescriptor {
    /// Stable package identifier, including invalid package directory fallback.
    /// 稳定插件包标识，包含无效包的目录名回退。
    pub id: String,
    /// Package semantic version when the manifest is valid.
    /// 清单有效时的插件包语义版本。
    pub version: Option<String>,
    /// Canonical package root.
    /// 规范化插件包根目录。
    pub package_root: PathBuf,
    /// Exact managed activation and its live package lease; absent for bundled and legacy packages.
    /// 精确受管激活及其活动包租约；内置包及旧平铺包没有此字段。
    pub(crate) managed_package: Option<VulcanPluginManagedPackage>,
    /// Effective package origin selected by deterministic root precedence.
    /// 通过确定性根目录优先级选定的生效插件包来源。
    pub origin: VulcanPluginPackageOrigin,
    /// Deterministic SHA-256 digest of every distributable package file.
    /// 插件全部可分发文件的确定性 SHA-256 摘要。
    pub content_fingerprint: String,
    /// Effective availability.
    /// 有效可用状态。
    pub availability: VulcanPluginAvailability,
    /// Desired enabled state after default and user policy projection.
    /// 应用默认值与用户策略投影后的期望启用状态。
    pub desired_enabled: bool,
    /// Current host lifecycle state.
    /// 当前宿主生命周期状态。
    pub runtime_state: VulcanPluginRuntimeState,
    /// Whether the optional execution runtime required by this package is attached.
    /// 当前插件包需要的可选执行运行时是否已经附加。
    pub runtime_execution_available: bool,
    /// Monotonic generation used to reject stale provider requests.
    /// 用于拒绝陈旧 Provider 请求的单调代次。
    pub generation: u64,
    /// Current owner-bound active session count.
    /// 当前绑定所有者的活动会话数量。
    pub active_session_count: usize,
    /// Default user-facing name.
    /// 默认用户可见名称。
    pub name: String,
    /// Default concise capability description.
    /// 默认简洁能力描述。
    pub capability: String,
    /// Localized user-facing metadata keyed by locale.
    /// 以语言为键的本地化用户可见元数据。
    pub locales: BTreeMap<String, VulcanPluginLocaleManifest>,
    /// Validated atomic tool definitions.
    /// 通过校验的原子工具定义集合。
    pub tools: Vec<VulcanPluginToolDescriptor>,
    /// Validated Lua hook declarations.
    /// 通过校验的 Lua hook 声明集合。
    pub hooks: Vec<VulcanPluginHookDescriptor>,
    /// Validated plugin-owned MCP server declarations.
    /// 通过校验的插件自有 MCP 服务器声明集合。
    pub mcp_servers: Vec<VulcanPluginMcpServerDescriptor>,
    /// Validated prompt contributions.
    /// 通过校验的提示词贡献集合。
    pub prompt_contributions: Vec<VulcanPluginPromptContribution>,
    /// Validated read-only Agent Skills contributed by this plugin.
    /// 当前插件贡献并通过校验的只读 Agent Skills 集合。
    pub skills: Vec<PromptSkill>,
    /// Validated read-only subagent definitions contributed by this plugin.
    /// 当前插件贡献并通过校验的只读子代理定义集合。
    pub agents: Vec<SubagentDefinition>,
    /// Valid manifest used by the generic runtime.
    /// 通用运行时使用的有效清单。
    pub manifest: Option<Arc<VulcanPluginManifest>>,
    /// Deterministic validation diagnostics.
    /// 确定性的校验诊断集合。
    pub diagnostics: Vec<String>,
}

impl VulcanPluginDescriptor {
    /// Returns the managed selection retained by this descriptor, or `None` for legacy/bundled packages.
    /// 返回此描述符保留的受管选择；旧平铺包及内置包返回 `None`。
    /// Borrow or clone its generation handle while consuming files beyond this descriptor's lifetime.
    /// 在此描述符寿命之外使用包文件时，必须借用或克隆其中的代次句柄。
    pub fn managed_package(&self) -> Option<&VulcanPluginManagedPackage> {
        self.managed_package.as_ref()
    }

    /// Returns localized metadata or the manifest defaults when no exact locale exists.
    /// 返回本地化元数据；不存在精确语言时回退到清单默认值。
    ///
    /// # Parameters
    /// 参数
    /// `locale` is the requested UI locale key.
    /// `locale` 是请求的界面语言键。
    ///
    /// # Returns
    /// 返回值
    /// Returns the effective display name and capability text.
    /// 返回生效的显示名称与能力文本。
    pub fn localized_display(&self, locale: &str) -> (&str, &str) {
        self.locales
            .get(locale)
            .map(|localized| (localized.name.as_str(), localized.capability.as_str()))
            .unwrap_or((self.name.as_str(), self.capability.as_str()))
    }
}

/// Immutable catalog revision consumed atomically by runtime preparation.
/// 由运行时准备过程原子消费的不可变目录修订版。
#[derive(Debug, Clone)]
pub struct VulcanPluginCatalogSnapshot {
    /// Service-owned runtime incarnation, attached when the manager projects this catalog.
    /// 由管理器投影目录时附加的服务运行实例身份。
    pub service_epoch: String,
    /// Monotonic in-process revision assigned by the manager.
    /// 由管理器分配的进程内单调修订号。
    pub revision: u64,
    /// Canonical Vulcan Plugin root scanned for this snapshot.
    /// 当前快照扫描的规范化 Vulcan 插件根目录。
    pub root: PathBuf,
    /// Optional canonical user plugin root merged below the system root.
    /// 合并在系统根之后的可选规范化用户插件根目录。
    pub user_root: Option<PathBuf>,
    /// Discovered plugins keyed by stable identifier.
    /// 以稳定标识为键的已发现插件集合。
    pub plugins: BTreeMap<String, VulcanPluginDescriptor>,
    /// Enabled tools keyed by globally unique tool identifier.
    /// 以全局唯一工具标识为键的已启用工具集合。
    pub enabled_tools: BTreeMap<String, VulcanPluginToolDescriptor>,
    /// Enabled hook declarations keyed by globally unique registration identifier.
    /// 以全局唯一注册标识为键的已启用 hook 声明集合。
    pub enabled_hooks: BTreeMap<String, VulcanPluginHookDescriptor>,
    /// Enabled plugin MCP servers keyed by their canonical namespaced identifiers.
    /// 以规范命名空间标识为键的已启用插件 MCP 服务器集合。
    pub enabled_mcp_servers: BTreeMap<String, VulcanPluginMcpServerDescriptor>,
    /// Enabled plugin-level prompt contributions.
    /// 已启用的插件级提示词贡献集合。
    pub prompt_contributions: Vec<VulcanPluginPromptContribution>,
    /// Enabled plugin Agent Skills keyed by canonical namespaced identifier.
    /// 以规范命名空间标识索引的已启用插件 Agent Skills。
    pub enabled_skills: BTreeMap<String, PromptSkill>,
    /// Enabled plugin subagents keyed by canonical namespaced identifier.
    /// 以规范命名空间标识索引的已启用插件子代理。
    pub enabled_agents: BTreeMap<String, SubagentDefinition>,
    /// Catalog-level deterministic diagnostics.
    /// 目录级确定性诊断集合。
    pub diagnostics: Vec<String>,
}

impl VulcanPluginCatalogSnapshot {
    /// Builds one empty snapshot before first discovery succeeds.
    /// 在首次发现成功前构建一个空快照。
    ///
    /// # Parameters
    /// 参数
    /// `root` is the trusted Vulcan Plugin catalog root.
    /// `root` 是受信任的 Vulcan 插件目录根。
    ///
    /// # Returns
    /// 返回值
    /// Returns an empty catalog snapshot with revision zero.
    /// 返回修订号为零的空目录快照。
    pub fn empty(root: PathBuf) -> Self {
        Self {
            service_epoch: String::new(),
            revision: 0,
            root,
            user_root: None,
            plugins: BTreeMap::new(),
            enabled_tools: BTreeMap::new(),
            enabled_hooks: BTreeMap::new(),
            enabled_mcp_servers: BTreeMap::new(),
            prompt_contributions: Vec::new(),
            enabled_skills: BTreeMap::new(),
            enabled_agents: BTreeMap::new(),
            diagnostics: Vec::new(),
        }
    }
}

/// Discovers and validates every direct child package under one trusted Vulcan Plugin root.
/// 在一个可信 Vulcan 插件根目录下发现并校验所有直接子插件包。
///
/// # Parameters
/// 参数
/// `root` is the catalog root, policy sets contain explicit overrides, and `revision` labels the result.
/// `root` 是目录根，策略集合保存显式覆盖，`revision` 标记生成结果。
///
/// # Returns
/// 返回值
/// Returns discovered packages and diagnostics; global contribution surfaces are built after final selection.
/// 返回已发现插件及诊断；全局贡献表面在最终选择完成后构建。
fn discover_plugin_root_catalog(
    root: &Path,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
    revision: u64,
) -> VulcanPluginCatalogSnapshot {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut snapshot = VulcanPluginCatalogSnapshot::empty(canonical_root.clone());
    snapshot.revision = revision;
    let entries = match fs::read_dir(&canonical_root) {
        Ok(entries) => entries,
        Err(error) => {
            snapshot.diagnostics.push(format!(
                "Failed to read Vulcan Plugin root `{}`: {error}",
                canonical_root.display()
            ));
            return snapshot;
        }
    };
    let mut package_roots = Vec::new();
    for entry_result in entries {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                snapshot.diagnostics.push(format!(
                    "Failed to enumerate a Vulcan Plugin root entry: {error}"
                ));
                continue;
            }
        };
        let path = entry.path();
        // Private generation storage is discovered through its activation records, never as a package.
        // 私有代次存储通过激活记录发现，绝不作为插件包发现。
        if crate::VulcanPluginGenerationStore::is_store_entry(&path) {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                snapshot.diagnostics.push(format!(
                    "Failed to inspect Vulcan Plugin root entry `{}`: {error}",
                    path.display()
                ));
                continue;
            }
        };
        // Package-root links are passed to the package validator so their rejection remains observable.
        // 插件包根链接会交给包校验器处理，确保拒绝原因可观察。
        if crate::metadata_is_link_or_reparse_point(&metadata) {
            package_roots.push(path);
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        let manifest_path = path.join(VULCAN_PLUGIN_MANIFEST_FILE);
        match fs::metadata(&manifest_path) {
            Ok(metadata) if metadata.is_file() => package_roots.push(path),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => snapshot.diagnostics.push(format!(
                "Failed to inspect Vulcan Plugin manifest `{}`: {error}",
                manifest_path.display()
            )),
        }
    }
    package_roots.sort();
    for package_root in package_roots {
        let descriptor = load_vulcan_plugin_package(
            &canonical_root,
            &package_root,
            disabled_plugin_ids,
            enabled_plugin_ids,
        );
        let plugin_id = descriptor.id.clone();
        if snapshot.plugins.contains_key(&plugin_id) {
            snapshot.diagnostics.push(format!(
                "Duplicate Vulcan Plugin id `{plugin_id}` was ignored."
            ));
            continue;
        }
        snapshot.plugins.insert(plugin_id, descriptor);
    }
    // Global names are reserved only after managed selection and bundled precedence are resolved.
    // 仅在受管选择及内置优先级确定后才保留全局名称。
    snapshot
}

/// Discovers bundled and user plugin roots with deterministic bundled-package precedence.
/// 使用确定性的内建包优先级发现内建与用户插件根目录。
///
/// `bundled_root` is the trusted distribution root, `user_root` is the install root owned by the
/// current user, policy sets contain explicit enablement overrides, and `revision` labels the
/// immutable result.
/// `bundled_root` 是受信任发行根，`user_root` 是当前用户拥有的安装根，策略集合保存显式启停覆盖，
/// `revision` 标记不可变结果。
///
/// Returns one merged catalog where a bundled package shadows the same user plugin id.
/// 返回一份合并目录；内建 Vulcan 插件会遮蔽同 ID 的用户插件。
pub fn load_vulcan_plugin_catalog(
    bundled_root: &Path,
    user_root: Option<&Path>,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
    revision: u64,
) -> VulcanPluginCatalogSnapshot {
    let mut bundled = discover_plugin_root_catalog(
        bundled_root,
        disabled_plugin_ids,
        enabled_plugin_ids,
        revision,
    );
    for plugin in bundled.plugins.values_mut() {
        plugin.origin = VulcanPluginPackageOrigin::Bundled;
    }
    let mut user = user_root.map_or_else(
        || {
            let mut snapshot = VulcanPluginCatalogSnapshot::empty(bundled.root.clone());
            snapshot.revision = revision;
            snapshot
        },
        |user_root| {
            generations::load_user_catalog(
                user_root,
                disabled_plugin_ids,
                enabled_plugin_ids,
                revision,
            )
        },
    );
    for plugin in user.plugins.values_mut() {
        plugin.origin = VulcanPluginPackageOrigin::UserInstalled;
    }
    bundled.user_root = user_root.map(|user_root| {
        user_root
            .canonicalize()
            .unwrap_or_else(|_| user_root.to_path_buf())
    });
    bundled.diagnostics.extend(user.diagnostics);
    for (plugin_id, descriptor) in user.plugins {
        if bundled.plugins.contains_key(&plugin_id) {
            bundled.diagnostics.push(format!(
                "User plugin `{plugin_id}` is shadowed by the bundled Vulcan plugin with the same id."
            ));
            continue;
        }
        bundled.plugins.insert(plugin_id, descriptor);
    }
    rebuild_catalog_surface(&mut bundled);
    bundled
}

/// Loads one standalone package for local installation inspection.
/// 加载一个独立插件包用于本地安装检查。
///
/// # Parameters
/// 参数
/// `package_root` must be a direct package directory whose name matches its manifest identity.
/// `package_root` 必须是名称与清单身份一致的直接插件包目录。
///
/// # Returns
/// 返回值
/// Returns one fully validated user-installed descriptor.
/// 返回一个完整校验的用户安装插件描述符。
pub fn inspect_user_plugin_package(package_root: &Path) -> Result<VulcanPluginDescriptor, String> {
    let canonical_package_root = package_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve local plugin package: {error}"))?;
    if !canonical_package_root.is_dir() {
        return Err(String::from("Local plugin package is not a directory."));
    }
    let catalog_root = canonical_package_root
        .parent()
        .ok_or_else(|| String::from("Local plugin package has no parent directory."))?;
    let empty = BTreeSet::new();
    let mut descriptor =
        load_vulcan_plugin_package(catalog_root, &canonical_package_root, &empty, &empty);
    descriptor.origin = VulcanPluginPackageOrigin::UserInstalled;
    if !matches!(
        descriptor.availability,
        VulcanPluginAvailability::Enabled | VulcanPluginAvailability::Disabled
    ) {
        return Err(descriptor.diagnostics.join("; "));
    }
    Ok(descriptor)
}

/// Rebuilds enabled provider, hook, and prompt surfaces after catalog merging.
/// 在目录合并后重建已启用的 Provider、Hook 与提示词表面。
///
/// `snapshot` is the exact merged catalog whose derived surfaces are replaced atomically.
/// `snapshot` 是需要原子替换派生表面的精确合并目录。
///
/// Returns no value.
/// 不返回值。
fn rebuild_catalog_surface(snapshot: &mut VulcanPluginCatalogSnapshot) {
    snapshot.enabled_tools.clear();
    snapshot.enabled_hooks.clear();
    snapshot.enabled_mcp_servers.clear();
    snapshot.prompt_contributions.clear();
    snapshot.enabled_skills.clear();
    snapshot.enabled_agents.clear();
    let mut reserved_tool_ids = BTreeSet::new();
    for descriptor in snapshot.plugins.values_mut() {
        if descriptor.availability != VulcanPluginAvailability::Enabled {
            continue;
        }
        if let Some(tool_id) = descriptor
            .tools
            .iter()
            .find(|tool| reserved_tool_ids.contains(&tool.id))
            .map(|tool| tool.id.clone())
        {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(format!(
                "Tool id `{tool_id}` conflicts with another enabled Vulcan plugin."
            ));
            continue;
        }
        reserved_tool_ids.extend(descriptor.tools.iter().map(|tool| tool.id.clone()));
        for tool in &descriptor.tools {
            snapshot.enabled_tools.insert(tool.id.clone(), tool.clone());
        }
        for hook in &descriptor.hooks {
            snapshot
                .enabled_hooks
                .insert(hook.registration_id.clone(), hook.clone());
        }
        for server in &descriptor.mcp_servers {
            snapshot.enabled_mcp_servers.insert(
                format!("{}:{}", server.plugin_id, server.id),
                server.clone(),
            );
        }
        snapshot
            .prompt_contributions
            .extend(descriptor.prompt_contributions.iter().cloned());
        for skill in &descriptor.skills {
            snapshot
                .enabled_skills
                .insert(skill.name.clone(), skill.clone());
        }
        for agent in &descriptor.agents {
            snapshot
                .enabled_agents
                .insert(agent.id.clone(), agent.clone());
        }
    }
}

/// Loads and validates one exact package without allowing path escape.
/// 加载并校验一个精确插件包，同时禁止路径逃逸。
fn load_vulcan_plugin_package(
    catalog_root: &Path,
    package_root: &Path,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
) -> VulcanPluginDescriptor {
    let canonical_package_root = package_root
        .canonicalize()
        .unwrap_or_else(|_| package_root.to_path_buf());
    let fallback_id = package_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("invalid-vulcan-plugin")
        .to_owned();
    let unavailable = |diagnostic: String| VulcanPluginDescriptor {
        id: fallback_id.clone(),
        version: None,
        package_root: canonical_package_root.clone(),
        managed_package: None,
        origin: VulcanPluginPackageOrigin::Bundled,
        content_fingerprint: String::new(),
        availability: VulcanPluginAvailability::Unavailable,
        desired_enabled: false,
        runtime_state: VulcanPluginRuntimeState::Unavailable,
        runtime_execution_available: false,
        generation: 0,
        active_session_count: 0,
        name: fallback_id.clone(),
        capability: String::from("Vulcan Plugin package is unavailable."),
        locales: BTreeMap::new(),
        tools: Vec::new(),
        hooks: Vec::new(),
        mcp_servers: Vec::new(),
        prompt_contributions: Vec::new(),
        skills: Vec::new(),
        agents: Vec::new(),
        manifest: None,
        diagnostics: vec![diagnostic],
    };
    // Inspect the literal catalog entry before using its canonical target so package-root links cannot inherit trust.
    // 在使用规范化目标前检查目录中的字面条目，避免插件包根链接继承信任。
    let package_root_metadata = match fs::symlink_metadata(package_root) {
        Ok(metadata) => metadata,
        Err(error) => {
            return unavailable(format!(
                "Failed to inspect Vulcan Plugin package root: {error}"
            ));
        }
    };
    if crate::metadata_is_link_or_reparse_point(&package_root_metadata)
        || !package_root_metadata.is_dir()
    {
        return unavailable(String::from(
            "Vulcan Plugin package root must be a regular directory and must not be a symbolic link or reparse point.",
        ));
    }
    if !canonical_package_root.starts_with(catalog_root) {
        return unavailable(String::from(
            "Vulcan Plugin package root escapes the trusted catalog root.",
        ));
    }
    let manifest_path = canonical_package_root.join(VULCAN_PLUGIN_MANIFEST_FILE);
    let manifest_text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) => return unavailable(format!("Failed to read plugin manifest: {error}")),
    };
    let manifest: VulcanPluginManifest = match serde_yaml::from_str(&manifest_text) {
        Ok(manifest) => manifest,
        Err(error) => return unavailable(format!("Failed to decode plugin manifest: {error}")),
    };
    if let Err(error) = validate_manifest_identity(&manifest, &fallback_id) {
        return unavailable(error);
    }
    let mut descriptor = VulcanPluginDescriptor {
        id: manifest.id.clone(),
        version: Some(manifest.version.clone()),
        package_root: canonical_package_root.clone(),
        managed_package: None,
        origin: VulcanPluginPackageOrigin::Bundled,
        content_fingerprint: String::new(),
        availability: VulcanPluginAvailability::Enabled,
        desired_enabled: if enabled_plugin_ids.contains(&manifest.id) {
            true
        } else if disabled_plugin_ids.contains(&manifest.id) {
            false
        } else {
            manifest.default_enabled
        },
        runtime_state: VulcanPluginRuntimeState::Enabled,
        runtime_execution_available: true,
        generation: 0,
        active_session_count: 0,
        name: manifest.display.name.clone(),
        capability: manifest.display.capability.clone(),
        locales: BTreeMap::new(),
        tools: Vec::new(),
        hooks: Vec::new(),
        mcp_servers: Vec::new(),
        prompt_contributions: Vec::new(),
        skills: Vec::new(),
        agents: Vec::new(),
        manifest: Some(Arc::new(manifest.clone())),
        diagnostics: Vec::new(),
    };
    if current_platform_is_excluded(&manifest.platforms.excluded_targets) {
        descriptor.availability = VulcanPluginAvailability::UnsupportedPlatform;
        descriptor.runtime_state = VulcanPluginRuntimeState::UnsupportedPlatform;
        descriptor.diagnostics.push(String::from(
            "Vulcan Plugin is not supported on the current target platform.",
        ));
        return descriptor;
    }
    if !descriptor.desired_enabled {
        descriptor.availability = VulcanPluginAvailability::Disabled;
        descriptor.runtime_state = VulcanPluginRuntimeState::Disabled;
    }
    if let Err(error) = validate_runtime_files(&canonical_package_root, &manifest) {
        descriptor.availability = VulcanPluginAvailability::Unavailable;
        descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
        descriptor.diagnostics.push(error);
        return descriptor;
    }
    descriptor.content_fingerprint = match compute_package_fingerprint(&canonical_package_root) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(error);
            return descriptor;
        }
    };
    descriptor.skills = match load_plugin_prompt_skills(
        &manifest.id,
        &manifest.display.name,
        &manifest.version,
        &canonical_package_root.join("skills"),
    ) {
        Ok(skills) => skills,
        Err(errors) => {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor
                .diagnostics
                .extend(errors.into_iter().map(|error| {
                    format!(
                        "Plugin skill `{}` is invalid: {}",
                        error.path.display(),
                        error.message
                    )
                }));
            return descriptor;
        }
    };
    let agent_catalog =
        load_plugin_subagent_catalog(&manifest.id, canonical_package_root.join("agents"));
    if !agent_catalog.errors.is_empty() {
        descriptor.availability = VulcanPluginAvailability::Unavailable;
        descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
        descriptor
            .diagnostics
            .extend(agent_catalog.errors.into_iter().map(|error| {
                format!(
                    "Plugin agent `{}` is invalid: {}",
                    error.path.display(),
                    error.message
                )
            }));
        return descriptor;
    }
    descriptor.agents = agent_catalog.definitions;
    let plugin_skill_ids = descriptor
        .skills
        .iter()
        .map(|skill| skill.name.as_str())
        .collect::<BTreeSet<_>>();
    for agent in &descriptor.agents {
        if let Some(missing_skill_id) = agent
            .prompt_skills
            .policy
            .selectors
            .iter()
            .find(|skill_id| !plugin_skill_ids.contains(skill_id.as_str()))
        {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(format!(
                "Plugin agent `{}` references unavailable same-plugin skill `{missing_skill_id}`.",
                agent.id
            ));
            return descriptor;
        }
    }
    if !manifest.has_execution_declarations()
        && descriptor.skills.is_empty()
        && descriptor.agents.is_empty()
    {
        descriptor.availability = VulcanPluginAvailability::Unavailable;
        descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
        descriptor.diagnostics.push(String::from(
            "A plugin must contribute at least one executable or content component.",
        ));
        return descriptor;
    }
    let mut tool_ids = BTreeSet::new();
    let mut groups = BTreeSet::new();
    let lua_tools = manifest
        .lua_runtime
        .as_ref()
        .map(|runtime| runtime.tools.as_slice())
        .unwrap_or_default();
    for tool in lua_tools {
        if !tool_ids.insert(tool.id.clone()) {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(format!(
                "Duplicate tool id `{}` in plugin manifest.",
                tool.id
            ));
            return descriptor;
        }
        groups.insert(tool.group.clone());
        let parameters_path = match resolve_package_file(
            &canonical_package_root,
            &tool.parameters_file,
            "tool parameters schema",
        ) {
            Ok(path) => path,
            Err(error) => {
                descriptor.availability = VulcanPluginAvailability::Unavailable;
                descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(error);
                return descriptor;
            }
        };
        let parameters_schema = match read_json_file(&parameters_path) {
            Ok(schema) if schema.is_object() => schema,
            Ok(_) => {
                descriptor.availability = VulcanPluginAvailability::Unavailable;
                descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(format!(
                    "Tool `{}` parameters schema must be a JSON object.",
                    tool.id
                ));
                return descriptor;
            }
            Err(error) => {
                descriptor.availability = VulcanPluginAvailability::Unavailable;
                descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(error);
                return descriptor;
            }
        };
        if let Err(error) = validate_tool_parameters_schema(tool, &parameters_schema) {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(error);
            return descriptor;
        }
        descriptor.tools.push(VulcanPluginToolDescriptor {
            plugin_id: manifest.id.clone(),
            plugin_generation: 0,
            id: tool.id.clone(),
            group: tool.group.clone(),
            description: tool.description.clone(),
            parameters_schema,
            available_in_plan: tool.available_in_plan,
            manifest: tool.clone(),
        });
    }
    if !descriptor.tools.is_empty() && groups.len() != 1 {
        descriptor.availability = VulcanPluginAvailability::Unavailable;
        descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
        descriptor.diagnostics.push(String::from(
            "A bundled Vulcan Plugin with tools must declare exactly one atomic tool group.",
        ));
        return descriptor;
    }
    if let Err(error) = validate_tool_execution_topology(lua_tools) {
        descriptor.availability = VulcanPluginAvailability::Unavailable;
        descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
        descriptor.diagnostics.push(error);
        return descriptor;
    }
    descriptor.mcp_servers = manifest
        .mcp
        .servers
        .iter()
        .map(|server| VulcanPluginMcpServerDescriptor {
            plugin_id: manifest.id.clone(),
            plugin_generation: 0,
            credential_binding: crate::VulcanPluginCredentialBinding::Legacy,
            credential_lease: None,
            configuration: None,
            id: server.id.clone(),
            name: server.name.clone(),
            manifest: server.clone(),
        })
        .collect();
    let mut hook_ids = BTreeSet::new();
    let lua_hooks = manifest
        .lua_runtime
        .as_ref()
        .map(|runtime| runtime.hooks.as_slice())
        .unwrap_or_default();
    for hook in lua_hooks {
        if !hook_ids.insert(hook.id.clone()) {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(format!(
                "Duplicate hook id `{}` in plugin manifest.",
                hook.id
            ));
            return descriptor;
        }
        if let Err(error) = validate_hook_manifest(hook) {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(error);
            return descriptor;
        }
        descriptor.hooks.push(VulcanPluginHookDescriptor {
            plugin_id: manifest.id.clone(),
            plugin_generation: 0,
            registration_id: format!("vulcan-plugin/{}/{}", manifest.id, hook.id),
            manifest: hook.clone(),
        });
    }
    let mut prompt_ids = BTreeSet::new();
    for prompt in &manifest.prompt_contributions {
        if !prompt_ids.insert(prompt.id.clone()) {
            descriptor.availability = VulcanPluginAvailability::Unavailable;
            descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(format!(
                "Duplicate prompt contribution id `{}` in plugin manifest.",
                prompt.id
            ));
            return descriptor;
        }
        match load_prompt_contribution(&canonical_package_root, &manifest.id, prompt, &tool_ids) {
            Ok(contribution) => descriptor.prompt_contributions.push(contribution),
            Err(error) => {
                descriptor.availability = VulcanPluginAvailability::Unavailable;
                descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(error);
                return descriptor;
            }
        }
    }
    for (locale, relative_path) in &manifest.locales {
        let locale_path = match resolve_package_file(
            &canonical_package_root,
            relative_path,
            "plugin locale file",
        ) {
            Ok(path) => path,
            Err(error) => {
                descriptor.availability = VulcanPluginAvailability::Unavailable;
                descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(error);
                return descriptor;
            }
        };
        let locale_text = match fs::read_to_string(&locale_path) {
            Ok(text) => text,
            Err(error) => {
                descriptor.availability = VulcanPluginAvailability::Unavailable;
                descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
                descriptor
                    .diagnostics
                    .push(format!("Failed to read plugin locale `{locale}`: {error}"));
                return descriptor;
            }
        };
        let localized = match serde_yaml::from_str::<VulcanPluginLocaleManifest>(&locale_text) {
            Ok(localized) => localized,
            Err(error) => {
                descriptor.availability = VulcanPluginAvailability::Unavailable;
                descriptor.runtime_state = VulcanPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(format!(
                    "Failed to decode plugin locale `{locale}`: {error}"
                ));
                return descriptor;
            }
        };
        descriptor.locales.insert(locale.clone(), localized);
    }
    descriptor
}

/// Validates version, identity, host-owned package origin, and identifier grammar.
/// 校验版本、身份、宿主持有的插件包来源与标识符语法。
pub(crate) fn validate_manifest_identity(
    manifest: &VulcanPluginManifest,
    package_directory_id: &str,
) -> Result<(), String> {
    manifest.validate_runtime_requirements()?;
    validate_identifier(&manifest.id, "plugin id")?;
    if manifest.id != package_directory_id {
        return Err(format!(
            "Plugin id `{}` does not match package directory `{package_directory_id}`.",
            manifest.id
        ));
    }
    if manifest.version.trim().is_empty() {
        return Err(String::from("Plugin version must not be empty."));
    }
    let unique_excluded_targets = manifest
        .platforms
        .excluded_targets
        .iter()
        .collect::<BTreeSet<_>>();
    if unique_excluded_targets.len() != manifest.platforms.excluded_targets.len() {
        return Err(String::from(
            "Plugin excluded_targets contains duplicate target triples.",
        ));
    }
    for target in &manifest.platforms.excluded_targets {
        validate_target_triple(target)?;
    }
    let Some(runtime) = manifest.lua_runtime.as_ref() else {
        return validate_non_lua_manifest(manifest);
    };
    validate_non_lua_manifest(manifest)?;
    if runtime.max_active_sessions == 0 {
        return Err(String::from(
            "Plugin max_active_sessions must be greater than zero.",
        ));
    }
    if runtime.session_idle_timeout_sec == Some(0) {
        return Err(String::from(
            "Plugin session_idle_timeout_sec must be greater than zero when declared.",
        ));
    }
    validate_lua_identifier(&runtime.invoke_function, "runtime invoke function")?;
    validate_lua_identifier(&runtime.shutdown_function, "runtime shutdown function")?;
    if let Some(function) = &runtime.hook_shutdown_function {
        validate_lua_identifier(function, "runtime hook shutdown function")?;
    }
    let mut binary_ids = BTreeSet::new();
    for binary in &manifest.execution.native_binaries {
        validate_identifier(&binary.id, "binary id")?;
        if !binary_ids.insert(binary.id.as_str()) {
            return Err(format!("Duplicate plugin binary id `{}`.", binary.id));
        }
        if binary.targets.is_empty() {
            return Err(format!(
                "Plugin binary `{}` must declare at least one target path.",
                binary.id
            ));
        }
        for target in binary.targets.keys() {
            validate_target_triple(target)?;
        }
    }
    let mut node_entry_ids = BTreeSet::new();
    for entry in &manifest.execution.node_entries {
        validate_identifier(&entry.id, "Node entry id")?;
        if !node_entry_ids.insert(entry.id.as_str()) {
            return Err(format!("Duplicate plugin Node entry id `{}`.", entry.id));
        }
    }
    let mut python_entry_ids = BTreeSet::new();
    for entry in &manifest.execution.python_entries {
        validate_identifier(&entry.id, "Python entry id")?;
        if !python_entry_ids.insert(entry.id.as_str()) {
            return Err(format!("Duplicate plugin Python entry id `{}`.", entry.id));
        }
    }
    let mut process_entry_ids = binary_ids.clone();
    for entry_id in node_entry_ids.iter().chain(python_entry_ids.iter()) {
        if !process_entry_ids.insert(entry_id) {
            return Err(format!(
                "Duplicate plugin process entry id `{entry_id}` across entry kinds."
            ));
        }
    }
    let unique_capabilities = manifest.permissions.iter().collect::<BTreeSet<_>>();
    if unique_capabilities.len() != manifest.permissions.len() {
        return Err(String::from(
            "Plugin capabilities contains duplicate values.",
        ));
    }
    if manifest.execution.dependencies.install_mode == VulcanPluginDependencyInstallMode::Registry
        && !unique_capabilities.contains(&VulcanPluginPermission::DependencyRegistryNetwork)
    {
        return Err(String::from(
            "Registry dependency installation requires dependency_registry_network capability.",
        ));
    }
    // uniqueLuaModules rejects ambiguous duplicate access declarations before lease creation.
    // uniqueLuaModules 在创建租约前拒绝含义不明确的重复访问声明。
    let unique_lua_modules = runtime.lua_modules.iter().collect::<BTreeSet<_>>();
    if unique_lua_modules.len() != runtime.lua_modules.len() {
        return Err(String::from(
            "Plugin runtime lua_modules contains duplicate module names.",
        ));
    }
    if !runtime.lua_modules.is_empty()
        && !unique_capabilities.contains(&VulcanPluginPermission::RuntimeLuaModules)
    {
        return Err(String::from(
            "Plugin runtime lua_modules require capability `runtime_lua_modules`.",
        ));
    }
    for module_name in &runtime.lua_modules {
        if !matches!(
            module_name.as_str(),
            "lcurl.safe" | "openssl.cipher" | "openssl.rand"
        ) {
            return Err(format!(
                "Plugin runtime Lua module `{module_name}` is not allowed by the host."
            ));
        }
    }
    if runtime
        .lua_modules
        .iter()
        .any(|module_name| module_name == "lcurl.safe")
        && !unique_capabilities.contains(&VulcanPluginPermission::NetworkHttps)
    {
        return Err(String::from(
            "Plugin runtime module `lcurl.safe` requires capability `network_https`.",
        ));
    }
    for (has_resources, capability, label) in [
        (
            !manifest.execution.native_binaries.is_empty(),
            VulcanPluginPermission::ProcessBinary,
            "native binary entries",
        ),
        (
            !manifest.execution.node_entries.is_empty(),
            VulcanPluginPermission::ProcessNode,
            "Node entries",
        ),
        (
            !manifest.execution.python_entries.is_empty(),
            VulcanPluginPermission::ProcessPython,
            "Python entries",
        ),
        (
            manifest.config.is_some(),
            VulcanPluginPermission::Config,
            "configuration",
        ),
        (
            !manifest.ui.mounts.is_empty(),
            VulcanPluginPermission::UiHostContext,
            "dynamic UI mounts",
        ),
    ] {
        if has_resources && !unique_capabilities.contains(&capability) {
            return Err(format!(
                "Plugin {label} require capability `{}`.",
                crate::runtime::plugin_permission_key(capability)
            ));
        }
    }
    for (label, function) in [
        (
            "lifecycle install function",
            runtime.lifecycle.install.as_ref(),
        ),
        (
            "lifecycle enable function",
            runtime.lifecycle.enable.as_ref(),
        ),
        (
            "lifecycle disable function",
            runtime.lifecycle.disable.as_ref(),
        ),
        (
            "lifecycle uninstall function",
            runtime.lifecycle.uninstall.as_ref(),
        ),
    ] {
        if let Some(function) = function {
            validate_lua_identifier(function, label)?;
        }
    }
    if runtime.lifecycle.install.is_some() && runtime.lifecycle.uninstall.is_none() {
        return Err(String::from(
            "Plugin lifecycle install requires an uninstall compensation callback.",
        ));
    }
    if runtime.lifecycle.enable.is_some() && runtime.lifecycle.disable.is_none() {
        return Err(String::from(
            "Plugin lifecycle enable requires a disable compensation callback.",
        ));
    }
    let mut action_ids = BTreeSet::new();
    for action in &runtime.actions {
        validate_identifier(&action.id, "action id")?;
        validate_lua_identifier(&action.function, "action function")?;
        if !action_ids.insert(action.id.as_str()) {
            return Err(format!("Duplicate plugin action id `{}`.", action.id));
        }
        if action.timeout_ms == 0 {
            return Err(format!(
                "Plugin action `{}` timeout_ms must be greater than zero.",
                action.id
            ));
        }
        if action.result_cache_ms > 0 && action.effect != crate::VulcanPluginActionEffect::Read {
            return Err(format!(
                "Plugin action `{}` may cache results only when explicitly declared read-only.",
                action.id
            ));
        }
        if !action.snapshot_time_arguments.is_empty()
            && action.effect != crate::VulcanPluginActionEffect::Read
        {
            return Err(format!(
                "Plugin action `{}` may snapshot time only for read queries.",
                action.id
            ));
        }
        if action
            .snapshot_time_arguments
            .iter()
            .collect::<BTreeSet<_>>()
            .len()
            != action.snapshot_time_arguments.len()
        {
            return Err(format!(
                "Plugin action `{}` repeats a snapshot time argument.",
                action.id
            ));
        }
        for argument in &action.snapshot_time_arguments {
            validate_lua_identifier(argument, "snapshot time argument")?;
        }
        if action.callers.is_empty() {
            return Err(format!(
                "Plugin action `{}` must declare at least one caller.",
                action.id
            ));
        }
        if action.callers.iter().collect::<BTreeSet<_>>().len() != action.callers.len() {
            return Err(format!(
                "Plugin action `{}` callers contains duplicate values.",
                action.id
            ));
        }
    }
    let mut mount_ids = BTreeSet::new();
    for mount in &manifest.ui.mounts {
        validate_identifier(&mount.id, "UI mount id")?;
        validate_lua_identifier(&mount.export, "UI mount export")?;
        if mount
            .label
            .as_ref()
            .is_some_and(|label| label.trim().is_empty())
        {
            return Err(format!(
                "Plugin UI mount `{}` label must not be empty.",
                mount.id
            ));
        }
        if mount
            .label_message_key
            .as_ref()
            .is_some_and(|key| key.trim().is_empty())
        {
            return Err(format!(
                "Plugin UI mount `{}` label_message_key must not be empty.",
                mount.id
            ));
        }
        if !mount_ids.insert(mount.id.as_str()) {
            return Err(format!("Duplicate plugin UI mount id `{}`.", mount.id));
        }
    }
    for tool in &runtime.tools {
        if let Some(reuse) = &tool.execution.reusable {
            if manifest.schema_version != crate::VULCAN_PLUGIN_MANIFEST_SCHEMA_VERSION
                || tool.execution.kind != VulcanPluginToolExecutionKind::Stateless
            {
                return Err(format!(
                    "Tool `{}` reuse requires a version-two stateless declaration.",
                    tool.id
                ));
            }
            for function in [&reuse.before_call, &reuse.after_call, &reuse.reuse_check] {
                validate_lua_identifier(function, "tool reuse callback")?;
            }
            // Names remain exact identities; duplicate or empty entries cannot disguise a broader policy.
            // 名称保持精确身份；重复或空条目不能掩盖更宽策略。
            let mut initialization = BTreeSet::new();
            for name in &reuse.initialization_capabilities {
                if name.trim().is_empty() || name.contains('\0') || !initialization.insert(name) {
                    return Err(format!(
                        "Tool `{}` initialization capabilities must be nonempty distinct names without NUL.",
                        tool.id
                    ));
                }
            }
            // Hygiene cannot alias a business export or another cleanup phase.
            // 清理不能与业务导出或其他清理阶段共用名称。
            let hygiene = [&reuse.before_call, &reuse.after_call, &reuse.reuse_check];
            // Include every captured public export, not just the shared tool dispatcher.
            // 包含每个捕获的公开导出，不仅包含共用工具分发器。
            let public = [&runtime.invoke_function, &runtime.shutdown_function]
                .into_iter()
                .chain(runtime.hook_shutdown_function.iter())
                .chain(runtime.hooks.iter().map(|hook| &hook.function))
                .chain(runtime.actions.iter().map(|action| &action.function))
                .chain(
                    [
                        &runtime.lifecycle.install,
                        &runtime.lifecycle.enable,
                        &runtime.lifecycle.disable,
                        &runtime.lifecycle.uninstall,
                    ]
                    .into_iter()
                    .flatten(),
                );
            if hygiene.into_iter().collect::<BTreeSet<_>>().len() != hygiene.len()
                || public.into_iter().any(|export| hygiene.contains(&export))
            {
                return Err(format!(
                    "Tool `{}` hygiene callbacks must be distinct private functions.",
                    tool.id
                ));
            }
            if reuse.max_resident_vms == 0
                || reuse.max_running_calls == 0
                || reuse.max_running_calls > reuse.max_resident_vms
                || reuse.idle_ttl_ms == 0
                || reuse.max_uses == 0
                || (reuse.ordering == crate::VulcanPluginToolOrdering::KeySerial
                    && reuse.max_running_calls != 1)
            {
                return Err(format!(
                    "Tool `{}` has invalid reuse capacity, ordering or retirement limits.",
                    tool.id
                ));
            }
        }
        validate_identifier(&tool.id, "tool id")?;
        validate_identifier(&tool.group, "tool group")?;
        validate_identifier(&tool.execution.operation, "tool operation")?;
        if !tool.description.is_ascii() || tool.description.trim().is_empty() {
            return Err(format!(
                "Tool `{}` description must be non-empty English ASCII text.",
                tool.id
            ));
        }
        if tool.execution.timeout_ms == 0 && tool.execution.timeout_grace_ms == 0 {
            return Err(format!(
                "Tool `{}` timeout_ms must be greater than zero.",
                tool.id
            ));
        }
        if tool
            .execution
            .timeout_ms
            .checked_add(tool.execution.timeout_grace_ms)
            .is_none()
        {
            return Err(format!(
                "Tool `{}` fixed timeout and grace window overflow u64.",
                tool.id
            ));
        }
        match (
            &tool.execution.timeout_argument,
            tool.execution.max_timeout_ms,
        ) {
            (Some(argument), Some(maximum)) if maximum > 0 => {
                validate_identifier(argument, "timeout argument")?;
                if maximum
                    .checked_add(tool.execution.timeout_grace_ms)
                    .is_none()
                {
                    return Err(format!(
                        "Tool `{}` maximum timeout and grace window overflow u64.",
                        tool.id
                    ));
                }
            }
            (None, None) => {}
            _ => {
                return Err(format!(
                    "Tool `{}` must declare timeout_argument and max_timeout_ms together.",
                    tool.id
                ));
            }
        }
    }
    Ok(())
}

/// Validates manifest domains that do not depend on the optional Lua protocol.
/// 校验不依赖可选 Lua 协议的清单领域。
///
/// # Parameters
/// 参数
/// `manifest` is the exact first-release manifest being accepted by the package loader.
/// `manifest` 是插件包加载器正在接纳的精确首发清单。
///
/// # Returns
/// 返回值
/// Returns success only when common resources, permissions, UI, and MCP declarations are coherent.
/// 仅当通用资源、权限、界面与 MCP 声明彼此一致时返回成功。
fn validate_non_lua_manifest(manifest: &VulcanPluginManifest) -> Result<(), String> {
    let required_files = manifest.required_files.iter().collect::<BTreeSet<_>>();
    if required_files.len() != manifest.required_files.len() {
        return Err(String::from(
            "Plugin required_files contains duplicate paths.",
        ));
    }
    if manifest
        .execution
        .dependencies
        .manifest
        .as_ref()
        .is_some_and(|path| path.trim().is_empty())
    {
        return Err(String::from(
            "Plugin execution dependency manifest must not be empty when declared.",
        ));
    }
    if manifest.execution.dependencies.install_mode == VulcanPluginDependencyInstallMode::Registry
        && manifest.execution.dependencies.manifest.is_none()
    {
        return Err(String::from(
            "Registry dependency installation requires an execution dependency manifest.",
        ));
    }
    if (!manifest.execution.node_entries.is_empty()
        || !manifest.execution.python_entries.is_empty())
        && manifest.execution.dependencies.manifest.is_none()
    {
        return Err(String::from(
            "Plugin Node and Python entries require an execution dependency manifest.",
        ));
    }

    let permissions = manifest.permissions.iter().collect::<BTreeSet<_>>();
    if permissions.len() != manifest.permissions.len() {
        return Err(String::from(
            "Plugin permissions contains duplicate values.",
        ));
    }
    if manifest.lua_runtime.is_none()
        && permissions.contains(&VulcanPluginPermission::RuntimeLuaModules)
    {
        return Err(String::from(
            "Permission `runtime_lua_modules` requires a Lua runtime declaration.",
        ));
    }
    if manifest.execution.dependencies.install_mode == VulcanPluginDependencyInstallMode::Registry
        && !permissions.contains(&VulcanPluginPermission::DependencyRegistryNetwork)
    {
        return Err(String::from(
            "Registry dependency installation requires permission `dependency_registry_network`.",
        ));
    }

    let mut process_entry_ids = BTreeSet::new();
    for binary in &manifest.execution.native_binaries {
        validate_identifier(&binary.id, "native binary id")?;
        if !process_entry_ids.insert(binary.id.as_str()) {
            return Err(format!(
                "Duplicate plugin process entry id `{}`.",
                binary.id
            ));
        }
        if binary.targets.is_empty() {
            return Err(format!(
                "Plugin native binary `{}` must declare at least one target path.",
                binary.id
            ));
        }
        for target in binary.targets.keys() {
            validate_target_triple(target)?;
        }
    }
    for entry in &manifest.execution.node_entries {
        validate_identifier(&entry.id, "Node entry id")?;
        if !process_entry_ids.insert(entry.id.as_str()) {
            return Err(format!("Duplicate plugin process entry id `{}`.", entry.id));
        }
    }
    for entry in &manifest.execution.python_entries {
        validate_identifier(&entry.id, "Python entry id")?;
        if !process_entry_ids.insert(entry.id.as_str()) {
            return Err(format!("Duplicate plugin process entry id `{}`.", entry.id));
        }
    }
    for (has_resources, permission, label) in [
        (
            !manifest.execution.native_binaries.is_empty(),
            VulcanPluginPermission::ProcessBinary,
            "native binary entries",
        ),
        (
            !manifest.execution.node_entries.is_empty(),
            VulcanPluginPermission::ProcessNode,
            "Node entries",
        ),
        (
            !manifest.execution.python_entries.is_empty(),
            VulcanPluginPermission::ProcessPython,
            "Python entries",
        ),
        (
            manifest.config.is_some(),
            VulcanPluginPermission::Config,
            "configuration",
        ),
        (
            !manifest.ui.mounts.is_empty(),
            VulcanPluginPermission::UiHostContext,
            "dynamic UI mounts",
        ),
    ] {
        if has_resources && !permissions.contains(&permission) {
            return Err(format!(
                "Plugin {label} require permission `{}`.",
                crate::runtime::plugin_permission_key(permission)
            ));
        }
    }

    let mut server_ids = BTreeSet::new();
    for server in &manifest.mcp.servers {
        validate_identifier(&server.id, "MCP server id")?;
        if !server_ids.insert(server.id.as_str()) {
            return Err(format!("Duplicate plugin MCP server id `{}`.", server.id));
        }
        if server.name.trim().is_empty() {
            return Err(format!(
                "Plugin MCP server `{}` name must not be empty.",
                server.id
            ));
        }
        validate_mcp_transport(
            manifest,
            &server.id,
            server.scope,
            &server.transport,
            &permissions,
        )?;
    }
    Ok(())
}

/// Validates one plugin MCP transport and every referenced managed resource.
/// 校验单个插件 MCP 传输及其引用的全部受管资源。
///
/// # Parameters
/// 参数
/// `manifest` owns referenced resources, `server_id` identifies diagnostics, `scope` selects lifetime, `transport` is exact,
/// and `permissions` is the already deduplicated permission set.
/// `manifest` 持有被引用资源，`server_id` 标识诊断，`scope` 选择生命周期，`transport` 是精确传输，`permissions` 是已去重权限集合。
///
/// # Returns
/// 返回值
/// Returns success when the transport is closed over declared resources and authorized permissions.
/// 当传输仅引用已声明资源且具备授权权限时返回成功。
fn validate_mcp_transport(
    manifest: &VulcanPluginManifest,
    server_id: &str,
    scope: VulcanPluginMcpScopeManifest,
    transport: &VulcanPluginMcpTransportManifest,
    permissions: &BTreeSet<&VulcanPluginPermission>,
) -> Result<(), String> {
    match transport {
        VulcanPluginMcpTransportManifest::Stdio {
            launcher,
            args,
            env,
        } => {
            // One endpoint source is bound to one owner grant; repeating it in args or env
            // would multiply bearer-capability exposure without adding any functionality.
            // 一份端点来源绑定一份所有者授权；在参数或环境中重复只会扩大承载能力泄露面。
            let endpoint_sources = args
                .iter()
                .chain(env.values())
                .filter(|value| {
                    matches!(value, VulcanPluginValueSourceManifest::CoreBrowserEndpoint)
                })
                .count();
            match scope {
                VulcanPluginMcpScopeManifest::Global if endpoint_sources != 0 => {
                    return Err(format!(
                        "Plugin MCP server `{server_id}` core_browser_endpoint requires conversation scope."
                    ));
                }
                VulcanPluginMcpScopeManifest::Conversation if endpoint_sources != 1 => {
                    return Err(format!(
                        "Plugin MCP server `{server_id}` conversation scope requires exactly one core_browser_endpoint source."
                    ));
                }
                _ => {}
            }
            if !permissions.contains(&VulcanPluginPermission::ProcessSession) {
                return Err(format!(
                    "Plugin MCP server `{server_id}` stdio transport requires permission `process_session`."
                ));
            }
            match launcher {
                VulcanPluginMcpStdioLauncherManifest::NodeEntry { entry_id } => {
                    if !permissions.contains(&VulcanPluginPermission::ProcessNode) {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` node_entry requires permission `process_node`."
                        ));
                    }
                    if !manifest
                        .execution
                        .node_entries
                        .iter()
                        .any(|entry| entry.id == *entry_id)
                    {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` references unknown Node entry `{entry_id}`."
                        ));
                    }
                }
                VulcanPluginMcpStdioLauncherManifest::NodePackageBin { package, bin } => {
                    if !permissions.contains(&VulcanPluginPermission::ProcessNode) {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` node_package_bin requires permission `process_node`."
                        ));
                    }
                    if manifest.execution.dependencies.manifest.is_none() {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` node_package_bin requires an execution dependency manifest."
                        ));
                    }
                    if !valid_npm_package_name(package)
                        || validate_identifier(bin, "npm bin key").is_err()
                    {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` node_package_bin package or bin name is invalid."
                        ));
                    }
                }
                VulcanPluginMcpStdioLauncherManifest::NativeBinary { binary_id } => {
                    if !permissions.contains(&VulcanPluginPermission::ProcessBinary) {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` native_binary requires permission `process_binary`."
                        ));
                    }
                    if !manifest
                        .execution
                        .native_binaries
                        .iter()
                        .any(|binary| binary.id == *binary_id)
                    {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` references unknown native binary `{binary_id}`."
                        ));
                    }
                }
            }
            for name in env.keys() {
                if name.is_empty() || name.contains(['=', '\0']) {
                    return Err(format!(
                        "Plugin MCP server `{server_id}` stdio environment variable name is invalid."
                    ));
                }
            }
            for value in args.iter().chain(env.values()) {
                if !matches!(value, VulcanPluginValueSourceManifest::CoreBrowserEndpoint) {
                    validate_plugin_value_source(manifest, server_id, value, permissions)?;
                }
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
            if scope == VulcanPluginMcpScopeManifest::Conversation {
                return Err(format!(
                    "Plugin MCP server `{server_id}` conversation scope requires stdio transport."
                ));
            }
            if !permissions.contains(&VulcanPluginPermission::NetworkHttps) {
                return Err(format!(
                    "Plugin MCP server `{server_id}` remote transport requires permission `network_https`."
                ));
            }
            validate_plugin_value_source(manifest, server_id, url, permissions)?;
            validate_literal_plugin_mcp_remote_url(server_id, url)?;
            for (name, value) in headers {
                reqwest::header::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                    format!("Plugin MCP server `{server_id}` header name `{name}` is invalid.")
                })?;
                if plugin_mcp_header_requires_secret(name)
                    && !matches!(value, VulcanPluginValueSourceManifest::Secret { .. })
                {
                    return Err(format!(
                        "Plugin MCP server `{server_id}` credential header `{name}` must use the secret source."
                    ));
                }
                validate_plugin_value_source(manifest, server_id, value, permissions)?;
            }
            if let Some(oauth) = oauth {
                if let Some(client_id) = &oauth.client_id {
                    validate_plugin_value_source(manifest, server_id, client_id, permissions)?;
                }
                if let Some(client_secret) = &oauth.client_secret {
                    if !matches!(
                        client_secret,
                        VulcanPluginValueSourceManifest::Secret { .. }
                    ) {
                        return Err(format!(
                            "Plugin MCP server `{server_id}` OAuth client_secret must use the secret source."
                        ));
                    }
                    validate_plugin_value_source(manifest, server_id, client_secret, permissions)?;
                }
                if let Some(redirect_uri) = &oauth.redirect_uri {
                    validate_plugin_value_source(manifest, server_id, redirect_uri, permissions)?;
                }
                if oauth.scopes.iter().any(|scope| scope.trim().is_empty())
                    || oauth.scopes.iter().collect::<BTreeSet<_>>().len() != oauth.scopes.len()
                {
                    return Err(format!(
                        "Plugin MCP server `{server_id}` OAuth scopes must be non-empty and unique."
                    ));
                }
            }
        }
    }
    Ok(())
}

/// Returns whether one request Header has defined credential-bearing semantics and must use the Secret Store.
/// 返回单个请求 Header 是否具有明确的凭据承载语义并必须使用 Secret Store。
///
/// # Parameters
/// 参数
/// `name` is an already validated HTTP Header name from one remote MCP declaration.
/// `name` 是远程 MCP 声明中已经校验的 HTTP Header 名称。
///
/// # Returns
/// 返回值
/// Returns `true` for standard authentication and session Headers plus established API-token Header names.
/// 对标准认证与会话 Header 以及既定 API Token Header 名称返回 `true`。
fn plugin_mcp_header_requires_secret(name: &str) -> bool {
    [
        "authorization",
        "proxy-authorization",
        "cookie",
        "api-key",
        "x-api-key",
        "x-auth-token",
        "x-access-token",
    ]
    .iter()
    .any(|sensitive_name| name.eq_ignore_ascii_case(sensitive_name))
}

/// Validates one typed plugin value source without probing alternative locations.
/// 校验单个带类型插件值来源，且不探测任何候选位置。
///
/// # Parameters
/// 参数
/// `manifest` supplies the configuration contract, `server_id` identifies diagnostics, and `source` is exact.
/// `manifest` 提供配置契约，`server_id` 标识诊断，`source` 是精确值来源。
///
/// # Returns
/// 返回值
/// Returns success when the selected source has one non-empty and structurally authorized key.
/// 当选定来源具有唯一非空且结构上已授权的键时返回成功。
fn validate_plugin_value_source(
    manifest: &VulcanPluginManifest,
    server_id: &str,
    source: &VulcanPluginValueSourceManifest,
    permissions: &BTreeSet<&VulcanPluginPermission>,
) -> Result<(), String> {
    let (kind, value) = match source {
        VulcanPluginValueSourceManifest::Literal { value } => ("literal", value.as_str()),
        VulcanPluginValueSourceManifest::Config { key } => {
            if manifest.config.is_none() {
                return Err(format!(
                    "Plugin MCP server `{server_id}` references configuration key `{key}` without a configuration contract."
                ));
            }
            if !permissions.contains(&VulcanPluginPermission::Config) {
                return Err(format!(
                    "Plugin MCP server `{server_id}` configuration source requires permission `config`."
                ));
            }
            ("configuration key", key.as_str())
        }
        VulcanPluginValueSourceManifest::Secret { key } => {
            if !permissions.contains(&VulcanPluginPermission::SecretsRead) {
                return Err(format!(
                    "Plugin MCP server `{server_id}` secret source requires permission `secrets_read`."
                ));
            }
            ("secret key", key.as_str())
        }
        VulcanPluginValueSourceManifest::Environment { name } => {
            if !permissions.contains(&VulcanPluginPermission::SecretsRead) {
                return Err(format!(
                    "Plugin MCP server `{server_id}` environment source requires permission `secrets_read`."
                ));
            }
            ("environment variable name", name.as_str())
        }
        VulcanPluginValueSourceManifest::CoreBrowserEndpoint => {
            return Err(format!(
                "Plugin MCP server `{server_id}` core_browser_endpoint is valid only in conversation-scoped stdio args or env."
            ));
        }
    };
    if value.trim().is_empty() {
        return Err(format!(
            "Plugin MCP server `{server_id}` {kind} must not be empty."
        ));
    }
    Ok(())
}

/// Statically validates a manifest-literal remote MCP URL without resolving dynamic sources.
/// 静态校验清单字面量远程 MCP URL，且不解析动态来源。
///
/// # Parameters
/// 参数
/// `server_id` identifies diagnostics and `source` is the exact typed URL source.
/// `server_id` 标识诊断，`source` 是精确的带类型 URL 来源。
///
/// # Returns
/// 返回值
/// Returns success for dynamic sources or a credential-free fragment-free HTTPS literal.
/// 动态来源或不含凭据与片段的 HTTPS 字面量校验成功时返回成功。
fn validate_literal_plugin_mcp_remote_url(
    server_id: &str,
    source: &VulcanPluginValueSourceManifest,
) -> Result<(), String> {
    let VulcanPluginValueSourceManifest::Literal { value } = source else {
        return Ok(());
    };
    let url = reqwest::Url::parse(value).map_err(|error| {
        format!("Plugin MCP server `{server_id}` literal URL is invalid: {error}")
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "Plugin MCP server `{server_id}` literal URL must be credential-free, fragment-free HTTPS."
        ));
    }
    Ok(())
}

/// Returns whether one npm package name has a safe unscoped or scoped structure.
/// 返回单个 npm 包名是否具有安全的非作用域或作用域结构。
fn valid_npm_package_name(package: &str) -> bool {
    let segments = package.split('/').collect::<Vec<_>>();
    let shape_valid = match segments.as_slice() {
        [name] => !name.is_empty() && !name.starts_with('@'),
        [scope, name] => scope.starts_with('@') && scope.len() > 1 && !name.is_empty(),
        _ => false,
    };
    shape_valid
        && segments.iter().all(|segment| {
            *segment != "."
                && *segment != ".."
                && segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '@' | '-' | '_' | '.')
                })
        })
}

/// Validates one declarative Lua hook before host registration.
/// 在宿主注册前校验一个声明式 Lua hook。
fn validate_hook_manifest(hook: &VulcanPluginHookManifest) -> Result<(), String> {
    validate_identifier(&hook.id, "hook id")?;
    if hook.point.trim().is_empty()
        || hook
            .point
            .split('.')
            .any(|segment| validate_identifier(segment, "hook point segment").is_err())
    {
        return Err(format!(
            "Hook `{}` point must be a non-empty dot-separated identifier.",
            hook.id
        ));
    }
    // A tool hook must bind one original source identity; other hook points cannot silently ignore it.
    // 工具 Hook 必须绑定一份原始来源身份；其他点位不能静默忽略这个目标。
    let tool_point = matches!(
        hook.point.as_str(),
        VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION
            | VULCAN_PLUGIN_HOOK_TOOL_BEFORE
            | VULCAN_PLUGIN_HOOK_TOOL_AFTER
    );
    match (tool_point, hook.tool_target.as_ref()) {
        (true, None) => return Err(format!("Hook `{}` must declare tool_target.", hook.id)),
        (false, Some(_)) => {
            return Err(format!(
                "Hook `{}` declares tool_target for a non-tool point.",
                hook.id
            ));
        }
        (true, Some(target)) => {
            if target.namespace.is_empty()
                || target.namespace != target.namespace.trim()
                || target.namespace.contains('*')
                || target.original_name.is_empty()
                || target.original_name != target.original_name.trim()
                || target.original_name.contains('*')
            {
                return Err(format!(
                    "Hook `{}` tool_target must use exact non-empty namespace and original_name.",
                    hook.id
                ));
            }
            let mode_supported = match hook.point.as_str() {
                VULCAN_PLUGIN_HOOK_TOOL_REGISTRATION | VULCAN_PLUGIN_HOOK_TOOL_BEFORE => {
                    matches!(hook.mode, HookMode::Transform | HookMode::Decision)
                }
                VULCAN_PLUGIN_HOOK_TOOL_AFTER => hook.mode == HookMode::Transform,
                _ => unreachable!(),
            };
            if !mode_supported {
                return Err(format!(
                    "Hook `{}` uses an unsupported mode for `{}`.",
                    hook.id, hook.point
                ));
            }
        }
        (false, None) => {}
    }
    validate_lua_identifier(&hook.function, "hook function")?;
    if hook.timeout_ms == Some(0) {
        return Err(format!(
            "Hook `{}` timeout_ms must be greater than zero.",
            hook.id
        ));
    }
    let unique_capability_count = hook
        .requested_capabilities
        .iter()
        .collect::<BTreeSet<_>>()
        .len();
    if unique_capability_count != hook.requested_capabilities.len() {
        return Err(format!(
            "Hook `{}` requested_capabilities contains duplicate values.",
            hook.id
        ));
    }
    for (label, value) in [
        ("display_name", hook.display_name.as_deref()),
        ("progress_label", hook.progress_label.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(format!("Hook `{}` {label} must not be empty.", hook.id));
        }
    }
    Ok(())
}

/// Validates that one tool schema matches the host-owned lifecycle arguments used by its execution kind.
/// 验证单个工具 Schema 是否匹配其执行类型使用的宿主生命周期参数。
fn validate_tool_parameters_schema(
    tool: &VulcanPluginToolManifest,
    schema: &Value,
) -> Result<(), String> {
    if schema.get("type").and_then(Value::as_str) != Some("object") {
        return Err(format!(
            "Tool `{}` parameters schema must declare type `object`.",
            tool.id
        ));
    }
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            format!(
                "Tool `{}` parameters schema must declare object properties.",
                tool.id
            )
        })?;
    let required = schema
        .get("required")
        .map(|value| {
            value
                .as_array()
                .ok_or_else(|| {
                    format!("Tool `{}` required schema field must be an array.", tool.id)
                })?
                .iter()
                .map(|value| {
                    value.as_str().ok_or_else(|| {
                        format!(
                            "Tool `{}` required schema entries must be strings.",
                            tool.id
                        )
                    })
                })
                .collect::<Result<BTreeSet<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    if let Some(reserved) = VULCAN_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS
        .iter()
        .find(|reserved| properties.contains_key(**reserved) || required.contains(**reserved))
    {
        return Err(format!(
            "Tool `{}` must not declare host-owned payload field `{reserved}`.",
            tool.id
        ));
    }
    let has_session_id = properties.contains_key(VULCAN_PLUGIN_SESSION_ID_ARGUMENT);
    let requires_session_id = required.contains(VULCAN_PLUGIN_SESSION_ID_ARGUMENT);
    match tool.execution.kind {
        VulcanPluginToolExecutionKind::Stateless | VulcanPluginToolExecutionKind::SessionOpen => {
            if has_session_id || requires_session_id {
                return Err(format!(
                    "Tool `{}` must not declare the host-owned session_id argument.",
                    tool.id
                ));
            }
        }
        VulcanPluginToolExecutionKind::SessionCall
        | VulcanPluginToolExecutionKind::SessionClose => {
            let session_schema = properties
                .get(VULCAN_PLUGIN_SESSION_ID_ARGUMENT)
                .and_then(Value::as_object);
            if !requires_session_id
                || session_schema
                    .and_then(|schema| schema.get("type"))
                    .and_then(Value::as_str)
                    != Some("string")
            {
                return Err(format!(
                    "Tool `{}` must declare required string property session_id.",
                    tool.id
                ));
            }
        }
    }
    if tool.execution.kind == VulcanPluginToolExecutionKind::SessionClose
        && (properties.len() != 1 || tool.execution.timeout_argument.is_some())
    {
        return Err(format!(
            "Session-close tool `{}` may only declare the host-owned session_id argument.",
            tool.id
        ));
    }
    if let (Some(argument), Some(maximum)) = (
        tool.execution.timeout_argument.as_deref(),
        tool.execution.max_timeout_ms,
    ) {
        let timeout_schema = properties
            .get(argument)
            .and_then(Value::as_object)
            .ok_or_else(|| {
                format!(
                    "Tool `{}` timeout argument `{argument}` is missing from parameters properties.",
                    tool.id
                )
            })?;
        if timeout_schema.get("type").and_then(Value::as_str) != Some("integer") {
            return Err(format!(
                "Tool `{}` timeout argument `{argument}` must use integer schema type.",
                tool.id
            ));
        }
        let schema_maximum = timeout_schema
            .get("maximum")
            .and_then(Value::as_u64)
            .ok_or_else(|| {
                format!(
                    "Tool `{}` timeout argument `{argument}` must declare an unsigned maximum.",
                    tool.id
                )
            })?;
        if schema_maximum > maximum {
            return Err(format!(
                "Tool `{}` timeout schema maximum {schema_maximum} exceeds host maximum {maximum}.",
                tool.id
            ));
        }
    }
    Ok(())
}

/// Validates that every stateful plugin surface has one unambiguous open and close boundary.
/// 验证每个有状态插件工具面都具有唯一明确的打开与关闭边界。
fn validate_tool_execution_topology(tools: &[VulcanPluginToolManifest]) -> Result<(), String> {
    let open_count = tools
        .iter()
        .filter(|tool| tool.execution.kind == VulcanPluginToolExecutionKind::SessionOpen)
        .count();
    let call_count = tools
        .iter()
        .filter(|tool| tool.execution.kind == VulcanPluginToolExecutionKind::SessionCall)
        .count();
    let close_count = tools
        .iter()
        .filter(|tool| tool.execution.kind == VulcanPluginToolExecutionKind::SessionClose)
        .count();
    let has_stateful_tools = open_count + call_count + close_count > 0;
    if has_stateful_tools && (open_count != 1 || close_count != 1) {
        return Err(format!(
            "A stateful Vulcan Plugin must declare exactly one session-open tool and one session-close tool; found {open_count} open and {close_count} close tools."
        ));
    }
    Ok(())
}

/// Validates package files required by every declared plugin component.
/// 校验每个已声明插件组件所需的插件包文件。
fn validate_runtime_files(
    package_root: &Path,
    manifest: &VulcanPluginManifest,
) -> Result<(), String> {
    let dependencies = manifest
        .execution
        .dependencies
        .manifest
        .as_deref()
        .map(|relative_path| {
            let dependency_file =
                resolve_package_file(package_root, relative_path, "execution dependency manifest")?;
            PackageDependencyManifest::load_from_path(&dependency_file)
        })
        .transpose()?;
    if (manifest.lua_runtime.is_some()
        || !manifest.execution.node_entries.is_empty()
        || !manifest.execution.python_entries.is_empty())
        && dependencies.is_none()
    {
        return Err(String::from(
            "Lua, Node, and Python components require an execution dependency manifest.",
        ));
    }
    for required_file in &manifest.required_files {
        resolve_package_file(package_root, required_file, "required plugin file")?;
    }
    for binary in &manifest.execution.native_binaries {
        // currentTarget selects the only target artifact required in this platform-specific package.
        // currentTarget 选择当前平台专属插件包唯一必须存在的目标制品。
        let current_target = env!("VULCAN_PLUGIN_RUNTIME_TARGET");
        // relativePath must be declared explicitly; platform gaps cannot remain latent until invocation.
        // relativePath 必须显式声明，平台缺口不能延迟到调用阶段才暴露。
        let relative_path = binary.targets.get(current_target).ok_or_else(|| {
            format!(
                "Plugin binary `{}` does not declare current target `{current_target}`.",
                binary.id
            )
        })?;
        resolve_package_file(package_root, relative_path, "plugin binary")?;
    }
    for entry in &manifest.execution.node_entries {
        resolve_package_file(package_root, &entry.file, "plugin Node entry")?;
        resolve_package_directory(package_root, &entry.cwd, "plugin Node working directory")?;
    }
    for entry in &manifest.execution.python_entries {
        resolve_package_file(package_root, &entry.file, "plugin Python entry")?;
        resolve_package_directory(package_root, &entry.cwd, "plugin Python working directory")?;
    }
    if let Some(dependencies) = dependencies.as_ref() {
        validate_plugin_process_dependencies(package_root, manifest, dependencies)?;
    }
    if let Some(runtime) = manifest.lua_runtime.as_ref() {
        resolve_package_file(package_root, &runtime.bundle, "Lua runtime bundle")?;
        for action in &runtime.actions {
            if let Some(path) = &action.input_schema_file {
                read_json_file(&resolve_package_file(
                    package_root,
                    path,
                    "plugin action input schema",
                )?)?;
            }
            if let Some(path) = &action.output_schema_file {
                read_json_file(&resolve_package_file(
                    package_root,
                    path,
                    "plugin action output schema",
                )?)?;
            }
        }
    }
    for mount in &manifest.ui.mounts {
        let entry = resolve_package_file(package_root, &mount.entry, "plugin UI module")?;
        if entry.extension().and_then(|extension| extension.to_str()) != Some("mjs") {
            return Err(format!(
                "Plugin UI mount `{}` entry must use the `.mjs` extension.",
                mount.id
            ));
        }
    }
    if let Some(config) = &manifest.config {
        let schema = read_json_file(&resolve_package_file(
            package_root,
            &config.schema_file,
            "plugin configuration schema",
        )?)?;
        let defaults = read_json_file(&resolve_package_file(
            package_root,
            &config.defaults_file,
            "plugin configuration defaults",
        )?)?;
        if !defaults.is_object() {
            return Err(String::from(
                "Plugin configuration defaults must contain a JSON object.",
            ));
        }
        validate_json_schema_instance(&schema, &defaults, "plugin configuration defaults")?;
    }
    Ok(())
}

/// Validates process dependency inputs and bundled artifact completeness.
/// 校验进程依赖输入与随包制品完整性。
fn validate_plugin_process_dependencies(
    package_root: &Path,
    manifest: &VulcanPluginManifest,
    dependencies: &PackageDependencyManifest,
) -> Result<(), String> {
    if !manifest.execution.node_entries.is_empty() {
        let node = dependencies.node_runtime.as_ref().ok_or_else(|| {
            "Plugin Node entries require node_runtime in dependencies.yaml.".to_string()
        })?;
        let package_json = resolve_package_file(
            package_root,
            &node.package_json,
            "plugin Node package manifest",
        )?;
        resolve_package_file(
            package_root,
            &node.lockfile,
            "plugin Node dependency lockfile",
        )?;
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Bundled
        {
            let package = read_json_file(&package_json)?;
            if node_package_declares_dependencies(&package)? {
                let node_modules = package_json
                    .parent()
                    .ok_or_else(|| {
                        "Plugin Node package manifest has no parent directory.".to_string()
                    })?
                    .join("node_modules");
                resolve_existing_package_directory(
                    package_root,
                    &node_modules,
                    "bundled Node dependency directory",
                )?;
            }
        }
    }
    if !manifest.execution.python_entries.is_empty() {
        let python = dependencies.python_runtime.as_ref().ok_or_else(|| {
            "Plugin Python entries require python_runtime in dependencies.yaml.".to_string()
        })?;
        let lockfile = resolve_package_file(
            package_root,
            &python.lockfile,
            "plugin Python dependency lockfile",
        )?;
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Bundled
            && python_lockfile_declares_dependencies(&lockfile)?
        {
            resolve_package_directory(
                package_root,
                "python/vendor",
                "bundled Python dependency directory",
            )?;
        }
    }
    Ok(())
}

/// Reports whether a Node package manifest declares any installable dependency.
/// 判断 Node 包清单是否声明任一需要安装的依赖。
pub(crate) fn node_package_declares_dependencies(package: &Value) -> Result<bool, String> {
    let object = package
        .as_object()
        .ok_or_else(|| "Plugin Node package manifest must contain an object.".to_string())?;
    for field in [
        "dependencies",
        "devDependencies",
        "optionalDependencies",
        "peerDependencies",
    ] {
        if let Some(value) = object.get(field) {
            let dependencies = value.as_object().ok_or_else(|| {
                format!("Plugin Node package field `{field}` must contain an object.")
            })?;
            if !dependencies.is_empty() {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// Reports whether one Python requirements lockfile contains a package requirement.
/// 判断一个 Python requirements 锁文件是否包含包依赖。
fn python_lockfile_declares_dependencies(lockfile: &Path) -> Result<bool, String> {
    let text = fs::read_to_string(lockfile).map_err(|error| {
        format!(
            "Failed to read plugin Python dependency lockfile `{}`: {error}",
            lockfile.display()
        )
    })?;
    Ok(text.lines().any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with('#')
    }))
}

/// Validates one already-joined package directory without accepting a second relative interpretation.
/// 校验一个已拼接插件目录，且不接受第二次相对路径解释。
fn resolve_existing_package_directory(
    package_root: &Path,
    directory: &Path,
    label: &str,
) -> Result<PathBuf, String> {
    let canonical_root = package_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve plugin package root: {error}"))?;
    let canonical_directory = directory.canonicalize().map_err(|error| {
        format!(
            "Failed to resolve {label} `{}`: {error}",
            directory.display()
        )
    })?;
    if !canonical_directory.starts_with(&canonical_root) || !canonical_directory.is_dir() {
        return Err(format!(
            "{label} escapes the plugin package root or is not a directory."
        ));
    }
    Ok(canonical_directory)
}

/// Validates one JSON instance against a self-contained plugin schema.
/// 根据一份自包含插件 Schema 校验一个 JSON 实例。
///
/// # Parameters
/// 参数
/// `schema` is package-owned schema data, `instance` is the exact value, and `label` names it.
/// `schema` 是插件包持有的 Schema 数据，`instance` 是精确值，`label` 用于命名。
///
/// # Returns
/// 返回值
/// Returns unit when the schema compiles and the instance validates.
/// Schema 可编译且实例校验通过时返回空值。
pub(crate) fn validate_json_schema_instance(
    schema: &Value,
    instance: &Value,
    label: &str,
) -> Result<(), String> {
    reject_external_schema_references(schema)?;
    let validator = jsonschema::validator_for(schema)
        .map_err(|error| format!("Invalid {label} JSON Schema: {error}"))?;
    validator
        .validate(instance)
        .map_err(|error| format!("Invalid {label}: {error}"))
}

/// Rejects external JSON Schema references while preserving reference-shaped annotation data.
/// 拒绝外部 JSON Schema 引用，同时保留形似引用的注解数据。
///
/// # Parameters
/// 参数
/// `schema` is the current semantically identified schema node.
/// `schema` 是当前按语义识别出的 Schema 节点。
///
/// # Returns
/// 返回值
/// Returns unit for local-only schemas or an explicit error for one external reference.
/// Schema 仅含本地引用时返回空值，存在外部引用时返回显式错误。
fn reject_external_schema_references(schema: &Value) -> Result<(), String> {
    let Some(schema_object) = schema.as_object() else {
        return Ok(());
    };
    for keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
        if let Some(reference) = schema_object.get(keyword) {
            let is_local_fragment = reference
                .as_str()
                .is_some_and(|reference| reference.starts_with('#'));
            if !is_local_fragment {
                // The untrusted reference is deliberately omitted because a URI can contain credentials.
                // 刻意不回显不受信任的引用，因为 URI 可能包含凭据。
                return Err(format!(
                    "Plugin JSON Schema contains an external or invalid `{keyword}`; only local fragment references are allowed."
                ));
            }
        }
    }

    // directSchemaKeywords covers single-schema applicators across supported JSON Schema drafts.
    // directSchemaKeywords 覆盖受支持 JSON Schema 方言中的单 Schema 应用器。
    for keyword in [
        "additionalItems",
        "additionalProperties",
        "contains",
        "contentSchema",
        "else",
        "if",
        "items",
        "not",
        "propertyNames",
        "then",
        "unevaluatedItems",
        "unevaluatedProperties",
    ] {
        if let Some(child_schema) = schema_object.get(keyword) {
            reject_external_schema_references(child_schema)?;
        }
    }

    // schemaMapKeywords covers maps whose values are independently evaluated schemas.
    // schemaMapKeywords 覆盖值会被独立求值为 Schema 的映射关键字。
    for keyword in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "patternProperties",
        "properties",
    ] {
        if let Some(children) = schema_object.get(keyword).and_then(Value::as_object) {
            for child_schema in children.values() {
                reject_external_schema_references(child_schema)?;
            }
        }
    }

    // schemaArrayKeywords covers combinators and both modern prefixItems and legacy array-form items.
    // schemaArrayKeywords 覆盖组合器、现代 prefixItems 与旧式数组形 items。
    for keyword in ["allOf", "anyOf", "items", "oneOf", "prefixItems"] {
        if let Some(children) = schema_object.get(keyword).and_then(Value::as_array) {
            for child_schema in children {
                reject_external_schema_references(child_schema)?;
            }
        }
    }

    // dependencies values are schemas or property-name arrays in legacy drafts.
    // 旧方言中的 dependencies 值可以是 Schema 或属性名数组。
    if let Some(dependencies) = schema_object.get("dependencies").and_then(Value::as_object) {
        for child_schema in dependencies
            .values()
            .filter(|value| value.is_object() || value.is_boolean())
        {
            reject_external_schema_references(child_schema)?;
        }
    }
    Ok(())
}

/// Resolves one package-relative directory and rejects absolute paths, traversal, and symlinks.
/// 解析一个插件包相对目录，并拒绝绝对路径、目录穿越与符号链接。
///
/// # Parameters
/// 参数
/// `package_root` is the validated package root, `relative_path` is untrusted manifest input,
/// and `label` identifies the field in diagnostics.
/// `package_root` 是已校验插件包根，`relative_path` 是不可信清单输入，`label` 标识诊断字段。
///
/// # Returns
/// 返回值
/// Returns the canonical contained directory.
/// 返回规范化且位于包内的目录。
fn resolve_package_directory(
    package_root: &Path,
    relative_path: &str,
    label: &str,
) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        return Err(format!("Vulcan Plugin {label} must be relative."));
    }
    let joined = package_root.join(relative);
    let canonical = joined
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan Plugin {label}: {error}"))?;
    if !canonical.starts_with(package_root) {
        return Err(format!("Vulcan Plugin {label} escapes the package root."));
    }
    let metadata = fs::symlink_metadata(&joined)
        .map_err(|error| format!("Failed to inspect Vulcan Plugin {label}: {error}"))?;
    if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
        return Err(format!(
            "Vulcan Plugin {label} must be a regular directory and must not be a symbolic link or reparse point."
        ));
    }
    Ok(canonical)
}

/// Loads one prompt contribution and verifies its atomic tool dependency set.
/// 加载单个提示词贡献并校验其原子工具依赖集合。
fn load_prompt_contribution(
    package_root: &Path,
    plugin_id: &str,
    prompt: &VulcanPluginPromptManifest,
    tool_ids: &BTreeSet<String>,
) -> Result<VulcanPluginPromptContribution, String> {
    validate_identifier(&prompt.id, "prompt contribution id")?;
    if prompt.required_tools.is_empty()
        || prompt
            .required_tools
            .iter()
            .any(|tool_id| !tool_ids.contains(tool_id))
    {
        return Err(format!(
            "Prompt contribution `{}` must reference only declared tools and cannot be empty.",
            prompt.id
        ));
    }
    let path = resolve_package_file(package_root, &prompt.file, "prompt contribution")?;
    let body = fs::read_to_string(&path).map_err(|error| {
        format!(
            "Failed to read prompt contribution `{}`: {error}",
            prompt.id
        )
    })?;
    if body.trim().is_empty() {
        return Err(format!("Prompt contribution `{}` is empty.", prompt.id));
    }
    Ok(VulcanPluginPromptContribution {
        plugin_id: plugin_id.to_owned(),
        id: prompt.id.clone(),
        body,
        required_tools: prompt.required_tools.clone(),
    })
}

/// Resolves one package-relative existing file and rejects symlink/path escape.
/// 解析一个插件包相对的既有文件，并拒绝符号链接或路径逃逸。
fn resolve_package_file(
    package_root: &Path,
    relative_path: &str,
    label: &str,
) -> Result<PathBuf, String> {
    let relative = Path::new(relative_path);
    if relative.is_absolute() {
        return Err(format!("Vulcan Plugin {label} path must be relative."));
    }
    let candidate = package_root.join(relative);
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("Failed to resolve Vulcan Plugin {label}: {error}"))?;
    if !canonical.starts_with(package_root) || !canonical.is_file() {
        return Err(format!(
            "Vulcan Plugin {label} escapes the package or is not a file."
        ));
    }
    Ok(canonical)
}

/// Reads one exact JSON file without accepting non-JSON formats.
/// 读取一个精确 JSON 文件，不接受其他格式。
fn read_json_file(path: &Path) -> Result<Value, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read JSON file `{}`: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|error| format!("Failed to decode JSON file `{}`: {error}", path.display()))
}

/// Computes a deterministic SHA-256 digest over every regular package path and file body.
/// 对插件包内每个普通文件路径与文件内容计算确定性的 SHA-256 摘要。
pub(crate) fn compute_package_fingerprint(package_root: &Path) -> Result<String, String> {
    // files are sorted before hashing so enumeration order cannot alter authorization identity.
    // files 在哈希前排序，确保枚举顺序不会改变授权身份。
    let mut files = Vec::new();
    collect_package_files(package_root, package_root, &mut files)?;
    files.sort();
    // digest owns the collision-resistant package authorization hash state.
    // digest 持有抗碰撞的插件包授权哈希状态。
    let mut digest = Sha256::new();
    for file in files {
        // relative is the exact contained package path included before its file body.
        // relative 是在文件内容前纳入摘要的精确包内路径。
        let relative = file
            .strip_prefix(package_root)
            .map_err(|error| format!("Failed to derive Vulcan Plugin fingerprint path: {error}"))?;
        update_package_digest_path(&mut digest, relative);
        digest.update([0]);
        // bytes contains the exact distributable file body protected by authorization.
        // bytes 保存受授权保护的精确可分发文件内容。
        let bytes = fs::read(&file).map_err(|error| {
            format!(
                "Failed to read Vulcan Plugin fingerprint file `{}`: {error}",
                file.display()
            )
        })?;
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(&bytes);
        digest.update([0xff]);
    }
    Ok(hex::encode(digest.finalize()))
}

/// Recursively collects regular package files while rejecting symbolic links and path escape.
/// 递归收集普通插件文件，同时拒绝符号链接与路径逃逸。
fn collect_package_files(
    package_root: &Path,
    directory: &Path,
    files: &mut Vec<PathBuf>,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| {
            format!(
                "Failed to read Vulcan Plugin package directory `{}`: {error}",
                directory.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to enumerate Vulcan Plugin package: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Failed to inspect Vulcan Plugin package entry `{}`: {error}",
                path.display()
            )
        })?;
        if crate::metadata_is_link_or_reparse_point(&metadata) {
            return Err(format!(
                "Vulcan Plugin package entry `{}` must not be a symbolic link or reparse point.",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_package_files(package_root, &path, files)?;
        } else if metadata.is_file() {
            let canonical = path.canonicalize().map_err(|error| {
                format!(
                    "Failed to canonicalize Vulcan Plugin package entry `{}`: {error}",
                    path.display()
                )
            })?;
            if !canonical.starts_with(package_root) {
                return Err(format!(
                    "Vulcan Plugin package entry `{}` escapes the trusted package root.",
                    path.display()
                ));
            }
            files.push(canonical);
        }
    }
    Ok(())
}

/// Updates a package digest with one exact platform-native relative path representation.
/// 使用一个精确的平台原生相对路径表示更新插件包摘要。
///
/// # Parameters
/// 参数
/// `digest` is the active SHA-256 state and `path` is a contained relative package path.
/// `digest` 是活动 SHA-256 状态，`path` 是受约束的包内相对路径。
fn update_package_digest_path(digest: &mut Sha256, path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        // bytes preserves non-Unicode Unix path identities without lossy replacement.
        // bytes 在不进行有损替换的情况下保留非 Unicode Unix 路径身份。
        let bytes = path.as_os_str().as_bytes();
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        // units preserves the native Windows UTF-16 path identity.
        // units 保留原生 Windows UTF-16 路径身份。
        let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
        digest.update((units.len() as u64).to_le_bytes());
        for unit in units {
            digest.update(unit.to_le_bytes());
        }
    }
}

/// Validates one lowercase kebab/snake identifier used in host dispatch.
/// 校验一个用于宿主调度的小写短横线或下划线标识符。
pub(crate) fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase);
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid Vulcan Plugin {label} `{value}`."))
    }
}

/// Validates one dot-separated Lua module path.
/// 校验一个以点分隔的 Lua 模块路径。
/// Validates one Lua identifier before embedding it into fixed host code.
/// 在将标识符嵌入固定宿主代码前校验单个 Lua 标识符。
fn validate_lua_identifier(value: &str, label: &str) -> Result<(), String> {
    let mut bytes = value.bytes();
    let valid_start = bytes
        .next()
        .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
    if valid_start && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
        Ok(())
    } else {
        Err(format!("Invalid Vulcan Plugin {label} `{value}`."))
    }
}

/// Validates one standard Cargo target triple used by the platform exclusion list.
/// 校验一个平台排除列表使用的标准 Cargo 目标三元组。
fn validate_target_triple(target: &str) -> Result<(), String> {
    let valid = !target.is_empty()
        && target.contains('-')
        && target.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        });
    if valid {
        Ok(())
    } else {
        Err(format!(
            "Invalid Vulcan Plugin excluded target triple `{target}`."
        ))
    }
}

/// Matches the current compiled target against manifest exclusions.
/// 将当前编译目标与清单排除项进行匹配。
fn current_platform_is_excluded(excluded_targets: &[String]) -> bool {
    let current_target = env!("VULCAN_PLUGIN_RUNTIME_TARGET");
    excluded_targets
        .iter()
        .any(|excluded| excluded == current_target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Loads `root` through the public single-root path with the supplied policies and revision.
    /// 使用传入策略及修订，通过公开单根路径加载 `root`。
    /// Returns the final validated surface rather than the private discovery-only intermediate result.
    /// 返回最终已校验表面，而非私有的仅发现中间结果。
    fn load_plugin_root_catalog(
        root: &Path,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
        revision: u64,
    ) -> VulcanPluginCatalogSnapshot {
        load_vulcan_plugin_catalog(
            root,
            None,
            disabled_plugin_ids,
            enabled_plugin_ids,
            revision,
        )
    }

    /// Verifies generic tool hooks require an exact original identity and a phase-compatible mode.
    /// 验证通用工具 Hook 必须声明精确原始身份及与阶段兼容的模式。
    #[test]
    fn generic_tool_hook_declarations_are_exact_and_phase_bound() {
        let valid = serde_json::json!({
            "id": "navigate-filter",
            "point": "tool.before",
            "tool_target": {
                "source_kind": "mcp",
                "namespace": "plugin:playwright:browser",
                "original_name": "browser_navigate"
            },
            "mode": "transform",
            "function": "before_navigate"
        });
        let hook: VulcanPluginHookManifest =
            serde_json::from_value(valid.clone()).expect("decode exact hook");
        validate_hook_manifest(&hook).expect("accept exact original tool hook");
        for (changed, expected) in [
            (
                serde_json::json!({"point": "tool.before", "tool_target": null}),
                "tool_target",
            ),
            (
                serde_json::json!({"point": "conversation.turn.precheck", "tool_target": valid["tool_target"]}),
                "non-tool",
            ),
            (
                serde_json::json!({"point": "tool.before", "tool_target": {"source_kind": "mcp", "namespace": "*", "original_name": "browser_navigate"}}),
                "exact",
            ),
            (
                serde_json::json!({"point": "tool.after", "mode": "decision"}),
                "unsupported mode",
            ),
        ] {
            let mut candidate = valid.clone();
            for (key, value) in changed.as_object().expect("changed fields") {
                candidate[key] = value.clone();
            }
            let hook: VulcanPluginHookManifest =
                serde_json::from_value(candidate).expect("decode invalid hook shape");
            assert!(
                validate_hook_manifest(&hook)
                    .expect_err("reject invalid target")
                    .contains(expected)
            );
        }
    }

    /// Verifies plugin schemas allow local references and opaque annotation data while rejecting external reference resolution without echoing values.
    /// 验证插件 Schema 接受本地引用与不透明注解数据，同时拒绝外部引用解析且不回显引用值。
    #[test]
    fn plugin_schema_reference_guard_is_semantic_and_credential_safe() {
        let local_schema = serde_json::json!({
            "type": "object",
            "properties": {"name": {"$ref": "#/$defs/name"}},
            "$defs": {"name": {"type": "string"}},
            "default": {"$ref": "https://example.test/annotation-only"}
        });
        validate_json_schema_instance(
            &local_schema,
            &serde_json::json!({"name": "Vulcan"}),
            "test configuration",
        )
        .expect("local references and annotation data should validate");

        for reference_keyword in ["$ref", "$dynamicRef", "$recursiveRef"] {
            let external_schema = serde_json::json!({
                "type": "object",
                "properties": {
                    "credential": {(reference_keyword): "https://user:secret@example.test/schema.json"}
                }
            });
            let error = validate_json_schema_instance(
                &external_schema,
                &serde_json::json!({}),
                "test configuration",
            )
            .expect_err("external schema references must be rejected");
            assert!(error.contains("only local fragment references"));
            assert!(error.contains(reference_keyword));
            assert!(!error.contains("user:secret"));
            assert!(!error.contains("example.test"));
        }
    }

    /// Writes one minimal valid package used by catalog tests.
    /// 写入一个供目录测试使用的最小有效插件包。
    fn write_valid_package(root: &Path) {
        let package = root.join("test-plugin");
        fs::create_dir_all(package.join("runtime")).expect("create runtime directory");
        fs::create_dir_all(package.join("schemas")).expect("create schema directory");
        fs::create_dir_all(package.join("model")).expect("create model directory");
        fs::write(package.join("dependencies.yaml"), "dependencies: []\n")
            .expect("write dependencies");
        fs::write(package.join("runtime/required.txt"), "required\n")
            .expect("write required runtime asset");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
            "return { dispatch=function() end, shutdown=function() end, on_precheck=function() return '{\"ok\":true,\"result\":{}}' end }\n",
        )
        .expect("write runtime");
        fs::write(
            package.join("schemas/open.json"),
            r#"{"type":"object","properties":{}}"#,
        )
        .expect("write schema");
        fs::write(package.join("model/instructions.md"), "Use the test tool.")
            .expect("write prompt");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: test-plugin
version: 1.0.0
default_enabled: true
display:
  name: Test Plugin
  capability: Test capability
required_files: [runtime/required.txt]
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  max_active_sessions: 2
  tools:
    - id: test_open
      group: test_tools
      description: Run one stateless test operation.
      parameters_file: schemas/open.json
      execution:
        kind: stateless
        operation: open
        timeout_ms: 1000
  hooks:
    - id: precheck
      point: conversation.turn.precheck
      mode: transform
      function: on_precheck
      priority: 10
      timeout_ms: 1000
      failure_policy: stop
prompt_contributions:
  - id: usage
    file: model/instructions.md
    required_tools: [test_open]
"#,
        )
        .expect("write manifest");
    }

    /// Writes one runtime-free plugin containing a namespaced skill and an agent bound to it.
    /// 写入一个不含运行时且包含命名空间技能与绑定智能体的插件。
    fn write_content_package(root: &Path, attached_skill: &str) {
        write_named_content_package(root, "content-plugin", attached_skill);
    }

    /// Writes one MCP-only package using the clean initial schema without a Lua runtime.
    /// 使用干净首发 Schema 写入一个不含 Lua Runtime 的纯 MCP 插件包。
    ///
    /// # Parameters
    /// 参数
    /// `root` is the trusted plugin root and `extra_manifest` appends caller-selected transport details.
    /// `root` 是可信插件根，`extra_manifest` 追加调用方选择的传输细节。
    fn write_mcp_only_package(root: &Path, extra_manifest: &str) {
        let package = root.join("mcp-plugin");
        fs::create_dir_all(&package).expect("create MCP package");
        fs::write(
            package.join("plugin.yaml"),
            format!(
                "schema_version: 1\nid: mcp-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: MCP Plugin\n  capability: Managed MCP fixture\n{extra_manifest}"
            ),
        )
        .expect("write MCP manifest");
    }

    /// Writes one managed package-bin stdio MCP fixture with caller-selected lifetime and sources.
    /// 写入一个由调用方选择生命周期及来源的受管包入口 stdio MCP 夹具。
    ///
    /// # Parameters
    /// 参数
    /// `root` owns the package; `scope` is the exact manifest lifetime; `args` is YAML array content.
    /// `root` 持有插件包；`scope` 是精确清单生命周期；`args` 是 YAML 参数数组内容。
    fn write_scoped_stdio_mcp_package(root: &Path, scope: &str, args: &str) {
        write_mcp_only_package(
            root,
            &format!(
                "permissions:\n  - process_node\n  - process_session\nexecution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\nmcp:\n  servers:\n    - id: browser\n      name: Browser\n      scope: {scope}\n      transport:\n        type: stdio\n        launcher:\n          type: node_package_bin\n          package: '@playwright/mcp'\n          bin: playwright-mcp\n        args:\n{args}"
            ),
        );
        fs::write(
            root.join("mcp-plugin/dependencies.yaml"),
            "dependencies: []\n",
        )
        .expect("write MCP dependency declaration");
    }

    /// Writes one named runtime-free plugin for namespace-collision tests.
    /// 为命名空间冲突测试写入一个具名无运行时插件。
    fn write_named_content_package(root: &Path, plugin_id: &str, attached_skill: &str) {
        let package = root.join(plugin_id);
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
            format!(
                "---\nname: Content Reviewer\nskills:\n  - {attached_skill}\n---\nReview the requested change."
            ),
        )
        .expect("write content agent");
        fs::write(
            package.join("plugin.yaml"),
            format!(
                r#"schema_version: 1
id: {plugin_id}
version: 1.0.0
default_enabled: true
display:
  name: Content Plugin
  capability: Read-only review guidance
"#
            ),
        )
        .expect("write content manifest");
    }

    /// Writes one named stateless package with an exact ordered tool set for collision tests.
    /// 为冲突测试写入一个带精确有序工具集合的命名无状态插件包。
    ///
    /// # Parameters
    /// 参数
    /// `root` is the catalog root, `plugin_id` is the package identity, and `tool_ids` preserves manifest order.
    /// `root` 是目录根，`plugin_id` 是插件身份，`tool_ids` 保持清单顺序。
    pub(super) fn write_named_stateless_package(root: &Path, plugin_id: &str, tool_ids: &[&str]) {
        let package = root.join(plugin_id);
        fs::create_dir_all(package.join("runtime")).expect("create named runtime directory");
        fs::create_dir_all(package.join("schemas")).expect("create named schema directory");
        fs::write(package.join("dependencies.yaml"), "dependencies: []\n")
            .expect("write named dependencies");
        fs::write(
            package.join("runtime/plugin.bundle.lua"),
            "return { dispatch=function() end, shutdown=function() end }\n",
        )
        .expect("write named runtime");
        fs::write(
            package.join("schemas/stateless.json"),
            r#"{"type":"object","properties":{}}"#,
        )
        .expect("write named schema");
        let tools = tool_ids
            .iter()
            .map(|tool_id| {
                format!(
                    "    - id: {tool_id}\n      group: collision_tools\n      description: Run one collision fixture operation.\n      parameters_file: schemas/stateless.json\n      execution:\n        kind: stateless\n        operation: run\n        timeout_ms: 1000\n"
                )
            })
            .collect::<String>();
        fs::write(
            package.join("plugin.yaml"),
            format!(
                "schema_version: 1\nid: {plugin_id}\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: {plugin_id}\n  capability: Collision fixture\nexecution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\nlua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\n  tools:\n{tools}"
            ),
        )
        .expect("write named manifest");
    }

    /// Verifies a valid package contributes its complete atomic surface.
    /// 验证有效插件包会贡献完整原子工具面。
    #[test]
    fn valid_package_loads_atomic_tools_and_prompt() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 7);
        assert_eq!(snapshot.revision, 7);
        assert_eq!(snapshot.enabled_tools.len(), 1);
        assert_eq!(snapshot.enabled_hooks.len(), 1);
        assert_eq!(snapshot.prompt_contributions.len(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Enabled
        );
    }

    /// Verifies a runtime-free package publishes its complete skill and agent surface atomically.
    /// 验证无运行时插件包会原子发布完整技能与智能体界面。
    #[test]
    fn content_package_loads_skills_and_agents_without_runtime() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_content_package(temp.path(), "review-helper");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 3);
        let plugin = &snapshot.plugins["content-plugin"];

        assert_eq!(plugin.availability, VulcanPluginAvailability::Enabled);
        assert!(
            !plugin
                .manifest
                .as_ref()
                .expect("content manifest should exist")
                .has_lua_runtime()
        );
        assert_eq!(plugin.skills.len(), 1);
        assert_eq!(plugin.agents.len(), 1);
        assert!(
            snapshot
                .enabled_skills
                .contains_key("content-plugin:review-helper")
        );
        assert!(
            snapshot
                .enabled_agents
                .contains_key("content-plugin:reviewer")
        );
        assert_eq!(
            plugin.agents[0].prompt_skills.policy.selectors,
            vec![String::from("content-plugin:review-helper")]
        );
        assert!(snapshot.enabled_tools.is_empty());
        assert!(snapshot.enabled_hooks.is_empty());
    }

    /// Verifies the initial schema accepts an MCP-only HTTPS plugin and publishes its namespaced descriptor.
    /// 验证首发 Schema 接受纯 HTTPS MCP 插件，并发布带命名空间的描述符。
    #[test]
    fn mcp_only_initial_schema_publishes_without_lua_runtime() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_mcp_only_package(
            temp.path(),
            r#"permissions:
  - network_https
mcp:
  servers:
    - id: documentation
      name: Documentation
      transport:
        type: http
        url:
          source: literal
          value: https://example.test/mcp
"#,
        );
        let snapshot =
            load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 11);
        let plugin = &snapshot.plugins["mcp-plugin"];
        assert_eq!(plugin.availability, VulcanPluginAvailability::Enabled);
        assert!(
            !plugin
                .manifest
                .as_ref()
                .expect("valid MCP manifest")
                .has_lua_runtime()
        );
        assert!(
            snapshot
                .enabled_mcp_servers
                .contains_key("mcp-plugin:documentation")
        );
    }

    /// Verifies a conversation-scoped stdio server declares exactly one owner-granted endpoint.
    /// 验证会话作用域 stdio 服务器恰好声明一个所有者授权端点。
    #[test]
    fn conversation_mcp_catalog_requires_one_endpoint_only_in_stdio() {
        let valid_root = tempfile::tempdir().expect("create conversation fixture");
        write_scoped_stdio_mcp_package(
            valid_root.path(),
            "conversation",
            "          - source: core_browser_endpoint\n",
        );
        let valid =
            load_plugin_root_catalog(valid_root.path(), &BTreeSet::new(), &BTreeSet::new(), 20);
        assert_eq!(
            valid.plugins["mcp-plugin"].availability,
            VulcanPluginAvailability::Enabled,
            "{:?}",
            valid.plugins["mcp-plugin"].diagnostics
        );
        assert_eq!(
            valid.enabled_mcp_servers["mcp-plugin:browser"]
                .manifest
                .scope,
            VulcanPluginMcpScopeManifest::Conversation
        );

        let global_root = tempfile::tempdir().expect("create global misuse fixture");
        write_scoped_stdio_mcp_package(
            global_root.path(),
            "global",
            "          - source: core_browser_endpoint\n",
        );
        let global =
            load_plugin_root_catalog(global_root.path(), &BTreeSet::new(), &BTreeSet::new(), 21);
        assert!(
            global.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|message| message.contains("requires conversation scope"))
        );

        let duplicate_root = tempfile::tempdir().expect("create duplicate endpoint fixture");
        write_scoped_stdio_mcp_package(
            duplicate_root.path(),
            "conversation",
            "          - source: core_browser_endpoint\n          - source: core_browser_endpoint\n",
        );
        let duplicate = load_plugin_root_catalog(
            duplicate_root.path(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            22,
        );
        assert!(
            duplicate.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|message| message.contains("exactly one core_browser_endpoint"))
        );

        let missing_root = tempfile::tempdir().expect("create missing endpoint fixture");
        write_scoped_stdio_mcp_package(
            missing_root.path(),
            "conversation",
            "          - source: literal\n            value: --no-browser\n",
        );
        let missing =
            load_plugin_root_catalog(missing_root.path(), &BTreeSet::new(), &BTreeSet::new(), 24);
        assert!(
            missing.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|message| message.contains("exactly one core_browser_endpoint"))
        );

        let remote_root = tempfile::tempdir().expect("create remote conversation fixture");
        write_mcp_only_package(
            remote_root.path(),
            r#"permissions:
  - network_https
mcp:
  servers:
    - id: remote
      name: Remote
      scope: conversation
      transport:
        type: http
        url:
          source: literal
          value: https://example.test/mcp
"#,
        );
        let remote =
            load_plugin_root_catalog(remote_root.path(), &BTreeSet::new(), &BTreeSet::new(), 23);
        assert!(
            remote.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|message| message.contains("conversation scope requires stdio"))
        );
    }

    /// Verifies the unreleased Lua-centric `runtime` field is rejected by the strict initial schema.
    /// 验证严格的首发清单会拒绝未发布的 Lua 中心 `runtime` 字段。
    #[test]
    fn initial_schema_rejects_unreleased_runtime_field() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_mcp_only_package(
            temp.path(),
            "runtime:\n  bundle: runtime/plugin.bundle.lua\n",
        );
        let snapshot =
            load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 12);
        let plugin = &snapshot.plugins["mcp-plugin"];
        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(plugin.manifest.is_none());
        assert!(
            plugin
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("unknown field `runtime`"))
        );
    }

    /// Verifies Secret-backed MCP values require the exact host Secret permission.
    /// 验证 Secret 支撑的 MCP 值必须声明精确宿主 Secret 权限。
    #[test]
    fn mcp_secret_source_requires_secrets_read_permission() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_mcp_only_package(
            temp.path(),
            r#"permissions:
  - network_https
mcp:
  servers:
    - id: protected
      name: Protected
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
        let snapshot =
            load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 13);
        let plugin = &snapshot.plugins["mcp-plugin"];
        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(
            plugin
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("secrets_read"))
        );
    }

    /// Verifies static MCP validation rejects insecure literal URLs, invalid header names, and unapproved environment reads.
    /// 验证 MCP 静态校验会拒绝不安全字面量 URL、无效 Header 名与未授权环境变量读取。
    #[test]
    fn mcp_static_transport_validation_closes_security_boundaries() {
        let insecure_url_root = tempfile::tempdir().expect("create insecure URL fixture");
        write_mcp_only_package(
            insecure_url_root.path(),
            r#"permissions:
  - network_https
mcp:
  servers:
    - id: insecure
      name: Insecure
      transport:
        type: http
        url:
          source: literal
          value: http://example.test/mcp
"#,
        );
        let insecure_snapshot = load_plugin_root_catalog(
            insecure_url_root.path(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            14,
        );
        assert!(
            insecure_snapshot.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("credential-free, fragment-free HTTPS"))
        );

        let invalid_header_root = tempfile::tempdir().expect("create invalid header fixture");
        write_mcp_only_package(
            invalid_header_root.path(),
            r#"permissions:
  - network_https
mcp:
  servers:
    - id: invalid-header
      name: Invalid Header
      transport:
        type: http
        url:
          source: literal
          value: https://example.test/mcp
        headers:
          "bad header":
            source: literal
            value: value
"#,
        );
        let invalid_header_snapshot = load_plugin_root_catalog(
            invalid_header_root.path(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            15,
        );
        assert!(
            invalid_header_snapshot.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("header name"))
        );

        // credential_header_root declares an authentication Header in ordinary manifest text.
        // credential_header_root 在普通清单文本中声明认证 Header。
        let credential_header_root = tempfile::tempdir().expect("create credential header fixture");
        write_mcp_only_package(
            credential_header_root.path(),
            r#"permissions:
  - network_https
mcp:
  servers:
    - id: exposed-credential
      name: Exposed Credential
      transport:
        type: http
        url:
          source: literal
          value: https://example.test/mcp
        headers:
          Authorization:
            source: literal
            value: Bearer must-not-be-stored-here
"#,
        );
        // credential_header_snapshot proves manifest and ordinary configuration cannot retain known credentials.
        // credential_header_snapshot 证明清单与普通配置不能保留已知凭据。
        let credential_header_snapshot = load_plugin_root_catalog(
            credential_header_root.path(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            16,
        );
        assert!(
            credential_header_snapshot.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic
                    .contains("credential header `Authorization` must use the secret source"))
        );

        let environment_root = tempfile::tempdir().expect("create environment fixture");
        write_mcp_only_package(
            environment_root.path(),
            r#"permissions:
  - network_https
mcp:
  servers:
    - id: environment
      name: Environment
      transport:
        type: http
        url:
          source: environment
          name: PRIVATE_MCP_URL
"#,
        );
        let environment_snapshot = load_plugin_root_catalog(
            environment_root.path(),
            &BTreeSet::new(),
            &BTreeSet::new(),
            17,
        );
        assert!(
            environment_snapshot.plugins["mcp-plugin"]
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic
                    .contains("environment source requires permission `secrets_read`"))
        );
    }

    /// Verifies different plugins may contribute the same package-local component names.
    /// 验证不同插件可以贡献相同的包内组件名称。
    #[test]
    fn plugin_namespaces_isolate_identical_local_component_names() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_named_content_package(temp.path(), "first-plugin", "review-helper");
        write_named_content_package(temp.path(), "second-plugin", "review-helper");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 4);

        assert_eq!(snapshot.plugins.len(), 2);
        assert!(
            snapshot
                .enabled_skills
                .contains_key("first-plugin:review-helper")
        );
        assert!(
            snapshot
                .enabled_skills
                .contains_key("second-plugin:review-helper")
        );
        assert!(
            snapshot
                .enabled_agents
                .contains_key("first-plugin:reviewer")
        );
        assert!(
            snapshot
                .enabled_agents
                .contains_key("second-plugin:reviewer")
        );
    }

    /// Verifies an unresolved plugin-local skill binding rejects the whole package without partial publication.
    /// 验证无法解析的插件内技能绑定会拒绝整个插件包，且不会部分发布。
    #[test]
    fn content_package_rejects_missing_agent_skill_binding() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_content_package(temp.path(), "missing-helper");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 4);
        let plugin = &snapshot.plugins["content-plugin"];

        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(snapshot.enabled_skills.is_empty());
        assert!(snapshot.enabled_agents.is_empty());
        assert!(
            plugin
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("missing-helper"))
        );
    }

    /// Verifies a runtime-free package cannot smuggle execution capabilities beside content components.
    /// 验证无运行时插件不能在内容组件旁夹带执行能力。
    #[test]
    fn content_package_rejects_execution_declarations() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_content_package(temp.path(), "review-helper");
        fs::write(
            temp.path().join("content-plugin/plugin.yaml"),
            r#"schema_version: 1
id: content-plugin
version: 1.0.0
default_enabled: true
display:
  name: Content Plugin
  capability: Invalid execution declaration fixture
permissions:
  - runtime_lua_modules
"#,
        )
        .expect("write invalid content manifest");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 5);
        let plugin = &snapshot.plugins["content-plugin"];

        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(snapshot.enabled_skills.is_empty());
        assert!(snapshot.enabled_agents.is_empty());
        assert!(
            plugin
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.contains("requires a Lua runtime"))
        );
    }

    /// Verifies a runtime-free package must contribute at least one conventional content component.
    /// 验证无运行时插件必须至少贡献一个约定式内容组件。
    #[test]
    fn empty_content_package_is_unavailable() {
        let temp = tempfile::tempdir().expect("create temp directory");
        let package = temp.path().join("empty-plugin");
        fs::create_dir_all(&package).expect("create empty content package");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: empty-plugin
version: 1.0.0
default_enabled: true
display:
  name: Empty Plugin
  capability: Empty content fixture
"#,
        )
        .expect("write empty content manifest");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 6);
        let plugin = &snapshot.plugins["empty-plugin"];

        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(
            plugin
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic
                    .contains("at least one executable or content component"))
        );
    }

    /// Verifies a rejected package cannot reserve non-conflicting tool ids before a later package loads.
    /// 验证被拒插件无法在后续插件加载前占用其未冲突工具标识。
    #[test]
    fn rejected_package_does_not_leak_partial_global_tool_reservations() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_named_stateless_package(temp.path(), "a-plugin", &["shared_tool"]);
        write_named_stateless_package(
            temp.path(),
            "b-plugin",
            &["recoverable_tool", "shared_tool"],
        );
        write_named_stateless_package(temp.path(), "c-plugin", &["recoverable_tool"]);

        let snapshot =
            load_vulcan_plugin_catalog(temp.path(), None, &BTreeSet::new(), &BTreeSet::new(), 1);

        assert_eq!(
            snapshot.plugins["b-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert_eq!(
            snapshot.plugins["c-plugin"].availability,
            VulcanPluginAvailability::Enabled
        );
        assert!(snapshot.enabled_tools.contains_key("shared_tool"));
        assert!(snapshot.enabled_tools.contains_key("recoverable_tool"));
    }

    /// Verifies a user-disabled package contributes neither tools nor model guidance.
    /// 验证被用户停用的插件包既不贡献工具也不贡献模型说明。
    #[test]
    fn disabled_package_contributes_no_provider_surface() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let disabled = BTreeSet::from([String::from("test-plugin")]);
        let snapshot = load_plugin_root_catalog(temp.path(), &disabled, &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert!(snapshot.enabled_hooks.is_empty());
        assert!(snapshot.prompt_contributions.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Disabled
        );
    }

    /// Verifies an explicit enabled override activates a package whose manifest default is disabled.
    /// 验证显式启用覆盖可以激活清单默认停用的插件包。
    #[test]
    fn enabled_override_activates_default_disabled_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen("default_enabled: true", "default_enabled: false", 1);
        fs::write(manifest_path, manifest).expect("write opt-in manifest");

        let without_override =
            load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            without_override.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Disabled
        );
        assert!(without_override.enabled_tools.is_empty());
        assert!(without_override.enabled_hooks.is_empty());

        let enabled = BTreeSet::from([String::from("test-plugin")]);
        let with_override = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &enabled, 2);
        assert_eq!(
            with_override.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Enabled
        );
        assert_eq!(with_override.enabled_tools.len(), 1);
        assert_eq!(with_override.enabled_hooks.len(), 1);
        assert_eq!(with_override.prompt_contributions.len(), 1);
    }

    /// Verifies an incomplete stateful lifecycle is rejected before any tool becomes visible.
    /// 验证不完整的有状态生命周期会在任何工具可见前被拒绝。
    #[test]
    fn incomplete_stateful_tool_topology_is_rejected() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen("kind: stateless", "kind: session_open", 1);
        fs::write(manifest_path, manifest).expect("write incomplete lifecycle manifest");

        let snapshot =
            load_vulcan_plugin_catalog(temp.path(), None, &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("exactly one"));
    }

    /// Verifies an enable callback without disable compensation is rejected before activation.
    /// 验证缺少停用补偿的启用回调会在激活前被拒绝。
    #[test]
    fn enable_lifecycle_without_disable_compensation_is_rejected() {
        // temp owns the isolated package whose lifecycle declaration is changed.
        // temp 持有生命周期声明被修改的隔离插件包。
        let temp = tempfile::tempdir().expect("create lifecycle validation directory");
        write_valid_package(temp.path());
        // manifestPath identifies the exact fixture manifest changed below.
        // manifestPath 标识下方修改的精确测试清单。
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        // manifest appends an unsafe one-way enable callback declaration.
        // manifest 追加一个不安全的单向启用回调声明。
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read lifecycle fixture manifest")
            .replacen(
                "  max_active_sessions: 2\n",
                "  max_active_sessions: 2\n  lifecycle:\n    enable: on_enable\n",
                1,
            );
        fs::write(manifest_path, manifest).expect("write incomplete lifecycle callbacks");

        // snapshot must withhold every provider surface from the invalid package.
        // snapshot 必须阻止无效插件包贡献任何 Provider 能力面。
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("disable compensation"));
    }

    /// Verifies stateful call schemas must expose the exact required host session identifier.
    /// 验证有状态调用 Schema 必须暴露精确且必填的宿主会话标识。
    #[test]
    fn session_call_without_required_session_id_is_rejected() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen("kind: stateless", "kind: session_call", 1);
        fs::write(manifest_path, manifest).expect("write invalid call manifest");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("session_id"));
    }

    /// Verifies manifest timeout arithmetic is bounded before any runtime path can become stuck closing.
    /// 验证清单超时算术会在任何运行时路径进入关闭卡死状态前受到边界约束。
    #[test]
    fn overflowing_tool_timeout_is_rejected() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen(
                "timeout_ms: 1000",
                "timeout_ms: 18446744073709551615\n        timeout_grace_ms: 1",
                1,
            );
        fs::write(manifest_path, manifest).expect("write overflowing timeout manifest");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(
            snapshot.plugins["test-plugin"].diagnostics[0].contains("overflow"),
            "{:?}",
            snapshot.plugins["test-plugin"].diagnostics
        );
    }

    /// Verifies provider schemas cannot claim fields whose values are owned by host dispatch.
    /// 验证 Provider Schema 不能声明由宿主调度持有其值的字段。
    #[test]
    fn host_owned_payload_field_in_schema_is_rejected() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        fs::write(
            temp.path().join("test-plugin/schemas/open.json"),
            r#"{"type":"object","properties":{"request_id":{"type":"string"}}}"#,
        )
        .expect("write reserved payload schema");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("request_id"));
    }

    /// Verifies duplicate plugin-local hook identifiers reject the whole package.
    /// 验证重复的插件内 hook 标识会拒绝整个插件包。
    #[test]
    fn duplicate_hook_identifier_rejects_whole_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path).expect("read manifest");
        let duplicate = "    - id: precheck\n      point: conversation.turn.completed\n      mode: effect\n      function: on_precheck\n";
        let manifest = manifest.replace(
            "prompt_contributions:\n",
            &format!("{duplicate}prompt_contributions:\n"),
        );
        fs::write(manifest_path, manifest).expect("write duplicate hook manifest");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_hooks.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(
            snapshot.plugins["test-plugin"].diagnostics[0].contains("Duplicate hook id"),
            "{:?}",
            snapshot.plugins["test-plugin"].diagnostics
        );
    }

    /// Verifies unknown manifest schema versions are rejected deterministically.
    /// 验证未知清单结构版本会被确定性拒绝。
    #[test]
    fn unknown_manifest_schema_version_is_rejected() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen("schema_version: 1", "schema_version: 99", 1);
        fs::write(manifest_path, manifest).expect("write unsupported manifest");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("Unsupported"));
    }

    /// Verifies absolute package file paths cannot escape the trusted plugin root.
    /// 验证绝对插件文件路径不能逃逸可信插件根目录。
    #[test]
    fn absolute_schema_path_is_rejected() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let outside = temp.path().join("outside.json");
        fs::write(&outside, r#"{"type":"object"}"#).expect("write outside schema");
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen(
                "parameters_file: schemas/open.json",
                &format!(
                    "parameters_file: {}",
                    outside.to_string_lossy().replace('\\', "/")
                ),
                1,
            );
        fs::write(manifest_path, manifest).expect("write absolute schema manifest");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("must be relative"));
    }

    /// Verifies parent traversal cannot resolve a schema outside the package directory.
    /// 验证父目录穿越不能解析到插件目录之外的 Schema。
    #[test]
    fn parent_traversal_schema_path_is_rejected() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        fs::write(temp.path().join("outside.json"), r#"{"type":"object"}"#)
            .expect("write outside schema");
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen(
                "parameters_file: schemas/open.json",
                "parameters_file: ../outside.json",
                1,
            );
        fs::write(manifest_path, manifest).expect("write traversal manifest");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("escapes"));
    }

    /// Verifies package symbolic links are rejected before their targets can affect loading.
    /// 验证插件符号链接会在其目标影响加载前被拒绝。
    #[cfg(unix)]
    #[test]
    fn symbolic_link_package_entry_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let outside = temp.path().join("outside.txt");
        fs::write(&outside, "outside").expect("write outside file");
        symlink(&outside, temp.path().join("test-plugin/linked.txt"))
            .expect("create package symlink");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("symbolic link"));
    }

    /// Verifies a package-root symbolic link is rejected even when its target remains inside the catalog.
    /// 验证即使目标仍位于目录内部，插件包根符号链接也会被拒绝。
    #[cfg(unix)]
    #[test]
    fn symbolic_link_package_root_is_rejected() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("create temp directory");
        let storage_root = temp.path().join("storage");
        write_valid_package(&storage_root);
        symlink(
            storage_root.join("test-plugin"),
            temp.path().join("test-plugin"),
        )
        .expect("create package-root symlink");

        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("symbolic link"));
    }

    /// Verifies duplicate tool identifiers reject the complete atomic package.
    /// 验证重复工具标识会拒绝完整原子插件包。
    #[test]
    fn duplicate_tool_identifier_rejects_whole_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read manifest")
            .replacen(
                "  hooks:\n",
                "    - id: test_open\n      group: test_tools\n      description: Open one duplicate test session.\n      parameters_file: schemas/open.json\n      execution:\n        kind: session_open\n        operation: open\n        timeout_ms: 1000\n  hooks:\n",
                1,
            );
        fs::write(manifest_path, manifest).expect("write duplicate tool manifest");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("Duplicate tool id"));
    }

    /// Verifies a missing runtime bundle makes the whole plugin unavailable.
    /// 验证缺失运行时 bundle 会使整个插件不可用。
    #[test]
    fn missing_runtime_bundle_rejects_whole_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        fs::remove_file(temp.path().join("test-plugin/runtime/plugin.bundle.lua"))
            .expect("remove runtime bundle");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("Lua runtime bundle"));
    }

    /// Verifies a missing manifest-declared runtime asset makes the whole package unavailable.
    /// 验证清单声明的运行资产缺失时整个插件包会变为不可用。
    #[test]
    fn missing_required_runtime_file_rejects_whole_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        fs::remove_file(temp.path().join("test-plugin/runtime/required.txt"))
            .expect("remove required runtime asset");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("required plugin file"));
    }

    /// Verifies package code changes alter the fingerprint even when the semantic version is unchanged.
    /// 验证即使语义版本未变化，插件代码变化也会改变内容指纹。
    #[test]
    fn package_content_change_updates_runtime_fingerprint() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let first = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        let first_fingerprint = first.plugins["test-plugin"].content_fingerprint.clone();
        assert_eq!(first_fingerprint.len(), 64);
        assert!(
            first_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        fs::write(
            temp.path().join("test-plugin/runtime/plugin.bundle.lua"),
            "return { dispatch=function() return true end, shutdown=function() end, on_precheck=function() return '{\"ok\":true,\"result\":{}}' end }\n",
        )
        .expect("update runtime module");
        let second = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 2);
        assert_ne!(
            first_fingerprint,
            second.plugins["test-plugin"].content_fingerprint
        );
    }

    /// Verifies malformed schema files reject the whole atomic package.
    /// 验证格式错误的 Schema 文件会拒绝整个原子插件包。
    #[test]
    fn malformed_schema_rejects_whole_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        fs::write(
            temp.path().join("test-plugin/schemas/open.json"),
            "not-json",
        )
        .expect("corrupt schema");
        let snapshot = load_plugin_root_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
    }

    /// Verifies allowlisted Lua modules require both explicit capabilities and bundled trust.
    /// 验证白名单 Lua 模块同时要求显式能力与内建信任类型。
    #[test]
    fn allowlisted_runtime_lua_modules_load_for_bundled_plugin() {
        // root owns one isolated bundled plugin catalog.
        // root 持有一个隔离的内建插件目录。
        let root = tempfile::tempdir().expect("create plugin catalog root");
        write_valid_package(root.path());
        // manifestPath addresses the exact fixture manifest before discovery.
        // manifestPath 在目录发现前寻址精确测试清单。
        let manifest_path = root.path().join("test-plugin/plugin.yaml");
        // manifest adds the two required capabilities and all currently authorized modules.
        // manifest 增加两项必需能力与当前全部已授权模块。
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read fixture manifest")
            .replace(
                "lua_runtime:\n",
                "permissions:\n  - runtime_lua_modules\n  - network_https\nlua_runtime:\n  lua_modules: [lcurl.safe, openssl.cipher, openssl.rand]\n",
            );
        fs::write(&manifest_path, manifest).expect("write Lua module manifest");
        // snapshot proves catalog validation accepts the complete declaration.
        // snapshot 证明目录校验接受完整声明。
        let snapshot = load_plugin_root_catalog(root.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        let plugin = snapshot.plugins.get("test-plugin").expect("load plugin");
        assert_eq!(plugin.availability, VulcanPluginAvailability::Enabled);
        assert!(plugin.diagnostics.is_empty(), "{:?}", plugin.diagnostics);
    }

    /// Verifies unknown Lua modules cannot enter a fixed plugin bridge lease.
    /// 验证未知 Lua 模块无法进入固定插件桥租约。
    #[test]
    fn unknown_runtime_lua_module_rejects_package() {
        // root owns one isolated bundled plugin catalog.
        // root 持有一个隔离的内建插件目录。
        let root = tempfile::tempdir().expect("create plugin catalog root");
        write_valid_package(root.path());
        // manifestPath addresses the exact fixture manifest before discovery.
        // manifestPath 在目录发现前寻址精确测试清单。
        let manifest_path = root.path().join("test-plugin/plugin.yaml");
        // manifest requests one module absent from the host allowlist.
        // manifest 请求一个不存在于宿主白名单的模块。
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read fixture manifest")
            .replace(
                "lua_runtime:\n",
                "permissions:\n  - runtime_lua_modules\nlua_runtime:\n  lua_modules: [socket]\n",
            );
        fs::write(&manifest_path, manifest).expect("write unknown Lua module manifest");
        // snapshot preserves the unavailable descriptor and its exact diagnostic.
        // snapshot 保留不可用描述符及其精确诊断。
        let snapshot = load_plugin_root_catalog(root.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        let plugin = snapshot.plugins.get("test-plugin").expect("load plugin");
        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(plugin.diagnostics.iter().any(|diagnostic| diagnostic
            .contains("runtime Lua module `socket` is not allowed")));
    }

    /// Verifies module declarations cannot silently bypass capability authorization.
    /// 验证模块声明无法静默绕过能力授权。
    #[test]
    fn runtime_lua_modules_without_capability_reject_package() {
        // root owns one isolated bundled plugin catalog.
        // root 持有一个隔离的内建插件目录。
        let root = tempfile::tempdir().expect("create plugin catalog root");
        write_valid_package(root.path());
        // manifestPath addresses the exact fixture manifest before discovery.
        // manifestPath 在目录发现前寻址精确测试清单。
        let manifest_path = root.path().join("test-plugin/plugin.yaml");
        // manifest declares an allowed module but omits runtime_lua_modules.
        // manifest 声明一个允许模块，但省略 runtime_lua_modules 能力。
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read fixture manifest")
            .replace(
                "lua_runtime:\n",
                "lua_runtime:\n  lua_modules: [openssl.rand]\n",
            );
        fs::write(&manifest_path, manifest).expect("write unauthorized Lua module manifest");
        // snapshot preserves the unavailable descriptor and its exact diagnostic.
        // snapshot 保留不可用描述符及其精确诊断。
        let snapshot = load_plugin_root_catalog(root.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        let plugin = snapshot.plugins.get("test-plugin").expect("load plugin");
        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(plugin.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("lua_modules require capability `runtime_lua_modules`")
        }));
    }

    /// Verifies HTTPS-capable lua-cURL access cannot omit the visible network capability.
    /// 验证具备 HTTPS 能力的 lua-cURL 访问不能省略可见网络能力。
    #[test]
    fn lcurl_runtime_module_without_network_capability_rejects_package() {
        // root owns one isolated bundled plugin catalog.
        // root 持有一个隔离的内建插件目录。
        let root = tempfile::tempdir().expect("create plugin catalog root");
        write_valid_package(root.path());
        // manifestPath addresses the exact fixture manifest before discovery.
        // manifestPath 在目录发现前寻址精确测试清单。
        let manifest_path = root.path().join("test-plugin/plugin.yaml");
        // manifest requests lua-cURL while omitting network_https.
        // manifest 请求 lua-cURL，但省略 network_https。
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read fixture manifest")
            .replace(
                "lua_runtime:\n",
                "permissions:\n  - runtime_lua_modules\nlua_runtime:\n  lua_modules: [lcurl.safe]\n",
            );
        fs::write(&manifest_path, manifest).expect("write network capability fixture");
        // snapshot preserves the unavailable descriptor and its exact diagnostic.
        // snapshot 保留不可用描述符及其精确诊断。
        let snapshot = load_plugin_root_catalog(root.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        let plugin = snapshot.plugins.get("test-plugin").expect("load plugin");
        assert_eq!(plugin.availability, VulcanPluginAvailability::Unavailable);
        assert!(plugin.diagnostics.iter().any(|diagnostic| {
            diagnostic.contains("`lcurl.safe` requires capability `network_https`")
        }));
    }

    /// Verifies the first-release manifest rejects a package-owned origin field.
    /// 验证首发清单拒绝由插件包自行声明来源字段。
    #[test]
    fn manifest_rejects_package_owned_origin_kind() {
        let temp = tempfile::tempdir().expect("create origin validation directory");
        write_valid_package(temp.path());
        let manifest_path = temp.path().join("test-plugin/plugin.yaml");
        let manifest = fs::read_to_string(&manifest_path)
            .expect("read bundled manifest")
            .replacen(
                "version: 1.0.0\n",
                "version: 1.0.0\nkind: user_installed\n",
                1,
            );
        fs::write(manifest_path, manifest).expect("write forbidden origin field");
        let snapshot =
            load_vulcan_plugin_catalog(temp.path(), None, &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            VulcanPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("kind"));
    }

    /// Verifies a bundled package deterministically shadows the same authorized user package identity.
    /// 验证内建插件包会确定性覆盖同一身份的已授权用户插件包。
    #[test]
    fn bundled_plugin_identity_shadows_user_plugin_identity() {
        let temp = tempfile::tempdir().expect("create precedence validation directory");
        let bundled_root = temp.path().join("bundled");
        let user_root = temp.path().join("user");
        fs::create_dir_all(&bundled_root).expect("create bundled root");
        fs::create_dir_all(&user_root).expect("create user root");
        write_valid_package(&bundled_root);
        write_valid_package(&user_root);
        let snapshot = load_vulcan_plugin_catalog(
            &bundled_root,
            Some(&user_root),
            &BTreeSet::new(),
            &BTreeSet::new(),
            1,
        );
        assert_eq!(
            snapshot.plugins["test-plugin"].origin,
            VulcanPluginPackageOrigin::Bundled
        );
        assert!(
            snapshot
                .diagnostics
                .iter()
                .any(|issue| issue.contains("shadowed"))
        );
    }

    /// Verifies every repository-bundled package satisfies the same production loader contract.
    /// 验证仓库内建的每个插件包都满足同一套生产加载器契约。
    #[test]
    fn bundled_vulcan_plugin_packages_pass_production_catalog_validation() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = crate_root
            .parent()
            .and_then(Path::parent)
            .expect("resolve workspace root");
        let snapshot = load_plugin_root_catalog(
            &workspace_root.join("plugins"),
            &BTreeSet::new(),
            &BTreeSet::new(),
            1,
        );
        assert!(!snapshot.plugins.is_empty());
        assert!(
            snapshot.plugins.values().all(|plugin| {
                // The validated manifest is authoritative for the package's default state.
                // 已校验清单是插件包默认状态的权威来源。
                let manifest = plugin
                    .manifest
                    .as_ref()
                    .expect("validated bundled manifest");
                plugin.availability
                    == if manifest.default_enabled {
                        VulcanPluginAvailability::Enabled
                    } else {
                        VulcanPluginAvailability::Disabled
                    }
            }),
            "bundled plugin validation failed: {:?}",
            snapshot
                .plugins
                .values()
                .map(|plugin| (&plugin.id, &plugin.diagnostics))
                .collect::<Vec<_>>()
        );
        assert!(!snapshot.enabled_tools.is_empty());
    }

    /// Verifies platform exclusion uses the exact compile-time Cargo target on every host OS.
    /// 验证平台排除逻辑会在每种宿主操作系统上使用精确的编译期 Cargo 目标。
    #[test]
    fn platform_exclusion_matches_compile_time_target() {
        let current_target = String::from(env!("VULCAN_PLUGIN_RUNTIME_TARGET"));
        assert!(current_platform_is_excluded(&[current_target]));
        assert!(!current_platform_is_excluded(&[String::from(
            "never-matches-current-target"
        )]));
    }
}
