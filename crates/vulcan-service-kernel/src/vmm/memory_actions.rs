//! VMM memory actions helpers.
//! VMM memory actions 辅助逻辑。

use super::*;

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
pub(crate) async fn service_run_vmm_memory_action(
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
pub(crate) async fn service_suppress_context_injection_item(
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
pub(crate) async fn service_save_vmm_memory_config(
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
pub(crate) fn vmm_json_error(error: serde_json::Error) -> String {
    format!("序列化 VMM 响应失败：{error}")
}
