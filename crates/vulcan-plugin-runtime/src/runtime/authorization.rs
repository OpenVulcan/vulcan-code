//! Exact plugin authorization records and sanitized permission previews.
//! 精确插件授权记录与脱敏权限预览。

use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use crate::{VulcanPluginMcpTransportManifest, VulcanPluginPackageOrigin, VulcanPluginPermission};
use serde::{Deserialize, Serialize};

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

/// Checks whether one effective plugin descriptor carries a current complete approval.
/// 检查一个生效插件描述符是否带有当前完整批准。
///
/// # Parameters
/// 参数
/// `plugin` is the exact effective package generation.
/// `plugin` 是精确的生效插件代次。
///
/// # Returns
/// 返回值
/// Returns true for bundled plugins or a managed selection approving exactly the required permissions; an unavailable
/// user row without a managed selection is never authorized.
/// 对内建插件或批准集合恰为所需权限的受管选择返回 true；没有受管选择的不可用用户条目永不授权。
pub(super) fn plugin_is_authorized(plugin: &crate::VulcanPluginDescriptor) -> Result<bool, String> {
    if plugin.origin == VulcanPluginPackageOrigin::Bundled {
        return Ok(true);
    }
    // The managed selection is the sole approval authority.
    // 受管选择是唯一批准权威。
    let Some(managed) = plugin.managed_package() else {
        return Ok(false);
    };
    // Invalid catalog placeholders cannot authorize a package.
    // 无效目录占位条目不能授权插件包。
    let required = plugin_required_permissions(plugin).map_err(|error| error.message)?;
    Ok(managed.approved_permissions() == required)
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
}

#[cfg(test)]
mod tests;
