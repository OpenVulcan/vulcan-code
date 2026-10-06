//! OS credential-store integration for plugin-scoped MCP secrets.
//! 插件作用域 MCP Secret 的操作系统凭据库存储集成。

use keyring::{Entry, Error as KeyringError};
use sha2::{Digest, Sha256};
use vulcan_agent_runtime::mcp::PluginMcpSecretStore;
use vulcan_plugin_runtime::{VulcanPluginCredentialBinding, VulcanPluginCredentialRevision};

mod staging;
#[cfg(test)]
mod tests;

/// Secure-store namespace owned by Vulcan Code for revisioned plugin credentials.
/// Vulcan Code 持有的修订化插件凭据安全存储命名空间。
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
            VulcanPluginCredentialBinding::Revision(revision) => {
                versioned_plugin_secret_entry(plugin_id, server_id, secret_key, revision)?
            }
            // No revision was ever selected, so no secure entry exists to read.
            // 从未选定修订，因此不存在可读取的安全条目。
            VulcanPluginCredentialBinding::Unconfigured => return Ok(None),
        };
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(_) => Err(String::from(
                "Failed to read the selected plugin credential securely.",
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
            VulcanPluginCredentialBinding::Revision(revision) => {
                versioned_plugin_secret_entry(plugin_id, server_id, secret_key, revision)?
            }
            VulcanPluginCredentialBinding::Unconfigured => {
                return Err(format!(
                    "Plugin MCP secret `{secret_key}` is not configured for `{plugin_id}:{server_id}`."
                ));
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
    // Revision accounts use one architecture-independent length encoding.
    // 修订账户使用单个与架构无关的长度编码。
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
