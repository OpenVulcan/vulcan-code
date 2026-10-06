//! Exact plugin authorization records and sanitized permission previews.
//! 精确插件授权记录与脱敏权限预览。

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager, read_plugin_json_state};
use crate::{VulcanPluginMcpTransportManifest, VulcanPluginPackageOrigin, VulcanPluginPermission};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// User-facing authorization preview derived from one validated catalog descriptor.
/// 从一个已校验目录描述符派生的面向用户授权预览。
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
    /// Secret key requirements for local MCP installation validation, without secret values.
    /// 本地MCP安装验证所需的Secret键声明，不包含秘密值。
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

/// One exact plugin-scoped Secret key required for local MCP installation validation.
/// 本地MCP安装验证所需的单个精确插件作用域Secret键。
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

/// Durable authorization record bound to an exact plugin version and content fingerprint.
/// 绑定到精确插件版本与内容指纹的持久授权记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct VulcanPluginAuthorizationRecord {
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

/// Returns the deterministic authorization document path for one validated plugin identity.
/// 返回一个已校验插件身份的确定性授权文档路径。
///
/// # Parameters
/// 参数
/// `state_root` is the fixed per-user plugin state root.
/// `state_root` 是固定的用户插件状态根。
/// `plugin_id` is the validated plugin identity selecting its state directory.
/// `plugin_id` 是选择所属状态目录的已校验插件身份。
///
/// # Returns
/// 返回值
/// Returns the deterministic authorization.json path without filesystem mutation.
/// 返回不修改文件系统的确定性authorization.json路径。
pub(super) fn plugin_authorization_state_path(state_root: &Path, plugin_id: &str) -> PathBuf {
    state_root.join(plugin_id).join("authorization.json")
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
pub(super) fn authorization_record_for_descriptor(
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
pub(super) fn build_permission_preview(
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<VulcanPluginPermissionPreview, VulcanPluginRuntimeError> {
    let manifest = plugin.manifest.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new(format!(
            "Vulcan plugin `{}` has no valid manifest.",
            plugin.id
        ))
    })?;
    let permissions = plugin_required_permissions(plugin)?;
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

/// Derive the complete sorted capability keys from `plugin` using the sole manifest permission mapping.
/// 通过唯一清单权限映射，从 `plugin` 派生完整有序能力键。
/// Returns the canonical approval list, or rejects a descriptor without a validated manifest.
/// 返回规范批准列表，或拒绝不具有已校验清单的描述符。
pub(crate) fn plugin_required_permissions(
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<Vec<String>, VulcanPluginRuntimeError> {
    // Preview, immutable selection and runtime authorization must compare the same keys.
    // 预览、不可变选择及运行时授权必须比较相同键。
    let manifest = plugin.manifest.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Plugin permission inspection requires a valid manifest.")
    })?;
    // Ordering has one authority rather than independently normalized stored approvals.
    // 排序具有唯一权威，不独立规范化存储中的批准权限。
    let mut permissions = manifest
        .permissions
        .iter()
        .map(|permission| plugin_permission_key(*permission).to_string())
        .collect::<Vec<_>>();
    permissions.sort();
    Ok(permissions)
}

/// Returns the stable persisted key for one plugin permission.
/// 返回单个插件权限的稳定持久化键。
///
/// # Parameters
/// 参数
/// `capability` is the exact declared permission variant.
/// `capability` 是精确声明的权限变体。
///
/// # Returns
/// 返回值
/// Returns the unchanged stable persisted permission key.
/// 返回不变的稳定持久化权限键。
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
        VulcanPluginPermission::SystemVldbAccess => "system_vldb_access",
        VulcanPluginPermission::ModelInvoke => "model_invoke",
        VulcanPluginPermission::DependencyRegistryNetwork => "dependency_registry_network",
        VulcanPluginPermission::StorageSqlite => "storage_sqlite",
        VulcanPluginPermission::StorageLancedb => "storage_lancedb",
    }
}

/// Returns the stable host key for one dynamic UI mount point.
/// 返回单个动态 UI 挂载点的稳定宿主键。
///
/// # Parameters
/// 参数
/// `point` is the exact manifest UI mount point.
/// `point` 是精确的清单界面挂载点。
///
/// # Returns
/// 返回值
/// Returns the unchanged stable host mount key.
/// 返回不变的稳定宿主挂载键。
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
pub(super) fn plugin_is_authorized(
    state_root: &Path,
    plugin: &crate::VulcanPluginDescriptor,
) -> Result<bool, String> {
    if plugin.origin == VulcanPluginPackageOrigin::Bundled {
        return Ok(true);
    }
    // Invalid catalog placeholders cannot authorize a package or cause a secondary authority lookup.
    // 无效目录占位条目不能授权插件包，也不能导致查询第二权威。
    let required = plugin_required_permissions(plugin).map_err(|error| error.message)?;
    // A managed selection is the sole approval authority; never consult or repair a legacy sidecar.
    // 受管选择是唯一批准权威；绝不查询或修补旧附属文件。
    if let Some(managed) = plugin.managed_package() {
        return Ok(managed.approved_permissions() == required);
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
    Ok(record.schema_version == 1
        && record.plugin_id == plugin.id
        && Some(record.version.as_str()) == plugin.version.as_deref()
        && record.content_fingerprint == plugin.content_fingerprint
        && record.permissions == required)
}

impl VulcanPluginRuntimeManager {
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
        // Retain the real publication gate through ordinary authorization persistence and original selection.
        // 跨普通授权持久化及原选择保留真实发布门。
        let registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.require_routing_publication_locked(&registration, None)?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        if plugin.origin != VulcanPluginPackageOrigin::UserInstalled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` is bundled and does not accept user authorization records."
            )));
        }
        if plugin.managed_package().is_some() {
            return Err(VulcanPluginRuntimeError::new(
                "Managed plugin approval belongs to its activation transaction; legacy authorization writes are not allowed.",
            ));
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
}

#[cfg(test)]
mod tests;
