//! VMM memory actions helpers.
//! VMM memory actions 辅助逻辑。

use super::*;
#[path = "session_list.rs"]
mod session_list;
use crate::{
    commit_and_emit_project_settings_changed_delta,
    emit_context_injection_changed_delta_for_session, emit_workbench_event,
    mutate_project_configuration, workspace_project_paths_from_state,
};
use std::collections::BTreeMap;
use std::sync::Arc;
use vulcan_service_api::{
    ProjectSettingsDomain, ServiceEventEmitter, VmmMemoryActionResponse, VmmNativeClearPreview,
    VmmNativeClearResult, VmmNativeMemoryLevelCounts, VmmNativeMemoryRecordSummary,
    VmmNativeMemoryRecordsSummary, VmmNativeMemorySessionSummary, VmmNativeMigrationPreview,
    VmmNativeMigrationRecordSummary, VmmNativeMigrationResult, VmmNativeProfileRecordSummary,
    VmmNativeProfileRecordsSummary, VmmNativeProfileStateCounts, VmmNativeProjectBindingSummary,
    VmmNativeProjectCatalogSummary, VmmNativeProjectContentCounts, VmmNativeProjectDeletePreview,
    VmmNativeProjectSummary, VmmNativeRecycleBatchSummary, VmmNativeRecycleBinSummary,
    VmmNativeResourceSettingsSummary, VmmNativeRetentionApplyResult,
    VmmNativeRetentionPolicySummary, VmmNativeRetentionStatusSummary, VmmNativeSessionRename,
    VmmNativeTimelineItemSummary, VmmNativeTurnRecordSummary, VmmNativeTurnStatusCounts,
    VmmNativeVectorRebuildPreview, VmmNativeVectorRebuildResult, VmmNativeVectorStatusSummary,
    VmmProjectDisplaySummary, VmmUserDisplaySummary,
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
    if request.action.trim() == "load_static_profile_for_panel" {
        return service_load_static_profile_for_panel(state, request).await;
    }
    if request.action.trim() == "load_dynamic_memory_for_panel" {
        return service_load_dynamic_memory_for_panel(state, request).await;
    }
    if request.action.trim() == "suppress_context_injection_item" {
        return service_suppress_context_injection_item(app, state, request).await;
    }
    if matches!(
        request.action.trim(),
        "native_project_catalog"
            | "native_memory_records"
            | "native_session_list"
            | "native_removal_impact"
            | "native_removal_preview"
            | "native_removal_recycle"
            | "native_session_archive_preview"
            | "native_session_archive"
            | "native_session_restart_preview"
            | "native_session_restart"
            | "native_profile_records"
            | "native_project_create_and_bind"
            | "native_project_bind_existing"
            | "native_project_set_access"
            | "native_project_unbind"
            | "native_project_replace_query_links"
            | "native_project_rename"
            | "native_project_delete_preview"
            | "native_project_delete"
            | "native_project_migration_preview"
            | "native_project_migrate"
            | "native_project_clear_preview"
            | "native_project_clear"
            | "native_resource_catalog"
            | "native_runtime_policy"
            | "native_host_project_policy"
            | "native_host_project_policy_update"
            | "native_runtime_policy_update"
            | "native_retrieval_policy_update"
            | "native_text_policy_update"
            | "native_profile_policy_update"
            | "native_retention_settings_update"
            | "native_project_retention"
            | "native_project_retention_update"
            | "native_project_profile_policy"
            | "native_project_profile_policy_update"
            | "native_resource_read"
            | "native_resource_write"
            | "native_resource_reset"
            | "native_resource_settings_update"
            | "native_pii_test"
            | "native_pii_formula_test"
            | "native_noise_test"
            | "native_retention_status"
            | "native_retention_preview"
            | "native_retention_run"
            | "native_lexical_status"
            | "native_lexical_rebuild"
            | "native_vector_status"
            | "native_vector_rebuild_preview"
            | "native_vector_rebuild"
            | "native_recycle_bin"
            | "native_recycle_records"
            | "native_recycle_restore"
            | "native_recycle_purge"
    ) {
        // Binding actions carry an exact host ID; unrelated maintenance never clears these caches.
        // 绑定动作携带准确宿主 ID；无关维护不得清除此类缓存。
        let binding_owner = matches!(
            request.action.trim(),
            "native_project_create_and_bind"
                | "native_project_bind_existing"
                | "native_project_set_access"
                | "native_project_unbind"
                | "native_project_replace_query_links"
        )
        .then(|| required_native_text(&request.payload, "vulcanProjectId"))
        .transpose()?;
        let result = service_run_native_vmm_maintenance_action(state.clone(), request).await?;
        if let Some(host_project_id) = binding_owner {
            // Resolve the same installed maintenance owner after the successful binding transaction.
            // 在绑定事务成功后解析同一个已安装维护归属方。
            let maintenance = state
                .lock()
                .map_err(|error| format!("原生项目关联已更新，但读取会话缓存运行态失败：{error}"))?
                .native_vmm_maintenance
                .clone()
                .ok_or_else(|| {
                    "原生项目关联已更新，但维护运行时已不可用，无法同步会话缓存。".to_owned()
                })?;
            for event in maintenance
                .reconcile_host_binding_caches(&state, &host_project_id)
                .await
                .map_err(|error| format!("原生项目关联已更新，但会话记忆缓存同步失败：{error}"))?
            {
                crate::settings_runtime_projection::emit_service_event_with_trace(
                    &app,
                    "native_vmm_binding_context_changed",
                    vulcan_service_api::ServiceEvent::WorkbenchDelta(Box::new(event)),
                );
            }
        }
        return Ok(result);
    }
    if request.action.trim().starts_with("management_") {
        return super::management_http::service_run_vmm_management_http_action(state, request)
            .await;
    }
    let (layer, management_user_ref, management_project_ref, host_project_id) =
        vmm_action_context(&state, &request)?;
    let action = parse_vmm_memory_action(&request)?;
    let request_preview = vmm_action_request_preview(&action)?;
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&layer, None, None);
    if matches!(
        &action,
        vulcan_service_runtime::vmm_management::VmmWorkbenchAction::LoadUsersAndProjects
            | vulcan_service_runtime::vmm_management::VmmWorkbenchAction::ResolveUserAndProject
            | vulcan_service_runtime::vmm_management::VmmWorkbenchAction::EnsureProjectPreview
            | vulcan_service_runtime::vmm_management::VmmWorkbenchAction::ResolveUser { .. }
            | vulcan_service_runtime::vmm_management::VmmWorkbenchAction::DeleteUser { .. }
            | vulcan_service_runtime::vmm_management::VmmWorkbenchAction::EnsureProject { .. }
            | vulcan_service_runtime::vmm_management::VmmWorkbenchAction::DeleteProject { .. }
            | vulcan_service_runtime::vmm_management::VmmWorkbenchAction::MigrateProject { .. }
    ) {
        ensure_external_action_backend(effective.backend_mode)?;
    }
    let is_health_probe = matches!(
        &action,
        vulcan_service_runtime::vmm_management::VmmWorkbenchAction::Healthz
    );
    let tracks_external_health =
        is_health_probe && effective.backend_mode == vulcan_vmm::VmmBackendMode::ExternalGrpc;
    let grpc_target = effective.grpc_target.clone();
    let outcome = match vulcan_service_runtime::vmm_management::run_vmm_workbench_action(
        action,
        layer,
        management_user_ref,
        management_project_ref,
        host_project_id,
    )
    .await
    {
        Ok(outcome) => {
            if tracks_external_health {
                record_external_vmm_health_success(&grpc_target);
            }
            outcome
        }
        Err(error) => {
            if tracks_external_health {
                record_external_vmm_health_failure(&grpc_target, &error);
            }
            if is_health_probe {
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

/// Loads current VMM profile nodes for one existing conversation without changing prompt state.
/// 为一个现有会话加载当前 VMM 画像节点，不改变提示词状态。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace and VMM runtime; `request` identifies the exact session.
/// `state` 持有当前工作区与 VMM 运行时；`request` 标识精确会话。
///
/// # Returns
/// 返回值
/// A read-only profile result, or an explicit ownership/read error.
/// 返回只读画像结果，或明确的归属及读取错误。
pub(super) async fn service_load_static_profile_for_panel(
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let session_id = request
        .conversation_id
        .as_deref()
        .ok_or_else(|| "Static profile read requires a conversation ID.".to_owned())?
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid static profile conversation ID: {error}"))?;
    let (hook_runtime, workspace_path, host_project_id) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock static profile workspace: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| "Static profile workspace is not ready.".to_owned())?;
        if !workspace
            .sessions
            .iter()
            .any(|session| session.id == session_id)
        {
            return Err("Static profile conversation is not in the current workspace.".to_owned());
        }
        let runtime = guard
            .agent_runtime
            .as_ref()
            .ok_or_else(|| "Static profile runtime is not ready.".to_owned())?;
        (
            runtime.vmm_hook_runtime.clone(),
            crate::workspace_path_for_session(workspace, session_id),
            crate::owning_project_id_for_session(workspace, session_id).unwrap_or_default(),
        )
    };
    let nodes = hook_runtime
        .read_static_profile_for_panel(session_id, &workspace_path, &host_project_id)
        .await?;
    let profile = nodes.and_then(|nodes| {
        let sections = static_vmm_profile_sections(&nodes);
        (!sections.is_empty()).then(|| VmmStaticProfileSummary {
            updated_at: Some(current_unix_millis_text()),
            sections,
        })
    });
    // profile_node_count reports actual grouped nodes without fabricating any absent profile.
    // profile_node_count 只统计实际分组节点，不虚构缺失画像。
    let profile_node_count = profile.as_ref().map_or(0, |profile| {
        profile
            .sections
            .iter()
            .flat_map(|section| &section.dates)
            .map(|date| date.nodes.len())
            .sum::<usize>()
    });
    Ok(VmmMemoryActionResult {
        action: request.action,
        summary: format!("已读取 {profile_node_count} 条当前有效静态画像。"),
        kind: String::from("static_profile"),
        request: json!({ "conversationId": session_id.to_string() }),
        response: json!(profile),
        data: VmmMemoryActionResponse::StaticProfile(profile),
    })
}

/// Reads the exact session's durable VMM leases without hydrating its prompt runtime.
/// 不水合提示词运行态，读取准确会话的长期 VMM 租约。
///
/// # Parameters
/// 参数
/// `state` owns the indexed V1 locator and controller registry.
/// `state` 持有索引化 V1 定位器与控制器注册表。
/// `request` identifies the selected conversation.
/// `request` 指定当前选中会话。
///
/// # Returns
/// 返回值
/// Active and explicitly expired resident cards from the authoritative controller.
/// 返回权威控制器中的有效租约卡片和明确标记的过期驻留卡片。
pub(super) async fn service_load_dynamic_memory_for_panel(
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let session_id = request
        .conversation_id
        .as_deref()
        .ok_or_else(|| "Dynamic memory read requires a conversation ID.".to_owned())?
        .parse::<crate::SessionId>()
        .map_err(|error| format!("Invalid dynamic memory conversation ID: {error}"))?;
    let (host_runtime, registry) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Failed to lock dynamic memory workspace: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| "Dynamic memory workspace is not ready.".to_owned())?;
        if !workspace
            .sessions
            .iter()
            .any(|session| session.id == session_id)
        {
            return Err("Dynamic memory conversation is not in the current workspace.".to_owned());
        }
        (
            guard
                .controller_runtime
                .clone()
                .ok_or_else(|| "Dynamic memory session storage is not ready.".to_owned())?,
            guard.vmm_controller_registry.clone(),
        )
    };
    let locator = vulcan_workspace_store::workspace_store::locate_session_v1_storage(
        &host_runtime,
        &session_id.to_string(),
    )
    .await
    .map_err(|error| format!("Failed to locate dynamic memory session: {error}"))?;
    let controller = registry
        .get_or_open(locator, session_id, &current_unix_millis_text())
        .map_err(String::from)?;
    let snapshot = controller.snapshot()?;
    let cards = crate::workbench_session_panels::context_injection_cards_to_summary(
        snapshot
            .active_panel_card()
            .into_iter()
            .chain(snapshot.expired_panel_card())
            .collect(),
    );
    Ok(VmmMemoryActionResult {
        action: request.action,
        summary: format!("已读取 {} 张当前会话 VMM 驻留记忆卡片。", cards.len()),
        kind: String::from("dynamic_memory_cards"),
        request: json!({ "conversationId": session_id.to_string() }),
        response: json!(cards),
        data: VmmMemoryActionResponse::DynamicMemoryCards(cards),
    })
}

/// Runs one host-owned native VMM project maintenance action.
/// 执行一次宿主持有的原生 VMM 项目维护动作。
async fn service_run_native_vmm_maintenance_action(
    state: SharedServiceBridgeState,
    request: VmmMemoryActionRequest,
) -> Result<VmmMemoryActionResult, String> {
    let layer = provider_configuration_from_state(&state)?.vmm;
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&layer, None, None);
    if effective.backend_mode != vulcan_vmm::VmmBackendMode::Native
        && request.action.trim() != "native_project_catalog"
    {
        return Err(String::from("原生 VMM 项目维护仅在内部原生模式下可用。"));
    }
    let maintenance = {
        let guard = state
            .lock()
            .map_err(|error| format!("锁定原生 VMM 维护运行时失败：{error}"))?;
        guard
            .native_vmm_maintenance
            .clone()
            .ok_or_else(|| String::from("原生 VMM 维护运行时尚未就绪。"))?
    };
    let action = request.action.trim();
    let request_preview = request.payload.clone();
    match action {
        "native_removal_preview" => {
            // Decode a closed source selection without accepting hidden profile deletion flags.
            // 解码封闭来源选择，不接受隐藏的画像删除标志。
            let query = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("记忆回收选择格式错误：{error}"))?;
            // A preview is read-only and freezes exact IDs rather than just counts.
            // 预览为只读操作，冻结准确 ID 而不只是数量。
            let preview = maintenance.preview_manual_removal(query).await?;
            native_action_result(
                action,
                "已准备回收预览，用户画像和项目画像均保留。".to_owned(),
                "native_removal_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeRemovalPreview(preview.clone()),
            )
        }
        "native_removal_recycle" => {
            // Confirmation carries no replacement target list or profile deletion option.
            // 确认不携带替换目标列表或画像删除选项。
            let confirmation: vulcan_service_api::VmmNativeRemovalConfirmation =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("记忆回收确认格式错误：{error}"))?;
            if !confirmation.confirmed {
                return Err("请先确认回收影响。".to_owned());
            }
            // The maintenance owner validates the token, storage version and effective policy.
            // 维护归属方校验令牌、存储版本及有效策略。
            let result = maintenance
                .execute_manual_removal(&request.command_id, &confirmation.preview_id)
                .await?;
            native_action_result(
                action,
                "所选原始记录已移入回收站，用户画像和项目画像均保留。".to_owned(),
                "native_removal",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeRemoval(result.clone()),
            )
        }
        "native_removal_impact" => {
            let query = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("记忆来源影响查询格式错误：{error}"))?;
            let impact = maintenance.inspect_removal(query).await?;
            native_action_result(
                action,
                "已检查所选来源的关联记录，未删除任何数据。".to_owned(),
                "native_removal_impact",
                request_preview,
                &impact,
                VmmMemoryActionResponse::NativeRemovalImpact(impact.clone()),
            )
        }
        "native_project_retention" => {
            let query: vulcan_service_api::VmmNativeProjectRetentionQuery =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("记忆项目保留策略查询格式错误：{error}"))?;
            let policy = maintenance
                .project_retention_summary(query.project_id)
                .await?;
            native_action_result(
                action,
                "已加载记忆项目保留规则与继承状态。".to_owned(),
                "native_project_retention",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeProjectRetention(Box::new(policy.clone())),
            )
        }
        "native_project_profile_policy" => {
            let query: vulcan_service_api::VmmNativeProjectProfilePolicyQuery =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("记忆项目画像规则查询格式错误：{error}"))?;
            let policy = maintenance
                .project_profile_policy_summary(query.project_id)
                .await?;
            native_action_result(
                action,
                "已加载记忆项目画像规则与继承状态。".to_owned(),
                "native_project_profile_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeProjectProfilePolicy(Box::new(policy.clone())),
            )
        }
        "native_project_profile_policy_update" => {
            let edit = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("记忆项目画像规则编辑格式错误：{error}"))?;
            let policy = maintenance.update_project_profile_policy(edit).await?;
            native_action_result(
                action,
                "记忆项目画像规则已保存，将用于新的画像处理命令。".to_owned(),
                "native_project_profile_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeProjectProfilePolicy(Box::new(policy.clone())),
            )
        }
        "native_project_retention_update" => {
            let edit = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("记忆项目保留策略编辑格式错误：{error}"))?;
            let policy = maintenance.update_project_retention(edit).await?;
            native_action_result(
                action,
                "记忆项目保留规则已保存，将用于下一轮清洗。".to_owned(),
                "native_project_retention",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeProjectRetention(Box::new(policy.clone())),
            )
        }
        "native_host_project_policy" => {
            let host_project_id = required_native_text(&request.payload, "hostProjectId")?;
            let policy = maintenance.host_project_policy(&host_project_id).await?;
            native_action_result(
                action,
                "已加载原生宿主项目策略及继承状态。".to_owned(),
                "native_host_project_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeHostProjectPolicy(Box::new(policy.clone())),
            )
        }
        "native_host_project_policy_update" => {
            let edit = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("原生宿主项目策略格式错误：{error}"))?;
            let policy = maintenance.update_host_project_policy(edit).await?;
            native_action_result(
                action,
                "原生宿主项目策略已保存，将用于新命令。".to_owned(),
                "native_host_project_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeHostProjectPolicy(Box::new(policy.clone())),
            )
        }
        "native_runtime_policy" => {
            let policy = maintenance.runtime_policy().await?;
            native_action_result(
                action,
                String::from("已加载原生 VMM 召回与提炼策略。"),
                "native_runtime_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeRuntimePolicy(Box::new(policy.clone())),
            )
        }
        "native_runtime_policy_update" => {
            // Strict decoding rejects unknown fields instead of accepting unsupported settings.
            // 严格解码拒绝未知字段，不接纳尚未支持的设置。
            let recall = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("原生 VMM 召回与提炼策略格式错误：{error}"))?;
            let policy = maintenance.update_runtime_policy(recall).await?;
            native_action_result(
                action,
                String::from("原生 VMM 召回与提炼策略已保存，将用于新命令。"),
                "native_runtime_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeRuntimePolicy(Box::new(policy.clone())),
            )
        }
        "native_retrieval_policy_update" => {
            // Strict decoding keeps host-owned score identities out of user requests.
            // 严格解码将宿主持有的评分身份排除在用户请求之外。
            let retrieval = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("原生 VMM 检索排序策略格式错误：{error}"))?;
            let policy = maintenance.update_retrieval_policy(retrieval).await?;
            native_action_result(
                action,
                String::from("原生 VMM 检索排序策略已保存，将用于新命令。"),
                "native_runtime_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeRuntimePolicy(Box::new(policy.clone())),
            )
        }
        "native_profile_policy_update" => {
            // Explicit typed groups keep user and project formulas separate; no scope ID is accepted here.
            // 显式强类型组保持用户与项目公式独立；此处不接受作用域 ID。
            let profiles = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("原生 VMM 画像规则格式错误：{error}"))?;
            let policy = maintenance.update_profile_policy(profiles).await?;
            native_action_result(
                action,
                String::from("原生 VMM 画像规则已保存，仅影响后续生成或修订的节点。"),
                "native_runtime_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeRuntimePolicy(Box::new(policy.clone())),
            )
        }
        "native_text_policy_update" => {
            // Only text reduction controls are editable; privacy and reasoning cleanup remain mandatory.
            // 仅允许编辑文本缩减控制项；隐私与思考内容清理保持强制。
            let text = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("原生 VMM 文本清洗策略格式错误：{error}"))?;
            let policy = maintenance.update_text_policy(text).await?;
            native_action_result(
                action,
                String::from("原生 VMM 文本清洗策略已保存，仅影响新入库片段。"),
                "native_runtime_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeRuntimePolicy(Box::new(policy.clone())),
            )
        }
        "native_retention_settings_update" => {
            // Only declared cleanup fields are accepted; Core batch limits remain host-owned.
            // 仅接受声明的清洗字段；Core 批次上限仍由宿主持有。
            let settings = serde_json::from_value(request.payload.clone())
                .map_err(|error| format!("原生 VMM 清洗设置格式错误：{error}"))?;
            let policy = maintenance.update_retention_settings(settings).await?;
            native_action_result(
                action,
                String::from("原生 VMM 清洗设置已保存，周期工作器将使用新配置。"),
                "native_runtime_policy",
                request_preview,
                &policy,
                VmmMemoryActionResponse::NativeRuntimePolicy(Box::new(policy.clone())),
            )
        }
        "native_resource_catalog" => {
            let catalog = maintenance.resource_catalog().await?;
            native_action_result(
                action,
                format!("已加载 {} 份原生 VMM 资源文件。", catalog.files.len()),
                "native_resource_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeResourceCatalog(catalog.clone()),
            )
        }
        "native_resource_read" => {
            let path = required_native_text(&request.payload, "path")?;
            let file = maintenance.resource_file(&path).await?;
            native_action_result(
                action,
                format!("已加载原生 VMM 资源 `{path}`。"),
                "native_resource_file",
                request_preview,
                &file,
                VmmMemoryActionResponse::NativeResourceFile(file.clone()),
            )
        }
        "native_resource_write" => {
            let path = required_native_text(&request.payload, "path")?;
            let content = required_native_text_verbatim(&request.payload, "content")?;
            let file = maintenance
                .write_resource_file(
                    &path,
                    content,
                    required_native_bool(&request.payload, "confirmedPurpose")?,
                    required_native_bool(&request.payload, "confirmedModification")?,
                )
                .await?;
            native_action_result(
                action,
                format!("原生 VMM 用户覆盖 `{path}` 已保存并立即激活。"),
                "native_resource_file",
                request_preview,
                &file,
                VmmMemoryActionResponse::NativeResourceFile(file.clone()),
            )
        }
        "native_resource_reset" => {
            let path = required_native_text(&request.payload, "path")?;
            let file = maintenance.reset_resource_file(&path).await?;
            native_action_result(
                action,
                format!("原生 VMM 资源 `{path}` 已恢复内置内容并立即激活。"),
                "native_resource_file",
                request_preview,
                &file,
                VmmMemoryActionResponse::NativeResourceFile(file.clone()),
            )
        }
        "native_resource_settings_update" => {
            let settings = VmmNativeResourceSettingsSummary {
                prompt_bundle: required_native_text(&request.payload, "promptBundle")?,
                pii_language: required_native_text(&request.payload, "piiLanguage")?,
                noise_language: required_native_text(&request.payload, "noiseLanguage")?,
                noise_enabled: required_native_bool(&request.payload, "noiseEnabled")?,
                noise_semantic_enabled: required_native_bool(
                    &request.payload,
                    "noiseSemanticEnabled",
                )?,
                noise_semantic_threshold: required_native_f64(
                    &request.payload,
                    "noiseSemanticThreshold",
                )?,
            };
            let catalog = maintenance.update_resource_settings(settings).await?;
            native_action_result(
                action,
                String::from("原生 VMM 资源组合设置已保存并立即激活。"),
                "native_resource_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeResourceCatalog(catalog.clone()),
            )
        }
        "native_pii_test" => {
            let text = required_native_text_verbatim(&request.payload, "text")?;
            let result = maintenance.test_pii(&text).await?;
            native_action_result(
                action,
                format!("PII 测试完成：命中 {} 条规则。", result.redactions.len()),
                "native_pii_test",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativePiiTest(result.clone()),
            )
        }
        "native_pii_formula_test" => {
            let text = required_native_text_verbatim(&request.payload, "text")?;
            let pattern = required_native_text_verbatim(&request.payload, "pattern")?;
            let condition = optional_native_text_verbatim(&request.payload, "condition")?;
            let replacement = required_native_string_verbatim(&request.payload, "replacement")?;
            let result = maintenance.test_pii_formula(
                &text,
                &pattern,
                condition.as_deref(),
                &replacement,
            )?;
            let accepted_count = result
                .matches
                .iter()
                .filter(|entry| entry.condition_accepted)
                .count();
            native_action_result(
                action,
                format!(
                    "PII 临时规则测试完成：{} 个正则候选，{accepted_count} 个通过条件公式。",
                    result.matches.len()
                ),
                "native_pii_formula_test",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativePiiFormulaTest(result.clone()),
            )
        }
        "native_noise_test" => {
            let text = required_native_text_verbatim(&request.payload, "text")?;
            let result = maintenance.test_noise(&text).await?;
            native_action_result(
                action,
                if result.is_noise {
                    format!(
                        "确定性噪音测试命中类别 `{}`。",
                        result.category.as_deref().unwrap_or("unknown")
                    )
                } else {
                    String::from("确定性噪音测试未命中正则规则。")
                },
                "native_noise_test",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeNoiseTest(result.clone()),
            )
        }
        "native_retention_status" => {
            let status = native_retention_status(maintenance.retention_status().await?);
            native_action_result(
                action,
                if status.running {
                    String::from("原生 VMM 定期清洗正在运行。")
                } else if status.enabled {
                    String::from("原生 VMM 定期清洗已启用。")
                } else {
                    String::from("原生 VMM 定期清洗当前未启用。")
                },
                "native_retention_status",
                request_preview,
                &status,
                VmmMemoryActionResponse::NativeRetentionStatus(status.clone()),
            )
        }
        "native_retention_preview" => {
            let preview = maintenance.preview_retention_cleanup().await?;
            native_action_result(
                action,
                String::from("已生成只读清洗预览，确认前不会更改记忆内容。"),
                "native_retention_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeRetentionPreview(preview.clone()),
            )
        }
        "native_retention_run" => {
            if !required_native_bool(&request.payload, "confirmed")? {
                return Err(String::from("请先查看清洗影响并明确确认。"));
            }
            let preview_id = required_native_text(&request.payload, "previewId")?;
            let result = native_retention_apply_result(
                maintenance
                    .execute_retention_cleanup(&request.command_id, &preview_id)
                    .await?,
            );
            native_action_result(
                action,
                format!(
                    "原生 VMM 清洗完成：收敛 {} 条到期画像，回收 {} 条记忆、{} 个回合，入队 {} 个并完成 {} 个冷回合作业，清除 {} 个到期批次。",
                    result.expired_profile_nodes,
                    result.recycled_memories,
                    result.recycled_turns,
                    result.enqueued_cold_jobs,
                    result.completed_cold_jobs,
                    result.purged_trash_batches
                ),
                "native_retention_run",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeRetentionRun(result.clone()),
            )
        }
        "native_lexical_status" => {
            let status = maintenance.lexical_index_status().await?;
            native_action_result(
                action,
                String::from("已读取原生全文索引计数，未修改任何内容。"),
                "native_lexical_status",
                request_preview,
                &status,
                VmmMemoryActionResponse::NativeLexicalStatus(status.clone()),
            )
        }
        "native_lexical_rebuild" => {
            let result = maintenance
                .rebuild_lexical_index(
                    required_native_storage_version(&request.payload, "expectedStorageVersion")?,
                    required_native_bool(&request.payload, "confirmed")?,
                )
                .await?;
            native_action_result(
                action,
                format!(
                    "已从当前记忆内容重建 {} 条全文索引，未重新生成向量。",
                    result.reindexed_memory_count
                ),
                "native_lexical_rebuild",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeLexicalRebuild(result.clone()),
            )
        }
        "native_vector_status" => {
            let status = native_vector_status(maintenance.vector_status().await?);
            native_action_result(
                action,
                if !status.configured {
                    String::from("原生 VMM Embedding 路由尚未就绪。")
                } else if status.rebuild_required_memory_count > 0 {
                    format!(
                        "当前 Embedding 代次需要重建 {} 条活动记忆。",
                        status.rebuild_required_memory_count
                    )
                } else {
                    String::from("原生 VMM 活动记忆向量均与当前 Embedding 代次一致。")
                },
                "native_vector_status",
                request_preview,
                &status,
                VmmMemoryActionResponse::NativeVectorStatus(status.clone()),
            )
        }
        "native_vector_rebuild_preview" => {
            let preview =
                native_vector_rebuild_preview(maintenance.preview_vector_rebuild().await?);
            native_action_result(
                action,
                format!(
                    "向量重建预检完成：{} 条活动记忆中有 {} 条需要重建，涉及 {} 个项目、{} 个批次。",
                    preview.active_memory_count,
                    preview.rebuild_memory_count,
                    preview.affected_project_count,
                    preview.batch_count
                ),
                "native_vector_rebuild_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeVectorRebuildPreview(preview.clone()),
            )
        }
        "native_vector_rebuild" => {
            let result = native_vector_rebuild_result(
                maintenance
                    .execute_vector_rebuild(
                        &request.command_id,
                        required_native_storage_version(
                            &request.payload,
                            "expectedStorageVersion",
                        )?,
                        &required_native_text(&request.payload, "expectedPreviewFingerprint")?,
                        required_native_bool(&request.payload, "confirmed")?,
                    )
                    .await?,
            );
            native_action_result(
                action,
                format!(
                    "原生 VMM 向量重建完成：已在 {} 个批次中重建 {} 条活动记忆。",
                    result.batch_count, result.rebuilt_memory_count
                ),
                "native_vector_rebuild",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeVectorRebuild(result.clone()),
            )
        }
        "native_recycle_bin" => {
            let project_id = vmm_payload_u64(&request.payload, "vmmProjectId")
                .filter(|project_id| *project_id > 0);
            let recycle_bin = native_recycle_bin(maintenance.recycle_bin(project_id).await?);
            native_action_result(
                action,
                format!("已加载 {} 个原生 VMM 回收批次。", recycle_bin.batches.len()),
                "native_recycle_bin",
                request_preview,
                &recycle_bin,
                VmmMemoryActionResponse::NativeRecycleBin(recycle_bin.clone()),
            )
        }
        "native_recycle_records" => {
            // Read the exact retained batch, not the active memory-records projection.
            // 读取准确保留批次，而非活动记忆记录投影。
            let query: vulcan_service_api::VmmNativeRecycleRecordQuery =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("回收明细查询参数无效：{error}"))?;
            let page = maintenance.recycle_records(query).await?;
            native_action_result(
                action,
                format!("已加载 {} 条回收明细。", page.records.len()),
                "native_recycle_records",
                request_preview,
                &page,
                VmmMemoryActionResponse::NativeRecycleRecords(page.clone()),
            )
        }
        "native_recycle_restore" => {
            let batch_id = required_native_text(&request.payload, "batchId")?;
            let storage_version =
                required_native_storage_version(&request.payload, "storageVersion")?;
            let batch = native_recycle_batch(
                maintenance
                    .restore_recycle_batch(&request.command_id, &batch_id, storage_version)
                    .await?,
            );
            native_action_result(
                action,
                format!(
                    "已恢复回收批次 `{batch_id}`：{} 个回合、{} 条记忆。",
                    batch.turn_count, batch.memory_count
                ),
                "native_recycle_batch",
                request_preview,
                &batch,
                VmmMemoryActionResponse::NativeRecycleBatch(batch.clone()),
            )
        }
        "native_recycle_purge" => {
            let batch_id = required_native_text(&request.payload, "batchId")?;
            let storage_version =
                required_native_storage_version(&request.payload, "storageVersion")?;
            let batch = native_recycle_batch(
                maintenance
                    .purge_recycle_batch(&request.command_id, &batch_id, storage_version)
                    .await?,
            );
            native_action_result(
                action,
                format!(
                    "已永久清除回收批次 `{batch_id}`：{} 个回合、{} 条记忆。",
                    batch.turn_count, batch.memory_count
                ),
                "native_recycle_batch",
                request_preview,
                &batch,
                VmmMemoryActionResponse::NativeRecycleBatch(batch.clone()),
            )
        }
        "native_project_catalog" => {
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                format!(
                    "已加载 {} 个原生 VMM 项目和 {} 条 VulcanCode 项目绑定。",
                    catalog.projects.len(),
                    catalog.bindings.len()
                ),
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_session_archive_preview" => {
            // Decode a closed selection schema; do not guess field aliases or session ownership.
            // 解码封闭选择结构，不猜测字段别名或会话归属。
            let selection: vulcan_service_api::VmmNativeSessionArchiveSelection =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("归档选择参数无效：{error}"))?;
            let preview = maintenance.preview_session_archive(selection).await?;
            native_action_result(
                action,
                "已生成归档预览；此操作只改变管理分类，不删除内容或关闭记忆。".to_owned(),
                "native_session_archive_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeSessionArchivePreview(preview.clone()),
            )
        }
        "native_session_archive" => {
            // Confirmation carries only the server preview identity and explicit user approval.
            // 确认只携带服务端预览身份及用户明确同意。
            let confirmation: vulcan_service_api::VmmNativeSessionArchiveConfirmation =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("归档确认参数无效：{error}"))?;
            if !confirmation.confirmed {
                return Err("请先查看归档预览并明确确认。".to_owned());
            }
            let result = maintenance
                .execute_session_archive(&request.command_id, &confirmation.preview_id)
                .await?;
            native_action_result(
                action,
                format!(
                    "已{} {} 个记忆会话；内容与记忆功能保持不变。",
                    if result.selection.archived {
                        "归档"
                    } else {
                        "取消归档"
                    },
                    result.selection.session_source_ids.len()
                ),
                "native_session_archive",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeSessionArchive(result.clone()),
            )
        }
        "native_session_restart_preview" => {
            // Decode only exact owner/session identities; restart never accepts replacement content.
            // 只解码准确归属及会话身份，重新启用不接受替代内容。
            let selection: vulcan_service_api::VmmNativeSessionRestartSelection =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("重新启用选择参数无效：{error}"))?;
            let preview = maintenance.preview_session_restart(selection).await?;
            native_action_result(
                action,
                "已生成重新启用预览；不会恢复已删除内容或重放历史对话。".to_owned(),
                "native_session_restart_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeSessionRestartPreview(preview.clone()),
            )
        }
        "native_session_restart" => {
            // Explicit confirmation binds the command to an immutable server preview.
            // 明确确认将命令绑定到不可变服务端预览。
            let confirmation: vulcan_service_api::VmmNativeSessionRestartConfirmation =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("重新启用确认参数无效：{error}"))?;
            if !confirmation.confirmed {
                return Err("请先查看重新启用预览并明确确认。".to_owned());
            }
            let result = maintenance
                .execute_session_restart(&request.command_id, &confirmation.preview_id)
                .await?;
            native_action_result(
                action,
                format!(
                    "已重新启用 {} 个记忆会话；仅后续正常交互使用记忆，不恢复已删除内容。",
                    result.selection.session_source_ids.len()
                ),
                "native_session_restart",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeSessionRestart(result.clone()),
            )
        }
        "native_session_list" => {
            let query: vulcan_service_api::VmmNativeSessionListQuery =
                serde_json::from_value(request.payload.clone())
                    .map_err(|error| format!("会话列表参数无效：{error}"))?;
            session_list::validate(&query)?;
            let (version, sessions) = maintenance.session_summaries(query.vmm_project_id).await?;
            let titles = native_session_titles(&state)?;
            let page = session_list::paginate(
                query,
                version,
                sessions
                    .into_iter()
                    .map(|session| session_list::project_session_summary(session, &titles))
                    .collect(),
            )?;
            native_action_result(
                action,
                format!(
                    "已加载 {} 个会话，共匹配 {} 个。",
                    page.sessions.len(),
                    page.total
                ),
                "native_session_list",
                request_preview,
                &page,
                VmmMemoryActionResponse::NativeSessionList(page.clone()),
            )
        }
        "native_memory_records" => {
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let session_source_id = vmm_payload_text(&request.payload, "sessionSourceId")
                .map(|value| value.trim().to_owned())
                .filter(|value| !value.is_empty());
            let records = native_memory_records(
                maintenance
                    .memory_records(project_id, session_source_id.as_deref())
                    .await?,
                &native_session_titles(&state)?,
            );
            native_action_result(
                action,
                match records.selected_session_source_id.as_deref() {
                    Some(session_id) => format!(
                        "已加载原生 VMM 项目 {project_id} 的会话 `{session_id}`：{} 个回合、{} 条记忆。",
                        records.turns.len(),
                        records.memories.len()
                    ),
                    None => format!(
                        "已加载原生 VMM 项目 {project_id}：{} 个会话、{} 个回合、{} 条记忆。",
                        records.sessions.len(),
                        records.turns.len(),
                        records.memories.len()
                    ),
                },
                "native_memory_records",
                request_preview,
                &records,
                VmmMemoryActionResponse::NativeMemoryRecords(records.clone()),
            )
        }
        "native_profile_records" => {
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let target = required_native_profile_target(&request.payload)?;
            let records =
                native_profile_records(maintenance.profile_records(project_id, target).await?);
            native_action_result(
                action,
                format!(
                    "已加载原生 VMM {}画像：{} 个活动，{} 个待处理，{} 个非活动，{} 个过期，{} 个待生效。",
                    if records.target == "user" {
                        "用户"
                    } else {
                        "项目"
                    },
                    records.counts.active,
                    records.counts.pending,
                    records.counts.inactive,
                    records.counts.expired,
                    records.counts.scheduled,
                ),
                "native_profile_records",
                request_preview,
                &records,
                VmmMemoryActionResponse::NativeProfileRecords(records.clone()),
            )
        }
        "native_project_create_and_bind" => {
            let vulcan_project_id = required_native_text(&request.payload, "vulcanProjectId")?;
            let project_name = required_native_text(&request.payload, "projectName")?;
            let project = maintenance
                .create_and_bind_project(&vulcan_project_id, &project_name)
                .await?;
            crate::vmm_hooks::reconcile_vmm_writeback_mirrors(
                &mut *state
                    .lock()
                    .map_err(|error| format!("锁定 VMM 写回镜像失败：{error}"))?,
            )?;
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                format!(
                    "VulcanCode 项目 `{vulcan_project_id}` 已绑定到原生 VMM 项目 `{}`。",
                    project.name
                ),
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_project_bind_existing" => {
            let vulcan_project_id = required_native_text(&request.payload, "vulcanProjectId")?;
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let project = maintenance
                .bind_existing_project(&vulcan_project_id, project_id)
                .await?;
            crate::vmm_hooks::reconcile_vmm_writeback_mirrors(
                &mut *state
                    .lock()
                    .map_err(|error| format!("锁定 VMM 写回镜像失败：{error}"))?,
            )?;
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                format!(
                    "VulcanCode 项目 `{vulcan_project_id}` 已绑定到原生 VMM 项目 `{}`。",
                    project.name
                ),
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_project_set_access" => {
            let vulcan_project_id = required_native_text(&request.payload, "vulcanProjectId")?;
            let owner_project_id = required_native_project_id(&request.payload, "ownerProjectId")?;
            let linked_project_ids = vmm_payload_u64_list(&request.payload, "linkedProjectIds")?;
            let access = maintenance
                .set_project_access(&vulcan_project_id, owner_project_id, &linked_project_ids)
                .await?;
            crate::vmm_hooks::reconcile_vmm_writeback_mirrors(
                &mut *state
                    .lock()
                    .map_err(|error| format!("锁定 VMM 写回镜像失败：{error}"))?,
            )?;
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                format!(
                    "VulcanCode 项目 `{vulcan_project_id}` 已绑定到原生 VMM 项目 `{}`，并配置 {} 个连带查询项目。",
                    access.owner.name,
                    access.linked.len()
                ),
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_project_unbind" => {
            let vulcan_project_id = required_native_text(&request.payload, "vulcanProjectId")?;
            maintenance.unbind_project(&vulcan_project_id).await?;
            crate::vmm_hooks::reconcile_vmm_writeback_mirrors(
                &mut *state
                    .lock()
                    .map_err(|error| format!("锁定 VMM 写回镜像失败：{error}"))?,
            )?;
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                format!("VulcanCode 项目 `{vulcan_project_id}` 已解除原生 VMM 绑定。"),
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_project_replace_query_links" => {
            let vulcan_project_id = required_native_text(&request.payload, "vulcanProjectId")?;
            let linked_project_ids = vmm_payload_u64_list(&request.payload, "linkedProjectIds")?;
            maintenance
                .replace_query_links(&vulcan_project_id, &linked_project_ids)
                .await?;
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                format!(
                    "VulcanCode 项目 `{vulcan_project_id}` 的连带查询项目已更新为 {} 个。",
                    linked_project_ids.len()
                ),
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_project_rename" => {
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let name = required_native_text(&request.payload, "name")?;
            let project = maintenance.rename_project(project_id, &name).await?;
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                format!(
                    "原生 VMM 项目 {} 已改名为 `{}`，现有 ID 绑定保持不变。",
                    project.id, project.name
                ),
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_project_delete_preview" => {
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let preview = native_project_delete_preview(
                maintenance.preview_project_delete(project_id).await?,
            );
            native_action_result(
                action,
                format!(
                    "删除预检完成：{} 个归属项目、{} 个连带查询项目、{} 条记忆将受影响。",
                    preview.project.owner_vulcan_project_ids.len(),
                    preview.project.linked_vulcan_project_ids.len(),
                    preview.content.memories
                ),
                "native_project_delete_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeProjectDeletePreview(preview.clone()),
            )
        }
        "native_project_delete" => {
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let expected_storage_version =
                required_native_storage_version(&request.payload, "expectedStorageVersion")?;
            let deleted = maintenance
                .delete_project(
                    &request.command_id,
                    project_id,
                    expected_storage_version,
                    vmm_payload_bool(&request.payload, "confirmDetachBindings"),
                )
                .await?;
            let disable_failures = disable_deleted_native_project_owners(
                &state,
                &deleted.usage.owner_vulcan_project_ids,
            )
            .await;
            let catalog = native_project_catalog(maintenance.project_catalog().await?);
            native_action_result(
                action,
                if disable_failures.is_empty() {
                    format!(
                        "原生 VMM 项目 `{}` 已删除；{} 个 VulcanCode 项目的 VMM 已关闭。",
                        deleted.project.name,
                        deleted.usage.owner_vulcan_project_ids.len()
                    )
                } else {
                    format!(
                        "原生 VMM 项目 `{}` 已删除且绑定已解除；项目配置关闭写回失败：[{}]，这些项目仍因无绑定而无法使用 VMM。",
                        deleted.project.name,
                        disable_failures.join(", ")
                    )
                },
                "native_project_catalog",
                request_preview,
                &catalog,
                VmmMemoryActionResponse::NativeProjectCatalog(catalog.clone()),
            )
        }
        "native_project_migration_preview" => {
            let source_project_id =
                required_native_project_id(&request.payload, "sourceProjectId")?;
            let target_project_id =
                required_native_project_id(&request.payload, "targetProjectId")?;
            let preview = native_migration_preview(
                maintenance
                    .preview_migration(source_project_id, target_project_id)
                    .await?,
            );
            native_action_result(
                action,
                format!(
                    "迁移预检完成：来源 {} 个会话、{} 条记忆；目标迁移后 {} 个会话、{} 条记忆；会话冲突 {} 个。",
                    preview.source.sessions,
                    preview.source.memories,
                    preview.target_after.sessions,
                    preview.target_after.memories,
                    preview.session_renames.len()
                ),
                "native_migration_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeMigrationPreview(preview.clone()),
            )
        }
        "native_project_migrate" => {
            let source_project_id =
                required_native_project_id(&request.payload, "sourceProjectId")?;
            let target_project_id =
                required_native_project_id(&request.payload, "targetProjectId")?;
            let expected_storage_version =
                required_native_storage_version(&request.payload, "expectedStorageVersion")?;
            let result = native_migration_result(
                maintenance
                    .execute_migration(
                        &request.command_id,
                        source_project_id,
                        target_project_id,
                        expected_storage_version,
                        &required_native_text(&request.payload, "expectedPreviewFingerprint")?,
                        vmm_payload_bool(&request.payload, "acceptSessionRenames"),
                    )
                    .await?,
            );
            native_action_result(
                action,
                format!(
                    "项目记忆迁移完成：迁移 {} 个会话、{} 个回合、{} 条记忆和 {} 个项目画像节点。",
                    result.moved.sessions,
                    result.moved.turns,
                    result.moved.memories,
                    result.moved.profile_nodes
                ),
                "native_migration",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeMigration(result.clone()),
            )
        }
        "native_project_clear_preview" => {
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let preview = native_clear_preview(maintenance.preview_clear(project_id).await?);
            native_action_result(
                action,
                format!(
                    "清空预检完成：将删除 {} 个会话、{} 个回合、{} 条记忆和 {} 个项目画像节点。",
                    preview.removed.sessions,
                    preview.removed.turns,
                    preview.removed.memories,
                    preview.removed.profile_nodes
                ),
                "native_clear_preview",
                request_preview,
                &preview,
                VmmMemoryActionResponse::NativeClearPreview(preview.clone()),
            )
        }
        "native_project_clear" => {
            let project_id = required_native_project_id(&request.payload, "vmmProjectId")?;
            let expected_storage_version =
                required_native_storage_version(&request.payload, "expectedStorageVersion")?;
            let result = native_clear_result(
                maintenance
                    .execute_clear(&request.command_id, project_id, expected_storage_version)
                    .await?,
            );
            native_action_result(
                action,
                format!(
                    "项目记忆已清空：删除 {} 个会话、{} 个回合、{} 条记忆和 {} 个项目画像节点。",
                    result.removed.sessions,
                    result.removed.turns,
                    result.removed.memories,
                    result.removed.profile_nodes
                ),
                "native_clear",
                request_preview,
                &result,
                VmmMemoryActionResponse::NativeClear(result.clone()),
            )
        }
        _ => Err(format!(
            "unsupported native VMM maintenance action `{action}`"
        )),
    }
}

/// Requires one positive native VMM numeric identifier from an action payload.
/// 从动作载荷中读取一个必需的正数原生 VMM 标识符。
fn required_native_project_id(payload: &Value, key: &str) -> Result<u64, String> {
    vmm_payload_u64(payload, key)
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("原生 VMM 动作缺少有效字段 `{key}`。"))
}

/// Requires one trimmed non-empty native VMM action text field.
/// 读取一个必需的、裁剪后非空的原生 VMM 动作文本字段。
fn required_native_text(payload: &Value, key: &str) -> Result<String, String> {
    vmm_payload_text(payload, key)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("原生 VMM 动作缺少有效字段 `{key}`。"))
}

/// Requires one closed native profile target token from an action payload.
/// 从动作载荷中读取一个封闭的原生画像目标标记。
fn required_native_profile_target(
    payload: &Value,
) -> Result<crate::native_vmm::NativeVmmProfileTarget, String> {
    match required_native_text(payload, "target")?.as_str() {
        "user" => Ok(crate::native_vmm::NativeVmmProfileTarget::User),
        "project" => Ok(crate::native_vmm::NativeVmmProfileTarget::Project),
        target => Err(format!(
            "原生 VMM 画像目标 `{target}` 无效；只允许 `user` 或 `project`。"
        )),
    }
}

/// Requires one non-empty native VMM action text field without trimming its content.
/// 读取一个必需的非空原生 VMM 动作文本字段，同时不裁剪其内容。
fn required_native_text_verbatim(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("原生 VMM 动作缺少有效字段 `{key}`。"))
}

/// Requires one native VMM string field verbatim while allowing an intentional empty value.
/// 逐字读取一个必需的原生 VMM 字符串字段，同时允许有意传入空值。
fn required_native_string_verbatim(payload: &Value, key: &str) -> Result<String, String> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("原生 VMM 动作缺少字符串字段 `{key}`。"))
}

/// Reads one optional native VMM string field verbatim and normalizes blank content to absence.
/// 逐字读取一个可选的原生 VMM 字符串字段，并将空白内容归一化为缺失。
fn optional_native_text_verbatim(payload: &Value, key: &str) -> Result<Option<String>, String> {
    let Some(value) = payload.as_object().and_then(|object| object.get(key)) else {
        return Ok(None);
    };
    let value = value
        .as_str()
        .ok_or_else(|| format!("原生 VMM 动作字段 `{key}` 必须是字符串。"))?;
    Ok((!value.trim().is_empty()).then(|| value.to_owned()))
}

/// Requires one explicit native VMM boolean field and preserves false distinctly from absence.
/// 读取一个明确的原生 VMM 布尔字段，并区分 false 与缺失。
fn required_native_bool(payload: &Value, key: &str) -> Result<bool, String> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("原生 VMM 动作缺少有效布尔字段 `{key}`。"))
}

/// Requires one finite native VMM floating-point field.
/// 读取一个必需的有限原生 VMM 浮点字段。
fn required_native_f64(payload: &Value, key: &str) -> Result<f64, String> {
    payload
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
        .ok_or_else(|| format!("原生 VMM 动作缺少有效数值字段 `{key}`。"))
}

/// Requires one native VMM storage version, including the valid initial version zero.
/// 读取一个必需的原生 VMM 存储版本，包含合法的初始版本零。
fn required_native_storage_version(payload: &Value, key: &str) -> Result<u64, String> {
    vmm_payload_u64(payload, key).ok_or_else(|| format!("原生 VMM 动作缺少有效字段 `{key}`。"))
}

/// Builds one standard WebUI action result from a serializable native response.
/// 从可序列化的原生响应构建标准 WebUI 动作结果。
fn native_action_result<T: serde::Serialize>(
    action: &str,
    summary: String,
    kind: &str,
    request: Value,
    response: &T,
    data: VmmMemoryActionResponse,
) -> Result<VmmMemoryActionResult, String> {
    Ok(VmmMemoryActionResult {
        action: action.to_owned(),
        summary,
        kind: kind.to_owned(),
        request,
        response: serde_json::to_value(response)
            .map_err(|error| format!("序列化原生 VMM 动作结果失败：{error}"))?,
        data,
    })
}

/// Maps the internal native project catalog into the stable service API.
/// 将内部原生项目目录映射为稳定服务 API。
fn native_project_catalog(
    catalog: crate::native_vmm::NativeVmmProjectCatalog,
) -> VmmNativeProjectCatalogSummary {
    VmmNativeProjectCatalogSummary {
        projects: catalog
            .projects
            .into_iter()
            .map(|item| native_project_summary(item.project, item.usage))
            .collect(),
        bindings: catalog
            .bindings
            .into_iter()
            .map(|binding| VmmNativeProjectBindingSummary {
                vulcan_project_id: binding.vulcan_project_id,
                owner_project_id: binding.owner_project_id,
                linked_project_ids: binding.linked_project_ids,
            })
            .collect(),
    }
}

/// Maps one internal retention result into the stable service API.
/// 将一个内部保留结果映射为稳定服务 API。
fn native_retention_apply_result(
    result: crate::native_vmm::NativeVmmRetentionApplyResult,
) -> VmmNativeRetentionApplyResult {
    VmmNativeRetentionApplyResult {
        expired_profile_nodes: result.expired_profile_nodes,
        recycled_memories: result.recycled_memories,
        recycled_turns: result.recycled_turns,
        enqueued_cold_jobs: result.enqueued_cold_jobs,
        completed_cold_jobs: result.completed_cold_jobs,
        purged_trash_batches: result.purged_trash_batches,
        project_failures: result
            .project_failures
            .into_iter()
            .map(
                |failure| vulcan_service_api::VmmNativeRetentionProjectFailure {
                    project_id: failure.project_id,
                    error: failure.error,
                },
            )
            .collect(),
    }
}

/// Maps internal retention state and its frozen Core policy into the service API.
/// 将内部保留状态及其冻结 Core 策略映射为服务 API。
fn native_retention_status(
    status: crate::native_vmm::NativeVmmRetentionStatus,
) -> VmmNativeRetentionStatusSummary {
    VmmNativeRetentionStatusSummary {
        enabled: status.enabled,
        running: status.running,
        profile_expiry_running: status.profile_expiry_running,
        interval_minutes: status.interval_minutes,
        profile_expiry_interval_seconds: status.profile_expiry_interval_seconds,
        policy: VmmNativeRetentionPolicySummary {
            schema_version: status.policy.schema_version,
            turn_hot_window_size: status.policy.turn_hot_window_size,
            session_idle_ms: status.policy.session_idle_ms,
            trash_retention_ms: status.policy.trash_retention_ms,
            protected_priority_floor: status.policy.protected_priority_floor,
            protected_memory_level_floor: status.policy.protected_memory_level_floor,
            skip_protected_shared_memories: status.policy.skip_protected_shared_memories,
            profile_expiry_limit: status.policy.profile_expiry_limit,
            memory_recycle_limit: status.policy.memory_recycle_limit,
            cold_turn_scan_limit: status.policy.cold_turn_scan_limit,
            cold_turn_job_limit: status.policy.cold_turn_job_limit,
            idle_session_limit: status.policy.idle_session_limit,
            vector_gc_limit: status.policy.vector_gc_limit,
            trash_purge_limit: status.policy.trash_purge_limit,
        },
        last_started_at_unix_ms: status.last_started_at_unix_ms,
        last_finished_at_unix_ms: status.last_finished_at_unix_ms,
        last_error: status.last_error,
        last_result: status.last_result.map(native_retention_apply_result),
        profile_expiry_last_started_at_unix_ms: status.profile_expiry_last_started_at_unix_ms,
        profile_expiry_last_finished_at_unix_ms: status.profile_expiry_last_finished_at_unix_ms,
        profile_expiry_last_error: status.profile_expiry_last_error,
        profile_expiry_last_result: status
            .profile_expiry_last_result
            .map(native_retention_apply_result),
        pending_cold_jobs: status.pending_cold_jobs,
        recycle_batch_count: status.recycle_batch_count,
    }
}

/// Maps native vector compatibility into the stable browser-facing contract.
/// 将原生向量兼容状态映射为稳定的浏览器契约。
fn native_vector_status(
    status: crate::native_vmm::NativeVmmVectorStatus,
) -> VmmNativeVectorStatusSummary {
    VmmNativeVectorStatusSummary {
        configured: status.configured,
        current_vector_space_id: status.current_vector_space_id,
        embedding_dimension: status.embedding_dimension,
        active_memory_count: status.active_memory_count,
        recycle_bin_memory_count: status.recycle_bin_memory_count,
        current_memory_count: status.current_memory_count,
        current_recycle_bin_memory_count: status.current_recycle_bin_memory_count,
        rebuild_required_memory_count: status.rebuild_required_memory_count,
        active_rebuild_required_memory_count: status.active_rebuild_required_memory_count,
        recycle_bin_rebuild_required_memory_count: status.recycle_bin_rebuild_required_memory_count,
        legacy_vector_space_count: status.legacy_vector_space_count,
        pending_analysis_job_count: status.pending_analysis_job_count,
        error: status.error,
    }
}

/// Maps one internal vector-rebuild preview into the stable browser-facing contract.
/// 将一份内部向量重建预检映射为稳定的浏览器契约。
fn native_vector_rebuild_preview(
    preview: crate::native_vmm::NativeVmmVectorRebuildPreview,
) -> VmmNativeVectorRebuildPreview {
    VmmNativeVectorRebuildPreview {
        storage_version: preview.storage_version,
        target_vector_space_id: preview.target_vector_space_id,
        embedding_dimension: preview.embedding_dimension,
        active_memory_count: preview.active_memory_count,
        recycle_bin_memory_count: preview.recycle_bin_memory_count,
        rebuild_memory_count: preview.rebuild_memory_count,
        active_rebuild_memory_count: preview.active_rebuild_memory_count,
        recycle_bin_rebuild_memory_count: preview.recycle_bin_rebuild_memory_count,
        affected_project_count: preview.affected_project_count,
        batch_count: preview.batch_count,
        preview_fingerprint: preview.preview_fingerprint,
    }
}

/// Maps one completed native vector rebuild into the stable browser-facing contract.
/// 将一次已完成原生向量重建映射为稳定的浏览器契约。
fn native_vector_rebuild_result(
    result: crate::native_vmm::NativeVmmVectorRebuildResult,
) -> VmmNativeVectorRebuildResult {
    VmmNativeVectorRebuildResult {
        storage_version: result.storage_version,
        target_vector_space_id: result.target_vector_space_id,
        rebuilt_memory_count: result.rebuilt_memory_count,
        batch_count: result.batch_count,
    }
}

/// Maps one internal native recycle batch into the stable service API.
/// 将一个内部原生回收批次映射为稳定服务 API。
fn native_recycle_batch(
    batch: crate::native_vmm::NativeVmmRecycleBatchSummary,
) -> VmmNativeRecycleBatchSummary {
    VmmNativeRecycleBatchSummary {
        manual_selection: batch.manual_selection,
        batch_id: batch.batch_id,
        project_id: batch.project_id,
        reason: batch.reason,
        recycled_at_unix_ms: batch.recycled_at_unix_ms,
        expires_at_unix_ms: batch.expires_at_unix_ms,
        turn_count: batch.turn_count,
        memory_count: batch.memory_count,
        profile_node_count: batch.profile_node_count,
        context_evidence_count: batch.context_evidence_count,
    }
}

/// Maps one internal recycle-bin projection into the stable service API.
/// 将一个内部回收站投影映射为稳定服务 API。
fn native_recycle_bin(
    recycle_bin: crate::native_vmm::NativeVmmRecycleBin,
) -> VmmNativeRecycleBinSummary {
    VmmNativeRecycleBinSummary {
        storage_version: recycle_bin.storage_version,
        batches: recycle_bin
            .batches
            .into_iter()
            .map(native_recycle_batch)
            .collect(),
    }
}

/// Loads current and archived VulcanCode session titles without requiring them for provenance.
/// 加载当前及已归档 VulcanCode 会话标题，但不将标题作为来源关联条件。
///
/// # Parameters
/// 参数
/// `state` is the loaded service state containing the optional host workspace snapshot.
/// `state` 是包含可选宿主工作区快照的已加载服务状态。
///
/// # Returns
/// 返回值
/// A map keyed only by stable session identity; active-session titles take precedence.
/// 返回仅以稳定会话身份为键的映射；活动会话标题优先。
fn native_session_titles(
    state: &SharedServiceBridgeState,
) -> Result<BTreeMap<String, String>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("锁定原生 VMM 会话标题目录失败：{error}"))?;
    let Some(workspace) = guard.workspace_snapshot.as_ref() else {
        return Ok(BTreeMap::new());
    };
    let mut titles = BTreeMap::new();
    for session in &workspace.sessions {
        titles.insert(session.id.to_string(), session.title.clone());
    }
    for session in &workspace.archived_sessions {
        // Active metadata is authoritative when a transition temporarily exposes the same id twice.
        // 状态切换暂时同时暴露同一标识时，活动元数据保持权威。
        titles
            .entry(session.id.to_string())
            .or_insert_with(|| session.title.clone());
    }
    Ok(titles)
}

/// Maps internal native provenance records into the stable service API.
/// 将内部原生来源记录映射为稳定服务 API。
fn native_memory_records(
    records: crate::native_vmm::NativeVmmMemoryRecords,
    session_titles: &BTreeMap<String, String>,
) -> VmmNativeMemoryRecordsSummary {
    VmmNativeMemoryRecordsSummary {
        storage_version: records.storage_version,
        project_id: records.project_id,
        sessions: records
            .sessions
            .into_iter()
            .map(|session| session_list::project_session_summary(session, session_titles))
            .collect(),
        selected_session_source_id: records.selected_session_source_id,
        turns: records
            .turns
            .into_iter()
            .map(|turn| VmmNativeTurnRecordSummary {
                turn_id: turn.turn_id,
                previous_turn_ids: turn.previous_turn_ids,
                next_turn_ids: turn.next_turn_ids,
                session_source_id: turn.session_source_id,
                created_at_unix_ms: turn.created_at_unix_ms,
                status: turn.status,
                extracted: turn.extracted,
                user_preview: turn.user_preview,
                assistant_preview: turn.assistant_preview,
                timeline: turn
                    .timeline
                    .into_iter()
                    .map(|item| VmmNativeTimelineItemSummary {
                        role: item.role,
                        content: item.content,
                        occurred_at_unix_ms: item.occurred_at_unix_ms,
                    })
                    .collect(),
                analysis_details: turn.analysis_details,
                generated_memory_ids: turn.generated_memory_ids,
                generated_profile_node_ids: turn.generated_profile_node_ids,
                memory_levels: native_memory_level_counts(turn.memory_levels),
                analysis_job_state: turn.analysis_job_state,
                analysis_attempt_count: turn.analysis_attempt_count,
                analysis_last_error: turn.analysis_last_error,
            })
            .collect(),
        memories: records
            .memories
            .into_iter()
            .map(|memory| VmmNativeMemoryRecordSummary {
                memory_id: memory.memory_id,
                source_turn_id: memory.source_turn_id,
                session_source_id: memory.session_source_id,
                summary: memory.summary,
                content: memory.content,
                category: memory.category,
                source_kind: memory.source_kind,
                state: memory.state,
                memory_level: memory.memory_level,
                priority: memory.priority,
                tags: memory.tags,
                created_at_unix_ms: memory.created_at_unix_ms,
                updated_at_unix_ms: memory.updated_at_unix_ms,
                expires_at_unix_ms: memory.expires_at_unix_ms,
                status_reason: memory.status_reason,
            })
            .collect(),
        profile_nodes: records
            .profile_nodes
            .into_iter()
            .map(native_profile_record)
            .collect(),
    }
}

/// Maps one internal native profile record into the stable service API shape.
/// 将一条内部原生画像记录映射为稳定服务 API 结构。
///
/// # Parameters
/// 参数
/// `node` is one complete native profile audit record.
/// `node` 是一条完整的原生画像审计记录。
///
/// # Returns
/// 返回值
/// The API projection preserving target, lifecycle, and exact source-turn identities.
/// 返回保留目标、生命周期与准确来源回合身份的 API 投影。
fn native_profile_record(
    node: crate::native_vmm::NativeVmmProfileRecordSummary,
) -> VmmNativeProfileRecordSummary {
    VmmNativeProfileRecordSummary {
        profile_node_id: node.profile_node_id,
        target: node.target,
        stored_state: node.stored_state,
        effective_state: node.effective_state,
        active_for_injection: node.active_for_injection,
        content: node.content,
        priority: node.priority,
        level: node.level,
        source_kind: node.source_kind,
        source_id: node.source_id,
        source_turn_ids: node.source_turn_ids,
        effective_date_at_unix_ms: node.effective_date_at_unix_ms,
        expires_at_unix_ms: node.expires_at_unix_ms,
        status_reason: node.status_reason,
        updated_at_unix_ms: node.updated_at_unix_ms,
    }
}

/// Maps all-state native profile records into the stable service API.
/// 将全状态原生画像记录映射为稳定服务 API。
fn native_profile_records(
    records: crate::native_vmm::NativeVmmProfileRecords,
) -> VmmNativeProfileRecordsSummary {
    VmmNativeProfileRecordsSummary {
        storage_version: records.storage_version,
        project_id: records.project_id,
        target: records.target,
        counts: VmmNativeProfileStateCounts {
            active: records.counts.active,
            inactive: records.counts.inactive,
            pending: records.counts.pending,
            expired: records.counts.expired,
            scheduled: records.counts.scheduled,
        },
        nodes: records
            .nodes
            .into_iter()
            .map(native_profile_record)
            .collect(),
    }
}

/// Maps internal memory-level counts into the stable service API.
/// 将内部记忆等级统计映射为稳定服务 API。
fn native_memory_level_counts(
    counts: crate::native_vmm::NativeVmmMemoryLevelCounts,
) -> VmmNativeMemoryLevelCounts {
    VmmNativeMemoryLevelCounts {
        session: counts.session,
        phase: counts.phase,
        stable: counts.stable,
        persistent: counts.persistent,
    }
}

/// Maps internal turn-status counts into the stable service API.
/// 将内部回合状态统计映射为稳定服务 API。
fn native_turn_status_counts(
    counts: crate::native_vmm::NativeVmmTurnStatusCounts,
) -> VmmNativeTurnStatusCounts {
    VmmNativeTurnStatusCounts {
        pending: counts.pending,
        completed: counts.completed,
        processed_unknown: counts.processed_unknown,
        analysis_bypassed: counts.analysis_bypassed,
        corrupted: counts.corrupted,
    }
}

/// Maps one internal native VMM project and its consumers into the service API.
/// 将一个内部原生 VMM 项目及其使用方映射为服务 API。
fn native_project_summary(
    project: vulcan_workspace_store::vmm_data_plane::NativeVmmProject,
    usage: vulcan_workspace_store::vmm_data_plane::NativeVmmProjectUsage,
) -> VmmNativeProjectSummary {
    VmmNativeProjectSummary {
        project_id: project.id,
        name: project.name,
        tag: project.tag,
        protected: project.protected,
        owner_vulcan_project_ids: usage.owner_vulcan_project_ids,
        linked_vulcan_project_ids: usage.linked_vulcan_project_ids,
    }
}

/// Maps one internal native project deletion preview into the service API.
/// 将一份内部原生项目删除预检映射为服务 API。
fn native_project_delete_preview(
    preview: crate::native_vmm::NativeVmmProjectDeletePreview,
) -> VmmNativeProjectDeletePreview {
    VmmNativeProjectDeletePreview {
        storage_version: preview.storage_version,
        project: native_project_summary(preview.project, preview.usage),
        content: native_content_counts(preview.content),
    }
}

/// Persists a project-level VMM disable override after its owner binding is deleted.
/// 在所有者绑定被删除后持久化项目级 VMM 禁用覆盖。
async fn disable_deleted_native_project_owners(
    state: &SharedServiceBridgeState,
    owner_project_ids: &[String],
) -> Vec<String> {
    let project_paths = match workspace_project_paths_from_state(state, owner_project_ids) {
        Ok(paths) => paths,
        Err(error) => {
            tracing::warn!(
                "[webui-service] failed to lock workspace after native VMM deletion: {error}"
            );
            return owner_project_ids.to_vec();
        }
    };
    let Some(project_paths) = project_paths else {
        return owner_project_ids.to_vec();
    };
    let mut failures = Vec::new();
    for (owner_project_id, project_path) in project_paths {
        let commit_state = Arc::clone(state);
        let result = mutate_project_configuration(
            &std::path::PathBuf::from(&project_path),
            "关闭已解除绑定项目的 VMM 配置失败",
            move |project_configuration| {
                project_configuration.vmm.enabled = Some(false);
                Ok(())
            },
            move |project_configuration| {
                update_cached_project_vmm_layer(&project_path, project_configuration.vmm.clone());
                refresh_exact_vmm_hook_project_projection(
                    &commit_state,
                    &project_path,
                    &project_configuration.vmm,
                )
            },
        )
        .await;
        if let Err(error) = result {
            tracing::warn!(
                "[webui-service] failed to disable VMM for detached project `{owner_project_id}`: {error}"
            );
            failures.push(owner_project_id.clone());
        }
    }
    failures
}

/// Maps internal project-content counts into the stable service API.
/// 将内部项目内容计数映射为稳定服务 API。
fn native_content_counts(
    counts: crate::native_vmm::NativeVmmProjectContentCounts,
) -> VmmNativeProjectContentCounts {
    VmmNativeProjectContentCounts {
        sessions: counts.sessions,
        turns: counts.turns,
        memories: counts.memories,
        profile_nodes: counts.profile_nodes,
        context_evidence: counts.context_evidence,
        compact_boundaries: counts.compact_boundaries,
        vector_rows: counts.vector_rows,
    }
}

/// Maps an internal deterministic session rename into the stable service API.
/// 将内部确定性会话改名映射为稳定服务 API。
fn native_session_rename(
    rename: crate::native_vmm::NativeVmmSessionRename,
) -> VmmNativeSessionRename {
    VmmNativeSessionRename {
        from: rename.from,
        to: rename.to,
    }
}

/// Maps an internal migration preview into the stable service API.
/// 将内部迁移预检映射为稳定服务 API。
fn native_migration_preview(
    preview: crate::native_vmm::NativeVmmMigrationPreview,
) -> VmmNativeMigrationPreview {
    VmmNativeMigrationPreview {
        storage_version: preview.storage_version,
        source_project_id: preview.source_project_id,
        target_project_id: preview.target_project_id,
        source: native_content_counts(preview.source),
        target_before: native_content_counts(preview.target_before),
        target_after: native_content_counts(preview.target_after),
        session_renames: preview
            .session_renames
            .into_iter()
            .map(native_session_rename)
            .collect(),
        records: preview
            .records
            .into_iter()
            .map(native_migration_record_summary)
            .collect(),
        preview_fingerprint: preview.preview_fingerprint,
    }
}

/// Maps one internal migration record summary into the stable service API.
/// 将一条内部迁移记录摘要映射为稳定服务 API。
fn native_migration_record_summary(
    record: crate::native_vmm::NativeVmmMigrationRecordSummary,
) -> VmmNativeMigrationRecordSummary {
    VmmNativeMigrationRecordSummary {
        record_kind: record.record_kind,
        record_id: record.record_id,
        session_source_id: record.session_source_id,
        summary: record.summary,
    }
}

/// Maps an internal migration result into the stable service API.
/// 将内部迁移结果映射为稳定服务 API。
fn native_migration_result(
    result: crate::native_vmm::NativeVmmMigrationResult,
) -> VmmNativeMigrationResult {
    VmmNativeMigrationResult {
        storage_version: result.storage_version,
        moved: native_content_counts(result.moved),
        session_renames: result
            .session_renames
            .into_iter()
            .map(native_session_rename)
            .collect(),
    }
}

/// Maps an internal clear preview into the stable service API.
/// 将内部清空预检映射为稳定服务 API。
fn native_clear_preview(
    preview: crate::native_vmm::NativeVmmClearPreview,
) -> VmmNativeClearPreview {
    VmmNativeClearPreview {
        storage_version: preview.storage_version,
        project_id: preview.project_id,
        removed: native_content_counts(preview.removed),
    }
}

/// Maps an internal clear result into the stable service API.
/// 将内部清空结果映射为稳定服务 API。
fn native_clear_result(result: crate::native_vmm::NativeVmmClearResult) -> VmmNativeClearResult {
    VmmNativeClearResult {
        storage_version: result.storage_version,
        removed: native_content_counts(result.removed),
    }
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
            tracing::warn!("[webui-service] failed to refresh VMM health workbench: {error}");
        }
    }
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
    // binding_failure is returned only after the global configuration is durably enabled.
    // binding_failure 仅在全局配置已经持久启用后返回。
    let binding_failure = save_vmm_memory_config(&state, request).await?;
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
    if let Some(binding_failure) = binding_failure {
        return Err(binding_failure);
    }
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

/// Carries one request-scoped external connection used before setup is confirmed.
/// 携带一份在配置确认前使用的请求级外置连接。
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VmmExternalActionConnection {
    /// Exact external gRPC endpoint.
    /// 准确的外置 gRPC 地址。
    grpc_target: String,
    /// Optional external gRPC API key.
    /// 可选的外置 gRPC API 密钥。
    #[serde(default)]
    grpc_api_key: String,
}

/// Parses an explicit request-scoped external connection without touching persisted configuration.
/// 解析一份明确的请求级外置连接，且不修改持久化配置。
///
/// # Parameters
/// 参数
/// `request` is the exact VMM action carrying an optional `connection` object.
/// `request` 是携带可选 `connection` 对象的准确 VMM 动作。
///
/// # Returns
/// 返回值
/// A standalone enabled external layer when supplied, otherwise no override.
/// 提供连接时返回独立且已启用的外置配置层，否则返回空值。
///
/// # Errors
/// Returns an error for project-scoped use, unknown fields, or an empty endpoint.
/// 在项目作用域使用、包含未知字段或地址为空时返回错误。
pub(super) fn vmm_external_action_layer_override(
    request: &VmmMemoryActionRequest,
) -> Result<Option<vulcan_vmm::VmmConfigLayer>, String> {
    let Some(connection) = request.payload.get("connection") else {
        return Ok(None);
    };
    if !request.scope.trim().eq_ignore_ascii_case("global") {
        return Err(String::from("临时外置 VMM 连接只允许用于全局配置向导。"));
    }
    let connection: VmmExternalActionConnection = serde_json::from_value(connection.clone())
        .map_err(|error| format!("解析临时外置 VMM 连接失败：{error}"))?;
    let grpc_target = connection.grpc_target.trim();
    if grpc_target.is_empty() {
        return Err(String::from("临时外置 VMM gRPC 地址不能为空。"));
    }
    Ok(Some(vulcan_vmm::VmmConfigLayer {
        backend_mode: Some(vulcan_vmm::VmmBackendMode::ExternalGrpc),
        enabled: Some(true),
        grpc_target: Some(grpc_target.to_owned()),
        grpc_api_key: (!connection.grpc_api_key.is_empty()).then_some(connection.grpc_api_key),
        ..vulcan_vmm::VmmConfigLayer::default()
    }))
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
/// The resolved VMM layer, catalog references, and native host-project identity for the call.
/// 返回调用使用的 VMM 配置层、目录引用与原生宿主项目身份。
pub(crate) fn vmm_action_context(
    state: &SharedServiceBridgeState,
    request: &VmmMemoryActionRequest,
) -> Result<(vulcan_vmm::VmmConfigLayer, String, String, String), String> {
    if let Some(layer) = vmm_external_action_layer_override(request)? {
        validate_transient_external_action(request.action.trim())?;
        let management_user_ref = vmm_payload_text(&request.payload, "userRef").unwrap_or_default();
        let management_project_ref =
            vmm_payload_text(&request.payload, "projectRef").unwrap_or_default();
        return Ok((
            layer,
            management_user_ref,
            management_project_ref,
            String::new(),
        ));
    }
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
    let global_effective =
        vulcan_vmm::VmmEffectiveConfig::resolve(&provider_configuration.vmm, None, None);
    let payload_user_ref = vmm_payload_text(&request.payload, "userRef");
    let payload_project_ref = vmm_payload_text(&request.payload, "projectRef");
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
        let management_user_ref = payload_user_ref
            .or_else(|| project_layer.user_id.map(|value| value.to_string()))
            .or_else(|| layer.user_id.map(|value| value.to_string()))
            .unwrap_or_default();
        let management_project_ref = payload_project_ref
            .clone()
            .or_else(|| project_layer.project_id.map(|value| value.to_string()))
            .or_else(|| layer.project_id.map(|value| value.to_string()))
            .unwrap_or_default();
        let host_project_id = if effective.backend_mode == vulcan_vmm::VmmBackendMode::Native
            && payload_project_ref.is_none()
        {
            project.id.clone()
        } else {
            String::new()
        };
        return Ok((
            layer,
            management_user_ref,
            management_project_ref,
            host_project_id,
        ));
    }
    let layer = vmm_action_layer_from_effective(&global_effective);
    let management_user_ref = payload_user_ref
        .or_else(|| layer.user_id.map(|value| value.to_string()))
        .unwrap_or_default();
    let management_project_ref = payload_project_ref
        .clone()
        .or_else(|| layer.project_id.map(|value| value.to_string()))
        .unwrap_or_default();
    let host_project_id = if global_effective.backend_mode == vulcan_vmm::VmmBackendMode::Native
        && payload_project_ref.is_none()
    {
        vmm_request_project_scope(&workspace_snapshot, request.project_id.as_deref())
            .map(|project| project.id.clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    Ok((
        layer,
        management_user_ref,
        management_project_ref,
        host_project_id,
    ))
}

/// Restricts request-scoped external connections to non-destructive setup operations.
/// 将请求级外置连接限制为非破坏性的配置操作。
///
/// # Parameters
/// `action` is the exact VMM workbench action requested before global activation.
/// `action` 是全局激活前请求的准确 VMM 工作台动作。
///
/// # Returns
/// Returns success for setup probes and identity creation only; every other action is rejected.
/// 对配置探测与身份创建返回成功；拒绝其他所有动作。
pub(super) fn validate_transient_external_action(action: &str) -> Result<(), String> {
    if matches!(
        action,
        "healthz" | "load_users_projects" | "resolve_user" | "ensure_project"
    ) {
        return Ok(());
    }
    Err(format!(
        "请求级外置 VMM 连接不允许执行动作 `{action}`；请先完成外置后端配置。"
    ))
}

/// Ensures persisted external actions cannot cross the active global backend boundary.
/// 确保持久化外置动作不能跨越当前全局后端边界。
///
/// # Parameters
/// `backend_mode` is the globally resolved VMM backend selection.
/// `backend_mode` 是全局解析后的 VMM 后端选择。
///
/// # Returns
/// Returns success only while the external gRPC backend is globally selected.
/// 仅在全局选择外置 gRPC 后端时返回成功。
pub(super) fn ensure_external_action_backend(
    backend_mode: vulcan_vmm::VmmBackendMode,
) -> Result<(), String> {
    if backend_mode == vulcan_vmm::VmmBackendMode::ExternalGrpc {
        return Ok(());
    }
    Err(String::from(
        "外置 VMM 动作仅在全局选择外置 gRPC 后端后可用。",
    ))
}

/// Converts an effective VMM configuration into a standalone action layer.
/// 将 VMM 最终配置转换为独立动作配置层。
pub(crate) fn vmm_action_layer_from_effective(
    effective: &vulcan_vmm::VmmEffectiveConfig,
) -> vulcan_vmm::VmmConfigLayer {
    vulcan_vmm::VmmConfigLayer {
        backend_mode: Some(effective.backend_mode),
        native_project_binding_mode: Some(effective.native_project_binding_mode),
        external_project_binding_mode: Some(effective.external_project_binding_mode),
        native_fixed_project_id: Some(effective.native_fixed_project_id),
        external_project_prefix: Some(effective.external_project_prefix.clone()),
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
