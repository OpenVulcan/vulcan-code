//! Service glue of the in-app updater: one updater per process, its events, settings and RPCs.
//! 应用内更新器的服务胶水：每进程一个更新器，及其事件、设置与 RPC。

use std::sync::{Arc, OnceLock};

use vulcan_app_update::{AppUpdater, RestartPlan, UpdateHost};
use vulcan_provider_config::config::{
    PersistedAppUpdateConfiguration, PersistedAppUpdateRoute, ProviderConfigurationFile,
};
use vulcan_service_api::{
    AppUpdateRouteSetting, AppUpdateSettings, AppUpdateSnapshot, ServiceEvent, ServiceEventEmitter,
    ServiceRestartingEvent,
};

use crate::{
    ServiceShellHost, SharedServiceBridgeState, emit_service_event_with_trace,
    service_state_access::mutate_provider_configuration,
};

/// The updater of this process, created once by [`install_app_updater`].
/// 本进程的更新器，由 [`install_app_updater`] 创建一次。
static APP_UPDATER: OnceLock<AppUpdater> = OnceLock::new();

/// Reports updater changes to clients and hands restarts to the host shell.
/// 将更新器变化报告给客户端，并把重启交给宿主壳。
struct KernelUpdateHost {
    /// Event sink of every client.
    /// 每个客户端的事件出口。
    emitter: ServiceEventEmitter,
    /// The concrete service process.
    /// 具体的服务进程。
    shell_host: Arc<dyn ServiceShellHost>,
}

impl UpdateHost for KernelUpdateHost {
    fn changed(&self, snapshot: &AppUpdateSnapshot) {
        emit_service_event_with_trace(
            &self.emitter,
            "app-update",
            ServiceEvent::AppUpdateChanged(Box::new(snapshot.clone())),
        );
    }

    fn restart(&self, event: ServiceRestartingEvent, plan: RestartPlan) -> Result<(), String> {
        // Clients learn first, so the desktop can relaunch itself once the service is gone.
        // 先告知客户端，使桌面端在服务退出后能自行重新启动。
        emit_service_event_with_trace(
            &self.emitter,
            "service-restarting",
            ServiceEvent::ServiceRestarting(Box::new(event)),
        );
        self.shell_host.restart_service(plan)
    }
}

/// Effective update settings: the saved choices over the in-memory defaults.
/// 生效的更新设置：已保存的选择覆盖内存中的默认值。
pub(crate) fn app_update_settings(configuration: &ProviderConfigurationFile) -> AppUpdateSettings {
    let saved = configuration.app_update;
    AppUpdateSettings {
        preview: saved.preview.unwrap_or(false),
        route: match saved.route {
            None | Some(PersistedAppUpdateRoute::Auto) => AppUpdateRouteSetting::Auto,
            Some(PersistedAppUpdateRoute::International) => AppUpdateRouteSetting::International,
            Some(PersistedAppUpdateRoute::China) => AppUpdateRouteSetting::China,
        },
    }
}

/// Creates and starts the updater of this process; later calls do nothing.
/// 创建并启动本进程的更新器；之后的调用不做任何事。
pub(crate) fn install_app_updater(state: &SharedServiceBridgeState, emitter: &ServiceEventEmitter) {
    if APP_UPDATER.get().is_some() {
        return;
    }
    let shell_host = match state.lock() {
        Ok(guard) => Arc::clone(&guard.shell_host),
        Err(_) => return,
    };
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            tracing::warn!("[app-update] executable path is unknown: {error}");
            return;
        }
    };
    let settings = vulcan_provider_config::config::load_configuration_file()
        .map(|configuration| app_update_settings(&configuration))
        .unwrap_or_default();
    let updater = AppUpdater::new(
        &executable,
        settings,
        format!("VulcanCode/{}", vulcan_product_info::version()),
        Box::new(KernelUpdateHost {
            emitter: emitter.clone(),
            shell_host,
        }),
    );
    if APP_UPDATER.set(updater.clone()).is_ok() {
        updater.start();
    }
}

/// The updater of this process.
/// 本进程的更新器。
fn updater() -> Result<&'static AppUpdater, String> {
    APP_UPDATER
        .get()
        .ok_or_else(|| String::from("the updater is not running in this process"))
}

/// Current update state.
/// 当前更新状态。
pub(crate) fn service_app_update_status() -> Result<AppUpdateSnapshot, String> {
    Ok(updater()?.snapshot())
}

/// Starts a check for a newer version.
/// 开始检查更新版本。
pub(crate) fn service_check_app_update() -> Result<AppUpdateSnapshot, String> {
    updater()?.check()
}

/// Starts downloading the offered version.
/// 开始下载所提供的版本。
pub(crate) fn service_download_app_update() -> Result<AppUpdateSnapshot, String> {
    updater()?.download()
}

/// Cancels a running download or repair.
/// 取消进行中的下载或修复。
pub(crate) fn service_cancel_app_update() -> Result<AppUpdateSnapshot, String> {
    Ok(updater()?.cancel())
}

/// Restarts into the version made ready, or runs its installer.
/// 重启到已就绪的版本，或运行其安装包。
pub(crate) fn service_restart_to_update() -> Result<AppUpdateSnapshot, String> {
    updater()?.restart()
}

/// Starts repairing the running version.
/// 开始修复正在运行的版本。
pub(crate) fn service_repair_app_install() -> Result<AppUpdateSnapshot, String> {
    updater()?.repair()
}

/// Clears the notice about a version that failed to start.
/// 清除有关某版本启动失败的提示。
pub(crate) fn service_dismiss_app_update_notice() -> Result<AppUpdateSnapshot, String> {
    updater()?.dismiss_rollback()
}

/// Saves the update settings chosen by the user and applies them.
/// 保存用户选择的更新设置并应用。
pub(crate) async fn service_save_app_update_settings(
    state: SharedServiceBridgeState,
    settings: AppUpdateSettings,
) -> Result<AppUpdateSnapshot, String> {
    let updater = updater()?;
    // Persist only on explicit user action; loading never writes the defaults.
    // 仅在用户明确操作时持久化；加载时从不写入默认值。
    mutate_provider_configuration(
        &state,
        "Failed to save the update settings",
        |configuration| {
            configuration.app_update = persisted_settings(&settings);
            Ok(())
        },
        |_, _| (),
    )
    .await?;
    Ok(updater.set_settings(settings))
}

/// The saved form of `settings`: a value equal to its default is left out, so turning a choice
/// back removes it from the file instead of writing the default.
/// `settings` 的保存形式：等于默认值的值不写出，因此改回某项选择时会将其从文件中移除，而不是写入默认值。
fn persisted_settings(settings: &AppUpdateSettings) -> PersistedAppUpdateConfiguration {
    let defaults = AppUpdateSettings::default();
    PersistedAppUpdateConfiguration {
        preview: (settings.preview != defaults.preview).then_some(settings.preview),
        route: (settings.route != defaults.route).then(|| match settings.route {
            AppUpdateRouteSetting::Auto => PersistedAppUpdateRoute::Auto,
            AppUpdateRouteSetting::International => PersistedAppUpdateRoute::International,
            AppUpdateRouteSetting::China => PersistedAppUpdateRoute::China,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saving the defaults leaves nothing in the file; a changed choice is saved and read back.
    /// 保存默认值不会在文件中留下任何内容；改变的选择会被保存并读回。
    #[test]
    fn saved_settings_leave_defaults_out_and_keep_choices() {
        assert!(persisted_settings(&AppUpdateSettings::default()).is_default());
        let chosen = AppUpdateSettings {
            preview: true,
            route: AppUpdateRouteSetting::International,
        };
        let configuration = ProviderConfigurationFile {
            app_update: persisted_settings(&chosen),
            ..ProviderConfigurationFile::default()
        };
        assert_eq!(app_update_settings(&configuration), chosen);
    }

    /// Unsaved settings stay at their defaults and saved ones are read back exactly.
    /// 未保存的设置保持默认值，已保存的设置被准确读回。
    #[test]
    fn settings_default_in_memory_and_follow_saved_choices() {
        let configuration = ProviderConfigurationFile::default();
        assert_eq!(
            app_update_settings(&configuration),
            AppUpdateSettings::default()
        );
        let configuration = ProviderConfigurationFile {
            app_update: PersistedAppUpdateConfiguration {
                preview: Some(true),
                route: Some(PersistedAppUpdateRoute::China),
            },
            ..ProviderConfigurationFile::default()
        };
        assert_eq!(
            app_update_settings(&configuration),
            AppUpdateSettings {
                preview: true,
                route: AppUpdateRouteSetting::China,
            }
        );
    }
}
