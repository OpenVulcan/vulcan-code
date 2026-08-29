//! VMM WebUI management and snapshot adapters.
//! VMM WebUI 管理与快照适配逻辑。

use std::{collections::BTreeMap, path::PathBuf};

use serde_json::{Value, json};
use vulcan_agent_runtime::provider_config::ProviderConfigurationFile;
use vulcan_context_injection::InjectionKind;
use vulcan_service_api::{
    VmmCapabilitySummary, VmmConfigDraftSummary, VmmConfigSaveRequest, VmmDataSpaceSummary,
    VmmDeploymentMode, VmmDisplayIdentitySummary, VmmEffectiveSummary, VmmExperienceStateSummary,
    VmmHealthState, VmmLifecycleState, VmmManagementCatalogSummary, VmmManagementSectionSummary,
    VmmMemoryActionRequest, VmmMemoryActionResult, VmmMemorySnapshot, VmmPrerequisiteSummary,
    VmmProjectScopeSummary, VmmRecentActivitySummary, VmmRuntimeSummary, VmmSetupState,
    VmmStaticProfileDateGroupSummary, VmmStaticProfileNodeSummary, VmmStaticProfileSectionSummary,
    VmmStaticProfileSummary, WEBUI_REDACTED_SECRET_PLACEHOLDER, WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionRuntimeStateEntry, TaskProjectSummary};

use crate::{
    SharedServiceBridgeState, compact_panel_preview, current_unix_millis_text,
    provider_configuration_from_state, refresh_workbench_snapshot, selected_workspace_session,
    session_summary_project_id,
    vmm_hooks::{
        cached_project_vmm_layer, refresh_exact_vmm_hook_project_projection,
        refresh_guard_vmm_hook_snapshot, refresh_service_vmm_hook_snapshot,
        update_cached_project_vmm_layer,
    },
    vmm_project_scope_path, vmm_request_project_scope,
};

mod config;
mod external_health;
mod health_monitor;
mod management_http;
mod memory_actions;
mod snapshot;
mod static_profile;

pub(super) use config::*;
use external_health::*;
pub(super) use health_monitor::*;
pub(super) use memory_actions::*;
pub(super) use snapshot::*;
pub(super) use static_profile::*;

/// Returns the deployment mode selected by the current authoritative global configuration.
/// 返回当前权威全局配置选择的部署模式。
///
/// # Parameters
/// 参数
/// `global_layer` is the current persisted global VMM configuration.
/// `global_layer` 是当前持久化的全局 VMM 配置。
///
/// # Returns
/// 返回值
/// The explicit disabled, managed, or external deployment mode.
/// 返回明确的禁用、托管或外部部署模式。
pub(crate) fn current_vmm_deployment_mode(
    global_layer: &vulcan_vmm::VmmConfigLayer,
) -> VmmDeploymentMode {
    vmm_deployment_mode(global_layer)
}

/// Resolves current external VMM service readiness from authoritative configuration and health state.
/// 从权威配置与健康状态解析当前外部 VMM 服务就绪事实。
///
/// # Parameters
/// 参数
/// `global_layer` is the current persisted global VMM configuration.
/// `global_layer` 是当前持久化的全局 VMM 配置。
///
/// # Returns
/// 返回值
/// `true` only for a fully configured external endpoint with a matching successful probe.
/// 仅对配置完整且拥有匹配成功探测的外部端点返回 `true`。
pub(crate) fn current_external_vmm_service_ready(
    global_layer: &vulcan_vmm::VmmConfigLayer,
) -> bool {
    if vmm_deployment_mode(global_layer) != VmmDeploymentMode::External {
        return false;
    }
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(global_layer, None, None);
    let scope_configured =
        effective.diagnose_memory_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
    external_vmm_service_ready(&effective.grpc_target, scope_configured)
}

#[cfg(test)]
mod tests;
