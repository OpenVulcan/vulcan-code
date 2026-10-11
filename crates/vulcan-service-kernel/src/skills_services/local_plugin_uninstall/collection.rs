//! Physical deletion uses only existing original Core collection and secure-store readback.
//! 物理删除仅使用既有原核心回收及安全存储回读。

use super::*;
use owner::{OriginalRemoval, UninstallResources};

/// Collect the exact real original after Full removal; resources retain authority throughout partial errors.
/// 在完整移除之后回收精确真实原实例；resources 在部分错误期间始终保留权威。
/// Returns confirmed local/history/credential absence, without removing the service owner.
/// 返回已确认本地／历史／凭据缺失，不删除服务所有者。
pub(super) fn collect(resources: &mut UninstallResources) -> Result<(), String> {
    if resources.collection_confirmed {
        return Ok(());
    }
    if resources.committed_full.is_none()
        || resources.callback.is_some()
        || resources.callback_result.is_none()
    {
        return Err(String::from(
            "The full publication or callback physical retirement of the original uninstall is not confirmed yet.",
        ));
    }
    // Existing OS backend addresses the versioned credential journals.
    // 既有操作系统后端寻址版本化凭据日志。
    let backend = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
    let OriginalRemoval {
        store,
        selection,
        state: local,
    } = resources
        .original
        .as_mut()
        .ok_or("The physical owner of the original uninstall is missing.")?;
    if resources
        .runtime
        .collect_deactivation_state(local, selection)
        .map_err(|error| error.to_string())?
        != vulcan_plugin_runtime::VulcanPluginDeactivationStateObservation::Collected
    {
        return Err(String::from(
            "The local state of the original uninstall has not actually been reclaimed yet.",
        ));
    }
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
    resources.collection_confirmed = true;
    Ok(())
}
