//! Unattended mode defaults: the system deny policy, project replacements, and the prefill a
//! session's enable dialog reads.
//! 无人值守模式默认值：系统拒绝策略、项目替换值，以及会话开启对话框读取的预填值。

use vulcan_provider_config::provider_config::{
    load_project_configuration_file, save_project_configuration_file,
};
use vulcan_service_api::{
    ProjectUnattendedDefaultSaveRequest, ProjectUnattendedDefaultSnapshot,
    UnattendedDefaultRequest, UnattendedDefaultResponse, UnattendedSystemDefaultSaveRequest,
    UnattendedSystemDefaultSnapshot,
};
use vulcan_session_core::v1::config::MAX_UNATTENDED_DENY_POLICY_CHARS;

use super::services::{project_directory, read_state, save_settings};
use crate::{SharedServiceBridgeState, parse_session_id};

/// Refuses a deny policy longer than every layer accepts.
/// 拒绝超过各层允许长度的拒绝策略。
fn check_length(policy: &str) -> Result<(), String> {
    if policy.chars().count() > MAX_UNATTENDED_DENY_POLICY_CHARS {
        return Err(format!(
            "The deny policy is longer than {MAX_UNATTENDED_DENY_POLICY_CHARS} characters."
        ));
    }
    Ok(())
}

/// Returns the saved system default deny policy.
/// 返回已保存的系统默认拒绝策略。
pub(crate) fn service_unattended_system_default(
    state: SharedServiceBridgeState,
) -> Result<UnattendedSystemDefaultSnapshot, String> {
    let (_, settings) = read_state(&state)?;
    Ok(UnattendedSystemDefaultSnapshot {
        deny_policy: settings.unattended_deny_policy,
        max_chars: MAX_UNATTENDED_DENY_POLICY_CHARS,
    })
}

/// Saves the system default deny policy; running sessions keep the text they confirmed.
/// 保存系统默认拒绝策略；运行中的会话保留其已确认的文本。
pub(crate) async fn service_save_unattended_system_default(
    state: SharedServiceBridgeState,
    request: UnattendedSystemDefaultSaveRequest,
) -> Result<UnattendedSystemDefaultSnapshot, String> {
    check_length(&request.deny_policy)?;
    let settings = save_settings(&state, |settings| {
        settings.unattended_deny_policy = request.deny_policy;
        Ok(())
    })
    .await?;
    Ok(UnattendedSystemDefaultSnapshot {
        deny_policy: settings.unattended_deny_policy,
        max_chars: MAX_UNATTENDED_DENY_POLICY_CHARS,
    })
}

/// Projects one project's saved default next to the system default it would inherit.
/// 将一个项目已保存的默认值与其将继承的系统默认值一并投影。
fn project_snapshot(
    state: &SharedServiceBridgeState,
    project_id: String,
    deny_policy: Option<String>,
) -> Result<ProjectUnattendedDefaultSnapshot, String> {
    let (_, settings) = read_state(state)?;
    Ok(ProjectUnattendedDefaultSnapshot {
        project_id,
        deny_policy,
        system_deny_policy: settings.unattended_deny_policy,
        max_chars: MAX_UNATTENDED_DENY_POLICY_CHARS,
    })
}

/// Returns one project's default deny policy.
/// 返回一个项目的默认拒绝策略。
pub(crate) fn service_project_unattended_default(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<ProjectUnattendedDefaultSnapshot, String> {
    let directory = project_directory(&state, &project_id)?;
    let configuration = load_project_configuration_file(&directory)?;
    project_snapshot(
        &state,
        project_id,
        configuration.permission_judge.unattended_deny_policy,
    )
}

/// Saves one project's default deny policy; `None` returns it to the system default.
/// 保存一个项目的默认拒绝策略；`None` 使其恢复采用系统默认值。
pub(crate) fn service_save_project_unattended_default(
    state: SharedServiceBridgeState,
    request: ProjectUnattendedDefaultSaveRequest,
) -> Result<ProjectUnattendedDefaultSnapshot, String> {
    if let Some(policy) = &request.deny_policy {
        check_length(policy)?;
    }
    let directory = project_directory(&state, &request.project_id)?;
    let mut configuration = load_project_configuration_file(&directory)?;
    configuration.permission_judge.unattended_deny_policy = request.deny_policy;
    save_project_configuration_file(&directory, &configuration)?;
    project_snapshot(
        &state,
        request.project_id,
        configuration.permission_judge.unattended_deny_policy,
    )
}

/// Returns the text that prefills a session's enable dialog: the project default when its
/// project sets one, else the system default, else empty. Nothing is applied until the user
/// confirms the dialog.
/// 返回预填会话开启对话框的文本：其项目设置时为项目默认值，否则为系统默认值，再否则为空。
/// 用户确认对话框之前不会应用任何内容。
pub(crate) fn service_session_unattended_default(
    state: SharedServiceBridgeState,
    request: UnattendedDefaultRequest,
) -> Result<UnattendedDefaultResponse, String> {
    let session_id = parse_session_id(&request.session_id)
        .ok_or_else(|| String::from("Invalid unattended mode session identity."))?;
    let project_id = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("Workspace is not ready."))?;
        crate::workspace_session_with_archive_status(workspace, session_id)
            .ok_or_else(|| String::from("Unattended mode requires an existing regular session."))?
            .0
            .project_id
            .clone()
    };
    let project_policy = match project_id {
        Some(project_id) => {
            let directory = project_directory(&state, &project_id)?;
            load_project_configuration_file(&directory)?
                .permission_judge
                .unattended_deny_policy
        }
        None => None,
    };
    let (_, settings) = read_state(&state)?;
    let (deny_policy, source) = match project_policy {
        Some(policy) => (policy, "project"),
        None if !settings.unattended_deny_policy.is_empty() => {
            (settings.unattended_deny_policy, "system")
        }
        None => (String::new(), "none"),
    };
    Ok(UnattendedDefaultResponse {
        deny_policy,
        source: String::from(source),
        max_chars: MAX_UNATTENDED_DENY_POLICY_CHARS,
    })
}
