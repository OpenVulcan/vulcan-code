//! Secure package discovery and immutable System Plugin catalog snapshots.
//! 安全的插件包发现与不可变系统插件目录快照。

use crate::{
    SYSTEM_PLUGIN_MANIFEST_SCHEMA_VERSION, SYSTEM_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS,
    SYSTEM_PLUGIN_SESSION_ID_ARGUMENT, SystemPluginHookManifest, SystemPluginKind,
    SystemPluginLocaleManifest, SystemPluginManifest, SystemPluginPromptManifest,
    SystemPluginToolExecutionKind, SystemPluginToolManifest,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Fixed package manifest file name recognized by the host.
/// 宿主识别的固定插件包清单文件名。
pub const SYSTEM_PLUGIN_MANIFEST_FILE: &str = "plugin.yaml";

/// Effective package availability after platform and user policy projection.
/// 应用平台与用户策略投影后的插件包有效可用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemPluginAvailability {
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
pub enum SystemPluginRuntimeState {
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
    /// Plugin package or dependency validation failed.
    /// 插件包或依赖校验失败。
    Unavailable,
}

/// One validated model-visible tool definition loaded from a plugin package.
/// 从插件包加载并通过校验的单个模型可见工具定义。
#[derive(Debug, Clone)]
pub struct SystemPluginToolDescriptor {
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
    pub manifest: SystemPluginToolManifest,
}

/// One validated Lua hook declaration loaded from a plugin package.
/// 从插件包加载并通过校验的单个 Lua hook 声明。
#[derive(Debug, Clone)]
pub struct SystemPluginHookDescriptor {
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
    pub manifest: SystemPluginHookManifest,
}

/// One validated plugin-level model instruction contribution.
/// 单个通过校验的插件级模型说明贡献。
#[derive(Debug, Clone)]
pub struct SystemPluginPromptContribution {
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
pub struct SystemPluginDescriptor {
    /// Stable package identifier, including invalid package directory fallback.
    /// 稳定插件包标识，包含无效包的目录名回退。
    pub id: String,
    /// Package semantic version when the manifest is valid.
    /// 清单有效时的插件包语义版本。
    pub version: Option<String>,
    /// Canonical package root.
    /// 规范化插件包根目录。
    pub package_root: PathBuf,
    /// Stable in-process fingerprint of every distributable package file.
    /// 插件全部可分发文件的稳定进程内指纹。
    pub content_fingerprint: u64,
    /// Effective availability.
    /// 有效可用状态。
    pub availability: SystemPluginAvailability,
    /// Desired enabled state after default and user policy projection.
    /// 应用默认值与用户策略投影后的期望启用状态。
    pub desired_enabled: bool,
    /// Current host lifecycle state.
    /// 当前宿主生命周期状态。
    pub runtime_state: SystemPluginRuntimeState,
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
    pub locales: BTreeMap<String, SystemPluginLocaleManifest>,
    /// Validated atomic tool definitions.
    /// 通过校验的原子工具定义集合。
    pub tools: Vec<SystemPluginToolDescriptor>,
    /// Validated Lua hook declarations.
    /// 通过校验的 Lua hook 声明集合。
    pub hooks: Vec<SystemPluginHookDescriptor>,
    /// Validated prompt contributions.
    /// 通过校验的提示词贡献集合。
    pub prompt_contributions: Vec<SystemPluginPromptContribution>,
    /// Valid manifest used by the generic runtime.
    /// 通用运行时使用的有效清单。
    pub manifest: Option<Arc<SystemPluginManifest>>,
    /// Deterministic validation diagnostics.
    /// 确定性的校验诊断集合。
    pub diagnostics: Vec<String>,
}

impl SystemPluginDescriptor {
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
pub struct SystemPluginCatalogSnapshot {
    /// Monotonic in-process revision assigned by the manager.
    /// 由管理器分配的进程内单调修订号。
    pub revision: u64,
    /// Canonical System Plugin root scanned for this snapshot.
    /// 当前快照扫描的规范化系统插件根目录。
    pub root: PathBuf,
    /// Discovered plugins keyed by stable identifier.
    /// 以稳定标识为键的已发现插件集合。
    pub plugins: BTreeMap<String, SystemPluginDescriptor>,
    /// Enabled tools keyed by globally unique tool identifier.
    /// 以全局唯一工具标识为键的已启用工具集合。
    pub enabled_tools: BTreeMap<String, SystemPluginToolDescriptor>,
    /// Enabled hook declarations keyed by globally unique registration identifier.
    /// 以全局唯一注册标识为键的已启用 hook 声明集合。
    pub enabled_hooks: BTreeMap<String, SystemPluginHookDescriptor>,
    /// Enabled plugin-level prompt contributions.
    /// 已启用的插件级提示词贡献集合。
    pub prompt_contributions: Vec<SystemPluginPromptContribution>,
    /// Catalog-level deterministic diagnostics.
    /// 目录级确定性诊断集合。
    pub diagnostics: Vec<String>,
}

impl SystemPluginCatalogSnapshot {
    /// Builds one empty snapshot before first discovery succeeds.
    /// 在首次发现成功前构建一个空快照。
    ///
    /// # Parameters
    /// 参数
    /// `root` is the trusted System Plugin catalog root.
    /// `root` 是受信任的系统插件目录根。
    ///
    /// # Returns
    /// 返回值
    /// Returns an empty catalog snapshot with revision zero.
    /// 返回修订号为零的空目录快照。
    pub fn empty(root: PathBuf) -> Self {
        Self {
            revision: 0,
            root,
            plugins: BTreeMap::new(),
            enabled_tools: BTreeMap::new(),
            enabled_hooks: BTreeMap::new(),
            prompt_contributions: Vec::new(),
            diagnostics: Vec::new(),
        }
    }
}

/// Discovers and validates every direct child package under one trusted System Plugin root.
/// 在一个可信系统插件根目录下发现并校验所有直接子插件包。
///
/// # Parameters
/// 参数
/// `root` is the catalog root, policy sets contain explicit overrides, and `revision` labels the result.
/// `root` 是目录根，策略集合保存显式覆盖，`revision` 标记生成结果。
///
/// # Returns
/// 返回值
/// Returns a deterministic snapshot with valid packages and rejection diagnostics.
/// 返回包含有效插件及拒绝诊断的确定性快照。
pub fn load_system_plugin_catalog(
    root: &Path,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
    revision: u64,
) -> SystemPluginCatalogSnapshot {
    let canonical_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let mut snapshot = SystemPluginCatalogSnapshot::empty(canonical_root.clone());
    snapshot.revision = revision;
    let entries = match fs::read_dir(&canonical_root) {
        Ok(entries) => entries,
        Err(error) => {
            snapshot.diagnostics.push(format!(
                "Failed to read System Plugin root `{}`: {error}",
                canonical_root.display()
            ));
            return snapshot;
        }
    };
    let mut package_roots = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir() && path.join(SYSTEM_PLUGIN_MANIFEST_FILE).is_file())
        .collect::<Vec<_>>();
    package_roots.sort();
    let mut global_tool_ids = BTreeSet::new();
    for package_root in package_roots {
        let descriptor = load_system_plugin_package(
            &canonical_root,
            &package_root,
            disabled_plugin_ids,
            enabled_plugin_ids,
        );
        let plugin_id = descriptor.id.clone();
        if snapshot.plugins.contains_key(&plugin_id) {
            snapshot.diagnostics.push(format!(
                "Duplicate System Plugin id `{plugin_id}` was ignored."
            ));
            continue;
        }
        if descriptor.availability == SystemPluginAvailability::Enabled {
            let duplicate_tool = descriptor
                .tools
                .iter()
                .find(|tool| global_tool_ids.contains(&tool.id))
                .map(|tool| tool.id.clone());
            if let Some(tool_id) = duplicate_tool {
                let mut unavailable = descriptor;
                unavailable.availability = SystemPluginAvailability::Unavailable;
                unavailable.runtime_state = SystemPluginRuntimeState::Unavailable;
                unavailable.diagnostics.push(format!(
                    "Tool id `{tool_id}` conflicts with another enabled System Plugin."
                ));
                snapshot.plugins.insert(plugin_id, unavailable);
                continue;
            }
            global_tool_ids.extend(descriptor.tools.iter().map(|tool| tool.id.clone()));
            for tool in &descriptor.tools {
                snapshot.enabled_tools.insert(tool.id.clone(), tool.clone());
            }
            for hook in &descriptor.hooks {
                snapshot
                    .enabled_hooks
                    .insert(hook.registration_id.clone(), hook.clone());
            }
            snapshot
                .prompt_contributions
                .extend(descriptor.prompt_contributions.iter().cloned());
        }
        snapshot.plugins.insert(plugin_id, descriptor);
    }
    snapshot
}

/// Loads and validates one exact package without allowing path escape.
/// 加载并校验一个精确插件包，同时禁止路径逃逸。
fn load_system_plugin_package(
    catalog_root: &Path,
    package_root: &Path,
    disabled_plugin_ids: &BTreeSet<String>,
    enabled_plugin_ids: &BTreeSet<String>,
) -> SystemPluginDescriptor {
    let canonical_package_root = package_root
        .canonicalize()
        .unwrap_or_else(|_| package_root.to_path_buf());
    let fallback_id = package_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("invalid-system-plugin")
        .to_owned();
    let unavailable = |diagnostic: String| SystemPluginDescriptor {
        id: fallback_id.clone(),
        version: None,
        package_root: canonical_package_root.clone(),
        content_fingerprint: 0,
        availability: SystemPluginAvailability::Unavailable,
        desired_enabled: false,
        runtime_state: SystemPluginRuntimeState::Unavailable,
        generation: 0,
        active_session_count: 0,
        name: fallback_id.clone(),
        capability: String::from("System Plugin package is unavailable."),
        locales: BTreeMap::new(),
        tools: Vec::new(),
        hooks: Vec::new(),
        prompt_contributions: Vec::new(),
        manifest: None,
        diagnostics: vec![diagnostic],
    };
    if !canonical_package_root.starts_with(catalog_root) {
        return unavailable(String::from(
            "System Plugin package root escapes the trusted catalog root.",
        ));
    }
    let manifest_path = canonical_package_root.join(SYSTEM_PLUGIN_MANIFEST_FILE);
    let manifest_text = match fs::read_to_string(&manifest_path) {
        Ok(text) => text,
        Err(error) => return unavailable(format!("Failed to read plugin manifest: {error}")),
    };
    let manifest: SystemPluginManifest = match serde_yaml::from_str(&manifest_text) {
        Ok(manifest) => manifest,
        Err(error) => return unavailable(format!("Failed to decode plugin manifest: {error}")),
    };
    if let Err(error) = validate_manifest_identity(&manifest, &fallback_id) {
        return unavailable(error);
    }
    let mut descriptor = SystemPluginDescriptor {
        id: manifest.id.clone(),
        version: Some(manifest.version.clone()),
        package_root: canonical_package_root.clone(),
        content_fingerprint: 0,
        availability: SystemPluginAvailability::Enabled,
        desired_enabled: if enabled_plugin_ids.contains(&manifest.id) {
            true
        } else if disabled_plugin_ids.contains(&manifest.id) {
            false
        } else {
            manifest.default_enabled
        },
        runtime_state: SystemPluginRuntimeState::Enabled,
        generation: 0,
        active_session_count: 0,
        name: manifest.display.name.clone(),
        capability: manifest.display.capability.clone(),
        locales: BTreeMap::new(),
        tools: Vec::new(),
        hooks: Vec::new(),
        prompt_contributions: Vec::new(),
        manifest: Some(Arc::new(manifest.clone())),
        diagnostics: Vec::new(),
    };
    if current_platform_is_excluded(&manifest.platforms.excluded_targets) {
        descriptor.availability = SystemPluginAvailability::UnsupportedPlatform;
        descriptor.runtime_state = SystemPluginRuntimeState::UnsupportedPlatform;
        descriptor.diagnostics.push(String::from(
            "System Plugin is not supported on the current target platform.",
        ));
        return descriptor;
    }
    if !descriptor.desired_enabled {
        descriptor.availability = SystemPluginAvailability::Disabled;
        descriptor.runtime_state = SystemPluginRuntimeState::Disabled;
    }
    if let Err(error) = validate_runtime_files(&canonical_package_root, &manifest) {
        descriptor.availability = SystemPluginAvailability::Unavailable;
        descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
        descriptor.diagnostics.push(error);
        return descriptor;
    }
    descriptor.content_fingerprint = match compute_package_fingerprint(&canonical_package_root) {
        Ok(fingerprint) => fingerprint,
        Err(error) => {
            descriptor.availability = SystemPluginAvailability::Unavailable;
            descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(error);
            return descriptor;
        }
    };
    let mut tool_ids = BTreeSet::new();
    let mut groups = BTreeSet::new();
    for tool in &manifest.tools {
        if !tool_ids.insert(tool.id.clone()) {
            descriptor.availability = SystemPluginAvailability::Unavailable;
            descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
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
                descriptor.availability = SystemPluginAvailability::Unavailable;
                descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(error);
                return descriptor;
            }
        };
        let parameters_schema = match read_json_file(&parameters_path) {
            Ok(schema) if schema.is_object() => schema,
            Ok(_) => {
                descriptor.availability = SystemPluginAvailability::Unavailable;
                descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(format!(
                    "Tool `{}` parameters schema must be a JSON object.",
                    tool.id
                ));
                return descriptor;
            }
            Err(error) => {
                descriptor.availability = SystemPluginAvailability::Unavailable;
                descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(error);
                return descriptor;
            }
        };
        if let Err(error) = validate_tool_parameters_schema(tool, &parameters_schema) {
            descriptor.availability = SystemPluginAvailability::Unavailable;
            descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(error);
            return descriptor;
        }
        descriptor.tools.push(SystemPluginToolDescriptor {
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
        descriptor.availability = SystemPluginAvailability::Unavailable;
        descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
        descriptor.diagnostics.push(String::from(
            "A bundled System Plugin with tools must declare exactly one atomic tool group.",
        ));
        return descriptor;
    }
    if let Err(error) = validate_tool_execution_topology(&manifest.tools) {
        descriptor.availability = SystemPluginAvailability::Unavailable;
        descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
        descriptor.diagnostics.push(error);
        return descriptor;
    }
    let mut hook_ids = BTreeSet::new();
    for hook in &manifest.hooks {
        if !hook_ids.insert(hook.id.clone()) {
            descriptor.availability = SystemPluginAvailability::Unavailable;
            descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(format!(
                "Duplicate hook id `{}` in plugin manifest.",
                hook.id
            ));
            return descriptor;
        }
        if let Err(error) = validate_hook_manifest(hook) {
            descriptor.availability = SystemPluginAvailability::Unavailable;
            descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(error);
            return descriptor;
        }
        descriptor.hooks.push(SystemPluginHookDescriptor {
            plugin_id: manifest.id.clone(),
            plugin_generation: 0,
            registration_id: format!("system-plugin/{}/{}", manifest.id, hook.id),
            manifest: hook.clone(),
        });
    }
    let mut prompt_ids = BTreeSet::new();
    for prompt in &manifest.prompt_contributions {
        if !prompt_ids.insert(prompt.id.clone()) {
            descriptor.availability = SystemPluginAvailability::Unavailable;
            descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
            descriptor.diagnostics.push(format!(
                "Duplicate prompt contribution id `{}` in plugin manifest.",
                prompt.id
            ));
            return descriptor;
        }
        match load_prompt_contribution(&canonical_package_root, &manifest.id, prompt, &tool_ids) {
            Ok(contribution) => descriptor.prompt_contributions.push(contribution),
            Err(error) => {
                descriptor.availability = SystemPluginAvailability::Unavailable;
                descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
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
                descriptor.availability = SystemPluginAvailability::Unavailable;
                descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
                descriptor.diagnostics.push(error);
                return descriptor;
            }
        };
        let locale_text = match fs::read_to_string(&locale_path) {
            Ok(text) => text,
            Err(error) => {
                descriptor.availability = SystemPluginAvailability::Unavailable;
                descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
                descriptor
                    .diagnostics
                    .push(format!("Failed to read plugin locale `{locale}`: {error}"));
                return descriptor;
            }
        };
        let localized = match serde_yaml::from_str::<SystemPluginLocaleManifest>(&locale_text) {
            Ok(localized) => localized,
            Err(error) => {
                descriptor.availability = SystemPluginAvailability::Unavailable;
                descriptor.runtime_state = SystemPluginRuntimeState::Unavailable;
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

/// Validates version, identity, trust kind, and identifier grammar.
/// 校验版本、身份、信任类型与标识符语法。
fn validate_manifest_identity(
    manifest: &SystemPluginManifest,
    package_directory_id: &str,
) -> Result<(), String> {
    if manifest.schema_version != SYSTEM_PLUGIN_MANIFEST_SCHEMA_VERSION {
        return Err(format!(
            "Unsupported System Plugin manifest schema version `{}`.",
            manifest.schema_version
        ));
    }
    if manifest.kind != SystemPluginKind::BundledSystem {
        return Err(String::from(
            "Only bundled System Plugins are trusted by this host.",
        ));
    }
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
    if manifest.runtime.max_active_sessions == 0 {
        return Err(String::from(
            "Plugin max_active_sessions must be greater than zero.",
        ));
    }
    if manifest.runtime.session_idle_timeout_sec == Some(0) {
        return Err(String::from(
            "Plugin session_idle_timeout_sec must be greater than zero when declared.",
        ));
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
    validate_lua_path(&manifest.runtime.module, "runtime module")?;
    validate_lua_identifier(&manifest.runtime.invoke_function, "runtime invoke function")?;
    validate_lua_identifier(
        &manifest.runtime.shutdown_function,
        "runtime shutdown function",
    )?;
    if let Some(function) = &manifest.runtime.hook_shutdown_function {
        validate_lua_identifier(function, "runtime hook shutdown function")?;
    }
    for tool in &manifest.tools {
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

/// Validates one declarative Lua hook before host registration.
/// 在宿主注册前校验一个声明式 Lua hook。
fn validate_hook_manifest(hook: &SystemPluginHookManifest) -> Result<(), String> {
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
    tool: &SystemPluginToolManifest,
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
    if let Some(reserved) = SYSTEM_PLUGIN_RESERVED_PAYLOAD_ARGUMENTS
        .iter()
        .find(|reserved| properties.contains_key(**reserved) || required.contains(**reserved))
    {
        return Err(format!(
            "Tool `{}` must not declare host-owned payload field `{reserved}`.",
            tool.id
        ));
    }
    let has_session_id = properties.contains_key(SYSTEM_PLUGIN_SESSION_ID_ARGUMENT);
    let requires_session_id = required.contains(SYSTEM_PLUGIN_SESSION_ID_ARGUMENT);
    match tool.execution.kind {
        SystemPluginToolExecutionKind::Stateless | SystemPluginToolExecutionKind::SessionOpen => {
            if has_session_id || requires_session_id {
                return Err(format!(
                    "Tool `{}` must not declare the host-owned session_id argument.",
                    tool.id
                ));
            }
        }
        SystemPluginToolExecutionKind::SessionCall
        | SystemPluginToolExecutionKind::SessionClose => {
            let session_schema = properties
                .get(SYSTEM_PLUGIN_SESSION_ID_ARGUMENT)
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
    if tool.execution.kind == SystemPluginToolExecutionKind::SessionClose
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
fn validate_tool_execution_topology(tools: &[SystemPluginToolManifest]) -> Result<(), String> {
    let open_count = tools
        .iter()
        .filter(|tool| tool.execution.kind == SystemPluginToolExecutionKind::SessionOpen)
        .count();
    let call_count = tools
        .iter()
        .filter(|tool| tool.execution.kind == SystemPluginToolExecutionKind::SessionCall)
        .count();
    let close_count = tools
        .iter()
        .filter(|tool| tool.execution.kind == SystemPluginToolExecutionKind::SessionClose)
        .count();
    let has_stateful_tools = open_count + call_count + close_count > 0;
    if has_stateful_tools && (open_count != 1 || close_count != 1) {
        return Err(format!(
            "A stateful System Plugin must declare exactly one session-open tool and one session-close tool; found {open_count} open and {close_count} close tools."
        ));
    }
    Ok(())
}

/// Validates dependency and Lua module files required for lease creation.
/// 校验创建租约所需的依赖文件与 Lua 模块文件。
fn validate_runtime_files(
    package_root: &Path,
    manifest: &SystemPluginManifest,
) -> Result<(), String> {
    resolve_package_file(
        package_root,
        &manifest.runtime.dependencies_file,
        "runtime dependency manifest",
    )?;
    let module_path = format!("{}.lua", manifest.runtime.module.replace('.', "/"));
    resolve_package_file(package_root, &module_path, "runtime Lua module")?;
    let unique_required_files = manifest
        .runtime
        .required_files
        .iter()
        .collect::<BTreeSet<_>>();
    if unique_required_files.len() != manifest.runtime.required_files.len() {
        return Err(String::from(
            "System Plugin runtime required_files contains duplicate paths.",
        ));
    }
    for required_file in &manifest.runtime.required_files {
        resolve_package_file(package_root, required_file, "required runtime file")?;
    }
    Ok(())
}

/// Loads one prompt contribution and verifies its atomic tool dependency set.
/// 加载单个提示词贡献并校验其原子工具依赖集合。
fn load_prompt_contribution(
    package_root: &Path,
    plugin_id: &str,
    prompt: &SystemPluginPromptManifest,
    tool_ids: &BTreeSet<String>,
) -> Result<SystemPluginPromptContribution, String> {
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
    Ok(SystemPluginPromptContribution {
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
        return Err(format!("System Plugin {label} path must be relative."));
    }
    let candidate = package_root.join(relative);
    let canonical = candidate
        .canonicalize()
        .map_err(|error| format!("Failed to resolve System Plugin {label}: {error}"))?;
    if !canonical.starts_with(package_root) || !canonical.is_file() {
        return Err(format!(
            "System Plugin {label} escapes the package or is not a file."
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

/// Computes a deterministic FNV-1a fingerprint over every regular package file.
/// 对插件内每个普通文件计算确定性的 FNV-1a 指纹。
fn compute_package_fingerprint(package_root: &Path) -> Result<u64, String> {
    let mut files = Vec::new();
    collect_package_files(package_root, package_root, &mut files)?;
    files.sort();
    let mut fingerprint = 0xcbf29ce484222325_u64;
    for file in files {
        let relative = file
            .strip_prefix(package_root)
            .map_err(|error| format!("Failed to derive System Plugin fingerprint path: {error}"))?;
        update_fnv1a(&mut fingerprint, relative.to_string_lossy().as_bytes());
        update_fnv1a(&mut fingerprint, &[0]);
        let bytes = fs::read(&file).map_err(|error| {
            format!(
                "Failed to read System Plugin fingerprint file `{}`: {error}",
                file.display()
            )
        })?;
        update_fnv1a(&mut fingerprint, &bytes);
        update_fnv1a(&mut fingerprint, &[0xff]);
    }
    Ok(fingerprint)
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
                "Failed to read System Plugin package directory `{}`: {error}",
                directory.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to enumerate System Plugin package: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "Failed to inspect System Plugin package entry `{}`: {error}",
                path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "System Plugin package entry `{}` must not be a symbolic link.",
                path.display()
            ));
        }
        if metadata.is_dir() {
            collect_package_files(package_root, &path, files)?;
        } else if metadata.is_file() {
            let canonical = path.canonicalize().map_err(|error| {
                format!(
                    "Failed to canonicalize System Plugin package entry `{}`: {error}",
                    path.display()
                )
            })?;
            if !canonical.starts_with(package_root) {
                return Err(format!(
                    "System Plugin package entry `{}` escapes the trusted package root.",
                    path.display()
                ));
            }
            files.push(canonical);
        }
    }
    Ok(())
}

/// Updates one FNV-1a hash state with an exact byte slice.
/// 使用精确字节切片更新一个 FNV-1a 哈希状态。
fn update_fnv1a(fingerprint: &mut u64, bytes: &[u8]) {
    for byte in bytes {
        *fingerprint ^= u64::from(*byte);
        *fingerprint = fingerprint.wrapping_mul(0x100000001b3);
    }
}

/// Validates one lowercase kebab/snake identifier used in host dispatch.
/// 校验一个用于宿主调度的小写短横线或下划线标识符。
fn validate_identifier(value: &str, label: &str) -> Result<(), String> {
    let valid = !value.is_empty()
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_'
        })
        && value.as_bytes().first().is_some_and(u8::is_ascii_lowercase);
    if valid {
        Ok(())
    } else {
        Err(format!("Invalid System Plugin {label} `{value}`."))
    }
}

/// Validates one dot-separated Lua module path.
/// 校验一个以点分隔的 Lua 模块路径。
fn validate_lua_path(value: &str, label: &str) -> Result<(), String> {
    if value
        .split('.')
        .all(|segment| validate_lua_identifier(segment, label).is_ok())
    {
        Ok(())
    } else {
        Err(format!("Invalid System Plugin {label} `{value}`."))
    }
}

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
        Err(format!("Invalid System Plugin {label} `{value}`."))
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
            "Invalid System Plugin excluded target triple `{target}`."
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
            package.join("runtime/plugin.lua"),
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
kind: bundled_system
default_enabled: true
display:
  name: Test Plugin
  capability: Test capability
runtime:
  dependencies_file: dependencies.yaml
  required_files: [runtime/required.txt]
  module: runtime.plugin
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

    /// Writes one named stateless package with an exact ordered tool set for collision tests.
    /// 为冲突测试写入一个带精确有序工具集合的命名无状态插件包。
    ///
    /// # Parameters
    /// 参数
    /// `root` is the catalog root, `plugin_id` is the package identity, and `tool_ids` preserves manifest order.
    /// `root` 是目录根，`plugin_id` 是插件身份，`tool_ids` 保持清单顺序。
    fn write_named_stateless_package(root: &Path, plugin_id: &str, tool_ids: &[&str]) {
        let package = root.join(plugin_id);
        fs::create_dir_all(package.join("runtime")).expect("create named runtime directory");
        fs::create_dir_all(package.join("schemas")).expect("create named schema directory");
        fs::write(package.join("dependencies.yaml"), "dependencies: []\n")
            .expect("write named dependencies");
        fs::write(
            package.join("runtime/plugin.lua"),
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
                    "  - id: {tool_id}\n    group: collision_tools\n    description: Run one collision fixture operation.\n    parameters_file: schemas/stateless.json\n    execution:\n      kind: stateless\n      operation: run\n      timeout_ms: 1000\n"
                )
            })
            .collect::<String>();
        fs::write(
            package.join("plugin.yaml"),
            format!(
                "schema_version: 1\nid: {plugin_id}\nversion: 1.0.0\nkind: bundled_system\ndefault_enabled: true\ndisplay:\n  name: {plugin_id}\n  capability: Collision fixture\nruntime:\n  dependencies_file: dependencies.yaml\n  module: runtime.plugin\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\ntools:\n{tools}"
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
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 7);
        assert_eq!(snapshot.revision, 7);
        assert_eq!(snapshot.enabled_tools.len(), 1);
        assert_eq!(snapshot.enabled_hooks.len(), 1);
        assert_eq!(snapshot.prompt_contributions.len(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Enabled
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
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);

        assert_eq!(
            snapshot.plugins["b-plugin"].availability,
            SystemPluginAvailability::Unavailable
        );
        assert_eq!(
            snapshot.plugins["c-plugin"].availability,
            SystemPluginAvailability::Enabled
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
        let snapshot = load_system_plugin_catalog(temp.path(), &disabled, &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert!(snapshot.enabled_hooks.is_empty());
        assert!(snapshot.prompt_contributions.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Disabled
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
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            without_override.plugins["test-plugin"].availability,
            SystemPluginAvailability::Disabled
        );
        assert!(without_override.enabled_tools.is_empty());
        assert!(without_override.enabled_hooks.is_empty());

        let enabled = BTreeSet::from([String::from("test-plugin")]);
        let with_override = load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &enabled, 2);
        assert_eq!(
            with_override.plugins["test-plugin"].availability,
            SystemPluginAvailability::Enabled
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
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("exactly one"));
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

        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
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
                "timeout_ms: 18446744073709551615\n      timeout_grace_ms: 1",
                1,
            );
        fs::write(manifest_path, manifest).expect("write overflowing timeout manifest");

        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("overflow"));
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

        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
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
        let duplicate = "  - id: precheck\n    point: conversation.turn.completed\n    mode: effect\n    function: on_precheck\n";
        let manifest = manifest.replace(
            "prompt_contributions:\n",
            &format!("{duplicate}prompt_contributions:\n"),
        );
        fs::write(manifest_path, manifest).expect("write duplicate hook manifest");
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_hooks.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("Duplicate hook id"));
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
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
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
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
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
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
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
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
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
                "hooks:\n",
                "  - id: test_open\n    group: test_tools\n    description: Open one duplicate test session.\n    parameters_file: schemas/open.json\n    execution:\n      kind: session_open\n      operation: open\n      timeout_ms: 1000\nhooks:\n",
                1,
            );
        fs::write(manifest_path, manifest).expect("write duplicate tool manifest");
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("Duplicate tool id"));
    }

    /// Verifies a missing runtime module makes the whole plugin unavailable.
    /// 验证缺失运行时模块会使整个插件不可用。
    #[test]
    fn missing_runtime_module_rejects_whole_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        fs::remove_file(temp.path().join("test-plugin/runtime/plugin.lua"))
            .expect("remove runtime module");
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("runtime Lua module"));
    }

    /// Verifies a missing manifest-declared runtime asset makes the whole package unavailable.
    /// 验证清单声明的运行资产缺失时整个插件包会变为不可用。
    #[test]
    fn missing_required_runtime_file_rejects_whole_package() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        fs::remove_file(temp.path().join("test-plugin/runtime/required.txt"))
            .expect("remove required runtime asset");
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
        );
        assert!(snapshot.plugins["test-plugin"].diagnostics[0].contains("required runtime file"));
    }

    /// Verifies package code changes alter the fingerprint even when the semantic version is unchanged.
    /// 验证即使语义版本未变化，插件代码变化也会改变内容指纹。
    #[test]
    fn package_content_change_updates_runtime_fingerprint() {
        let temp = tempfile::tempdir().expect("create temp directory");
        write_valid_package(temp.path());
        let first = load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        let first_fingerprint = first.plugins["test-plugin"].content_fingerprint;
        fs::write(
            temp.path().join("test-plugin/runtime/plugin.lua"),
            "return { dispatch=function() return true end, shutdown=function() end, on_precheck=function() return '{\"ok\":true,\"result\":{}}' end }\n",
        )
        .expect("update runtime module");
        let second = load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 2);
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
        let snapshot =
            load_system_plugin_catalog(temp.path(), &BTreeSet::new(), &BTreeSet::new(), 1);
        assert!(snapshot.enabled_tools.is_empty());
        assert_eq!(
            snapshot.plugins["test-plugin"].availability,
            SystemPluginAvailability::Unavailable
        );
    }

    /// Verifies every repository-bundled package satisfies the same production loader contract.
    /// 验证仓库内建的每个插件包都满足同一套生产加载器契约。
    #[test]
    fn bundled_system_packages_pass_production_catalog_validation() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = crate_root
            .parent()
            .and_then(Path::parent)
            .expect("resolve workspace root");
        let snapshot = load_system_plugin_catalog(
            &workspace_root.join("plugins/system"),
            &BTreeSet::new(),
            &BTreeSet::new(),
            1,
        );
        assert!(!snapshot.plugins.is_empty());
        assert!(
            snapshot
                .plugins
                .values()
                .all(|plugin| plugin.availability == SystemPluginAvailability::Enabled)
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
