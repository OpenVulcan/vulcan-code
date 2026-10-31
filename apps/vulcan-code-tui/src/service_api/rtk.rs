// Mirror of `service/rtk-service.ts`.
// `service/rtk-service.ts` 的镜像。

use serde_json::{Value, json};

use super::{ServiceCall, Slot};

/// Builds the request for `service_rtk_settings`.
/// 构建 `service_rtk_settings` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_rtk_settings() -> ServiceCall {
    ServiceCall::load(
        "load_rtk_settings",
        "service_rtk_settings",
        json!({}),
        Slot::RtkSettings,
    )
}

/// Builds the request for `service_save_rtk_settings`; the next turn applies the settings.
/// 构建 `service_save_rtk_settings` 的请求；下一回合应用这些设置。
/// `settings` is the complete `RtkSettings` document.
/// `settings` 为完整的 `RtkSettings` 文档。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn save_rtk_settings(settings: Value) -> ServiceCall {
    ServiceCall::mutate(
        "save_rtk_settings",
        "service_save_rtk_settings",
        json!({ "request": { "settings": settings } }),
        &[Slot::RtkSettings],
    )
}

/// Builds the request for `service_rtk_statistics`; RTK failures arrive as an unavailable snapshot.
/// 构建 `service_rtk_statistics` 的请求；RTK 失败以不可用快照返回。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn load_rtk_statistics() -> ServiceCall {
    ServiceCall::load(
        "load_rtk_statistics",
        "service_rtk_statistics",
        json!({}),
        Slot::RtkStatistics,
    )
}

/// Builds the request for `service_clear_rtk_statistics`.
/// 构建 `service_clear_rtk_statistics` 的请求。
/// Returns an unexecuted ServiceCall; the backend owns transport and state refresh.
/// 返回尚未执行的 ServiceCall；传输与状态刷新由后端负责。
pub fn clear_rtk_statistics() -> ServiceCall {
    ServiceCall::mutate(
        "clear_rtk_statistics",
        "service_clear_rtk_statistics",
        json!({}),
        &[Slot::RtkStatistics],
    )
}
