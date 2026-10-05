//! Generic Vulcan plugin service boundaries for mounted UI modules and business actions.
//! 面向挂载界面模块与业务 Action 的通用 Vulcan 插件服务边界。

use serde_json::Value;
use std::path::{Path, PathBuf};
use vulcan_plugin_runtime::{
    VulcanPluginActionCaller, VulcanPluginActionRequest, VulcanPluginRuntimeState,
    VulcanPluginUiMountKind,
};
use vulcan_service_api::{PluginTextAssetSource, PluginUiActionPolicy, PluginUiModuleSource};

use crate::{SharedServiceBridgeState, service_vulcan_plugin_runtime};

/// Maximum uncompressed UTF-8 source size accepted for one dynamic plugin UI module.
/// 单个动态插件界面模块允许的最大未压缩 UTF-8 源码大小。
const PLUGIN_UI_MODULE_MAX_BYTES: u64 = 1024 * 1024;

/// Maximum UTF-8 text size returned for one manifest-declared plugin asset.
/// 单个清单声明插件文本资源允许返回的最大 UTF-8 文本大小。
const PLUGIN_TEXT_ASSET_MAX_BYTES: u64 = 256 * 1024;

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
    runtime_identity: String,
    identity: vulcan_plugin_runtime::VulcanPluginActionIdentity,
) -> Result<Value, String> {
    let runtime = service_vulcan_plugin_runtime(&state)?;
    // reuseEnabledRuntime is authorized only for global plugin pages without a workspace-bound action context.
    // reuseEnabledRuntime 仅对没有工作区绑定 Action 上下文的全局插件页面授权。
    let reuse_enabled_runtime = workspace_path.is_none();
    let workspace_root = resolve_plugin_action_workspace_root(workspace_path.as_deref())?;
    runtime
        .invoke_ui_action(
            VulcanPluginActionRequest {
                plugin_id,
                action_id,
                arguments,
                caller: VulcanPluginActionCaller::PluginUi,
                workspace_root,
                reuse_enabled_runtime,
            },
            &runtime_identity,
            identity,
        )
        .await
        .map_err(|error| error.to_string())
}

/// Reads or cancels one caller-owned invocation without exposing business payloads.
/// 读取或取消单个调用方持有的调用，不暴露业务载荷。
/// The authenticated renderer supplies its nonce and original runtime identity.
/// 已认证渲染器提供自身随机标识与原始运行实例身份。
pub(crate) fn service_plugin_action_status(
    state: SharedServiceBridgeState,
    plugin_id: String,
    runtime_identity: String,
    identity: vulcan_plugin_runtime::VulcanPluginActionIdentity,
    cancel: bool,
) -> Result<Option<vulcan_plugin_runtime::VulcanPluginActionStatus>, String> {
    service_vulcan_plugin_runtime(&state)?
        .find_action_status(&plugin_id, &runtime_identity, &identity, cancel)
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
    let runtime = service_vulcan_plugin_runtime(&state)?;
    let snapshot = runtime.snapshot();
    let plugin = snapshot
        .plugins
        .get(&plugin_id)
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is not installed."))?;
    if plugin.runtime_state != VulcanPluginRuntimeState::Enabled {
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
    // mountExport is the exact named factory export from the stable snapshot; settings layouts have none.
    // mountExport 是稳定快照中的精确命名工厂导出；设置版面没有导出。
    let mount_export = mount.export.clone();
    // mountKind selects the accepted entry extension and whether the configuration schema is attached.
    // mountKind 决定接受的入口扩展名，以及是否附带配置 Schema。
    let mount_kind = mount.kind;
    // configSchemaFile is the package-relative configuration schema exposed only to settings layouts.
    // configSchemaFile 是仅向设置版面公开的插件包相对配置 Schema。
    let config_schema_file = manifest
        .config
        .as_ref()
        .filter(|_| mount_kind == VulcanPluginUiMountKind::Settings)
        .map(|config| config.schema_file.clone());
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
    if current_plugin.runtime_state != VulcanPluginRuntimeState::Enabled
        || current_plugin.content_fingerprint != content_fingerprint
        || current_plugin.generation != plugin.generation
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
    validate_plugin_ui_module_path(&package_root, &entry_path, mount_kind)?;
    let source = read_bounded_plugin_ui_text(&plugin_id, &entry_path, "UI module")?;
    // configSchema is re-read from the same execution copy so the layout and its schema share one generation.
    // configSchema 从同一执行副本重新读取，使版面与其 Schema 同属一个代次。
    let config_schema = config_schema_file
        .map(|relative| {
            let path = package_root.join(relative).canonicalize().map_err(|error| {
                format!("Failed to resolve Vulcan plugin `{plugin_id}` configuration schema: {error}")
            })?;
            validate_plugin_text_asset_path(&package_root, &path)?;
            let text = read_bounded_plugin_ui_text(&plugin_id, &path, "configuration schema")?;
            serde_json::from_str::<Value>(&text).map_err(|error| {
                format!("Vulcan plugin `{plugin_id}` configuration schema is not JSON: {error}")
            })
        })
        .transpose()?;
    Ok(PluginUiModuleSource {
        runtime_identity: format!("{}:{}", snapshot.service_epoch, plugin.generation),
        action_policies: manifest.lua_runtime.as_ref().map_or_else(Vec::new, |lua| {
            lua.actions
                .iter()
                .filter(|action| action.callers.contains(&VulcanPluginActionCaller::PluginUi))
                .map(|action| PluginUiActionPolicy {
                    id: action.id.clone(),
                    effect: match action.effect {
                        vulcan_plugin_runtime::VulcanPluginActionEffect::Read => "read",
                        vulcan_plugin_runtime::VulcanPluginActionEffect::Mutation => "mutation",
                    }
                    .to_owned(),
                    result_cache_ms: action.result_cache_ms,
                })
                .collect()
        }),
        plugin_id,
        mount_id,
        kind: mount_kind.key().to_owned(),
        export_name: mount_export,
        source,
        config_schema,
        messages_by_locale,
    })
}

/// Read one bounded UTF-8 package file served to the plugin UI boundary.
/// 读取一个提供给插件界面边界、有大小上限的 UTF-8 插件包文件。
///
/// # Parameters
/// 参数
/// `plugin_id` and `label` name the file in errors; `path` is an already validated canonical path.
/// `plugin_id` 与 `label` 在错误中命名文件；`path` 是已校验的规范路径。
///
/// # Returns
/// 返回值
/// Returns the complete text only when both the inspected and read sizes stay within the module limit.
/// 仅当检查与读取时的大小都在模块上限内时返回完整文本。
fn read_bounded_plugin_ui_text(plugin_id: &str, path: &Path, label: &str) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        format!("Failed to inspect Vulcan plugin `{plugin_id}` {label}: {error}")
    })?;
    if metadata.len() > PLUGIN_UI_MODULE_MAX_BYTES {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` {label} exceeds {PLUGIN_UI_MODULE_MAX_BYTES} bytes."
        ));
    }
    let source = std::fs::read_to_string(path).map_err(|error| {
        format!("Failed to read Vulcan plugin `{plugin_id}` {label} as UTF-8: {error}")
    })?;
    if source.len() as u64 > PLUGIN_UI_MODULE_MAX_BYTES {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` {label} changed while being read or exceeds {PLUGIN_UI_MODULE_MAX_BYTES} bytes."
        ));
    }
    Ok(source)
}

/// Loads one bounded UTF-8 text asset explicitly declared by an enabled plugin manifest.
/// 加载一个由已启用插件清单明确声明且有大小边界的 UTF-8 文本资源。
///
/// # Parameters
/// 参数
/// `state` owns the current runtime while `plugin_id` and `relative_path` select one exact package asset.
/// `state` 持有当前运行时，`plugin_id` 与 `relative_path` 选择一个精确包内资源。
///
/// # Returns
/// 返回值
/// Returns the authenticated plugin identity, declared relative path, and untouched UTF-8 content.
/// 返回经过认证的插件身份、已声明相对路径与原样 UTF-8 内容。
pub(crate) fn service_read_plugin_text_asset(
    state: SharedServiceBridgeState,
    plugin_id: String,
    relative_path: String,
) -> Result<PluginTextAssetSource, String> {
    // runtime resolves the current installed package through the host-owned plugin catalog.
    // runtime 通过宿主自有插件目录解析当前已安装包。
    let runtime = service_vulcan_plugin_runtime(&state)?;
    // snapshot provides one stable manifest identity before package-root preparation.
    // snapshot 在准备包根目录前提供一个稳定清单身份。
    let snapshot = runtime.snapshot();
    // plugin is the exact catalog entry selected by the authenticated mounted UI.
    // plugin 是经过认证的挂载界面所选择的精确目录条目。
    let plugin = snapshot
        .plugins
        .get(&plugin_id)
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is not installed."))?;
    if plugin.runtime_state != VulcanPluginRuntimeState::Enabled {
        return Err(format!("Vulcan plugin `{plugin_id}` is not enabled."));
    }
    // manifest owns the only allowlist accepted by this generic text-resource boundary.
    // manifest 持有此通用文本资源边界接受的唯一允许列表。
    let manifest = plugin
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` has no valid manifest."))?;
    if !manifest
        .required_files
        .iter()
        .any(|declared_path| declared_path == &relative_path)
    {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` does not declare required text asset `{relative_path}`."
        ));
    }
    // contentFingerprint detects a catalog generation race during root resolution.
    // contentFingerprint 检测根目录解析期间的目录代次竞争。
    let content_fingerprint = plugin.content_fingerprint.clone();
    // packageRoot is the trusted execution copy selected by package origin.
    // packageRoot 是根据插件包来源选择的可信执行副本。
    let package_root = runtime
        .plugin_execution_package_root(&plugin_id)
        .map_err(|error| error.to_string())?;
    // currentSnapshot verifies that authorization did not change during root preparation.
    // currentSnapshot 校验根目录准备期间授权状态没有变化。
    let current_snapshot = runtime.snapshot();
    // currentPlugin is the post-preparation descriptor compared with the original identity.
    // currentPlugin 是与原始身份比较的准备后描述符。
    let current_plugin = current_snapshot
        .plugins
        .get(&plugin_id)
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is no longer installed."))?;
    if current_plugin.runtime_state != VulcanPluginRuntimeState::Enabled
        || current_plugin.content_fingerprint != content_fingerprint
    {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` changed while text asset `{relative_path}` was being resolved."
        ));
    }
    // assetPath is the canonical file selected by the exact manifest path.
    // assetPath 是由精确清单路径选择的规范化文件。
    let asset_path = package_root
        .join(&relative_path)
        .canonicalize()
        .map_err(|error| {
            format!(
                "Failed to resolve Vulcan plugin `{plugin_id}` text asset `{relative_path}`: {error}"
            )
        })?;
    validate_plugin_text_asset_path(&package_root, &asset_path)?;
    // metadata enforces the byte bound before allocating the text buffer.
    // metadata 在分配文本缓冲区前执行字节大小边界。
    let metadata = std::fs::metadata(&asset_path).map_err(|error| {
        format!(
            "Failed to inspect Vulcan plugin `{plugin_id}` text asset `{relative_path}`: {error}"
        )
    })?;
    if metadata.len() > PLUGIN_TEXT_ASSET_MAX_BYTES {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` text asset `{relative_path}` exceeds {PLUGIN_TEXT_ASSET_MAX_BYTES} bytes."
        ));
    }
    // content preserves the exact package text while requiring valid UTF-8.
    // content 保留精确包文本，同时要求内容是有效 UTF-8。
    let content = std::fs::read_to_string(&asset_path).map_err(|error| {
        format!(
            "Failed to read Vulcan plugin `{plugin_id}` text asset `{relative_path}` as UTF-8: {error}"
        )
    })?;
    if content.len() as u64 > PLUGIN_TEXT_ASSET_MAX_BYTES {
        return Err(format!(
            "Vulcan plugin `{plugin_id}` text asset `{relative_path}` changed while being read or exceeds {PLUGIN_TEXT_ASSET_MAX_BYTES} bytes."
        ));
    }
    Ok(PluginTextAssetSource {
        plugin_id,
        path: relative_path,
        content,
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
fn validate_plugin_ui_module_path(
    package_root: &Path,
    entry_path: &Path,
    kind: VulcanPluginUiMountKind,
) -> Result<(), String> {
    let extension = kind.entry_extension();
    if !entry_path.starts_with(package_root)
        || !entry_path.is_file()
        || entry_path.extension().and_then(|value| value.to_str()) != Some(extension)
    {
        return Err(format!(
            "Vulcan plugin UI entry escapes its package root or is not a regular `.{extension}` file."
        ));
    }
    Ok(())
}

/// Validates that one declared text asset remains a regular file inside its package root.
/// 校验一个已声明文本资源仍是插件包根内的普通文件。
///
/// # Parameters
/// 参数
/// `package_root` is the trusted package root and `asset_path` is the canonical selected file.
/// `package_root` 是可信包根目录，`asset_path` 是规范化后的已选文件。
///
/// # Returns
/// 返回值
/// Returns success only for one contained regular file.
/// 仅对受目录约束的普通文件返回成功。
fn validate_plugin_text_asset_path(package_root: &Path, asset_path: &Path) -> Result<(), String> {
    if !asset_path.starts_with(package_root) || !asset_path.is_file() {
        return Err(String::from(
            "Vulcan plugin text asset escapes its package root or is not a regular file.",
        ));
    }
    Ok(())
}
