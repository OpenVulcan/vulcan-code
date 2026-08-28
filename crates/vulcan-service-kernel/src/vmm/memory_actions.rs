//! VMM memory actions helpers.
//! VMM memory actions 辅助逻辑。

use super::*;
use crate::{
    commit_and_emit_project_settings_changed_delta,
    emit_context_injection_changed_delta_for_session, emit_workbench_event,
};
use vulcan_service_api::{
    ProjectSettingsDomain, ServiceEventEmitter, VmmMemoryActionResponse, VmmProjectDisplaySummary,
    VmmUserDisplaySummary,
};

/// Runs one VMM memory management action and returns its structured result.
/// 执行单个 VMM 记忆管理动作并返回结构化结果。
///
/// # Parameters
/// 参数
/// `app` broadcasts state changes to every connected WebUI client.
/// `app` 将状态变更广播给每个已连接 WebUI 客户端。
/// `state` is the shared service-kernel state used to read configuration and session state.
/// `state` 是用于读取配置与会话状态的共享 service-kernel 状态。
/// `request` is the WebUI VMM action request to execute.
/// `request` 是需要执行的 WebUI VMM 动作请求。
///
/// # Returns
/// 返回值
/// A structured WebUI VMM action result or a user-visible error message.
/// 返回结构化 WebUI VMM 动作结果，或用户可见的错误消息。
pub(crate) async fn service_run_vmm_memory_action(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    normalize_vmm_memory_action_command_id(&request.command_id)?;
    if request.action.trim() == "suppress_context_injection_item" {
        return service_suppress_context_injection_item(app, state, request).await;
    }
    if matches!(
        request.action.trim(),
        "managed_runtime_start"
            | "managed_runtime_stop"
            | "managed_runtime_restart"
            | "managed_runtime_open_log"
    ) {
        return service_control_managed_vmm_runtime(app, state, request).await;
    }
    if matches!(
        request.action.trim(),
        "managed_maintenance_start" | "managed_maintenance_status" | "managed_maintenance_cancel"
    ) {
        return service_control_managed_vmm_maintenance(app, state, request).await;
    }
    if request.action.trim().starts_with("management_") {
        return super::management_http::service_run_vmm_management_http_action(state, request)
            .await;
    }
    if request.action.trim() == "apply_profile_instruction"
        && let Some(snapshot) = crate::managed_memory_mesh_runtime::snapshot_from_state(&state)
        && !snapshot.capabilities.profile_instruction_ready
    {
        return Err(snapshot
            .capabilities
            .profile_instruction_error
            .unwrap_or_else(|| String::from("托管 VMM 手工画像指令能力不可用。")));
    }
    let (layer, management_user_ref, management_project_ref) =
        vmm_action_context(&state, &request)?;
    let action = parse_vmm_memory_action(&request)?;
    let request_preview = vmm_action_request_preview(&action)?;
    let is_health_probe = matches!(
        &action,
        vulcan_service_runtime::vmm_management::VmmWorkbenchAction::Healthz
    );
    let grpc_target = layer.grpc_target.clone().unwrap_or_default();
    let outcome = match vulcan_service_runtime::vmm_management::run_vmm_workbench_action(
        action,
        layer,
        management_user_ref,
        management_project_ref,
    )
    .await
    {
        Ok(outcome) => {
            if is_health_probe {
                record_external_vmm_health_success(&grpc_target);
            }
            outcome
        }
        Err(error) => {
            if is_health_probe {
                record_external_vmm_health_failure(&grpc_target, &error);
                refresh_vmm_health_workbench(&app, &state).await;
            }
            return Err(error);
        }
    };
    let management_catalog = vmm_management_catalog_from_outcome(&outcome);
    let result = vmm_memory_action_result(&request.action, request_preview, outcome)?;
    if let Some(management_catalog) = management_catalog {
        // workbench commits the refreshed catalog as an in-memory product cache.
        // workbench 将刷新后的目录提交为内存中的产品缓存。
        let workbench = {
            let mut guard = state
                .lock()
                .map_err(|error| format!("锁定 VMM 管理目录缓存失败：{error}"))?;
            guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
            guard.workbench.revision = guard.workbench_generation;
            guard.workbench.vmm_memory.management_catalog = management_catalog;
            hydrate_vmm_default_identity(&mut guard.workbench.vmm_memory);
            guard.workbench.clone()
        };
        emit_workbench_event(&app, &workbench);
    } else if is_health_probe {
        refresh_vmm_health_workbench(&app, &state).await;
    }
    Ok(result)
}

/// Rebuilds and emits the workbench after one explicit VMM health probe.
/// 在一次显式 VMM 健康探测后重建并发送工作台。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench to connected WebUI clients.
/// `app` 将刷新后的工作台发送给已连接的 WebUI 客户端。
/// `state` owns the authoritative service and workbench state.
/// `state` 持有权威服务状态与工作台状态。
///
/// # Returns
/// 返回值
/// This function returns no value; refresh failures are logged without hiding the probe result.
/// 此函数不返回值；刷新失败只记录日志，不会遮蔽探测结果。
async fn refresh_vmm_health_workbench(app: &ServiceEventEmitter, state: &SharedServiceBridgeState) {
    match refresh_workbench_snapshot(state.clone()).await {
        Ok(workbench) => emit_workbench_event(app, &workbench),
        Err(error) => {
            eprintln!("[webui-service] failed to refresh VMM health workbench: {error}");
        }
    }
}

/// Executes one host-owned backup and official vector-rebuild command.
/// 执行一项宿主持有的备份与正式向量重建命令。
///
/// # Parameters
/// 参数
/// `app` broadcasts refreshed product state, `state` owns runtime dependencies, and `request`
/// selects start, status, or cancellation.
/// `app` 广播刷新的产品状态，`state` 持有运行依赖，`request` 选择启动、查询或取消。
///
/// # Returns
/// 返回值
/// The latest typed maintenance status after the requested transition.
/// 返回请求状态转换后的最近强类型维护状态。
async fn service_control_managed_vmm_maintenance(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let action = request.action.trim().to_string();
    let maintenance = crate::managed_memory_mesh_maintenance::runtime();
    let summary = match action.as_str() {
        "managed_maintenance_status" => maintenance.snapshot(),
        "managed_maintenance_cancel" => maintenance.cancel()?,
        "managed_maintenance_start" => {
            let lifecycle = crate::managed_memory_mesh_runtime::snapshot_from_state(&state)
                .ok_or_else(|| String::from("请先完成托管记忆配置检查，再处理历史数据。"))?;
            if !matches!(
                lifecycle.data_space_state,
                vulcan_service_api::VmmDataSpaceState::UnmarkedHistory
                    | vulcan_service_api::VmmDataSpaceState::FingerprintChanged
                    | vulcan_service_api::VmmDataSpaceState::MarkerCorrupt
                    | vulcan_service_api::VmmDataSpaceState::MarkerVersionUnsupported
                    | vulcan_service_api::VmmDataSpaceState::SchemaMigrationRequired
            ) {
                return Err(String::from("当前记忆数据状态不需要执行备份与向量重建。"));
            }
            let provider_configuration = provider_configuration_from_state(&state)?;
            let ai_configuration =
                vulcan_provider_config::config::load_public_ai_connections_file()
                    .map_err(|error| format!("重新加载 VMM 内部推理配置失败：{error}"))?
                    .configuration;
            let vmm_config = provider_configuration.vmm.clone();
            let install = vmm_config
                .install
                .as_ref()
                .ok_or_else(|| String::from("尚未启用 Vulcan Code 托管 VMM。"))?;
            if !install.managed_install_enabled {
                return Err(String::from("尚未启用 Vulcan Code 托管 VMM。"));
            }
            let binary_path = install
                .managed_binary_path
                .as_deref()
                .map(std::path::PathBuf::from)
                .or_else(crate::service_bootstrap::default_managed_vmm_binary_path)
                .ok_or_else(|| String::from("无法解析托管 VMM 可执行文件路径。"))?;
            let (catalogs, optional_inference_routes, controller_endpoint) = {
                let guard = state
                    .lock()
                    .map_err(|error| format!("读取 VMM 维护依赖失败：{error}"))?;
                let runtime = guard
                    .agent_runtime
                    .as_ref()
                    .ok_or_else(|| String::from("公共 AI Runtime 尚未发布。"))?;
                // Rebuild must use the same verified inference prerequisites as first initialization.
                // 重建必须使用与首次初始化相同的已验证推理前置条件。
                crate::configured_inference::validate_vmm_required_inference_routes(
                    &runtime.public_ai_runtime,
                    &ai_configuration,
                )?;
                let optional_inference_routes =
                    crate::configured_inference::inspect_vmm_optional_inference_routes(
                        &runtime.public_ai_runtime,
                        &ai_configuration,
                    );
                let catalogs = runtime.public_ai_runtime.catalog_snapshot()?;
                let controller_endpoint = guard
                    .controller_runtime
                    .as_ref()
                    .map(|controller| controller.endpoint.clone())
                    .ok_or_else(|| String::from("共享 Controller 尚未发布。"))?;
                (catalogs, optional_inference_routes, controller_endpoint)
            };
            let capabilities =
                crate::managed_memory_mesh_runtime::configured_vmm_capability_preflight(
                    &ai_configuration,
                    &catalogs,
                    &optional_inference_routes,
                )?;
            let preparation = crate::managed_memory_mesh_runtime::prepare_managed_vmm_maintenance(
                &binary_path,
                controller_endpoint.as_str(),
                &vmm_config,
                &capabilities,
            )?;
            // preparation completes every fallible preflight before the existing runtime owner is disturbed.
            // preparation 在干扰现有运行时所有者之前完成全部可能失败的预检。
            let existing = state
                .lock()
                .map_err(|error| format!("锁定 VMM 运行状态失败：{error}"))?
                .managed_memory_mesh_runtime
                .take();
            if let Some(existing) = existing
                && let Err(error) = existing
                    .lock()
                    .map_err(|lock_error| format!("锁定 VMM 子进程失败：{lock_error}"))?
                    .stop()
            {
                crate::managed_memory_mesh_runtime::cleanup_managed_vmm_maintenance_manifest(
                    &preparation.manifest_path,
                );
                return Err(error);
            }
            crate::vmm_hooks::clear_managed_vmm_transport_overlay();
            if let Err(error) = refresh_service_vmm_hook_snapshot(&state) {
                crate::managed_memory_mesh_runtime::cleanup_managed_vmm_maintenance_manifest(
                    &preparation.manifest_path,
                );
                return Err(error);
            }
            match maintenance.start(preparation.clone()) {
                Ok(summary) => summary,
                Err(error) => {
                    crate::managed_memory_mesh_runtime::cleanup_managed_vmm_maintenance_manifest(
                        &preparation.manifest_path,
                    );
                    return Err(error);
                }
            }
        }
        _ => return Err(format!("不支持的 VMM 维护动作：{action}")),
    };
    if summary.phase == vulcan_service_api::VmmMaintenancePhase::Completed {
        crate::managed_memory_mesh_runtime::publish_maintenance_data_space_ready();
    }
    let workbench = refresh_workbench_snapshot(state).await?;
    emit_workbench_event(&app, &workbench);
    let response = serde_json::to_value(&summary)
        .map_err(|error| format!("序列化 VMM 维护状态失败：{error}"))?;
    Ok(VmmMemoryActionResult {
        action,
        summary: summary.message.clone(),
        kind: String::from("maintenance"),
        request: serde_json::to_value(&request)
            .map_err(|error| format!("序列化 VMM 维护请求失败：{error}"))?,
        response,
        data: VmmMemoryActionResponse::Maintenance(summary),
    })
}

/// Projects a successful users/projects outcome into the in-memory user-facing catalog.
/// 将成功的用户/项目结果投影为面向用户的内存目录。
///
/// # Parameters
/// 参数
/// `outcome` is the exact typed result returned by the VMM management runtime.
/// `outcome` 是 VMM 管理运行时返回的准确类型化结果。
///
/// # Returns
/// 返回值
/// A catalog only for the users/projects operation; unrelated outcomes return `None`.
/// 仅用户/项目操作返回目录；无关结果返回 `None`。
fn vmm_management_catalog_from_outcome(
    outcome: &vulcan_service_runtime::vmm_management::VmmManagementOutcome,
) -> Option<VmmManagementCatalogSummary> {
    use vulcan_service_runtime::vmm_management::VmmManagementOutcome;

    let VmmManagementOutcome::UsersAndProjects { users, projects } = outcome else {
        return None;
    };
    let user_rows = users
        .users
        .iter()
        .map(|user| VmmUserDisplaySummary {
            stable_key: user.user_id.to_string(),
            display_name: user.user_name.clone(),
            linked_project_count: None,
            memory_status: String::from("available"),
            protected: user.user_name == "default",
        })
        .collect();
    let project_rows = projects
        .projects
        .iter()
        .map(vmm_project_display_summary)
        .collect();
    Some(VmmManagementCatalogSummary {
        loading: false,
        users: user_rows,
        projects: project_rows,
        last_refreshed_at: Some(crate::workbench_refresh::current_unix_millis_text()),
        error: None,
    })
}

/// Builds one product-facing project row with the canonical VMM action path.
/// 构建一条面向产品的项目记录，并提供规范的 VMM 动作路径。
pub(super) fn vmm_project_display_summary(
    project: &vulcan_vmm::ProjectEntry,
) -> VmmProjectDisplaySummary {
    // VMM's ProjectRecord.Path contract is Team/Space/Project. The protocol display_path adds
    // an internal numeric prefix for diagnostics, so actions must use the structured names.
    // VMM 的 ProjectRecord.Path 契约是 Team/Space/Project。协议 display_path 会为诊断附加
    // 内部数字前缀，因此动作必须使用结构化名称。
    let display_path = format!(
        "{}/{}/{}",
        project.team_name, project.space_name, project.project_name
    );
    VmmProjectDisplaySummary {
        stable_key: project.project_id.to_string(),
        display_name: project.project_name.clone(),
        action_path: display_path.clone(),
        display_path,
        task_project_key: None,
        has_override: None,
        state: String::from("available"),
        protected: project.team_name == "default"
            && project.space_name == "default"
            && project.project_name == "default",
    }
}

/// Executes one explicit host-owned managed VMM lifecycle command.
/// 执行一项显式宿主持有的托管 VMM 生命周期命令。
///
/// # Parameters
/// 参数
/// `app` broadcasts the refreshed workbench, `state` owns the child, and `request` selects the command.
/// `app` 广播刷新后的工作台，`state` 持有子进程，`request` 选择具体命令。
///
/// # Returns
/// 返回值
/// A structured lifecycle result after the command and state projection complete.
/// 命令与状态投影完成后返回结构化生命周期结果。
async fn service_control_managed_vmm_runtime(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let action = request.action.trim().to_string();
    if action == "managed_runtime_open_log" {
        let snapshot = crate::managed_memory_mesh_runtime::snapshot_from_state(&state)
            .ok_or_else(|| String::from("当前没有可查看的托管 VMM 运行时状态。"))?;
        let log_path = snapshot.log_path.to_string_lossy().into_owned();
        if log_path.trim().is_empty() {
            return Err(String::from("当前托管 VMM 状态未提供日志路径。"));
        }
        crate::service_bridge_io::shell_host_from_state(&state)?.open_path(log_path.as_str())?;
        let request_preview = serde_json::to_value(&request)
            .map_err(|error| format!("序列化 VMM 日志请求失败：{error}"))?;
        let response = serde_json::json!({ "logPath": log_path });
        return Ok(VmmMemoryActionResult {
            action,
            summary: String::from("已打开托管 VMM 运行日志。"),
            kind: String::from("managed_runtime"),
            request: request_preview,
            response: response.clone(),
            data: VmmMemoryActionResponse::ManagedRuntime(response),
        });
    }
    if action == "managed_runtime_stop" {
        let existing = state
            .lock()
            .map_err(|error| format!("锁定 VMM 运行状态失败：{error}"))?
            .managed_memory_mesh_runtime
            .take();
        if let Some(existing) = existing {
            existing
                .lock()
                .map_err(|error| format!("锁定 VMM 子进程失败：{error}"))?
                .stop()?;
        } else {
            return Err(String::from("当前没有由 Vulcan Code 持有的 VMM 子进程。"));
        }
        crate::vmm_hooks::clear_managed_vmm_transport_overlay();
        refresh_service_vmm_hook_snapshot(&state)?;
    }

    let mut logs = Vec::new();
    if action == "managed_runtime_start" || action == "managed_runtime_restart" {
        if action == "managed_runtime_start"
            && crate::managed_memory_mesh_runtime::snapshot_from_state(&state)
                .is_some_and(|snapshot| snapshot.phase == "ready")
        {
            return Err(String::from("托管 VMM 已经处于就绪状态。"));
        }
        let provider_configuration = provider_configuration_from_state(&state)?;
        let ai_configuration = vulcan_provider_config::config::load_public_ai_connections_file()
            .map_err(|error| format!("重新加载 VMM 内部推理配置失败：{error}"))?
            .configuration;
        let mut vmm_config = provider_configuration.vmm.clone();
        let install = vmm_config
            .install
            .as_mut()
            .ok_or_else(|| String::from("尚未启用 Vulcan Code 托管 VMM。"))?;
        if !install.managed_install_enabled {
            return Err(String::from("尚未启用 Vulcan Code 托管 VMM。"));
        }
        install.auto_start_policy = Some(String::from("on_start"));
        crate::service_bootstrap::reconcile_managed_memory_mesh_runtime(
            &state,
            &ai_configuration,
            &vmm_config,
            &mut logs,
        )
        .await;
        let snapshot = crate::managed_memory_mesh_runtime::snapshot_from_state(&state)
            .ok_or_else(|| String::from("托管 VMM 未发布运行状态。"))?;
        if snapshot.phase != "ready" {
            // failed_workbench persists the failed lifecycle projection before the command returns.
            // failed_workbench 在命令返回前持久投影失败的生命周期状态。
            let failed_workbench = refresh_workbench_snapshot(state.clone()).await?;
            emit_workbench_event(&app, &failed_workbench);
            return Err(snapshot
                .last_error
                .unwrap_or_else(|| format!("托管 VMM 启动后状态为 {}。", snapshot.phase)));
        }
    }

    let workbench = refresh_workbench_snapshot(state).await?;
    emit_workbench_event(&app, &workbench);
    let snapshot = &workbench.vmm_memory.runtime;
    let request_preview = serde_json::to_value(&request)
        .map_err(|error| format!("序列化 VMM 控制请求失败：{error}"))?;
    let response = serde_json::json!({
        "phase": snapshot.managed_phase,
        "grpcTarget": snapshot.managed_grpc_target,
        "controllerEndpoint": snapshot.managed_controller_endpoint,
        "controllerSpace": snapshot.managed_controller_space,
        "dataRoot": snapshot.managed_data_root,
        "logs": logs,
    });
    Ok(VmmMemoryActionResult {
        action: action.clone(),
        summary: match action.as_str() {
            "managed_runtime_stop" => String::from("托管 VMM 已停止。"),
            "managed_runtime_restart" => String::from("托管 VMM 已重启并通过鉴权健康检查。"),
            _ => String::from("托管 VMM 已启动并通过鉴权健康检查。"),
        },
        kind: String::from("managed_runtime"),
        request: request_preview,
        response: response.clone(),
        data: VmmMemoryActionResponse::ManagedRuntime(response),
    })
}

/// Suppresses one context-injection item for the exact requested session and broadcasts its projection.
/// 为准确请求的会话抑制单条上下文注入内容并广播其投影。
///
/// # Parameters
/// 参数
/// `app` broadcasts the changed exact-session projection to every connected client.
/// `app` 将变更后的准确会话投影广播给每个已连接客户端。
/// `state` owns the retained workspace mutation and persistence runtime.
/// `state` 持有保留的工作区变更与持久化运行时。
/// `request` carries the exact conversation identity and memory item identity.
/// `request` 携带准确的会话身份与记忆条目身份。
///
/// # Returns
/// 返回值
/// A structured suppression result after persistence and broadcast complete.
/// 持久化与广播完成后返回结构化抑制结果。
pub(crate) async fn service_suppress_context_injection_item(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let dedupe_key = vmm_payload_text(&request.payload, "dedupeKey");
    let memory_id = vmm_payload_u64(&request.payload, "memoryId");
    if dedupe_key.is_none() && memory_id.is_none() {
        return Err(String::from("缺少需要隐藏的记忆条目标识。"));
    }
    // suppression_key is the exact semantic lease identity selected by the request.
    // suppression_key 是请求选择的精确语义租约身份。
    let suppression_key = dedupe_key
        .clone()
        .or_else(|| memory_id.map(|memory_id| format!("memory:{memory_id}")))
        .ok_or_else(|| String::from("缺少需要隐藏的记忆条目标识。"))?;
    // request_command_id is validated before any retained or durable state can change.
    // request_command_id 在任何保留状态或持久状态发生变化前完成校验。
    let command_id = vmm_suppression_command_id(&request.command_id)?;
    // mutation_timestamp is shared by the ordinary projection and the durable controller command.
    // mutation_timestamp 由普通投影与持久控制器命令共同使用。
    let mutation_timestamp = current_unix_millis_text();

    // storage_locator is resolved from the exact registered V1 session before ordinary-state mutation.
    // storage_locator 在普通状态变更前从精确注册的 V1 会话解析。
    let (resolved_session_id, storage_locator) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("工作区尚未载入，无法隐藏动态记忆。"))?;
        let session_id = vmm_suppression_session_id(workspace, request.conversation_id.as_deref())?;
        let handle = guard
            .session_runtime_registry
            .get(session_id)
            .map_err(|error| format!("V1 会话 `{session_id}` 尚未完成精确 hydration：{error}"))?;
        let storage_locator = handle.storage_locator().map_err(|error| {
            format!("Failed to resolve V1 VMM locator for `{session_id}`: {error}")
        })?;
        (session_id, storage_locator)
    };

    let (session_id, context_changed, vmm_changed) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        // vmm_controller_registry is process-owned and never exposes mutable VMM state.
        // vmm_controller_registry 由进程持有，绝不暴露可变 VMM 状态。
        let vmm_controller_registry = guard.vmm_controller_registry.clone();
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("工作区尚未载入，无法隐藏动态记忆。"))?;
        // session_id is resolved from the initiating request instead of mutable UI selection state.
        // session_id 从动作发起请求解析，而不是依赖可变的 UI 选中状态。
        let session_id = resolved_session_id;
        // runtime_state is detached so a controller failure cannot partially mutate retained workspace state.
        // runtime_state 保持分离，使控制器失败无法部分修改保留的工作区状态。
        let mut runtime_state = workspace
            .session_runtime_state
            .get(&session_id)
            .cloned()
            .unwrap_or_else(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
        // context_changed tracks only ordinary context-injection projection mutation.
        // context_changed 仅跟踪普通上下文注入投影变更。
        let mut context_changed = false;
        if let Some(dedupe_key) = dedupe_key.as_deref() {
            context_changed |= runtime_state
                .context_injection_state
                .remove_items_by_dedupe_key(Some(InjectionKind::Transient), dedupe_key);
        }
        if !context_changed && let Some(memory_id) = memory_id {
            context_changed |= runtime_state
                .context_injection_state
                .remove_items_by_metadata_u64(
                    Some(InjectionKind::Transient),
                    "memory_id",
                    memory_id,
                );
        }
        if context_changed {
            runtime_state.updated_at = mutation_timestamp.clone();
        }
        // vmm_controller is committed while the service guard freezes the ordinary-state baseline.
        // vmm_controller 在服务 guard 冻结普通状态基线期间完成提交。
        let vmm_controller = vmm_controller_registry.get_or_open(
            storage_locator,
            session_id,
            &mutation_timestamp,
        )?;
        // vmm_changed reports only the controller-owned lease mutation.
        // vmm_changed 仅报告控制器持有的租约变更。
        let mut vmm_changed = false;
        let vmm_commit =
            vmm_controller.execute_command(&command_id, &mutation_timestamp, |candidate| {
                vmm_changed = candidate
                    .revoke_lease(&suppression_key, &mutation_timestamp)?
                    .changed;
                Ok(())
            })?;
        if !vmm_commit.applied {
            vmm_changed = false;
        }
        if context_changed {
            workspace
                .session_runtime_state
                .insert(session_id, runtime_state);
        }
        (session_id, context_changed, vmm_changed)
    };
    // changed combines two independently owned projections without sharing mutable VMM state.
    // changed 合并两个独立所有投影，且不共享可变 VMM 状态。
    let changed = context_changed || vmm_changed;
    // The VMM controller command is the durable owner; retained runtime state is a live UI projection only.
    // VMM Controller 命令是持久化所有者；保留的运行时状态仅作为实时 UI 投影。
    // Projection emission is idempotent and likewise repairs an interrupted prior response path.
    // 投影发送具备幂等性，同样用于修复先前被中断的响应路径。
    emit_context_injection_changed_delta_for_session(&app, &state, session_id)?;

    let response = json!({
        "changed": changed,
        "suppressedDedupeKey": dedupe_key,
        "suppressedMemoryId": memory_id,
    });
    Ok(VmmMemoryActionResult {
        action: request.action,
        summary: if changed {
            String::from("已在当前会话中隐藏该动态记忆。")
        } else {
            String::from("未找到需要隐藏的动态记忆，可能已被移除。")
        },
        kind: String::from("suppress_context_injection_item"),
        request: json!({
            "commandId": request.command_id,
            "conversationId": request.conversation_id,
            "dedupeKey": dedupe_key,
            "memoryId": memory_id,
        }),
        response: response.clone(),
        data: VmmMemoryActionResponse::ContextInjection(response),
    })
}

/// Builds the durable controller command id for one exact suppression request.
/// 为一次精确抑制请求构建持久控制器命令标识。
///
/// # Parameters
/// 参数
/// `request_command_id` is the UUID generated once when the user operation begins.
/// `request_command_id` 是用户操作开始时仅生成一次的 UUID。
///
/// # Returns
/// 返回值
/// A normalized session-controller command id, or an explicit invalid-UUID error.
/// 返回规范化的会话控制器命令标识，或明确的 UUID 无效错误。
pub(crate) fn vmm_suppression_command_id(request_command_id: &str) -> Result<String, String> {
    // parsed_command_id is normalized once before the controller namespace prefix is attached.
    // parsed_command_id 在附加控制器命名空间前仅规范化一次。
    let parsed_command_id = normalize_vmm_memory_action_command_id(request_command_id)?;
    Ok(format!("suppress_memory:{parsed_command_id}"))
}

/// Validates and normalizes one WebUI VMM action command identity.
/// 校验并规范化一个 WebUI VMM 动作命令身份。
///
/// # Parameters
/// 参数
/// `request_command_id` is the exact caller-supplied command UUID.
/// `request_command_id` 是调用方提供的精确命令 UUID。
///
/// # Returns
/// 返回值
/// The parsed UUID, or an explicit invalid-command diagnostic.
/// 返回解析后的 UUID，或明确的无效命令诊断。
fn normalize_vmm_memory_action_command_id(request_command_id: &str) -> Result<uuid::Uuid, String> {
    if request_command_id != request_command_id.trim() {
        return Err(String::from(
            "VMM memory action commandId must not contain surrounding whitespace.",
        ));
    }
    uuid::Uuid::parse_str(request_command_id)
        .map_err(|error| format!("VMM memory action commandId must be a valid UUID: {error}"))
}

/// Resolves and validates the exact regular session targeted by a context-injection suppression.
/// 解析并校验上下文注入抑制动作所指向的准确常规会话。
///
/// # Parameters
/// 参数
/// `workspace` is the retained workspace containing active regular sessions.
/// `workspace` 是包含活动常规会话的保留工作区。
/// `conversation_id` is the explicit persisted conversation id captured by the initiating WebUI route.
/// `conversation_id` 是发起动作的 WebUI 路由捕获的明确已持久化会话标识。
///
/// # Returns
/// 返回值
/// The validated session id, or an explicit error when the request is missing, malformed, or stale.
/// 返回校验后的会话标识；请求缺失、格式错误或已失效时返回明确错误。
pub(crate) fn vmm_suppression_session_id(
    workspace: &vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    conversation_id: Option<&str>,
) -> Result<vulcan_workbench_model::SessionId, String> {
    // conversation_id is mandatory because falling back to current selection can mutate the wrong session.
    // conversation_id 为必填项，因为回退到当前选中状态可能修改错误会话。
    let conversation_id = conversation_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| String::from("缺少需要隐藏动态记忆的会话标识。"))?;
    // session_id is the strongly typed identity used by workspace runtime-state maps.
    // session_id 是工作区运行态映射使用的强类型身份标识。
    let session_id = conversation_id
        .parse::<vulcan_workbench_model::SessionId>()
        .map_err(|error| format!("无效的动态记忆会话标识：{error}"))?;
    if !workspace
        .sessions
        .iter()
        .any(|session| session.id == session_id)
    {
        return Err(String::from("目标会话已不存在，无法隐藏动态记忆。"));
    }
    Ok(session_id)
}

/// Saves one VMM memory configuration draft and refreshes the WebUI snapshot.
/// 保存单个 VMM 记忆配置草稿并刷新 WebUI 快照。
///
/// # Parameters
/// 参数
/// `app` broadcasts the refreshed snapshot and project-scoped invalidation.
/// `app` 广播刷新后的快照与项目级失效通知。
/// `state` is the shared service-kernel state that owns provider and workspace snapshots.
/// `state` 是持有 Provider 与工作区快照的共享 service-kernel 状态。
/// `request` is the VMM configuration draft submitted by the WebUI.
/// `request` 是 WebUI 提交的 VMM 配置草稿。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the configuration has been saved.
/// 返回配置保存后的刷新工作台快照。
pub(crate) async fn service_save_vmm_memory_config(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: VmmConfigSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    // project_scope distinguishes one project target from an intentional global invalidation.
    // project_scope 区分单个项目目标与有意使用的全局失效通知。
    let project_scope = request.scope.trim().eq_ignore_ascii_case("project");
    // changed_project_id retains the exact project identity before the request is consumed.
    // changed_project_id 在请求被消费前保留准确的项目身份。
    let changed_project_id = project_scope.then(|| request.project_id.clone()).flatten();
    save_vmm_memory_config(&state, request).await?;
    // workbench is the exact snapshot returned to the initiating client and broadcast to peers.
    // workbench 是返回给发起客户端并广播给其他端的准确快照。
    let workbench = refresh_workbench_snapshot(state.clone()).await?;
    emit_workbench_event(&app, &workbench);
    // project_settings_target is absent only for a valid global save that affects every project.
    // project_settings_target 仅在影响所有项目的有效全局保存中为空。
    let project_settings_target = if project_scope {
        Some(changed_project_id.ok_or_else(|| String::from("项目 VMM 配置缺少项目标识。"))?)
    } else {
        None
    };
    commit_and_emit_project_settings_changed_delta(
        &app,
        &state,
        project_settings_target,
        ProjectSettingsDomain::Vmm,
    )?;
    Ok(workbench)
}

/// Loads one project-scoped VMM configuration draft for the WebUI project settings modal.
/// 为 WebUI 项目设置弹窗加载单个项目级 VMM 配置草稿。
///
/// # Parameters
/// 参数
/// `state` is the shared service-kernel state used to resolve the project scope.
/// `state` 是用于解析项目作用域的共享 service-kernel 状态。
/// `project_id` identifies the task project whose VMM draft should be loaded.
/// `project_id` 标识需要加载 VMM 草稿的任务项目。
///
/// # Returns
/// 返回值
/// The editable project VMM draft summary for the WebUI.
/// 返回供 WebUI 编辑的项目级 VMM 草稿摘要。
pub(crate) fn service_load_project_vmm_config(
    state: SharedServiceBridgeState,
    project_id: String,
) -> Result<VmmConfigDraftSummary, String> {
    load_project_vmm_config(&state, &project_id)
}

/// Builds the VMM action context for one WebUI management request.
/// 为单个 WebUI 管理请求构建 VMM 动作上下文。
///
/// # Parameters
/// 参数
/// `state` is the shared service state used to read provider and workspace snapshots.
/// `state` 是用于读取 Provider 与工作区快照的共享服务状态。
/// `request` is the WebUI VMM action request whose scope and payload choose the context.
/// `request` 是由作用域和载荷决定上下文的 WebUI VMM 动作请求。
///
/// # Returns
/// 返回值
/// The resolved VMM layer plus user and project references for the management call.
/// 返回管理调用使用的 VMM 配置层，以及用户和项目引用。
pub(crate) fn vmm_action_context(
    state: &SharedServiceBridgeState,
    request: &VmmMemoryActionRequest,
) -> Result<(vulcan_vmm::VmmConfigLayer, String, String), String> {
    let (provider_configuration, workspace_snapshot) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard
            .workspace_snapshot
            .clone()
            .ok_or_else(|| String::from("工作区尚未载入，无法执行 VMM 管理动作。"))?;
        let provider_configuration = guard
            .agent_runtime
            .as_ref()
            .map(|runtime| runtime.provider_configuration.clone())
            .ok_or_else(|| String::from("service is still starting; agent runtime is not ready"))?;
        (provider_configuration, workspace_snapshot)
    };
    let scope = request.scope.trim().to_ascii_lowercase();
    if scope == "project" {
        let project = vmm_request_project_scope(&workspace_snapshot, request.project_id.as_deref())
            .ok_or_else(|| String::from("请先选择一个项目作用域。"))?;
        let project_layer = cached_project_vmm_layer(&project.path);
        let effective = vulcan_vmm::VmmEffectiveConfig::resolve(
            &provider_configuration.vmm,
            Some(&project_layer),
            None,
        );
        let layer = vmm_action_layer_from_effective(&effective);
        let management_user_ref = vmm_payload_text(&request.payload, "userRef")
            .or_else(|| project_layer.user_id.map(|value| value.to_string()))
            .or_else(|| layer.user_id.map(|value| value.to_string()))
            .unwrap_or_default();
        let management_project_ref = vmm_payload_text(&request.payload, "projectRef")
            .or_else(|| project_layer.project_id.map(|value| value.to_string()))
            .or_else(|| layer.project_id.map(|value| value.to_string()))
            .unwrap_or_default();
        return Ok((layer, management_user_ref, management_project_ref));
    }
    let layer = provider_configuration.vmm;
    let management_user_ref = vmm_payload_text(&request.payload, "userRef")
        .or_else(|| layer.user_id.map(|value| value.to_string()))
        .unwrap_or_default();
    let management_project_ref = vmm_payload_text(&request.payload, "projectRef")
        .or_else(|| layer.project_id.map(|value| value.to_string()))
        .unwrap_or_default();
    Ok((layer, management_user_ref, management_project_ref))
}

/// Converts an effective VMM configuration into a standalone action layer.
/// 将 VMM 最终配置转换为独立动作配置层。
pub(crate) fn vmm_action_layer_from_effective(
    effective: &vulcan_vmm::VmmEffectiveConfig,
) -> vulcan_vmm::VmmConfigLayer {
    vulcan_vmm::VmmConfigLayer {
        enabled: Some(effective.enabled),
        grpc_target: (!effective.grpc_target.trim().is_empty())
            .then(|| effective.grpc_target.clone()),
        grpc_api_key: effective.grpc_api_key.clone(),
        management_base_url: None,
        management_bearer_token: None,
        management_timeout_ms: None,
        handshake_timeout_ms: Some(effective.handshake_timeout_ms),
        receive_timeout_ms: Some(effective.receive_timeout_ms),
        max_receive_message_bytes: effective.max_receive_message_bytes,
        grpc_keepalive_time_ms: Some(effective.grpc_keepalive_time_ms),
        grpc_keepalive_timeout_ms: Some(effective.grpc_keepalive_timeout_ms),
        grpc_keepalive_permit_without_calls: Some(effective.grpc_keepalive_permit_without_calls),
        user_id: effective.user_id,
        project_id: effective.project_id,
        visible_memory_injection: Some(effective.visible_memory_injection),
        implicit_memory_turns: Some(effective.implicit_memory_turns),
        profile_refresh_turns: Some(effective.profile_refresh_turns),
        profile_node_limit: Some(effective.profile_node_limit),
        session_compact_recall: Some(effective.session_compact_recall),
        precheck_enabled: Some(effective.precheck_enabled),
        memory_writeback_enabled: Some(effective.memory_writeback_enabled),
        ai_tools_enabled: Some(effective.ai_tools_enabled),
        injection_tag: Some(effective.injection_tag.clone()),
        injection_description: Some(effective.injection_description.clone()),
        install: Some(effective.install.clone()),
    }
}

/// Parses one WebUI VMM action request into the runtime action enum.
/// 将单个 WebUI VMM 动作请求解析为运行时动作枚举。
///
/// # Parameters
/// 参数
/// `request` is the WebUI action request with an action id and JSON payload.
/// `request` 是包含动作标识与 JSON 载荷的 WebUI 动作请求。
///
/// # Returns
/// 返回值
/// The concrete service-runtime VMM workbench action represented by the request.
/// 返回该请求对应的具体 service-runtime VMM 工作台动作。
pub(crate) fn parse_vmm_memory_action(
    request: &VmmMemoryActionRequest,
) -> Result<vulcan_service_runtime::vmm_management::VmmWorkbenchAction, String> {
    let payload = &request.payload;
    match request.action.trim() {
        "healthz" => Ok(vulcan_service_runtime::vmm_management::VmmWorkbenchAction::Healthz),
        "load_users_projects" => {
            Ok(vulcan_service_runtime::vmm_management::VmmWorkbenchAction::LoadUsersAndProjects)
        }
        "resolve_user_project" => {
            Ok(vulcan_service_runtime::vmm_management::VmmWorkbenchAction::ResolveUserAndProject)
        }
        "ensure_project_preview" => {
            Ok(vulcan_service_runtime::vmm_management::VmmWorkbenchAction::EnsureProjectPreview)
        }
        "load_profile_nodes" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::LoadProfileNodes {
                target: parse_vmm_profile_target(
                    vmm_payload_text(payload, "target")
                        .as_deref()
                        .unwrap_or("user"),
                )?,
            },
        ),
        "load_profile_bundle" => {
            Ok(vulcan_service_runtime::vmm_management::VmmWorkbenchAction::LoadProfileBundle)
        }
        "apply_profile_instruction" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::ApplyProfileInstruction {
                target: parse_vmm_profile_target(
                    vmm_payload_text(payload, "target")
                        .as_deref()
                        .unwrap_or("user"),
                )?,
                instruction: vmm_payload_text(payload, "instruction").unwrap_or_default(),
            },
        ),
        "resolve_user" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::ResolveUser {
                user_ref: vmm_payload_text(payload, "userRef").unwrap_or_default(),
                confirm_create: vmm_payload_bool(payload, "confirmCreate"),
            },
        ),
        "delete_user" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::DeleteUser {
                user_ref: vmm_payload_text(payload, "userRef").unwrap_or_default(),
                confirmation_code: vmm_payload_text(payload, "confirmationCode")
                    .unwrap_or_default(),
            },
        ),
        "ensure_project" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::EnsureProject {
                project_path: vmm_payload_text(payload, "projectPath").unwrap_or_default(),
                confirm_create: vmm_payload_bool(payload, "confirmCreate"),
            },
        ),
        "delete_project" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::DeleteProject {
                project_path: vmm_payload_text(payload, "projectPath").unwrap_or_default(),
                confirm_delete: vmm_payload_bool(payload, "confirmDelete"),
            },
        ),
        "migrate_project" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::MigrateProject {
                source_project_path: vmm_payload_text(payload, "sourceProjectPath")
                    .unwrap_or_default(),
                target_project_path: vmm_payload_text(payload, "targetProjectPath")
                    .unwrap_or_default(),
                confirm_migrate: vmm_payload_bool(payload, "confirmMigrate"),
            },
        ),
        "search_memory_events" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::SearchMemoryEvents {
                queries: vmm_payload_string_list(payload, "queries"),
                top_k: vmm_payload_u32(payload, "topK").unwrap_or(5),
            },
        ),
        "get_turn_details" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::GetTurnDetails {
                turn_ids: vmm_payload_u64_list(payload, "turnIds")?,
            },
        ),
        "write_memories" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::WriteMemories {
                session_id: vmm_payload_text(payload, "sessionId")
                    .ok_or_else(|| String::from("WriteMemories 缺少明确的 sessionId。"))?,
                item: vulcan_vmm::WriteMemoryItem {
                    scope_level: vmm_payload_i32(payload, "scopeLevel").unwrap_or(2),
                    abstract_text: vmm_payload_text(payload, "abstractText").unwrap_or_default(),
                    details: vmm_payload_text(payload, "details").unwrap_or_default(),
                    category: vmm_payload_i32(payload, "category").unwrap_or(1),
                    priority: vmm_payload_i32(payload, "priority").unwrap_or(2),
                    memory_level: vmm_payload_i32(payload, "memoryLevel").unwrap_or(2),
                },
            },
        ),
        "delete_memories" => Ok(
            vulcan_service_runtime::vmm_management::VmmWorkbenchAction::DeleteMemories {
                memory_ids: vmm_payload_u64_list(payload, "memoryIds")?,
                reason: vmm_payload_text(payload, "reason").unwrap_or_default(),
            },
        ),
        other => Err(format!("不支持的 VMM 记忆动作：{other}")),
    }
}

/// Builds a JSON request preview for one VMM action.
/// 为单个 VMM 动作构建 JSON 请求预览。
pub(crate) fn vmm_action_request_preview(
    action: &vulcan_service_runtime::vmm_management::VmmWorkbenchAction,
) -> Result<Value, String> {
    serde_json::to_value(format!("{action:?}"))
        .map(|debug| json!({ "action": debug }))
        .map_err(|error| format!("序列化 VMM 请求预览失败：{error}"))
}

/// Projects one VMM management outcome into the WebUI action result shape.
/// 将单个 VMM 管理结果投影为 WebUI 动作结果结构。
pub(crate) fn vmm_memory_action_result(
    action: &str,
    request: Value,
    outcome: vulcan_service_runtime::vmm_management::VmmManagementOutcome,
) -> Result<VmmMemoryActionResult, String> {
    use vulcan_service_runtime::vmm_management::VmmManagementOutcome;

    let (kind, summary, response) = match outcome {
        VmmManagementOutcome::Healthz(status) => (
            "healthz",
            format!("VMM 连接正常：{status}"),
            json!({ "status": status }),
        ),
        VmmManagementOutcome::UsersAndProjects { users, projects } => {
            let summary = format!(
                "用户 {} 个；项目 {} 个。",
                users.users.len(),
                projects.projects.len()
            );
            (
                "users_projects",
                summary,
                json!({ "users": users.users, "projects": projects.projects }),
            )
        }
        VmmManagementOutcome::Resolved { user, project } => {
            let user_text = user
                .user
                .as_ref()
                .map(|entry| entry.user_name.clone())
                .unwrap_or_else(|| user.message.clone());
            let project_text = project
                .project
                .as_ref()
                .map(|entry| entry.project_name.clone())
                .unwrap_or_else(|| project.message.clone());
            (
                "resolved",
                format!("用户 {user_text}；项目 {project_text}。"),
                json!({ "user": user, "project": project }),
            )
        }
        VmmManagementOutcome::EnsureProject(response) => {
            let project_text = response
                .project
                .as_ref()
                .map(|entry| entry.project_name.clone())
                .unwrap_or_else(|| response.message.clone());
            let summary = if response.exists {
                format!("项目“{project_text}”已经存在。")
            } else if response.needs_confirm {
                format!("项目“{project_text}”尚未创建，确认后即可建立。")
            } else {
                format!("项目“{project_text}”已创建。")
            };
            ("ensure_project", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::DeleteUser(response) => {
            let summary = if response.requires_confirmation {
                String::from("删除该用户前还需要一次确认。")
            } else {
                format!(
                    "已删除 {} 个用户，并清理 {} 条记忆和 {} 条画像。",
                    response.deleted_users, response.deleted_memories, response.deleted_profiles
                )
            };
            ("delete_user", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::DeleteProject(response) => {
            let summary = if response.needs_confirm {
                String::from("删除该项目前还需要一次确认。")
            } else {
                format!(
                    "已删除 {} 个项目，并清理 {} 条记忆和 {} 条画像。",
                    response.deleted_projects, response.deleted_memories, response.deleted_profiles
                )
            };
            ("delete_project", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::MigrateProject(response) => {
            let summary = if response.needs_confirm {
                String::from("迁移该项目前还需要一次确认。")
            } else {
                format!(
                    "项目迁移完成：已迁移 {} 个会话和 {} 条记忆。",
                    response.migrated_sessions, response.migrated_memories
                )
            };
            ("migrate_project", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::ProfileNodes(response) => {
            let summary = format!("已读取 {} 条有效画像。", response.nodes.len());
            ("profile_nodes", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::ProfileBundle(response) => {
            let summary = String::from("画像资料已加载。");
            ("profile_bundle", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::ProfileInstruction(response) => {
            let summary = format!(
                "画像指令已应用：新增或更新 {} 条，停用 {} 条。",
                response.accepted_nodes.len(),
                response.retired_nodes.len()
            );
            (
                "profile_instruction",
                summary,
                vmm_response_value(response)?,
            )
        }
        VmmManagementOutcome::ResolveUser(response) => {
            let user_text = response
                .user
                .as_ref()
                .map(|entry| entry.user_name.clone())
                .unwrap_or_else(|| response.message.clone());
            let summary = if response.created {
                format!("用户“{user_text}”已创建。")
            } else if response.exists {
                format!("已找到用户“{user_text}”。")
            } else {
                format!("用户“{user_text}”尚未创建。")
            };
            ("resolve_user", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::SearchMemoryEvents(response) => {
            let hit_count = response
                .results
                .iter()
                .map(|group| group.hits.len())
                .sum::<usize>();
            let summary = format!("已找到 {hit_count} 条相关记忆。");
            (
                "search_memory_events",
                summary,
                vmm_response_value(response)?,
            )
        }
        VmmManagementOutcome::GetTurnDetails(response) => {
            let summary = format!("已读取 {} 个来源回合。", response.turns.len());
            ("get_turn_details", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::WriteMemories(response) => {
            let summary = format!("已写入 {} 条记忆。", response.items.len());
            ("write_memories", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::DeleteMemories(response) => {
            let summary = format!(
                "已删除 {} 条记忆，另有 {} 条未找到。",
                response.deleted_memory_ids.len(),
                response.not_found_memory_ids.len()
            );
            ("delete_memories", summary, vmm_response_value(response)?)
        }
    };
    let data = vmm_typed_action_response(kind, &response)?;
    Ok(VmmMemoryActionResult {
        action: action.to_string(),
        summary,
        kind: kind.to_string(),
        request,
        response,
        data,
    })
}

/// Decodes the kernel-owned response projection into the public typed action contract.
/// 将内核持有的响应投影解码为公开强类型动作契约。
///
/// # Parameters
/// 参数
/// `kind` is the exact result discriminator selected by `vmm_memory_action_result`.
/// `kind` 是 `vmm_memory_action_result` 选择的准确结果判别值。
/// `response` is the exact JSON projection produced from the corresponding VMM protocol type.
/// `response` 是从对应 VMM 协议类型生成的准确 JSON 投影。
///
/// # Returns
/// 返回值
/// A typed response that normal WebUI pages can consume without inspecting arbitrary JSON.
/// 返回普通 WebUI 页面无需检查任意 JSON 即可消费的强类型响应。
fn vmm_typed_action_response(
    kind: &str,
    response: &Value,
) -> Result<VmmMemoryActionResponse, String> {
    match kind {
        "healthz" => Ok(VmmMemoryActionResponse::Healthz {
            status: response
                .get("status")
                .and_then(Value::as_str)
                .ok_or_else(|| String::from("VMM healthz 响应缺少准确的 status 字段。"))?
                .to_string(),
        }),
        "users_projects" => Ok(VmmMemoryActionResponse::UsersProjects {
            users: decode_vmm_action_field(response, "users")?,
            projects: decode_vmm_action_field(response, "projects")?,
        }),
        "resolved" => Ok(VmmMemoryActionResponse::Resolved {
            user: decode_vmm_action_field(response, "user")?,
            project: decode_vmm_action_field(response, "project")?,
        }),
        "ensure_project" => Ok(VmmMemoryActionResponse::EnsureProject(
            decode_vmm_action_value(response)?,
        )),
        "delete_user" => Ok(VmmMemoryActionResponse::DeleteUser(
            decode_vmm_action_value(response)?,
        )),
        "delete_project" => Ok(VmmMemoryActionResponse::DeleteProject(
            decode_vmm_action_value(response)?,
        )),
        "migrate_project" => Ok(VmmMemoryActionResponse::MigrateProject(
            decode_vmm_action_value(response)?,
        )),
        "profile_nodes" => Ok(VmmMemoryActionResponse::ProfileNodes(
            decode_vmm_action_value(response)?,
        )),
        "profile_bundle" => Ok(VmmMemoryActionResponse::ProfileBundle(
            decode_vmm_action_value(response)?,
        )),
        "profile_instruction" => Ok(VmmMemoryActionResponse::ProfileInstruction(
            decode_vmm_action_value(response)?,
        )),
        "resolve_user" => Ok(VmmMemoryActionResponse::ResolveUser(
            decode_vmm_action_value(response)?,
        )),
        "search_memory_events" => Ok(VmmMemoryActionResponse::SearchMemoryEvents(
            decode_vmm_action_value(response)?,
        )),
        "get_turn_details" => Ok(VmmMemoryActionResponse::GetTurnDetails(
            decode_vmm_action_value(response)?,
        )),
        "write_memories" => Ok(VmmMemoryActionResponse::WriteMemories(
            decode_vmm_action_value(response)?,
        )),
        "delete_memories" => Ok(VmmMemoryActionResponse::DeleteMemories(
            decode_vmm_action_value(response)?,
        )),
        other => Err(format!("VMM 动作结果缺少强类型投影：{other}")),
    }
}

/// Decodes one exact response value into its protocol-owned Rust type.
/// 将一个准确响应值解码为协议持有的 Rust 类型。
fn decode_vmm_action_value<T>(response: &Value) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    serde_json::from_value(response.clone()).map_err(vmm_json_error)
}

/// Decodes one exact named field from a kernel-owned response object.
/// 从内核持有的响应对象解码一个准确命名字段。
fn decode_vmm_action_field<T>(response: &Value, field: &str) -> Result<T, String>
where
    T: serde::de::DeserializeOwned,
{
    let value = response
        .get(field)
        .ok_or_else(|| format!("VMM 动作响应缺少准确的 {field} 字段。"))?;
    decode_vmm_action_value(value)
}

/// Serializes one VMM response payload for a WebUI action result.
/// 为 WebUI 动作结果序列化单个 VMM 响应载荷。
fn vmm_response_value(response: impl serde::Serialize) -> Result<Value, String> {
    serde_json::to_value(response).map_err(vmm_json_error)
}

/// Formats a JSON projection error for VMM responses.
/// 格式化 VMM 响应 JSON 投影错误。
pub(crate) fn vmm_json_error(error: serde_json::Error) -> String {
    format!("序列化 VMM 响应失败：{error}")
}
