//! VMM WebUI management and snapshot adapters.
//! VMM WebUI 管理与快照适配逻辑。

use std::{collections::BTreeMap, path::PathBuf};

use serde_json::{Value, json};
use vulcan_agent_runtime::provider_config::ProviderConfigurationFile;
use vulcan_context_injection::InjectionKind;
use vulcan_service_api::{
    VmmConfigDraftSummary, VmmConfigSaveRequest, VmmEffectiveSummary, VmmManagementSectionSummary,
    VmmMemoryActionRequest, VmmMemoryActionResult, VmmMemorySnapshot, VmmProjectScopeSummary,
    VmmRuntimeSummary, VmmStaticProfileDateGroupSummary, VmmStaticProfileNodeSummary,
    VmmStaticProfileSectionSummary, VmmStaticProfileSummary, WEBUI_REDACTED_SECRET_PLACEHOLDER,
    WorkbenchSnapshot,
};
use vulcan_workbench_model::{SessionRuntimeStateEntry, TaskProjectSummary};

use crate::{
    SharedServiceBridgeState, compact_panel_preview, current_unix_millis_text,
    persist_workspace_session_update, provider_configuration_from_state,
    refresh_workbench_snapshot, selected_session_runtime_state_mut, selected_workspace_session,
    vmm_hooks::{
        cached_project_vmm_layer, refresh_guard_vmm_hook_snapshot,
        refresh_service_vmm_hook_snapshot, update_cached_project_vmm_layer,
    },
    vmm_project_scope_path, vmm_request_project_scope,
};

mod config;
mod memory_actions;
mod snapshot;
mod static_profile;

pub(super) use config::*;
pub(super) use memory_actions::*;
pub(super) use snapshot::*;
pub(super) use static_profile::*;

#[cfg(test)]
mod tests;
