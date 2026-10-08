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
/// `native_host_ai_status` is the latest side-effect-free native capability validation snapshot.
/// `native_host_ai_status` 是最近一次无副作用的原生能力校验快照。
///
/// # Returns
/// 返回值
/// The complete VMM memory snapshot consumed by the WebUI memory surface.
/// 返回 WebUI 记忆界面使用的完整 VMM 记忆快照。
pub(crate) fn vmm_memory_snapshot(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    selected_runtime_state: Option<&SessionRuntimeStateEntry>,
    native_host_ai_status: Option<&crate::native_vmm::NativeVmmHostAiStatus>,
) -> VmmMemorySnapshot {
    let selected_session = selected_workspace_session(workspace);
    let selected_project = vmm_request_project_scope(
        workspace,
        selected_session.and_then(session_summary_project_id),
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
    let mut runtime =
        vmm_runtime_summary(selected_runtime_state, &effective, native_host_ai_status);
    if vmm_deployment_mode(&provider_configuration.vmm) == VmmDeploymentMode::External {
        let scope_configured =
            effective.diagnose_memory_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
        apply_external_vmm_health_probe(
            &effective.grpc_target,
            scope_configured,
            effective.precheck_enabled,
            effective.memory_writeback_enabled,
            &mut runtime,
        );
    }
    let experience_state = vmm_experience_state(&effective, &provider_configuration.vmm, &runtime);
    let prerequisites = vmm_prerequisites(&effective, &provider_configuration.vmm, &runtime);
    let capabilities = vmm_capabilities(&effective, &runtime);
    let recent_activity = VmmRecentActivitySummary {
        pending_writebacks: runtime.outbox_count,
        completed_turns: runtime.completed_turns,
        last_profile_update_at: runtime.updated_at.clone(),
    };
    // displayIdentity is hydrated from the global VMM ids and the runtime management catalog.
    // displayIdentity 由全局 VMM 标识与运行时管理目录解析，不复用 Vulcan Code 当前任务项目。
    let display_identity = VmmDisplayIdentitySummary::default();
    VmmMemorySnapshot {
        experience_state,
        prerequisites,
        capabilities,
        management_catalog: VmmManagementCatalogSummary::default(),
        recent_activity,
        display_identity,
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
                    memory_enabled: global_effective.enabled
                        && vulcan_vmm::VmmEffectiveConfig::resolve(
                            &provider_configuration.vmm,
                            Some(&project_vmm),
                            None,
                        )
                        .enabled,
                    project_id: project.id.clone(),
                    name: project.title.clone(),
                    path: project.path.clone(),
                    has_override: !project_vmm.is_empty(),
                    bound_project_id: project_vmm.project_id,
                }
            })
            .collect(),
        selected_project_id: selected_project.map(|project| project.id.clone()),
        selected_conversation_id: selected_session.map(|session| session.id.to_string()),
        runtime,
        sections: vmm_management_sections(effective.backend_mode),
        notice: vmm_memory_notice(&effective, selected_project),
        project_binding_notice: None,
    }
}

/// Resolves the globally configured VMM identity to human-readable catalog values.
/// 将全局配置的 VMM 身份解析为目录中的人类可读值。
///
/// # Parameters
/// 参数
/// `snapshot` is the product snapshot whose global draft and management catalog are current.
/// `snapshot` 是全局草稿与管理目录均已更新的产品快照。
///
/// # Returns
/// 返回值
/// This function returns nothing and updates only the user-facing default identity projection.
/// 本函数不返回值，仅更新面向用户的默认身份投影。
pub(crate) fn hydrate_vmm_default_identity(snapshot: &mut VmmMemorySnapshot) {
    let user_key = snapshot.global_draft.user_id.trim();
    snapshot.display_identity.user_name = snapshot
        .management_catalog
        .users
        .iter()
        .find(|user| user.stable_key == user_key)
        .map(|user| user.display_name.clone());
    let project_key = snapshot.global_draft.project_id.trim();
    let project = snapshot
        .management_catalog
        .projects
        .iter()
        .find(|project| project.stable_key == project_key);
    snapshot.display_identity.project_name = project.map(|project| project.display_name.clone());
    snapshot.display_identity.project_path = project.map(|project| project.display_path.clone());
}

/// Projects product-facing VMM state dimensions from authoritative configuration and runtime state.
/// 从权威配置与运行状态投影面向产品的 VMM 状态维度。
///
/// # Parameters
/// 参数
/// `effective` is the resolved VMM configuration.
/// `effective` 是解析后的 VMM 配置。
/// `global_layer` is the persisted global configuration layer.
/// `global_layer` 是持久化的全局配置层。
/// `runtime` is the current runtime summary.
/// `runtime` 是当前运行态摘要。
///
/// # Returns
/// 返回值
/// The independent deployment, setup, lifecycle, and health dimensions.
/// 返回相互独立的部署、设置、生命周期和健康维度。
fn vmm_experience_state(
    effective: &vulcan_vmm::VmmEffectiveConfig,
    global_layer: &vulcan_vmm::VmmConfigLayer,
    runtime: &VmmRuntimeSummary,
) -> VmmExperienceStateSummary {
    let deployment = vmm_deployment_mode(global_layer);
    let lifecycle = vmm_lifecycle_state(runtime);
    let required_configuration_ready =
        effective.diagnose_memory_configuration(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
    let setup = if deployment == VmmDeploymentMode::Disabled {
        VmmSetupState::NotConfigured
    } else if !required_configuration_ready {
        VmmSetupState::Incomplete
    } else if runtime.backend_error.is_some() {
        VmmSetupState::Blocked
    } else {
        VmmSetupState::Ready
    };
    let optional_capability_failed = runtime.precheck_error.is_some()
        || runtime.postaction_error.is_some()
        || runtime.profile_instruction_error.is_some()
        || runtime.rerank_error.is_some();
    let health = if !required_configuration_ready {
        VmmHealthState::Unavailable
    } else if runtime.process_ready {
        if optional_capability_failed {
            VmmHealthState::Degraded
        } else {
            VmmHealthState::Healthy
        }
    } else if runtime.backend_error.is_some() {
        VmmHealthState::Unavailable
    } else if deployment == VmmDeploymentMode::External {
        VmmHealthState::Unknown
    } else if matches!(lifecycle, VmmLifecycleState::Failed) {
        VmmHealthState::Unavailable
    } else {
        VmmHealthState::Unknown
    };
    VmmExperienceStateSummary {
        deployment,
        setup,
        lifecycle,
        health,
    }
}

/// Resolves the configured VMM deployment mode without inspecting error text.
/// 在不解析错误文本的前提下解析已配置的 VMM 部署方式。
///
/// # Parameters
/// 参数
/// `global_layer` is the persisted global VMM layer.
/// `global_layer` 是持久化的全局 VMM 配置层。
///
/// # Returns
/// 返回值
/// The configured backend, or Disabled only when no deployment has been selected.
/// 返回已配置后端；仅在尚未选择部署时返回 Disabled。
pub(super) fn vmm_deployment_mode(global_layer: &vulcan_vmm::VmmConfigLayer) -> VmmDeploymentMode {
    // A saved backend or legacy endpoint survives disabling memory. Runtime defaults alone
    // cannot prove setup; legacy enable-only configurations continue to select native memory.
    // 已保存后端或旧版端点在关闭记忆后仍然有效。运行时默认值不能证明已配置；
    // 仅保存启用状态的旧版配置仍选择原生记忆。
    match global_layer.backend_mode {
        Some(vulcan_vmm::VmmBackendMode::Native) => VmmDeploymentMode::Native,
        Some(vulcan_vmm::VmmBackendMode::ExternalGrpc) => VmmDeploymentMode::External,
        None if global_layer
            .grpc_target
            .as_deref()
            .is_some_and(|target| !target.trim().is_empty()) =>
        {
            VmmDeploymentMode::External
        }
        None if global_layer.enabled == Some(true) => VmmDeploymentMode::Native,
        None => VmmDeploymentMode::Disabled,
    }
}

/// Maps the native or external runtime state to a stable lifecycle enum.
/// 将原生或外置运行状态映射为稳定生命周期枚举。
///
/// # Parameters
/// 参数
/// `runtime` is the current runtime summary.
/// `runtime` 是当前运行态摘要。
///
/// # Returns
/// 返回值
/// A stable lifecycle value used by the WebUI.
/// 返回 WebUI 使用的稳定生命周期值。
fn vmm_lifecycle_state(runtime: &VmmRuntimeSummary) -> VmmLifecycleState {
    if runtime.backend_error.is_some() {
        VmmLifecycleState::Failed
    } else if runtime.process_ready {
        VmmLifecycleState::Running
    } else {
        VmmLifecycleState::Stopped
    }
}

/// Builds structured prerequisites from authoritative configuration fields.
/// 从权威配置字段构建结构化前置条件。
///
/// # Parameters
/// 参数
/// `effective` is the resolved VMM configuration.
/// `effective` 是解析后的 VMM 配置。
/// `global_layer` is the persisted global VMM layer.
/// `global_layer` 是持久化的全局 VMM 配置层。
/// `runtime` is the current runtime summary.
/// `runtime` 是当前运行态摘要。
///
/// # Returns
/// 返回值
/// The ordered list of setup and runtime prerequisites.
/// 返回有序的设置与运行前置条件列表。
fn vmm_prerequisites(
    effective: &vulcan_vmm::VmmEffectiveConfig,
    global_layer: &vulcan_vmm::VmmConfigLayer,
    runtime: &VmmRuntimeSummary,
) -> Vec<VmmPrerequisiteSummary> {
    let mut prerequisites = Vec::new();
    if !effective.enabled {
        prerequisites.push(vmm_prerequisite(
            "memory_enabled",
            "missing",
            "warning",
            "memory.vmm.prerequisite.memoryEnabled.title",
            "memory.vmm.prerequisite.memoryEnabled.description",
            Some("configure_rules"),
            Some("rules"),
            None,
        ));
    }
    let external = vmm_deployment_mode(global_layer) == VmmDeploymentMode::External;
    if external && effective.grpc_target.trim().is_empty() {
        prerequisites.push(vmm_prerequisite(
            "transport",
            "missing",
            "error",
            "memory.vmm.prerequisite.transport.title",
            "memory.vmm.prerequisite.transport.description",
            Some("configure_deployment"),
            Some("advanced"),
            None,
        ));
    }
    if external && effective.user_id.filter(|value| *value > 0).is_none() {
        prerequisites.push(vmm_prerequisite(
            "user",
            "missing",
            "error",
            "memory.vmm.prerequisite.user.title",
            "memory.vmm.prerequisite.user.description",
            Some("configure_identity"),
            Some("identity_projects"),
            None,
        ));
    }
    if external && effective.project_id.filter(|value| *value > 0).is_none() {
        prerequisites.push(vmm_prerequisite(
            "project",
            "missing",
            "error",
            "memory.vmm.prerequisite.project.title",
            "memory.vmm.prerequisite.project.description",
            Some("configure_identity"),
            Some("identity_projects"),
            None,
        ));
    }
    if let Some(error) = runtime.backend_error.clone() {
        prerequisites.push(vmm_prerequisite(
            if external {
                "external_transport"
            } else {
                "native_host_ai"
            },
            "failed",
            "error",
            if external {
                "memory.vmm.prerequisite.externalTransport.title"
            } else {
                "memory.vmm.prerequisite.inference.title"
            },
            if external {
                "memory.vmm.prerequisite.externalTransport.description"
            } else {
                "memory.vmm.prerequisite.inference.description"
            },
            Some(if external {
                "configure_deployment"
            } else {
                "configure_inference"
            }),
            Some(if external { "advanced" } else { "inference" }),
            Some(error),
        ));
    }
    prerequisites
}

/// Creates one structured VMM prerequisite.
/// 创建单个结构化 VMM 前置条件。
///
/// # Parameters
/// 参数
/// All parameters map directly to the service contract fields with no fallback lookup.
/// 所有参数直接映射到服务契约字段，不执行候选字段回退。
///
/// # Returns
/// 返回值
/// A complete prerequisite summary.
/// 返回完整前置条件摘要。
#[allow(clippy::too_many_arguments)]
fn vmm_prerequisite(
    id: &str,
    state: &str,
    severity: &str,
    title_key: &str,
    description_key: &str,
    action_kind: Option<&str>,
    action_target: Option<&str>,
    technical_detail: Option<String>,
) -> VmmPrerequisiteSummary {
    VmmPrerequisiteSummary {
        id: id.to_string(),
        state: state.to_string(),
        severity: severity.to_string(),
        title_key: title_key.to_string(),
        description_key: description_key.to_string(),
        technical_detail,
        action_kind: action_kind.map(str::to_string),
        action_target: action_target.map(str::to_string),
    }
}

/// Builds product-facing capability rows from effective settings and runtime checks.
/// 从最终设置与运行检查构建面向产品的能力行。
///
/// # Parameters
/// 参数
/// `effective` is the resolved VMM configuration.
/// `effective` 是解析后的 VMM 配置。
/// `runtime` is the current runtime summary.
/// `runtime` 是当前运行态摘要。
///
/// # Returns
/// 返回值
/// The stable capability list rendered by overview and settings pages.
/// 返回总览与设置页面渲染的稳定能力列表。
fn vmm_capabilities(
    effective: &vulcan_vmm::VmmEffectiveConfig,
    runtime: &VmmRuntimeSummary,
) -> Vec<VmmCapabilitySummary> {
    vec![
        vmm_capability(
            "automatic_recall",
            runtime.precheck_ready,
            effective.precheck_enabled,
            runtime.precheck_error.as_deref(),
            "inference",
        ),
        vmm_capability(
            "automatic_writeback",
            runtime.postaction_ready,
            effective.memory_writeback_enabled,
            runtime.postaction_error.as_deref(),
            "inference",
        ),
        vmm_capability(
            "ai_memory_tools",
            runtime.process_ready && runtime.scope_ready,
            effective.ai_tools_enabled,
            None,
            "rules",
        ),
        vmm_capability(
            "profile_instruction",
            runtime.profile_instruction_ready,
            true,
            runtime.profile_instruction_error.as_deref(),
            "inference",
        ),
        vmm_capability(
            "rerank",
            runtime.rerank_ready,
            true,
            runtime.rerank_error.as_deref(),
            "inference",
        ),
    ]
}

/// Creates one stable capability summary.
/// 创建单个稳定能力摘要。
///
/// # Parameters
/// 参数
/// `id` is the stable capability identifier.
/// `id` 是稳定能力标识。
/// `available` reports runtime availability.
/// `available` 表示运行时可用性。
/// `enabled` reports user configuration.
/// `enabled` 表示用户配置。
/// `error` is an optional controlled degradation reason.
/// `error` 是可选的受控降级原因。
/// `configure_target` is the owning settings page.
/// `configure_target` 是所属设置页面。
///
/// # Returns
/// 返回值
/// A complete capability summary.
/// 返回完整能力摘要。
fn vmm_capability(
    id: &str,
    available: bool,
    enabled: bool,
    error: Option<&str>,
    configure_target: &str,
) -> VmmCapabilitySummary {
    let state = if !enabled {
        "disabled"
    } else if available {
        "ready"
    } else if error.is_some() {
        "degraded"
    } else {
        "unavailable"
    };
    VmmCapabilitySummary {
        id: id.to_string(),
        available,
        enabled,
        state: state.to_string(),
        reason: error.map(str::to_string),
        configure_target: Some(configure_target.to_string()),
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
        // Backend ownership is global-only in VmmEffectiveConfig::resolve; stale project values are not executable.
        // VmmEffectiveConfig::resolve 规定后端仅由全局持有；旧项目层的值不是可执行后端。
        backend_mode: effective.backend_mode.as_str().to_string(),
        native_project_binding_mode: layer
            .native_project_binding_mode
            .unwrap_or(effective.native_project_binding_mode)
            .as_str()
            .to_string(),
        external_project_binding_mode: layer
            .external_project_binding_mode
            .unwrap_or(effective.external_project_binding_mode)
            .as_str()
            .to_string(),
        native_fixed_project_id: layer
            .native_fixed_project_id
            .unwrap_or(effective.native_fixed_project_id)
            .to_string(),
        external_project_prefix: layer
            .external_project_prefix
            .clone()
            .unwrap_or_else(|| effective.external_project_prefix.clone()),
        enabled: layer.enabled.unwrap_or(effective.enabled),
        grpc_target: layer
            .grpc_target
            .clone()
            .unwrap_or_else(|| effective.grpc_target.clone()),
        grpc_api_key: vulcan_service_api::redacted_webui_secret(
            layer
                .grpc_api_key
                .as_deref()
                .or(effective.grpc_api_key.as_deref()),
        ),
        management_base_url: layer.management_base_url.clone().unwrap_or_default(),
        management_bearer_token: vulcan_service_api::redacted_webui_secret(
            layer.management_bearer_token.as_deref(),
        ),
        management_timeout_ms: layer.management_timeout_ms.unwrap_or(30_000).to_string(),
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
    effective: &vulcan_vmm::VmmEffectiveConfig,
    native_host_ai_status: Option<&crate::native_vmm::NativeVmmHostAiStatus>,
) -> VmmRuntimeSummary {
    let native = effective.backend_mode == vulcan_vmm::VmmBackendMode::Native;
    let native_ready = native_host_ai_status.is_some_and(|status| status.ready);
    let scope_ready = effective.enabled
        && effective.diagnose_memory_flow(native) == vulcan_vmm::VmmConfigDiagnostic::Ready
        && (!native || native_ready);
    let native_backend_state = match native_host_ai_status {
        Some(status) if status.ready => "ready",
        Some(status) if status.checked => "unavailable",
        _ => "unchecked",
    };
    let backend_error = native
        .then(|| {
            native_host_ai_status
                .and_then(|status| status.error.clone())
                .or_else(|| {
                    (effective.enabled && !native_ready)
                        .then(|| String::from("原生 VMM 尚未完成宿主 AI 能力校验"))
                })
        })
        .flatten();
    let mut summary = VmmRuntimeSummary {
        process_ready: native && native_ready,
        backend_state: if native {
            native_backend_state.to_string()
        } else {
            String::from("external")
        },
        backend_error,
        scope_ready,
        precheck_ready: scope_ready && effective.precheck_enabled,
        postaction_ready: scope_ready && effective.memory_writeback_enabled,
        profile_instruction_ready: scope_ready
            && (!native
                || native_host_ai_status.is_some_and(|status| status.profile_instruction_ready)),
        profile_instruction_error: native
            .then(|| {
                native_host_ai_status.and_then(|status| status.profile_instruction_error.clone())
            })
            .flatten(),
        rerank_ready: scope_ready
            && (!native || native_host_ai_status.is_some_and(|status| status.rerank_ready)),
        rerank_error: native
            .then(|| native_host_ai_status.and_then(|status| status.rerank_error.clone()))
            .flatten(),
        ..VmmRuntimeSummary::default()
    };
    if let Some(runtime_state) = runtime_state {
        summary.outbox_count = runtime_state.vmm_writeback_outbox.len();
        summary.profile_sync_count = runtime_state.vmm_profile_sync.nodes_by_id.len();
        summary.completed_turns = runtime_state.vmm_profile_sync.completed_turns_since_sync;
        summary.updated_at = runtime_state.vmm_profile_sync.updated_at.clone();
        summary.profile_preview = runtime_state
            .vmm_profile_sync
            .base_profile_content
            .as_deref()
            .map(compact_panel_preview)
            .unwrap_or_default();
    }
    summary
}

/// Returns only the VMM management sections supported by the selected backend.
/// 仅返回所选后端支持的 VMM 管理分区。
///
/// # Parameters
/// 参数
/// `backend_mode` is the globally selected native or external backend.
/// `backend_mode` 是全局选择的原生或外置后端。
///
/// # Returns
/// 返回值
/// The backend-specific ordered WebUI section list.
/// 返回后端专用的有序 WebUI 分区列表。
pub(crate) fn vmm_management_sections(
    backend_mode: vulcan_vmm::VmmBackendMode,
) -> Vec<VmmManagementSectionSummary> {
    // Native pages expose one-level projects plus their host-owned resource and retention controls.
    // 原生页面暴露一级项目及其宿主持有的资源与保留控制。
    let native = backend_mode == vulcan_vmm::VmmBackendMode::Native;
    let content_description = if native {
        "使用自然语言搜索、查看、写入和删除当前绑定原生项目的长期记忆。"
    } else {
        "按会话和回合查看、归档、回收、恢复并维护外置 VMM 长期记忆。"
    };
    let identity_label = if native {
        "记忆项目"
    } else {
        "用户与项目"
    };
    let identity_description = if native {
        "管理一级原生记忆项目、项目归属、连带召回、迁移、清空和删除。"
    } else {
        "管理外置 VMM 身份、项目绑定、迁移和删除。"
    };
    [
        (
            "overview",
            "OVERVIEW",
            "状态总览",
            "查看记忆服务状态、完成首次配置并处理阻塞问题。",
        ),
        (
            "inference",
            "INFERENCE",
            "内部推理",
            "为记忆检索、整理和画像任务选择专用模型。",
        ),
        (
            "rules",
            "RULES",
            "记忆规则",
            "编辑记忆规则，并应用到全局或指定项目。",
        ),
        (
            "resource_rules",
            "RESOURCE RULES",
            "规则文件",
            "按提示词、PII 和噪音分类查看、测试并安全覆盖原生 VMM 规则文件。",
        ),
        ("content", "CONTENT", "记忆数据", content_description),
        (
            "identity_projects",
            "IDENTITY AND PROJECTS",
            identity_label,
            identity_description,
        ),
        (
            "profile",
            "PROFILE",
            "画像管理",
            "查看画像摘要并应用人工画像指令。",
        ),
        (
            "maintenance",
            "RETENTION",
            "清洗与回收",
            "查看周期清洗状态、立即运行清洗，并恢复或永久清除回收批次。",
        ),
        (
            "advanced",
            "ADVANCED",
            "高级设置",
            "选择内置原生或外置 gRPC 后端，并查看对应配置与技术诊断。",
        ),
    ]
    .into_iter()
    .filter(|(id, _, _, _)| {
        native || !matches!(*id, "inference" | "resource_rules" | "maintenance")
    })
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
