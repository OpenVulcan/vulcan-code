//! VMM memory actions helpers.
//! VMM memory actions 辅助逻辑。

use super::*;
use crate::{
    commit_and_emit_project_settings_changed_delta,
    emit_context_injection_changed_delta_for_session, emit_workbench_event,
};
use vulcan_service_api::{ProjectSettingsDomain, ServiceEventEmitter};

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
    if request.action.trim() == "suppress_context_injection_item" {
        return service_suppress_context_injection_item(app, state, request).await;
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
        // session_id is resolved from the initiating request instead of mutable UI selection state.
        // session_id 从动作发起请求解析，而不是依赖可变的 UI 选中状态。
        let session_id = vmm_suppression_session_id(workspace, request.conversation_id.as_deref())?;
        // runtime_state is the exact session entry that owns the rendered context injection.
        // runtime_state 是拥有当前渲染上下文注入内容的准确会话条目。
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
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
        emit_context_injection_changed_delta_for_session(&app, &state, session_id)?;
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
            "conversationId": request.conversation_id,
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
                .map(|entry| format!("{}:{}", entry.user_id, entry.user_name))
                .unwrap_or_else(|| user.message.clone());
            let project_text = project
                .project
                .as_ref()
                .map(|entry| format!("{}:{}", entry.project_id, entry.display_path))
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
                .map(|entry| format!("{}:{}", entry.project_id, entry.display_path))
                .unwrap_or_else(|| response.message.clone());
            let summary = format!(
                "项目预检：{project_text}；exists={}；needs_confirm={}。",
                response.exists, response.needs_confirm
            );
            ("ensure_project", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::DeleteUser(response) => {
            let summary = format!(
                "删除用户：{}；requires_confirmation={}；deleted_users={}；deleted_memories={}；deleted_profiles={}。",
                response.message,
                response.requires_confirmation,
                response.deleted_users,
                response.deleted_memories,
                response.deleted_profiles
            );
            ("delete_user", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::DeleteProject(response) => {
            let summary = format!(
                "删除项目：{}；needs_confirm={}；deleted_projects={}；deleted_memories={}；deleted_profiles={}。",
                response.message,
                response.needs_confirm,
                response.deleted_projects,
                response.deleted_memories,
                response.deleted_profiles
            );
            ("delete_project", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::MigrateProject(response) => {
            let summary = format!(
                "迁移项目：{}；needs_confirm={}；migrated_sessions={}；migrated_memories={}；rebuilt_vectors={}。",
                response.message,
                response.needs_confirm,
                response.migrated_sessions,
                response.migrated_memories,
                response.rebuilt_vector_rows
            );
            ("migrate_project", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::ProfileNodes(response) => {
            let summary = format!("已读取 {} 个 active 画像节点。", response.nodes.len());
            ("profile_nodes", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::ProfileBundle(response) => {
            let summary = format!(
                "画像 bundle 已读取：combined={} 字符，team={}，space={}，project={}，user={}。",
                response.combined_text.chars().count(),
                response.team_profile.chars().count(),
                response.space_profile.chars().count(),
                response.project_profile.chars().count(),
                response.user_profile.chars().count()
            );
            ("profile_bundle", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::ProfileInstruction(response) => {
            let summary = format!(
                "画像指令已应用：instruction_id={}；accepted={}；retired={}。",
                response.instruction_id,
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
                .map(|entry| format!("{}:{}", entry.user_id, entry.user_name))
                .unwrap_or_else(|| response.message.clone());
            let summary = format!(
                "用户解析：{user_text}；created={}；exists={}。",
                response.created, response.exists
            );
            ("resolve_user", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::SearchMemoryEvents(response) => {
            let hit_count = response
                .results
                .iter()
                .map(|group| group.hits.len())
                .sum::<usize>();
            let summary = format!(
                "记忆检索完成：queries={}；hits={}；trace={}。",
                response.results.len(),
                hit_count,
                response.trace_id
            );
            (
                "search_memory_events",
                summary,
                vmm_response_value(response)?,
            )
        }
        VmmManagementOutcome::GetTurnDetails(response) => {
            let summary = format!(
                "Turn 详情读取完成：turns={}；trace={}。",
                response.turns.len(),
                response.trace_id
            );
            ("get_turn_details", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::WriteMemories(response) => {
            let summary = format!(
                "主动写入完成：items={}；trace={}。",
                response.items.len(),
                response.trace_id
            );
            ("write_memories", summary, vmm_response_value(response)?)
        }
        VmmManagementOutcome::DeleteMemories(response) => {
            let summary = format!(
                "记忆删除完成：deleted={}；not_found={}；vectors={}；trace={}。",
                response.deleted_memory_ids.len(),
                response.not_found_memory_ids.len(),
                response.deleted_vector_rows,
                response.trace_id
            );
            ("delete_memories", summary, vmm_response_value(response)?)
        }
    };
    Ok(VmmMemoryActionResult {
        action: action.to_string(),
        summary,
        kind: kind.to_string(),
        request,
        response,
    })
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
