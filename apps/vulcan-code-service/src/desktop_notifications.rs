//! Native service failure notifications without a webview or IPC framework.
//! 不依赖网页视图或 IPC 框架的原生服务故障通知。

/// Stable service notification identity retained from the former native shell configuration.
/// 从原生外壳旧配置保留的稳定服务通知身份。
const SERVICE_NOTIFICATION_ID: &str = "ai.vulcan.code";

/// Submits a plain-text service failure using the existing OS identity and returns delivery errors.
/// 使用既有系统身份提交纯文本服务故障，并返回投递错误。
/// This never registers identities or changes notification permissions; the Electron GUI owns user notification setup.
/// 此操作不注册身份或修改通知权限；用户通知设置由 Electron GUI 持有。
pub(crate) fn show_service_failure(message: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    notify_rust::set_application(SERVICE_NOTIFICATION_ID).map_err(|error| error.to_string())?;
    let mut notification = notify_rust::Notification::new();
    notification.summary("Vulcan Code Service").body(message);
    #[cfg(windows)]
    notification.app_id(SERVICE_NOTIFICATION_ID);
    #[cfg(not(any(windows, target_os = "macos")))]
    notification.appname(SERVICE_NOTIFICATION_ID);
    notification
        .show()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
