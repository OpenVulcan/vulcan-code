//! Manifest-bound plugin process and MCP stdio launch specifications.
//! 绑定清单的插件进程与MCP stdio启动规格。

use super::environment::{
    ensure_plugin_execution_environment, plugin_execution_package_root,
    plugin_python_environment_executable,
};
use super::{
    VulcanPluginBridgeLeaseScope, VulcanPluginManagerState, VulcanPluginRuntimeError,
    VulcanPluginRuntimeManager, canonical_bridge_workspace_root, canonical_plugin_file,
    read_plugin_json_state, required_json_string,
};
use crate::{
    VulcanPluginDependencyInstallMode, VulcanPluginManifest, VulcanPluginMcpStdioLauncherManifest,
    VulcanPluginPermission,
};
use luaskills::{
    ManagedRuntimeKind, PackageDependencyManifest, current_managed_runtime_platform_key,
    resolve_managed_runtime_install,
};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Resolve validated process arguments against the caller's exact retained plugin and workspace.
/// 针对调用方保留的精确插件及工作区解析已校验进程参数。
/// Return the authorized launch specification without consulting a newer catalog descriptor.
/// 返回已授权启动规格，不读取更新的目录描述符。
pub(super) fn resolve_plugin_process_for_snapshot(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
    workspace_root: &Path,
    payload: &Map<String, Value>,
) -> Result<Value, String> {
    // The package ID, process declarations and dependency environment share one frozen owner.
    // 包 ID、进程声明及依赖环境共用一个冻结所有者。
    let plugin_id = &plugin.id;
    // Required process entry is business input; it never supplies a program or cwd.
    // 必需进程入口属于业务输入；它绝不提供程序或 cwd。
    let entry_id = required_json_string(payload, "entry_id")?;
    // Only the two explicitly supported process protocols are accepted.
    // 仅接受两个显式支持的进程协议。
    let mode = required_json_string(payload, "mode")?;
    if mode != "exec" && mode != "session" {
        return Err("Vulcan plugin process mode must be `exec` or `session`.".to_string());
    }
    // The manifest belongs to the original authorized generation.
    // 清单属于原已授权代次。
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
                workspace_root,
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
pub(super) fn resolve_plugin_process_spec(
    state: &VulcanPluginManagerState,
    request_args: &Value,
) -> Result<Value, String> {
    // The compatibility envelope is accepted only from the legacy trusted bridge.
    // 兼容信封仅从旧可信桥接接收。
    let request = request_args
        .as_object()
        .ok_or_else(|| "Vulcan plugin process request must be an object.".to_string())?;
    // Read the exact bridge-injected plugin identity.
    // 读取桥接注入的精确插件身份。
    let plugin_id = required_json_string(request, "plugin_id")?;
    // Lifecycle scope is validated before resolving executable resources.
    // 在解析可执行资源前校验生命周期作用域。
    let scope = VulcanPluginBridgeLeaseScope::parse(required_json_string(request, "lease_scope")?)?;
    // Workspace authorization belongs to the host envelope.
    // 工作区授权属于宿主信封。
    let workspace = canonical_bridge_workspace_root(request)?;
    // Process-specific options retain their existing strict interpretation.
    // 进程特定选项保留既有严格解释。
    let payload = request
        .get("payload")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            "Vulcan plugin process request requires object field `payload`.".to_string()
        })?;
    // Retain the descriptor before releasing the short catalog lock.
    // 在释放短目录锁前保留描述符。
    let plugin = state
        .snapshot
        .read()
        .map_err(|_| "Vulcan plugin catalog lock was poisoned.".to_string())?
        .plugins
        .get(plugin_id)
        .cloned()
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is not installed."))?;
    if plugin.runtime_state != scope.required_runtime_state() {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` runtime state does not authorize process bridge scope `{}`.",
            scope.as_str()
        ));
    }
    resolve_plugin_process_for_snapshot(state, &plugin, &workspace, payload)
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
///
/// # Parameters
/// 参数
/// `state` is the shared manager state owning fixed managed-runtime roots.
/// `state` 是持有固定受管运行时根的共享管理器状态。
/// `manifest` is the validated package or installed npm manifest selected by the caller.
/// `manifest` 是调用方选定的已校验插件包清单或已安装npm清单。
/// `environment_root` is the exact private environment for this plugin generation.
/// `environment_root` 是当前插件代次的精确私有环境。
///
/// # Returns
/// 返回值
/// Returns the exact declared managed Node executable, or a declaration/install error.
/// 返回精确声明的受管Node可执行文件，或声明/安装错误。
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
///
/// # Parameters
/// 参数
/// `state` is the shared manager state owning fixed managed-runtime roots.
/// `state` 是持有固定受管运行时根的共享管理器状态。
/// `manifest` is the validated package or installed npm manifest selected by the caller.
/// `manifest` 是调用方选定的已校验插件包清单或已安装npm清单。
/// `environment_root` is the exact private environment for this plugin generation.
/// `environment_root` 是当前插件代次的精确私有环境。
/// `plugin` is the validated descriptor pinning identity and package generation.
/// `plugin` 是固定身份与包代次的已校验描述符。
/// `package` is the exact manifest-declared npm package name.
/// `package` 是清单声明的精确npm包名。
/// `bin` is the exact manifest-declared executable key.
/// `bin` 是清单声明的精确可执行入口键。
///
/// # Returns
/// 返回值
/// Returns a Shell-free specification for the contained installed package bin, or an identity/path error.
/// 返回受目录约束的已安装包入口的不使用Shell启动规格，或身份/路径错误。
pub(super) fn resolve_plugin_node_package_bin(
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
    // The installed package tree must remain inside this plugin generation's private root.
    // 已安装包目录树必须位于当前插件代次的私有根目录内。
    let canonical_environment_root = environment_root
        .canonicalize()
        .map_err(|error| format!("Failed to resolve plugin environment root: {error}"))?;
    if !node_modules_root.starts_with(&canonical_environment_root) {
        return Err(String::from(
            "Plugin node_modules directory escapes the plugin-private environment.",
        ));
    }
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
///
/// # Parameters
/// 参数
/// `package` is the exact manifest-declared npm package name.
/// `package` 是清单声明的精确npm包名。
///
/// # Returns
/// 返回值
/// Returns the validated node_modules-relative package path, or an invalid-name error.
/// 返回已校验的node_modules相对包路径，或非法名称错误。
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
///
/// # Parameters
/// 参数
/// `manifest` is the validated package or installed npm manifest selected by the caller.
/// `manifest` 是调用方选定的已校验插件包清单或已安装npm清单。
/// `package` is the exact manifest-declared npm package name.
/// `package` 是清单声明的精确npm包名。
/// `bin` is the exact manifest-declared executable key.
/// `bin` 是清单声明的精确可执行入口键。
///
/// # Returns
/// 返回值
/// Returns the exact declared bin target borrowed from the installed manifest, or an explicit error.
/// 返回从已安装清单借用的精确bin目标，或明确错误。
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
///
/// # Parameters
/// 参数
/// `source` is the validated process-options object.
/// `source` 是已校验的进程选项对象。
/// `target` is the host-owned process specification receiving the field.
/// `target` 是接收字段的宿主持有进程规格。
/// `field` is the exact optional process field being validated.
/// `field` 是当前校验的精确可选进程字段。
///
/// # Returns
/// 返回值
/// Returns success after copying a present string or leaving an absent field untouched; errors on other types.
/// 返回复制已存在字符串或保持缺失字段不变后的成功结果；其他类型返回错误。
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
///
/// # Parameters
/// 参数
/// `source` is the validated process-options object.
/// `source` 是已校验的进程选项对象。
/// `target` is the host-owned process specification receiving the field.
/// `target` 是接收字段的宿主持有进程规格。
/// `field` is the exact optional process field being validated.
/// `field` 是当前校验的精确可选进程字段。
///
/// # Returns
/// 返回值
/// Returns success after copying a present positive integer or preserving absence; errors on invalid values.
/// 返回复制已存在正整数或保留缺失后的成功结果；非法值返回错误。
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

/// Canonicalizes one package-relative directory without allowing root escape.
/// 规范化一个插件包相对目录，且不允许逃逸包根目录。
///
/// # Parameters
/// 参数
/// `root` is the trusted package root containing the declared directory.
/// `root` 是包含已声明目录的可信插件包根。
/// `relative` is the manifest-declared package directory path.
/// `relative` 是清单声明的插件包目录路径。
/// `label` is the diagnostic label naming the expected directory.
/// `label` 是命名预期目录的诊断标签。
///
/// # Returns
/// 返回值
/// Returns a canonical existing directory contained by the package root, or a boundary/type error.
/// 返回位于插件包根内的规范化已存在目录，或边界/类型错误。
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
///
/// # Parameters
/// 参数
///
/// # Returns
/// 返回值
/// Returns the compile-time supported Rust target triple without arguments, or an unsupported-target error.
/// 返回无需参数的当前编译目标三元组，或不支持目标错误。
pub(super) fn current_plugin_target_triple() -> Result<&'static str, String> {
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

impl VulcanPluginRuntimeManager {
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
    /// Returns a contained Shell-free specification using the declared native binary or managed runtime.
    /// 返回使用已声明原生二进制或受管运行时、受目录约束且不使用Shell的启动规格。
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
}

#[cfg(all(test, windows))]
mod tests;
