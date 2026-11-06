//! In-app update RPC routing.
//! 应用内更新 RPC 路由。

use super::*;
use crate::app_update_services;

/// Exact public RPC wire names owned by the update domain.
/// 更新领域持有的精确公开 RPC 线协议名称。
#[cfg(test)]
pub(super) const RPC_METHODS: &[&str] = &[
    "service_app_update_status",
    "service_check_app_update",
    "service_download_app_update",
    "service_cancel_app_update",
    "service_restart_to_update",
    "service_repair_app_install",
    "service_dismiss_app_update_notice",
    "service_install_desktop_profile",
    "service_save_app_update_settings",
];

/// Dispatches one exact update RPC.
/// 分发一个精确更新 RPC。
pub(super) async fn dispatch(
    state: SharedServiceBridgeState,
    method: &str,
    params: &Value,
) -> Result<ServiceRpcDispatch, WebuiServiceRpcError> {
    let result = match method {
        "service_app_update_status" => {
            rpc_json_result(app_update_services::service_app_update_status())
        }
        "service_check_app_update" => {
            rpc_json_result(app_update_services::service_check_app_update())
        }
        "service_download_app_update" => {
            rpc_json_result(app_update_services::service_download_app_update())
        }
        "service_cancel_app_update" => {
            rpc_json_result(app_update_services::service_cancel_app_update())
        }
        "service_restart_to_update" => {
            rpc_json_result(app_update_services::service_restart_to_update())
        }
        "service_repair_app_install" => {
            rpc_json_result(app_update_services::service_repair_app_install())
        }
        "service_dismiss_app_update_notice" => {
            rpc_json_result(app_update_services::service_dismiss_app_update_notice())
        }
        "service_install_desktop_profile" => {
            rpc_json_result(app_update_services::service_install_desktop_profile())
        }
        "service_save_app_update_settings" => rpc_json_result(
            app_update_services::service_save_app_update_settings(
                state,
                rpc_required_param(params, "settings")?,
            )
            .await,
        ),
        _ => return Ok(ServiceRpcDispatch::Unknown),
    };
    Ok(ServiceRpcDispatch::Handled(result))
}
