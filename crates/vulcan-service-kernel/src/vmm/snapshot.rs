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
    managed_runtime: Option<&crate::managed_memory_mesh_runtime::ManagedMemoryMeshSnapshot>,
) -> VmmMemorySnapshot {
    let selected_session = selected_workspace_session(workspace);
    let selected_project = vmm_request_project_scope(
        workspace,
        selected_session.and_then(session_summary_project_id),
    );
    let project_layer = selected_project
        .map(|project| cached_project_vmm_layer(&project.path))
        .unwrap_or_default();
    let mut effective_global_layer = provider_configuration.vmm.clone();
    if let Some(grpc_target) = crate::vmm_hooks::managed_vmm_public_target() {
        effective_global_layer.enabled = Some(true);
        effective_global_layer.grpc_target = Some(grpc_target);
        effective_global_layer.grpc_api_key = None;
    }
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        &effective_global_layer,
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
    let mut runtime = vmm_runtime_summary(selected_runtime_state, managed_runtime);
    if vmm_deployment_mode(&provider_configuration.vmm) == VmmDeploymentMode::External {
        let scope_configured =
            effective.diagnose_memory_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
        apply_external_vmm_health_probe(&effective.grpc_target, scope_configured, &mut runtime);
    }
    let experience_state = vmm_experience_state(&effective, &provider_configuration.vmm, &runtime);
    let prerequisites = vmm_prerequisites(&effective, &provider_configuration.vmm, &runtime);
    let capabilities = vmm_capabilities(&effective, &runtime);
    let data_space_state = vmm_data_space_summary(runtime.data_space_state);
    let maintenance = crate::managed_memory_mesh_maintenance::runtime().snapshot();
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
        data_space_state,
        maintenance,
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
        sections: vmm_management_sections(),
        notice: vmm_memory_notice(&effective, selected_project),
        project_binding_notice: None,
    }
}

/// Rebuilds every product-facing VMM state dimension after a managed-runtime transition.
/// 在托管运行时发生状态转换后重建所有面向产品的 VMM 状态维度。
///
/// # Parameters
/// 参数
/// `retained_snapshot` owns the explicit in-memory management catalog that cannot be rebuilt from workspace files.
/// `retained_snapshot` 持有无法从工作区文件重建的显式内存管理目录。
/// `workspace` is the authoritative retained workspace used to resolve the selected session and project.
/// `workspace` 是用于解析当前会话与项目的权威保留工作区。
/// `provider_configuration` is the authoritative provider configuration containing the global VMM layer.
/// `provider_configuration` 是包含全局 VMM 配置层的权威 Provider 配置。
/// `managed_runtime` is the exact managed child-process lifecycle snapshot.
/// `managed_runtime` 是准确的托管子进程生命周期快照。
///
/// # Returns
/// 返回值
/// Returns one internally consistent VMM snapshot while preserving the explicit management catalog.
/// 返回一份内部一致且保留显式管理目录的 VMM 快照。
pub(crate) fn refresh_vmm_memory_runtime_projection(
    retained_snapshot: &VmmMemorySnapshot,
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    provider_configuration: &ProviderConfigurationFile,
    managed_runtime: &crate::managed_memory_mesh_runtime::ManagedMemoryMeshSnapshot,
) -> VmmMemorySnapshot {
    // The selected session runtime belongs to the retained workspace and must stay aligned with
    // the project and conversation identities rebuilt by `vmm_memory_snapshot`.
    // 当前会话运行态归属于保留工作区，必须与 `vmm_memory_snapshot` 重建的项目和会话身份保持一致。
    let selected_runtime_state = workspace
        .session_runtime_state
        .get(&workspace.selected_session);
    let mut refreshed = vmm_memory_snapshot(
        workspace,
        provider_configuration,
        selected_runtime_state,
        Some(managed_runtime),
    );
    // Management rows are an explicit runtime cache. Rebuilding lifecycle-derived state must
    // never discard rows that were loaded through the management transport.
    // 管理记录是显式运行时缓存；重建生命周期派生状态绝不能丢弃通过管理传输加载的记录。
    refreshed.management_catalog = retained_snapshot.management_catalog.clone();
    hydrate_vmm_default_identity(&mut refreshed);
    refreshed
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
        effective.diagnose_memory_flow(true) == vulcan_vmm::VmmConfigDiagnostic::Ready;
    let data_space_blocked = matches!(
        runtime.data_space_state,
        vulcan_service_api::VmmDataSpaceState::UnmarkedHistory
            | vulcan_service_api::VmmDataSpaceState::FingerprintChanged
            | vulcan_service_api::VmmDataSpaceState::MarkerCorrupt
            | vulcan_service_api::VmmDataSpaceState::MarkerVersionUnsupported
            | vulcan_service_api::VmmDataSpaceState::SchemaMigrationRequired
    );
    let setup = if deployment == VmmDeploymentMode::Disabled {
        VmmSetupState::NotConfigured
    } else if !required_configuration_ready {
        VmmSetupState::Incomplete
    } else if data_space_blocked {
        VmmSetupState::Blocked
    } else if matches!(
        lifecycle,
        VmmLifecycleState::Starting | VmmLifecycleState::Restarting
    ) {
        VmmSetupState::Checking
    } else if matches!(lifecycle, VmmLifecycleState::Failed) {
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
    // Both an authenticated managed-health failure and an explicit external connection error make the service unavailable.
    // 经过鉴权的托管健康失败与明确的外部连接错误都会使服务进入不可用状态。
    } else if runtime.managed_phase == "unhealthy"
        || (deployment == VmmDeploymentMode::External && runtime.managed_last_error.is_some())
    {
        VmmHealthState::Unavailable
    } else if deployment == VmmDeploymentMode::External {
        VmmHealthState::Unknown
    } else if matches!(
        lifecycle,
        VmmLifecycleState::Crashed | VmmLifecycleState::Failed
    ) {
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

/// Projects the internal data-space inspection into user-facing translation keys and actions.
/// 将内部数据空间检查投影为用户可见的翻译键与操作。
///
/// # Parameters
/// 参数
/// `state` is the exact managed data-space inspection state.
/// `state` 是准确的托管数据空间检查状态。
///
/// # Returns
/// 返回值
/// A stable product-facing data-space summary.
/// 返回稳定的产品级数据空间摘要。
fn vmm_data_space_summary(state: vulcan_service_api::VmmDataSpaceState) -> VmmDataSpaceSummary {
    use vulcan_service_api::VmmDataSpaceState;

    let (title_key, description_key, action_kind) = match state {
        VmmDataSpaceState::Unknown => (
            "memory.dataSpace.unknown.title",
            "memory.dataSpace.unknown.description",
            None,
        ),
        VmmDataSpaceState::Empty => (
            "memory.dataSpace.empty.title",
            "memory.dataSpace.empty.description",
            None,
        ),
        VmmDataSpaceState::Initialized | VmmDataSpaceState::Ready => (
            "memory.dataSpace.ready.title",
            "memory.dataSpace.ready.description",
            None,
        ),
        VmmDataSpaceState::UnmarkedHistory => (
            "memory.dataSpace.unmarkedHistory.title",
            "memory.dataSpace.unmarkedHistory.description",
            Some("backup_and_rebuild"),
        ),
        VmmDataSpaceState::FingerprintChanged => (
            "memory.dataSpace.fingerprintChanged.title",
            "memory.dataSpace.fingerprintChanged.description",
            Some("backup_and_rebuild"),
        ),
        VmmDataSpaceState::MarkerCorrupt => (
            "memory.dataSpace.markerCorrupt.title",
            "memory.dataSpace.markerCorrupt.description",
            Some("inspect_data"),
        ),
        VmmDataSpaceState::MarkerVersionUnsupported => (
            "memory.dataSpace.markerVersionUnsupported.title",
            "memory.dataSpace.markerVersionUnsupported.description",
            Some("migrate_data"),
        ),
        VmmDataSpaceState::SchemaMigrationRequired => (
            "memory.dataSpace.schemaMigrationRequired.title",
            "memory.dataSpace.schemaMigrationRequired.description",
            Some("migrate_data"),
        ),
        VmmDataSpaceState::RebuildRunning => (
            "memory.dataSpace.rebuildRunning.title",
            "memory.dataSpace.rebuildRunning.description",
            Some("view_progress"),
        ),
    };
    VmmDataSpaceSummary {
        state,
        title_key: title_key.to_string(),
        description_key: description_key.to_string(),
        action_kind: action_kind.map(str::to_string),
    }
}

/// Resolves the configured VMM deployment mode without inspecting error text.
/// 在不解析错误文本的前提下解析已配置的 VMM 部署方式。
///
/// # Parameters
/// 参数
/// `effective` is the resolved VMM configuration.
/// `effective` 是解析后的 VMM 配置。
/// `global_layer` is the persisted global VMM layer.
/// `global_layer` 是持久化的全局 VMM 配置层。
///
/// # Returns
/// 返回值
/// The configured deployment mode.
/// 返回已配置的部署方式。
pub(super) fn vmm_deployment_mode(global_layer: &vulcan_vmm::VmmConfigLayer) -> VmmDeploymentMode {
    if global_layer.enabled != Some(true) {
        return VmmDeploymentMode::Disabled;
    }
    // Built-in effective values keep runtime clients ready but do not prove that a user configured
    // a deployment. Only explicit global transport or enable state may classify external VMM.
    // 内置最终值用于让运行时客户端保持就绪，但不能证明用户已经配置部署。只有明确的
    // 全局传输或启用状态才能归类为外部 VMM。
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
        None => VmmDeploymentMode::Native,
    }
}

/// Maps the managed runtime phase to a stable lifecycle enum.
/// 将托管运行阶段映射为稳定生命周期枚举。
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
    match runtime.managed_phase.as_str() {
        "ready" | "unhealthy" => VmmLifecycleState::Running,
        "starting" => VmmLifecycleState::Starting,
        "stopping" => VmmLifecycleState::Stopping,
        "restarting" => VmmLifecycleState::Restarting,
        "crashed" => VmmLifecycleState::Crashed,
        "waiting_retry" => VmmLifecycleState::WaitingRetry,
        "failed" => VmmLifecycleState::Failed,
        "external" if runtime.process_ready => VmmLifecycleState::Running,
        "external" | "stopped" => VmmLifecycleState::Stopped,
        _ => VmmLifecycleState::Stopped,
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
    if effective.grpc_target.trim().is_empty() {
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
    if effective.user_id.filter(|value| *value > 0).is_none() {
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
    if effective.project_id.filter(|value| *value > 0).is_none() {
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
    let managed = global_layer
        .install
        .as_ref()
        .map(|install| install.managed_install_enabled)
        .unwrap_or(effective.install.managed_install_enabled);
    if managed && matches!(vmm_lifecycle_state(runtime), VmmLifecycleState::Failed) {
        prerequisites.push(vmm_prerequisite(
            "managed_runtime",
            "failed",
            "error",
            "memory.vmm.prerequisite.managedRuntime.title",
            "memory.vmm.prerequisite.managedRuntime.description",
            Some("retry_runtime"),
            Some("overview"),
            runtime.managed_last_error.clone(),
        ));
    }
    if !managed && let Some(error) = runtime.managed_last_error.clone() {
        prerequisites.push(vmm_prerequisite(
            "external_transport",
            "failed",
            "error",
            "memory.vmm.prerequisite.externalTransport.title",
            "memory.vmm.prerequisite.externalTransport.description",
            Some("configure_deployment"),
            Some("advanced"),
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
        backend_mode: layer
            .backend_mode
            .unwrap_or(effective.backend_mode)
            .as_str()
            .to_string(),
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
        grpc_api_key: layer
            .grpc_api_key
            .clone()
            .or_else(|| effective.grpc_api_key.clone())
            .unwrap_or_default(),
        management_base_url: layer.management_base_url.clone().unwrap_or_default(),
        management_bearer_token: layer
            .management_bearer_token
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|_| String::from(WEBUI_REDACTED_SECRET_PLACEHOLDER))
            .unwrap_or_default(),
        management_timeout_ms: layer.management_timeout_ms.unwrap_or(30_000).to_string(),
        managed_install_enabled: layer
            .install
            .as_ref()
            .map(|install| install.managed_install_enabled)
            .unwrap_or(effective.install.managed_install_enabled),
        managed_binary_path: layer
            .install
            .as_ref()
            .and_then(|install| install.managed_binary_path.clone())
            .or_else(|| effective.install.managed_binary_path.clone())
            .unwrap_or_default(),
        auto_start_policy: layer
            .install
            .as_ref()
            .and_then(|install| install.auto_start_policy.clone())
            .or_else(|| effective.install.auto_start_policy.clone())
            .unwrap_or_else(|| String::from("on_start")),
        managed_runtime_settings: vmm_managed_runtime_settings_draft(
            layer
                .install
                .as_ref()
                .map(|install| &install.runtime_settings)
                .unwrap_or(&effective.install.runtime_settings),
        ),
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

/// Converts persisted managed VMM behavior into an editable service draft.
/// 将持久化的托管 VMM 行为转换为可编辑服务草稿。
fn vmm_managed_runtime_settings_draft(
    settings: &vulcan_vmm::VmmManagedRuntimeSettings,
) -> vulcan_service_api::VmmManagedRuntimeSettingsDraft {
    vulcan_service_api::VmmManagedRuntimeSettingsDraft {
        pii_default_language: settings.pii_default_language.clone(),
        noise_enabled: settings.noise_enabled,
        noise_default_language: settings.noise_default_language.clone(),
        noise_semantic_enabled: settings.noise_semantic_enabled,
        noise_semantic_threshold: settings.noise_semantic_threshold.to_string(),
        prompt_language: settings.prompt_language.clone(),
        precheck_intent_timeout: settings.precheck_intent_timeout.clone(),
        precheck_top_k: settings.precheck_top_k.to_string(),
        precheck_search_scope: settings.precheck_search_scope.clone(),
        precheck_similarity_threshold: settings.precheck_similarity_threshold.to_string(),
        postaction_input_mode: settings.postaction_input_mode.clone(),
        postaction_turn_threshold: settings.postaction_turn_threshold.to_string(),
        postaction_token_threshold: settings.postaction_token_threshold.to_string(),
        postaction_idle_timeout: settings.postaction_idle_timeout.clone(),
        postaction_history_turns: settings.postaction_history_turns.to_string(),
        postaction_max_input_tokens: settings.postaction_max_input_tokens.to_string(),
        postaction_analysis_timeout: settings.postaction_analysis_timeout.clone(),
        postaction_failure_pass_threshold: settings.postaction_failure_pass_threshold.to_string(),
        postaction_max_queue_workers: settings.postaction_max_queue_workers.to_string(),
        inference_max_connections_per_host: settings.inference_max_connections_per_host.to_string(),
        pipeline_max_search_keywords: settings.pipeline_max_search_keywords.to_string(),
        pipeline_min_similarity_score: settings.pipeline_min_similarity_score.to_string(),
        pipeline_replace_min_similarity_score: settings
            .pipeline_replace_min_similarity_score
            .to_string(),
        pipeline_hard_dedupe_threshold: settings.pipeline_hard_dedupe_threshold.to_string(),
        pipeline_hard_dedupe_pool_top_k: settings.pipeline_hard_dedupe_pool_top_k.to_string(),
        pipeline_hybrid_enabled: settings.pipeline_hybrid_enabled,
        pipeline_lexical_top_k: settings.pipeline_lexical_top_k.to_string(),
        pipeline_rrf_k: settings.pipeline_rrf_k.to_string(),
        pipeline_mmr_enabled: settings.pipeline_mmr_enabled,
        pipeline_mmr_lambda: settings.pipeline_mmr_lambda.to_string(),
        pipeline_weibull_enabled: settings.pipeline_weibull_enabled,
        pipeline_weibull_shape: settings.pipeline_weibull_shape.to_string(),
        pipeline_weibull_scale_hours: settings.pipeline_weibull_scale_hours.to_string(),
        pipeline_weibull_min_multiplier: settings.pipeline_weibull_min_multiplier.to_string(),
        pipeline_weibull_reinforce_weight: settings.pipeline_weibull_reinforce_weight.to_string(),
        pipeline_weibull_cross_session_boost: settings
            .pipeline_weibull_cross_session_boost
            .to_string(),
        retention_enabled: settings.retention_enabled,
        retention_scan_interval: settings.retention_scan_interval.clone(),
        retention_turn_keep_extra_turns: settings.retention_turn_keep_extra_turns.to_string(),
        retention_session_idle_after: settings.retention_session_idle_after.clone(),
        retention_trash_duration: settings.retention_trash_duration.clone(),
        retention_priority_floor: settings.retention_priority_floor.clone(),
        retention_memory_level_floor: settings.retention_memory_level_floor.clone(),
        retention_skip_protected_shared: settings.retention_skip_protected_shared,
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
    managed_runtime: Option<&crate::managed_memory_mesh_runtime::ManagedMemoryMeshSnapshot>,
) -> VmmRuntimeSummary {
    let data_space_state = managed_runtime
        .map(|runtime| runtime.data_space_state)
        .unwrap_or_default();
    let managed_phase = managed_runtime
        .map(|runtime| runtime.phase.clone())
        .unwrap_or_else(|| String::from("external"));
    let managed_grpc_target = managed_runtime.and_then(|runtime| runtime.grpc_target.clone());
    let managed_controller_endpoint = managed_runtime
        .map(|runtime| runtime.controller_endpoint.clone())
        .unwrap_or_default();
    let managed_controller_space = managed_runtime
        .map(|runtime| runtime.controller_space_id.clone())
        .unwrap_or_default();
    let managed_data_root = managed_runtime
        .map(|runtime| runtime.data_root.to_string_lossy().into_owned())
        .unwrap_or_default();
    let managed_log_path = managed_runtime
        .map(|runtime| runtime.log_path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let managed_last_error = managed_runtime.and_then(|runtime| runtime.last_error.clone());
    let process_ready = managed_runtime
        .map(|runtime| runtime.phase == "ready")
        .unwrap_or(false);
    let capabilities = managed_runtime.map(|runtime| &runtime.capabilities);
    // Preflight success describes configuration validity; live readiness additionally requires a running child.
    // 预检成功只代表配置有效；实时可用还必须要求子进程正在运行。
    let scope_ready = process_ready
        && managed_runtime
            .map(|runtime| runtime.scope_ready)
            .unwrap_or(false);
    // service_ready is the sole runtime capability boundary: a live child without a configured
    // scope is still a stopped service for recall, writeback, tools, profiles, and reranking.
    // service_ready 是唯一运行能力边界：只有子进程但尚未配置作用域时，召回、回写、工具、画像与
    // 重排都必须继续按服务未启动处理。
    let service_ready = process_ready && scope_ready;
    let precheck_ready = service_ready
        && capabilities
            .map(|capabilities| capabilities.precheck_ready)
            .unwrap_or(false);
    let postaction_ready = service_ready
        && capabilities
            .map(|capabilities| capabilities.postaction_ready)
            .unwrap_or(false);
    let profile_instruction_ready = service_ready
        && capabilities
            .map(|capabilities| capabilities.profile_instruction_ready)
            .unwrap_or(false);
    let rerank_ready = service_ready
        && capabilities
            .map(|capabilities| capabilities.rerank_ready)
            .unwrap_or(false);
    let Some(runtime_state) = runtime_state else {
        return VmmRuntimeSummary {
            data_space_state,
            process_ready,
            managed_phase,
            managed_grpc_target,
            managed_controller_endpoint,
            managed_controller_space,
            managed_data_root,
            managed_log_path,
            managed_last_error,
            scope_ready,
            scope_error: managed_runtime.and_then(|runtime| runtime.scope_error.clone()),
            precheck_ready,
            precheck_error: capabilities
                .and_then(|capabilities| capabilities.precheck_error.clone()),
            postaction_ready,
            postaction_error: capabilities
                .and_then(|capabilities| capabilities.postaction_error.clone()),
            profile_instruction_ready,
            profile_instruction_error: capabilities
                .and_then(|capabilities| capabilities.profile_instruction_error.clone()),
            rerank_ready,
            rerank_error: capabilities.and_then(|capabilities| capabilities.rerank_error.clone()),
            ..VmmRuntimeSummary::default()
        };
    };
    VmmRuntimeSummary {
        data_space_state,
        process_ready,
        managed_phase,
        managed_grpc_target,
        managed_controller_endpoint,
        managed_controller_space,
        managed_data_root,
        managed_log_path,
        managed_last_error,
        scope_ready,
        scope_error: managed_runtime.and_then(|runtime| runtime.scope_error.clone()),
        precheck_ready,
        precheck_error: capabilities.and_then(|capabilities| capabilities.precheck_error.clone()),
        postaction_ready,
        postaction_error: capabilities
            .and_then(|capabilities| capabilities.postaction_error.clone()),
        profile_instruction_ready,
        profile_instruction_error: capabilities
            .and_then(|capabilities| capabilities.profile_instruction_error.clone()),
        rerank_ready,
        rerank_error: capabilities.and_then(|capabilities| capabilities.rerank_error.clone()),
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
            "content",
            "CONTENT",
            "记忆数据",
            "按会话和回合查看、归档、回收、恢复并维护长期记忆。",
        ),
        (
            "identity_projects",
            "IDENTITY AND PROJECTS",
            "用户与项目",
            "管理当前身份、项目绑定、迁移和删除。",
        ),
        (
            "profile",
            "PROFILE",
            "画像管理",
            "查看画像摘要并应用人工画像指令。",
        ),
        (
            "advanced",
            "ADVANCED",
            "高级设置",
            "查看诊断信息并执行备份、重建和高级维护。",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managed_memory_mesh_runtime::{
        ManagedMemoryMeshSnapshot, ManagedVmmCapabilityPreflight,
    };

    /// Verifies the page snapshot does not expose preflight success after the managed child stops.
    /// 验证托管子进程停止后，页面快照不会把预检成功误报为实时可用。
    #[test]
    fn stopped_managed_runtime_summary_disables_live_capabilities() {
        let root = std::env::temp_dir().join("vulcan-vmm-stopped-page-snapshot");
        let managed = ManagedMemoryMeshSnapshot {
            data_space_state: vulcan_service_api::VmmDataSpaceState::Ready,
            phase: String::from("stopped"),
            grpc_target: None,
            controller_endpoint: String::from("http://127.0.0.1:19801"),
            controller_space_id: String::from("vulcan-code"),
            data_root: root.join("data"),
            log_path: root.join("managed.log"),
            last_error: None,
            scope_ready: true,
            scope_error: None,
            capabilities: ManagedVmmCapabilityPreflight {
                embedding_dimension: 1024,
                embedding_fingerprint: None,
                precheck_ready: true,
                precheck_error: None,
                postaction_ready: true,
                postaction_error: None,
                profile_instruction_ready: true,
                profile_instruction_error: None,
                rerank_ready: true,
                rerank_error: None,
            },
        };

        let runtime = vmm_runtime_summary(None, Some(&managed));

        assert!(!runtime.process_ready);
        assert!(!runtime.scope_ready);
        assert!(!runtime.precheck_ready);
        assert!(!runtime.postaction_ready);
        assert!(!runtime.profile_instruction_ready);
        assert!(!runtime.rerank_ready);
    }

    /// Verifies an owned child with a failed heartbeat remains running but unavailable.
    /// 验证心跳失败但仍由宿主持有的子进程保持“运行中”，同时健康状态为“不可用”。
    #[test]
    fn unhealthy_managed_runtime_keeps_process_and_health_dimensions_separate() {
        // global_layer describes one fully configured managed VMM deployment.
        // global_layer 描述一份配置完整的托管 VMM 部署。
        let global_layer = vulcan_vmm::VmmConfigLayer {
            enabled: Some(true),
            grpc_target: Some(String::from("127.0.0.1:17901")),
            user_id: Some(1),
            project_id: Some(1),
            install: Some(vulcan_vmm::VmmInstallReservation {
                managed_install_enabled: true,
                ..vulcan_vmm::VmmInstallReservation::default()
            }),
            ..vulcan_vmm::VmmConfigLayer::default()
        };
        // effective is the exact resolved configuration used by product-state projection.
        // effective 是产品状态投影使用的准确解析配置。
        let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&global_layer, None, None);
        // runtime retains the live child generation while exposing its failed heartbeat.
        // runtime 在保留存活子进程代次的同时暴露心跳失败。
        let runtime = VmmRuntimeSummary {
            managed_phase: String::from("unhealthy"),
            managed_last_error: Some(String::from("heartbeat timeout")),
            ..VmmRuntimeSummary::default()
        };

        // experience keeps lifecycle and transport health as independent dimensions.
        // experience 将生命周期与传输健康保持为相互独立的维度。
        let experience = vmm_experience_state(&effective, &global_layer, &runtime);

        assert_eq!(experience.lifecycle, VmmLifecycleState::Running);
        assert_eq!(experience.health, VmmHealthState::Unavailable);
    }
}
