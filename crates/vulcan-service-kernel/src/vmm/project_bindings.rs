//! Automatic VMM project binding for newly added VulcanCode projects.
//! 新增 VulcanCode 项目的 VMM 自动绑定逻辑。

use super::*;
use crate::mutate_project_configuration;
use vulcan_service_api::VmmProjectBindingNotice;

/// Applies the authoritative global backend binding policy after one project is persisted.
/// 在一个项目完成持久化后应用权威的全局后端绑定策略。
///
/// # Parameters
/// `state` owns the configured backend and native project manager.
/// `state` 持有所配置的后端与原生项目管理器。
/// `project` is the newly persisted VulcanCode project whose stable id is its VMM tag.
/// `project` 是刚持久化的 VulcanCode 项目，其稳定标识同时作为 VMM 标签。
///
/// # Returns
/// Returns no notice while VMM is disabled, otherwise the exact native or external binding outcome.
/// VMM 禁用时不返回通知，否则返回准确的原生或外置绑定结果。
pub(crate) async fn bind_new_vulcan_project_to_vmm(
    state: &SharedServiceBridgeState,
    project: &TaskProjectSummary,
) -> Option<VmmProjectBindingNotice> {
    let configuration = match provider_configuration_from_state(state) {
        Ok(configuration) => configuration,
        Err(error) => {
            return Some(binding_failure_notice(
                project,
                format!("读取 VMM 全局配置失败：{error}"),
            ));
        }
    };
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&configuration.vmm, None, None);
    if !effective.enabled {
        return None;
    }
    if effective.backend_mode == vulcan_vmm::VmmBackendMode::ExternalGrpc {
        return Some(match effective.external_project_binding_mode {
            vulcan_vmm::VmmNativeProjectBindingMode::Fixed => {
                bind_new_external_fixed_project(project, &configuration.vmm, &effective).await
            }
            vulcan_vmm::VmmNativeProjectBindingMode::PerProject => {
                bind_new_external_project(state, project, &configuration.vmm, &effective).await
            }
        });
    }
    let maintenance = match state.lock() {
        Ok(guard) => guard.native_vmm_maintenance.clone(),
        Err(error) => {
            return Some(binding_failure_notice(
                project,
                format!("锁定原生 VMM 项目管理器失败：{error}"),
            ));
        }
    };
    let Some(maintenance) = maintenance else {
        return Some(binding_failure_notice(
            project,
            String::from("原生 VMM 项目管理器尚未就绪。"),
        ));
    };
    let result = match effective.native_project_binding_mode {
        vulcan_vmm::VmmNativeProjectBindingMode::Fixed => {
            maintenance
                .bind_existing_project(&project.id, effective.native_fixed_project_id)
                .await
        }
        vulcan_vmm::VmmNativeProjectBindingMode::PerProject => {
            maintenance
                .create_and_bind_project(&project.id, &project.id)
                .await
        }
    };
    match result {
        Ok(vmm_project) => Some(VmmProjectBindingNotice {
            vulcan_project_id: project.id.clone(),
            status: String::from("bound"),
            vmm_project_name: Some(vmm_project.name.clone()),
            native_vmm_project_id: Some(vmm_project.id),
            message: format!("已绑定到 VMM 记忆项目 {}。", vmm_project.name),
        }),
        Err(error) => Some(binding_failure_notice(project, error)),
    }
}

/// Resolves the configured fixed external VMM project without writing a project override.
/// 解析已配置的固定外置 VMM 项目，不写入项目级覆盖。
///
/// # Parameters
/// `project` is the newly persisted VulcanCode project inheriting the global owner.
/// `project` 是继承全局所有者的新增 VulcanCode 项目。
/// `layer` is the exact external transport configuration.
/// `layer` 是准确的外置传输配置。
/// `effective` carries the required global fixed project id.
/// `effective` 携带必需的全局固定项目标识。
///
/// # Returns
/// Returns an explicit inherited-binding success or retryable failure notice.
/// 返回明确的继承绑定成功通知或可重试失败通知。
async fn bind_new_external_fixed_project(
    project: &TaskProjectSummary,
    layer: &vulcan_vmm::VmmConfigLayer,
    effective: &vulcan_vmm::VmmEffectiveConfig,
) -> VmmProjectBindingNotice {
    let Some(project_id) = effective.project_id.filter(|project_id| *project_id > 0) else {
        return binding_failure_notice(project, String::from("外置 VMM 固定项目 ID 未配置。"));
    };
    let outcome = vulcan_service_runtime::vmm_management::run_vmm_workbench_action(
        vulcan_service_runtime::vmm_management::VmmWorkbenchAction::LoadUsersAndProjects,
        layer.clone(),
        String::new(),
        String::new(),
    )
    .await;
    let remote_projects = match outcome {
        Ok(vulcan_service_runtime::vmm_management::VmmManagementOutcome::UsersAndProjects {
            projects,
            ..
        }) => projects.projects,
        Ok(_) => {
            return binding_failure_notice(
                project,
                String::from("外置 VMM 返回了不匹配的项目目录结果。"),
            );
        }
        Err(error) => return binding_failure_notice(project, error),
    };
    let Some(vmm_project) = remote_projects
        .into_iter()
        .find(|candidate| candidate.project_id == project_id)
    else {
        return binding_failure_notice(project, format!("外置 VMM 固定项目 {project_id} 不存在。"));
    };
    VmmProjectBindingNotice {
        vulcan_project_id: project.id.clone(),
        status: String::from("bound"),
        vmm_project_name: Some(vmm_project.project_name.clone()),
        native_vmm_project_id: None,
        message: format!("已绑定到 VMM 记忆项目 {}。", vmm_project.project_name),
    }
}

/// Ensures the configured external Team/Space/project path and stores its returned project id.
/// 确保已配置的外置 Team/Space/project 路径存在，并保存其返回的项目标识。
///
/// # Parameters
/// `state` owns project configuration persistence and hook projection refresh.
/// `state` 持有项目配置持久化与 hook 投影刷新。
/// `project` is the newly persisted VulcanCode project.
/// `project` 是刚持久化的 VulcanCode 项目。
/// `layer` is the exact external transport configuration.
/// `layer` 是准确的外置传输配置。
/// `effective` carries the normalized automatic project prefix.
/// `effective` 携带规范化后的自动项目创建前缀。
///
/// # Returns
/// Returns one explicit success or retryable failure notice.
/// 返回一项明确的成功通知或可重试失败通知。
async fn bind_new_external_project(
    state: &SharedServiceBridgeState,
    project: &TaskProjectSummary,
    layer: &vulcan_vmm::VmmConfigLayer,
    effective: &vulcan_vmm::VmmEffectiveConfig,
) -> VmmProjectBindingNotice {
    let project_path = format!("{}/{}", effective.external_project_prefix, project.id);
    let outcome = vulcan_service_runtime::vmm_management::run_vmm_workbench_action(
        vulcan_service_runtime::vmm_management::VmmWorkbenchAction::EnsureProject {
            project_path,
            confirm_create: true,
        },
        layer.clone(),
        String::new(),
        String::new(),
    )
    .await;
    let response = match outcome {
        Ok(vulcan_service_runtime::vmm_management::VmmManagementOutcome::EnsureProject(
            response,
        )) => response,
        Ok(_) => {
            return binding_failure_notice(
                project,
                String::from("外置 VMM 返回了不匹配的项目创建结果。"),
            );
        }
        Err(error) => return binding_failure_notice(project, error),
    };
    let Some(vmm_project) = response.project else {
        return binding_failure_notice(project, response.message);
    };
    let project_config_path = project.path.clone();
    let persist_result = mutate_project_configuration(
        &std::path::PathBuf::from(&project_config_path),
        "保存外置 VMM 项目 ID 绑定失败",
        |project_configuration| {
            project_configuration.vmm.project_id = Some(vmm_project.project_id);
            Ok(())
        },
        |project_configuration| {
            update_cached_project_vmm_layer(
                &project_config_path,
                project_configuration.vmm.clone(),
            );
            refresh_exact_vmm_hook_project_projection(
                state,
                &project_config_path,
                &project_configuration.vmm,
            )
        },
    )
    .await;
    if let Err(error) = persist_result {
        return binding_failure_notice(project, error);
    }
    VmmProjectBindingNotice {
        vulcan_project_id: project.id.clone(),
        status: String::from("bound"),
        vmm_project_name: Some(vmm_project.project_name.clone()),
        native_vmm_project_id: None,
        message: format!("已绑定到 VMM 记忆项目 {}。", vmm_project.project_name),
    }
}

/// Builds the explicit non-fatal notice returned when project creation succeeds but binding fails.
/// 构建项目创建成功但绑定失败时返回的明确非致命通知。
///
/// # Parameters
/// `project` is the successfully persisted VulcanCode project.
/// `project` 是已成功持久化的 VulcanCode 项目。
/// `error` is the exact binding failure returned by the host data plane.
/// `error` 是宿主数据平面返回的准确绑定失败。
///
/// # Returns
/// Returns a retry-oriented failure notice without changing project-creation success.
/// 返回面向重试的失败通知，不改变项目创建成功结果。
fn binding_failure_notice(project: &TaskProjectSummary, error: String) -> VmmProjectBindingNotice {
    VmmProjectBindingNotice {
        vulcan_project_id: project.id.clone(),
        status: String::from("failed"),
        vmm_project_name: None,
        native_vmm_project_id: None,
        message: format!("VMM 记忆项目绑定失败：{error}。可在项目设置中重新绑定或创建后绑定。"),
    }
}
