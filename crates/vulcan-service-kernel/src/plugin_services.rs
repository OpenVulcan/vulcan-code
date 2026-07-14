//! Generic Vulcan plugin service boundaries for mounted UI modules and business actions.
//! 面向挂载界面模块与业务 Action 的通用 Vulcan 插件服务边界。

use serde_json::Value;
use std::path::{Path, PathBuf};
use vulcan_plugin_runtime::{
    SystemPluginActionCaller, SystemPluginActionRequest, SystemPluginRuntimeState,
};
use vulcan_service_api::PluginUiModuleSource;

use crate::{SharedServiceBridgeState, service_system_plugin_runtime};

/// Maximum uncompressed UTF-8 source size accepted for one dynamic plugin UI module.
/// 单个动态插件界面模块允许的最大未压缩 UTF-8 源码大小。
const PLUGIN_UI_MODULE_MAX_BYTES: u64 = 1024 * 1024;

/// Invokes one manifest-declared plugin business action for the owning mounted UI.
/// 为所属挂载界面调用一个清单声明的插件业务 Action。
///
/// # Parameters
/// 参数
/// `state` owns the active runtime; ids select the exact action and arguments are schema validated.
/// `state` 持有活动运行时；标识选择精确 Action，参数会经过 Schema 校验。
/// `workspace_path` optionally supplies the current host workspace context.
/// `workspace_path` 可选提供当前宿主工作区上下文。
///
/// # Returns
/// 返回值
/// Returns the exact schema-validated plugin action result.
/// 返回经过 Schema 校验的精确插件 Action 结果。
pub(crate) async fn service_invoke_plugin_action(
    state: SharedServiceBridgeState,
    plugin_id: String,
    action_id: String,
    arguments: Value,
    workspace_path: Option<String>,
) -> Result<Value, String> {
    let runtime = service_system_plugin_runtime(&state)?;
    // reuseEnabledRuntime is authorized only for global plugin pages without a workspace-bound action context.
    // reuseEnabledRuntime 仅对没有工作区绑定 Action 上下文的全局插件页面授权。
    let reuse_enabled_runtime = workspace_path.is_none();
    let workspace_root = resolve_plugin_action_workspace_root(workspace_path.as_deref())?;
    runtime
        .invoke_action(SystemPluginActionRequest {
            plugin_id,
            action_id,
            arguments,
            caller: SystemPluginActionCaller::PluginUi,
            workspace_root,
            reuse_enabled_runtime,
        })
        .await
        .map_err(|error| error.to_string())
}

/// Loads one validated uncompressed ESM module declared by an enabled plugin UI mount.
/// 加载一个由已启用插件界面挂载项声明且通过校验的未压缩 ESM 模块。
///
/// # Parameters
/// 参数
/// `state` owns the current catalog while ids select one exact manifest mount.
/// `state` 持有当前目录，标识选择一个精确清单挂载项。
///
/// # Returns
/// 返回值
/// Returns untouched bounded UTF-8 source plus its immutable mount identity.
/// 返回原样且有大小边界的 UTF-8 源码及其不可变挂载身份。
pub(crate) fn service_load_plugin_ui_module(
    state: SharedServiceBridgeState,
    plugin_id: String,
    mount_id: String,
) -> Result<PluginUiModuleSource, String> {
    let runtime = service_system_plugin_runtime(&state)?;
    let snapshot = runtime.snapshot();
    let plugin = snapshot
        .plugins
        .get(&plugin_id)
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is not installed."))?;
    if plugin.runtime_state != SystemPluginRuntimeState::Enabled {
        return Err(format!("Vulcan plugin `{plugin_id}` is not enabled."));
    }
    let manifest = plugin
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` has no valid manifest."))?;
    let mount = manifest
        .ui
        .mounts
        .iter()
        .find(|mount| mount.id == mount_id)
        .ok_or_else(|| {
            format!("Vulcan plugin `{plugin_id}` does not declare UI mount `{mount_id}`.")
        })?;
    // mountEntry is the exact package-relative module path from the stable snapshot.
    // mountEntry 是稳定快照中的精确插件包相对模块路径。
    let mount_entry = mount.entry.clone();
    // mountExport is the exact named factory export from the stable snapshot.
    // mountExport 是稳定快照中的精确命名工厂导出。
    let mount_export = mount.export.clone();
    // contentFingerprint detects a catalog generation race during root resolution.
    // contentFingerprint 检测根目录解析期间的目录代次竞争。
    let content_fingerprint = plugin.content_fingerprint.clone();
    // messagesByLocale retains only manifest-validated plugin-owned UI messages.
    // messagesByLocale 仅保留通过清单校验的插件自有界面文案。
    let messages_by_locale = plugin
        .locales
        .iter()
        .map(|(locale, messages)| (locale.clone(), messages.messages.clone()))
        .collect();
    // packageRoot is the trusted execution copy selected by package origin.
    // packageRoot 是根据插件包来源选择的可信执行副本。
    let package_root = runtime
        .plugin_execution_package_root(&plugin_id)
        .map_err(|error| error.to_string())?;
    // currentSnapshot verifies the plugin remained enabled after root preparation.
    // currentSnapshot 校验插件在根目录准备后仍保持启用。
    let current_snapshot = runtime.snapshot();
    // currentPlugin is the post-preparation descriptor compared with the original identity.
    // currentPlugin 是与原始身份比较的准备后描述符。
    let current_plugin = current_snapshot
        .plugins
        .get(&plugin_id)
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is no longer installed."))?;
    if current_plugin.runtime_state != SystemPluginRuntimeState::Enabled
        || current_plugin.content_fingerprint != content_fingerprint
    {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` changed while its UI module was being resolved."
        ));
    }
    let entry_path = package_root
        .join(&mount_entry)
        .canonicalize()
        .map_err(|error| {
            format!("Failed to resolve Vulcan plugin `{plugin_id}` UI module: {error}")
        })?;
    validate_plugin_ui_module_path(&package_root, &entry_path)?;
    let metadata = std::fs::metadata(&entry_path).map_err(|error| {
        format!("Failed to inspect Vulcan plugin `{plugin_id}` UI module: {error}")
    })?;
    if metadata.len() > PLUGIN_UI_MODULE_MAX_BYTES {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` UI module exceeds {PLUGIN_UI_MODULE_MAX_BYTES} bytes."
        ));
    }
    let source = std::fs::read_to_string(&entry_path).map_err(|error| {
        format!("Failed to read Vulcan plugin `{plugin_id}` UI module as UTF-8: {error}")
    })?;
    if source.len() as u64 > PLUGIN_UI_MODULE_MAX_BYTES {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` UI module changed while being read or exceeds {PLUGIN_UI_MODULE_MAX_BYTES} bytes."
        ));
    }
    Ok(PluginUiModuleSource {
        plugin_id,
        mount_id,
        export_name: mount_export,
        source,
        messages_by_locale,
    })
}

/// Resolves an optional UI action workspace into one canonical existing directory.
/// 把可选界面 Action 工作区解析为一个规范化且已存在的目录。
///
/// # Parameters
/// 参数
/// `workspace_path` is either an explicit host context or absent for a global plugin page.
/// `workspace_path` 是显式宿主上下文；全局插件页可以省略。
///
/// # Returns
/// 返回值
/// Returns the canonical explicit directory or the canonical host process directory.
/// 返回规范化后的显式目录或宿主进程目录。
fn resolve_plugin_action_workspace_root(workspace_path: Option<&str>) -> Result<PathBuf, String> {
    let requested = match workspace_path
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(workspace_path) => PathBuf::from(workspace_path),
        None => std::env::current_dir()
            .map_err(|error| format!("Failed to read plugin action host directory: {error}"))?,
    };
    let canonical = requested
        .canonicalize()
        .map_err(|error| format!("Failed to resolve plugin action workspace: {error}"))?;
    if !canonical.is_dir() {
        return Err(String::from("Plugin action workspace is not a directory."));
    }
    Ok(canonical)
}

/// Validates that one dynamic module remains a regular `.mjs` file inside its package root.
/// 校验一个动态模块仍是插件包根内的普通 `.mjs` 文件。
///
/// # Parameters
/// 参数
/// `package_root` and `entry_path` are canonical filesystem paths.
/// `package_root` 与 `entry_path` 均为规范化文件系统路径。
///
/// # Returns
/// 返回值
/// Returns success only for a contained regular ESM module.
/// 仅对受目录约束的普通 ESM 模块返回成功。
fn validate_plugin_ui_module_path(package_root: &Path, entry_path: &Path) -> Result<(), String> {
    if !entry_path.starts_with(package_root)
        || !entry_path.is_file()
        || entry_path.extension().and_then(|value| value.to_str()) != Some("mjs")
    {
        return Err(String::from(
            "Vulcan plugin UI module escapes its package root or is not a regular `.mjs` file.",
        ));
    }
    Ok(())
}
