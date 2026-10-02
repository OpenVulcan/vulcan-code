//! Immutable plugin execution environments and declared dependency installation.
//! 不可变插件执行环境与已声明依赖安装。

use super::{
    VulcanPluginManagerState, VulcanPluginRuntimeError, VulcanPluginRuntimeManager,
    canonical_plugin_file, copy_plugin_directory_tree, write_plugin_json_record,
};
use crate::{
    VulcanPluginDependencyInstallMode, VulcanPluginManifest, VulcanPluginMcpStdioLauncherManifest,
    VulcanPluginMcpTransportManifest, VulcanPluginPackageOrigin,
};
use luaskills::{
    ManagedRuntimeKind, PackageDependencyManifest, current_managed_runtime_platform_key,
    resolve_managed_runtime_install,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;
use vulcan_runtime_assets::text_decode::decode_complete_user_text;

mod leases;
pub(super) use leases::lock_plugin_execution_environment_cleanup;
pub(super) use leases::retain_plugin_execution_environment;

/// Windows creation flag that prevents a console window for a child process.
/// 防止子进程创建控制台窗口的 Windows 创建标志。
#[cfg(windows)]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

/// Applies the no-console flag to one background plugin dependency process on Windows.
/// 在 Windows 上为一个后台插件依赖进程应用无控制台标志。
///
/// # Parameters
/// 参数
/// `command` is the prepared dependency process configuration.
/// `command` 是已准备的依赖子进程配置。
///
/// # Returns
/// 返回值
/// Returns no value; modifies the command in every Windows build profile.
/// 返回空值；在所有 Windows 构建配置中修改命令。
fn apply_windows_no_window_to_command(command: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;

        command.creation_flags(WINDOWS_CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = command;
    }
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
/// Returns bundled source directly or the private user environment selected by its exact identity marker.
/// 系统内置插件返回源目录，用户插件返回由精确身份标记选定的私有环境。
pub(super) fn plugin_execution_package_root(
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
pub(super) fn ensure_plugin_execution_environment(
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
    // Keep builders and their staging directories outside every cleanup's exclusive interval.
    // 让构建者及其暂存目录避开所有清理的排他区间。
    let _environment = retain_plugin_execution_environment(state, &plugin.id)?;
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

/// Checks a published plugin environment without creating files or accessing a registry.
/// 检查已发布插件环境，不创建文件，也不访问依赖注册表。
///
/// # Parameters
/// 参数
/// `state` owns the private runtime root and `plugin` identifies the exact package generation.
/// `state` 持有私有运行时根，`plugin` 标识精确的插件包代次。
///
/// # Returns
/// 返回值
/// Returns the existing root, `None` for an unprepared generation, or an explicit integrity error.
/// 返回已有根目录、表示代次尚未准备的 `None`，或明确的完整性错误。
fn inspect_plugin_execution_environment(
    state: &VulcanPluginManagerState,
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<Option<PathBuf>, String> {
    // manifest and version come from the validated catalog generation, never from a caller path.
    // manifest 和 version 来自已校验目录代次，绝不来自调用方路径。
    let manifest = plugin
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Vulcan plugin `{}` has no valid manifest.", plugin.id))?;
    let version = plugin
        .version
        .as_deref()
        .ok_or_else(|| format!("Vulcan plugin `{}` has no validated version.", plugin.id))?;
    if !manifest.has_lua_runtime()
        && manifest.mcp.servers.is_empty()
        && manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Bundled
        && manifest.execution.native_binaries.is_empty()
        && manifest.execution.node_entries.is_empty()
        && manifest.execution.python_entries.is_empty()
    {
        return Ok(Some(plugin.package_root.clone()));
    }
    // destination is derived solely from the manager root and exact catalog identity.
    // destination 仅由管理器根目录和精确目录身份推导。
    let mut destination = state
        .plugin_runtime_root
        .join(&plugin.id)
        .join(version)
        .join(&plugin.content_fingerprint);
    // Built-in bundled sources already contain dependencies; private publication happens on use.
    // 内置自带依赖的源码包已包含依赖；私有代次在使用时发布。
    let bundled_source = !destination.exists()
        && plugin.origin == crate::VulcanPluginPackageOrigin::Bundled
        && manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Bundled;
    if bundled_source {
        destination = plugin.package_root.clone();
    } else if !destination.exists() {
        return Ok(None);
    }
    // expected_marker forbids treating an old or tampered environment as ready.
    // expected_marker 禁止将旧代次或被篡改的环境视为就绪。
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
    if !bundled_source {
        validate_plugin_environment_marker(&destination, &expected_marker)?;
    }
    if manifest.execution.dependencies.manifest.is_some() {
        // dependency_file is the package-owned, already-authorized lock declaration.
        // dependency_file 是插件包自有且已授权的锁定依赖声明。
        let dependency_file = canonical_plugin_file(
            &destination,
            manifest
                .execution
                .dependencies
                .manifest
                .as_deref()
                .ok_or_else(|| String::from("Plugin registry dependencies require a manifest."))?,
            "dependency manifest",
        )?;
        // dependencies identifies exactly which managed runtimes and private install roots must exist.
        // dependencies 精确标识必须存在的受管运行时与私有安装根目录。
        let dependencies = PackageDependencyManifest::load_from_path(&dependency_file)?;
        // platform matches the immutable managed runtime installation key.
        // platform 与不可变受管运行时安装键一致。
        let platform = current_managed_runtime_platform_key()?;
        if let Some(node) = &dependencies.node_runtime {
            resolve_managed_runtime_install(
                &state.managed_runtime_distribution_root,
                ManagedRuntimeKind::Node,
                &node.version,
                &platform,
            )?;
            // package_json locates the exact node_modules directory installed by pnpm.
            // package_json 定位 pnpm 安装的精确 node_modules 目录。
            let package_json =
                canonical_plugin_file(&destination, &node.package_json, "Node package manifest")?;
            // Dependency-free bundled Node entries do not require an empty node_modules directory.
            // 无依赖的内置 Node 入口不需要空的 node_modules 目录。
            let package = super::read_plugin_json_state(&package_json, "Node package manifest")
                .map_err(|error| error.to_string())?;
            if crate::catalog::node_package_declares_dependencies(&package)?
                && !package_json
                    .parent()
                    .ok_or_else(|| String::from("Node package manifest has no parent directory."))?
                    .join("node_modules")
                    .is_dir()
            {
                return Ok(None);
            }
        }
        if let Some(python) = &dependencies.python_runtime
            && manifest.execution.dependencies.install_mode
                == VulcanPluginDependencyInstallMode::Registry
        {
            resolve_managed_runtime_install(
                &state.managed_runtime_distribution_root,
                ManagedRuntimeKind::Python,
                &python.version,
                &platform,
            )?;
            if !plugin_python_environment_executable(&destination).is_file() {
                return Ok(None);
            }
        }
        for server in &manifest.mcp.servers {
            if let VulcanPluginMcpTransportManifest::Stdio {
                launcher: VulcanPluginMcpStdioLauncherManifest::NodePackageBin { package, bin },
                ..
            } = &server.transport
            {
                // The exact declared package bin must resolve before an AI-visible MCP server starts.
                // AI 可见 MCP 服务启动前，必须能解析清单声明的精确包入口。
                if super::process::resolve_plugin_node_package_bin(
                    state,
                    plugin,
                    manifest,
                    &destination,
                    package,
                    bin,
                )
                .is_err()
                {
                    return Ok(None);
                }
            }
        }
    }
    destination.canonicalize().map(Some).map_err(|error| {
        format!(
            "Failed to resolve Vulcan plugin environment `{}`: {error}",
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
pub(super) fn remove_plugin_execution_environment_generation(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
    version: &str,
    fingerprint: &str,
) -> Result<(), String> {
    // Absence is a no-op; an existing root must be exclusively owned before inspecting descendants.
    // 根缺失时无需处理；根存在时须先排他持有再检查下级目录。
    if !state.plugin_runtime_root.exists() {
        return Ok(());
    }
    let _cleanup = lock_plugin_execution_environment_cleanup(state, plugin_id)?;
    let environment = state
        .plugin_runtime_root
        .join(plugin_id)
        .join(version)
        .join(fingerprint);
    if !environment.exists() {
        return Ok(());
    }
    // Every untrusted path component must be a real directory before recursive deletion.
    // 递归删除前，所有非可信路径组件都必须是真实目录。
    for component in [
        state.plugin_runtime_root.join(plugin_id),
        state.plugin_runtime_root.join(plugin_id).join(version),
        environment.clone(),
    ] {
        // metadata rejects a junction or symlink even when its target remains inside the runtime root.
        // metadata 即使在目标仍位于运行时根目录内，也拒绝目录联接或符号链接。
        let metadata = fs::symlink_metadata(&component).map_err(|error| {
            format!(
                "Failed to inspect plugin environment component `{}`: {error}",
                component.display()
            )
        })?;
        if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
            return Err(format!(
                "Plugin environment component `{}` is not a plain directory.",
                component.display()
            ));
        }
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

/// Validates one published plugin environment against its exact expected identity.
/// 根据精确预期身份校验一个已发布插件环境。
///
/// # Parameters
/// 参数
/// `environment_root` is the exact staged or published private environment directory.
/// `environment_root` 是精确的暂存或已发布私有环境目录。
/// `expected` is the complete generation identity expected by the caller.
/// `expected` 是调用方要求的完整代次身份。
///
/// # Returns
/// 返回值
/// Returns success when the stored marker exactly matches, or a read/parse/identity error.
/// 返回标记精确匹配时的成功结果，或读取/解析/身份错误。
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
///
/// # Parameters
/// 参数
/// `environment_root` is the exact staged or published private environment directory.
/// `environment_root` 是精确的暂存或已发布私有环境目录。
/// `state` is the shared manager state owning fixed host runtime roots.
/// `state` 是持有固定宿主运行时根的共享管理器状态。
/// `manifest` is the validated package declaration authorizing registry dependencies.
/// `manifest` 是授权注册表依赖的已校验包声明。
///
/// # Returns
/// 返回值
/// Returns success after all declared dependency installations, or the original failure diagnostic.
/// 返回全部声明依赖安装后的成功结果，或原始失败诊断。
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
///
/// # Parameters
/// 参数
/// `distribution_root` is the fixed shared managed-distribution root.
/// `distribution_root` 是固定的共享受管发行根。
/// `family` is the declared Node or Python runtime family.
/// `family` 是已声明的Node或Python运行时家族。
/// `runtime` is the exact pnpm or uv package-manager identity.
/// `runtime` 是精确的pnpm或uv包管理器身份。
/// `version` is the exact declared package-manager version.
/// `version` 是精确声明的包管理器版本。
/// `platform` is the matching managed platform key.
/// `platform` 是匹配的受管平台键。
///
/// # Returns
/// 返回值
/// Returns the contained executable selected by the exact install manifest, or an explicit error.
/// 返回由精确安装清单选定且位于目录内的可执行文件，或明确错误。
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

/// Runs one dependency installation synchronously and reports failure output without a local timeout.
/// 同步执行依赖安装并报告失败输出，此处不设置本地超时。
///
/// # Parameters
/// 参数
/// `command` is the prepared dependency process configuration.
/// `command` 是已准备的依赖子进程配置。
/// `label` is the diagnostic action label for this dependency command.
/// `label` 是本次依赖命令的诊断动作标签。
///
/// # Returns
/// 返回值
/// Returns success on a successful exit, otherwise the decoded stdout/stderr diagnostic.
/// 返回成功退出时的成功结果，否则返回解码后的标准输出/错误诊断。
fn run_plugin_dependency_command(command: &mut Command, label: &str) -> Result<(), String> {
    apply_windows_no_window_to_command(command);
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
///
/// # Parameters
/// 参数
/// `mode` is the manifest-authorized dependency installation policy.
/// `mode` 是清单授权的依赖安装策略。
///
/// # Returns
/// 返回值
/// Returns the stable bundled or registry key for the declared mode.
/// 返回声明模式对应的稳定bundled或registry键。
fn plugin_dependency_install_mode_key(mode: VulcanPluginDependencyInstallMode) -> &'static str {
    match mode {
        VulcanPluginDependencyInstallMode::Bundled => "bundled",
        VulcanPluginDependencyInstallMode::Registry => "registry",
    }
}

/// Returns the platform-specific interpreter inside one plugin-owned Python environment.
/// 返回插件自有 Python 环境内的平台专用解释器。
///
/// # Parameters
/// 参数
/// `environment_root` is the exact staged or published private environment directory.
/// `environment_root` 是精确的暂存或已发布私有环境目录。
///
/// # Returns
/// 返回值
/// Returns the platform-specific interpreter path under the same private .venv.
/// 返回同一私有.venv下的平台专用解释器路径。
pub(super) fn plugin_python_environment_executable(environment_root: &Path) -> PathBuf {
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

impl VulcanPluginRuntimeManager {
    /// Reports whether the exact current plugin generation has a prepared execution environment.
    /// 报告当前精确插件代次是否已有准备完成的执行环境。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` identifies one plugin in the current validated catalog snapshot.
    /// `plugin_id` 标识当前已校验目录快照中的一个插件。
    ///
    /// # Returns
    /// 返回值
    /// Returns readiness without side effects, or an explicit integrity error.
    /// 无副作用地返回就绪状态，或明确的完整性错误。
    pub fn plugin_environment_ready(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // snapshot freezes the descriptor while readiness is checked.
        // snapshot 在检查就绪状态期间固定描述符。
        let snapshot = self.snapshot();
        // plugin is selected by its exact unmodified catalog name.
        // plugin 通过未经修改的精确目录名称选取。
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        inspect_plugin_execution_environment(&self.state, plugin)
            .map(|root| root.is_some())
            .map_err(VulcanPluginRuntimeError::new)
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
    /// Returns the content-only source root or the canonical environment for the current package fingerprint.
    /// 返回纯内容包源目录，或当前包指纹对应的规范化环境目录。
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
        // A complete marker with missing installed files is repairable only while no session owns the plugin.
        // 标记完整但安装文件缺失时，仅在没有会话持有插件期间才允许修复。
        if inspect_plugin_execution_environment(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?
            .is_none()
        {
            // version is the exact catalog identity of the private environment being replaced.
            // version 是将被替换的私有环境的精确目录身份。
            let version = plugin.version.as_deref().ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{plugin_id}` has no validated version."
                ))
            })?;
            // destination is derived exclusively from validated catalog fields.
            // destination 完全由已校验的目录字段推导。
            let destination = self
                .state
                .plugin_runtime_root
                .join(&plugin.id)
                .join(version)
                .join(&plugin.content_fingerprint);
            if destination.exists() {
                if plugin.runtime_state != crate::VulcanPluginRuntimeState::Disabled
                    || self.active_session_count(plugin_id)? != 0
                    || self.has_cleanup_diagnostics(plugin_id)?
                {
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "Vulcan plugin `{plugin_id}` has an incomplete environment but is not fully disabled."
                    )));
                }
                remove_plugin_execution_environment_generation(
                    &self.state,
                    &plugin.id,
                    version,
                    &plugin.content_fingerprint,
                )
                .map_err(VulcanPluginRuntimeError::new)?;
            }
        }
        // prepared_root is not sufficient evidence of readiness: the dependency installer can
        // publish a marker even when the manifest names a nonexistent MCP package executable.
        // prepared_root 不足以证明就绪：依赖安装器可能在清单声明不存在的 MCP 包入口时仍发布标记。
        let prepared_root = ensure_plugin_execution_environment(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?;
        // The same read-only integrity gate used before enable must pass before an installation
        // or update transaction can commit this environment as a usable generation.
        // 安装或更新事务提交可用代次前，必须通过启用前使用的同一只读完整性关卡。
        if inspect_plugin_execution_environment(&self.state, plugin)
            .map_err(VulcanPluginRuntimeError::new)?
            .is_none()
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` prepared environment is incomplete."
            )));
        }
        Ok(prepared_root)
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
    /// Returns bundled source directly or a private user copy selected by its generation identity marker.
    /// 系统内置插件直接返回源目录，用户插件返回由代次身份标记选定的私有副本。
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
}

#[cfg(test)]
mod tests;
