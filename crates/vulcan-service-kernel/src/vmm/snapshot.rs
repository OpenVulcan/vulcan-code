//! VMM snapshot helpers.
//! VMM snapshot 辅助逻辑。

use super::*;

/// Creates the detailed VMM memory snapshot for the WebUI memory area.
/// 为 WebUI 记忆区域创建详细 VMM 记忆快照。
///
/// # Parameters
/// 参数
/// `workspace` is the retained workspace snapshot used to resolve project scopes.
/// `workspace` 是用于解析项目作用域的保留工作区快照。
/// `provider_configuration` is the shared provider configuration containing global VMM settings.
/// `provider_configuration` 是包含全局 VMM 设置的共享 Provider 配置。
/// `selected_runtime_state` is the optional runtime state for the selected session.
/// `selected_runtime_state` 是当前选中会话的可选运行态。
///
/// # Returns
/// 返回值
/// The complete VMM memory snapshot consumed by the WebUI memory surface.
/// 返回 WebUI 记忆界面使用的完整 VMM 记忆快照。
pub(crate) fn vmm_memory_snapshot(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    selected_runtime_state: Option<&SessionRuntimeStateEntry>,
) -> VmmMemorySnapshot {
    let selected_session = selected_workspace_session(workspace);
    let selected_project = vmm_request_project_scope(
        workspace,
        selected_session.and_then(|session| session.project_id.as_deref()),
    );
    let project_layer = selected_project
        .map(|project| cached_project_vmm_layer(&project.path))
        .unwrap_or_default();
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        &provider_configuration.vmm,
        Some(&project_layer),
        selected_runtime_state.map(|state| &state.vmm),
    );
    let global_effective =
        vulcan_vmm::VmmEffectiveConfig::resolve(&provider_configuration.vmm, None, None);
    let project_effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        &provider_configuration.vmm,
        Some(&project_layer),
        None,
    );
    VmmMemorySnapshot {
        effective: vmm_effective_summary(&effective),
        global_draft: vmm_config_draft_summary(
            &provider_configuration.vmm,
            &global_effective,
            !provider_configuration.vmm.is_empty(),
            true,
        ),
        project_draft: vmm_config_draft_summary(
            &project_layer,
            &project_effective,
            !project_layer.is_empty(),
            false,
        ),
        project_scopes: workspace
            .task_projects
            .iter()
            .map(|project| {
                let project_vmm = cached_project_vmm_layer(&project.path);
                VmmProjectScopeSummary {
                    project_id: project.id.clone(),
                    name: project.title.clone(),
                    path: project.path.clone(),
                    has_override: !project_vmm.is_empty(),
                }
            })
            .collect(),
        selected_project_id: selected_project.map(|project| project.id.clone()),
        selected_conversation_id: selected_session.map(|session| session.id.to_string()),
        runtime: vmm_runtime_summary(selected_runtime_state),
        sections: vmm_management_sections(),
        notice: vmm_memory_notice(&effective, selected_project),
    }
}

/// Projects one effective VMM configuration into WebUI summary fields.
/// 将单个 VMM 最终配置投影为 WebUI 摘要字段。
pub(crate) fn vmm_effective_summary(
    effective: &vulcan_vmm::VmmEffectiveConfig,
) -> VmmEffectiveSummary {
    let grpc_configured = !effective.grpc_target.trim().is_empty();
    VmmEffectiveSummary {
        enabled: effective.enabled,
        grpc_target: effective.grpc_target.clone(),
        grpc_configured,
        user_id: effective.user_id,
        project_id: effective.project_id,
        diagnostic: vmm_diagnostic_key(effective.diagnose_memory_flow(grpc_configured)),
        precheck_diagnostic: vmm_diagnostic_key(effective.diagnose_precheck_flow(grpc_configured)),
        writeback_diagnostic: vmm_diagnostic_key(
            effective.diagnose_writeback_flow(grpc_configured),
        ),
        ai_tools_enabled: effective.ai_tools_enabled,
        precheck_enabled: effective.precheck_enabled,
        memory_writeback_enabled: effective.memory_writeback_enabled,
        implicit_memory_turns: effective.implicit_memory_turns,
        profile_refresh_turns: effective.profile_refresh_turns,
        profile_node_limit: effective.profile_node_limit,
        injection_tag: effective.injection_tag.clone(),
        injection_description: effective.injection_description.clone(),
    }
}

/// Projects one VMM config layer and effective fallback into an editable draft.
/// 将单个 VMM 配置层与最终回退值投影为可编辑草稿。
///
/// # Parameters
/// 参数
/// `layer` is the persisted VMM configuration layer being edited.
/// `layer` 是正在编辑的持久化 VMM 配置层。
/// `effective` is the already-resolved fallback configuration.
/// `effective` 是已经解析完成的回退配置。
/// `has_override` records whether this layer should be shown as explicitly overridden.
/// `has_override` 记录该层是否应显示为显式覆盖。
///
/// # Returns
/// 返回值
/// The editable WebUI draft summary with layer values falling back to effective values.
/// 返回按最终配置回退后的 WebUI 可编辑草稿摘要。
pub(crate) fn vmm_config_draft_summary(
    layer: &vulcan_vmm::VmmConfigLayer,
    effective: &vulcan_vmm::VmmEffectiveConfig,
    has_override: bool,
    force_explicit_policy_modes: bool,
) -> VmmConfigDraftSummary {
    VmmConfigDraftSummary {
        has_override,
        enabled: layer.enabled.unwrap_or(effective.enabled),
        grpc_target: layer
            .grpc_target
            .clone()
            .unwrap_or_else(|| effective.grpc_target.clone()),
        grpc_api_key: layer
            .grpc_api_key
            .clone()
            .or_else(|| effective.grpc_api_key.clone())
            .unwrap_or_default(),
        user_id: layer
            .user_id
            .or(effective.user_id)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        project_id: layer
            .project_id
            .or(effective.project_id)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        receive_timeout_ms: layer
            .receive_timeout_ms
            .unwrap_or(effective.receive_timeout_ms)
            .to_string(),
        handshake_timeout_ms: layer
            .handshake_timeout_ms
            .unwrap_or(effective.handshake_timeout_ms)
            .to_string(),
        max_receive_message_bytes: layer
            .max_receive_message_bytes
            .or(effective.max_receive_message_bytes)
            .map(|value| value.to_string())
            .unwrap_or_default(),
        grpc_keepalive_time_ms: layer
            .grpc_keepalive_time_ms
            .unwrap_or(effective.grpc_keepalive_time_ms)
            .to_string(),
        grpc_keepalive_timeout_ms: layer
            .grpc_keepalive_timeout_ms
            .unwrap_or(effective.grpc_keepalive_timeout_ms)
            .to_string(),
        grpc_keepalive_permit_without_calls: layer
            .grpc_keepalive_permit_without_calls
            .unwrap_or(effective.grpc_keepalive_permit_without_calls),
        implicit_memory_turns: layer
            .implicit_memory_turns
            .unwrap_or(effective.implicit_memory_turns)
            .to_string(),
        profile_refresh_turns: layer
            .profile_refresh_turns
            .unwrap_or(effective.profile_refresh_turns)
            .to_string(),
        profile_node_limit: layer
            .profile_node_limit
            .unwrap_or(effective.profile_node_limit)
            .to_string(),
        ai_tools_enabled: layer.ai_tools_enabled.unwrap_or(effective.ai_tools_enabled),
        precheck_enabled: layer.precheck_enabled.unwrap_or(effective.precheck_enabled),
        precheck_mode: vmm_layer_policy_mode_for_draft(
            layer.precheck_enabled,
            effective.precheck_enabled,
            force_explicit_policy_modes,
        ),
        memory_writeback_enabled: layer
            .memory_writeback_enabled
            .unwrap_or(effective.memory_writeback_enabled),
        memory_writeback_mode: vmm_layer_policy_mode_for_draft(
            layer.memory_writeback_enabled,
            effective.memory_writeback_enabled,
            force_explicit_policy_modes,
        ),
        injection_tag: layer
            .injection_tag
            .clone()
            .unwrap_or_else(|| effective.injection_tag.clone()),
        injection_description: layer
            .injection_description
            .clone()
            .unwrap_or_else(|| effective.injection_description.clone()),
    }
}

/// Converts one optional layer policy into a draft mode key.
/// 将单个可选层级策略转换为草稿模式键。
fn vmm_layer_policy_mode_for_draft(
    value: Option<bool>,
    effective: bool,
    force_explicit: bool,
) -> String {
    if value.is_none() && force_explicit {
        return vmm_layer_policy_mode(Some(effective));
    }
    vmm_layer_policy_mode(value)
}

/// Converts one optional layer policy into the WebUI tri-state mode key.
/// 将单个可选层级策略转换为 WebUI 三态模式键。
fn vmm_layer_policy_mode(value: Option<bool>) -> String {
    match value {
        None => String::from("inherit"),
        Some(true) => String::from("enabled"),
        Some(false) => String::from("disabled"),
    }
}

/// Summarizes selected-session VMM runtime state.
/// 汇总当前选中会话的 VMM 运行态。
pub(crate) fn vmm_runtime_summary(
    runtime_state: Option<&SessionRuntimeStateEntry>,
) -> VmmRuntimeSummary {
    let Some(runtime_state) = runtime_state else {
        return VmmRuntimeSummary::default();
    };
    VmmRuntimeSummary {
        outbox_count: runtime_state.vmm_writeback_outbox.len(),
        profile_sync_count: runtime_state.vmm_profile_sync.nodes_by_id.len(),
        completed_turns: runtime_state.vmm_profile_sync.completed_turns_since_sync,
        updated_at: runtime_state.vmm_profile_sync.updated_at.clone(),
        profile_preview: runtime_state
            .vmm_profile_sync
            .base_profile_content
            .as_deref()
            .map(compact_panel_preview)
            .unwrap_or_default(),
    }
}

/// Returns the VMM management sections owned by the WebUI workbench.
/// 返回由 WebUI 工作台持有的 VMM 管理分区。
pub(crate) fn vmm_management_sections() -> Vec<VmmManagementSectionSummary> {
    [
        (
            "user_manager",
            "USER MANAGER",
            "用户管理",
            "解析、创建预检和删除长期用户。",
        ),
        (
            "project_manager",
            "PROJECT MANAGER",
            "项目管理",
            "解析、创建预检、删除和迁移项目路径。",
        ),
        (
            "profile_center",
            "PROFILE CENTER",
            "画像中心",
            "查看 active profile nodes 并应用手工画像指令。",
        ),
        (
            "profile_bundle",
            "PROFILE BUNDLE TEST",
            "画像 Bundle 测试",
            "读取当前 user/project 的完整画像 Bundle。",
        ),
        (
            "tools_debug",
            "TOOLS DEBUG",
            "工具调试",
            "模拟 AI 专属工具检索、详情展开和主动写入。",
        ),
        (
            "memory_settings",
            "MEMORY SETTINGS",
            "记忆设置",
            "配置记忆开关、TTL、画像刷新和注入策略。",
        ),
        (
            "grpc_transport",
            "GRPC TRANSPORT",
            "gRPC 传输",
            "配置全局 VMM 连接和传输参数。",
        ),
    ]
    .into_iter()
    .map(
        |(id, key, label, description)| VmmManagementSectionSummary {
            id: id.to_string(),
            key: key.to_string(),
            label: label.to_string(),
            description: description.to_string(),
        },
    )
    .collect()
}

/// Builds a compact VMM page notice.
/// 构建紧凑的 VMM 页面提示。
pub(crate) fn vmm_memory_notice(
    effective: &vulcan_vmm::VmmEffectiveConfig,
    selected_project: Option<&TaskProjectSummary>,
) -> String {
    format!(
        "当前项目：{}；诊断：{}；用户/项目：{}/{}。",
        selected_project
            .map(|project| project.title.as_str())
            .unwrap_or("未选择项目"),
        vmm_diagnostic_key(
            effective.diagnose_memory_flow(!effective.grpc_target.trim().is_empty())
        ),
        effective
            .user_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("-")),
        effective
            .project_id
            .map(|value| value.to_string())
            .unwrap_or_else(|| String::from("-")),
    )
}

/// Returns a stable key for one VMM diagnostic.
/// 返回单个 VMM 诊断的稳定键。
pub(crate) fn vmm_diagnostic_key(diagnostic: vulcan_vmm::VmmConfigDiagnostic) -> String {
    match diagnostic {
        vulcan_vmm::VmmConfigDiagnostic::Ready => "ready",
        vulcan_vmm::VmmConfigDiagnostic::DisabledByConfig => "disabled_by_config",
        vulcan_vmm::VmmConfigDiagnostic::MissingGrpcTarget => "missing_grpc_target",
        vulcan_vmm::VmmConfigDiagnostic::GrpcOffline => "grpc_offline",
        vulcan_vmm::VmmConfigDiagnostic::MissingUserId => "missing_user_id",
        vulcan_vmm::VmmConfigDiagnostic::MissingProjectId => "missing_project_id",
    }
    .to_string()
}
