//! OS credential-store integration for plugin-scoped MCP secrets.
//! 插件作用域 MCP Secret 的操作系统凭据库存储集成。

use keyring::{Entry, Error as KeyringError};
use sha2::{Digest, Sha256};
use vulcan_agent_runtime::mcp::PluginMcpSecretStore;
use vulcan_plugin_runtime::{VulcanPluginCredentialBinding, VulcanPluginCredentialRevision};

mod staging;
#[cfg(test)]
mod tests;

/// Stable OS credential service name owned by Vulcan Code.
/// Vulcan Code 持有的稳定操作系统凭据服务名。
const VULCAN_PLUGIN_MCP_SECRET_SERVICE: &str = "vulcan-code.plugin-mcp";
/// Separate secure-store namespace for explicitly versioned credentials; legacy accounts are never probed.
/// 显式版本凭据的独立安全存储命名空间；绝不探测旧账户。
const VERSIONED_PLUGIN_MCP_SECRET_SERVICE: &str = "vulcan-code.plugin-mcp.versioned-v1";

/// Stateless adapter over the current platform's secure credential store.
/// 当前平台安全凭据库的无状态适配器。
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct OsPluginMcpSecretStore;

impl OsPluginMcpSecretStore {
    /// Read an optional slot from binding's exact namespace for an explicit immutable update.
    /// 为显式不可变更新，从 binding 的精确命名空间读取可选槽位。
    /// The plugin/server/key tuple is already declared; absence returns None without probing another revision.
    /// 插件／服务器／键元组已经声明；缺失返回 None，不探测另一修订。
    pub(crate) fn read_selected_optional_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
        binding: &VulcanPluginCredentialBinding,
    ) -> Result<Option<String>, String> {
        // Namespace selection is explicit and identical to ordinary descriptor-bound execution.
        // 命名空间选择显式且与普通描述符绑定执行一致。
        let entry = match binding {
            VulcanPluginCredentialBinding::Legacy => {
                plugin_secret_entry(plugin_id, server_id, secret_key)?
            }
            VulcanPluginCredentialBinding::Revision(revision) => {
                versioned_plugin_secret_entry(plugin_id, server_id, secret_key, revision)?
            }
        };
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(_) => Err(String::from(
                "Failed to read the selected plugin credential securely.",
            )),
        }
    }

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
        binding: &VulcanPluginCredentialBinding,
    ) -> Result<String, String> {
        // Select exactly once from the descriptor's declared namespace; missing values never fall back.
        // 从描述符声明命名空间精确选择一次；缺失值绝不回退。
        let entry = match binding {
            VulcanPluginCredentialBinding::Legacy => {
                plugin_secret_entry(plugin_id, server_id, secret_key)?
            }
            VulcanPluginCredentialBinding::Revision(revision) => {
                versioned_plugin_secret_entry(plugin_id, server_id, secret_key, revision)?
            }
        };
        match entry.get_password() {
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

/// Open the exact isolated secure entry for `plugin_id`, `server_id`, `secret_key` and `revision`.
/// 打开 `plugin_id`、`server_id`、`secret_key` 及 `revision` 的精确隔离安全条目。
/// Returns one backend entry without reading, overwriting or falling back to another revision.
/// 返回单个后端条目，不读取、覆盖或回退其他修订。
fn versioned_plugin_secret_entry(
    plugin_id: &str,
    server_id: &str,
    secret_key: &str,
    revision: &VulcanPluginCredentialRevision,
) -> Result<Entry, String> {
    // Revision accounts use one architecture-independent length encoding; legacy account bytes remain untouched.
    // 修订账户使用单个与架构无关的长度编码；旧账户字节保持不变。
    let account = versioned_plugin_secret_account(plugin_id, server_id, secret_key, revision)?;
    Entry::new(VERSIONED_PLUGIN_MCP_SECRET_SERVICE, &account)
        .map_err(|_| String::from("Failed to open the operating-system credential store."))
}

/// Derive a stable account for one exact revision tuple; returns a digest without credential values.
/// 为单个精确修订元组派生稳定账户；返回不含凭据值的摘要。
/// All namespace components must be nonempty and contain no NUL; boundaries use unsigned 64-bit big-endian lengths.
/// 所有命名空间组件必须非空且不含空字符；边界使用无符号六十四位大端长度。
fn versioned_plugin_secret_account(
    plugin_id: &str,
    server_id: &str,
    secret_key: &str,
    revision: &VulcanPluginCredentialRevision,
) -> Result<String, String> {
    // Explicit framing prevents component concatenation collisions without normalizing declared identifiers.
    // 显式分帧防止组件串接冲突，不规范化声明标识符。
    let mut digest = Sha256::new();
    for component in [plugin_id, revision.as_str(), server_id, secret_key] {
        if component.is_empty() || component.contains('\0') {
            return Err(String::from(
                "Plugin credential namespace fields must be nonempty and contain no NUL.",
            ));
        }
        digest.update((component.len() as u64).to_be_bytes());
        digest.update(component.as_bytes());
    }
    Ok(format!("sha256:{}", hex::encode(digest.finalize())))
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
