// Mirror of `service/app-update-service.ts`.
// `service/app-update-service.ts` 的镜像。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

/// Builds the request for `service_app_update_status`.
/// 构建 `service_app_update_status` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_app_update_status() -> ServiceCall {
    ServiceCall::load(
        "load_app_update_status",
        "service_app_update_status",
        json!({}),
        Slot::AppUpdate,
    )
}

/// Builds one parameterless updater call that answers with a fresh snapshot.
/// 构建一个无参数、以新快照应答的更新器调用。
fn updater_call(function: &'static str, method: &'static str) -> ServiceCall {
    ServiceCall::mutate(function, method, json!({}), &[Slot::AppUpdate])
}

/// Builds the request for `service_check_app_update`.
/// 构建 `service_check_app_update` 的请求。
pub fn check_app_update() -> ServiceCall {
    updater_call("check_app_update", "service_check_app_update")
}

/// Builds the request for `service_download_app_update`.
/// 构建 `service_download_app_update` 的请求。
pub fn download_app_update() -> ServiceCall {
    updater_call("download_app_update", "service_download_app_update")
}

/// Builds the request for `service_cancel_app_update`.
/// 构建 `service_cancel_app_update` 的请求。
pub fn cancel_app_update() -> ServiceCall {
    updater_call("cancel_app_update", "service_cancel_app_update")
}

/// Builds the request for `service_restart_to_update`.
/// 构建 `service_restart_to_update` 的请求。
pub fn restart_to_update() -> ServiceCall {
    updater_call("restart_to_update", "service_restart_to_update")
}

/// Builds the request for `service_repair_app_install`.
/// 构建 `service_repair_app_install` 的请求。
pub fn repair_app_install() -> ServiceCall {
    updater_call("repair_app_install", "service_repair_app_install")
}

/// Builds the request for `service_dismiss_app_update_notice`.
/// 构建 `service_dismiss_app_update_notice` 的请求。
pub fn dismiss_app_update_notice() -> ServiceCall {
    updater_call(
        "dismiss_app_update_notice",
        "service_dismiss_app_update_notice",
    )
}

/// Builds the request for `service_install_desktop_profile`.
/// 构建 `service_install_desktop_profile` 的请求。
pub fn install_desktop_profile() -> ServiceCall {
    updater_call("install_desktop_profile", "service_install_desktop_profile")
}

/// Builds the request for `service_save_app_update_settings`.
/// 构建 `service_save_app_update_settings` 的请求。
/// `settings` is the complete `AppUpdateSettings` document.
/// `settings` 为完整的 `AppUpdateSettings` 文档。
pub fn save_app_update_settings(settings: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_app_update_settings",
        "service_save_app_update_settings",
        json!({ "settings": settings }),
        &[Slot::AppUpdate],
    )
}
