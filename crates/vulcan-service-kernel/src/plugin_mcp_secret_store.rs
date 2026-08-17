//! OS credential-store integration for plugin-scoped MCP secrets.
//! 插件作用域 MCP Secret 的操作系统凭据库存储集成。

use keyring::{Entry, Error as KeyringError};
use sha2::{Digest, Sha256};
use vulcan_agent_runtime::mcp::PluginMcpSecretStore;

/// Stable OS credential service name owned by Vulcan Code.
/// Vulcan Code 持有的稳定操作系统凭据服务名。
const VULCAN_PLUGIN_MCP_SECRET_SERVICE: &str = "vulcan-code.plugin-mcp";

/// Stateless adapter over the current platform's secure credential store.
/// 当前平台安全凭据库的无状态适配器。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OsPluginMcpSecretStore;

impl OsPluginMcpSecretStore {
    /// Reads one exact secret for transactional rollback without treating absence as an error.
    /// 为事务回滚读取单个精确 Secret，且不把不存在视为错误。
    pub(crate) fn read_optional_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
    ) -> Result<Option<String>, String> {
        match plugin_secret_entry(plugin_id, server_id, secret_key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(_) => Err(String::from(
                "Failed to read the plugin MCP secret securely.",
            )),
        }
    }

    /// Stores or replaces one exact plugin-scoped MCP secret.
    /// 存储或替换单个精确插件作用域 MCP Secret。
    ///
    /// # Parameters
    /// 参数
    /// Plugin, server, and secret identifiers select the namespace; `value` is never persisted elsewhere.
    /// 插件、服务器与 Secret 标识选择命名空间；`value` 绝不会持久化到其他位置。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the operating-system credential store accepts the value.
    /// 操作系统凭据库接受该值后返回成功。
    pub(crate) fn write_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
        value: &str,
    ) -> Result<(), String> {
        if value.is_empty() {
            return Err(String::from("Plugin MCP secret value must not be empty."));
        }
        plugin_secret_entry(plugin_id, server_id, secret_key)?
            .set_password(value)
            .map_err(|_| String::from("Failed to store the plugin MCP secret securely."))
    }

    /// Deletes one exact plugin-scoped MCP secret.
    /// 删除单个精确插件作用域 MCP Secret。
    ///
    /// # Parameters
    /// 参数
    /// Plugin, server, and secret identifiers select the complete credential namespace.
    /// 插件、服务器与 Secret 标识选择完整凭据命名空间。
    ///
    /// # Returns
    /// 返回值
    /// Returns success when the credential is absent or has been deleted.
    /// 凭据不存在或已经删除时返回成功。
    pub(crate) fn delete_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
    ) -> Result<(), String> {
        match plugin_secret_entry(plugin_id, server_id, secret_key)?.delete_credential() {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(String::from(
                "Failed to delete the plugin MCP secret securely.",
            )),
        }
    }
}

impl PluginMcpSecretStore for OsPluginMcpSecretStore {
    /// Reads one exact secret scoped to a plugin MCP server.
    /// 读取限定到某个插件 MCP 服务器的精确 Secret。
    fn read_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
    ) -> Result<String, String> {
        match plugin_secret_entry(plugin_id, server_id, secret_key)?.get_password() {
            Ok(value) => Ok(value),
            Err(KeyringError::NoEntry) => Err(format!(
                "Plugin MCP secret `{secret_key}` is not configured for `{plugin_id}:{server_id}`."
            )),
            Err(_) => Err(format!(
                "Plugin MCP secret `{secret_key}` could not be read securely for `{plugin_id}:{server_id}`."
            )),
        }
    }
}

/// Builds one OS credential entry from a collision-resistant length-delimited namespace digest.
/// 根据抗碰撞的长度分隔命名空间摘要构建单个操作系统凭据条目。
fn plugin_secret_entry(
    plugin_id: &str,
    server_id: &str,
    secret_key: &str,
) -> Result<Entry, String> {
    if plugin_id.is_empty() || server_id.is_empty() || secret_key.is_empty() {
        return Err(String::from(
            "Plugin MCP secret namespace fields must not be empty.",
        ));
    }
    let mut digest = Sha256::new();
    for component in [plugin_id, server_id, secret_key] {
        digest.update(component.len().to_be_bytes());
        digest.update(component.as_bytes());
    }
    let account = format!("sha256:{}", hex::encode(digest.finalize()));
    Entry::new(VULCAN_PLUGIN_MCP_SECRET_SERVICE, &account)
        .map_err(|_| String::from("Failed to open the operating-system credential store."))
}
