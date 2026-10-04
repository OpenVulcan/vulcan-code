//! Physical deletion uses only existing original Core collection and secure-store readback.
//! 物理删除仅使用既有原核心回收及安全存储回读。

use super::*;
use owner::{OriginalRemoval, UninstallResources};

/// Collect the exact real original after Full removal; plugin/resources retain authority throughout partial errors.
/// 在完整移除之后回收精确真实原实例；plugin／resources 在部分错误期间始终保留权威。
/// Returns confirmed local/history/credential absence, without removing the service owner.
/// 返回已确认本地／历史／凭据缺失，不删除服务所有者。
pub(super) fn collect(plugin: &str, resources: &mut UninstallResources) -> Result<(), String> {
    if resources.collection_confirmed {
        return Ok(());
    }
    if resources.committed_full.is_none()
        || resources.callback.is_some()
        || resources.callback_result.is_none()
    {
        return Err(String::from("原卸载完整发布或回调物理退役尚未确认。"));
    }
    // Existing OS backend addresses versioned journals and explicit legacy keys through separate contracts.
    // 既有操作系统后端通过独立契约寻址版本化日志及明确旧账户键。
    let backend = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
    match resources
        .original
        .as_mut()
        .ok_or("原卸载物理所有者缺失。")?
    {
        OriginalRemoval::Managed {
            store,
            selection,
            state: local,
        } => {
            if resources
                .runtime
                .collect_deactivation_state(local, selection)
                .map_err(|error| error.to_string())?
                != vulcan_plugin_runtime::VulcanPluginDeactivationStateObservation::Collected
            {
                return Err(String::from("原卸载本地状态尚未实际回收。"));
            }
            resources
                .runtime
                .collect_deactivation_flat_predecessor(local, selection)
                .map_err(|error| error.to_string())?;
            resources
                .runtime
                .confirm_deactivation_flat_predecessor_collected(local, selection)
                .map_err(|error| error.to_string())?;
            // The frozen real inventory includes historical and unfinished records, not only the selected package.
            // 固定真实清单包含历史及未完成记录，而非仅已选插件包。
            for (identity, _) in selection.package_inventory() {
                store
                    .collect_deactivation_inventory_package_retained(selection, identity)
                    .map_err(|error| error.to_string())?;
            }
            for record in selection.credential_inventory() {
                store
                    .collect_deactivation_inventory_credentials_retained(
                        selection,
                        record.revision(),
                        &backend,
                    )
                    .map_err(|error| error.to_string())?;
            }
            store
                .confirm_deactivation_inventory_collected_retained(selection, &backend)
                .map_err(|error| error.to_string())?;
        }
        OriginalRemoval::Flat(flat) => {
            if resources
                .runtime
                .collect_legacy_user_plugin_uninstallation(flat)
                .map_err(|error| error.to_string())?
                != vulcan_plugin_runtime::VulcanPluginLegacyUninstallationObservation::Collected
            {
                return Err(String::from("原平铺卸载尚未实际回收。"));
            }
            if resources
                .runtime
                .observe_legacy_user_plugin_uninstallation(flat)
                .map_err(|error| error.to_string())?
                != vulcan_plugin_runtime::VulcanPluginLegacyUninstallationObservation::Collected
            {
                return Err(String::from("原平铺卸载实际缺失回读失败。"));
            }
        }
    }
    // Legacy ownership was captured from actual declarations before effects; no versioned slot is guessed here.
    // 旧账户归属在副作用之前从实际声明捕获；此处不猜测版本化槽位。
    for (server, key) in &resources.legacy_keys {
        backend.delete_secret(plugin, server, key)?;
        if backend.read_optional_secret(plugin, server, key)?.is_some() {
            return Err(String::from("原卸载旧凭据实际缺失回读失败。"));
        }
    }
    resources.collection_confirmed = true;
    Ok(())
}
