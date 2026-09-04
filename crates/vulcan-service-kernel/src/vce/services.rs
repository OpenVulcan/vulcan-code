//! Service, management, and native-tool projections for VCE.
//! VCE 的服务、管理与原生工具投影。

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde_json::to_value;
use uuid::Uuid;
use vulcan_agent_runtime::VceRetrievalHandler;
use vulcan_context_engine::{
    ChunkKind, FileStatus, HitKind, QueryIntent, RefreshMode, RefreshReason, SearchSource,
    VceChannelState, VceDegradedFeature, VceEngineState, VceFileQuery, VceIntegrityIssueKind,
    VceMaintenanceMode, VceMaintenancePlan, VceManagementQuery, VceManagementQueryRequest,
    VceManagementQueryResult, VceOperationKind, VceOperationPhase, VcePage as CoreVcePage,
    VcePageRequest as CoreVcePageRequest,
};
use vulcan_service_api as api;
use vulcan_workbench_model::{ProjectVceSettings, TaskProjectSummary};

use crate::{
    ServiceEvent, ServiceEventEmitter, SharedServiceBridgeState,
    commit_and_emit_project_settings_changed_delta, refresh_workbench_snapshot,
    webui_workspace_persist_lock,
};

use super::{
    VceConfigurationReadiness, VceProjectLifecycle, VceRuntimeManager, VceRuntimeProjectStatus,
    VceRuntimeRetrieveResult, VceWatcherService, require_vce_configuration_ready,
    vce_configuration_readiness,
};

/// One startup or settings reconciliation report that preserves per-project failures.
/// 一份保留逐项目失败信息的启动或设置协调报告。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VceReconcileReport {
    /// Projects successfully enabled or disabled according to current settings.
    /// 已按当前设置成功启用或禁用的项目。
    pub(crate) reconciled_projects: Vec<String>,
    /// Bounded project-specific failures that did not stop the service.
    /// 未阻断服务的有界项目级失败。
    pub(crate) failures: Vec<(String, String)>,
}

/// Build the project-scoped callback injected into native tool execution preparation.
/// 构建注入原生工具执行准备阶段的项目作用域回调。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative global model configuration and project intent.
/// `state` 持有权威全局模型配置与项目意图。
/// `manager` owns the active project runtimes and committed retrieval snapshots.
/// `manager` 持有活动项目运行时与已提交检索快照。
///
/// # Returns
/// 返回值
/// A callback that projects VCE results into the stable service JSON contract.
/// 返回将 VCE 结果投影为稳定服务 JSON 契约的回调。
pub(crate) fn vce_retrieval_handler(
    state: SharedServiceBridgeState,
    manager: VceRuntimeManager,
) -> VceRetrievalHandler {
    VceRetrievalHandler::new(move |project_id, information_request| {
        let state = state.clone();
        let manager = manager.clone();
        Box::pin(async move {
            require_vce_project_runtime_ready(&state, &manager, &project_id).await?;
            let result = manager.retrieve(&project_id, &information_request).await?;
            // Revalidate after provider and index work so a concurrent public-AI revocation cannot
            // publish a result admitted under the previous global contract.
            // 在 Provider 与索引工作完成后重新校验，防止并发公共 AI 撤销发布按旧全局契约准入的结果。
            require_vce_project_runtime_ready(&state, &manager, &project_id).await?;
            to_value(vce_retrieval_response(project_id, result))
                .map_err(|error| format!("serialize VCE retrieval response failed: {error}"))
        })
    })
}

/// Reconcile every active workbench project without allowing one failure to stop peers.
/// 协调所有活动工作台项目，且不允许单个失败阻断其他项目。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace snapshot, manager, and shared watcher subscriber.
/// `state` 持有当前工作区快照、管理器与共享监听订阅器。
///
/// # Returns
/// 返回值
/// A deterministic report containing successes and isolated failures.
/// 返回包含成功项与隔离失败项的确定性报告。
pub(crate) async fn reconcile_vce_projects(
    state: &SharedServiceBridgeState,
) -> Result<VceReconcileReport, String> {
    let readiness = vce_configuration_readiness(state);
    let (projects, manager, watcher) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let projects = guard
            .workspace_snapshot
            .as_ref()
            .map(|workspace| workspace.task_projects.clone())
            .unwrap_or_default();
        (
            projects,
            guard.vce_runtime.clone(),
            guard.vce_watcher.clone(),
        )
    };
    let mut report = VceReconcileReport::default();
    for project in projects {
        match reconcile_vce_project_handles(
            manager.as_ref(),
            watcher.as_ref(),
            &project,
            &readiness,
        )
        .await
        {
            Ok(()) => report.reconciled_projects.push(project.id),
            Err(error) => report
                .failures
                .push((project.id, bounded_service_error(error))),
        }
    }
    Ok(report)
}

/// Reconcile one authoritative project after a settings mutation.
/// 在设置修改后协调一个权威项目。
///
/// # Parameters
/// 参数
/// `state` owns the refreshed workspace snapshot and runtime handles.
/// `state` 持有已刷新的工作区快照与运行时句柄。
/// `project_id` selects exactly one active project.
/// `project_id` 精确选择一个活动项目。
///
/// # Returns
/// 返回值
/// Success after runtime and watcher ownership match persisted settings.
/// 运行时及监听所有权与持久化设置一致后返回成功。
pub(crate) async fn reconcile_vce_project(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<(), String> {
    let readiness = vce_configuration_readiness(state);
    let (project, manager, watcher) = vce_project_runtime_inputs(state, project_id)?;
    reconcile_vce_project_handles(manager.as_ref(), watcher.as_ref(), &project, &readiness).await
}

/// Schedules one complete VCE reconciliation after a committed public-AI publication.
/// 在公共 AI 发布提交后安排一次完整 VCE 协调。
///
/// # Parameters
/// 参数
/// `app` emits authoritative project invalidations after reconciliation settles.
/// `app` 在协调收敛后发送权威项目失效通知。
/// `state` owns the service runtime used for non-blocking lifecycle reconciliation.
/// `state` 持有用于非阻塞生命周期协调的服务运行时。
pub(crate) fn schedule_vce_reconciliation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
) {
    let runtime = state
        .lock()
        .ok()
        .and_then(|guard| guard.service_runtime.clone());
    let Some(runtime) = runtime else {
        return;
    };
    runtime.spawn(async move {
        match reconcile_vce_projects(&state).await {
            Ok(report) => {
                for project_id in report
                    .reconciled_projects
                    .iter()
                    .chain(report.failures.iter().map(|(project_id, _)| project_id))
                {
                    emit_vce_project_changed(&app, &state, project_id).await;
                }
            }
            Err(error) => {
                eprintln!("[webui-service] VCE configuration reconciliation failed: {error}");
            }
        }
    });
}

/// Verifies that one exact project runtime still matches the current global embedding contract.
/// 验证一个精确项目运行时仍匹配当前全局向量契约。
///
/// # Parameters
/// 参数
/// `state` owns global configuration and persisted project intent.
/// `state` 持有全局配置与持久化项目意图。
/// `manager` owns active project runtime identities.
/// `manager` 持有活动项目运行时身份。
/// `project_id` is supplied only by the current execution context.
/// `project_id` 仅由当前执行上下文提供。
///
/// # Returns
/// 返回值
/// Success only for an enabled runtime whose revision and embedding fingerprint remain current.
/// 仅当已启用运行时的修订号与向量指纹仍为当前值时返回成功。
async fn require_vce_project_runtime_ready(
    state: &SharedServiceBridgeState,
    manager: &VceRuntimeManager,
    project_id: &str,
) -> Result<(), String> {
    let readiness = vce_configuration_readiness(state);
    if !readiness.status.ready {
        return Err(String::from(
            "VCE retrieval is unavailable because a required LLM, embedding, or rerank route is not ready.",
        ));
    }
    let project = vce_project_runtime_inputs(state, project_id)
        .map_err(|_| String::from("VCE retrieval project context is unavailable."))?
        .0;
    if !project.vce.enabled {
        return Err(format!(
            "VCE retrieval is disabled for project `{project_id}`."
        ));
    }
    let expected_fingerprint = readiness
        .embedding_physical_fingerprint
        .as_deref()
        .ok_or_else(|| String::from("VCE readiness is missing its embedding fingerprint."))?;
    let status = manager
        .project_status(project_id)
        .await
        .ok_or_else(|| format!("VCE project `{project_id}` has no active retrieval runtime."))?;
    if !runtime_status_can_serve_requests(Some(&status), project.vce.revision, expected_fingerprint)
    {
        return Err(format!(
            "VCE project `{project_id}` does not have a current committed retrieval checkpoint."
        ));
    }
    Ok(())
}

/// Loads the authoritative global VCE configuration prerequisite state.
/// 加载权威 VCE 全局配置前置状态。
///
/// # Parameters
/// 参数
/// `state` owns the current public-AI configuration and executable model catalog.
/// `state` 持有当前公共 AI 配置与可执行模型目录。
///
/// # Returns
/// 返回值
/// A provider-independent fail-closed status suitable for the built-in plugin page.
/// 返回适用于内置插件页面且不依赖 Provider 的失败关闭状态。
pub(crate) fn service_get_vce_configuration_status(
    state: SharedServiceBridgeState,
) -> Result<api::VceGlobalConfigurationStatus, String> {
    Ok(vce_configuration_readiness(&state).status)
}

/// Load one complete management overview for the code-context page.
/// 为代码上下文页面加载一份完整管理概览。
///
/// # Parameters
/// 参数
/// `state` owns persisted project settings and active runtime state.
/// `state` 持有持久化项目设置与活动运行时状态。
/// `request` selects the exact project.
/// `request` 选择精确项目。
///
/// # Returns
/// 返回值
/// A provider-independent overview, including disabled and unavailable states.
/// 返回与 Provider 无关的概览，包含禁用与不可用状态。
pub(crate) async fn service_get_vce_project_overview(
    state: SharedServiceBridgeState,
    request: api::VceProjectRequest,
) -> Result<api::VceProjectOverviewResponse, String> {
    let readiness = vce_configuration_readiness(&state);
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    let settings = vce_settings_summary(&project.vce);
    if !readiness.status.ready {
        let status = if project.vce.enabled {
            blocked_configuration_status(&project, &readiness.status)
        } else {
            disabled_status(&project)
        };
        return Ok(api::VceProjectOverviewResponse {
            configuration: readiness.status,
            status,
            settings,
            ai: None,
            counts: None,
            integrity: None,
        });
    }
    if !project.vce.enabled {
        return Ok(api::VceProjectOverviewResponse {
            configuration: readiness.status,
            status: disabled_status(&project),
            settings,
            ai: None,
            counts: None,
            integrity: None,
        });
    }
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    let status = manager
        .project_status(&project.id)
        .await
        .ok_or_else(|| format!("VCE 项目 `{}` 尚未协调。", project.id))?;
    let snapshot = if status.engine.is_some() {
        Some(manager.management_snapshot(&project.id).await?)
    } else {
        None
    };
    Ok(vce_overview_from_runtime(
        readiness.status,
        settings,
        status,
        snapshot,
    ))
}

/// Replace one project's complete VCE settings under optimistic revision control.
/// 在乐观修订控制下替换单个项目的完整 VCE 设置。
///
/// # Parameters
/// 参数
/// `app` emits focused settings and lifecycle invalidations.
/// `app` 发送聚焦设置与生命周期失效通知。
/// `state` owns persistence and the runtime manager.
/// `state` 持有持久化与运行管理器。
/// `request` contains the expected revision and complete next editable settings.
/// `request` 包含预期修订号与完整下一版可编辑设置。
///
/// # Returns
/// 返回值
/// The persisted settings summary after runtime reconciliation.
/// 返回运行时协调后的持久化设置摘要。
pub(crate) async fn service_update_vce_project_settings(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: api::VceProjectSettingsUpdateRequest,
) -> Result<api::VceProjectSettingsSummary, String> {
    if request.settings.enabled {
        require_vce_configuration_ready(&state)?;
    }
    let project_id = stable_project_id(&request.project_id)?;
    let persist_lock = webui_workspace_persist_lock();
    let persist_guard = persist_lock.lock().await;
    let (project, controller_runtime) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let project = active_project_from_guard(&guard, &project_id)?;
        let controller = guard
            .controller_runtime
            .clone()
            .ok_or_else(|| String::from("控制器运行时尚未就绪。"))?;
        (project, controller)
    };
    if project.vce.revision != request.expected_revision {
        return Err(format!(
            "VCE 设置修订冲突：期望 {}，当前 {}。",
            request.expected_revision, project.vce.revision
        ));
    }
    let next_revision = request
        .expected_revision
        .checked_add(1)
        .ok_or_else(|| String::from("VCE 设置修订号已耗尽。"))?;
    let next = project_vce_settings_from_input(
        request.settings,
        next_revision,
        project.vce.migrated_from_codebase_memory,
    );
    next.validate()?;
    vulcan_workspace_store::workspace_store::persist_project_vce_settings(
        controller_runtime,
        &project_id,
        request.expected_revision,
        &next,
    )
    .await?;
    refresh_workbench_snapshot(state.clone()).await?;
    drop(persist_guard);
    let reconcile_error = reconcile_vce_project(&state, &project_id).await.err();
    commit_and_emit_project_settings_changed_delta(
        &app,
        &state,
        Some(project_id.clone()),
        api::ProjectSettingsDomain::Vce,
    )?;
    emit_vce_project_changed(&app, &state, &project_id).await;
    if let Some(error) = reconcile_error {
        return Err(format!("VCE 设置已保存，但运行时协调失败：{error}"));
    }
    Ok(vce_settings_summary(&next))
}

/// Request an explicit full project refresh with revision validation.
/// 在修订校验下请求显式项目全量刷新。
///
/// # Parameters
/// 参数
/// `app` emits the resulting project lifecycle event.
/// `app` 发送结果项目生命周期事件。
/// `state` owns the selected runtime.
/// `state` 持有所选运行时。
/// `request` guards the command with the current settings revision.
/// `request` 使用当前设置修订号保护命令。
///
/// # Returns
/// 返回值
/// The refreshed project overview.
/// 返回刷新后的项目概览。
pub(crate) async fn service_refresh_vce_project(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: api::VceProjectMutationRequest,
) -> Result<api::VceProjectOverviewResponse, String> {
    require_vce_configuration_ready(&state)?;
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    validate_mutation_revision(&project, &request)?;
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    let refresh_result = manager
        .refresh_full(&project.id, RefreshReason::Manual)
        .await;
    emit_vce_project_changed(&app, &state, &project.id).await;
    refresh_result?;
    service_get_vce_project_overview(
        state,
        api::VceProjectRequest {
            project_id: project.id,
        },
    )
    .await
}

/// List one bounded page of project file/index rows.
/// 列出一页有界项目文件与索引记录。
pub(crate) async fn service_list_vce_files(
    state: SharedServiceBridgeState,
    request: api::VceFileListRequest,
) -> Result<api::VcePageResponse<api::VceFileSummary>, String> {
    let manager = vce_manager_for_project(&state, &request.project_id)?;
    let query = VceManagementQuery::Files(VceFileQuery {
        path_prefix: request.path_prefix,
        status: request
            .status
            .as_deref()
            .map(parse_file_status)
            .transpose()?,
        auxiliary_failures_only: request.auxiliary_failures_only,
        page: core_page_request(request.page)?,
    });
    match execute_management_query(&manager, &request.project_id, query).await? {
        VceManagementQueryResult::Files(page) => Ok(map_page(page, map_file_summary)),
        _ => Err(String::from("VCE 文件查询返回了不匹配的结果类型。")),
    }
}

/// Load one exact Blob detail without exposing storage records.
/// 加载一份精确 Blob 详情且不暴露存储记录。
pub(crate) async fn service_get_vce_blob_detail(
    state: SharedServiceBridgeState,
    request: api::VceBlobDetailRequest,
) -> Result<Option<api::VceBlobDetailResponse>, String> {
    let manager = vce_manager_for_project(&state, &request.project_id)?;
    let query = VceManagementQuery::BlobDetail {
        blob_name: request.blob_name,
    };
    match execute_management_query(&manager, &request.project_id, query).await? {
        VceManagementQueryResult::BlobDetail(detail) => Ok(detail.map(map_blob_detail)),
        _ => Err(String::from("VCE Blob 查询返回了不匹配的结果类型。")),
    }
}

/// List one bounded page of durable VCE operations.
/// 列出一页有界 VCE 持久操作。
pub(crate) async fn service_list_vce_operations(
    state: SharedServiceBridgeState,
    request: api::VcePagedProjectRequest,
) -> Result<api::VcePageResponse<api::VceOperationSummary>, String> {
    let manager = vce_manager_for_project(&state, &request.project_id)?;
    let query = VceManagementQuery::Operations(core_page_request(request.page)?);
    match execute_management_query(&manager, &request.project_id, query).await? {
        VceManagementQueryResult::Operations(page) => Ok(map_page(page, map_operation)),
        _ => Err(String::from("VCE 作业查询返回了不匹配的结果类型。")),
    }
}

/// List one bounded page of VCE integrity issues.
/// 列出一页有界 VCE 完整性问题。
pub(crate) async fn service_list_vce_integrity_issues(
    state: SharedServiceBridgeState,
    request: api::VcePagedProjectRequest,
) -> Result<api::VcePageResponse<api::VceIntegrityIssueSummary>, String> {
    let manager = vce_manager_for_project(&state, &request.project_id)?;
    let query = VceManagementQuery::IntegrityIssues(core_page_request(request.page)?);
    match execute_management_query(&manager, &request.project_id, query).await? {
        VceManagementQueryResult::IntegrityIssues(page) => Ok(map_page(page, map_integrity_issue)),
        _ => Err(String::from("VCE 完整性查询返回了不匹配的结果类型。")),
    }
}

/// Build a read-only maintenance plan that performs no mutation.
/// 构建一份不执行修改的只读维护计划。
pub(crate) async fn service_plan_vce_maintenance(
    state: SharedServiceBridgeState,
    request: api::VceMaintenancePlanRequest,
) -> Result<api::VceMaintenancePlanResponse, String> {
    let manager = vce_manager_for_project(&state, &request.project_id)?;
    let mode = parse_maintenance_mode(&request.mode)?;
    let plan = load_maintenance_plan(&manager, &request.project_id, mode).await?;
    Ok(map_maintenance_plan(plan))
}

/// Execute an exact current maintenance plan under settings and checkpoint guards.
/// 在设置与检查点保护下执行一份精确的当前维护计划。
pub(crate) async fn service_execute_vce_maintenance(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: api::VceMaintenanceExecutionRequest,
) -> Result<api::VceProjectOverviewResponse, String> {
    require_vce_configuration_ready(&state)?;
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    validate_project_revision(&project, request.expected_settings_revision)?;
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    validate_checkpoint(
        &manager,
        &project.id,
        Some(request.expected_checkpoint_version),
    )
    .await?;
    let mode = parse_maintenance_mode(&request.mode)?;
    let plan = load_maintenance_plan(&manager, &project.id, mode).await?;
    let planned_ids = plan
        .actions
        .iter()
        .map(|action| action.action_id.clone())
        .collect::<BTreeSet<_>>();
    let requested_ids = request.action_ids.into_iter().collect::<BTreeSet<_>>();
    if planned_ids.is_empty() {
        return Err(String::from("当前 VCE 维护计划没有可执行操作。"));
    }
    if requested_ids != planned_ids {
        return Err(String::from("VCE 维护操作已经变化，请重新生成计划并确认。"));
    }
    let execution = match mode {
        VceMaintenanceMode::Repair => {
            manager.cleanup_staging(&project.id).await?;
            manager
                .rebuild_full(&project.id, RefreshReason::Recovery)
                .await
                .map(|_| ())
        }
        VceMaintenanceMode::Rebuild => manager
            .rebuild_full(&project.id, RefreshReason::Manual)
            .await
            .map(|_| ()),
        VceMaintenanceMode::CleanupStaging => manager.cleanup_staging(&project.id).await,
    };
    emit_vce_project_changed(&app, &state, &project.id).await;
    execution?;
    service_get_vce_project_overview(
        state,
        api::VceProjectRequest {
            project_id: project.id,
        },
    )
    .await
}

/// Retry one visible failed operation as a complete recovery refresh.
/// 将一项可见失败操作重试为一次完整恢复刷新。
pub(crate) async fn service_retry_vce_operation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: api::VceOperationMutationRequest,
) -> Result<api::VceProjectOverviewResponse, String> {
    require_vce_configuration_ready(&state)?;
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    validate_project_revision(&project, request.expected_settings_revision)?;
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    validate_checkpoint(&manager, &project.id, request.expected_checkpoint_version).await?;
    validate_retryable_operation(&manager, &project.id, &request.operation_id).await?;
    let retry_result = manager
        .refresh_full(&project.id, RefreshReason::Recovery)
        .await;
    emit_vce_project_changed(&app, &state, &project.id).await;
    retry_result?;
    service_get_vce_project_overview(
        state,
        api::VceProjectRequest {
            project_id: project.id,
        },
    )
    .await
}

/// Request cancellation of the currently active refresh under an operation guard.
/// 在操作身份保护下请求取消当前活动刷新。
pub(crate) async fn service_cancel_vce_operation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: api::VceOperationMutationRequest,
) -> Result<api::VceProjectOverviewResponse, String> {
    require_vce_configuration_ready(&state)?;
    let (project, manager, _) = vce_project_runtime_inputs(&state, &request.project_id)?;
    validate_project_revision(&project, request.expected_settings_revision)?;
    let manager = manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))?;
    validate_checkpoint(&manager, &project.id, request.expected_checkpoint_version).await?;
    manager
        .cancel_refresh(&project.id, Some(&request.operation_id))
        .await?;
    emit_vce_project_changed(&app, &state, &project.id).await;
    service_get_vce_project_overview(
        state,
        api::VceProjectRequest {
            project_id: project.id,
        },
    )
    .await
}

/// Reconcile one project against its current settings and watcher handles.
/// 根据当前设置与监听句柄协调单个项目。
async fn reconcile_vce_project_handles(
    manager: Option<&VceRuntimeManager>,
    watcher: Option<&VceWatcherService>,
    project: &TaskProjectSummary,
    readiness: &VceConfigurationReadiness,
) -> Result<(), String> {
    if !project.vce.enabled || !readiness.status.ready {
        if let Some(watcher) = watcher {
            watcher.unwatch_project(&project.id).await;
        }
        if let Some(manager) = manager {
            manager.disable_project(&project.id).await?;
        }
        return Ok(());
    }
    let expected_fingerprint = readiness
        .embedding_physical_fingerprint
        .as_deref()
        .ok_or_else(|| String::from("VCE 就绪状态缺少向量物理指纹。"))?;
    let manager = manager.ok_or_else(|| String::from("VCE 运行时不可用。"))?;
    let current_status = manager.project_status(&project.id).await;
    if runtime_status_matches_configuration(
        current_status.as_ref(),
        project.vce.revision,
        expected_fingerprint,
    ) {
        return Ok(());
    }
    let project_root = PathBuf::from(&project.path);
    // watcher is the required host-owned observation boundary for every enabled project.
    // watcher 是每个已启用项目必需的宿主持有观察边界。
    let watcher = watcher.ok_or_else(|| String::from("VCE 文件监听服务不可用。"))?;
    // Arm the filesystem watch before the authoritative full scan. Every write after the scan
    // boundary is then queued for a serialized path refresh instead of falling into an unobserved
    // activation window.
    // 在权威全量扫描前建立文件监听，使扫描边界之后的每次写入都进入串行路径刷新队列，避免落入
    // 无监听的激活窗口。
    watcher.watch_project(&project.id, &project_root)?;
    match manager
        .enable_project(&project.id, &project_root, project.vce.clone())
        .await
    {
        Ok(_) => {
            let status = manager.project_status(&project.id).await;
            if !runtime_status_matches_configuration(
                status.as_ref(),
                project.vce.revision,
                expected_fingerprint,
            ) {
                return Ok(());
            }
            Ok(())
        }
        Err(error) => {
            let status = manager.project_status(&project.id).await;
            if status
                .as_ref()
                .is_some_and(|status| status.config_revision != project.vce.revision)
            {
                return Ok(());
            }
            watcher.unwatch_project(&project.id).await;
            Err(error)
        }
    }
}

/// Return whether a runtime status represents the requested settings and embedding contract.
/// 返回运行时状态是否表示请求的设置与向量契约。
///
/// # Parameters
/// 参数
/// `status` is the latest manager-owned project state.
/// `status` 是管理器持有的最新项目状态。
/// `expected_revision` is the revision owned by the reconciliation command.
/// `expected_revision` 是协调命令持有的修订号。
/// `expected_fingerprint` is the current global embedding compatibility identity.
/// `expected_fingerprint` 是当前全局向量兼容性身份。
///
/// # Returns
/// 返回值
/// `true` only when an active runtime owns both exact identities.
/// 仅当活动运行时同时持有两个精确身份时返回 `true`。
fn runtime_status_matches_configuration(
    status: Option<&VceRuntimeProjectStatus>,
    expected_revision: u64,
    expected_fingerprint: &str,
) -> bool {
    status.is_some_and(|status| {
        status.config_revision == expected_revision
            && status.embedding_physical_fingerprint.as_deref() == Some(expected_fingerprint)
    })
}

/// Return whether a matching runtime can currently serve committed retrieval data.
/// 返回一个匹配的运行时当前是否能够提供已提交检索数据。
///
/// # Parameters
/// 参数
/// `status` is the latest manager-owned project state.
/// `status` 是管理器持有的最新项目状态。
/// `expected_revision` and `expected_fingerprint` are the current authoritative identities.
/// `expected_revision` 与 `expected_fingerprint` 是当前权威身份。
///
/// # Returns
/// 返回值
/// `true` only when the runtime is current, usable, and owns one committed checkpoint.
/// 仅当运行时为当前版本、处于可用状态且持有一个已提交检查点时返回 `true`。
fn runtime_status_can_serve_requests(
    status: Option<&VceRuntimeProjectStatus>,
    expected_revision: u64,
    expected_fingerprint: &str,
) -> bool {
    runtime_status_matches_configuration(status, expected_revision, expected_fingerprint)
        && status.is_some_and(|status| {
            matches!(
                status.lifecycle,
                VceProjectLifecycle::Ready | VceProjectLifecycle::Refreshing
            ) && status
                .engine
                .as_ref()
                .and_then(|engine| engine.checkpoint_version)
                .is_some()
        })
}

/// Capture one exact project and the current VCE handles without retaining the state lock.
/// 捕获一个精确项目与当前 VCE 句柄且不持续持有状态锁。
fn vce_project_runtime_inputs(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<
    (
        TaskProjectSummary,
        Option<VceRuntimeManager>,
        Option<VceWatcherService>,
    ),
    String,
> {
    let project_id = stable_project_id(project_id)?;
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    Ok((
        active_project_from_guard(&guard, &project_id)?,
        guard.vce_runtime.clone(),
        guard.vce_watcher.clone(),
    ))
}

/// Resolve one active project from the authoritative workspace snapshot.
/// 从权威工作区快照中解析一个活动项目。
fn active_project_from_guard(
    guard: &crate::ServiceBridgeState,
    project_id: &str,
) -> Result<TaskProjectSummary, String> {
    guard
        .workspace_snapshot
        .as_ref()
        .and_then(|workspace| {
            workspace
                .task_projects
                .iter()
                .find(|project| project.id == project_id)
        })
        .cloned()
        .ok_or_else(|| format!("项目 `{project_id}` 不存在。"))
}

/// Resolve the required manager only after confirming the project is enabled.
/// 仅在确认项目已启用后解析必需的管理器。
fn vce_manager_for_project(
    state: &SharedServiceBridgeState,
    project_id: &str,
) -> Result<VceRuntimeManager, String> {
    require_vce_configuration_ready(state)?;
    let (project, manager, _) = vce_project_runtime_inputs(state, project_id)?;
    if !project.vce.enabled {
        return Err(format!("VCE 项目 `{}` 已禁用。", project.id));
    }
    manager.ok_or_else(|| String::from("VCE 运行时尚未就绪。"))
}

/// Execute one typed management request with a unique operation identity.
/// 使用唯一操作身份执行一项类型化管理请求。
async fn execute_management_query(
    manager: &VceRuntimeManager,
    project_id: &str,
    query: VceManagementQuery,
) -> Result<VceManagementQueryResult, String> {
    let response = manager
        .management_query(
            project_id,
            VceManagementQueryRequest {
                operation_id: format!("vce:{project_id}:management:{}", Uuid::new_v4()),
                query,
            },
        )
        .await?;
    Ok(response.result)
}

/// Load one typed maintenance plan from the exact project index.
/// 从精确项目索引加载一份类型化维护计划。
async fn load_maintenance_plan(
    manager: &VceRuntimeManager,
    project_id: &str,
    mode: VceMaintenanceMode,
) -> Result<VceMaintenancePlan, String> {
    let query = VceManagementQuery::MaintenancePlan { mode };
    match execute_management_query(manager, project_id, query).await? {
        VceManagementQueryResult::MaintenancePlan(plan) => Ok(plan),
        _ => Err(String::from("VCE 维护计划查询返回了不匹配的结果类型。")),
    }
}

/// Project one core maintenance plan into its public service representation.
/// 将一份内核维护计划投影为公共服务表示。
fn map_maintenance_plan(plan: VceMaintenancePlan) -> api::VceMaintenancePlanResponse {
    api::VceMaintenancePlanResponse {
        mode: maintenance_mode_text(plan.mode).to_owned(),
        actions: plan
            .actions
            .into_iter()
            .map(|action| api::VceMaintenanceActionSummary {
                action_id: action.action_id,
                description: action.description,
                blob_names: action.blob_names,
                destructive: action.destructive,
            })
            .collect(),
        affected_blob_count: plan.affected_blob_count,
    }
}

/// Validate one enabled project settings revision before a host mutation.
/// 在宿主修改前校验一个已启用项目的设置修订。
fn validate_project_revision(
    project: &TaskProjectSummary,
    expected_revision: u64,
) -> Result<(), String> {
    if !project.vce.enabled {
        return Err(format!("VCE 项目 `{}` 已禁用。", project.id));
    }
    if project.vce.revision != expected_revision {
        return Err(format!(
            "VCE 设置修订冲突：期望 {expected_revision}，当前 {}。",
            project.vce.revision
        ));
    }
    Ok(())
}

/// Validate an optional published-checkpoint precondition against current runtime state.
/// 针对当前运行态校验可选已发布检查点前置条件。
async fn validate_checkpoint(
    manager: &VceRuntimeManager,
    project_id: &str,
    expected_checkpoint: Option<u64>,
) -> Result<(), String> {
    let Some(expected_checkpoint) = expected_checkpoint else {
        return Ok(());
    };
    let status = manager
        .project_status(project_id)
        .await
        .ok_or_else(|| format!("VCE 项目 `{project_id}` 尚未协调。"))?;
    let actual_checkpoint = status.engine.and_then(|engine| engine.checkpoint_version);
    if actual_checkpoint != Some(expected_checkpoint) {
        return Err(format!(
            "VCE 检查点冲突：期望 {expected_checkpoint}，当前 {actual_checkpoint:?}。"
        ));
    }
    Ok(())
}

/// Validate that a visible operation exists and ended in a retryable phase.
/// 校验一项可见操作存在且结束于可重试阶段。
async fn validate_retryable_operation(
    manager: &VceRuntimeManager,
    project_id: &str,
    operation_id: &str,
) -> Result<(), String> {
    if operation_id.trim().is_empty() {
        return Err(String::from("VCE operation_id 不能为空。"));
    }
    let query = VceManagementQuery::Operations(
        CoreVcePageRequest::new(None, 500).map_err(|error| error.to_string())?,
    );
    let page = match execute_management_query(manager, project_id, query).await? {
        VceManagementQueryResult::Operations(page) => page,
        _ => return Err(String::from("VCE 作业查询返回了不匹配的结果类型。")),
    };
    let operation = page
        .items
        .into_iter()
        .find(|operation| operation.operation_id == operation_id)
        .ok_or_else(|| format!("VCE 操作 `{operation_id}` 不存在或已超出历史窗口。"))?;
    if !matches!(
        operation.phase,
        VceOperationPhase::Failed | VceOperationPhase::Cancelled
    ) {
        return Err(format!("VCE 操作 `{operation_id}` 当前阶段不可重试。"));
    }
    Ok(())
}

/// Emit one current VCE project event when the project remains available.
/// 在项目仍可用时发送一条当前 VCE 项目事件。
async fn emit_vce_project_changed(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    project_id: &str,
) {
    // configurationReady distinguishes global fail-closed removal from an unexpected runtime loss.
    // configurationReady 区分全局失败关闭移除与意外运行时丢失。
    let configuration_ready = vce_configuration_readiness(state).status.ready;
    let Ok((project, manager, _)) = vce_project_runtime_inputs(state, project_id) else {
        return;
    };
    let status = match manager {
        Some(manager) => manager.project_status(project_id).await,
        None => None,
    };
    let lifecycle = status
        .as_ref()
        .map(|status| lifecycle_state(status.lifecycle))
        .unwrap_or(if project.vce.enabled {
            if configuration_ready {
                api::VceProjectLifecycleState::Failed
            } else {
                api::VceProjectLifecycleState::BlockedConfiguration
            }
        } else {
            api::VceProjectLifecycleState::Disabled
        });
    let checkpoint_version = status
        .as_ref()
        .and_then(|status| status.engine.as_ref())
        .and_then(|engine| engine.checkpoint_version);
    app.emit(ServiceEvent::VceProjectChanged(Box::new(
        api::VceProjectChangedEvent {
            project_id: project.id,
            settings_revision: project.vce.revision,
            lifecycle,
            checkpoint_version,
        },
    )));
}

/// Emit one lifecycle event directly from an enabled runtime after background work settles.
/// 在后台工作收敛后直接从已启用运行时发送一项生命周期事件。
pub(crate) async fn emit_vce_runtime_changed(
    app: &ServiceEventEmitter,
    manager: &VceRuntimeManager,
    project_id: &str,
) {
    let Some(status) = manager.project_status(project_id).await else {
        return;
    };
    let checkpoint_version = status
        .engine
        .as_ref()
        .and_then(|engine| engine.checkpoint_version);
    app.emit(ServiceEvent::VceProjectChanged(Box::new(
        api::VceProjectChangedEvent {
            project_id: project_id.to_owned(),
            settings_revision: status.config_revision,
            lifecycle: lifecycle_state(status.lifecycle),
            checkpoint_version,
        },
    )));
}

/// Project one functional retrieval result into the model-facing API shape.
/// 将一份功能检索结果投影为模型侧 API 形状。
fn vce_retrieval_response(
    project_id: String,
    runtime: VceRuntimeRetrieveResult,
) -> api::VceRetrievalResponse {
    let result = runtime.result;
    api::VceRetrievalResponse {
        project_id,
        request_id: result.request_id,
        settings_revision: runtime.settings_revision,
        checkpoint_version: runtime.checkpoint_version,
        intent: query_intent_text(result.query_plan.intent).to_owned(),
        queries: result.query_plan.queries,
        hits: result
            .hits
            .into_iter()
            .map(|hit| api::VceRetrievalHit {
                id: hit.id,
                blob_name: hit.blob_name,
                path: hit.path,
                start_line: hit.range.start_line,
                end_line: hit.range.end_line,
                content: hit.content,
                score: hit.score,
                source: search_source_text(hit.source).to_owned(),
                kind: hit_kind_text(hit.kind).to_owned(),
            })
            .collect(),
        formatted_context: result.formatted_context,
        degraded_features: result
            .degraded_features
            .into_iter()
            .map(|feature| degraded_feature_text(feature).to_owned())
            .collect(),
        degradations: result
            .degradations
            .into_iter()
            .map(|item| {
                format!(
                    "{} [{}]: {}",
                    degraded_feature_text(item.feature),
                    item.operation_id,
                    item.reason
                )
            })
            .collect(),
    }
}

/// Build a provider-independent overview from manager and core snapshots.
/// 根据管理器与内核快照构建与 Provider 无关的概览。
fn vce_overview_from_runtime(
    configuration: api::VceGlobalConfigurationStatus,
    settings: api::VceProjectSettingsSummary,
    status: VceRuntimeProjectStatus,
    snapshot: Option<vulcan_context_engine::VceManagementSnapshot>,
) -> api::VceProjectOverviewResponse {
    let ai = snapshot
        .as_ref()
        .map(|snapshot| api::VceAiCapabilitySummary {
            registered: snapshot.ai.registered,
            embedding_available: snapshot.ai.embedding_available,
            query_rewrite_available: snapshot.ai.query_rewrite_available,
            rerank_available: snapshot.ai.rerank_available,
        });
    let lifecycle = lifecycle_state(status.lifecycle);
    let counts = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.index.as_ref())
        .map(|index| api::VceIndexCountsSummary {
            files: index.counts.files,
            searchable_files: index.counts.searchable_files,
            skipped_files: index.counts.skipped_files,
            deleted_files: index.counts.deleted_files,
            blobs: index.counts.blobs,
            chunks: index.counts.chunks,
            vectors: index.counts.vectors,
            path_documents: index.counts.path_documents,
            symbols: index.counts.symbols,
        });
    let integrity = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.index.as_ref())
        .map(|index| api::VceIndexIntegritySummary {
            index_exists: index.integrity.index_exists,
            searchable: index.integrity.searchable,
            checkpoint_blob_count: index.integrity.checkpoint_blob_count,
            indexed_blob_count: index.integrity.indexed_blob_count,
            missing_blob_count: index.integrity.missing_blob_count,
            orphan_blob_count: index.integrity.orphan_blob_count,
            detail: index.integrity.detail.clone(),
        });
    let checkpoint_blobs = counts.as_ref().map_or(0, |counts| counts.blobs);
    let workspace = status.workspace.as_ref();
    api::VceProjectOverviewResponse {
        configuration,
        settings,
        status: api::VceProjectStatusSummary {
            project_id: status.project_id,
            enabled: true,
            settings_revision: status.config_revision,
            lifecycle,
            engine_state: status
                .engine
                .as_ref()
                .map(|engine| engine_state(engine.state)),
            workspace_generation: status
                .engine
                .as_ref()
                .map(|engine| engine.workspace_generation),
            checkpoint_version: status
                .engine
                .as_ref()
                .and_then(|engine| engine.checkpoint_version),
            present_files: workspace.map_or(0, |workspace| workspace.present_files),
            checkpoint_blobs,
            last_error: status.last_error,
        },
        ai,
        counts,
        integrity,
    }
}

/// Build the project status used when global model configuration fails closed.
/// 构建全局模型配置失败关闭时使用的项目状态。
fn blocked_configuration_status(
    project: &TaskProjectSummary,
    configuration: &api::VceGlobalConfigurationStatus,
) -> api::VceProjectStatusSummary {
    api::VceProjectStatusSummary {
        project_id: project.id.clone(),
        enabled: false,
        settings_revision: project.vce.revision,
        lifecycle: api::VceProjectLifecycleState::BlockedConfiguration,
        engine_state: None,
        workspace_generation: None,
        checkpoint_version: None,
        present_files: 0,
        checkpoint_blobs: 0,
        last_error: Some(bounded_service_error(format!(
            "VCE 全局模型配置未就绪：{}",
            configuration.issues.join("；")
        ))),
    }
}

/// Build the lightweight disabled project status.
/// 构建轻量禁用项目状态。
fn disabled_status(project: &TaskProjectSummary) -> api::VceProjectStatusSummary {
    api::VceProjectStatusSummary {
        project_id: project.id.clone(),
        enabled: false,
        settings_revision: project.vce.revision,
        lifecycle: api::VceProjectLifecycleState::Disabled,
        engine_state: None,
        workspace_generation: None,
        checkpoint_version: None,
        present_files: 0,
        checkpoint_blobs: 0,
        last_error: None,
    }
}

/// Convert persisted settings into the public service summary.
/// 将持久化设置转换为公共服务摘要。
fn vce_settings_summary(settings: &ProjectVceSettings) -> api::VceProjectSettingsSummary {
    api::VceProjectSettingsSummary {
        enabled: settings.enabled,
        revision: settings.revision,
        migrated_from_codebase_memory: settings.migrated_from_codebase_memory,
        max_file_bytes: settings.max_file_bytes,
        extra_ignore_patterns: settings.extra_ignore_patterns.clone(),
        allow_query_rewrite: settings.allow_query_rewrite,
        allow_rerank: settings.allow_rerank,
        embedding_max_concurrency: settings.embedding_max_concurrency,
        query_max_concurrency: settings.query_max_concurrency,
        max_results: settings.max_results,
        max_context_chars: settings.max_context_chars,
    }
}

/// Construct a complete persisted settings row from user-editable input.
/// 根据用户可编辑输入构造一条完整持久化设置记录。
fn project_vce_settings_from_input(
    input: api::VceProjectSettingsInput,
    revision: u64,
    migrated_from_codebase_memory: bool,
) -> ProjectVceSettings {
    ProjectVceSettings {
        enabled: input.enabled,
        revision,
        migrated_from_codebase_memory,
        max_file_bytes: input.max_file_bytes,
        extra_ignore_patterns: input.extra_ignore_patterns,
        allow_query_rewrite: input.allow_query_rewrite,
        allow_rerank: input.allow_rerank,
        embedding_max_concurrency: input.embedding_max_concurrency,
        query_max_concurrency: input.query_max_concurrency,
        max_results: input.max_results,
        max_context_chars: input.max_context_chars,
    }
}

/// Validate project settings and checkpoint preconditions for a mutation.
/// 校验项目设置与检查点修改前置条件。
fn validate_mutation_revision(
    project: &TaskProjectSummary,
    request: &api::VceProjectMutationRequest,
) -> Result<(), String> {
    validate_project_revision(project, request.expected_settings_revision)
}

/// Convert one core file page to the public generic page.
/// 将一页内核文件结果转换为公共通用分页。
fn map_page<T, U>(page: CoreVcePage<T>, map: impl Fn(T) -> U) -> api::VcePageResponse<U> {
    api::VcePageResponse {
        items: page.items.into_iter().map(map).collect(),
        next_cursor: page.next_cursor,
        total: page.total,
    }
}

/// Convert and validate one public page request.
/// 转换并校验一项公共分页请求。
fn core_page_request(request: api::VcePageRequest) -> Result<CoreVcePageRequest, String> {
    CoreVcePageRequest::new(request.cursor, request.limit).map_err(|error| error.to_string())
}

/// Convert one core file summary.
/// 转换一项内核文件摘要。
fn map_file_summary(item: vulcan_context_engine::VceFileSummary) -> api::VceFileSummary {
    api::VceFileSummary {
        path: item.path,
        blob_name: item.blob_name,
        committed_blob_name: item.committed_blob_name,
        status: file_status_text(item.status).to_owned(),
        generation: item.generation,
        size: item.size,
        chunk_count: item.chunk_count,
        vector_count: item.vector_count,
        symbol_count: item.symbol_count,
        path_channel: map_channel_state(item.path_channel),
        symbol_channel: map_channel_state(item.symbol_channel),
        last_error: item.last_error,
    }
}

/// Convert one core Blob detail.
/// 转换一项内核 Blob 详情。
fn map_blob_detail(item: vulcan_context_engine::VceBlobDetail) -> api::VceBlobDetailResponse {
    api::VceBlobDetailResponse {
        blob_name: item.blob_name,
        path: item.path,
        generation: item.generation,
        structure_analyzer_id: item.structure_analyzer_id,
        structure_fallback_reason: item.structure_fallback_reason,
        chunks: item
            .chunks
            .into_iter()
            .map(|chunk| api::VceChunkSummary {
                chunk_id: chunk.chunk_id,
                start_line: chunk.range.start_line,
                end_line: chunk.range.end_line,
                start_char_offset: chunk.start_char_offset,
                kind: chunk_kind_text(chunk.kind).to_owned(),
                preview: chunk.preview,
                vector_ready: chunk.vector_ready,
            })
            .collect(),
        path_channel: map_channel_state(item.path_channel),
        symbol_channel: map_channel_state(item.symbol_channel),
    }
}

/// Convert one durable operation summary.
/// 转换一项持久操作摘要。
fn map_operation(item: vulcan_context_engine::VceOperationSummary) -> api::VceOperationSummary {
    api::VceOperationSummary {
        operation_id: item.operation_id,
        kind: operation_kind_text(item.kind).to_owned(),
        phase: operation_phase_text(item.phase).to_owned(),
        generation: item.generation,
        refresh_mode: item
            .refresh_mode
            .map(|value| refresh_mode_text(value).to_owned()),
        refresh_reason: item
            .refresh_reason
            .map(|value| refresh_reason_text(value).to_owned()),
        completed_units: item.completed_units,
        total_units: item.total_units,
        attempt: item.attempt,
        coalesced: item.coalesced,
        error: item.error,
    }
}

/// Convert one core integrity issue.
/// 转换一项内核完整性问题。
fn map_integrity_issue(
    item: vulcan_context_engine::VceIntegrityIssue,
) -> api::VceIntegrityIssueSummary {
    api::VceIntegrityIssueSummary {
        issue_id: item.issue_id,
        kind: integrity_issue_kind_text(item.kind).to_owned(),
        path: item.path,
        blob_name: item.blob_name,
        message: item.message,
    }
}

/// Convert one auxiliary channel state.
/// 转换一项辅助通道状态。
fn map_channel_state(state: VceChannelState) -> api::VceChannelStateSummary {
    match state {
        VceChannelState::Ready => api::VceChannelStateSummary {
            state: String::from("ready"),
            reason: None,
        },
        VceChannelState::Pending => api::VceChannelStateSummary {
            state: String::from("pending"),
            reason: None,
        },
        VceChannelState::NotApplicable => api::VceChannelStateSummary {
            state: String::from("not_applicable"),
            reason: None,
        },
        VceChannelState::Failed { reason } => api::VceChannelStateSummary {
            state: String::from("failed"),
            reason: Some(reason),
        },
    }
}

/// Normalize a project identity supplied across a service boundary.
/// 规范化通过服务边界提供的项目身份。
fn stable_project_id(project_id: &str) -> Result<String, String> {
    let stable = project_id.trim();
    if stable.is_empty() {
        Err(String::from("项目标识不能为空。"))
    } else {
        Ok(stable.to_owned())
    }
}

/// Bound one retained service diagnostic.
/// 限制一条保留的服务诊断长度。
fn bounded_service_error(error: String) -> String {
    error.chars().take(2_048).collect()
}

/// Parse one public file status filter.
/// 解析一个公共文件状态过滤器。
fn parse_file_status(value: &str) -> Result<FileStatus, String> {
    match value {
        "present" => Ok(FileStatus::Present),
        "skipped" => Ok(FileStatus::Skipped),
        "deleted" => Ok(FileStatus::Deleted),
        _ => Err(format!("不支持的 VCE 文件状态：`{value}`。")),
    }
}

/// Parse one public maintenance mode.
/// 解析一个公共维护模式。
fn parse_maintenance_mode(value: &str) -> Result<VceMaintenanceMode, String> {
    match value {
        "repair" => Ok(VceMaintenanceMode::Repair),
        "rebuild" => Ok(VceMaintenanceMode::Rebuild),
        "cleanup_staging" => Ok(VceMaintenanceMode::CleanupStaging),
        _ => Err(format!("不支持的 VCE 维护模式：`{value}`。")),
    }
}

/// Return the stable public lifecycle state.
/// 返回稳定公共生命周期状态。
fn lifecycle_state(value: VceProjectLifecycle) -> api::VceProjectLifecycleState {
    match value {
        VceProjectLifecycle::Activating => api::VceProjectLifecycleState::Starting,
        VceProjectLifecycle::Restoring => api::VceProjectLifecycleState::Restoring,
        VceProjectLifecycle::Ready => api::VceProjectLifecycleState::Ready,
        VceProjectLifecycle::Refreshing => api::VceProjectLifecycleState::Indexing,
        VceProjectLifecycle::BlockedConfiguration => {
            api::VceProjectLifecycleState::BlockedConfiguration
        }
        VceProjectLifecycle::RebuildRequired => api::VceProjectLifecycleState::RebuildRequired,
        VceProjectLifecycle::Failed => api::VceProjectLifecycleState::Failed,
        VceProjectLifecycle::Stopping => api::VceProjectLifecycleState::Stopping,
    }
}

/// Return the stable public engine state.
/// 返回稳定公共引擎状态。
fn engine_state(value: VceEngineState) -> api::VceEngineStateSummary {
    match value {
        VceEngineState::Cold => api::VceEngineStateSummary::Cold,
        VceEngineState::Ready => api::VceEngineStateSummary::Ready,
        VceEngineState::PendingRefresh => api::VceEngineStateSummary::PendingRefresh,
        VceEngineState::Refreshing => api::VceEngineStateSummary::Refreshing,
        VceEngineState::Retrieving => api::VceEngineStateSummary::Retrieving,
        VceEngineState::Failed => api::VceEngineStateSummary::Failed,
    }
}

/// Return the stable query-intent text.
/// 返回稳定查询意图文本。
fn query_intent_text(value: QueryIntent) -> &'static str {
    match value {
        QueryIntent::Symbol => "symbol",
        QueryIntent::CallChain => "call_chain",
        QueryIntent::Reference => "reference",
        QueryIntent::Path => "path",
        QueryIntent::Feature => "feature",
        QueryIntent::Overview => "overview",
        QueryIntent::Compound => "compound",
    }
}

/// Return the stable search-source text.
/// 返回稳定搜索来源文本。
fn search_source_text(value: SearchSource) -> &'static str {
    match value {
        SearchSource::Dense => "dense",
        SearchSource::Exact => "exact",
        SearchSource::Path => "path",
        SearchSource::Fused => "fused",
    }
}

/// Return the stable hit-kind text.
/// 返回稳定命中种类文本。
fn hit_kind_text(value: HitKind) -> &'static str {
    match value {
        HitKind::Content => "content",
        HitKind::Definition => "definition",
        HitKind::Endpoint => "endpoint",
        HitKind::Reference => "reference",
    }
}

/// Return the stable optional degradation text.
/// 返回稳定可选降级文本。
fn degraded_feature_text(value: VceDegradedFeature) -> &'static str {
    match value {
        VceDegradedFeature::QueryRewrite => "query_rewrite",
        VceDegradedFeature::Rerank => "rerank",
        VceDegradedFeature::PathSearch => "path_search",
        VceDegradedFeature::ExactSearch => "exact_search",
    }
}

/// Return the stable file-status text.
/// 返回稳定文件状态文本。
fn file_status_text(value: FileStatus) -> &'static str {
    match value {
        FileStatus::Present => "present",
        FileStatus::Skipped => "skipped",
        FileStatus::Deleted => "deleted",
    }
}

/// Return the stable chunk-kind text.
/// 返回稳定代码块种类文本。
fn chunk_kind_text(value: ChunkKind) -> &'static str {
    match value {
        ChunkKind::Ast => "ast",
        ChunkKind::Recursive => "recursive",
        ChunkKind::Path => "path",
    }
}

/// Return the stable operation-kind text.
/// 返回稳定操作种类文本。
fn operation_kind_text(value: VceOperationKind) -> &'static str {
    match value {
        VceOperationKind::InitialBuild => "initial_build",
        VceOperationKind::IncrementalRefresh => "incremental_refresh",
        VceOperationKind::VerifyIntegrity => "verify_integrity",
        VceOperationKind::Repair => "repair",
        VceOperationKind::Rebuild => "rebuild",
    }
}

/// Return the stable operation-phase text.
/// 返回稳定操作阶段文本。
fn operation_phase_text(value: VceOperationPhase) -> &'static str {
    match value {
        VceOperationPhase::Queued => "queued",
        VceOperationPhase::Reconciling => "reconciling",
        VceOperationPhase::ReadingSource => "reading_source",
        VceOperationPhase::Structuring => "structuring",
        VceOperationPhase::Embedding => "embedding",
        VceOperationPhase::Staging => "staging",
        VceOperationPhase::Publishing => "publishing",
        VceOperationPhase::Completed => "completed",
        VceOperationPhase::Cancelled => "cancelled",
        VceOperationPhase::Failed => "failed",
    }
}

/// Return the stable refresh-mode text.
/// 返回稳定刷新模式文本。
fn refresh_mode_text(value: RefreshMode) -> &'static str {
    match value {
        RefreshMode::Full => "full",
        RefreshMode::Paths => "paths",
    }
}

/// Return the stable refresh-reason text.
/// 返回稳定刷新原因文本。
fn refresh_reason_text(value: RefreshReason) -> &'static str {
    match value {
        RefreshReason::Initial => "initial",
        RefreshReason::FileWatcher => "file_watcher",
        RefreshReason::GitWatcher => "git_watcher",
        RefreshReason::Manual => "manual",
        RefreshReason::Recovery => "recovery",
    }
}

/// Return the stable integrity-issue text.
/// 返回稳定完整性问题文本。
fn integrity_issue_kind_text(value: VceIntegrityIssueKind) -> &'static str {
    match value {
        VceIntegrityIssueKind::MissingBlob => "missing_blob",
        VceIntegrityIssueKind::IncompleteContentIndex => "incomplete_content_index",
        VceIntegrityIssueKind::PathIndexMismatch => "path_index_mismatch",
        VceIntegrityIssueKind::SymbolIndexMismatch => "symbol_index_mismatch",
        VceIntegrityIssueKind::VectorSpaceMismatch => "vector_space_mismatch",
        VceIntegrityIssueKind::OrphanedStaging => "orphaned_staging",
    }
}

/// Return the stable maintenance-mode text.
/// 返回稳定维护模式文本。
fn maintenance_mode_text(value: VceMaintenanceMode) -> &'static str {
    match value {
        VceMaintenanceMode::Repair => "repair",
        VceMaintenanceMode::Rebuild => "rebuild",
        VceMaintenanceMode::CleanupStaging => "cleanup_staging",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies every public maintenance mode is parsed without aliases or fallback.
    /// 验证每个公共维护模式均在无别名或回退的情况下解析。
    #[test]
    fn maintenance_modes_are_exact() {
        assert_eq!(
            parse_maintenance_mode("cleanup_staging"),
            Ok(VceMaintenanceMode::CleanupStaging)
        );
        assert!(parse_maintenance_mode("cleanup").is_err());
    }

    /// Verifies VCE settings input preserves host-owned migration state.
    /// 验证 VCE 设置输入保留宿主持有的迁移状态。
    #[test]
    fn settings_input_preserves_host_owned_fields() {
        let input = api::VceProjectSettingsInput {
            enabled: true,
            max_file_bytes: 1_048_576,
            extra_ignore_patterns: Vec::new(),
            allow_query_rewrite: false,
            allow_rerank: true,
            embedding_max_concurrency: 4,
            query_max_concurrency: 3,
            max_results: 10,
            max_context_chars: 32_000,
        };
        let settings = project_vce_settings_from_input(input, 8, true);

        assert_eq!(settings.revision, 8);
        assert!(settings.migrated_from_codebase_memory);
        assert!(settings.enabled);
    }

    /// Verifies stale reconciliation completions cannot claim a different runtime contract.
    /// 验证陈旧协调结果不能认领不同的运行时契约。
    #[test]
    fn reconciliation_configuration_match_is_exact() {
        let status = VceRuntimeProjectStatus {
            project_id: String::from("project-a"),
            lifecycle: VceProjectLifecycle::Ready,
            config_revision: 7,
            embedding_physical_fingerprint: Some(String::from("sha256:embedding-a")),
            engine: None,
            workspace: None,
            last_error: None,
        };

        assert!(runtime_status_matches_configuration(
            Some(&status),
            7,
            "sha256:embedding-a"
        ));
        assert!(!runtime_status_matches_configuration(
            Some(&status),
            6,
            "sha256:embedding-a"
        ));
        assert!(!runtime_status_matches_configuration(
            Some(&status),
            7,
            "sha256:embedding-b"
        ));
        assert!(!runtime_status_matches_configuration(
            None,
            7,
            "sha256:embedding-a"
        ));
    }

    /// Verifies model-facing retrieval admission requires a usable committed checkpoint.
    /// 验证面向模型的检索准入要求一个可用的已提交检查点。
    #[test]
    fn runtime_request_admission_requires_committed_usable_state() {
        let mut status = VceRuntimeProjectStatus {
            project_id: String::from("project-a"),
            lifecycle: VceProjectLifecycle::Ready,
            config_revision: 7,
            embedding_physical_fingerprint: Some(String::from("sha256:embedding-a")),
            engine: Some(vulcan_context_engine::VceEngineSnapshot {
                state: VceEngineState::Ready,
                workspace_generation: 1,
                checkpoint_version: Some(3),
                last_refresh: None,
                last_retrieval: None,
                last_error: None,
            }),
            workspace: None,
            last_error: None,
        };

        assert!(runtime_status_can_serve_requests(
            Some(&status),
            7,
            "sha256:embedding-a"
        ));
        status
            .engine
            .as_mut()
            .expect("engine exists")
            .checkpoint_version = None;
        assert!(!runtime_status_can_serve_requests(
            Some(&status),
            7,
            "sha256:embedding-a"
        ));
        status
            .engine
            .as_mut()
            .expect("engine exists")
            .checkpoint_version = Some(3);
        status.lifecycle = VceProjectLifecycle::Failed;
        assert!(!runtime_status_can_serve_requests(
            Some(&status),
            7,
            "sha256:embedding-a"
        ));
    }
}
