//! Revision-only secure backend for the durable plugin credential preparation journal.
//! 用于持久插件凭据准备日志的仅修订安全后端。

use super::*;
use vulcan_plugin_runtime::{VulcanPluginCredentialBackend, VulcanPluginCredentialKey};

impl VulcanPluginCredentialBackend for OsPluginMcpSecretStore {
    /// Read only `plugin_id`/`revision`/`key`; actual OS absence returns None without consulting legacy accounts.
    /// 仅读取 `plugin_id`／`revision`／`key`；实际 OS 缺失返回 None，不查询旧账户。
    fn read(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        match versioned_plugin_secret_entry(plugin_id, &key.server_id, &key.secret_key, revision)?
            .get_password()
        {
            Ok(value) => Ok(Some(value)),
            Err(KeyringError::NoEntry) => Ok(None),
            Err(_) => Err(String::from(
                "Failed to read the versioned plugin credential securely.",
            )),
        }
    }

    /// Write nonempty `value` to the exact tuple under the caller's staging lease; returns the actual OS result.
    /// 在调用方暂存租约下，将非空 `value` 写入精确元组；返回实际 OS 结果。
    fn write(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        if value.is_empty() {
            return Err(String::from(
                "Configured plugin credential values must not be empty.",
            ));
        }
        versioned_plugin_secret_entry(plugin_id, &key.server_id, &key.secret_key, revision)?
            .set_password(value)
            .map_err(|_| String::from("Failed to stage the plugin credential securely."))
    }

    /// Delete only the exact candidate tuple; returns success for actual OS absence and preserves other failures.
    /// 仅删除精确候选元组；实际 OS 缺失返回成功，其他失败保留。
    fn delete(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        match versioned_plugin_secret_entry(plugin_id, &key.server_id, &key.secret_key, revision)?
            .delete_credential()
        {
            Ok(()) | Err(KeyringError::NoEntry) => Ok(()),
            Err(_) => Err(String::from(
                "Failed to discard the plugin credential securely.",
            )),
        }
    }
}
