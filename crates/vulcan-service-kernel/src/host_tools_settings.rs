//! Native host-tool settings projection.
//! 原生宿主工具设置投影。

use serde::Deserialize;
use vulcan_agent_runtime::provider_config::ProviderConfigurationFile;
use vulcan_host_tools::shell_host::{HostCommandShell, HostCommandShellSupport};
use vulcan_service_api::{
    CommandShellOptionSummary, CommandShellSettingsSummary, HostToolsSettingsSummary,
    NativeToolSummary, ServiceEventEmitter, WorkbenchSnapshot,
};

use crate::{
    SharedServiceBridgeState, emit_workbench_event, mutate_provider_configuration,
    refresh_settings_projection_locked,
};

/// Builds the WebUI native-tool settings snapshot from shared configuration.
/// 从共享配置构建 WebUI 原生工具设置快照。
///
/// # Parameters
/// 参数
/// `provider_configuration` supplies the persisted default shell.
/// `provider_configuration` 提供持久化的默认 Shell。
///
/// # Returns
/// 返回值
/// Returns the shell settings with an empty native-tool collection until runtime rows are supplied.
/// 返回 Shell 设置；运行时行被提供前原生工具集合为空。
pub(crate) fn host_tools_settings(
    provider_configuration: &ProviderConfigurationFile,
) -> HostToolsSettingsSummary {
    host_tools_settings_with_native_tools(provider_configuration, Vec::new())
}

/// Builds the WebUI native-tool settings snapshot with runtime rows.
/// 使用运行时行构建 WebUI 原生工具设置快照。
///
/// # Parameters
/// 参数
/// `provider_configuration` supplies the persisted default shell and `native_tools` contains exact runtime descriptors.
/// `provider_configuration` 提供持久化的默认 Shell，`native_tools` 包含精确运行时描述符。
///
/// # Returns
/// 返回值
/// Returns the supplied runtime-native tool descriptors with the shell settings.
/// 返回提供的运行时原生工具描述符及 Shell 设置。
pub(crate) fn host_tools_settings_with_native_tools(
    provider_configuration: &ProviderConfigurationFile,
    native_tools: Vec<NativeToolSummary>,
) -> HostToolsSettingsSummary {
    HostToolsSettingsSummary {
        native_tools,
        command_shell: command_shell_settings(provider_configuration),
    }
}

/// Projects the persisted default shell against the shells detected on this host.
/// 将持久化的默认 Shell 与本机检测到的 Shell 一起投影。
fn command_shell_settings(
    provider_configuration: &ProviderConfigurationFile,
) -> CommandShellSettingsSummary {
    let configured = provider_configuration.default_command_shell.clone();
    let support = HostCommandShellSupport::from_configured_default_id(configured.as_deref());
    CommandShellSettingsSummary {
        configured_shell_id: configured,
        resolved_shell_id: support
            .auto_shell()
            .map(|entry| entry.shell.id().to_owned()),
        options: support
            .supported_shells()
            .iter()
            .map(|entry| CommandShellOptionSummary {
                id: entry.shell.id().to_owned(),
                label: entry.shell.display_label().to_owned(),
            })
            .collect(),
    }
}

/// Default shell save request sent by the WebUI settings page.
/// WebUI 设置页发送的默认 Shell 保存请求。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DefaultCommandShellSaveRequest {
    /// Shell to prefer, or none to let the host pick automatically.
    /// 偏好的 Shell；为空时由宿主自动选择。
    shell_id: Option<String>,
}

/// Saves the default shell shared by the integrated terminal and the command tools.
/// 保存集成终端与命令工具共用的默认 Shell。
///
/// # Parameters
/// 参数
/// `app` publishes the refreshed workbench, `state` owns the configuration, and `request` names the shell.
/// `app` 发布刷新后的工作台，`state` 持有配置，`request` 指定 Shell。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot, or an error when the shell is not detected on this host.
/// 刷新后的工作台快照；Shell 未在本机检测到时返回错误。
pub(crate) async fn service_save_default_command_shell(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: DefaultCommandShellSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    if let Some(shell_id) = request.shell_id.as_deref() {
        let detected = HostCommandShell::parse(shell_id).is_some_and(|shell| {
            HostCommandShellSupport::from_configured_default_id(None)
                .supported_shells()
                .iter()
                .any(|entry| entry.shell == shell)
        });
        if !detected {
            return Err(format!("Shell `{shell_id}` is not available on this host."));
        }
    }
    let shell_id = request.shell_id;
    let workbench = mutate_provider_configuration(
        &state,
        "Failed to save the default shell",
        |configuration| {
            configuration.default_command_shell = shell_id;
            Ok(())
        },
        |guard, configuration| {
            refresh_settings_projection_locked(guard, configuration);
            guard.workbench.clone()
        },
    )
    .await?;
    emit_workbench_event(&app, &workbench);
    Ok(workbench)
}
