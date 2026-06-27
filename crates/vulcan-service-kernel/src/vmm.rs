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
    load_project_configuration, persist_workspace_session_update,
    provider_configuration_from_state, refresh_workbench_snapshot,
    selected_session_runtime_state_mut, selected_workspace_session,
    vmm_hooks::{refresh_guard_vmm_hook_snapshot, refresh_service_vmm_hook_snapshot},
    vmm_project_scope_path, vmm_request_project_scope,
};

/// Runs one VMM memory management action and returns its structured result.
/// 执行单个 VMM 记忆管理动作并返回结构化结果。
///
/// # Parameters
/// 参数
/// `state` is the shared service-kernel state used to read configuration and session state.
/// `state` 是用于读取配置与会话状态的共享 service-kernel 状态。
/// `request` is the WebUI VMM action request to execute.
/// `request` 是需要执行的 WebUI VMM 动作请求。
///
/// # Returns
/// 返回值
/// A structured WebUI VMM action result or a user-visible error message.
/// 返回结构化 WebUI VMM 动作结果，或用户可见的错误消息。
pub(super) async fn service_run_vmm_memory_action(
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    if request.action.trim() == "suppress_context_injection_item" {
        return service_suppress_context_injection_item(state, request).await;
    }
    let (layer, management_user_ref, management_project_ref) =
        vmm_action_context(&state, &request)?;
    let action = parse_vmm_memory_action(&request)?;
    let request_preview = vmm_action_request_preview(&action)?;
    let outcome = vulcan_service_runtime::vmm_management::run_vmm_workbench_action(
        action,
        layer,
        management_user_ref,
        management_project_ref,
    )
    .await?;
    vmm_memory_action_result(&request.action, request_preview, outcome)
}

/// Suppresses one context-injection item only for the selected session.
/// 仅在当前选中会话中抑制单条上下文注入内容。
async fn service_suppress_context_injection_item(
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let dedupe_key = vmm_payload_text(&request.payload, "dedupeKey");
    let memory_id = vmm_payload_u64(&request.payload, "memoryId");
    if dedupe_key.is_none() && memory_id.is_none() {
        return Err(String::from("缺少需要隐藏的记忆条目标识。"));
    }

    let (controller_runtime, workspace_snapshot, session_id, changed) = {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let controller_runtime = guard.controller_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; controller runtime is not ready")
        })?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("工作区尚未载入，无法隐藏动态记忆。"))?;
        let (session_id, runtime_state) = selected_session_runtime_state_mut(workspace)
            .ok_or_else(|| String::from("请先选择一个可用会话。"))?;
        let mut changed = false;
        if let Some(dedupe_key) = dedupe_key.as_deref() {
            changed |= runtime_state
                .context_injection_state
                .remove_items_by_dedupe_key(Some(InjectionKind::Transient), dedupe_key);
        }
        if !changed && let Some(memory_id) = memory_id {
            changed |= runtime_state
                .context_injection_state
                .remove_items_by_metadata_u64(
                    Some(InjectionKind::Transient),
                    "memory_id",
                    memory_id,
                );
        }
        if changed {
            runtime_state.updated_at = current_unix_millis_text();
        }
        (controller_runtime, workspace.clone(), session_id, changed)
    };

    if changed {
        persist_workspace_session_update(
            Some(&state),
            controller_runtime,
            &workspace_snapshot,
            session_id,
        )
        .await?;
    }

    Ok(VmmMemoryActionResult {
        action: request.action,
        summary: if changed {
            String::from("已在当前会话中隐藏该动态记忆。")
        } else {
            String::from("未找到需要隐藏的动态记忆，可能已被移除。")
        },
        kind: String::from("suppress_context_injection_item"),
        request: json!({
            "dedupeKey": dedupe_key,
            "memoryId": memory_id,
        }),
        response: json!({
            "changed": changed,
            "suppressedDedupeKey": dedupe_key,
            "suppressedMemoryId": memory_id,
        }),
    })
}

/// Saves one VMM memory configuration draft and refreshes the WebUI snapshot.
/// 保存单个 VMM 记忆配置草稿并刷新 WebUI 快照。
///
/// # Parameters
/// 参数
/// `state` is the shared service-kernel state that owns provider and workspace snapshots.
/// `state` 是持有 Provider 与工作区快照的共享 service-kernel 状态。
/// `request` is the VMM configuration draft submitted by the WebUI.
/// `request` 是 WebUI 提交的 VMM 配置草稿。
///
/// # Returns
/// 返回值
/// The refreshed workbench snapshot after the configuration has been saved.
/// 返回配置保存后的刷新工作台快照。
pub(super) async fn service_save_vmm_memory_config(
    state: SharedServiceBridgeState,
    request: VmmConfigSaveRequest,
) -> Result<WorkbenchSnapshot, String> {
    save_vmm_memory_config(&state, request)?;
    refresh_workbench_snapshot(state).await
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
pub(super) fn service_load_project_vmm_config(
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
fn vmm_action_context(
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
        let project_layer = load_project_configuration(&project.path).vmm;
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
fn vmm_action_layer_from_effective(
    effective: &vulcan_vmm::VmmEffectiveConfig,
) -> vulcan_vmm::VmmConfigLayer {
    vulcan_vmm::VmmConfigLayer {
        enabled: Some(effective.enabled),
        grpc_target: (!effective.grpc_target.trim().is_empty())
            .then(|| effective.grpc_target.clone()),
        grpc_api_key: effective.grpc_api_key.clone(),
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
fn parse_vmm_memory_action(
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
fn vmm_action_request_preview(
    action: &vulcan_service_runtime::vmm_management::VmmWorkbenchAction,
) -> Result<Value, String> {
    serde_json::to_value(format!("{action:?}"))
        .map(|debug| json!({ "action": debug }))
        .map_err(|error| format!("序列化 VMM 请求预览失败：{error}"))
}

/// Projects one VMM management outcome into the WebUI action result shape.
/// 将单个 VMM 管理结果投影为 WebUI 动作结果结构。
fn vmm_memory_action_result(
    action: &str,
    request: Value,
    outcome: vulcan_service_runtime::vmm_management::VmmManagementOutcome,
) -> Result<VmmMemoryActionResult, String> {
    let summary = outcome.summary();
    let (kind, response) = match outcome {
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::Healthz(status) => {
            ("healthz", json!({ "status": status }))
        }
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::UsersAndProjects {
            users,
            projects,
        } => (
            "users_projects",
            json!({ "users": users.users, "projects": projects.projects }),
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::Resolved {
            user,
            project,
        } => ("resolved", json!({ "user": user, "project": project })),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::EnsureProject(response) => (
            "ensure_project",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::DeleteUser(response) => (
            "delete_user",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::DeleteProject(response) => (
            "delete_project",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::MigrateProject(response) => (
            "migrate_project",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::ProfileNodes(response) => (
            "profile_nodes",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::ProfileBundle(response) => (
            "profile_bundle",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::ProfileInstruction(
            response,
        ) => (
            "profile_instruction",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::ResolveUser(response) => (
            "resolve_user",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::SearchMemoryEvents(
            response,
        ) => (
            "search_memory_events",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::GetTurnDetails(response) => (
            "get_turn_details",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::WriteMemories(response) => (
            "write_memories",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
        vulcan_service_runtime::vmm_management::VmmManagementOutcome::DeleteMemories(response) => (
            "delete_memories",
            serde_json::to_value(response).map_err(vmm_json_error)?,
        ),
    };
    Ok(VmmMemoryActionResult {
        action: action.to_string(),
        summary,
        kind: kind.to_string(),
        request,
        response,
    })
}

/// Formats a JSON projection error for VMM responses.
/// 格式化 VMM 响应 JSON 投影错误。
fn vmm_json_error(error: serde_json::Error) -> String {
    format!("序列化 VMM 响应失败：{error}")
}

/// Saves one VMM memory configuration draft.
/// 保存单个 VMM 记忆配置草稿。
///
/// # Parameters
/// 参数
/// `state` is the shared service state used to resolve and refresh persisted settings.
/// `state` 是用于解析并刷新持久化设置的共享服务状态。
/// `request` is the WebUI configuration save request.
/// `request` 是 WebUI 配置保存请求。
///
/// # Returns
/// 返回值
/// `Ok(())` after the configuration file and hook snapshot have been updated.
/// 配置文件与 hook 快照更新完成后返回 `Ok(())`。
fn save_vmm_memory_config(
    state: &SharedServiceBridgeState,
    mut request: VmmConfigSaveRequest,
) -> Result<(), String> {
    let scope = request.scope.trim().to_ascii_lowercase();
    if scope == "project" {
        let project_path = vmm_project_scope_path(state, request.project_id.as_deref())?;
        let mut project_configuration =
            vulcan_provider_config::config::load_project_configuration_file(&PathBuf::from(
                &project_path,
            ))
            .unwrap_or_default();
        project_configuration.vmm = if request.draft.has_override {
            vmm_config_layer_from_draft(&request.draft, false)?
        } else {
            vulcan_vmm::VmmConfigLayer::default()
        };
        vulcan_provider_config::config::save_project_configuration_file(
            &PathBuf::from(&project_path),
            &project_configuration,
        )?;
        refresh_service_vmm_hook_snapshot(state)?;
        return Ok(());
    }
    let mut configuration = provider_configuration_from_state(state)?;
    if request.draft.grpc_api_key.trim() == WEBUI_REDACTED_SECRET_PLACEHOLDER {
        request.draft.grpc_api_key = configuration.vmm.grpc_api_key.clone().unwrap_or_default();
    }
    configuration.vmm = vmm_config_layer_from_draft(&request.draft, true)?;
    vulcan_provider_config::config::save_configuration_file(&configuration)?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    if let Some(runtime) = guard.agent_runtime.as_mut() {
        runtime.provider_configuration = configuration.clone();
    }
    refresh_guard_vmm_hook_snapshot(&mut guard, &configuration);
    Ok(())
}

/// Loads one persisted project VMM layer and resolves its effective editable fallback.
/// 加载单个已持久化的项目 VMM 层，并解析其可编辑回退值。
fn load_project_vmm_config(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<VmmConfigDraftSummary, String> {
    let project_path = vmm_project_scope_path(state, Some(project_id))?;
    let provider_configuration = provider_configuration_from_state(state)?;
    let project_layer = load_project_configuration(&project_path).vmm;
    let project_effective = vulcan_vmm::VmmEffectiveConfig::resolve(
        &provider_configuration.vmm,
        Some(&project_layer),
        None,
    );
    Ok(vmm_config_draft_summary(
        &project_layer,
        &project_effective,
        !project_layer.is_empty(),
    ))
}

/// Converts a WebUI VMM draft into a persisted configuration layer.
/// 将 WebUI VMM 草稿转换为持久化配置层。
///
/// # Parameters
/// 参数
/// `draft` is the editable WebUI VMM configuration summary.
/// `draft` 是 WebUI 可编辑的 VMM 配置摘要。
/// `include_transport` controls whether transport fields should be persisted for this layer.
/// `include_transport` 控制当前配置层是否持久化传输字段。
///
/// # Returns
/// 返回值
/// The persisted VMM configuration layer represented by the draft.
/// 返回该草稿表示的持久化 VMM 配置层。
fn vmm_config_layer_from_draft(
    draft: &VmmConfigDraftSummary,
    include_transport: bool,
) -> Result<vulcan_vmm::VmmConfigLayer, String> {
    Ok(vulcan_vmm::VmmConfigLayer {
        enabled: Some(draft.enabled),
        grpc_target: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_target))
            .flatten(),
        grpc_api_key: include_transport
            .then(|| vmm_non_empty_text(&draft.grpc_api_key))
            .flatten(),
        handshake_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.handshake_timeout_ms, "VMM 握手超时"))
            .transpose()?
            .flatten(),
        receive_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.receive_timeout_ms, "VMM 请求超时"))
            .transpose()?
            .flatten(),
        max_receive_message_bytes: include_transport
            .then(|| {
                vmm_parse_optional_usize(&draft.max_receive_message_bytes, "VMM 最大接收字节数")
            })
            .transpose()?
            .flatten(),
        grpc_keepalive_time_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.grpc_keepalive_time_ms, "VMM keepalive 间隔"))
            .transpose()?
            .flatten(),
        grpc_keepalive_timeout_ms: include_transport
            .then(|| vmm_parse_optional_u64(&draft.grpc_keepalive_timeout_ms, "VMM keepalive 超时"))
            .transpose()?
            .flatten(),
        grpc_keepalive_permit_without_calls: include_transport
            .then_some(draft.grpc_keepalive_permit_without_calls),
        user_id: vmm_parse_optional_u64(&draft.user_id, "VMM 用户 ID")?,
        project_id: vmm_parse_optional_u64(&draft.project_id, "VMM 项目 ID")?,
        visible_memory_injection: Some(false),
        implicit_memory_turns: vmm_parse_optional_u32(
            &draft.implicit_memory_turns,
            "临时记忆 TTL",
        )?,
        profile_refresh_turns: vmm_parse_optional_u32(
            &draft.profile_refresh_turns,
            "画像刷新间隔",
        )?,
        profile_node_limit: vmm_parse_optional_u32(&draft.profile_node_limit, "画像节点上限")?,
        session_compact_recall: Some(true),
        precheck_enabled: Some(draft.precheck_enabled),
        ai_tools_enabled: Some(draft.ai_tools_enabled),
        injection_tag: vmm_non_empty_text(&draft.injection_tag),
        injection_description: vmm_non_empty_text(&draft.injection_description),
        install: None,
    })
}

/// Parses one profile target key from WebUI.
/// 从 WebUI 解析单个画像目标键。
fn parse_vmm_profile_target(value: &str) -> Result<vulcan_vmm::ProfileTarget, String> {
    match value.trim().to_ascii_lowercase().as_str() {
        "user" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetUser),
        "project" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetProject),
        "team" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetTeam),
        "space" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetSpace),
        "all" => Ok(vulcan_vmm::ProfileTarget::ProfileTargetAll),
        other => Err(format!("不支持的 VMM 画像目标：{other}")),
    }
}

/// Reads one optional string from an action payload.
/// 从动作载荷读取一个可选字符串。
fn vmm_payload_text(payload: &Value, key: &str) -> Option<String> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Reads one boolean from an action payload.
/// 从动作载荷读取一个布尔值。
fn vmm_payload_bool(payload: &Value, key: &str) -> bool {
    payload.get(key).and_then(Value::as_bool).unwrap_or(false)
}

/// Reads one optional u32 from an action payload.
/// 从动作载荷读取一个可选 u32。
fn vmm_payload_u32(payload: &Value, key: &str) -> Option<u32> {
    payload
        .get(key)
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
        })
        .and_then(|value| u32::try_from(value).ok())
}

/// Reads one optional i32 from an action payload.
/// 从动作载荷读取一个可选 i32。
fn vmm_payload_i32(payload: &Value, key: &str) -> Option<i32> {
    payload
        .get(key)
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str()?.trim().parse::<i64>().ok())
        })
        .and_then(|value| i32::try_from(value).ok())
}

/// Reads one optional u64 from an action payload.
/// 从动作载荷读取一个可选 u64。
fn vmm_payload_u64(payload: &Value, key: &str) -> Option<u64> {
    payload.get(key).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
    })
}

/// Reads a string list from an action payload.
/// 从动作载荷读取字符串列表。
fn vmm_payload_string_list(payload: &Value, key: &str) -> Vec<String> {
    if let Some(values) = payload.get(key).and_then(Value::as_array) {
        return values
            .iter()
            .filter_map(Value::as_str)
            .flat_map(vmm_split_text_list)
            .collect();
    }
    payload
        .get(key)
        .and_then(Value::as_str)
        .map(vmm_split_text_list)
        .unwrap_or_default()
}

/// Reads a u64 list from an action payload.
/// 从动作载荷读取 u64 列表。
fn vmm_payload_u64_list(payload: &Value, key: &str) -> Result<Vec<u64>, String> {
    if let Some(values) = payload.get(key).and_then(Value::as_array) {
        return values
            .iter()
            .map(|value| {
                value
                    .as_u64()
                    .or_else(|| value.as_str()?.trim().parse::<u64>().ok())
                    .ok_or_else(|| format!("无法解析 VMM u64 id：{value}"))
            })
            .collect();
    }
    vmm_payload_string_list(payload, key)
        .into_iter()
        .map(|value| {
            value
                .parse::<u64>()
                .map_err(|_| format!("无法解析 VMM u64 id：{value}"))
        })
        .collect()
}

/// Splits compact text list syntax used by the WebUI forms.
/// 拆分 WebUI 表单使用的紧凑文本列表语法。
fn vmm_split_text_list(value: &str) -> Vec<String> {
    value
        .split(['|', ',', '\n'])
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Returns normalized non-empty text.
/// 返回规范化后的非空文本。
fn vmm_non_empty_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// Parses an optional u64 draft field.
/// 解析可选 u64 草稿字段。
fn vmm_parse_optional_u64(value: &str, label: &str) -> Result<Option<u64>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u64>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

/// Parses an optional u32 draft field.
/// 解析可选 u32 草稿字段。
fn vmm_parse_optional_u32(value: &str, label: &str) -> Result<Option<u32>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<u32>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是 32 位非负整数。"))
}

/// Parses an optional usize draft field.
/// 解析可选 usize 草稿字段。
fn vmm_parse_optional_usize(value: &str, label: &str) -> Result<Option<usize>, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    trimmed
        .parse::<usize>()
        .map(Some)
        .map_err(|_| format!("{label} 必须是非负整数。"))
}

/// Builds the structured static VMM profile snapshot from active profile nodes.
/// 根据 active 画像节点构建结构化静态 VMM 画像快照。
///
/// # Parameters
/// 参数
/// `state` is the selected session runtime state containing profile-sync nodes.
/// `state` 是包含画像同步节点的当前会话运行态。
///
/// # Returns
/// 返回值
/// A grouped static profile summary when active profile nodes exist.
/// 存在 active 画像节点时返回分组后的静态画像摘要。
pub(super) fn static_vmm_profile_snapshot(
    state: &SessionRuntimeStateEntry,
) -> Option<VmmStaticProfileSummary> {
    let sections = static_vmm_profile_sections(&state.vmm_profile_sync.nodes_by_id);
    if sections.is_empty() {
        return None;
    }
    Some(VmmStaticProfileSummary {
        updated_at: state.vmm_profile_sync.updated_at.clone(),
        sections,
    })
}

/// Groups active VMM profile nodes by target and profile date.
/// 按目标作用域和画像日期分组 active VMM 画像节点。
fn static_vmm_profile_sections(
    nodes_by_id: &BTreeMap<u64, vulcan_vmm::ProfileNodeEntry>,
) -> Vec<VmmStaticProfileSectionSummary> {
    static_vmm_profile_targets()
        .into_iter()
        .filter_map(|target| static_vmm_profile_section(nodes_by_id, target))
        .collect()
}

/// Builds one static VMM profile section for a target scope.
/// 为单个目标作用域构建一个静态 VMM 画像段落。
fn static_vmm_profile_section(
    nodes_by_id: &BTreeMap<u64, vulcan_vmm::ProfileNodeEntry>,
    target: vulcan_vmm::ProfileTarget,
) -> Option<VmmStaticProfileSectionSummary> {
    let mut grouped: BTreeMap<String, Vec<&vulcan_vmm::ProfileNodeEntry>> = BTreeMap::new();
    for node in nodes_by_id.values().filter(|node| node.target == target) {
        if node.content.trim().is_empty() {
            continue;
        }
        grouped
            .entry(static_vmm_profile_date(node))
            .or_default()
            .push(node);
    }
    if grouped.is_empty() {
        return None;
    }
    let dates = grouped
        .into_iter()
        .map(|(date, mut nodes)| {
            nodes.sort_by_key(|node| node.profile_node_id);
            VmmStaticProfileDateGroupSummary {
                date,
                nodes: nodes
                    .into_iter()
                    .map(static_vmm_profile_node_summary)
                    .collect(),
            }
        })
        .collect();
    Some(VmmStaticProfileSectionSummary {
        target: static_vmm_profile_target_key(target).to_string(),
        label: static_vmm_profile_target_label(target).to_string(),
        dates,
    })
}

/// Converts one active VMM node into a WebUI profile node summary.
/// 将单个 active VMM 节点转换为 WebUI 画像节点摘要。
fn static_vmm_profile_node_summary(
    node: &vulcan_vmm::ProfileNodeEntry,
) -> VmmStaticProfileNodeSummary {
    VmmStaticProfileNodeSummary {
        id: node.profile_node_id,
        priority: static_vmm_profile_marker("P", &node.priority),
        level: static_vmm_profile_marker("L", &node.level),
        refresh_weight: format!("W{}", node.refresh_weight),
        content: node.content.trim().to_string(),
        source_kind: static_vmm_profile_source_kind(node.source_kind).to_string(),
        source_id: node.source_id,
        bind_id: node.bind_id,
    }
}

/// Returns VMM profile targets in stable display order.
/// 按稳定展示顺序返回 VMM 画像目标。
fn static_vmm_profile_targets() -> [vulcan_vmm::ProfileTarget; 4] {
    [
        vulcan_vmm::ProfileTarget::ProfileTargetTeam,
        vulcan_vmm::ProfileTarget::ProfileTargetSpace,
        vulcan_vmm::ProfileTarget::ProfileTargetProject,
        vulcan_vmm::ProfileTarget::ProfileTargetUser,
    ]
}

/// Returns one compact target key for WebUI state.
/// 返回一个用于 WebUI 状态的紧凑目标键。
fn static_vmm_profile_target_key(target: vulcan_vmm::ProfileTarget) -> &'static str {
    match target {
        vulcan_vmm::ProfileTarget::ProfileTargetTeam => "team",
        vulcan_vmm::ProfileTarget::ProfileTargetSpace => "space",
        vulcan_vmm::ProfileTarget::ProfileTargetProject => "project",
        vulcan_vmm::ProfileTarget::ProfileTargetUser => "user",
        vulcan_vmm::ProfileTarget::ProfileTargetAll => "all",
        vulcan_vmm::ProfileTarget::ProfileTargetUnspecified => "unspecified",
    }
}

/// Returns the visible target label for one VMM profile scope.
/// 返回单个 VMM 画像作用域的可见标签。
fn static_vmm_profile_target_label(target: vulcan_vmm::ProfileTarget) -> &'static str {
    match target {
        vulcan_vmm::ProfileTarget::ProfileTargetTeam => "TEAM",
        vulcan_vmm::ProfileTarget::ProfileTargetSpace => "SPACE",
        vulcan_vmm::ProfileTarget::ProfileTargetProject => "PROJECT",
        vulcan_vmm::ProfileTarget::ProfileTargetUser => "USER",
        vulcan_vmm::ProfileTarget::ProfileTargetAll => "ALL",
        vulcan_vmm::ProfileTarget::ProfileTargetUnspecified => "UNSPECIFIED",
    }
}

/// Normalizes a VMM profile marker such as priority or lifetime.
/// 规范化 VMM 画像优先级或生命周期标记。
fn static_vmm_profile_marker(prefix: &str, value: &str) -> String {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() {
        return format!("{prefix}?");
    }
    if value.starts_with(prefix) {
        value
    } else {
        format!("{prefix}{value}")
    }
}

/// Returns the profile date label for one VMM profile node.
/// 返回单个 VMM 画像节点的画像日期标签。
fn static_vmm_profile_date(node: &vulcan_vmm::ProfileNodeEntry) -> String {
    let date = node.profile_date.trim();
    if date.is_empty() {
        String::from("unknown-date")
    } else {
        date.to_string()
    }
}

/// Converts a VMM profile source kind into a stable API key.
/// 将 VMM 画像来源类型转换为稳定 API 键。
fn static_vmm_profile_source_kind(kind: vulcan_vmm::ProfileNodeSourceKind) -> &'static str {
    match kind {
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindTurnExtract => "turn_extract",
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindManualInstruction => {
            "manual_instruction"
        }
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindSystemSeed => "system_seed",
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindRetainedAfterUserDelete => {
            "retained_after_user_delete"
        }
        vulcan_vmm::ProfileNodeSourceKind::ProfileNodeSourceKindUnspecified => "unspecified",
    }
}

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
pub(super) fn vmm_memory_snapshot(
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
        .map(|project| load_project_configuration(&project.path).vmm)
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
        ),
        project_draft: vmm_config_draft_summary(
            &project_layer,
            &project_effective,
            !project_layer.is_empty(),
        ),
        project_scopes: workspace
            .task_projects
            .iter()
            .map(|project| {
                let project_vmm = load_project_configuration(&project.path).vmm;
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
fn vmm_effective_summary(effective: &vulcan_vmm::VmmEffectiveConfig) -> VmmEffectiveSummary {
    let grpc_configured = !effective.grpc_target.trim().is_empty();
    VmmEffectiveSummary {
        enabled: effective.enabled,
        grpc_target: effective.grpc_target.clone(),
        grpc_configured,
        user_id: effective.user_id,
        project_id: effective.project_id,
        diagnostic: vmm_diagnostic_key(effective.diagnose_memory_flow(grpc_configured)),
        precheck_diagnostic: vmm_diagnostic_key(effective.diagnose_precheck_flow(grpc_configured)),
        ai_tools_enabled: effective.ai_tools_enabled,
        precheck_enabled: effective.precheck_enabled,
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
fn vmm_config_draft_summary(
    layer: &vulcan_vmm::VmmConfigLayer,
    effective: &vulcan_vmm::VmmEffectiveConfig,
    has_override: bool,
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

/// Summarizes selected-session VMM runtime state.
/// 汇总当前选中会话的 VMM 运行态。
fn vmm_runtime_summary(runtime_state: Option<&SessionRuntimeStateEntry>) -> VmmRuntimeSummary {
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
fn vmm_management_sections() -> Vec<VmmManagementSectionSummary> {
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
fn vmm_memory_notice(
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
fn vmm_diagnostic_key(diagnostic: vulcan_vmm::VmmConfigDiagnostic) -> String {
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
    use serde_json::json;
    use vulcan_service_runtime::vmm_management::VmmWorkbenchAction;
    use vulcan_vmm::{ProfileNodeEntry, ProfileNodeSourceKind, ProfileTarget};

    /// Builds a default VMM action request for parser unit tests.
    /// 为解析器单元测试构建默认 VMM 动作请求。
    fn test_vmm_action_request(action: &str, payload: Value) -> VmmMemoryActionRequest {
        VmmMemoryActionRequest {
            action: action.to_string(),
            scope: String::from("global"),
            project_id: None,
            payload,
        }
    }

    /// Verifies write-memory action parsing keeps the WebUI payload mapping stable.
    /// 验证写入记忆动作解析保持 WebUI 载荷映射稳定。
    #[test]
    fn parse_vmm_memory_action_maps_write_memories_payload() {
        let request = test_vmm_action_request(
            "write_memories",
            json!({
                "sessionId": "session-a",
                "scopeLevel": 3,
                "abstractText": "short",
                "details": "long",
                "category": 4,
                "priority": 5,
                "memoryLevel": 6
            }),
        );

        let action = parse_vmm_memory_action(&request).expect("write action should parse");

        match action {
            VmmWorkbenchAction::WriteMemories { session_id, item } => {
                assert_eq!(session_id, "session-a");
                assert_eq!(item.scope_level, 3);
                assert_eq!(item.abstract_text, "short");
                assert_eq!(item.details, "long");
                assert_eq!(item.category, 4);
                assert_eq!(item.priority, 5);
                assert_eq!(item.memory_level, 6);
            }
            other => panic!("unexpected VMM workbench action: {other:?}"),
        }
    }

    /// Verifies VMM action parsing covers profile, search, delete and unknown-action branches.
    /// 验证 VMM 动作解析覆盖画像、搜索、删除和未知动作分支。
    #[test]
    fn parse_vmm_memory_action_covers_common_management_branches() {
        let profile_request = test_vmm_action_request("load_profile_nodes", json!({}));
        let search_request =
            test_vmm_action_request("search_memory_events", json!({ "queries": "alpha,beta" }));
        let delete_request = test_vmm_action_request(
            "delete_memories",
            json!({ "memoryIds": [7, "11"], "reason": "stale" }),
        );

        match parse_vmm_memory_action(&profile_request).expect("profile action should parse") {
            VmmWorkbenchAction::LoadProfileNodes { target } => {
                assert_eq!(target, ProfileTarget::ProfileTargetUser);
            }
            other => panic!("unexpected profile action: {other:?}"),
        }
        match parse_vmm_memory_action(&search_request).expect("search action should parse") {
            VmmWorkbenchAction::SearchMemoryEvents { queries, top_k } => {
                assert_eq!(queries, vec![String::from("alpha"), String::from("beta")]);
                assert_eq!(top_k, 5);
            }
            other => panic!("unexpected search action: {other:?}"),
        }
        match parse_vmm_memory_action(&delete_request).expect("delete action should parse") {
            VmmWorkbenchAction::DeleteMemories { memory_ids, reason } => {
                assert_eq!(memory_ids, vec![7, 11]);
                assert_eq!(reason, "stale");
            }
            other => panic!("unexpected delete action: {other:?}"),
        }
        let error = parse_vmm_memory_action(&test_vmm_action_request("unknown", json!({})))
            .expect_err("unknown action should fail");
        assert!(error.contains("不支持的 VMM 记忆动作"));
    }

    /// Verifies draft conversion parses transport and behavior fields without silent drift.
    /// 验证草稿转换会稳定解析传输字段与行为字段，避免静默漂移。
    #[test]
    fn vmm_config_layer_from_draft_parses_transport_and_behavior_fields() {
        let draft = VmmConfigDraftSummary {
            has_override: true,
            enabled: true,
            grpc_target: String::from(" http://127.0.0.1:50051 "),
            grpc_api_key: String::from(" secret "),
            user_id: String::from("7"),
            project_id: String::from("11"),
            receive_timeout_ms: String::from("1200"),
            handshake_timeout_ms: String::from("300"),
            max_receive_message_bytes: String::from("4096"),
            grpc_keepalive_time_ms: String::from("5000"),
            grpc_keepalive_timeout_ms: String::from("6000"),
            grpc_keepalive_permit_without_calls: true,
            implicit_memory_turns: String::from("9"),
            profile_refresh_turns: String::from("13"),
            profile_node_limit: String::from("17"),
            ai_tools_enabled: true,
            precheck_enabled: false,
            injection_tag: String::from("VMM_TAG"),
            injection_description: String::from("Visible memories"),
        };

        let layer =
            vmm_config_layer_from_draft(&draft, true).expect("draft should convert to VMM layer");

        assert_eq!(layer.enabled, Some(true));
        assert_eq!(layer.grpc_target.as_deref(), Some("http://127.0.0.1:50051"));
        assert_eq!(layer.grpc_api_key.as_deref(), Some("secret"));
        assert_eq!(layer.user_id, Some(7));
        assert_eq!(layer.project_id, Some(11));
        assert_eq!(layer.receive_timeout_ms, Some(1200));
        assert_eq!(layer.handshake_timeout_ms, Some(300));
        assert_eq!(layer.max_receive_message_bytes, Some(4096));
        assert_eq!(layer.grpc_keepalive_time_ms, Some(5000));
        assert_eq!(layer.grpc_keepalive_timeout_ms, Some(6000));
        assert_eq!(layer.grpc_keepalive_permit_without_calls, Some(true));
        assert_eq!(layer.implicit_memory_turns, Some(9));
        assert_eq!(layer.profile_refresh_turns, Some(13));
        assert_eq!(layer.profile_node_limit, Some(17));
        assert_eq!(layer.ai_tools_enabled, Some(true));
        assert_eq!(layer.precheck_enabled, Some(false));
        assert_eq!(layer.injection_tag.as_deref(), Some("VMM_TAG"));
        assert_eq!(
            layer.injection_description.as_deref(),
            Some("Visible memories")
        );
    }

    /// Verifies project-layer conversion strips transport-only fields.
    /// 验证项目层转换会剥离仅属于传输层的字段。
    #[test]
    fn vmm_config_layer_from_draft_omits_transport_fields_for_project_layer() {
        let draft = VmmConfigDraftSummary {
            enabled: true,
            grpc_target: String::from("http://127.0.0.1:50051"),
            grpc_api_key: String::from("secret"),
            receive_timeout_ms: String::from("1200"),
            handshake_timeout_ms: String::from("300"),
            max_receive_message_bytes: String::from("4096"),
            grpc_keepalive_time_ms: String::from("5000"),
            grpc_keepalive_timeout_ms: String::from("6000"),
            grpc_keepalive_permit_without_calls: true,
            user_id: String::from("7"),
            project_id: String::from("11"),
            implicit_memory_turns: String::from("9"),
            profile_refresh_turns: String::from("13"),
            profile_node_limit: String::from("17"),
            precheck_enabled: true,
            ai_tools_enabled: true,
            injection_tag: String::from("PROJECT_TAG"),
            injection_description: String::from("Project memories"),
            ..Default::default()
        };

        let layer =
            vmm_config_layer_from_draft(&draft, false).expect("project draft should convert");

        assert_eq!(layer.grpc_target, None);
        assert_eq!(layer.grpc_api_key, None);
        assert_eq!(layer.receive_timeout_ms, None);
        assert_eq!(layer.handshake_timeout_ms, None);
        assert_eq!(layer.max_receive_message_bytes, None);
        assert_eq!(layer.grpc_keepalive_time_ms, None);
        assert_eq!(layer.grpc_keepalive_timeout_ms, None);
        assert_eq!(layer.grpc_keepalive_permit_without_calls, None);
        assert_eq!(layer.user_id, Some(7));
        assert_eq!(layer.project_id, Some(11));
        assert_eq!(layer.injection_tag.as_deref(), Some("PROJECT_TAG"));
    }

    /// Verifies static profile snapshots group active nodes by target and date.
    /// 验证静态画像快照按作用域与日期分组 active 节点。
    #[test]
    fn static_vmm_profile_snapshot_groups_nodes_by_target_and_date() {
        let mut state = SessionRuntimeStateEntry::default();
        state.vmm_profile_sync.updated_at = Some(String::from("2026-06-27T00:00:00Z"));
        state.vmm_profile_sync.nodes_by_id.insert(
            42,
            ProfileNodeEntry {
                profile_node_id: 42,
                target: ProfileTarget::ProfileTargetProject,
                bind_id: 11,
                content: String::from("  project fact  "),
                priority: String::from("1"),
                level: String::from("L2"),
                refresh_weight: 5,
                profile_date: String::from("2026-06-27"),
                source_kind: ProfileNodeSourceKind::ProfileNodeSourceKindManualInstruction,
                source_id: 9,
                ..Default::default()
            },
        );

        let snapshot = static_vmm_profile_snapshot(&state).expect("profile snapshot should exist");

        assert_eq!(snapshot.updated_at.as_deref(), Some("2026-06-27T00:00:00Z"));
        assert_eq!(snapshot.sections.len(), 1);
        assert_eq!(snapshot.sections[0].target, "project");
        assert_eq!(snapshot.sections[0].dates.len(), 1);
        assert_eq!(snapshot.sections[0].dates[0].date, "2026-06-27");
        assert_eq!(snapshot.sections[0].dates[0].nodes[0].id, 42);
        assert_eq!(
            snapshot.sections[0].dates[0].nodes[0].content,
            "project fact"
        );
        assert_eq!(
            snapshot.sections[0].dates[0].nodes[0].source_kind,
            "manual_instruction"
        );
    }
}
